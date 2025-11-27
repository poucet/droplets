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
    fn count(&mut self, _is_input: bool) -> u32 {
        // One input (for CC learning) and one output (for CC commands)
        1
    }

    fn get(&mut self, index: u32, is_input: bool, writer: &mut NotePortInfoWriter) {
        if index == 0 {
            if is_input {
                writer.set(&NotePortInfo {
                    id: ClapId::new(1),
                    name: b"MIDI Learn",
                    supported_dialects: NoteDialects::MIDI,
                    preferred_dialect: Some(NoteDialect::Midi),
                });
            } else {
                writer.set(&NotePortInfo {
                    id: ClapId::new(2),
                    name: b"MIDI CC Out",
                    supported_dialects: NoteDialects::MIDI,
                    preferred_dialect: Some(NoteDialect::Midi),
                });
            }
        }
    }
}
