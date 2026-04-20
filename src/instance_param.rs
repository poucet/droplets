//! Plugin parameter surface.
//!
//! Two kinds of params are exposed to the host:
//!
//! 1. **Instance-ID** (read-only) — a fixed-value parameter whose displayed text
//!    is the Droplets instance ID (`droplets-a1b2c3d4`). Host-side controller
//!    scripts (e.g. the Bitwig extension) correlate a Droplets-device-on-track
//!    with a plugin instance running in the shared MCP server by scanning
//!    direct-parameter display strings for this pattern.
//!
//! 2. **16 automatable slot params** (`Slot 1`..`Slot 16`) — normalized 0..1
//!    floats backed by [`crate::params::DropletParams`]. These supersede the
//!    original CC-only design: modern soft synths (Bitwig Polysynth, Serum,
//!    Pigments, Omnisphere, …) expose host-automation parameters, not MIDI CC
//!    mappings, so exposing Droplets slots as host params lets users / the
//!    extension route them to synth params natively — right-click → Map in
//!    Bitwig, Configure → drag in Ableton. CC emission on slot change still
//!    happens (see [`crate::params::DropletParams::set_slot`]) so hardware
//!    targets continue to work.
//!
//! Slot param display strings follow the slot's current `name` field. The
//! default name is a generic `Slot N`, but the Bitwig extension rewrites it
//! to the primary instrument's Remote Controls Page 1 parameter name on
//! layout rebuilds, so automation lanes and the host's param list show the
//! user-meaningful name (e.g. "Filter Cutoff") rather than the slot number.

use clack_extensions::params::*;
use clack_plugin::events::io::{InputEvents, OutputEvents};
use clack_plugin::events::spaces::CoreEventSpace;
use clack_plugin::events::UnknownEvent;
use clack_plugin::utils::ClapId;
use std::ffi::CStr;
use std::fmt::Write as _;

use crate::params::{DropletParams, NUM_CC_SLOTS};
use crate::{DropletMainThread, DropletShared};
use crate::midi::DropletMidiProcessor;

/// The ClapId for the instance-ID parameter. Stable across the plugin's
/// lifetime so saved projects keep addressing the same param.
pub const INSTANCE_ID_PARAM: ClapId = ClapId::new(1);

/// Numeric value of the instance-ID param. Meaningless — the only thing that
/// matters is the displayed-value string, which we always render as the
/// `DropletShared::instance_id` field regardless of this value.
const INSTANCE_ID_VALUE: f64 = 0.0;

/// Base ClapId for slot params. Slot `i` is `SLOT_PARAM_BASE + i`. Chosen
/// well clear of the instance-ID param so collisions with future additions
/// are unlikely.
pub const SLOT_PARAM_BASE: u32 = 100;

/// Returns the slot index for a ClapId, if that id maps to a slot param.
fn slot_for_param(param_id: ClapId) -> Option<usize> {
    let raw: u32 = param_id.into();
    raw.checked_sub(SLOT_PARAM_BASE)
        .map(|i| i as usize)
        .filter(|&i| i < NUM_CC_SLOTS)
}

/// Apply an incoming param-value event to the shared param store. Called from
/// both the main-thread flush and the audio-thread flush/process paths so
/// host-driven automation is reflected regardless of which side the host
/// delivered the event on.
pub(crate) fn apply_slot_param_event(params: &DropletParams, event: &UnknownEvent) {
    let Some(CoreEventSpace::ParamValue(pv)) = event.as_core_event() else { return };
    let Some(param_id) = pv.param_id() else { return };
    let Some(slot) = slot_for_param(param_id) else { return };
    // Host-driven writes bypass CC emission. The emit-CC-on-set path exists
    // so LLM/MCP writes can reach external hardware; host-driven writes are
    // already the host automating the plugin param directly, and re-emitting
    // a CC would be redundant (and risks feedback loops with DAW-side CC
    // routings).
    params.slots[slot].value.store(pv.value().clamp(0.0, 1.0));
}

impl<'a> PluginMainThreadParams for DropletMainThread<'a> {
    fn count(&mut self) -> u32 {
        // 1 instance-ID + NUM_CC_SLOTS slot params.
        1 + NUM_CC_SLOTS as u32
    }

    fn get_info(&mut self, param_index: u32, info: &mut ParamInfoWriter) {
        if param_index == 0 {
            info.set(&ParamInfo {
                id: INSTANCE_ID_PARAM,
                // Read-only + hidden from automation: the host should neither
                // write to this nor try to automate it. It exists as an
                // information surface only.
                flags: ParamInfoFlags::IS_READONLY,
                cookie: Default::default(),
                name: b"Instance",
                module: b"",
                min_value: 0.0,
                max_value: 1.0,
                default_value: INSTANCE_ID_VALUE,
            });
            return;
        }

        let slot = param_index as usize - 1;
        if slot >= NUM_CC_SLOTS {
            return;
        }

        // Slot name is dynamic (extension rewrites it). CLAP ParamInfo name
        // is a fixed-size buffer: write the current slot name into a local
        // stack buffer so it's valid for the set() call.
        let name = self.shared.params.slots[slot].get_name();
        let mut name_buf = [0u8; 64];
        let bytes = name.as_bytes();
        let n = bytes.len().min(name_buf.len() - 1);
        name_buf[..n].copy_from_slice(&bytes[..n]);

        info.set(&ParamInfo {
            id: ClapId::new(SLOT_PARAM_BASE + slot as u32),
            flags: ParamInfoFlags::IS_AUTOMATABLE,
            cookie: Default::default(),
            name: &name_buf[..n],
            module: b"Slots",
            min_value: 0.0,
            max_value: 1.0,
            default_value: 0.0,
        });
    }

    fn get_value(&mut self, param_id: ClapId) -> Option<f64> {
        if param_id == INSTANCE_ID_PARAM {
            return Some(INSTANCE_ID_VALUE);
        }
        slot_for_param(param_id).map(|i| self.shared.params.slots[i].value.load())
    }

    fn value_to_text(
        &mut self,
        param_id: ClapId,
        value: f64,
        writer: &mut ParamDisplayWriter,
    ) -> std::fmt::Result {
        if param_id == INSTANCE_ID_PARAM {
            return write!(writer, "{}", self.shared.instance_id);
        }
        if slot_for_param(param_id).is_some() {
            // Render as percentage — matches the tone of "slot value 0..1 is
            // a normalized control" without committing to a unit the mapped
            // target's range actually uses.
            return write!(writer, "{:.1}%", value * 100.0);
        }
        Err(std::fmt::Error)
    }

    fn text_to_value(&mut self, param_id: ClapId, text: &CStr) -> Option<f64> {
        if param_id == INSTANCE_ID_PARAM {
            return None;
        }
        if slot_for_param(param_id).is_some() {
            let s = text.to_str().ok()?;
            let s = s.strip_suffix('%').unwrap_or(s).trim();
            let pct: f64 = s.parse().ok()?;
            return Some((pct / 100.0).clamp(0.0, 1.0));
        }
        None
    }

    fn flush(
        &mut self,
        input_parameter_changes: &InputEvents,
        _output_parameter_changes: &mut OutputEvents,
    ) {
        for event in input_parameter_changes {
            apply_slot_param_event(&self.shared.params, event);
        }
    }
}

impl<'a> PluginAudioProcessorParams for DropletMidiProcessor<'a> {
    fn flush(
        &mut self,
        input_parameter_changes: &InputEvents,
        _output_parameter_changes: &mut OutputEvents,
    ) {
        for event in input_parameter_changes {
            apply_slot_param_event(&self.shared.params, event);
        }
    }
}

// Silence the "unused" warning when only the audio side is compiled in some
// configurations; the impls above are used via trait dispatch.
#[allow(dead_code)]
fn _ensure_types_linked(_s: &DropletShared<'_>) {}
