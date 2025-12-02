//! Audio processor for Simply Droplets
//!
//! This is a pass-through audio processor that:
//! - Listens for incoming MIDI CC for learning mode
//! - Handles parameter automation from the DAW
//! - Outputs MIDI 1.0 and MIDI 2.0 (UMP) events

use clack_extensions::params::PluginAudioProcessorParams;
use clack_plugin::events::event_types::{MidiEvent, Midi2Event};
use clack_plugin::host::HostAudioProcessorHandle;
use clack_plugin::plugin::{PluginAudioProcessor, PluginError};
use clack_plugin::prelude::{InputEvents, OutputEvents};
use clack_plugin::process::{Audio, Events, PluginAudioConfiguration, Process, ProcessStatus};
use rtrb::Consumer;

use crate::mcp::{MidiMessage, CcMessage, NoteMessage};
use crate::{DropletMainThread, DropletShared};

pub mod ports;

/// MIDI 2.0 UMP Message Type for Channel Voice Messages
const UMP_MSG_TYPE_MIDI2_CHANNEL_VOICE: u8 = 0x4;

/// MIDI 2.0 UMP Opcodes (same as MIDI 1.0 status nibbles)
const UMP_OPCODE_NOTE_OFF: u8 = 0x8;
const UMP_OPCODE_NOTE_ON: u8 = 0x9;
const UMP_OPCODE_CONTROL_CHANGE: u8 = 0xB;

/// Build a MIDI 2.0 UMP packet for Note On/Off
/// Format: Word1 = [4g][Xc][nn][tt], Word2 = [vvvv][aaaa]
/// Where: 4=type, g=group, X=opcode, c=channel, nn=note, tt=attr_type, vvvv=velocity16, aaaa=attr
fn build_ump_note(group: u8, channel: u8, note: u8, velocity_16bit: u16, is_note_on: bool) -> [u32; 4] {
    let opcode = if is_note_on { UMP_OPCODE_NOTE_ON } else { UMP_OPCODE_NOTE_OFF };
    let attr_type: u8 = 0; // No attribute
    let attr_data: u16 = 0;

    // Word 1: [msg_type:4][group:4][opcode:4][channel:4][note:8][attr_type:8]
    let word1 = ((UMP_MSG_TYPE_MIDI2_CHANNEL_VOICE as u32) << 28)
        | ((group as u32 & 0x0F) << 24)
        | ((opcode as u32 & 0x0F) << 20)
        | ((channel as u32 & 0x0F) << 16)
        | ((note as u32) << 8)
        | (attr_type as u32);

    // Word 2: [velocity:16][attr_data:16]
    let word2 = ((velocity_16bit as u32) << 16) | (attr_data as u32);

    [word1, word2, 0, 0]
}

/// Build a MIDI 2.0 UMP packet for Control Change
/// Format: Word1 = [4g][Bc][cc][00], Word2 = [32-bit value]
fn build_ump_cc(group: u8, channel: u8, cc: u8, value_32bit: u32) -> [u32; 4] {
    // Word 1: [msg_type:4][group:4][opcode:4][channel:4][cc:8][reserved:8]
    let word1 = ((UMP_MSG_TYPE_MIDI2_CHANNEL_VOICE as u32) << 28)
        | ((group as u32 & 0x0F) << 24)
        | ((UMP_OPCODE_CONTROL_CHANGE as u32) << 20)
        | ((channel as u32 & 0x0F) << 16)
        | ((cc as u32) << 8);

    // Word 2: 32-bit CC value
    let word2 = value_32bit;

    [word1, word2, 0, 0]
}

pub struct DropletAudioProcessor<'a> {
    shared: &'a DropletShared<'a>,
    /// MIDI consumer - receives MIDI messages (CC and notes) from the MCP server
    midi_consumer: Consumer<MidiMessage>,
}

impl<'a> PluginAudioProcessor<'a, DropletShared<'a>, DropletMainThread<'a>>
    for DropletAudioProcessor<'a>
{
    fn activate(
        _host: HostAudioProcessorHandle<'a>,
        _main_thread: &mut DropletMainThread<'a>,
        shared: &'a DropletShared,
        audio_config: PluginAudioConfiguration,
    ) -> Result<Self, PluginError> {
        let sample_rate = audio_config.sample_rate as f32;
        crate::logger::log_audio_processor_activation(sample_rate);

        // Take ownership of the MIDI consumer from shared state
        let midi_consumer = shared
            .midi_consumer
            .lock()
            .unwrap()
            .take()
            .ok_or(PluginError::Message("MIDI consumer already taken"))?;

        Ok(Self { shared, midi_consumer })
    }

    fn process(
        &mut self,
        _process: Process,
        _audio: Audio,
        mut events: Events,
    ) -> Result<ProcessStatus, PluginError> {
        // Request main thread callback for GUI updates
        self.shared.host.request_callback();

        // Process incoming MIDI events for CC learning
        for event in events.input.iter() {
            if let Some(midi) = event.as_event::<MidiEvent>() {
                let data = midi.data();
                let status = data[0];
                // CC message: 0xB0-0xBF (176-191)
                if (0xB0..=0xBF).contains(&status) {
                    let channel = status & 0x0F;
                    let cc = data[1];
                    // Process learning - if a slot is in learning mode, assign this CC
                    if let Some(slot_idx) = self.shared.params.process_learn(channel, cc) {
                        log::info!(
                            "Learned CC{} on channel {} for slot {}",
                            cc,
                            channel + 1,
                            slot_idx
                        );
                    }
                }
            }
        }

        // Output MIDI from MCP server (lock-free read from ring buffer)
        // We output both MIDI 1.0 and MIDI 2.0 events - hosts will use whichever they prefer
        while let Ok(msg) = self.midi_consumer.pop() {
            match msg {
                MidiMessage::Cc(cc) => {
                    self.output_cc(&cc, &mut events);
                }
                MidiMessage::Note(note) => {
                    self.output_note(&note, &mut events);
                }
            }
        }

        // Pass through audio unchanged (this plugin is a parameter bridge)
        Ok(ProcessStatus::ContinueIfNotQuiet)
    }
}

impl<'a> DropletAudioProcessor<'a> {
    /// Output a CC message as both MIDI 1.0 and MIDI 2.0
    fn output_cc(&self, cc: &CcMessage, events: &mut Events) {
        // MIDI 1.0: 3-byte CC message
        let status = 0xB0 | (cc.channel & 0x0F);
        let midi1_data = [status, cc.cc, cc.value];
        let midi1_event = MidiEvent::new(0, 0, midi1_data);
        if let Err(e) = events.output.try_push(&midi1_event) {
            log::warn!("Failed to push MIDI 1.0 CC event: {:?}", e);
        }

        // MIDI 2.0: UMP with 32-bit CC value
        // Scale 7-bit or 14-bit to 32-bit for maximum resolution
        let value_32bit = if let Some(val_14) = cc.value_14bit {
            // Scale 14-bit (0-16383) to 32-bit
            ((val_14 as u32) << 18) | ((val_14 as u32) << 4)
        } else {
            // Scale 7-bit (0-127) to 32-bit
            let v = cc.value as u32;
            (v << 25) | (v << 18) | (v << 11) | (v << 4)
        };
        let ump_data = build_ump_cc(0, cc.channel, cc.cc, value_32bit);
        let midi2_event = Midi2Event::new(0, 0, ump_data);
        if let Err(e) = events.output.try_push(&midi2_event) {
            log::warn!("Failed to push MIDI 2.0 CC event: {:?}", e);
        }
    }

    /// Output a Note message as both MIDI 1.0 and MIDI 2.0
    fn output_note(&self, note: &NoteMessage, events: &mut Events) {
        // MIDI 1.0: 3-byte Note On/Off
        let status = if note.is_note_on {
            0x90 | (note.channel & 0x0F)
        } else {
            0x80 | (note.channel & 0x0F)
        };
        let midi1_data = [status, note.note, note.velocity];
        let midi1_event = MidiEvent::new(0, 0, midi1_data);
        if let Err(e) = events.output.try_push(&midi1_event) {
            log::warn!("Failed to push MIDI 1.0 note event: {:?}", e);
        }

        // MIDI 2.0: UMP with 16-bit velocity
        let ump_data = build_ump_note(0, note.channel, note.note, note.velocity_16bit, note.is_note_on);
        let midi2_event = Midi2Event::new(0, 0, ump_data);
        if let Err(e) = events.output.try_push(&midi2_event) {
            log::warn!("Failed to push MIDI 2.0 note event: {:?}", e);
        }
    }
}

/// Handle parameter changes from the DAW during audio processing
impl<'a> PluginAudioProcessorParams for DropletAudioProcessor<'a> {
    fn flush(&mut self, input_parameter_changes: &InputEvents, _output_parameter_changes: &mut OutputEvents) {
        for event in input_parameter_changes {
            if let Some(clack_plugin::events::spaces::CoreEventSpace::ParamValue(pv)) =
                event.as_core_event()
            {
                if let Some(param_id) = pv.param_id() {
                    if let Some(index) = crate::param_id_to_slot(param_id) {
                        self.shared.params.slots[index].value.store(pv.value());
                    }
                }
            }
        }
    }
}
