//! Port declarations for Simply Droplets
//!
//! Declares audio ports (stereo pass-through) and note ports (MIDI CC output).

use clack_extensions::audio_ports::*;
use clack_extensions::note_ports::*;
use clack_plugin::prelude::*;

use crate::DropletMainThread;

impl<'a> PluginAudioPortsImpl for DropletMainThread<'a> {
    fn count(&mut self, _is_input: bool) -> u32 {
        1 // One stereo port for input and output
    }

    fn get(&mut self, index: u32, _is_input: bool, writer: &mut AudioPortInfoWriter) {
        if index == 0 {
            writer.set(&AudioPortInfo {
                id: ClapId::new(0),
                name: b"main",
                channel_count: 2,
                flags: AudioPortFlags::IS_MAIN,
                port_type: Some(AudioPortType::STEREO),
                in_place_pair: None,
            });
        }
    }
}

impl<'a> PluginNotePortsImpl for DropletMainThread<'a> {
    fn count(&mut self, is_input: bool) -> u32 {
        if is_input {
            0 // No input note ports
        } else {
            1 // One output note port for MIDI CC
        }
    }

    fn get(&mut self, index: u32, is_input: bool, writer: &mut NotePortInfoWriter) {
        if !is_input && index == 0 {
            writer.set(&NotePortInfo {
                id: ClapId::new(1), // Different from audio port ID
                name: b"MIDI CC Out",
                supported_dialects: NoteDialects::MIDI,
                preferred_dialect: Some(NoteDialect::Midi),
            });
        }
    }
}
