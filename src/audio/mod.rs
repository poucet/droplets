//! Audio processor for Simply Droplets
//!
//! This is a pass-through audio processor that:
//! - Listens for incoming MIDI CC for learning mode
//! - Handles parameter automation from the DAW

use clack_extensions::params::PluginAudioProcessorParams;
use clack_plugin::events::event_types::MidiEvent;
use clack_plugin::host::HostAudioProcessorHandle;
use clack_plugin::plugin::{PluginAudioProcessor, PluginError};
use clack_plugin::prelude::{InputEvents, OutputEvents};
use clack_plugin::process::{Audio, Events, PluginAudioConfiguration, Process, ProcessStatus};
use rtrb::Consumer;

use crate::mcp::MidiMessage;
use crate::{DropletMainThread, DropletShared};

pub mod ports;

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
        events: Events,
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
        while let Ok(msg) = self.midi_consumer.pop() {
            let midi_data = match msg {
                MidiMessage::Cc(cc) => {
                    // MIDI CC status byte: 0xB0 + channel (0-15)
                    let status = 0xB0 | (cc.channel & 0x0F);
                    [status, cc.cc, cc.value]
                }
                MidiMessage::Note(note) => {
                    // Note On: 0x90 + channel, Note Off: 0x80 + channel
                    let status = if note.is_note_on {
                        0x90 | (note.channel & 0x0F)
                    } else {
                        0x80 | (note.channel & 0x0F)
                    };
                    [status, note.note, note.velocity]
                }
            };

            // Output on note port 0, at sample time 0
            let midi_event = MidiEvent::new(0, 0, midi_data);
            if let Err(e) = events.output.try_push(&midi_event) {
                log::warn!("Failed to push MIDI event: {:?}", e);
            }
        }

        // Pass through audio unchanged (this plugin is a parameter bridge)
        Ok(ProcessStatus::ContinueIfNotQuiet)
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
