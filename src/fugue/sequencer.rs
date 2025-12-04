//! Fugue Sequencer - audio thread playback engine
//!
//! Processes fugue commands and outputs MIDI events with sample-accurate timing.

use rtrb::Consumer;

use super::command::FugueCommand;
use super::state::FugueState;
use super::types::{CancelMode, FugueDefinition, FugueEvent, FugueInfo, QuantizeMode};
use crate::mcp::{CcMessage, MidiMessage, NoteMessage, PerNoteExpressionMessage};

/// Buffer capacity for output MIDI messages per process cycle
const OUTPUT_BUFFER_CAPACITY: usize = 256;

/// Audio-thread fugue sequencer
pub struct FugueSequencer {
    /// Ring buffer consumer for commands from MCP thread
    command_consumer: Consumer<FugueCommand>,
    /// Active and pending fugues
    fugues: Vec<FugueState>,
    /// Sample rate for timing calculations
    sample_rate: f64,
    /// Last known transport beat position (for jump detection)
    last_beat: f64,
    /// Output buffer for MIDI messages to emit this cycle
    output_buffer: Vec<(u32, MidiMessage)>,
}

impl FugueSequencer {
    /// Create a new sequencer with the given command consumer
    pub fn new(command_consumer: Consumer<FugueCommand>, sample_rate: f64) -> Self {
        Self {
            command_consumer,
            fugues: Vec::with_capacity(32),
            sample_rate,
            last_beat: 0.0,
            output_buffer: Vec::with_capacity(OUTPUT_BUFFER_CAPACITY),
        }
    }

    /// Process one audio buffer cycle
    ///
    /// Returns an iterator of (sample_offset, MidiMessage) to output.
    ///
    /// # Arguments
    /// * `is_playing` - Whether the transport is playing
    /// * `current_beat` - Current transport position in beats
    /// * `tempo_bpm` - Current tempo in BPM
    /// * `frames` - Number of frames in this buffer
    /// * `time_sig_numerator` - Time signature numerator (e.g., 4 for 4/4)
    pub fn process(
        &mut self,
        is_playing: bool,
        current_beat: f64,
        tempo_bpm: f64,
        frames: u32,
        time_sig_numerator: u32,
    ) -> impl Iterator<Item = (u32, MidiMessage)> + '_ {
        self.output_buffer.clear();

        // 1. Process incoming commands
        self.process_commands();

        // 2. If not playing, don't process fugues
        if !is_playing {
            self.last_beat = current_beat;
            return self.output_buffer.drain(..);
        }

        // 3. Detect transport jumps (seeking)
        let beat_jump = (current_beat - self.last_beat).abs();
        let expected_advance = tempo_bpm / 60.0 / self.sample_rate * frames as f64;
        if beat_jump > expected_advance * 2.0 + 0.01 {
            // Transport jumped - send note-offs for all active notes
            self.handle_transport_jump();
        }

        // 4. Calculate beat range for this buffer
        let beats_per_sample = tempo_bpm / 60.0 / self.sample_rate;
        let end_beat = current_beat + beats_per_sample * frames as f64;

        // 5. Start pending fugues whose quantization point has arrived
        self.start_pending_fugues(current_beat, end_beat, time_sig_numerator);

        // 6. Process active fugues
        self.process_active_fugues(current_beat, end_beat, beats_per_sample);

        // 7. Remove finished fugues
        self.fugues.retain(|f| !f.is_finished() || !f.get_active_notes().is_empty());

        self.last_beat = end_beat;
        self.output_buffer.drain(..)
    }

    /// Process incoming commands from the ring buffer
    fn process_commands(&mut self) {
        while let Ok(cmd) = self.command_consumer.pop() {
            match cmd {
                FugueCommand::Queue(def) => {
                    // Apply cancel mode before queueing
                    self.apply_cancel_mode(&def.cancel_mode);
                    let state = FugueState::new(def);
                    self.fugues.push(state);
                }
                FugueCommand::Cancel { id } => {
                    self.cancel_fugue_by_id(id);
                }
                FugueCommand::CancelByTag { tag } => {
                    self.cancel_fugues_by_tag(&tag);
                }
                FugueCommand::ClearAll => {
                    self.clear_all_fugues();
                }
            }
        }
    }

    /// Apply a cancel mode (cancel existing fugues as needed)
    fn apply_cancel_mode(&mut self, cancel_mode: &CancelMode) {
        match cancel_mode {
            CancelMode::None => {}
            CancelMode::CancelByTag(tag) => {
                self.cancel_fugues_by_tag(tag);
            }
            CancelMode::CancelAll => {
                self.clear_all_fugues();
            }
        }
    }

    /// Cancel a fugue by ID, sending note-offs for active notes
    fn cancel_fugue_by_id(&mut self, id: u64) {
        for fugue in &mut self.fugues {
            if fugue.definition.id == id && !fugue.is_finished() {
                send_note_offs_for_fugue(fugue, &mut self.output_buffer);
                // Mark as finished by setting loop count high
                fugue.current_loop = u32::MAX;
            }
        }
    }

    /// Cancel all fugues with a given tag
    fn cancel_fugues_by_tag(&mut self, tag: &str) {
        for fugue in &mut self.fugues {
            if fugue.definition.tag.as_deref() == Some(tag) && !fugue.is_finished() {
                send_note_offs_for_fugue(fugue, &mut self.output_buffer);
                fugue.current_loop = u32::MAX;
            }
        }
    }

    /// Clear all fugues, sending note-offs
    fn clear_all_fugues(&mut self) {
        for fugue in &mut self.fugues {
            send_note_offs_for_fugue(fugue, &mut self.output_buffer);
        }
        self.fugues.clear();
    }

    /// Handle transport jump by sending note-offs for all active notes
    fn handle_transport_jump(&mut self) {
        for fugue in &mut self.fugues {
            send_note_offs_for_fugue(fugue, &mut self.output_buffer);
            // Reset fugue to waiting state
            fugue.waiting_for_start = true;
            fugue.next_event_index = 0;
            fugue.target_start_beat = None;
        }
    }

    /// Start pending fugues whose quantization point has arrived
    fn start_pending_fugues(
        &mut self,
        current_beat: f64,
        end_beat: f64,
        time_sig_numerator: u32,
    ) {
        for fugue in &mut self.fugues {
            if !fugue.waiting_for_start {
                continue;
            }

            // Calculate target start beat if not already set
            if fugue.target_start_beat.is_none() {
                let target = calculate_quantize_target(
                    &fugue.definition.quantize,
                    current_beat,
                    time_sig_numerator,
                );
                fugue.target_start_beat = Some(target);
            }

            // Check if we've reached the target
            if let Some(target) = fugue.target_start_beat {
                // If playhead is past the target (e.g., transport restarted and jumped past it),
                // recalculate the target based on current position
                if target < current_beat {
                    let new_target = calculate_quantize_target(
                        &fugue.definition.quantize,
                        current_beat,
                        time_sig_numerator,
                    );
                    fugue.target_start_beat = Some(new_target);
                    // Check the new target immediately
                    if new_target >= current_beat && new_target < end_beat {
                        fugue.waiting_for_start = false;
                        fugue.start_beat = new_target;
                    }
                } else if target >= current_beat && target < end_beat {
                    fugue.waiting_for_start = false;
                    fugue.start_beat = target;
                }
            }
        }
    }

    /// Process active fugues and emit events
    fn process_active_fugues(
        &mut self,
        current_beat: f64,
        end_beat: f64,
        beats_per_sample: f64,
    ) {
        for fugue in &mut self.fugues {
            if fugue.waiting_for_start || fugue.is_finished() {
                continue;
            }

            // Calculate beat range within this fugue
            let local_start = current_beat - fugue.start_beat;
            let local_end = end_beat - fugue.start_beat;

            // Process events in this range
            while fugue.next_event_index < fugue.definition.events.len() {
                let event = &fugue.definition.events[fugue.next_event_index];

                if event.beat_offset >= local_end {
                    break; // Event is in the future
                }

                if event.beat_offset >= local_start {
                    // Event is in this buffer - calculate sample offset
                    let beat_delta = event.beat_offset - local_start;
                    let sample_offset = (beat_delta / beats_per_sample).round() as u32;

                    // Emit the event
                    let msg = fugue_event_to_midi(&event.event);
                    self.output_buffer.push((sample_offset, msg));

                    // Track note state
                    match event.event {
                        FugueEvent::NoteOn { channel, note, .. } => {
                            fugue.set_note_active(channel, note);
                        }
                        FugueEvent::NoteOff { channel, note } => {
                            fugue.set_note_inactive(channel, note);
                        }
                        _ => {}
                    }
                }

                fugue.next_event_index += 1;
            }

            // Check for loop boundary
            if local_end >= fugue.definition.duration_beats {
                if !fugue.is_finished() {
                    fugue.reset_for_loop();
                }
            }
        }
    }

    /// Get information about all active fugues (for MCP listing)
    pub fn list_fugues(&self, current_beat: f64) -> Vec<FugueInfo> {
        self.fugues
            .iter()
            .filter(|f| !f.is_finished())
            .map(|f| f.info(current_beat))
            .collect()
    }

    /// Get the number of active fugues
    pub fn active_count(&self) -> usize {
        self.fugues.iter().filter(|f| !f.is_finished()).count()
    }

    /// Get all active fugue definitions (for UI visualization)
    pub fn get_definitions(&self) -> Vec<FugueDefinition> {
        self.fugues
            .iter()
            .filter(|f| !f.is_finished())
            .map(|f| f.definition.clone())
            .collect()
    }
}

/// Send note-offs for all active notes in a fugue
fn send_note_offs_for_fugue(fugue: &mut FugueState, output_buffer: &mut Vec<(u32, MidiMessage)>) {
    for (channel, note) in fugue.get_active_notes() {
        let msg = MidiMessage::Note(NoteMessage::new(channel, note, 0, false));
        output_buffer.push((0, msg));
    }
    fugue.clear_active_notes();
}

/// Calculate the quantization target beat
fn calculate_quantize_target(
    quantize: &QuantizeMode,
    current_beat: f64,
    time_sig_numerator: u32,
) -> f64 {
    match quantize {
        QuantizeMode::Immediate => current_beat,
        QuantizeMode::NextBeat => current_beat.ceil(),
        QuantizeMode::NextBar => {
            let beats_per_bar = time_sig_numerator as f64;
            let current_bar = (current_beat / beats_per_bar).floor();
            (current_bar + 1.0) * beats_per_bar
        }
        QuantizeMode::NextBars(n) => {
            let beats_per_bar = time_sig_numerator as f64;
            let bar_group = (*n as f64) * beats_per_bar;
            let current_group = (current_beat / bar_group).floor();
            (current_group + 1.0) * bar_group
        }
    }
}

/// Convert a FugueEvent to a MidiMessage
fn fugue_event_to_midi(event: &FugueEvent) -> MidiMessage {
    match event {
        FugueEvent::NoteOn { channel, note, velocity } => {
            MidiMessage::Note(NoteMessage::new(*channel, *note, *velocity, true))
        }
        FugueEvent::NoteOff { channel, note } => {
            MidiMessage::Note(NoteMessage::new(*channel, *note, 0, false))
        }
        FugueEvent::Cc { channel, cc, value } => {
            MidiMessage::Cc(CcMessage::new(*channel, *cc, *value))
        }
        FugueEvent::PerNotePitchBend { channel, note, semitones } => {
            MidiMessage::PerNoteExpression(
                PerNoteExpressionMessage::pitch_bend_semitones(*channel, *note, *semitones)
            )
        }
        FugueEvent::PerNotePressure { channel, note, pressure } => {
            MidiMessage::PerNoteExpression(
                PerNoteExpressionMessage::pressure_normalized(*channel, *note, *pressure)
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quantize_immediate() {
        assert_eq!(calculate_quantize_target(&QuantizeMode::Immediate, 2.5, 4), 2.5);
    }

    #[test]
    fn test_quantize_next_beat() {
        assert_eq!(calculate_quantize_target(&QuantizeMode::NextBeat, 2.5, 4), 3.0);
        assert_eq!(calculate_quantize_target(&QuantizeMode::NextBeat, 3.0, 4), 3.0);
    }

    #[test]
    fn test_quantize_next_bar() {
        // In 4/4, bars are at 0, 4, 8, 12...
        assert_eq!(calculate_quantize_target(&QuantizeMode::NextBar, 2.5, 4), 4.0);
        assert_eq!(calculate_quantize_target(&QuantizeMode::NextBar, 4.0, 4), 8.0);
        assert_eq!(calculate_quantize_target(&QuantizeMode::NextBar, 7.9, 4), 8.0);
    }

    #[test]
    fn test_quantize_next_bars() {
        // NextBars(2) in 4/4 means 8-beat groups
        assert_eq!(calculate_quantize_target(&QuantizeMode::NextBars(2), 2.5, 4), 8.0);
        assert_eq!(calculate_quantize_target(&QuantizeMode::NextBars(2), 9.0, 4), 16.0);
    }
}
