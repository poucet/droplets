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

/// A MIDI CC message
#[derive(Clone, Copy, Debug)]
pub struct CcMessage {
    pub channel: u8,  // 0-15
    pub cc: u8,       // 0-127
    pub value: u8,    // 0-127 (MIDI 1.0 7-bit)
    pub value_14bit: Option<u16>, // MIDI 2.0: 0-16383 for high-res CC
}

impl CcMessage {
    /// Create a standard 7-bit CC message
    pub fn new(channel: u8, cc: u8, value: u8) -> Self {
        Self { channel, cc, value, value_14bit: None }
    }

    /// Create a high-resolution 14-bit CC message (MIDI 2.0)
    pub fn new_hires(channel: u8, cc: u8, value_14bit: u16) -> Self {
        // Scale 14-bit to 7-bit for MIDI 1.0 fallback
        let value = (value_14bit >> 7) as u8;
        Self { channel, cc, value, value_14bit: Some(value_14bit) }
    }
}

/// A MIDI Note message (Note On or Note Off)
#[derive(Clone, Copy, Debug)]
pub struct NoteMessage {
    pub channel: u8,     // 0-15
    pub note: u8,        // 0-127 (MIDI note number, 60 = C4)
    pub velocity: u8,    // 0-127 (MIDI 1.0 7-bit)
    pub velocity_16bit: u16, // MIDI 2.0: 0-65535 for 16-bit velocity
    pub is_note_on: bool,
}

impl NoteMessage {
    /// Create a note message with 7-bit velocity (scales to 16-bit internally)
    pub fn new(channel: u8, note: u8, velocity: u8, is_note_on: bool) -> Self {
        // Scale 7-bit to 16-bit: multiply by 512 (shift left 9) + copy MSBs
        let velocity_16bit = ((velocity as u16) << 9) | ((velocity as u16) << 2);
        Self { channel, note, velocity, velocity_16bit, is_note_on }
    }

    /// Create a note message with 16-bit velocity (MIDI 2.0 native)
    pub fn new_hires(channel: u8, note: u8, velocity_16bit: u16, is_note_on: bool) -> Self {
        // Scale 16-bit to 7-bit for MIDI 1.0 fallback
        let velocity = (velocity_16bit >> 9) as u8;
        Self { channel, note, velocity, velocity_16bit, is_note_on }
    }
}

/// Per-note expression message (MIDI 2.0 only)
/// These are expressions that target a specific note that is currently playing
#[derive(Clone, Copy, Debug)]
pub struct PerNoteExpressionMessage {
    pub channel: u8,           // 0-15
    pub note: u8,              // 0-127 (note number to target)
    pub expression_type: PerNoteExpressionType,
}

/// Types of per-note expressions in MIDI 2.0
#[derive(Clone, Copy, Debug)]
pub enum PerNoteExpressionType {
    /// Per-note pitch bend: 32-bit value (0x80000000 = center/no bend)
    /// Range: 0x00000000 (-max) to 0xFFFFFFFF (+max), 0x80000000 = center
    PitchBend { value: u32 },
    /// Per-note pressure/aftertouch: 32-bit value (0 to 0xFFFFFFFF)
    Pressure { value: u32 },
    /// Registered per-note controller (indexed)
    RegisteredController { index: u8, value: u32 },
    /// Assignable per-note controller (indexed)
    AssignableController { index: u8, value: u32 },
    /// Per-note management: detach/reset
    Management { flags: u8 },
}

impl PerNoteExpressionMessage {
    /// Create a per-note pitch bend message
    /// value: 32-bit pitch bend (0x80000000 = center)
    pub fn pitch_bend(channel: u8, note: u8, value: u32) -> Self {
        Self {
            channel,
            note,
            expression_type: PerNoteExpressionType::PitchBend { value },
        }
    }

    /// Create a per-note pitch bend from semitones (-64.0 to +64.0 range)
    pub fn pitch_bend_semitones(channel: u8, note: u8, semitones: f32) -> Self {
        // Map -64.0..+64.0 to 0..0xFFFFFFFF with 0.0 at center
        let normalized = (semitones / 64.0).clamp(-1.0, 1.0);
        let value = ((normalized * 0.5 + 0.5) * (u32::MAX as f32)) as u32;
        Self::pitch_bend(channel, note, value)
    }

    /// Create a per-note pressure/aftertouch message
    /// value: 32-bit pressure (0 to 0xFFFFFFFF)
    pub fn pressure(channel: u8, note: u8, value: u32) -> Self {
        Self {
            channel,
            note,
            expression_type: PerNoteExpressionType::Pressure { value },
        }
    }

    /// Create a per-note pressure from normalized value (0.0 to 1.0)
    pub fn pressure_normalized(channel: u8, note: u8, normalized: f32) -> Self {
        let value = (normalized.clamp(0.0, 1.0) * (u32::MAX as f32)) as u32;
        Self::pressure(channel, note, value)
    }

    /// Create a registered per-note controller message
    pub fn registered_controller(channel: u8, note: u8, index: u8, value: u32) -> Self {
        Self {
            channel,
            note,
            expression_type: PerNoteExpressionType::RegisteredController { index, value },
        }
    }

    /// Create an assignable per-note controller message
    pub fn assignable_controller(channel: u8, note: u8, index: u8, value: u32) -> Self {
        Self {
            channel,
            note,
            expression_type: PerNoteExpressionType::AssignableController { index, value },
        }
    }

    /// Create a per-note management message
    /// flags: bit 0 = detach, bit 1 = reset
    pub fn management(channel: u8, note: u8, detach: bool, reset: bool) -> Self {
        let flags = (detach as u8) | ((reset as u8) << 1);
        Self {
            channel,
            note,
            expression_type: PerNoteExpressionType::Management { flags },
        }
    }
}

/// Combined MIDI message type for the ring buffer
#[derive(Clone, Copy, Debug)]
pub enum MidiMessage {
    Cc(CcMessage),
    Note(NoteMessage),
    PerNoteExpression(PerNoteExpressionMessage),
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
    pub expression_type: Option<String>, // Per-note expression type (pitch_bend, pressure, etc.)
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

/// Last-known project layout pushed by a host controller extension.
///
/// Process-wide (not per-instance): the extension POSTs the full project
/// in one call. `None` until the first push — all MCP tools that consume
/// this handle that as "no host extension running, fall back gracefully."
static PROJECT_LAYOUT: OnceLock<RwLock<Option<super::project::ProjectLayout>>> = OnceLock::new();

/// Activity log for GUI visualization
static ACTIVITY_LOG: OnceLock<Mutex<VecDeque<ActivityEvent>>> = OnceLock::new();

const MAX_ACTIVITY_LOG_SIZE: usize = 50;
const RING_BUFFER_SIZE: usize = 256;

fn registry() -> &'static RwLock<HashMap<String, InstanceEntry>> {
    REGISTRY.get_or_init(|| RwLock::new(HashMap::new()))
}

fn project_layout_lock() -> &'static RwLock<Option<super::project::ProjectLayout>> {
    PROJECT_LAYOUT.get_or_init(|| RwLock::new(None))
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

        consumer
    }

    /// Unregister a plugin instance
    pub fn unregister(id: &str) {
        let mut reg = registry().write().unwrap();
        if reg.remove(id).is_some() {
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

    /// Send a per-note expression message to an instance (called from MCP server thread)
    /// These are MIDI 2.0 only - they target specific notes that are currently playing
    pub fn send_per_note_expression(instance: &str, msg: PerNoteExpressionMessage) -> Result<(), &'static str> {
        let reg = registry().read().unwrap();
        let entry = Self::find_entry(&reg, instance)?;
        let instance_name = entry.name.clone();

        let mut producer = entry.producer.lock().map_err(|_| "Producer lock poisoned")?;
        producer
            .push(MidiMessage::PerNoteExpression(msg))
            .map_err(|_| "Queue full - audio thread not consuming fast enough")?;

        Self::log_expression_activity(&instance_name, &msg);

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
                expression_type: None,
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
                expression_type: None,
            });

            while log.len() > MAX_ACTIVITY_LOG_SIZE {
                log.pop_front();
            }
        }
    }

    /// Log per-note expression activity for GUI visualization
    fn log_expression_activity(instance_name: &str, msg: &PerNoteExpressionMessage) {
        if let Ok(mut log) = activity_log().try_lock() {
            let timestamp_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);

            let (expr_name, value) = match msg.expression_type {
                PerNoteExpressionType::PitchBend { value } => ("pitch_bend", (value >> 24) as u8),
                PerNoteExpressionType::Pressure { value } => ("pressure", (value >> 24) as u8),
                PerNoteExpressionType::RegisteredController { index, value } => {
                    let _ = index; // Displayed in expr_name would need allocation
                    ("reg_ctrl", (value >> 24) as u8)
                }
                PerNoteExpressionType::AssignableController { index, value } => {
                    let _ = index;
                    ("assign_ctrl", (value >> 24) as u8)
                }
                PerNoteExpressionType::Management { flags } => ("management", flags),
            };

            log.push_back(ActivityEvent {
                timestamp_ms,
                instance: instance_name.to_string(),
                channel: msg.channel,
                cc: None,
                value,
                note: Some(msg.note),
                is_note_on: None,
                expression_type: Some(expr_name.to_string()),
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
            // Sync with FugueBridge
            crate::fugue::FugueBridge::update_name(&id, new_name);
            Ok(old_name)
        } else {
            Err("Instance not found")
        }
    }

    /// List all registered instances as (id, name) pairs
    pub fn list_instances() -> Vec<(String, String)> {
        registry()
            .read()
            .unwrap()
            .iter()
            .map(|(id, e)| (id.clone(), e.name.clone()))
            .collect()
    }

    /// Get the current name for a given instance ID
    pub fn get_name(id: &str) -> Option<String> {
        registry().read().unwrap().get(id).map(|e| e.name.clone())
    }

    /// Store the latest project layout pushed by a host controller extension.
    /// Replaces any prior snapshot — the extension sends the full project on
    /// every change.
    pub fn set_project_layout(layout: super::project::ProjectLayout) {
        *project_layout_lock().write().unwrap() = Some(layout);
    }

    /// Get a clone of the current project layout, if any. Cloning here keeps
    /// the read lock hold brief and the returned value owned, which matches
    /// how the MCP tools consume it.
    pub fn get_project_layout() -> Option<super::project::ProjectLayout> {
        project_layout_lock().read().unwrap().clone()
    }

    /// Resolve an instance reference (name, ID, or the sentinel "default")
    /// to its stable ID. Returns the MCP-conventional error message so
    /// callers can surface it directly.
    pub fn resolve_instance_id(instance: &str) -> Result<String, &'static str> {
        let reg = registry().read().unwrap();
        Self::find_id(&reg, instance)
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

    /// Get params for the default instance (first registered)
    /// Used by GUI server for HTTP API endpoints
    pub fn get_default_params() -> Option<ParamsRef> {
        let reg = registry().read().ok()?;
        reg.values().next().map(|e| Arc::clone(&e.params))
    }

    /// Set a parameter slot value (called from MCP server)
    ///
    /// If the slot is mapped to a CC, sends the MIDI CC message through the ring buffer.
    pub fn set_param(instance: &str, slot: usize, value: f64) -> Result<(), &'static str> {
        let reg = registry().read().unwrap();
        let entry = Self::find_entry(&reg, instance)?;

        if slot >= entry.params.len() {
            return Err("Slot index out of range");
        }

        // Set the slot value and get CC info if mapped
        if let Some((channel, cc, midi_value)) = entry.params.set_slot_value(slot, value) {
            // Send MIDI CC through the ring buffer
            let msg = CcMessage::new(channel, cc, midi_value);
            let mut producer = entry.producer.lock().map_err(|_| "Producer lock poisoned")?;
            producer.push(MidiMessage::Cc(msg)).map_err(|_| "Queue full")?;

            Self::log_cc_activity(&entry.name, &msg);
        }

        Ok(())
    }

    /// Rename a parameter slot (called from MCP server)
    pub fn rename_slot(instance: &str, slot: usize, name: &str) -> Result<String, &'static str> {
        let reg = registry().read().unwrap();
        let entry = Self::find_entry(&reg, instance)?;

        let old_name = entry
            .params
            .rename_slot(slot, name)
            .ok_or("Slot index out of range")?;
        Ok(old_name)
    }

    /// Append a new CC slot on the given instance. Returns the new slot's
    /// index.
    pub fn add_slot(instance: &str, cc: u8, name: &str) -> Result<usize, &'static str> {
        let reg = registry().read().unwrap();
        let entry = Self::find_entry(&reg, instance)?;
        let idx = entry.params.add_slot(cc, name);
        Ok(idx)
    }

    /// Remove a CC slot by index. Shifts subsequent slots down.
    pub fn remove_slot(instance: &str, slot: usize) -> Result<(), &'static str> {
        let reg = registry().read().unwrap();
        let entry = Self::find_entry(&reg, instance)?;
        if !entry.params.remove_slot(slot) {
            return Err("Slot index out of range");
        }
        Ok(())
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

        if slot >= entry.params.len() {
            return Err("Slot index out of range");
        }

        entry.params.start_learning(slot);
        Ok(())
    }

    /// Cancel CC learning mode on all slots
    pub fn cancel_learn(instance: &str) -> Result<(), &'static str> {
        let reg = registry().read().unwrap();
        let entry = Self::find_entry(&reg, instance)?;
        entry.params.cancel_learning();
        Ok(())
    }

    /// Manually map a CC number to a slot
    pub fn map_slot(instance: &str, slot: usize, cc: u8, channel: u8) -> Result<(), &'static str> {
        let reg = registry().read().unwrap();
        let entry = Self::find_entry(&reg, instance)?;

        let slot_ref = entry.params.get(slot).ok_or("Slot index out of range")?;
        slot_ref.set_cc(cc);
        slot_ref.set_channel(channel);
        Ok(())
    }

    /// Clear CC mapping from a slot
    pub fn unmap_slot(instance: &str, slot: usize) -> Result<(), &'static str> {
        let reg = registry().read().unwrap();
        let entry = Self::find_entry(&reg, instance)?;

        let slot_ref = entry.params.get(slot).ok_or("Slot index out of range")?;
        slot_ref.cc_number.store(255, std::sync::atomic::Ordering::Relaxed);
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
