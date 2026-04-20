//! Read-only plugin parameter exposing the instance ID.
//!
//! Exists so host-side controller scripts (e.g. the Bitwig extension) can
//! correlate a Droplets device-on-track with a plugin instance running in the
//! shared MCP server. The parameter is read-only, has no musical meaning, and
//! is shown as a formatted string by hosts that display parameter values —
//! `droplets-a1b2c3d4` etc.
//!
//! Controller-side reading path (Bitwig API): `device.setObservedParameterIds([id])`
//! + `addDirectParameterValueDisplayObserver` gives the formatted string we
//! write here via `value_to_text`.

use clack_extensions::params::*;
use clack_plugin::events::io::{InputEvents, OutputEvents};
use clack_plugin::utils::ClapId;
use std::ffi::CStr;
use std::fmt::Write as _;

use crate::{DropletMainThread, DropletShared};
use crate::midi::DropletMidiProcessor;

/// The ClapId for the instance-ID parameter. Picked arbitrarily; must be
/// stable across the plugin's lifetime so saved projects keep addressing
/// the same param.
pub const INSTANCE_ID_PARAM: ClapId = ClapId::new(1);

/// Numeric value of the instance-ID param. Meaningless — the only thing that
/// matters is the displayed-value string, which we always render as the
/// `DropletShared::instance_id` field regardless of this value.
const INSTANCE_ID_VALUE: f64 = 0.0;

/// Format a parameter display string from a given instance ID.
///
/// Extracted so tests can exercise it without standing up a plugin.
fn format_instance_id(writer: &mut ParamDisplayWriter, instance_id: &str) -> std::fmt::Result {
    write!(writer, "{}", instance_id)
}

impl<'a> PluginMainThreadParams for DropletMainThread<'a> {
    fn count(&mut self) -> u32 {
        1
    }

    fn get_info(&mut self, param_index: u32, info: &mut ParamInfoWriter) {
        if param_index != 0 {
            return;
        }
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
    }

    fn get_value(&mut self, param_id: ClapId) -> Option<f64> {
        if param_id == INSTANCE_ID_PARAM {
            Some(INSTANCE_ID_VALUE)
        } else {
            None
        }
    }

    fn value_to_text(
        &mut self,
        param_id: ClapId,
        _value: f64,
        writer: &mut ParamDisplayWriter,
    ) -> std::fmt::Result {
        if param_id == INSTANCE_ID_PARAM {
            format_instance_id(writer, &self.shared.instance_id)
        } else {
            Err(std::fmt::Error)
        }
    }

    fn text_to_value(&mut self, _param_id: ClapId, _text: &CStr) -> Option<f64> {
        // Read-only — text cannot be parsed back to a value.
        None
    }

    fn flush(
        &mut self,
        _input_parameter_changes: &InputEvents,
        _output_parameter_changes: &mut OutputEvents,
    ) {
        // Read-only param — nothing to handle.
    }
}

impl<'a> PluginAudioProcessorParams for DropletMidiProcessor<'a> {
    fn flush(
        &mut self,
        _input_parameter_changes: &InputEvents,
        _output_parameter_changes: &mut OutputEvents,
    ) {
        // Read-only param — nothing to handle.
    }
}

// Silence the "unused" warning when only the audio side is compiled in some
// configurations; the impls above are used via trait dispatch.
#[allow(dead_code)]
fn _ensure_types_linked(_s: &DropletShared<'_>) {}
