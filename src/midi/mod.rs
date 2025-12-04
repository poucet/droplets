//! MIDI processor for Simply Droplets
//!
//! This processor:
//! - Listens for incoming MIDI CC for learning mode
//! - Handles parameter automation from the DAW
//! - Outputs CLAP-native note events (for VST3 compatibility)
//! - Outputs MIDI 2.0 UMP for high-resolution in CLAP hosts
//! - Outputs MIDI 1.0 for CC (no native CLAP CC type)

use clack_extensions::params::PluginAudioProcessorParams;
use clack_plugin::events::event_types::{
    Midi2Event, MidiEvent, NoteExpressionEvent, NoteExpressionType, NoteOffEvent, NoteOnEvent,
    TransportFlags,
};
use clack_plugin::events::{Match, Pckn};
use clack_plugin::host::HostAudioProcessorHandle;
use clack_plugin::plugin::{PluginAudioProcessor, PluginError};
use clack_plugin::prelude::{InputEvents, OutputEvents};
use clack_plugin::process::{Audio, Events, PluginAudioConfiguration, Process, ProcessStatus};
use rtrb::Consumer;

use crate::fugue::{FugueInfoHandle, FugueSequencer};
use crate::mcp::{CcMessage, MidiMessage, NoteMessage, PerNoteExpressionMessage, PerNoteExpressionType};
use crate::{DropletMainThread, DropletShared};

pub mod ports;

// =============================================================================
// MIDI 2.0 UMP Constants and Builders
// =============================================================================

/// MIDI 2.0 UMP Message Type for Channel Voice Messages
const UMP_MSG_TYPE_MIDI2_CHANNEL_VOICE: u8 = 0x4;

/// MIDI 2.0 UMP Opcodes
const UMP_OPCODE_NOTE_OFF: u8 = 0x8;
const UMP_OPCODE_NOTE_ON: u8 = 0x9;
const UMP_OPCODE_CONTROL_CHANGE: u8 = 0xB;
const UMP_OPCODE_PER_NOTE_PITCH_BEND: u8 = 0x6;
const UMP_OPCODE_REG_PER_NOTE_CTRL: u8 = 0x0;
const UMP_OPCODE_ASSIGN_PER_NOTE_CTRL: u8 = 0x1;
const UMP_OPCODE_PER_NOTE_MGMT: u8 = 0xF;

/// Registered Per-Note Controller index for Pressure/Aftertouch
const RPN_PER_NOTE_PRESSURE: u8 = 0x07;

/// Build a MIDI 2.0 UMP packet for Note On/Off with 16-bit velocity
fn build_ump_note(group: u8, channel: u8, note: u8, velocity_16bit: u16, is_note_on: bool) -> [u32; 4] {
    let opcode = if is_note_on { UMP_OPCODE_NOTE_ON } else { UMP_OPCODE_NOTE_OFF };
    let word1 = ((UMP_MSG_TYPE_MIDI2_CHANNEL_VOICE as u32) << 28)
        | ((group as u32 & 0x0F) << 24)
        | ((opcode as u32 & 0x0F) << 20)
        | ((channel as u32 & 0x0F) << 16)
        | ((note as u32) << 8);
    let word2 = (velocity_16bit as u32) << 16;
    [word1, word2, 0, 0]
}

/// Build a MIDI 2.0 UMP packet for Control Change with 32-bit value
fn build_ump_cc(group: u8, channel: u8, cc: u8, value_32bit: u32) -> [u32; 4] {
    let word1 = ((UMP_MSG_TYPE_MIDI2_CHANNEL_VOICE as u32) << 28)
        | ((group as u32 & 0x0F) << 24)
        | ((UMP_OPCODE_CONTROL_CHANGE as u32) << 20)
        | ((channel as u32 & 0x0F) << 16)
        | ((cc as u32) << 8);
    [word1, value_32bit, 0, 0]
}

/// Build a MIDI 2.0 UMP packet for Per-Note Pitch Bend (32-bit, 0x80000000 = center)
fn build_ump_per_note_pitch_bend(group: u8, channel: u8, note: u8, value_32bit: u32) -> [u32; 4] {
    let word1 = ((UMP_MSG_TYPE_MIDI2_CHANNEL_VOICE as u32) << 28)
        | ((group as u32 & 0x0F) << 24)
        | ((UMP_OPCODE_PER_NOTE_PITCH_BEND as u32) << 20)
        | ((channel as u32 & 0x0F) << 16)
        | ((note as u32) << 8);
    [word1, value_32bit, 0, 0]
}

/// Build a MIDI 2.0 UMP packet for Registered Per-Note Controller
fn build_ump_reg_per_note_ctrl(group: u8, channel: u8, note: u8, index: u8, value_32bit: u32) -> [u32; 4] {
    let word1 = ((UMP_MSG_TYPE_MIDI2_CHANNEL_VOICE as u32) << 28)
        | ((group as u32 & 0x0F) << 24)
        | ((UMP_OPCODE_REG_PER_NOTE_CTRL as u32) << 20)
        | ((channel as u32 & 0x0F) << 16)
        | ((note as u32) << 8)
        | (index as u32);
    [word1, value_32bit, 0, 0]
}

/// Build a MIDI 2.0 UMP packet for Assignable Per-Note Controller
fn build_ump_assign_per_note_ctrl(group: u8, channel: u8, note: u8, index: u8, value_32bit: u32) -> [u32; 4] {
    let word1 = ((UMP_MSG_TYPE_MIDI2_CHANNEL_VOICE as u32) << 28)
        | ((group as u32 & 0x0F) << 24)
        | ((UMP_OPCODE_ASSIGN_PER_NOTE_CTRL as u32) << 20)
        | ((channel as u32 & 0x0F) << 16)
        | ((note as u32) << 8)
        | (index as u32);
    [word1, value_32bit, 0, 0]
}

/// Build a MIDI 2.0 UMP packet for Per-Note Management (flags: bit 0 = Detach, bit 1 = Reset)
fn build_ump_per_note_mgmt(group: u8, channel: u8, note: u8, flags: u8) -> [u32; 4] {
    let word1 = ((UMP_MSG_TYPE_MIDI2_CHANNEL_VOICE as u32) << 28)
        | ((group as u32 & 0x0F) << 24)
        | ((UMP_OPCODE_PER_NOTE_MGMT as u32) << 20)
        | ((channel as u32 & 0x0F) << 16)
        | ((note as u32) << 8)
        | (flags as u32 & 0x03);
    [word1, 0, 0, 0]
}

// =============================================================================
// MIDI Processor
// =============================================================================

pub struct DropletMidiProcessor<'a> {
    shared: &'a DropletShared<'a>,
    midi_consumer: Consumer<MidiMessage>,
    fugue_sequencer: FugueSequencer,
    fugue_info_handle: FugueInfoHandle,
    sample_rate: f64,
}

impl<'a> PluginAudioProcessor<'a, DropletShared<'a>, DropletMainThread<'a>>
    for DropletMidiProcessor<'a>
{
    fn activate(
        _host: HostAudioProcessorHandle<'a>,
        _main_thread: &mut DropletMainThread<'a>,
        shared: &'a DropletShared,
        audio_config: PluginAudioConfiguration,
    ) -> Result<Self, PluginError> {
        let sample_rate = audio_config.sample_rate;
        crate::logger::log_midi_processor_activation(sample_rate as f32);

        let midi_consumer = shared
            .midi_consumer
            .lock()
            .unwrap()
            .take()
            .ok_or(PluginError::Message("MIDI consumer already taken"))?;

        let fugue_consumer = shared
            .fugue_consumer
            .lock()
            .unwrap()
            .take()
            .ok_or(PluginError::Message("Fugue consumer already taken"))?;

        let fugue_info_handle = shared
            .fugue_info_handle
            .lock()
            .unwrap()
            .take()
            .ok_or(PluginError::Message("Fugue info handle already taken"))?;

        let fugue_sequencer = FugueSequencer::new(fugue_consumer, sample_rate);

        Ok(Self {
            shared,
            midi_consumer,
            fugue_sequencer,
            fugue_info_handle,
            sample_rate,
        })
    }

    fn process(
        &mut self,
        process: Process,
        mut audio: Audio,
        mut events: Events,
    ) -> Result<ProcessStatus, PluginError> {
        self.shared.host.request_callback();

        // Process incoming MIDI events for CC learning
        for event in events.input.iter() {
            if let Some(midi) = event.as_event::<MidiEvent>() {
                let data = midi.data();
                let status = data[0];
                if (0xB0..=0xBF).contains(&status) {
                    let channel = status & 0x0F;
                    let cc = data[1];
                    if let Some(slot_idx) = self.shared.params.process_learn(channel, cc) {
                        log::info!("Learned CC{} on channel {} for slot {}", cc, channel + 1, slot_idx);
                    }
                }
            }
        }

        // Output MIDI from MCP server (immediate messages)
        let mut has_midi = false;
        while let Ok(msg) = self.midi_consumer.pop() {
            has_midi = true;
            self.output_midi_message(&msg, 0, &mut events);
        }

        // Get transport info and process fugue sequencer
        let transport = process.transport;
        let frames = audio.frames_count();

        // Extract transport state
        let is_playing = transport
            .map(|t| t.flags.contains(TransportFlags::IS_PLAYING))
            .unwrap_or(false);

        let current_beat = transport
            .map(|t| t.song_pos_beats.to_float())
            .unwrap_or(0.0);

        let tempo = transport
            .map(|t| t.tempo)
            .unwrap_or(120.0);

        let time_sig_num = transport
            .map(|t| t.time_signature_numerator as u32)
            .unwrap_or(4);

        // Process fugue sequencer - collect events first to avoid borrow conflict
        let fugue_events: Vec<_> = self.fugue_sequencer.process(
            is_playing,
            current_beat,
            tempo,
            frames,
            time_sig_num,
        ).collect();

        for (sample_offset, msg) in fugue_events {
            has_midi = true;
            self.output_midi_message(&msg, sample_offset, &mut events);
        }

        // Update fugue info cache for MCP listing (lock-free)
        if self.fugue_sequencer.active_count() > 0 || is_playing {
            let infos = self.fugue_sequencer.list_fugues(current_beat);
            self.fugue_info_handle.update(infos);
        }

        // For VSTi: Output silence to audio buffers (required for instrument classification)
        // This makes Ableton treat us as a proper instrument with MIDI routing
        for mut port in audio.output_ports() {
            if let Ok(channels) = port.channels() {
                if let Some(mut channels_f32) = channels.into_f32() {
                    for channel in channels_f32.iter_mut() {
                        channel.fill(0.0);
                    }
                }
            }
        }

        // Always continue - we're an instrument that may output MIDI at any time
        if has_midi {
            Ok(ProcessStatus::Continue)
        } else {
            Ok(ProcessStatus::ContinueIfNotQuiet)
        }
    }
}

impl<'a> DropletMidiProcessor<'a> {
    /// Output a MIDI message with the given sample offset
    fn output_midi_message(&self, msg: &MidiMessage, sample_offset: u32, events: &mut Events) {
        match msg {
            MidiMessage::Cc(cc) => self.output_cc_at(cc, sample_offset, events),
            MidiMessage::Note(note) => self.output_note_at(note, sample_offset, events),
            MidiMessage::PerNoteExpression(expr) => self.output_per_note_expression_at(expr, sample_offset, events),
        }
    }

    /// Output a CC message as both MIDI 1.0 and MIDI 2.0 at sample offset
    fn output_cc_at(&self, cc: &CcMessage, sample_offset: u32, events: &mut Events) {
        // MIDI 1.0: 3-byte CC message
        let status = 0xB0 | (cc.channel & 0x0F);
        let midi1_data = [status, cc.cc, cc.value];
        let _ = events.output.try_push(&MidiEvent::new(sample_offset, 0, midi1_data));

        // MIDI 2.0: UMP with 32-bit CC value for high-resolution CLAP hosts
        let value_32bit = if let Some(val_14) = cc.value_14bit {
            ((val_14 as u32) << 18) | ((val_14 as u32) << 4)
        } else {
            let v = cc.value as u32;
            (v << 25) | (v << 18) | (v << 11) | (v << 4)
        };
        let _ = events.output.try_push(&Midi2Event::new(sample_offset, 0, build_ump_cc(0, cc.channel, cc.cc, value_32bit)));
    }

    /// Output a Note message as CLAP-native + MIDI 2.0 at sample offset
    fn output_note_at(&self, note: &NoteMessage, sample_offset: u32, events: &mut Events) {
        let velocity_f64 = note.velocity as f64 / 127.0;
        let pckn = Pckn::new(0u16, note.channel as u16, note.note as u16, Match::<u32>::All);

        // CLAP-native: Translated to VST3 by clap_wrapper
        if note.is_note_on {
            let _ = events.output.try_push(&NoteOnEvent::new(sample_offset, pckn, velocity_f64));
        } else {
            let _ = events.output.try_push(&NoteOffEvent::new(sample_offset, pckn, velocity_f64));
        }

        // MIDI 2.0: High-resolution 16-bit velocity for CLAP hosts
        let ump = build_ump_note(0, note.channel, note.note, note.velocity_16bit, note.is_note_on);
        let _ = events.output.try_push(&Midi2Event::new(sample_offset, 0, ump));
    }

    /// Output a per-note expression as CLAP-native + MIDI 2.0 at sample offset
    fn output_per_note_expression_at(&self, expr: &PerNoteExpressionMessage, sample_offset: u32, events: &mut Events) {
        let pckn = Pckn::new(0u16, expr.channel as u16, expr.note as u16, Match::<u32>::All);

        // CLAP-native NoteExpressionEvent (for VST3 compatibility)
        match expr.expression_type {
            PerNoteExpressionType::PitchBend { value } => {
                let normalized = (value as i64 - 0x80000000i64) as f64 / 0x80000000u32 as f64;
                let _ = events.output.try_push(&NoteExpressionEvent::new(sample_offset, pckn, NoteExpressionType::Tuning, normalized * 120.0));
            }
            PerNoteExpressionType::Pressure { value } => {
                let normalized = value as f64 / u32::MAX as f64;
                let _ = events.output.try_push(&NoteExpressionEvent::new(sample_offset, pckn, NoteExpressionType::Pressure, normalized));
            }
            _ => {}
        }

        // MIDI 2.0 UMP: Full expression support for CLAP hosts
        let ump = match expr.expression_type {
            PerNoteExpressionType::PitchBend { value } => {
                build_ump_per_note_pitch_bend(0, expr.channel, expr.note, value)
            }
            PerNoteExpressionType::Pressure { value } => {
                build_ump_reg_per_note_ctrl(0, expr.channel, expr.note, RPN_PER_NOTE_PRESSURE, value)
            }
            PerNoteExpressionType::RegisteredController { index, value } => {
                build_ump_reg_per_note_ctrl(0, expr.channel, expr.note, index, value)
            }
            PerNoteExpressionType::AssignableController { index, value } => {
                build_ump_assign_per_note_ctrl(0, expr.channel, expr.note, index, value)
            }
            PerNoteExpressionType::Management { flags } => {
                build_ump_per_note_mgmt(0, expr.channel, expr.note, flags)
            }
        };
        let _ = events.output.try_push(&Midi2Event::new(sample_offset, 0, ump));
    }

    /// Output a CC message as both MIDI 1.0 and MIDI 2.0
    fn output_cc(&self, cc: &CcMessage, events: &mut Events) {
        // MIDI 1.0: 3-byte CC message
        let status = 0xB0 | (cc.channel & 0x0F);
        let midi1_data = [status, cc.cc, cc.value];
        let _ = events.output.try_push(&MidiEvent::new(0, 0, midi1_data));

        // MIDI 2.0: UMP with 32-bit CC value for high-resolution CLAP hosts
        let value_32bit = if let Some(val_14) = cc.value_14bit {
            ((val_14 as u32) << 18) | ((val_14 as u32) << 4)
        } else {
            let v = cc.value as u32;
            (v << 25) | (v << 18) | (v << 11) | (v << 4)
        };
        let _ = events.output.try_push(&Midi2Event::new(0, 0, build_ump_cc(0, cc.channel, cc.cc, value_32bit)));
    }

    /// Output a Note message as CLAP-native + MIDI 2.0
    fn output_note(&self, note: &NoteMessage, events: &mut Events) {
        let velocity_f64 = note.velocity as f64 / 127.0;
        let pckn = Pckn::new(0u16, note.channel as u16, note.note as u16, Match::<u32>::All);

        // CLAP-native: Translated to VST3 by clap_wrapper
        if note.is_note_on {
            let _ = events.output.try_push(&NoteOnEvent::new(0, pckn, velocity_f64));
        } else {
            let _ = events.output.try_push(&NoteOffEvent::new(0, pckn, velocity_f64));
        }

        // MIDI 2.0: High-resolution 16-bit velocity for CLAP hosts
        let ump = build_ump_note(0, note.channel, note.note, note.velocity_16bit, note.is_note_on);
        let _ = events.output.try_push(&Midi2Event::new(0, 0, ump));
    }

    /// Output a per-note expression as CLAP-native + MIDI 2.0
    fn output_per_note_expression(&self, expr: &PerNoteExpressionMessage, events: &mut Events) {
        let pckn = Pckn::new(0u16, expr.channel as u16, expr.note as u16, Match::<u32>::All);

        // CLAP-native NoteExpressionEvent (for VST3 compatibility)
        match expr.expression_type {
            PerNoteExpressionType::PitchBend { value } => {
                let normalized = (value as i64 - 0x80000000i64) as f64 / 0x80000000u32 as f64;
                let _ = events.output.try_push(&NoteExpressionEvent::new(0, pckn, NoteExpressionType::Tuning, normalized * 120.0));
            }
            PerNoteExpressionType::Pressure { value } => {
                let normalized = value as f64 / u32::MAX as f64;
                let _ = events.output.try_push(&NoteExpressionEvent::new(0, pckn, NoteExpressionType::Pressure, normalized));
            }
            _ => {}
        }

        // MIDI 2.0 UMP: Full expression support for CLAP hosts
        let ump = match expr.expression_type {
            PerNoteExpressionType::PitchBend { value } => {
                build_ump_per_note_pitch_bend(0, expr.channel, expr.note, value)
            }
            PerNoteExpressionType::Pressure { value } => {
                build_ump_reg_per_note_ctrl(0, expr.channel, expr.note, RPN_PER_NOTE_PRESSURE, value)
            }
            PerNoteExpressionType::RegisteredController { index, value } => {
                build_ump_reg_per_note_ctrl(0, expr.channel, expr.note, index, value)
            }
            PerNoteExpressionType::AssignableController { index, value } => {
                build_ump_assign_per_note_ctrl(0, expr.channel, expr.note, index, value)
            }
            PerNoteExpressionType::Management { flags } => {
                build_ump_per_note_mgmt(0, expr.channel, expr.note, flags)
            }
        };
        let _ = events.output.try_push(&Midi2Event::new(0, 0, ump));
    }
}

/// Handle parameter changes from the DAW during processing
impl<'a> PluginAudioProcessorParams for DropletMidiProcessor<'a> {
    fn flush(&mut self, input_parameter_changes: &InputEvents, _output_parameter_changes: &mut OutputEvents) {
        for event in input_parameter_changes {
            if let Some(clack_plugin::events::spaces::CoreEventSpace::ParamValue(pv)) = event.as_core_event() {
                if let Some(param_id) = pv.param_id() {
                    if let Some(index) = crate::param_id_to_slot(param_id) {
                        self.shared.params.slots[index].value.store(pv.value());
                    }
                }
            }
        }
    }
}
