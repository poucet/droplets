//! Port declarations for Droplets
//!
//! Declares a stereo audio pass-through (required by the clap-wrapper's
//! VST3 bridge — Ableton refuses to instantiate the plugin when it
//! reports zero audio buses in this wrapper config) plus note I/O for
//! MIDI input (CC-learn) and MIDI output (to downstream instruments).

use clack_extensions::audio_ports::*;
use clack_extensions::note_ports::*;
use clack_plugin::prelude::*;

use crate::DropletMainThread;

impl<'a> PluginAudioPortsImpl for DropletMainThread<'a> {
    fn count(&mut self, _is_input: bool) -> u32 {
        1
    }

    fn get(&mut self, index: u32, _is_input: bool, writer: &mut AudioPortInfoWriter) {
        if index == 0 {
            writer.set(&AudioPortInfo {
                id: ClapId::new(0),
                name: b"Audio",
                channel_count: 2,
                flags: AudioPortFlags::IS_MAIN,
                port_type: Some(AudioPortType::STEREO),
                in_place_pair: None,
            });
        }
    }
}

impl<'a> PluginNotePortsImpl for DropletMainThread<'a> {
    fn count(&mut self, _is_input: bool) -> u32 {
        1
    }

    fn get(&mut self, index: u32, is_input: bool, writer: &mut NotePortInfoWriter) {
        if index == 0 {
            if is_input {
                // Input: Accept MIDI 1.0/2.0 for CC learning
                writer.set(&NotePortInfo {
                    id: ClapId::new(1),
                    name: b"MIDI In",
                    supported_dialects: NoteDialects::CLAP | NoteDialects::MIDI | NoteDialects::MIDI2,
                    preferred_dialect: Some(NoteDialect::Midi),
                });
            } else {
                // Output: Prefer MIDI dialect for CC support (CLAP has no native CC type)
                // Also support CLAP notes and MIDI 2.0 for high-res
                writer.set(&NotePortInfo {
                    id: ClapId::new(2),
                    name: b"MIDI Out",
                    supported_dialects: NoteDialects::CLAP | NoteDialects::MIDI | NoteDialects::MIDI2,
                    preferred_dialect: Some(NoteDialect::Midi),
                });
            }
        }
    }
}
