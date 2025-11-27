//! Audio processor for Simply Droplets
//!
//! This is a pass-through audio processor that:
//! - Listens for incoming MIDI CC for learning mode
//! - Outputs MIDI CC from the MCP server when AI sets slot values

use clack_plugin::events::event_types::MidiEvent;
use clack_plugin::host::HostAudioProcessorHandle;
use clack_plugin::plugin::{PluginAudioProcessor, PluginError};
use clack_plugin::process::{Audio, Events, PluginAudioConfiguration, Process, ProcessStatus};
use rtrb::Consumer;

use crate::mcp::CcMessage;
use crate::{DropletMainThread, DropletShared};

pub mod ports;

pub struct DropletAudioProcessor<'a> {
    shared: &'a DropletShared<'a>,
    /// CC consumer - receives CC messages from the MCP server
    cc_consumer: Consumer<CcMessage>,
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

        // Take ownership of the CC consumer from shared state
        let cc_consumer = shared
            .cc_consumer
            .lock()
            .unwrap()
            .take()
            .ok_or(PluginError::Message("CC consumer already taken"))?;

        Ok(Self { shared, cc_consumer })
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

        // Output MIDI CC from MCP server (lock-free read from ring buffer)
        while let Ok(cc) = self.cc_consumer.pop() {
            // MIDI CC status byte: 0xB0 + channel (0-15)
            let status = 0xB0 | (cc.channel & 0x0F);
            let midi_data = [status, cc.cc, cc.value];

            // Output on note port 0, at sample time 0
            let midi_event = MidiEvent::new(0, 0, midi_data);
            if let Err(e) = events.output.try_push(&midi_event) {
                log::warn!("Failed to push MIDI CC event: {:?}", e);
            }
        }

        // Pass through audio unchanged (this plugin is just a MIDI CC bridge)
        Ok(ProcessStatus::ContinueIfNotQuiet)
    }
}
