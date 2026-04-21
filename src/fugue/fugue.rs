//! Fugue - runtime state and event processing for a musical sequence
//!
//! A Fugue is an active instance of a FugueDefinition, tracking playback position,
//! active notes, CC state, and handling event emission with interpolation.

use super::types::{
    FugueDefinition, FugueEvent, FugueInfo, InterpolationMode, LoopMode, ProcessedEvent,
};
use crate::mcp::{CcMessage, MidiMessage, NoteMessage, PerNoteExpressionMessage};

/// Tracks an active CC ramp that spans multiple process cycles
#[derive(Debug, Clone)]
pub struct ActiveCcRamp {
    pub channel: u8,
    pub cc: u8,
    pub start_value: u8,
    pub end_value: u8,
    /// Beat when the ramp started
    pub start_beat: f64,
    /// Beat when the ramp ends (when next CC event triggers)
    pub end_beat: f64,
    pub interpolation: InterpolationMode,
}

/// Runtime state for an active fugue
pub struct Fugue {
    /// The fugue definition being played
    pub definition: FugueDefinition,
    /// Absolute beat when this fugue started (after quantization)
    pub start_beat: f64,
    /// Current loop iteration (0-indexed)
    pub current_loop: u32,
    /// Index of the next event to process
    pub next_event_index: usize,
    /// Bitset tracking active notes: 16 channels × 128 notes
    /// Each u64 covers 64 notes, so 2 u64s cover all 128 notes per channel
    pub active_notes: [[u64; 2]; 16],
    /// Whether this fugue is waiting for its quantization point
    pub waiting_for_start: bool,
    /// Target beat to start at (calculated from quantization)
    pub target_start_beat: Option<f64>,
    /// Whether this fugue has been cancelled (overrides loop mode)
    pub cancelled: bool,
    /// Last known CC value per channel/cc (for interpolation start values)
    /// None means no CC has been sent yet on this channel/cc
    cc_state: [[Option<u8>; 128]; 16],
    /// Active CC ramps that span multiple buffers
    active_ramps: Vec<ActiveCcRamp>,
}

// Helper to create default CC state array
const fn default_cc_state() -> [[Option<u8>; 128]; 16] {
    [[None; 128]; 16]
}

impl Fugue {
    /// Create a new fugue state from a definition
    pub fn new(definition: FugueDefinition) -> Self {
        Self {
            definition,
            start_beat: 0.0,
            current_loop: 0,
            next_event_index: 0,
            active_notes: [[0; 2]; 16],
            waiting_for_start: true,
            target_start_beat: None,
            cancelled: false,
            cc_state: default_cc_state(),
            active_ramps: Vec::new(),
        }
    }

    /// Mark a note as active (being played)
    pub fn set_note_active(&mut self, channel: u8, note: u8) {
        let ch = (channel & 0x0F) as usize;
        let n = note as usize;
        let idx = n / 64;
        let bit = n % 64;
        self.active_notes[ch][idx] |= 1u64 << bit;
    }

    /// Mark a note as inactive (released)
    pub fn set_note_inactive(&mut self, channel: u8, note: u8) {
        let ch = (channel & 0x0F) as usize;
        let n = note as usize;
        let idx = n / 64;
        let bit = n % 64;
        self.active_notes[ch][idx] &= !(1u64 << bit);
    }

    /// Check if a note is currently active
    pub fn is_note_active(&self, channel: u8, note: u8) -> bool {
        let ch = (channel & 0x0F) as usize;
        let n = note as usize;
        let idx = n / 64;
        let bit = n % 64;
        (self.active_notes[ch][idx] & (1u64 << bit)) != 0
    }

    /// Get all active notes as (channel, note) pairs
    pub fn get_active_notes(&self) -> Vec<(u8, u8)> {
        let mut notes = Vec::new();
        for (ch, channel_bits) in self.active_notes.iter().enumerate() {
            for (idx, &bits) in channel_bits.iter().enumerate() {
                if bits == 0 {
                    continue;
                }
                let base = idx * 64;
                for bit in 0..64 {
                    if (bits & (1u64 << bit)) != 0 {
                        notes.push((ch as u8, (base + bit) as u8));
                    }
                }
            }
        }
        notes
    }

    /// Clear all active notes
    pub fn clear_active_notes(&mut self) {
        self.active_notes = [[0; 2]; 16];
    }

    /// Check if this fugue has completed all loops or was cancelled
    pub fn is_finished(&self) -> bool {
        if self.cancelled {
            return true;
        }
        match self.definition.loop_mode {
            LoopMode::Once => self.current_loop >= 1,
            LoopMode::Times(n) => self.current_loop >= n,
            LoopMode::Forever => false,
        }
    }

    /// Mark this fugue as cancelled
    pub fn cancel(&mut self) {
        self.cancelled = true;
    }

    /// Reset for next loop iteration
    pub fn reset_for_loop(&mut self) {
        self.next_event_index = 0;
        self.current_loop += 1;
        self.start_beat += self.definition.duration_beats;
    }

    /// Re-anchor a looping fugue so its phase matches the given transport beat.
    ///
    /// Called on transport start and on relocate/jump so that forever-looping
    /// fugues resume as if they'd been playing along with the song timeline
    /// (phase-locked to the DAW, not wall-clock). Caller must emit note-offs
    /// for active notes before invoking — this clears all runtime state.
    ///
    /// **Phase is computed against the song's absolute grid at beat 0**, not
    /// against the fugue's original start_beat. A fugue queued mid-song at
    /// an unaligned bar (e.g. start_beat=4 for a duration=8 pattern) must
    /// still resume from its pattern-beat-0 when transport returns to song
    /// beat 0. Anchoring to the song grid makes loop boundaries predictable:
    /// the pattern's beat 0 always lands on transport beats 0, dur, 2*dur…
    ///
    /// Returns `true` if the fugue should remain; `false` if it should be
    /// dropped (mid-flight one-shots that can't meaningfully resume).
    pub fn phase_lock(&mut self, transport_beat: f64) -> bool {
        let dur = self.definition.duration_beats;
        if dur <= 0.0 {
            return true;
        }

        // Waiting fugues just re-quantize normally on the next buffer.
        if self.waiting_for_start {
            self.target_start_beat = None;
            return true;
        }

        match self.definition.loop_mode {
            LoopMode::Forever => {}
            LoopMode::Once | LoopMode::Times(_) => {
                // A mid-flight one-shot can't be meaningfully phase-locked —
                // its events are in the past. Mark finished; caller drops.
                self.cancelled = true;
                return false;
            }
        }

        let phase = transport_beat.rem_euclid(dur);
        self.start_beat = transport_beat - phase;
        self.current_loop = 0;
        // Tolerate ~1 MIDI tick of float drift when finding the first event
        // to play. Without this, a DAW transport-loop that wraps back to a
        // beat the DAW reports as `1e-15` (instead of exactly `0`) makes
        // `partition_point` walk past any event at beat 0 — because
        // `0.0 < 1e-15` is true — and the iteration silently drops its
        // beat-0 NoteOns. The symptom is "first note of the next loop
        // iteration doesn't play" and it reproduces most easily when the
        // DAW's loop length exactly matches the fugue's duration_beats
        // (so this `phase_lock` jump path intercepts instead of the
        // drift-immune `reset_for_loop`). One MIDI tick (1/960 beat) is
        // well below audible timing resolution and safely above any
        // plausible transport-position float error.
        const PHASE_LOCK_TOLERANCE: f64 = 1.0 / 960.0;
        self.next_event_index = self
            .definition
            .events
            .partition_point(|e| e.beat_offset < phase - PHASE_LOCK_TOLERANCE);
        self.active_ramps.clear();
        self.clear_active_notes();
        self.cc_state = default_cc_state();
        true
    }

    /// Get info about this fugue for listing
    pub fn info(&self, current_beat: f64, time_sig_numerator: u32) -> FugueInfo {
        let progress = if self.waiting_for_start {
            0.0
        } else {
            let local_beat = current_beat - self.start_beat;
            local_beat.clamp(0.0, self.definition.duration_beats)
        };

        FugueInfo {
            id: self.definition.id,
            tag: self.definition.tag.clone(),
            current_loop: self.current_loop,
            total_loops: match self.definition.loop_mode {
                LoopMode::Once => Some(1),
                LoopMode::Times(n) => Some(n),
                LoopMode::Forever => None,
            },
            is_waiting: self.waiting_for_start,
            progress_beats: progress,
            duration_beats: self.definition.duration_beats,
            start_beat: self.start_beat,
            quantize_interval_beats: self.definition.quantize.interval_beats(time_sig_numerator),
        }
    }

    /// Process events in the given beat range and return ProcessedEvents
    ///
    /// This handles:
    /// - Emitting instant events (notes, per-note expression)
    /// - Tracking CC state and generating CcRamp events for interpolation
    /// - Continuing active ramps from previous buffers
    ///
    /// # Arguments
    /// * `current_beat` - Start of the buffer in absolute beats
    /// * `end_beat` - End of the buffer in absolute beats
    /// * `beats_per_sample` - Conversion factor from beats to samples
    ///
    /// # Returns
    /// Vector of ProcessedEvents to be handled by the MIDI processor
    pub fn process_buffer(
        &mut self,
        current_beat: f64,
        end_beat: f64,
        beats_per_sample: f64,
    ) -> Vec<ProcessedEvent> {
        let mut events = Vec::new();

        if self.waiting_for_start || self.is_finished() {
            return events;
        }

        // Calculate local beat range within this fugue
        let local_start = current_beat - self.start_beat;
        let local_end = end_beat - self.start_beat;

        // Process active ramps that continue from previous buffers
        self.process_active_ramps(
            current_beat,
            end_beat,
            beats_per_sample,
            &mut events,
        );

        // Process events in this range
        while self.next_event_index < self.definition.events.len() {
            let timed_event = &self.definition.events[self.next_event_index];
            let beat_offset = timed_event.beat_offset;

            if beat_offset >= local_end {
                break; // Event is in the future
            }

            if beat_offset >= local_start {
                // Event is in this buffer - calculate sample offset.
                //
                // `.floor()`, not `.round()`: the iteration gate above
                // guarantees `beat_offset < local_end`, so mathematically
                // `beat_delta / beats_per_sample < frames`. But `.round()`
                // can round a value like `frames - 0.01` UP to `frames`,
                // which is one past the buffer end — the CLAP host drops
                // the event. In practice that silently ate the iteration-2
                // beat-0 NoteOn whenever floating-point drift put the buffer
                // boundary a hair past the fugue's duration boundary.
                // Floor keeps the offset strictly in `[0, frames)` while
                // losing at most one sample of timing precision (~20 µs at
                // 48 kHz, inaudible).
                let event_absolute_beat = self.start_beat + beat_offset;
                let beat_delta = event_absolute_beat - current_beat;
                let raw_sample_offset = (beat_delta / beats_per_sample).floor().max(0.0) as u32;

                // Clone event to avoid borrow issues
                let event = timed_event.event;

                // Left-skew NoteOffs by one audio frame so a NoteOff landing
                // at the same sample as a following NoteOn — adjacent
                // repeat notes mid-pattern, or the last NoteOff of one
                // loop iteration meeting the next iteration's beat-0
                // NoteOn within the same buffer — resolves to a strictly
                // earlier audio sample. Same-sample pairs otherwise reach
                // the synth as simultaneous events and several synths
                // collapse them, dropping the retrigger. emit_notes drops
                // zero-duration notes so this skew can never push a
                // NoteOff before its own NoteOn.
                //
                // Edge case not handled here: when the NoteOff's natural
                // sample_offset is already 0 (buffer start aligned with
                // the beat), saturating_sub leaves it at 0 and the race
                // falls through to output-buffer ordering. Rare in
                // practice — audio buffers aren't beat-aligned. A proper
                // fix would stash these NoteOffs for emission at
                // `frames - 1` of the previous buffer; deferred.
                let sample_offset = match event {
                    FugueEvent::NoteOff { .. } => raw_sample_offset.saturating_sub(1),
                    _ => raw_sample_offset,
                };

                // Process the event based on type
                self.process_event(
                    &event,
                    beat_offset,
                    sample_offset,
                    &mut events,
                );
            }

            self.next_event_index += 1;
        }

        events
    }

    /// Process a single fugue event
    fn process_event(
        &mut self,
        event: &FugueEvent,
        beat_offset: f64,
        sample_offset: u32,
        events: &mut Vec<ProcessedEvent>,
    ) {
        match event {
            FugueEvent::NoteOn { channel, note, velocity } => {
                self.set_note_active(*channel, *note);
                let msg = MidiMessage::Note(NoteMessage::new(*channel, *note, *velocity, true));
                events.push(ProcessedEvent::Instant { sample_offset, message: msg });
            }
            FugueEvent::NoteOff { channel, note } => {
                self.set_note_inactive(*channel, *note);
                let msg = MidiMessage::Note(NoteMessage::new(*channel, *note, 0, false));
                events.push(ProcessedEvent::Instant { sample_offset, message: msg });
            }
            FugueEvent::Cc { channel, cc, value, curve } => {
                self.process_cc_event(*channel, *cc, *value, *curve, beat_offset, sample_offset, events);
            }
            FugueEvent::PerNotePitchBend { channel, note, semitones } => {
                let msg = MidiMessage::PerNoteExpression(
                    PerNoteExpressionMessage::pitch_bend_semitones(*channel, *note, *semitones),
                );
                events.push(ProcessedEvent::Instant { sample_offset, message: msg });
            }
            FugueEvent::PerNotePressure { channel, note, pressure } => {
                let msg = MidiMessage::PerNoteExpression(
                    PerNoteExpressionMessage::pressure_normalized(*channel, *note, *pressure),
                );
                events.push(ProcessedEvent::Instant { sample_offset, message: msg });
            }
        }
    }

    /// Process a CC event — either emit instant or start a ramp.
    ///
    /// `event_curve` is the per-segment curve from the incoming event (the
    /// "curve to this point" convention). When `None`, the fugue-level
    /// `cc_interpolation` is used. This lets a single fugue combine curves
    /// across segments — e.g. exp up then log down for a filter pump.
    fn process_cc_event(
        &mut self,
        channel: u8,
        cc: u8,
        value: u8,
        event_curve: Option<InterpolationMode>,
        beat_offset: f64,
        sample_offset: u32,
        events: &mut Vec<ProcessedEvent>,
    ) {
        let ch = (channel & 0x0F) as usize;
        let cc_idx = cc as usize;

        // Check if we have a previous value for this CC
        let prev_value = self.cc_state[ch][cc_idx];

        // Update CC state
        self.cc_state[ch][cc_idx] = Some(value);

        // Resolve the curve for the segment arriving at this event.
        let segment_curve = event_curve.unwrap_or(self.definition.cc_interpolation);

        // If no previous value or stepped mode, emit instant CC.
        if prev_value.is_none() || segment_curve == InterpolationMode::None {
            let msg = MidiMessage::Cc(CcMessage::new(channel, cc, value));
            events.push(ProcessedEvent::Instant { sample_offset, message: msg });
            return;
        }

        let start_value = prev_value.unwrap();

        // If values are the same, just emit instant (no ramp needed)
        if start_value == value {
            let msg = MidiMessage::Cc(CcMessage::new(channel, cc, value));
            events.push(ProcessedEvent::Instant { sample_offset, message: msg });
            return;
        }

        // Find the previous CC event to determine ramp start beat
        // Look backwards in events to find the previous CC on this channel/cc
        let ramp_start_beat = self.find_previous_cc_beat(channel, cc, beat_offset);

        // Create ramp from previous value to new value using this segment's
        // curve. Cross-segment combinations (e.g. exp→log) "just work" because
        // each ramp carries its own interpolation mode on the audio thread.
        let absolute_start_beat = self.start_beat + ramp_start_beat;
        let absolute_end_beat = self.start_beat + beat_offset;

        // Remove any existing ramp for this channel/cc
        self.active_ramps.retain(|r| r.channel != channel || r.cc != cc);

        // Add new ramp
        self.active_ramps.push(ActiveCcRamp {
            channel,
            cc,
            start_value,
            end_value: value,
            start_beat: absolute_start_beat,
            end_beat: absolute_end_beat,
            interpolation: segment_curve,
        });
    }

    /// Find the beat offset of the previous CC event for this channel/cc
    fn find_previous_cc_beat(&self, channel: u8, cc: u8, current_beat_offset: f64) -> f64 {
        for i in (0..self.next_event_index).rev() {
            let event = &self.definition.events[i];
            if let FugueEvent::Cc { channel: ch, cc: c, .. } = event.event {
                if ch == channel && c == cc && event.beat_offset < current_beat_offset {
                    return event.beat_offset;
                }
            }
        }
        // If no previous event found, assume ramp starts at 0
        0.0
    }

    /// Process active ramps and emit CcRamp events for the current buffer
    fn process_active_ramps(
        &mut self,
        current_beat: f64,
        end_beat: f64,
        beats_per_sample: f64,
        events: &mut Vec<ProcessedEvent>,
    ) {
        // Remove finished ramps and emit events for active ones
        let mut finished_indices = Vec::new();

        for (idx, ramp) in self.active_ramps.iter().enumerate() {
            // Check if ramp overlaps with current buffer
            if ramp.end_beat <= current_beat {
                // Ramp already finished
                finished_indices.push(idx);
                continue;
            }

            if ramp.start_beat >= end_beat {
                // Ramp hasn't started yet
                continue;
            }

            // Calculate sample range for this buffer
            let ramp_start_in_buffer = ramp.start_beat.max(current_beat);
            let ramp_end_in_buffer = ramp.end_beat.min(end_beat);

            let start_sample = ((ramp_start_in_buffer - current_beat) / beats_per_sample)
                .round()
                .max(0.0) as u32;
            let end_sample = ((ramp_end_in_buffer - current_beat) / beats_per_sample)
                .round()
                .max(0.0) as u32;

            // Calculate interpolated values at buffer boundaries
            let ramp_duration = ramp.end_beat - ramp.start_beat;
            if ramp_duration <= 0.0 {
                continue;
            }

            let t_start = (ramp_start_in_buffer - ramp.start_beat) / ramp_duration;
            let t_end = (ramp_end_in_buffer - ramp.start_beat) / ramp_duration;

            let start_value = interpolate_value(
                ramp.start_value,
                ramp.end_value,
                t_start,
                ramp.interpolation,
            );
            let end_value = interpolate_value(
                ramp.start_value,
                ramp.end_value,
                t_end,
                ramp.interpolation,
            );

            events.push(ProcessedEvent::CcRamp {
                channel: ramp.channel,
                cc: ramp.cc,
                start_value,
                end_value,
                start_sample,
                end_sample,
                interpolation: ramp.interpolation,
            });

            // Mark ramp as finished if it ends in this buffer
            if ramp.end_beat <= end_beat {
                finished_indices.push(idx);
            }
        }

        // Remove finished ramps (in reverse order to preserve indices)
        for idx in finished_indices.into_iter().rev() {
            self.active_ramps.remove(idx);
        }
    }

    /// Clear CC state (called when fugue is reset or cancelled)
    pub fn clear_cc_state(&mut self) {
        self.cc_state = default_cc_state();
        self.active_ramps.clear();
    }
}

/// Interpolate between two values. The curve shape comes from
/// [`InterpolationMode::apply_curve`] — all curves are handled uniformly here.
fn interpolate_value(start: u8, end: u8, t: f64, mode: InterpolationMode) -> u8 {
    let tc = mode.apply_curve(t);
    let start_f = start as f64;
    let end_f = end as f64;
    (start_f + (end_f - start_f) * tc).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_fugue() -> FugueDefinition {
        FugueDefinition::new(vec![], 4.0)
    }

    #[test]
    fn test_note_tracking() {
        let def = make_test_fugue();
        let mut fugue = Fugue::new(def);

        // Initially no notes active
        assert!(!fugue.is_note_active(0, 60));
        assert!(fugue.get_active_notes().is_empty());

        // Set note active
        fugue.set_note_active(0, 60);
        assert!(fugue.is_note_active(0, 60));
        assert_eq!(fugue.get_active_notes(), vec![(0, 60)]);

        // Set another note on different channel
        fugue.set_note_active(1, 64);
        assert!(fugue.is_note_active(1, 64));
        assert_eq!(fugue.get_active_notes(), vec![(0, 60), (1, 64)]);

        // Clear one note
        fugue.set_note_inactive(0, 60);
        assert!(!fugue.is_note_active(0, 60));
        assert!(fugue.is_note_active(1, 64));

        // Clear all
        fugue.clear_active_notes();
        assert!(fugue.get_active_notes().is_empty());
    }

    #[test]
    fn test_note_tracking_high_notes() {
        let def = make_test_fugue();
        let mut fugue = Fugue::new(def);

        // Test notes in the high range (64-127)
        fugue.set_note_active(0, 100);
        fugue.set_note_active(0, 127);
        assert!(fugue.is_note_active(0, 100));
        assert!(fugue.is_note_active(0, 127));
        assert!(!fugue.is_note_active(0, 60));
    }
}
