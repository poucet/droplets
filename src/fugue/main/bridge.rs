//! FugueBridge - MCP to audio thread communication for fugues
//!
//! Similar to CcBridge but for fugue commands. Uses a separate ring buffer
//! to send FugueCommand messages to the audio thread.
//!
//! The info cache uses Arc<ArcSwap> for lock-free updates from the audio thread.

use arc_swap::ArcSwap;
use rtrb::{Consumer, Producer, RingBuffer};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use super::super::command::FugueCommand;
use super::super::types::{FugueDefinition, FugueInfo, TransportState};

/// Ring buffer size for fugue commands
/// Larger than MIDI ring buffer because FugueDefinition can be big
const RING_BUFFER_SIZE: usize = 64;

/// Instance entry for fugue bridge
struct FugueInstanceEntry {
    name: String,
    producer: Mutex<Producer<FugueCommand>>,
    /// Lock-free cache of fugue info (updated by audio thread, read by MCP/GUI)
    info_cache: Arc<ArcSwap<Vec<FugueInfo>>>,
    /// Lock-free cache of transport state (updated by audio thread, read by GUI)
    transport_cache: Arc<ArcSwap<TransportState>>,
    /// Lock-free cache of full fugue definitions (for UI visualization)
    definitions_cache: Arc<ArcSwap<Vec<FugueDefinition>>>,
}

/// Global registry of fugue producers (keyed by instance ID)
static FUGUE_REGISTRY: OnceLock<RwLock<HashMap<String, FugueInstanceEntry>>> = OnceLock::new();

fn registry() -> &'static RwLock<HashMap<String, FugueInstanceEntry>> {
    FUGUE_REGISTRY.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Bridge for sending fugue commands from MCP to audio thread
pub struct FugueBridge;

/// Handle for the audio thread to update caches without locking
pub struct FugueInfoHandle {
    info_cache: Arc<ArcSwap<Vec<FugueInfo>>>,
    transport_cache: Arc<ArcSwap<TransportState>>,
    definitions_cache: Arc<ArcSwap<Vec<FugueDefinition>>>,
}

impl FugueInfoHandle {
    /// Update the fugue info cache (lock-free, safe for audio thread)
    pub fn update(&self, infos: Vec<FugueInfo>) {
        self.info_cache.store(Arc::new(infos));
    }

    /// Update the transport state cache (lock-free, safe for audio thread)
    pub fn update_transport(&self, transport: TransportState) {
        self.transport_cache.store(Arc::new(transport));
    }

    /// Update the fugue definitions cache (lock-free, safe for audio thread)
    pub fn update_definitions(&self, definitions: Vec<FugueDefinition>) {
        self.definitions_cache.store(Arc::new(definitions));
    }
}

impl FugueBridge {
    /// Register a new plugin instance for fugue commands.
    ///
    /// Returns (Consumer, FugueInfoHandle) - the consumer for commands and
    /// a handle for lock-free info updates from the audio thread.
    pub fn register(id: &str, name: &str) -> (Consumer<FugueCommand>, FugueInfoHandle) {
        let (producer, consumer) = RingBuffer::new(RING_BUFFER_SIZE);
        let info_cache = Arc::new(ArcSwap::from_pointee(Vec::new()));
        let transport_cache = Arc::new(ArcSwap::from_pointee(TransportState::default()));
        let definitions_cache = Arc::new(ArcSwap::from_pointee(Vec::new()));

        let mut reg = registry().write().unwrap();
        reg.insert(
            id.to_string(),
            FugueInstanceEntry {
                name: name.to_string(),
                producer: Mutex::new(producer),
                info_cache: Arc::clone(&info_cache),
                transport_cache: Arc::clone(&transport_cache),
                definitions_cache: Arc::clone(&definitions_cache),
            },
        );

        (
            consumer,
            FugueInfoHandle {
                info_cache,
                transport_cache,
                definitions_cache,
            },
        )
    }

    /// Unregister a plugin instance
    pub fn unregister(id: &str) {
        let mut reg = registry().write().unwrap();
        if reg.remove(id).is_some() {
        }
    }

    /// Queue a fugue on an instance
    ///
    /// Returns the fugue ID that was assigned.
    pub fn queue(instance: &str, definition: FugueDefinition) -> Result<u64, &'static str> {
        let reg = registry().read().unwrap();
        let entry = find_entry(&reg, instance)?;

        let id = definition.id;
        let mut producer = entry.producer.lock().map_err(|_| "Producer lock poisoned")?;
        producer
            .push(FugueCommand::Queue(definition))
            .map_err(|_| "Queue full - audio thread not consuming fast enough")?;

        Ok(id)
    }

    /// Cancel a specific fugue by ID.
    ///
    /// Queues the cancel command for the audio thread (which will emit
    /// note-offs and drop the fugue from its sequencer), AND filters the
    /// UI-facing info/definitions caches on this thread so readers see
    /// the removal immediately — even if the transport is stopped and
    /// the audio thread isn't ticking.
    pub fn cancel(instance: &str, id: u64) -> Result<(), &'static str> {
        let reg = registry().read().unwrap();
        let entry = find_entry(&reg, instance)?;

        let mut producer = entry.producer.lock().map_err(|_| "Producer lock poisoned")?;
        producer
            .push(FugueCommand::Cancel { id })
            .map_err(|_| "Queue full")?;
        drop(producer);

        filter_info_cache(entry, |infos| infos.retain(|i| i.id != id));
        filter_definitions_cache(entry, |defs| defs.retain(|d| d.id != id));

        Ok(())
    }

    /// Cancel all fugues with a specific tag. See [`cancel`] for the
    /// main-thread cache-filtering rationale.
    pub fn cancel_by_tag(instance: &str, tag: &str) -> Result<(), &'static str> {
        let reg = registry().read().unwrap();
        let entry = find_entry(&reg, instance)?;

        let mut producer = entry.producer.lock().map_err(|_| "Producer lock poisoned")?;
        producer
            .push(FugueCommand::CancelByTag { tag: tag.to_string() })
            .map_err(|_| "Queue full")?;
        drop(producer);

        filter_info_cache(entry, |infos| {
            infos.retain(|i| i.tag.as_deref() != Some(tag))
        });
        filter_definitions_cache(entry, |defs| {
            defs.retain(|d| d.tag.as_deref() != Some(tag))
        });

        Ok(())
    }

    /// Clear all fugues on an instance. See [`cancel`] for the main-thread
    /// cache-filtering rationale.
    pub fn clear_all(instance: &str) -> Result<(), &'static str> {
        let reg = registry().read().unwrap();
        let entry = find_entry(&reg, instance)?;

        let mut producer = entry.producer.lock().map_err(|_| "Producer lock poisoned")?;
        producer
            .push(FugueCommand::ClearAll)
            .map_err(|_| "Queue full")?;
        drop(producer);

        filter_info_cache(entry, |infos| infos.clear());
        filter_definitions_cache(entry, |defs| defs.clear());

        Ok(())
    }

    /// Update instance name (called when CcBridge renames)
    pub fn update_name(id: &str, new_name: &str) {
        if let Ok(mut reg) = registry().write() {
            if let Some(entry) = reg.get_mut(id) {
                entry.name = new_name.to_string();
            }
        }
    }

    /// Check if an instance is registered
    pub fn has_instance(instance: &str) -> bool {
        registry()
            .read()
            .ok()
            .map(|reg| find_entry(&reg, instance).is_ok())
            .unwrap_or(false)
    }

    /// Every registered instance as `(id, name)` pairs. Used by the
    /// cross-instance `list_fugues` / `clear_fugues` / `cancel_*` fan-out
    /// paths — they enumerate here then dispatch per id.
    pub fn list_all_instances() -> Vec<(String, String)> {
        registry()
            .read()
            .ok()
            .map(|reg| {
                reg.iter()
                    .map(|(id, e)| (id.clone(), e.name.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// Find an instance entry by name or ID
fn find_entry<'a>(
    reg: &'a HashMap<String, FugueInstanceEntry>,
    name: &str,
) -> Result<&'a FugueInstanceEntry, &'static str> {
    if name == "default" {
        reg.values().next().ok_or("No instances connected")
    } else {
        reg.values()
            .find(|e| e.name == name)
            .or_else(|| reg.get(name))
            .ok_or("Instance not found")
    }
}

/// Apply `mutate` to a fresh clone of the current info cache and store it
/// back atomically. Main-thread only — lets cancel handlers reflect UI
/// changes instantly without waiting on the audio thread's next publish.
fn filter_info_cache(entry: &FugueInstanceEntry, mutate: impl FnOnce(&mut Vec<FugueInfo>)) {
    let current = entry.info_cache.load();
    let mut next = (**current).clone();
    mutate(&mut next);
    entry.info_cache.store(Arc::new(next));
}

/// Same shape as [`filter_info_cache`] for the definitions cache.
fn filter_definitions_cache(
    entry: &FugueInstanceEntry,
    mutate: impl FnOnce(&mut Vec<FugueDefinition>),
) {
    let current = entry.definitions_cache.load();
    let mut next = (**current).clone();
    mutate(&mut next);
    entry.definitions_cache.store(Arc::new(next));
}

impl FugueBridge {
    /// Get cached fugue info for an instance (called from MCP/GUI)
    ///
    /// This reads from the lock-free ArcSwap cache updated by the audio thread.
    pub fn get_fugue_info(instance: &str) -> Result<Vec<FugueInfo>, &'static str> {
        let reg = registry().read().unwrap();
        let entry = find_entry(&reg, instance)?;

        // Load from the lock-free cache
        let infos = entry.info_cache.load();
        Ok((**infos).clone())
    }

    /// Get cached transport state for an instance (called from GUI)
    pub fn get_transport(instance: &str) -> Result<TransportState, &'static str> {
        let reg = registry().read().unwrap();
        let entry = find_entry(&reg, instance)?;
        let transport = entry.transport_cache.load();
        Ok(**transport)
    }

    /// Get cached fugue definitions for an instance (called from GUI for visualization)
    pub fn get_definitions(instance: &str) -> Result<Vec<FugueDefinition>, &'static str> {
        let reg = registry().read().unwrap();
        let entry = find_entry(&reg, instance)?;
        let defs = entry.definitions_cache.load();
        Ok((**defs).clone())
    }

    /// Get a specific fugue definition by ID
    pub fn get_definition(instance: &str, id: u64) -> Result<Option<FugueDefinition>, &'static str> {
        let defs = Self::get_definitions(instance)?;
        Ok(defs.into_iter().find(|d| d.id == id))
    }

    /// Block until `id` appears in the info cache on `instance`, or until
    /// `timeout_ms` elapses. Returns `true` if the fugue became visible.
    ///
    /// Call this after `queue` from an HTTP/MCP handler that wants its
    /// follow-up read (or the caller's immediate refresh) to see the newly
    /// queued fugue. The ring buffer → audio thread → info cache path has
    /// a natural ~one-audio-block delay; without the wait, the caller sees
    /// stale data and has to re-poll.
    ///
    /// **Never call from the audio thread** — this sleeps.
    pub fn wait_for_fugue_visible(instance: &str, id: u64, timeout_ms: u64) -> bool {
        Self::wait_until(instance, timeout_ms, |infos| {
            infos.iter().any(|i| i.id == id)
        })
    }

    /// Block until `id` disappears from the info cache, or until `timeout_ms`
    /// elapses. Returns `true` if the fugue is gone. Mirror of
    /// [`wait_for_fugue_visible`] for cancel paths — without this, the UI
    /// reads the stale info cache and the cancelled fugue "sticks around."
    ///
    /// **Never call from the audio thread** — this sleeps.
    pub fn wait_for_fugue_gone(instance: &str, id: u64, timeout_ms: u64) -> bool {
        Self::wait_until(instance, timeout_ms, |infos| {
            !infos.iter().any(|i| i.id == id)
        })
    }

    /// Block until no fugue with a matching tag remains in the info cache.
    /// Used by `cancel_fugues_by_tag` handlers to guarantee the caller's
    /// next read is post-clear.
    pub fn wait_for_tag_gone(instance: &str, tag: &str, timeout_ms: u64) -> bool {
        Self::wait_until(instance, timeout_ms, |infos| {
            !infos.iter().any(|i| i.tag.as_deref() == Some(tag))
        })
    }

    /// Block until the info cache on `instance` is empty. Used by
    /// `clear_fugues` handlers.
    pub fn wait_for_no_fugues(instance: &str, timeout_ms: u64) -> bool {
        Self::wait_until(instance, timeout_ms, |infos| infos.is_empty())
    }

    /// Shared polling loop behind the `wait_for_*` helpers. 2ms backoff;
    /// bails at the first iteration where `predicate(&infos)` is true.
    fn wait_until(
        instance: &str,
        timeout_ms: u64,
        predicate: impl Fn(&[FugueInfo]) -> bool,
    ) -> bool {
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
        while std::time::Instant::now() < deadline {
            if let Ok(infos) = Self::get_fugue_info(instance) {
                if predicate(&infos) {
                    return true;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        false
    }
}
