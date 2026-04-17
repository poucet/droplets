//! Fugue Sequencer - audio thread playback engine
//!
//! Processes fugue commands and outputs MIDI events with sample-accurate timing.

use rtrb::Consumer;

use super::command::FugueCommand;
use super::fugue::Fugue;
use super::types::{CancelMode, FugueDefinition, FugueInfo, ProcessedEvent};
use crate::mcp::{MidiMessage, NoteMessage};

/// Buffer capacity for output MIDI messages per process cycle
const OUTPUT_BUFFER_CAPACITY: usize = 256;

/// Audio-thread fugue sequencer
pub struct FugueSequencer {
    /// Ring buffer consumer for commands from MCP thread
    command_consumer: Consumer<FugueCommand>,
    /// Active and pending fugues
    fugues: Vec<Fugue>,
    /// Sample rate for timing calculations
    sample_rate: f64,
    /// Last known transport beat position (for jump detection)
    last_beat: f64,
    /// Output buffer for processed events to emit this cycle
    output_buffer: Vec<ProcessedEvent>,
    /// Whether transport was playing in the previous cycle
    was_playing: bool,
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
            was_playing: false,
        }
    }

    /// Process one audio buffer cycle
    ///
    /// Returns an iterator of ProcessedEvents to output.
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
    ) -> impl Iterator<Item = ProcessedEvent> + '_ {
        self.output_buffer.clear();

        // 1. Process incoming commands
        self.process_commands();

        // 2. If not playing, send note-offs if we just stopped, then return
        if !is_playing {
            if self.was_playing {
                // Transport just stopped - send note-offs for all active notes
                self.send_all_note_offs();
            }
            self.was_playing = false;
            // Don't update last_beat here - we'll sync it when transport starts
            return self.output_buffer.drain(..);
        }

        // 3. Handle transport state transitions and jumps
        let just_started = !self.was_playing;
        self.was_playing = true;

        if just_started {
            // Transport just started - sync last_beat to current position
            // This is NOT a jump, just a normal start from wherever the playhead is
            self.last_beat = current_beat;
        } else {
            // Transport was already playing - detect jumps (seeking)
            let beat_jump = (current_beat - self.last_beat).abs();
            let expected_advance = tempo_bpm / 60.0 / self.sample_rate * frames as f64;
            if beat_jump > expected_advance * 2.0 + 0.01 {
                // Transport jumped while playing - send note-offs for all active notes
                self.handle_transport_jump();
            }
        }

        // 4. Calculate beat range for this buffer
        let beats_per_sample = tempo_bpm / 60.0 / self.sample_rate;
        let end_beat = current_beat + beats_per_sample * frames as f64;

        // 5. Start pending fugues whose quantization point has arrived
        self.start_pending_fugues(current_beat, end_beat, time_sig_numerator, beats_per_sample);

        // 6. Process active fugues
        self.process_active_fugues(current_beat, end_beat, beats_per_sample);

        // 7. Drop finished fugues. If a finished fugue still has active notes
        //    (held note whose auto-off fell past duration_beats — common at
        //    loop boundaries), emit note-offs for them before dropping so the
        //    synth doesn't get stuck. Previously the retain kept these around
        //    forever because the predicate never became false, so cancelled or
        //    finite-loop fugues lingered in the UI list.
        let output_buffer = &mut self.output_buffer;
        self.fugues.retain_mut(|f| {
            if !f.is_finished() {
                return true;
            }
            if !f.get_active_notes().is_empty() {
                send_note_offs_for_fugue(f, output_buffer);
            }
            false
        });

        self.last_beat = end_beat;
        self.output_buffer.drain(..)
    }

    /// Process incoming commands from the ring buffer
    fn process_commands(&mut self) {
        while let Ok(cmd) = self.command_consumer.pop() {
            match cmd {
                FugueCommand::Queue(def) => {
                    // Don't apply cancel mode here - defer until the fugue actually starts
                    // This ensures seamless transitions at quantization boundaries
                    let fugue = Fugue::new(def);
                    self.fugues.push(fugue);
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

    /// Cancel a fugue by ID, sending note-offs for active notes
    fn cancel_fugue_by_id(&mut self, id: u64) {
        for fugue in &mut self.fugues {
            if fugue.definition.id == id && !fugue.is_finished() {
                send_note_offs_for_fugue(fugue, &mut self.output_buffer);
                fugue.cancel();
            }
        }
    }

    /// Cancel all fugues with a given tag
    fn cancel_fugues_by_tag(&mut self, tag: &str) {
        for fugue in &mut self.fugues {
            if fugue.definition.tag.as_deref() == Some(tag) && !fugue.is_finished() {
                send_note_offs_for_fugue(fugue, &mut self.output_buffer);
                fugue.cancel();
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

    /// Send note-offs for all active notes in all fugues (e.g., when transport stops)
    fn send_all_note_offs(&mut self) {
        for fugue in &mut self.fugues {
            send_note_offs_for_fugue(fugue, &mut self.output_buffer);
        }
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

    /// Start pending fugues whose quantization point has arrived.
    ///
    /// Uses the interval-based quantization model: fugues start when the transport
    /// reaches a grid line (where current_beat % interval == 0). Beat 0 is always
    /// a valid grid line for all intervals.
    ///
    /// Cancel modes are applied here (not when queued) to ensure seamless transitions.
    ///
    /// **Tag-swap alignment:** when a new fugue has `cancel_mode: CancelByTag(X)`
    /// and a fugue with tag X is currently playing, the new fugue starts at that
    /// old fugue's next loop boundary — not its own independent quantize grid. This
    /// gives musically-expected "replace on the loop" behavior by default; without
    /// it, a 3-beat loop replaced by a bar-quantized fugue creates a gap.
    fn start_pending_fugues(
        &mut self,
        current_beat: f64,
        end_beat: f64,
        time_sig_numerator: u32,
        beats_per_sample: f64,
    ) {
        // Pre-compute each live tag's next loop boundary. Used below when a
        // waiting fugue has cancel_mode:tag:X to align to the fugue being
        // replaced instead of the new fugue's own quantize grid. When multiple
        // playing fugues share a tag (unusual but possible), use the earliest.
        let tag_loop_boundaries = self.tag_loop_boundaries(current_beat);

        let mut starting_fugues: Vec<(usize, CancelMode)> = Vec::new();

        for (idx, fugue) in self.fugues.iter_mut().enumerate() {
            if !fugue.waiting_for_start {
                continue;
            }

            // Default: use the fugue's own quantize grid.
            let quantize_target = fugue
                .definition
                .quantize
                .next_grid_line(current_beat, time_sig_numerator);

            // Tag-swap override: align to the cancelled tag's next loop
            // boundary if one is playing. Falls through to quantize otherwise.
            let tag_target = if let CancelMode::CancelByTag(tag) = &fugue.definition.cancel_mode {
                tag_loop_boundaries.get(tag).copied()
            } else {
                None
            };

            let target = tag_target.unwrap_or(quantize_target);

            fugue.target_start_beat = Some(target);

            // Check if the target falls within this buffer's beat range
            if target >= current_beat && target < end_beat {
                starting_fugues.push((idx, fugue.definition.cancel_mode.clone()));
            }
        }

        // Apply cancel modes and start fugues
        for (idx, cancel_mode) in starting_fugues {
            // Calculate sample offset for when this fugue starts
            let fugue = &self.fugues[idx];
            let target = fugue.target_start_beat.unwrap_or(current_beat);
            let sample_offset = ((target - current_beat) / beats_per_sample).round().max(0.0) as u32;

            // Apply cancel mode at the same sample offset as the new fugue starts
            self.apply_cancel_mode_at_offset(&cancel_mode, sample_offset);

            // Now start the fugue
            let fugue = &mut self.fugues[idx];
            fugue.waiting_for_start = false;
            fugue.start_beat = target;
        }
    }

    /// Map each currently-playing tagged fugue's tag to its NEXT loop boundary.
    /// The boundary is the beat at which that fugue's current loop iteration
    /// completes — i.e., the earliest moment a seamless replacement can start.
    /// Returns the earliest boundary when multiple playing fugues share a tag.
    fn tag_loop_boundaries(&self, current_beat: f64) -> std::collections::HashMap<String, f64> {
        use std::collections::HashMap;
        let mut out: HashMap<String, f64> = HashMap::new();
        for fugue in &self.fugues {
            if fugue.waiting_for_start || fugue.is_finished() {
                continue;
            }
            let tag = match &fugue.definition.tag {
                Some(t) => t.clone(),
                None => continue,
            };
            // Where the current loop iteration ends in absolute beats.
            let loop_len = fugue.definition.duration_beats;
            if loop_len <= 0.0 {
                continue;
            }
            let elapsed = (current_beat - fugue.start_beat).max(0.0);
            let completed_loops = (elapsed / loop_len).floor();
            let next_boundary = fugue.start_beat + (completed_loops + 1.0) * loop_len;
            out.entry(tag)
                .and_modify(|prev| { if next_boundary < *prev { *prev = next_boundary; } })
                .or_insert(next_boundary);
        }
        out
    }

    /// Apply a cancel mode with note-offs at a specific sample offset
    fn apply_cancel_mode_at_offset(&mut self, cancel_mode: &CancelMode, sample_offset: u32) {
        match cancel_mode {
            CancelMode::None => {}
            CancelMode::CancelByTag(tag) => {
                self.cancel_fugues_by_tag_at_offset(tag, sample_offset);
            }
            CancelMode::CancelAll => {
                self.clear_all_fugues_at_offset(sample_offset);
            }
        }
    }

    /// Cancel all fugues with a given tag, with note-offs at specific sample offset
    fn cancel_fugues_by_tag_at_offset(&mut self, tag: &str, sample_offset: u32) {
        for fugue in &mut self.fugues {
            if fugue.definition.tag.as_deref() == Some(tag) && !fugue.is_finished() && !fugue.waiting_for_start {
                send_note_offs_for_fugue_at_offset(fugue, &mut self.output_buffer, sample_offset);
                fugue.cancel();
            }
        }
    }

    /// Clear all active fugues, with note-offs at specific sample offset
    fn clear_all_fugues_at_offset(&mut self, sample_offset: u32) {
        for fugue in &mut self.fugues {
            if !fugue.waiting_for_start {
                send_note_offs_for_fugue_at_offset(fugue, &mut self.output_buffer, sample_offset);
            }
        }
        // Only clear non-waiting fugues, keep pending ones
        self.fugues.retain(|f| f.waiting_for_start);
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

            // Calculate local beat range
            let local_end = end_beat - fugue.start_beat;

            // Process events and collect into output buffer
            let events = fugue.process_buffer(current_beat, end_beat, beats_per_sample);
            self.output_buffer.extend(events);

            // Check for loop boundary
            if local_end >= fugue.definition.duration_beats && !fugue.is_finished() {
                // Reset for next loop
                fugue.reset_for_loop();

                // Process events from the start of the new loop if buffer extends into it
                let new_local_end = end_beat - fugue.start_beat;
                if new_local_end > 0.0 {
                    let more_events = fugue.process_buffer(current_beat, end_beat, beats_per_sample);
                    self.output_buffer.extend(more_events);
                }
            }
        }
    }

    /// Get information about all active fugues (for MCP listing)
    pub fn list_fugues(&self, current_beat: f64, time_sig_numerator: u32) -> Vec<FugueInfo> {
        self.fugues
            .iter()
            .filter(|f| !f.is_finished())
            .map(|f| f.info(current_beat, time_sig_numerator))
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

/// Send note-offs for all active notes in a fugue at sample offset 0
fn send_note_offs_for_fugue(fugue: &mut Fugue, output_buffer: &mut Vec<ProcessedEvent>) {
    send_note_offs_for_fugue_at_offset(fugue, output_buffer, 0);
}

/// Send note-offs for all active notes in a fugue at a specific sample offset
fn send_note_offs_for_fugue_at_offset(
    fugue: &mut Fugue,
    output_buffer: &mut Vec<ProcessedEvent>,
    sample_offset: u32,
) {
    for (channel, note) in fugue.get_active_notes() {
        let msg = MidiMessage::Note(NoteMessage::new(channel, note, 0, false));
        output_buffer.push(ProcessedEvent::Instant { sample_offset, message: msg });
    }
    fugue.clear_active_notes();
    fugue.clear_cc_state();
}

#[cfg(test)]
mod tests {
    use super::super::types::QuantizeMode;

    #[test]
    fn test_quantize_immediate() {
        // Immediate: always returns current beat (no grid)
        assert_eq!(QuantizeMode::Immediate.next_grid_line(2.5, 4), 2.5);
        assert!(QuantizeMode::Immediate.is_on_grid(2.5, 4));
    }

    #[test]
    fn test_quantize_beat() {
        // Beat: grid every 1 beat (0, 1, 2, 3...)
        assert_eq!(QuantizeMode::Beat.next_grid_line(2.5, 4), 3.0);
        assert_eq!(QuantizeMode::Beat.next_grid_line(3.0, 4), 3.0); // Already on grid
        assert!(QuantizeMode::Beat.is_on_grid(3.0, 4));
        assert!(!QuantizeMode::Beat.is_on_grid(2.5, 4));
    }

    #[test]
    fn test_quantize_bar() {
        // Bar: grid every 4 beats in 4/4 (0, 4, 8, 12...)
        assert_eq!(QuantizeMode::Bar.next_grid_line(2.5, 4), 4.0);
        assert_eq!(QuantizeMode::Bar.next_grid_line(4.0, 4), 4.0); // Already on grid
        assert_eq!(QuantizeMode::Bar.next_grid_line(7.9, 4), 8.0);
        assert!(QuantizeMode::Bar.is_on_grid(0.0, 4));
        assert!(QuantizeMode::Bar.is_on_grid(4.0, 4));
        assert!(!QuantizeMode::Bar.is_on_grid(2.5, 4));
    }

    #[test]
    fn test_quantize_bars() {
        // Bars(2): grid every 8 beats in 4/4 (0, 8, 16...)
        assert_eq!(QuantizeMode::Bars(2).next_grid_line(2.5, 4), 8.0);
        assert_eq!(QuantizeMode::Bars(2).next_grid_line(8.0, 4), 8.0); // Already on grid
        assert_eq!(QuantizeMode::Bars(2).next_grid_line(9.0, 4), 16.0);
        assert!(QuantizeMode::Bars(2).is_on_grid(0.0, 4));
        assert!(QuantizeMode::Bars(2).is_on_grid(8.0, 4));
        assert!(!QuantizeMode::Bars(2).is_on_grid(4.0, 4));
    }

    #[test]
    fn test_quantize_at_beat_zero() {
        // All quantize modes should consider beat 0 as on-grid
        assert!(QuantizeMode::Immediate.is_on_grid(0.0, 4));
        assert!(QuantizeMode::Beat.is_on_grid(0.0, 4));
        assert!(QuantizeMode::Bar.is_on_grid(0.0, 4));
        assert!(QuantizeMode::Bars(4).is_on_grid(0.0, 4));

        // All should return 0.0 as the grid line when at beat 0
        assert_eq!(QuantizeMode::Immediate.next_grid_line(0.0, 4), 0.0);
        assert_eq!(QuantizeMode::Beat.next_grid_line(0.0, 4), 0.0);
        assert_eq!(QuantizeMode::Bar.next_grid_line(0.0, 4), 0.0);
        assert_eq!(QuantizeMode::Bars(4).next_grid_line(0.0, 4), 0.0);
    }

    #[test]
    fn test_interval_beats() {
        assert_eq!(QuantizeMode::Immediate.interval_beats(4), None);
        assert_eq!(QuantizeMode::Beat.interval_beats(4), Some(1.0));
        assert_eq!(QuantizeMode::Bar.interval_beats(4), Some(4.0));
        assert_eq!(QuantizeMode::Bars(2).interval_beats(4), Some(8.0));
        // In 3/4 time
        assert_eq!(QuantizeMode::Bar.interval_beats(3), Some(3.0));
        assert_eq!(QuantizeMode::Bars(2).interval_beats(3), Some(6.0));
    }

    #[test]
    fn test_start_at_beat_zero_scenario() {
        // Simulate: User queues a 4-bar quantized fugue, then presses play from beat 0
        // The fugue should start immediately since beat 0 is on the 4-bar grid

        // 4-bar interval in 4/4 = 16 beats
        let mode = QuantizeMode::Bars(4);

        // At beat 0, we're on the grid
        assert!(mode.is_on_grid(0.0, 4));
        assert_eq!(mode.next_grid_line(0.0, 4), 0.0);

        // Even with a tiny offset (DAW quirk), we should still snap to beat 0
        // because 0.001 is within tolerance (3% of 16 = 0.48 beats)
        assert!(mode.is_on_grid(0.001, 4));
        assert_eq!(mode.next_grid_line(0.001, 4), 0.0); // Should snap back to 0.0

        // A larger offset (beyond tolerance) should go to next grid line
        assert!(!mode.is_on_grid(1.0, 4)); // 1.0 is not on the 16-beat grid
        assert_eq!(mode.next_grid_line(1.0, 4), 16.0); // Next grid is at 16.0

        // Close to the next grid line should snap forward
        assert!(mode.is_on_grid(15.9, 4)); // 15.9 is close to 16.0
        assert!((mode.next_grid_line(15.9, 4) - 16.0).abs() < 0.01);
    }

    #[test]
    fn test_grid_tolerance_scales_with_interval() {
        // For small intervals (1 beat), tolerance is 3% = 0.03 beats
        let beat_mode = QuantizeMode::Beat;
        assert!(beat_mode.is_on_grid(0.02, 4)); // Within 0.03 of beat 0
        assert!(!beat_mode.is_on_grid(0.05, 4)); // Beyond tolerance

        // For bar intervals (4 beats), tolerance is 3% = 0.12 beats
        let bar_mode = QuantizeMode::Bar;
        assert!(bar_mode.is_on_grid(0.1, 4)); // Within 0.12 of beat 0
        assert!(!bar_mode.is_on_grid(0.2, 4)); // Beyond tolerance
    }
}
