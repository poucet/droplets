//! CcBridge - Clean interface for MCP-to-audio-thread CC communication
//!
//! The MCP server writes directly to plugin instances via lock-free ring buffers.
//! No main thread involvement needed for CC messages.

use rtrb::{Consumer, Producer, RingBuffer};
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// A MIDI CC message (3 bytes, Copy, no heap allocation)
#[derive(Clone, Copy, Debug)]
pub struct CcMessage {
    pub channel: u8, // 0-15
    pub cc: u8,      // 0-127
    pub value: u8,   // 0-127
}

/// Activity event for GUI visualization
#[derive(Clone, Debug)]
pub struct ActivityEvent {
    pub timestamp_ms: u64,
    pub instance: String,
    pub channel: u8,
    pub cc: u8,
    pub value: u8,
}

/// Instance entry stored in the registry
struct InstanceEntry {
    name: String,
    producer: Mutex<Producer<CcMessage>>,
}

/// Global registry of plugin instances
static REGISTRY: OnceLock<RwLock<HashMap<String, InstanceEntry>>> = OnceLock::new();

/// Activity log for GUI visualization
static ACTIVITY_LOG: OnceLock<Mutex<VecDeque<ActivityEvent>>> = OnceLock::new();

const MAX_ACTIVITY_LOG_SIZE: usize = 50;
const RING_BUFFER_SIZE: usize = 256;

fn registry() -> &'static RwLock<HashMap<String, InstanceEntry>> {
    REGISTRY.get_or_init(|| RwLock::new(HashMap::new()))
}

fn activity_log() -> &'static Mutex<VecDeque<ActivityEvent>> {
    ACTIVITY_LOG.get_or_init(|| Mutex::new(VecDeque::with_capacity(MAX_ACTIVITY_LOG_SIZE)))
}

/// Clean interface for the CC bridge system
pub struct CcBridge;

impl CcBridge {
    /// Register a new plugin instance.
    ///
    /// Returns the Consumer that the audio thread uses to receive CC messages.
    /// The Producer is stored in the registry for the MCP server to write to.
    pub fn register(id: &str) -> Consumer<CcMessage> {
        let (producer, consumer) = RingBuffer::new(RING_BUFFER_SIZE);

        let mut reg = registry().write().unwrap();
        reg.insert(
            id.to_string(),
            InstanceEntry {
                name: id.to_string(),
                producer: Mutex::new(producer),
            },
        );

        log::info!("CcBridge: Registered instance '{}'", id);
        consumer
    }

    /// Unregister a plugin instance
    pub fn unregister(id: &str) {
        let mut reg = registry().write().unwrap();
        if reg.remove(id).is_some() {
            log::info!("CcBridge: Unregistered instance '{}'", id);
        }
    }

    /// Send a CC message to an instance (called from MCP server thread)
    ///
    /// This is lock-free on the audio thread side - we only take a read lock
    /// on the registry to find the producer, then push to the ring buffer.
    pub fn send(instance: &str, msg: CcMessage) -> Result<(), &'static str> {
        let reg = registry().read().unwrap();
        let entry = Self::find_entry(&reg, instance)?;
        let instance_name = entry.name.clone();

        // Lock the producer and push (brief lock, not on audio thread)
        let mut producer = entry.producer.lock().map_err(|_| "Producer lock poisoned")?;
        producer
            .push(msg)
            .map_err(|_| "Queue full - audio thread not consuming fast enough")?;

        // Log activity for GUI (non-critical, ignore lock failures)
        if let Ok(mut log) = activity_log().try_lock() {
            let timestamp_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);

            log.push_back(ActivityEvent {
                timestamp_ms,
                instance: instance_name,
                channel: msg.channel,
                cc: msg.cc,
                value: msg.value,
            });

            // Keep log bounded
            while log.len() > MAX_ACTIVITY_LOG_SIZE {
                log.pop_front();
            }
        }

        Ok(())
    }

    /// Rename an instance for easier AI reference
    pub fn rename(instance: &str, new_name: &str) -> Result<String, &'static str> {
        let mut reg = registry().write().unwrap();
        let id = Self::find_id(&reg, instance)?;

        if let Some(entry) = reg.get_mut(&id) {
            let old_name = entry.name.clone();
            entry.name = new_name.to_string();
            log::info!("CcBridge: Renamed '{}' to '{}'", old_name, new_name);
            Ok(old_name)
        } else {
            Err("Instance not found")
        }
    }

    /// List all registered instance names
    pub fn list_instances() -> Vec<String> {
        registry()
            .read()
            .unwrap()
            .values()
            .map(|e| e.name.clone())
            .collect()
    }

    /// Get the number of registered instances
    pub fn instance_count() -> usize {
        registry().read().unwrap().len()
    }

    /// Get recent activity for GUI display
    pub fn recent_activity() -> Vec<ActivityEvent> {
        activity_log()
            .lock()
            .ok()
            .map(|log| log.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Clear the activity log
    pub fn clear_activity() {
        if let Ok(mut log) = activity_log().lock() {
            log.clear();
        }
    }

    /// Find an instance entry by name or ID
    fn find_entry<'a>(
        reg: &'a HashMap<String, InstanceEntry>,
        name: &str,
    ) -> Result<&'a InstanceEntry, &'static str> {
        if name == "default" {
            // Return the first available instance
            reg.values().next().ok_or("No instances connected")
        } else {
            // Try to find by display name first, then by internal ID
            reg.values()
                .find(|e| e.name == name)
                .or_else(|| reg.get(name))
                .ok_or("Instance not found")
        }
    }

    /// Find an instance ID by name or ID
    fn find_id(
        reg: &HashMap<String, InstanceEntry>,
        name: &str,
    ) -> Result<String, &'static str> {
        if name == "default" {
            reg.keys().next().cloned().ok_or("No instances connected")
        } else {
            reg.iter()
                .find(|(id, e)| e.name == name || *id == name)
                .map(|(id, _)| id.clone())
                .ok_or("Instance not found")
        }
    }
}
