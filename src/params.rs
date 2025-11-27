//! CC Slot mapping system for Simply Droplets
//!
//! Each slot represents a CC mapping:
//! - CC number (learned from incoming MIDI CC)
//! - Target name (label like "Vital Filter Cutoff")
//! - Current value (0.0 - 1.0, set by AI via MCP)
//!
//! When the AI sets a slot value, the plugin outputs the corresponding
//! MIDI CC message which the DAW routes to the target plugin.

use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::sync::RwLock;

/// Number of CC slots available
pub const NUM_CC_SLOTS: usize = 16;

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

/// A single CC slot with mapping info
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
    pub fn new(index: usize) -> Self {
        Self {
            cc_number: AtomicU8::new(255), // 255 = unmapped
            channel: AtomicU8::new(0),     // Default to channel 1
            value: AtomicF64::new(0.0),
            name: RwLock::new(format!("Slot {}", index + 1)),
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

    pub fn clear_cc(&self) {
        self.cc_number.store(255, Ordering::Relaxed);
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

/// All plugin slots and AI message log
pub struct DropletParams {
    pub slots: [CcSlot; NUM_CC_SLOTS],
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
        Self {
            slots: std::array::from_fn(|i| CcSlot::new(i)),
            ai_messages: RwLock::new(Vec::new()),
        }
    }

    /// Set a slot value (called from MCP) - returns the CC info if mapped
    pub fn set_slot(&self, index: usize, value: f64) -> Option<(u8, u8, u8)> {
        if index < NUM_CC_SLOTS {
            let clamped = value.clamp(0.0, 1.0);
            self.slots[index].value.store(clamped);

            // Return (channel, cc, value) if mapped
            if let Some(cc) = self.slots[index].get_cc() {
                let channel = self.slots[index].get_channel();
                let midi_value = (clamped * 127.0).round() as u8;
                return Some((channel, cc, midi_value));
            }
        }
        None
    }

    /// Get a slot value
    pub fn get_slot(&self, index: usize) -> f64 {
        if index < NUM_CC_SLOTS {
            self.slots[index].value.load()
        } else {
            0.0
        }
    }

    /// Rename a slot
    pub fn rename_slot(&self, index: usize, name: &str) -> Option<String> {
        if index < NUM_CC_SLOTS {
            let old_name = self.slots[index].get_name();
            self.slots[index].set_name(name);
            Some(old_name)
        } else {
            None
        }
    }

    /// Start learning mode for a slot
    pub fn start_learning(&self, index: usize) {
        if index < NUM_CC_SLOTS {
            // Stop learning on all other slots
            for (i, slot) in self.slots.iter().enumerate() {
                if i == index {
                    slot.start_learning();
                } else {
                    slot.stop_learning();
                }
            }
        }
    }

    /// Check if any slot is learning and process incoming CC
    /// Returns the slot index that learned, if any
    pub fn process_learn(&self, channel: u8, cc: u8) -> Option<usize> {
        for (i, slot) in self.slots.iter().enumerate() {
            if slot.is_learning() {
                slot.set_cc(cc);
                slot.set_channel(channel);
                slot.stop_learning();
                return Some(i);
            }
        }
        None
    }

    /// Cancel all learning
    pub fn cancel_learning(&self) {
        for slot in self.slots.iter() {
            slot.stop_learning();
        }
    }

    /// Get slot info for GUI/MCP
    pub fn get_slot_info(&self, index: usize) -> Option<SlotInfo> {
        if index < NUM_CC_SLOTS {
            let slot = &self.slots[index];
            Some(SlotInfo {
                index,
                name: slot.get_name(),
                cc: slot.get_cc(),
                channel: slot.get_channel(),
                value: slot.value.load(),
                learning: slot.is_learning(),
            })
        } else {
            None
        }
    }

    /// Get all slots info
    pub fn get_all_slots(&self) -> Vec<SlotInfo> {
        (0..NUM_CC_SLOTS)
            .filter_map(|i| self.get_slot_info(i))
            .collect()
    }

    /// Add an AI message to the log
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

        // Keep only last 50 messages
        if messages.len() > 50 {
            let drain_count = messages.len() - 50;
            messages.drain(0..drain_count);
        }
    }

    /// Get recent AI messages
    pub fn get_ai_messages(&self) -> Vec<AiMessage> {
        self.ai_messages.read().unwrap().clone()
    }
}

impl Default for DropletParams {
    fn default() -> Self {
        Self::new()
    }
}

/// Slot info for serialization
#[derive(Clone, serde::Serialize)]
pub struct SlotInfo {
    pub index: usize,
    pub name: String,
    pub cc: Option<u8>,
    pub channel: u8,
    pub value: f64,
    pub learning: bool,
}
