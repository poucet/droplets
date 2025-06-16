use clack_extensions::audio_ports::*;
use clack_plugin::prelude::*;
use std::ffi::CStr;

use crate::DropletMainThread;

impl<'a> PluginAudioPortsImpl for DropletMainThread<'a> {
    fn count(&mut self, is_input: bool) -> u32 {
        if is_input { 1 } else { 1 }
    }

    fn get(&mut self, index: u32, is_input: bool, writer: &mut AudioPortInfoWriter) {
        if index > 0 {
            return;
        }

        if is_input {
            writer.set(&AudioPortInfo {
                id: ClapId::new(1),
                name: b"Input",
                flags: AudioPortFlags::empty(),
                channel_count: 2,
                port_type: Some(AudioPortType(CStr::from_bytes_with_nul(b"stereo\0").unwrap())),
                in_place_pair: Some(ClapId::new(1)),
            });
        } else {
            writer.set(&AudioPortInfo {
                id: ClapId::new(1), 
                name: b"Output",
                flags: AudioPortFlags::empty(),
                channel_count: 2,
                port_type: Some(AudioPortType(CStr::from_bytes_with_nul(b"stereo\0").unwrap())),
                in_place_pair: Some(ClapId::new(1)),
            });
        }
    }
}