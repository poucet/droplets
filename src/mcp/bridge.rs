//! CcBridge - Clean interface for MCP-to-audio-thread CC communication
//!
//! The MCP server writes directly to plugin instances via lock-free ring buffers.
//! No main thread involvement needed for CC messages.

use rtrb::{Consumer, Producer, RingBuffer};
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::params::DropletParams;

/// A MIDI CC message (3 bytes, Copy, no heap allocation)
#[derive(Clone, Copy, Debug)]
pub struct CcMessage {
    pub channel: u8, // 0-15
    pub cc: u8,      // 0-127
    pub value: u8,   // 0-127
}

/// A MIDI Note message (Note On or Note Off)
#[derive(Clone, Copy, Debug)]
pub struct NoteMessage {
    pub channel: u8,  // 0-15
    pub note: u8,     // 0-127 (MIDI note number, 60 = C4)
    pub velocity: u8, // 0-127 (0 = note off for Note On messages)
    pub is_note_on: bool,
}

/// Combined MIDI message type for the ring buffer
#[derive(Clone, Copy, Debug)]
pub enum MidiMessage {
    Cc(CcMessage),
    Note(NoteMessage),
}

/// Activity event for GUI visualization
#[derive(Clone, Debug)]
pub struct ActivityEvent {
    pub timestamp_ms: u64,
    pub instance: String,
    pub channel: u8,
    pub cc: Option<u8>,       // CC number (for CC messages)
    pub value: u8,            // CC value or velocity
    pub note: Option<u8>,     // Note number (for note messages)
    pub is_note_on: Option<bool>, // true = note on, false = note off
}

/// Shared parameter access for MCP server
pub type ParamsRef = Arc<DropletParams>;

/// Instance entry stored in the registry
struct InstanceEntry {
    name: String,
    producer: Mutex<Producer<MidiMessage>>,
    params: ParamsRef,
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
    /// Returns the Consumer that the audio thread uses to receive MIDI messages.
    /// The Producer is stored in the registry for the MCP server to write to.
    pub fn register(id: &str, params: ParamsRef) -> Consumer<MidiMessage> {
        let (producer, consumer) = RingBuffer::new(RING_BUFFER_SIZE);

        let mut reg = registry().write().unwrap();
        reg.insert(
            id.to_string(),
            InstanceEntry {
                name: id.to_string(),
                producer: Mutex::new(producer),
                params,
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
            .push(MidiMessage::Cc(msg))
            .map_err(|_| "Queue full - audio thread not consuming fast enough")?;

        // Log activity for GUI (non-critical, ignore lock failures)
        Self::log_cc_activity(&instance_name, &msg);

        Ok(())
    }

    /// Send a Note message to an instance (called from MCP server thread)
    pub fn send_note(instance: &str, msg: NoteMessage) -> Result<(), &'static str> {
        let reg = registry().read().unwrap();
        let entry = Self::find_entry(&reg, instance)?;
        let instance_name = entry.name.clone();

        let mut producer = entry.producer.lock().map_err(|_| "Producer lock poisoned")?;
        producer
            .push(MidiMessage::Note(msg))
            .map_err(|_| "Queue full - audio thread not consuming fast enough")?;

        Self::log_note_activity(&instance_name, &msg);

        Ok(())
    }

    /// Log CC activity for GUI visualization
    fn log_cc_activity(instance_name: &str, msg: &CcMessage) {
        if let Ok(mut log) = activity_log().try_lock() {
            let timestamp_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);

            log.push_back(ActivityEvent {
                timestamp_ms,
                instance: instance_name.to_string(),
                channel: msg.channel,
                cc: Some(msg.cc),
                value: msg.value,
                note: None,
                is_note_on: None,
            });

            while log.len() > MAX_ACTIVITY_LOG_SIZE {
                log.pop_front();
            }
        }
    }

    /// Log note activity for GUI visualization
    fn log_note_activity(instance_name: &str, msg: &NoteMessage) {
        if let Ok(mut log) = activity_log().try_lock() {
            let timestamp_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);

            log.push_back(ActivityEvent {
                timestamp_ms,
                instance: instance_name.to_string(),
                channel: msg.channel,
                cc: None,
                value: msg.velocity,
                note: Some(msg.note),
                is_note_on: Some(msg.is_note_on),
            });

            while log.len() > MAX_ACTIVITY_LOG_SIZE {
                log.pop_front();
            }
        }
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

    /// Set a parameter slot value (called from MCP server)
    ///
    /// If the slot is mapped to a CC, sends the MIDI CC message through the ring buffer.
    pub fn set_param(instance: &str, slot: usize, value: f64) -> Result<(), &'static str> {
        let reg = registry().read().unwrap();
        let entry = Self::find_entry(&reg, instance)?;

        if slot >= crate::params::NUM_CC_SLOTS {
            return Err("Slot index out of range (0-15)");
        }

        // Set the slot value and get CC info if mapped
        if let Some((channel, cc, midi_value)) = entry.params.set_slot(slot, value) {
            // Send MIDI CC through the ring buffer
            let msg = CcMessage { channel, cc, value: midi_value };
            let mut producer = entry.producer.lock().map_err(|_| "Producer lock poisoned")?;
            producer.push(MidiMessage::Cc(msg)).map_err(|_| "Queue full")?;

            Self::log_cc_activity(&entry.name, &msg);

            log::info!("CcBridge: Set slot {} = {:.2} -> CC{} = {} on '{}'", slot, value, cc, midi_value, entry.name);
        } else {
            log::info!("CcBridge: Set slot {} = {:.2} (unmapped) on '{}'", slot, value, entry.name);
        }

        Ok(())
    }

    /// Rename a parameter slot (called from MCP server)
    pub fn rename_slot(instance: &str, slot: usize, name: &str) -> Result<String, &'static str> {
        let reg = registry().read().unwrap();
        let entry = Self::find_entry(&reg, instance)?;

        if slot >= crate::params::NUM_CC_SLOTS {
            return Err("Slot index out of range (0-15)");
        }

        let old_name = entry.params.slots[slot].get_name();
        entry.params.rename_slot(slot, name);
        log::info!("CcBridge: Renamed slot {} from '{}' to '{}' on '{}'", slot, old_name, name, entry.name);
        Ok(old_name)
    }

    /// Get all parameter slots info for an instance (includes CC mapping info)
    pub fn get_slots(instance: &str) -> Result<Vec<crate::params::SlotInfo>, &'static str> {
        let reg = registry().read().unwrap();
        let entry = Self::find_entry(&reg, instance)?;
        Ok(entry.params.get_all_slots())
    }

    /// Start CC learning mode for a slot
    pub fn start_learn(instance: &str, slot: usize) -> Result<(), &'static str> {
        let reg = registry().read().unwrap();
        let entry = Self::find_entry(&reg, instance)?;

        if slot >= crate::params::NUM_CC_SLOTS {
            return Err("Slot index out of range (0-15)");
        }

        entry.params.start_learning(slot);
        log::info!("CcBridge: Started learning for slot {} on '{}'", slot, entry.name);
        Ok(())
    }

    /// Cancel CC learning mode on all slots
    pub fn cancel_learn(instance: &str) -> Result<(), &'static str> {
        let reg = registry().read().unwrap();
        let entry = Self::find_entry(&reg, instance)?;
        entry.params.cancel_learning();
        log::info!("CcBridge: Cancelled learning on '{}'", entry.name);
        Ok(())
    }

    /// Manually map a CC number to a slot
    pub fn map_slot(instance: &str, slot: usize, cc: u8, channel: u8) -> Result<(), &'static str> {
        let reg = registry().read().unwrap();
        let entry = Self::find_entry(&reg, instance)?;

        if slot >= crate::params::NUM_CC_SLOTS {
            return Err("Slot index out of range (0-15)");
        }

        entry.params.slots[slot].set_cc(cc);
        entry.params.slots[slot].set_channel(channel);
        log::info!("CcBridge: Mapped slot {} to CC{} ch{} on '{}'", slot, cc, channel + 1, entry.name);
        Ok(())
    }

    /// Clear CC mapping from a slot
    pub fn unmap_slot(instance: &str, slot: usize) -> Result<(), &'static str> {
        let reg = registry().read().unwrap();
        let entry = Self::find_entry(&reg, instance)?;

        if slot >= crate::params::NUM_CC_SLOTS {
            return Err("Slot index out of range (0-15)");
        }

        entry.params.slots[slot].clear_cc();
        log::info!("CcBridge: Unmapped slot {} on '{}'", slot, entry.name);
        Ok(())
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
