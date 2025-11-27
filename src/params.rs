//! Automatable parameters for Simply Droplets
//!
//! Exposes CC slots that can be:
//! - Set via MCP by AI
//! - Mapped to other plugin parameters via DAW modulation
//! - Renamed to reflect what they control

use clack_extensions::params::*;
use clack_plugin::events::event_types::ParamValueEvent;
use clack_plugin::events::UnknownEvent;
use clack_plugin::prelude::*;
use clack_plugin::utils::Cookie;
use std::ffi::CStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;

/// Number of CC slots available
pub const NUM_CC_SLOTS: usize = 16;

/// Atomic f64 storage for real-time safe parameter access
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

/// A single CC slot with value and custom name
pub struct CcSlot {
    /// Current value (0.0 - 1.0, normalized)
    pub value: AtomicF64,
    /// Custom name for this slot (e.g., "Vital Filter Cutoff")
    pub name: RwLock<String>,
}

impl CcSlot {
    pub fn new(index: usize) -> Self {
        Self {
            value: AtomicF64::new(0.0),
            name: RwLock::new(format!("CC Slot {}", index + 1)),
        }
    }

    pub fn get_name(&self) -> String {
        self.name.read().unwrap().clone()
    }

    pub fn set_name(&self, name: &str) {
        *self.name.write().unwrap() = name.to_string();
    }
}

/// All plugin parameters
pub struct DropletParams {
    pub slots: [CcSlot; NUM_CC_SLOTS],
}

impl DropletParams {
    pub fn new() -> Self {
        Self {
            slots: std::array::from_fn(|i| CcSlot::new(i)),
        }
    }

    /// Get parameter ID for a slot index
    pub fn slot_id(index: usize) -> ClapId {
        ClapId::new(index as u32)
    }

    /// Get slot index from parameter ID
    pub fn slot_index(id: ClapId) -> Option<usize> {
        let index = id.get() as usize;
        if index < NUM_CC_SLOTS {
            Some(index)
        } else {
            None
        }
    }

    /// Set a slot value (called from MCP)
    pub fn set_slot(&self, index: usize, value: f64) {
        if index < NUM_CC_SLOTS {
            self.slots[index].value.store(value.clamp(0.0, 1.0));
        }
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
    pub fn rename_slot(&self, index: usize, name: &str) {
        if index < NUM_CC_SLOTS {
            self.slots[index].set_name(name);
        }
    }

    /// Get slot info for GUI/MCP
    pub fn get_slot_info(&self, index: usize) -> Option<(String, f64)> {
        if index < NUM_CC_SLOTS {
            Some((self.slots[index].get_name(), self.slots[index].value.load()))
        } else {
            None
        }
    }

    /// Handle parameter events from the host
    pub fn handle_event(&self, event: &UnknownEvent) {
        if let Some(param_event) = event.as_event::<ParamValueEvent>() {
            if let Some(param_id) = param_event.param_id() {
                if let Some(index) = Self::slot_index(param_id) {
                    self.slots[index].value.store(param_event.value());
                }
            }
        }
    }
}

impl Default for DropletParams {
    fn default() -> Self {
        Self::new()
    }
}

use crate::DropletMainThread;

impl<'a> PluginMainThreadParams for DropletMainThread<'a> {
    fn count(&mut self) -> u32 {
        NUM_CC_SLOTS as u32
    }

    fn get_info(&mut self, param_index: u32, info: &mut ParamInfoWriter) {
        let index = param_index as usize;
        if index < NUM_CC_SLOTS {
            let name = self.shared.params.slots[index].get_name();
            let name_bytes = name.as_bytes();

            info.set(&ParamInfo {
                id: DropletParams::slot_id(index),
                flags: ParamInfoFlags::IS_AUTOMATABLE | ParamInfoFlags::IS_MODULATABLE,
                cookie: Cookie::empty(),
                name: name_bytes,
                module: b"CC Slots",
                min_value: 0.0,
                max_value: 1.0,
                default_value: 0.0,
            });
        }
    }

    fn get_value(&mut self, param_id: ClapId) -> Option<f64> {
        DropletParams::slot_index(param_id).map(|i| self.shared.params.get_slot(i))
    }

    fn value_to_text(
        &mut self,
        _param_id: ClapId,
        value: f64,
        writer: &mut ParamDisplayWriter,
    ) -> core::fmt::Result {
        use core::fmt::Write;
        // Display as percentage
        write!(writer, "{:.1}%", value * 100.0)
    }

    fn text_to_value(&mut self, _param_id: ClapId, text: &CStr) -> Option<f64> {
        let text = text.to_str().ok()?;
        let text = text.trim().trim_end_matches('%');
        let value: f64 = text.parse().ok()?;
        Some((value / 100.0).clamp(0.0, 1.0))
    }

    fn flush(
        &mut self,
        input_parameter_changes: &InputEvents,
        _output_parameter_changes: &mut OutputEvents,
    ) {
        for event in input_parameter_changes.iter() {
            self.shared.params.handle_event(&event);
        }
    }
}
