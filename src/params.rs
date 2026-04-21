//! CC Slot mapping system for Droplets.
//!
//! Each slot pairs a MIDI CC number + channel + human-readable name. When the
//! user wiggles a slot from the UI or an MCP tool sets one, the plugin emits
//! that CC on its MIDI output — the DAW routes it to whichever target the
//! user has hooked up (HW CC modulator in Bitwig, MIDI-learn on hardware, etc.).
//!
//! Slots are **dynamic**: users can add, remove, rename, and renumber them at
//! runtime. The internal storage is a `RwLock<Vec<Arc<CcSlot>>>`. Slots are
//! only ever accessed from the main/MCP threads — the audio thread drives
//! MIDI output through a separate ring buffer, not by reading these slots
//! directly — so the lock overhead is fine.

use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

/// Atomic f64 storage for real-time safe value access
#[repr(transparent)]
pub struct AtomicF64(AtomicU64);

impl AtomicF64 {
    pub const fn new(val: f64) -> Self {
        Self(AtomicU64::new(val.to_bits()))
    }

    pub fn load(&self) -> f64 {
        f64::from_bits(self.0.load(Ordering::Relaxed))
    }

    pub fn store(&self, val: f64) {
        self.0.store(val.to_bits(), Ordering::Relaxed);
    }
}

/// A single CC slot with mapping info.
pub struct CcSlot {
    /// CC number this slot is mapped to (0-127, or 255 for unmapped)
    pub cc_number: AtomicU8,
    /// MIDI channel for this slot (0-15)
    pub channel: AtomicU8,
    /// Current value (0.0 - 1.0, normalized)
    pub value: AtomicF64,
    /// Custom name for this slot (e.g., "Vital Filter Cutoff")
    pub name: RwLock<String>,
    /// Is this slot in learning mode (waiting for incoming CC)?
    pub learning: AtomicBool,
}

impl CcSlot {
    pub fn new(cc: u8, name: &str) -> Self {
        Self {
            cc_number: AtomicU8::new(cc.min(127)),
            channel: AtomicU8::new(0),
            value: AtomicF64::new(0.0),
            name: RwLock::new(name.to_string()),
            learning: AtomicBool::new(false),
        }
    }

    pub fn is_mapped(&self) -> bool {
        self.cc_number.load(Ordering::Relaxed) < 128
    }

    pub fn get_cc(&self) -> Option<u8> {
        let cc = self.cc_number.load(Ordering::Relaxed);
        if cc < 128 { Some(cc) } else { None }
    }

    pub fn set_cc(&self, cc: u8) {
        self.cc_number.store(cc.min(127), Ordering::Relaxed);
    }

    pub fn get_channel(&self) -> u8 {
        self.channel.load(Ordering::Relaxed)
    }

    pub fn set_channel(&self, ch: u8) {
        self.channel.store(ch.min(15), Ordering::Relaxed);
    }

    pub fn get_name(&self) -> String {
        self.name.read().unwrap().clone()
    }

    pub fn set_name(&self, name: &str) {
        *self.name.write().unwrap() = name.to_string();
    }

    pub fn start_learning(&self) {
        self.learning.store(true, Ordering::Relaxed);
    }

    pub fn stop_learning(&self) {
        self.learning.store(false, Ordering::Relaxed);
    }

    pub fn is_learning(&self) -> bool {
        self.learning.load(Ordering::Relaxed)
    }
}

/// Initial slot set for a fresh plugin instance. Covers CC 1 (mod wheel) and
/// the CC 71–76 MPE/GM conventions (filter + ADSR + vibrato) that most soft
/// synths either respond to out of the box or have a preset-author mapping
/// for. Users add/remove/rename beyond this via the Settings UI.
pub fn default_slots() -> Vec<(u8, &'static str)> {
    vec![
        (1, "Mod Wheel"),
        (71, "Filter Resonance"),
        (72, "Release"),
        (73, "Attack"),
        (74, "Filter Cutoff"),
        (75, "Decay"),
        (76, "Vibrato Rate"),
    ]
}

/// All plugin slots and AI message log.
pub struct DropletParams {
    slots: RwLock<Vec<Arc<CcSlot>>>,
    /// Recent AI messages (for UI display)
    ai_messages: RwLock<Vec<AiMessage>>,
}

/// An AI message to display in the UI
#[derive(Clone, serde::Serialize)]
pub struct AiMessage {
    pub timestamp_ms: u64,
    pub tool_name: String,
    pub message: String,
}

impl DropletParams {
    pub fn new() -> Self {
        let initial: Vec<Arc<CcSlot>> = default_slots()
            .into_iter()
            .map(|(cc, name)| Arc::new(CcSlot::new(cc, name)))
            .collect();
        Self {
            slots: RwLock::new(initial),
            ai_messages: RwLock::new(Vec::new()),
        }
    }

    /// Create an empty params instance (no default slots). Used when state
    /// load will repopulate the slot list from a persisted array.
    pub fn empty() -> Self {
        Self {
            slots: RwLock::new(Vec::new()),
            ai_messages: RwLock::new(Vec::new()),
        }
    }

    /// Pull a specific slot by index. Returns an `Arc<CcSlot>` so the caller
    /// can operate on it without holding the slots lock.
    pub fn get(&self, index: usize) -> Option<Arc<CcSlot>> {
        self.slots.read().unwrap().get(index).cloned()
    }

    /// Number of slots currently configured.
    pub fn len(&self) -> usize {
        self.slots.read().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.read().unwrap().is_empty()
    }

    /// Append a new slot. Returns its index.
    pub fn add_slot(&self, cc: u8, name: &str) -> usize {
        let mut slots = self.slots.write().unwrap();
        slots.push(Arc::new(CcSlot::new(cc, name)));
        slots.len() - 1
    }

    /// Remove a slot by index. Returns true if a slot was removed.
    pub fn remove_slot(&self, index: usize) -> bool {
        let mut slots = self.slots.write().unwrap();
        if index < slots.len() {
            slots.remove(index);
            true
        } else {
            false
        }
    }

    /// Replace the entire slot list. Used by state-load to restore a saved
    /// configuration atomically.
    pub fn replace_slots(&self, new: Vec<Arc<CcSlot>>) {
        *self.slots.write().unwrap() = new;
    }

    /// Set a slot value (called from MCP) - returns the CC info if mapped.
    pub fn set_slot_value(&self, index: usize, value: f64) -> Option<(u8, u8, u8)> {
        let slot = self.get(index)?;
        let clamped = value.clamp(0.0, 1.0);
        slot.value.store(clamped);

        if let Some(cc) = slot.get_cc() {
            let channel = slot.get_channel();
            let midi_value = (clamped * 127.0).round() as u8;
            return Some((channel, cc, midi_value));
        }
        None
    }

    /// Rename a slot. Returns the previous name if the slot existed.
    pub fn rename_slot(&self, index: usize, name: &str) -> Option<String> {
        let slot = self.get(index)?;
        let old = slot.get_name();
        slot.set_name(name);
        Some(old)
    }

    /// Start learning mode for a slot — stops learning on all others.
    pub fn start_learning(&self, index: usize) {
        let slots = self.slots.read().unwrap();
        for (i, slot) in slots.iter().enumerate() {
            if i == index {
                slot.start_learning();
            } else {
                slot.stop_learning();
            }
        }
    }

    /// Cancel all learning.
    pub fn cancel_learning(&self) {
        for slot in self.slots.read().unwrap().iter() {
            slot.stop_learning();
        }
    }

    /// Check if any slot is learning and process incoming CC. Returns the
    /// index that learned, if any.
    pub fn process_learn(&self, channel: u8, cc: u8) -> Option<usize> {
        let slots = self.slots.read().unwrap();
        for (i, slot) in slots.iter().enumerate() {
            if slot.is_learning() {
                slot.set_cc(cc);
                slot.set_channel(channel);
                slot.stop_learning();
                return Some(i);
            }
        }
        None
    }

    /// Snapshot every slot's state as a `SlotInfo` list.
    pub fn get_all_slots(&self) -> Vec<SlotInfo> {
        self.slots
            .read()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(i, slot)| SlotInfo {
                index: i,
                name: slot.get_name(),
                cc: slot.get_cc(),
                channel: slot.get_channel(),
                value: slot.value.load(),
                learning: slot.is_learning(),
            })
            .collect()
    }

    /// Snapshot a single slot.
    pub fn get_slot_info(&self, index: usize) -> Option<SlotInfo> {
        let slot = self.get(index)?;
        Some(SlotInfo {
            index,
            name: slot.get_name(),
            cc: slot.get_cc(),
            channel: slot.get_channel(),
            value: slot.value.load(),
            learning: slot.is_learning(),
        })
    }

    /// Add an AI message to the log.
    pub fn add_ai_message(&self, tool_name: &str, message: &str) {
        let timestamp_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let msg = AiMessage {
            timestamp_ms,
            tool_name: tool_name.to_string(),
            message: message.to_string(),
        };

        let mut messages = self.ai_messages.write().unwrap();
        messages.push(msg);

        if messages.len() > 50 {
            let drain_count = messages.len() - 50;
            messages.drain(0..drain_count);
        }
    }

    pub fn get_ai_messages(&self) -> Vec<AiMessage> {
        self.ai_messages.read().unwrap().clone()
    }
}

impl Default for DropletParams {
    fn default() -> Self {
        Self::new()
    }
}

/// Slot info for serialization (internal use - API type in gui::api::SlotInfo)
#[derive(Clone, serde::Serialize, schemars::JsonSchema)]
pub struct SlotInfo {
    pub index: usize,
    pub name: String,
    pub cc: Option<u8>,
    pub channel: u8,
    pub value: f64,
    pub learning: bool,
}
