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

use super::{FugueCommand, FugueDefinition, FugueInfo};

/// Ring buffer size for fugue commands
/// Larger than MIDI ring buffer because FugueDefinition can be big
const RING_BUFFER_SIZE: usize = 64;

/// Instance entry for fugue bridge
struct FugueInstanceEntry {
    name: String,
    producer: Mutex<Producer<FugueCommand>>,
    /// Lock-free cache of fugue info (updated by audio thread, read by MCP)
    info_cache: Arc<ArcSwap<Vec<FugueInfo>>>,
}

/// Global registry of fugue producers (keyed by instance ID)
static FUGUE_REGISTRY: OnceLock<RwLock<HashMap<String, FugueInstanceEntry>>> = OnceLock::new();

fn registry() -> &'static RwLock<HashMap<String, FugueInstanceEntry>> {
    FUGUE_REGISTRY.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Bridge for sending fugue commands from MCP to audio thread
pub struct FugueBridge;

/// Handle for the audio thread to update fugue info without locking
pub struct FugueInfoHandle {
    info_cache: Arc<ArcSwap<Vec<FugueInfo>>>,
}

impl FugueInfoHandle {
    /// Update the fugue info cache (lock-free, safe for audio thread)
    pub fn update(&self, infos: Vec<FugueInfo>) {
        self.info_cache.store(Arc::new(infos));
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

        let mut reg = registry().write().unwrap();
        reg.insert(
            id.to_string(),
            FugueInstanceEntry {
                name: name.to_string(),
                producer: Mutex::new(producer),
                info_cache: Arc::clone(&info_cache),
            },
        );

        log::info!("FugueBridge: Registered instance '{}'", id);
        (consumer, FugueInfoHandle { info_cache })
    }

    /// Unregister a plugin instance
    pub fn unregister(id: &str) {
        let mut reg = registry().write().unwrap();
        if reg.remove(id).is_some() {
            log::info!("FugueBridge: Unregistered instance '{}'", id);
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

        log::info!("FugueBridge: Queued fugue {} on '{}'", id, entry.name);
        Ok(id)
    }

    /// Cancel a specific fugue by ID
    pub fn cancel(instance: &str, id: u64) -> Result<(), &'static str> {
        let reg = registry().read().unwrap();
        let entry = find_entry(&reg, instance)?;

        let mut producer = entry.producer.lock().map_err(|_| "Producer lock poisoned")?;
        producer
            .push(FugueCommand::Cancel { id })
            .map_err(|_| "Queue full")?;

        log::info!("FugueBridge: Cancelled fugue {} on '{}'", id, entry.name);
        Ok(())
    }

    /// Cancel all fugues with a specific tag
    pub fn cancel_by_tag(instance: &str, tag: &str) -> Result<(), &'static str> {
        let reg = registry().read().unwrap();
        let entry = find_entry(&reg, instance)?;

        let mut producer = entry.producer.lock().map_err(|_| "Producer lock poisoned")?;
        producer
            .push(FugueCommand::CancelByTag { tag: tag.to_string() })
            .map_err(|_| "Queue full")?;

        log::info!("FugueBridge: Cancelled fugues with tag '{}' on '{}'", tag, entry.name);
        Ok(())
    }

    /// Clear all fugues on an instance
    pub fn clear_all(instance: &str) -> Result<(), &'static str> {
        let reg = registry().read().unwrap();
        let entry = find_entry(&reg, instance)?;

        let mut producer = entry.producer.lock().map_err(|_| "Producer lock poisoned")?;
        producer
            .push(FugueCommand::ClearAll)
            .map_err(|_| "Queue full")?;

        log::info!("FugueBridge: Cleared all fugues on '{}'", entry.name);
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

impl FugueBridge {
    /// Get cached fugue info for an instance (called from MCP)
    ///
    /// This reads from the lock-free ArcSwap cache updated by the audio thread.
    pub fn get_fugue_info(instance: &str) -> Result<Vec<FugueInfo>, &'static str> {
        let reg = registry().read().unwrap();
        let entry = find_entry(&reg, instance)?;

        // Load from the lock-free cache
        let infos = entry.info_cache.load();
        Ok((**infos).clone())
    }
}
