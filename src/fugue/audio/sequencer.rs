//! Fugue Sequencer - audio thread playback engine
//!
//! Processes fugue commands and outputs MIDI events with sample-accurate timing.

use rtrb::Consumer;

use super::super::command::FugueCommand;
use super::fugue::Fugue;
use super::super::types::{CancelMode, FugueDefinition, FugueEvent, FugueInfo, ProcessedEvent, StartMode};
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

    /// Process one audio buffer cycle.
    ///
    /// Fills `self.output_buffer` with events emitted this buffer.
    /// Call [`Self::events`] to iterate them afterwards — splitting
    /// the mutate and read phases lets the caller hold a shared
    /// borrow during iteration without conflicting with unrelated
    /// `&self` access (e.g. the MIDI processor's output helpers).
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
    ) {
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
            return;
        }

        // 3. Handle transport state transitions and jumps
        let just_started = !self.was_playing;
        self.was_playing = true;

        if just_started {
            // Transport just started - phase-lock all looping fugues to the
            // new transport position so they resume as if they'd been playing
            // along with the song timeline.
            self.phase_lock_all(current_beat);
            self.last_beat = current_beat;
        } else {
            // Transport was already playing - detect jumps (seeking).
            let beat_jump = (current_beat - self.last_beat).abs();
            let expected_advance = tempo_bpm / 60.0 / self.sample_rate * frames as f64;
            if beat_jump > expected_advance * 2.0 + 0.01 {
                self.phase_lock_all(current_beat);
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

        // The deconflict post-pass that used to run here (shifting
        // NoteOns one sample forward when they collided with a
        // phase_lock-emitted NoteOff at sample 0) is gone. The
        // targeted fix in `send_note_offs_for_fugue_skipping_beat0_retriggers`
        // eliminates the collision at source: phase_lock_all no
        // longer emits NoteOffs for pitches the new iteration is
        // about to retrigger, so no OFF/ON pair shares a sample on
        // the critical path. Same-pitch retriggers mid-pattern are
        // handled by TimedNote's own retrigger-eviction path inside
        // `Fugue::process_event`. The Feature 28 regression tests
        // cover both.
        self.last_beat = end_beat;
    }

    /// Events emitted by the most recent [`Self::process`] call. The
    /// slice is valid until the next `process` call (which clears the
    /// buffer before refilling it).
    pub fn events(&self) -> &[ProcessedEvent] {
        &self.output_buffer
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

    /// Phase-lock all fugues to the given transport beat.
    ///
    /// Called on transport start and on relocate. Looping fugues are re-anchored
    /// so their phase matches `transport_beat` (so beat 4 of a 4-beat loop lands
    /// on transport beat 4, 8, 12, ...). Mid-flight one-shots can't meaningfully
    /// resume, so they're dropped. Emits note-offs for all held notes first so
    /// the synth doesn't get stuck.
    fn phase_lock_all(&mut self, transport_beat: f64) {
        let output_buffer = &mut self.output_buffer;
        self.fugues.retain_mut(|f| {
            if !f.get_active_notes().is_empty() {
                // Targeted cleanup: skip emitting NoteOffs for pitches
                // that the new iteration's beat-0 event will retrigger
                // anyway. Without this we emit NoteOff@0 for a pitch
                // that's about to get a fresh NoteOn@0 from the same
                // buffer — same-sample OFF/ON, which is what the
                // deconflict post-pass used to rescue. Letting the
                // retrigger fire cleanly (synth keeps its envelope,
                // just re-triggers a new voice) is the musically
                // correct outcome and eliminates the race at its
                // source.
                //
                // Only applies when the new iteration actually starts
                // from beat 0 — i.e., the phase-lock target phase is 0
                // (to a tolerance). Otherwise the old pitch needs its
                // NoteOff now, because the new iteration will skip
                // past beat 0 without firing the matching NoteOn.
                let dur = f.definition.duration_beats;
                let phase = if dur > 0.0 { transport_beat.rem_euclid(dur) } else { 0.0 };
                let new_iter_starts_at_beat_zero =
                    phase.abs() < 1e-6 || (dur - phase).abs() < 1e-6;

                send_note_offs_for_fugue_skipping_beat0_retriggers(
                    f,
                    output_buffer,
                    new_iter_starts_at_beat_zero,
                );
            }
            f.phase_lock(transport_beat)
        });
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
            // Floor, not round — the `target < end_beat` check above
            // guarantees `(target - current_beat) / beats_per_sample < frames`,
            // and rounding UP could push sample_offset to `frames` which
            // the CLAP host drops as out-of-buffer. Same bug pattern as
            // event emission in Fugue::process_buffer.
            let sample_offset = ((target - current_beat) / beats_per_sample).floor().max(0.0) as u32;

            // Apply cancel mode at the same sample offset as the new fugue starts
            self.apply_cancel_mode_at_offset(&cancel_mode, sample_offset);

            // Now start the fugue. Place the iteration on the song-grid
            // (multiples of `duration_beats` from song-beat-0) rather
            // than at `target` directly — this is the critical invariant
            // that keeps pattern-beat-0 landing on the same transport
            // beats regardless of LLM queue latency, and matches what
            // `phase_lock` does on transport jumps. Without this, a fugue
            // queued mid-song plays at one offset and then shifts to
            // another the first time the DAW wraps.
            let fugue = &mut self.fugues[idx];
            let dur = fugue.definition.duration_beats;
            place_on_song_grid(fugue, target, dur);
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

            // Fugue pushes directly into the sequencer's shared output
            // buffer — no per-call Vec allocation.
            fugue.process_buffer(current_beat, end_beat, beats_per_sample, &mut self.output_buffer);

            // Check for loop boundary
            if local_end >= fugue.definition.duration_beats && !fugue.is_finished() {
                // Reset for next loop
                fugue.reset_for_loop();

                // Process events from the start of the new loop if buffer extends into it
                let new_local_end = end_beat - fugue.start_beat;
                if new_local_end > 0.0 {
                    fugue.process_buffer(current_beat, end_beat, beats_per_sample, &mut self.output_buffer);
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

/// Transition a `waiting_for_start` fugue to active, placing its
/// iteration boundaries on the song-grid `k · dur` from transport-0.
/// Handles both start modes from `StartMode`:
///
/// - `Phase` (default): the fugue joins the implicit always-running grid
///   at whatever phase the target currently sits at. `start_beat` is
///   snapped *back* to the most recent dur-multiple ≤ target, and
///   `next_event_index` is advanced past any events before the current
///   phase so the fugue plays from wherever it "should be" right now.
/// - `Boundary`: the fugue delays until the next dur-aligned boundary
///   ≥ target, then plays from pattern-beat-0. Implemented by parking
///   it back in `waiting_for_start` with a bumped `target_start_beat` —
///   the next process cycle starts it cleanly at the new target.
///
/// In both cases the post-start iteration sequence is identical: events
/// fire at transport beats `start_beat + beat_offset`, where `start_beat`
/// is a multiple of `dur` from song-zero and every subsequent iteration
/// is `start_beat + k·dur`. Phase_lock preserves this invariant on
/// transport jumps.
fn place_on_song_grid(fugue: &mut Fugue, target: f64, dur: f64) {
    if dur <= 0.0 {
        // Malformed fugue — fall back to the pre-fix behavior (start
        // exactly at target). Shouldn't occur: duration auto-sizing
        // guarantees dur ≥ 4 beats for any well-formed fugue.
        fugue.waiting_for_start = false;
        fugue.start_beat = target;
        fugue.next_event_index = 0;
        return;
    }
    let phase = target.rem_euclid(dur);
    match fugue.definition.start_mode {
        StartMode::Phase => {
            // Virtual start is the most-recent multiple of dur ≤ target.
            // Events before `phase` already happened in the implicit
            // past iteration — skip past them so the fugue plays from
            // its current song-phase position.
            fugue.waiting_for_start = false;
            fugue.start_beat = target - phase;
            fugue.next_event_index = fugue
                .definition
                .events
                .partition_point(|e| e.beat_offset < phase);
        }
        StartMode::Boundary => {
            // Already on a boundary: start from pattern-beat-0 right now.
            if phase == 0.0 {
                fugue.waiting_for_start = false;
                fugue.start_beat = target;
                fugue.next_event_index = 0;
                return;
            }
            // Otherwise push target to the next dur-aligned boundary
            // and keep waiting. `start_pending_fugues` will pick the
            // fugue up again on a future process cycle once the
            // transport reaches that boundary.
            let next_boundary = target + (dur - phase);
            fugue.target_start_beat = Some(next_boundary);
            // waiting_for_start stays true; we deliberately don't
            // clear it here.
        }
    }
}

/// Send note-offs for all active notes in a fugue at sample offset 0
fn send_note_offs_for_fugue(fugue: &mut Fugue, output_buffer: &mut Vec<ProcessedEvent>) {
    send_note_offs_for_fugue_at_offset(fugue, output_buffer, 0);
}

/// Same as [`send_note_offs_for_fugue`] but filters out held pitches
/// that a beat-0 TimedNote / NoteOn in the fugue's definition will
/// retrigger in the same buffer — called only by `phase_lock_all` to
/// eliminate the OFF@0 / ON@0 collision at transport jumps.
///
/// When `new_iter_starts_at_beat_zero` is false (phase-locked mid-
/// pattern), retriggers don't apply and every held note is released.
fn send_note_offs_for_fugue_skipping_beat0_retriggers(
    fugue: &mut Fugue,
    output_buffer: &mut Vec<ProcessedEvent>,
    new_iter_starts_at_beat_zero: bool,
) {
    // Scan the fugue's definition for beat-0 events whose (channel,
    // note) matches any currently-held note. Those are the retriggers
    // we can elide. Iterating until beat_offset > 0 is O(few events)
    // because the definition is sorted by beat_offset.
    let held = fugue.get_active_notes();
    let mut retrigger_targets: [(u8, u8); 32] = [(0xFF, 0xFF); 32];
    let mut retrigger_count = 0usize;
    if new_iter_starts_at_beat_zero {
        for ev in fugue.definition.events.iter() {
            if ev.beat_offset > 0.0 {
                break;
            }
            let pair = match ev.event {
                FugueEvent::TimedNote { channel, note, .. } => Some((channel, note)),
                FugueEvent::NoteOn { channel, note, .. } => Some((channel, note)),
                _ => None,
            };
            if let Some((ch, n)) = pair {
                if held.iter().any(|(hc, hn)| *hc == ch && *hn == n)
                    && retrigger_count < retrigger_targets.len()
                {
                    retrigger_targets[retrigger_count] = (ch, n);
                    retrigger_count += 1;
                }
            }
        }
    }

    for (channel, note) in &held {
        let will_retrigger = retrigger_targets[..retrigger_count]
            .iter()
            .any(|(c, n)| c == channel && n == note);
        if will_retrigger {
            // Elide: the new iteration's own NoteOn at sample 0 will
            // retrigger the synth voice. Emitting a NoteOff first would
            // just race that NoteOn in the same buffer.
            continue;
        }
        let msg = MidiMessage::Note(NoteMessage::new(*channel, *note, 0, false));
        output_buffer.push(ProcessedEvent::Instant { sample_offset: 0, message: msg });
    }
    fugue.clear_active_notes();
    fugue.clear_pending_note_offs();
    fugue.clear_cc_state();
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
    // Ring entries for TimedNote-scheduled NoteOffs are now spurious —
    // the notes they track were just released via the bitset flush
    // above. Clearing the ring prevents a "ghost NoteOff" from firing
    // later for a pitch the synth doesn't currently have voiced.
    fugue.clear_pending_note_offs();
    fugue.clear_cc_state();
}

#[cfg(test)]
mod daw_loop_tests {
    //! End-to-end tests for `FugueSequencer::process`, targeting the
    //! "first note drops on DAW transport loop" bug. These run entirely
    //! on the main thread — we drive the sequencer via its public
    //! interface with synthetic transport positions and inspect
    //! `ProcessedEvent` output, so we can reproduce the DAW-loop path
    //! without a plugin host.
    //!
    //! The core scenario is: DAW transport loop length equals the
    //! fugue's `duration_beats`, so when the DAW wraps, our
    //! `phase_lock_all` jump-handling path runs in the same buffer that
    //! would fire the new iteration's beat-0 NoteOn. If the beat-0
    //! NoteOn goes missing or lands wrong, the test catches it.
    use super::*;
    use crate::fugue::types::{FugueEvent, LoopMode, TimedFugueEvent};
    use rtrb::RingBuffer;

    const TEMPO: f64 = 120.0;
    const SAMPLE_RATE: f64 = 48_000.0;
    const TIME_SIG: u32 = 4;
    const BUFFER_FRAMES: u32 = 512;

    fn beats_per_sample() -> f64 {
        TEMPO / 60.0 / SAMPLE_RATE
    }

    fn buffer_beats() -> f64 {
        BUFFER_FRAMES as f64 * beats_per_sample()
    }

    /// Build a fugue with a NoteOn at each given `(beat, pitch, duration)`.
    /// Channel 0, velocity 100. The NoteOff lands at `beat + duration`.
    fn make_notes_fugue(notes: &[(f64, u8, f64)], duration_beats: f64) -> FugueDefinition {
        let mut events = Vec::new();
        for &(beat, pitch, dur) in notes {
            events.push(TimedFugueEvent::new(
                beat,
                FugueEvent::NoteOn { channel: 0, note: pitch, velocity: 100 },
            ));
            events.push(TimedFugueEvent::new(
                beat + dur,
                FugueEvent::NoteOff { channel: 0, note: pitch },
            ));
        }
        events.sort_by(|a, b| a.beat_offset.partial_cmp(&b.beat_offset).unwrap());
        FugueDefinition::new(events, duration_beats).with_loop_mode(LoopMode::Forever)
    }

    /// Queue a fugue + build a sequencer, bypassing the full `FugueBridge`
    /// global registry. Returns the sequencer with the fugue already
    /// active (the first `process` call consumes the Queue command).
    fn sequencer_with_fugue(def: FugueDefinition) -> FugueSequencer {
        let (mut prod, cons) = RingBuffer::<FugueCommand>::new(16);
        prod.push(FugueCommand::Queue(def)).expect("queue must fit");
        FugueSequencer::new(cons, SAMPLE_RATE)
    }

    /// Build a sequencer with no fugues queued yet. Returns both the
    /// producer (so the test can push `Queue` commands mid-playback,
    /// after transport has advanced to a non-zero beat) and the
    /// sequencer. Used by the Feature 25 tests where we need the
    /// quantize target at queue time to land off the dur-grid — which
    /// can't happen at transport 0, because every grid includes beat 0.
    fn empty_sequencer() -> (rtrb::Producer<FugueCommand>, FugueSequencer) {
        let (prod, cons) = RingBuffer::<FugueCommand>::new(16);
        (prod, FugueSequencer::new(cons, SAMPLE_RATE))
    }

    /// Collect every `(sample_offset, channel, note, is_on)` note event
    /// in the buffer. Lets tests assert on concrete event lists.
    fn collect_notes(events: &[ProcessedEvent]) -> Vec<(u32, u8, u8, bool)> {
        events
            .iter()
            .filter_map(|e| match e {
                ProcessedEvent::Instant { sample_offset, message: MidiMessage::Note(n) } => {
                    Some((*sample_offset, n.channel, n.note, n.is_note_on))
                }
                _ => None,
            })
            .collect()
    }

    /// Play `beats_to_play` worth of buffers on `seq`, returning the
    /// concatenated ProcessedEvents so the caller can assert on them.
    /// The final buffer is *not* included — use this to warm the
    /// sequencer up to a known transport position before the scenario
    /// under test.
    fn play_until(seq: &mut FugueSequencer, start_beat: f64, beats_to_play: f64) -> f64 {
        let mut current = start_beat;
        let end = start_beat + beats_to_play;
        while current < end {
            seq.process(true, current, TEMPO, BUFFER_FRAMES, TIME_SIG);
            current += buffer_beats();
        }
        current
    }

    #[test]
    fn beat_zero_note_fires_on_daw_loop_wrap_matching_fugue_duration() {
        // DAW loops a 4-beat region. Fugue is 4 beats with a note at
        // beat 0. After the DAW wraps, the new iteration's beat-0 NoteOn
        // must appear in the output buffer — that's the exact note the
        // bug report says is missing.
        let def = make_notes_fugue(
            &[(0.0, 60, 0.5), (1.0, 62, 0.5), (2.0, 64, 0.5), (3.0, 65, 0.5)],
            4.0,
        );
        let mut seq = sequencer_with_fugue(def);

        // Warm through one full fugue iteration at transport beats [0, 4).
        play_until(&mut seq, 0.0, 4.0);

        // DAW wraps back to 0. This is the buffer under test.
        seq.process(true, 0.0, TEMPO, BUFFER_FRAMES, TIME_SIG);
        let notes = collect_notes(seq.events());
        let beat_zero_on = notes
            .iter()
            .find(|(_, ch, note, on)| *on && *ch == 0 && *note == 60);
        assert!(
            beat_zero_on.is_some(),
            "beat-0 NoteOn (ch=0, note=60) missing after DAW loop wrap. Events: {:?}",
            notes
        );
    }

    #[test]
    fn beat_zero_note_fires_on_daw_loop_wrap_with_held_prior_note_same_pitch() {
        // Tighter version: the last note of the iteration holds the
        // same pitch that the next iteration opens with, so
        // `phase_lock_all` emits a NoteOff for pitch 60 at sample 0 of
        // the wrap buffer AND the fugue's beat-0 NoteOn is also for
        // pitch 60 at sample 0. Synths collapsing the pair is the exact
        // race the deconflict pass is meant to prevent.
        let def = make_notes_fugue(
            &[
                (0.0, 60, 0.5),
                // A long sustain whose NoteOff lands at 4.0 = duration,
                // so it's excluded from the old iteration and the pitch
                // is still "active" at wrap time.
                (2.0, 60, 2.0),
            ],
            4.0,
        );
        let mut seq = sequencer_with_fugue(def);

        play_until(&mut seq, 0.0, 4.0);

        seq.process(true, 0.0, TEMPO, BUFFER_FRAMES, TIME_SIG);
        let notes = collect_notes(seq.events());
        let note_on_for_60 = notes
            .iter()
            .find(|(_, ch, note, on)| *on && *ch == 0 && *note == 60);
        assert!(
            note_on_for_60.is_some(),
            "beat-0 NoteOn (ch=0, note=60) missing on wrap when the prior iteration left \
             pitch 60 held. Events: {:?}",
            notes
        );

        // Sanity: if a NoteOff for 60 also appears, it must come at a
        // sample offset strictly before the NoteOn. Otherwise the synth
        // eats the retrigger.
        if let Some(note_off) = notes
            .iter()
            .find(|(_, ch, note, on)| !*on && *ch == 0 && *note == 60)
        {
            let (off_sample, _, _, _) = *note_off;
            let (on_sample, _, _, _) = *note_on_for_60.unwrap();
            assert!(
                off_sample < on_sample,
                "NoteOff for pitch 60 at sample {} must precede NoteOn at sample {} \
                 in the wrap buffer; same-sample pairs race on real synths. Events: {:?}",
                off_sample,
                on_sample,
                notes
            );
        }
    }

    #[test]
    fn beat_zero_note_fires_on_each_fugue_internal_loop_without_daw_loop() {
        // Control case: no DAW loop. Fugue is 4 beats, transport runs
        // continuously. Every iteration boundary should fire a beat-0
        // NoteOn. If this fails, the bug is internal to the fugue's
        // reset_for_loop path, not the DAW-jump path.
        let def = make_notes_fugue(
            &[(0.0, 60, 0.5), (1.0, 62, 0.5)],
            4.0,
        );
        let mut seq = sequencer_with_fugue(def);

        // Play through iterations 1, 2, 3 of the fugue. We stop the
        // transport before it reaches beat 12 so iteration 4's beat-0
        // NoteOn doesn't sneak in. Each buffer advances ~0.02 beats, so
        // 11.9 gives ~560 buffers and three complete iterations.
        let mut all: Vec<(u32, u8, u8, bool)> = Vec::new();
        let mut current = 0.0;
        while current + buffer_beats() <= 11.9 {
            seq.process(true, current, TEMPO, BUFFER_FRAMES, TIME_SIG);
            all.extend(collect_notes(seq.events()));
            current += buffer_beats();
        }

        let note_on_60_count = all
            .iter()
            .filter(|(_, ch, note, on)| *on && *ch == 0 && *note == 60)
            .count();
        assert_eq!(
            note_on_60_count, 3,
            "expected 3 beat-0 NoteOns across 3 fugue iterations; got {}. All events: {:?}",
            note_on_60_count, all
        );
    }

    // -----------------------------------------------------------------
    // Feature 25 regression tests — iteration boundaries on song-grid
    // regardless of queue-time quantize target (LLM latency can't shift
    // musical alignment).
    // -----------------------------------------------------------------

    /// Drive the sequencer forward without any fugues queued, up to
    /// `target_beat` (roughly — we stop at the first buffer whose start
    /// is ≥ target). Returns the transport beat reached. Used by the
    /// queue-mid-play Feature 25 tests that need the quantize grid to
    /// round up to a non-dur-aligned target (which can only happen at
    /// a non-zero transport position).
    fn advance_empty(seq: &mut FugueSequencer, target_beat: f64) -> f64 {
        let mut current = 0.0;
        while current < target_beat {
            seq.process(true, current, TEMPO, BUFFER_FRAMES, TIME_SIG);
            current += buffer_beats();
        }
        current
    }

    /// Collect every note event over `total_beats` starting from
    /// `start_beat`. Returns `(absolute_beat, channel, note, is_on)`
    /// tuples so tests can assert on *when in the transport timeline*
    /// each event fired, not just which buffer.
    fn run_and_collect(
        seq: &mut FugueSequencer,
        start_beat: f64,
        total_beats: f64,
    ) -> Vec<(f64, u8, u8, bool)> {
        let mut events: Vec<(f64, u8, u8, bool)> = Vec::new();
        let mut current = start_beat;
        let end = start_beat + total_beats;
        while current < end {
            seq.process(true, current, TEMPO, BUFFER_FRAMES, TIME_SIG);
            for (sample, ch, note, on) in collect_notes(seq.events()) {
                let absolute = current + sample as f64 * beats_per_sample();
                events.push((absolute, ch, note, on));
            }
            current += buffer_beats();
        }
        events
    }

    /// Queue a 16-beat fugue at transport ~5 with `quantize: "bar"` —
    /// next bar is 8, which isn't a multiple of 16. Under Phase mode,
    /// the fugue joins the implicit song-grid at current phase: first
    /// pattern-beat-0 firing lands on transport 16 (next dur boundary),
    /// not transport 8. Events before beat 8 are skipped from the
    /// current iteration.
    #[test]
    fn phase_mode_puts_pattern_beat_0_on_song_grid_not_target() {
        use crate::fugue::types::QuantizeMode;
        let def = make_notes_fugue(&[(0.0, 60, 0.5)], 16.0)
            .with_quantize(QuantizeMode::Bar)
            .with_start_mode(StartMode::Phase);
        let (mut prod, mut seq) = empty_sequencer();

        // Advance to transport ~5 before pushing the Queue — the first
        // buffer that sees the queue command has current_beat > 4, so
        // quantize:bar rounds target up to 8 (mid-duration for dur=16).
        let queued_at = advance_empty(&mut seq, 5.0);
        prod.push(FugueCommand::Queue(def)).unwrap();

        let events = run_and_collect(&mut seq, queued_at, 20.0);
        let first_note_60 = events
            .iter()
            .find(|(_, ch, note, on)| *on && *ch == 0 && *note == 60);

        assert!(
            first_note_60.is_some(),
            "expected note 60 to fire. Events: {:?}",
            events
        );
        let (first_absolute, _, _, _) = first_note_60.unwrap();
        assert!(
            (*first_absolute - 16.0).abs() < 0.05,
            "pattern-beat-0 should fire at transport 16 (next multiple of dur=16), \
             got transport {:.3}. Events: {:?}",
            first_absolute,
            events
        );
    }

    /// Same setup but with `StartMode::Boundary`: the fugue waits for
    /// the next dur-boundary (transport 16) and plays from pattern-beat-0
    /// there. No notes should fire in the pre-boundary window (transport
    /// 8 → 16); leakage would mean Boundary mode is behaving like Phase.
    #[test]
    fn boundary_mode_waits_for_next_dur_boundary_and_stays_silent_until_then() {
        use crate::fugue::types::QuantizeMode;
        let def = make_notes_fugue(&[(0.0, 60, 0.5), (4.0, 62, 0.5)], 16.0)
            .with_quantize(QuantizeMode::Bar)
            .with_start_mode(StartMode::Boundary);
        let (mut prod, mut seq) = empty_sequencer();

        let queued_at = advance_empty(&mut seq, 5.0);
        prod.push(FugueCommand::Queue(def)).unwrap();

        let events = run_and_collect(&mut seq, queued_at, 22.0);

        // Pre-boundary window = events before transport 16 minus the
        // initial advance. If Boundary mode leaks, some event would
        // fire between transport 8 and 16.
        let leaked: Vec<_> = events
            .iter()
            .filter(|(t, _, _, on)| *on && *t >= 8.0 && *t < 15.95)
            .collect();
        assert!(
            leaked.is_empty(),
            "Boundary mode should stay silent from transport 8 → 16, \
             but these events fired in that window: {:?}",
            leaked
        );

        let note_60 = events.iter().find(|(_, ch, n, on)| *on && *ch == 0 && *n == 60);
        assert!(
            note_60.is_some(),
            "note 60 should fire at transport 16 under Boundary mode. Events: {:?}",
            events
        );
        let (t60, _, _, _) = note_60.unwrap();
        assert!(
            (*t60 - 16.0).abs() < 0.05,
            "note 60 should fire ≈ transport 16 under Boundary mode; got {:.3}",
            t60
        );

        let note_62 = events.iter().find(|(_, ch, n, on)| *on && *ch == 0 && *n == 62);
        if let Some((t62, _, _, _)) = note_62 {
            assert!(
                (*t62 - 20.0).abs() < 0.05,
                "note 62 (pattern-beat-4) should fire at transport 20 \
                 under Boundary mode; got {:.3}",
                t62
            );
        }
    }

    /// Feature 26 — case 1: DAW loop length == fugue duration,
    /// transport wraps *cleanly* (reported current_beat = 0.0 exactly).
    /// No drift, tolerance isn't what rescues the beat-0 event here —
    /// this passes with or without the Feature 26 widening. Kept as
    /// the "control" sibling of the drift test so both shapes of wrap
    /// are locked down.
    #[test]
    fn beat_zero_fires_on_wrap_dur_matches_cleanly() {
        let def = make_notes_fugue(&[(0.0, 60, 0.5)], 16.0);
        let mut seq = sequencer_with_fugue(def);
        play_until(&mut seq, 0.0, 17.0);

        seq.process(true, 0.0, TEMPO, BUFFER_FRAMES, TIME_SIG);
        let wrap_notes = collect_notes(seq.events());
        let wrap_note_on = wrap_notes
            .iter()
            .find(|(_, ch, note, on)| *on && *ch == 0 && *note == 60);
        assert!(
            wrap_note_on.is_some(),
            "beat-0 NoteOn missing after clean DAW wrap (current_beat = 0.0). \
             Events in wrap buffer: {:?}",
            wrap_notes
        );
    }

    /// Feature 26 — case 2: same setup, but the DAW reports the wrap
    /// with `current_beat` already inside the new loop iteration (the
    /// host consumed ~half a buffer before calling us back). The
    /// per-sample tolerance the old code used couldn't rescue this;
    /// the widened buffer-sized tolerance has to. Reproduction came
    /// from a real log with `transport = 0.015` right after a
    /// `beat_jump = 16` wrap.
    #[test]
    fn beat_zero_fires_on_wrap_with_mid_buffer_drift() {
        let def = make_notes_fugue(&[(0.0, 60, 0.5)], 16.0);
        let mut seq = sequencer_with_fugue(def);
        play_until(&mut seq, 0.0, 17.0);

        let drift = buffer_beats() * 0.5; // ~10.7 ms at 120 BPM 512-frame buffer
        seq.process(true, drift, TEMPO, BUFFER_FRAMES, TIME_SIG);
        let wrap_notes = collect_notes(seq.events());
        let wrap_note_on = wrap_notes
            .iter()
            .find(|(_, ch, note, on)| *on && *ch == 0 && *note == 60);
        assert!(
            wrap_note_on.is_some(),
            "beat-0 NoteOn missing after DAW wrap reported with mid-buffer drift \
             (current_beat = {:.4}). Events in wrap buffer: {:?}",
            drift,
            wrap_notes
        );
    }

    /// Queue-offset stability across a DAW wrap: queue mid-play so
    /// `target` ≠ multiple of dur, play one iteration, then simulate
    /// a DAW transport wrap back to 0. Pattern-beat-0 must fire again
    /// on transport 0 (song-grid), not shifted by the queue offset.
    /// This is the exact user-reported "queue-while-playing offset
    /// from then on" bug.
    #[test]
    fn queue_offset_stable_across_daw_wrap() {
        use crate::fugue::types::QuantizeMode;
        let def = make_notes_fugue(&[(0.0, 60, 0.5)], 16.0)
            .with_quantize(QuantizeMode::Bar)
            .with_start_mode(StartMode::Phase);
        let (mut prod, mut seq) = empty_sequencer();

        let queued_at = advance_empty(&mut seq, 5.0);
        prod.push(FugueCommand::Queue(def)).unwrap();

        // Play through a full iteration — pattern-beat-0 should have
        // fired at transport 16 under Phase mode (pre-existing test
        // above asserts this).
        let _pre_wrap = run_and_collect(&mut seq, queued_at, 17.0);

        // Simulate DAW wrap back to 0.
        seq.process(true, 0.0, TEMPO, BUFFER_FRAMES, TIME_SIG);
        let wrap_notes = collect_notes(seq.events());
        let wrap_note_on = wrap_notes
            .iter()
            .find(|(_, ch, note, on)| *on && *ch == 0 && *note == 60);

        assert!(
            wrap_note_on.is_some(),
            "after DAW wrap, pattern-beat-0 should fire on transport 0 (song-grid). \
             Events in wrap buffer: {:?}",
            wrap_notes
        );
    }
}

#[cfg(test)]
mod timed_note_tests {
    //! End-to-end tests for the `FugueEvent::TimedNote` path landed in
    //! Feature 28: the audio thread schedules NoteOffs via the
    //! `PendingNoteOff` ring rather than from pre-expanded
    //! NoteOn/NoteOff events in the definition. These tests drive
    //! `FugueSequencer::process` with synthetic transport positions
    //! and assert on the emitted `(sample_offset, channel, note, is_on)`
    //! tuples — same harness shape as `daw_loop_tests`.
    use super::*;
    use crate::fugue::types::{FugueEvent, LoopMode, TimedFugueEvent};
    use rtrb::RingBuffer;

    const TEMPO: f64 = 120.0;
    const SAMPLE_RATE: f64 = 48_000.0;
    const TIME_SIG: u32 = 4;
    const BUFFER_FRAMES: u32 = 512;

    fn beats_per_sample() -> f64 {
        TEMPO / 60.0 / SAMPLE_RATE
    }

    fn buffer_beats() -> f64 {
        BUFFER_FRAMES as f64 * beats_per_sample()
    }

    /// Build a fugue using `FugueEvent::TimedNote` events — the form
    /// `emit_notes` produces from LLM-authored notes since Feature 28.
    /// Channel 0, velocity 100 unless overridden.
    fn make_timed_notes_fugue(
        notes: &[(f64, u8, f64)],
        duration_beats: f64,
    ) -> FugueDefinition {
        let mut events = Vec::new();
        for &(beat, pitch, dur) in notes {
            events.push(TimedFugueEvent::new(
                beat,
                FugueEvent::TimedNote {
                    channel: 0,
                    note: pitch,
                    velocity: 100,
                    duration_beats: dur,
                },
            ));
        }
        events.sort_by(|a, b| a.beat_offset.partial_cmp(&b.beat_offset).unwrap());
        FugueDefinition::new(events, duration_beats).with_loop_mode(LoopMode::Forever)
    }

    fn sequencer_with_fugue(def: FugueDefinition) -> FugueSequencer {
        let (mut prod, cons) = RingBuffer::<FugueCommand>::new(16);
        prod.push(FugueCommand::Queue(def)).expect("queue must fit");
        FugueSequencer::new(cons, SAMPLE_RATE)
    }

    /// Collect note events across `total_beats` of playback, returning
    /// `(absolute_beat_of_sample, channel, note, is_on)` tuples so
    /// tests can assert on song-timeline position rather than
    /// buffer-local sample offsets.
    fn collect_notes_over(
        seq: &mut FugueSequencer,
        start_beat: f64,
        total_beats: f64,
    ) -> Vec<(f64, u8, u8, bool)> {
        let mut out: Vec<(f64, u8, u8, bool)> = Vec::new();
        let mut current = start_beat;
        let end = start_beat + total_beats;
        while current < end {
            seq.process(true, current, TEMPO, BUFFER_FRAMES, TIME_SIG);
            for e in seq.events() {
                if let ProcessedEvent::Instant { sample_offset, message: MidiMessage::Note(n) } = *e {
                    let absolute = current + sample_offset as f64 * beats_per_sample();
                    out.push((absolute, n.channel, n.note, n.is_note_on));
                }
            }
            current += buffer_beats();
        }
        out
    }

    /// A single TimedNote: NoteOn at the onset beat, NoteOff at
    /// `onset + duration_beats`. End-to-end sanity check for the
    /// new path.
    #[test]
    fn single_timed_note_fires_on_then_off_at_duration() {
        let def = make_timed_notes_fugue(&[(1.0, 60, 2.0)], 16.0);
        let mut seq = sequencer_with_fugue(def);
        let events = collect_notes_over(&mut seq, 0.0, 6.0);

        let on = events.iter().find(|(_, _, n, on)| *on && *n == 60);
        let off = events.iter().find(|(_, _, n, on)| !*on && *n == 60);
        let (t_on, _, _, _) = on.expect("NoteOn missing");
        let (t_off, _, _, _) = off.expect("NoteOff missing");
        assert!((t_on - 1.0).abs() < buffer_beats(), "NoteOn should fire ≈ beat 1.0, got {:.3}", t_on);
        assert!((t_off - 3.0).abs() < buffer_beats(), "NoteOff should fire ≈ beat 3.0 (= 1 + 2), got {:.3}", t_off);
    }

    /// Same-pitch retrigger: TimedNote A plays at beat 0 with duration
    /// 4; TimedNote B starts at beat 2 on the same (channel, note).
    /// The audio thread's retrigger-eviction path inside
    /// `process_event` must emit A's NoteOff *strictly before* B's
    /// NoteOn (not at the same sample) so the synth never sees a
    /// simultaneous OFF/ON pair that it might collapse.
    #[test]
    fn same_pitch_retrigger_emits_note_off_strictly_before_next_note_on() {
        let def = make_timed_notes_fugue(
            &[(0.0, 60, 4.0), (2.0, 60, 2.0)],
            16.0,
        );
        let mut seq = sequencer_with_fugue(def);
        let events = collect_notes_over(&mut seq, 0.0, 5.0);

        // At beat ≈ 2, we expect exactly two events for note 60:
        // A's NoteOff and B's NoteOn. The OFF must come at a smaller
        // absolute beat than the ON.
        let around_beat_2: Vec<&(f64, u8, u8, bool)> = events
            .iter()
            .filter(|(t, _, n, _)| *n == 60 && (*t - 2.0).abs() < buffer_beats() * 2.0)
            .collect();
        let off = around_beat_2
            .iter()
            .find(|(_, _, _, on)| !*on)
            .expect("A's NoteOff at retrigger point missing");
        let new_on = around_beat_2
            .iter()
            .find(|(_, _, _, on)| *on)
            .expect("B's NoteOn at retrigger point missing");
        assert!(
            off.0 < new_on.0,
            "retrigger: NoteOff at {:.6} should come strictly before NoteOn at {:.6}",
            off.0,
            new_on.0
        );

        // Beat 4 is past A's nominal end, but A was truncated by the
        // retrigger — there shouldn't be a second NoteOff for A.
        // B's NoteOff fires at beat 4 (= 2 + 2).
        let off_count: usize = events
            .iter()
            .filter(|(_, _, n, on)| *n == 60 && !*on)
            .count();
        assert_eq!(
            off_count, 2,
            "expect 2 NoteOffs total: A's eviction at retrigger + B's scheduled end. Events: {:?}",
            events
        );
    }

    /// Zero-duration TimedNote is dropped entirely — no NoteOn
    /// emitted, no ring slot consumed. Checked indirectly: a sibling
    /// TimedNote at a later beat fires normally, and the count of
    /// NoteOn events equals the count of non-zero-duration notes.
    #[test]
    fn zero_duration_timed_note_is_dropped() {
        let def = make_timed_notes_fugue(
            &[
                (0.0, 60, 0.0),  // zero-dur, should be skipped
                (1.0, 62, 1.0),  // normal note
                (2.0, 64, -0.5), // negative-dur, also skipped
            ],
            16.0,
        );
        let mut seq = sequencer_with_fugue(def);
        let events = collect_notes_over(&mut seq, 0.0, 5.0);

        let on_count: usize = events.iter().filter(|(_, _, _, on)| *on).count();
        let off_count: usize = events.iter().filter(|(_, _, _, on)| !*on).count();
        assert_eq!(on_count, 1, "only note 62 should NoteOn. Events: {:?}", events);
        assert_eq!(off_count, 1, "only note 62 should NoteOff. Events: {:?}", events);
        assert!(events.iter().any(|(_, _, n, on)| *n == 62 && *on));
    }

    /// Polyphony — many simultaneous TimedNotes fire independent
    /// NoteOn/NoteOff pairs. Exercises the ring insert path across
    /// distinct (channel, note) keys.
    #[test]
    fn many_concurrent_timed_notes_each_get_their_own_off() {
        // 16 notes, all starting at beat 0 with different pitches and
        // staggered durations.
        let notes: Vec<(f64, u8, f64)> = (0..16)
            .map(|i| (0.0, 60 + i as u8, 1.0 + 0.1 * i as f64))
            .collect();
        let def = make_timed_notes_fugue(&notes, 16.0);
        let mut seq = sequencer_with_fugue(def);
        let events = collect_notes_over(&mut seq, 0.0, 5.0);

        let on_count: usize = events.iter().filter(|(_, _, _, on)| *on).count();
        let off_count: usize = events.iter().filter(|(_, _, _, on)| !*on).count();
        assert_eq!(on_count, 16, "16 concurrent TimedNotes should produce 16 NoteOns. Events: {:?}", events);
        assert_eq!(off_count, 16, "...and 16 matching NoteOffs");

        // Every pitch from 60..76 should see exactly one OFF at
        // approximately beat 1.0 + 0.1*i.
        for i in 0..16 {
            let pitch = 60 + i as u8;
            let expected_end = 1.0 + 0.1 * i as f64;
            let off = events
                .iter()
                .find(|(_, _, n, on)| *n == pitch && !*on)
                .expect(&format!("missing NoteOff for pitch {}", pitch));
            assert!(
                (off.0 - expected_end).abs() < buffer_beats(),
                "pitch {} NoteOff at {:.3}, expected ≈ {:.3}",
                pitch,
                off.0,
                expected_end
            );
        }
    }

    /// Ring-full voice stealing: exceeding `MAX_PENDING_NOTE_OFFS`
    /// concurrent TimedNotes must evict the oldest slot and emit its
    /// NoteOff immediately so the synth doesn't hold a stuck voice.
    /// We assert that no pitch is left without a NoteOff and the
    /// total NoteOn/NoteOff counts match.
    #[test]
    fn ring_full_voice_steals_oldest_slot_and_emits_its_note_off() {
        // Want exactly `MAX_PENDING_NOTE_OFFS + 4` notes so we
        // exercise stealing without a crazy event count. Each uses
        // a unique (channel, note) pair so retrigger-eviction isn't
        // triggered — this stresses the "ring full, genuinely new
        // note, evict oldest" path.
        //
        // Stagger onsets by a tiny amount (50 μs in beats) so the
        // "oldest" seq is unambiguous — the notes pushed first get
        // the smallest seq numbers.
        const OVER: usize = 4;
        let count = super::super::fugue::MAX_PENDING_NOTE_OFFS + OVER;
        let notes: Vec<(f64, u8, f64)> = (0..count)
            .map(|i| {
                let beat = (i as f64) * 1e-6; // distinct but within one buffer
                let pitch = (i % 128) as u8;
                // Long durations so the notes DON'T self-release before
                // we finish spawning them.
                (beat, pitch, 8.0)
            })
            .collect();
        let def = make_timed_notes_fugue(&notes, 16.0);
        let mut seq = sequencer_with_fugue(def);

        // Play through a window that exceeds every note's end — every
        // live voice must have a matching NoteOff by the time we stop.
        let events = collect_notes_over(&mut seq, 0.0, 10.0);

        // Invariant: per (channel, note), NoteOn count >= NoteOff count
        // at any snapshot, and at the end they converge. Notes that
        // share a pitch inside the same burst (unlikely here since we
        // enumerated distinct pitches) would retrigger-evict.
        let on_count: usize = events.iter().filter(|(_, _, _, on)| *on).count();
        let off_count: usize = events.iter().filter(|(_, _, _, on)| !*on).count();
        assert_eq!(
            on_count, off_count,
            "every NoteOn must eventually pair with a NoteOff after voice-stealing \
             settled. on_count = {}, off_count = {}. MAX_PENDING = {}, total notes = {}.",
            on_count, off_count, super::super::fugue::MAX_PENDING_NOTE_OFFS, count
        );
    }

    /// Cross-buffer scheduling: a TimedNote whose end falls in a later
    /// buffer than its NoteOn must still fire its NoteOff at the
    /// right absolute beat. The ring persists across buffer
    /// callbacks; this test locks that behavior in.
    #[test]
    fn timed_note_off_fires_in_later_buffer() {
        // 5-beat note — longer than any reasonable single buffer. The
        // NoteOn lands in the first buffer, the NoteOff must land
        // several buffers later.
        let def = make_timed_notes_fugue(&[(0.0, 60, 5.0)], 16.0);
        let mut seq = sequencer_with_fugue(def);
        let events = collect_notes_over(&mut seq, 0.0, 7.0);

        let (t_on, _, _, _) = events.iter().find(|(_, _, n, on)| *on && *n == 60).expect("NoteOn missing");
        let (t_off, _, _, _) = events.iter().find(|(_, _, n, on)| !*on && *n == 60).expect("NoteOff missing");
        assert!(t_off - t_on > 4.0, "NoteOff should land roughly 5 beats after NoteOn; got delta {:.3}", t_off - t_on);
        assert!((t_off - 5.0).abs() < buffer_beats(), "NoteOff should fire ≈ beat 5.0, got {:.3}", t_off);
    }

    /// Loop-boundary NoteOff: a TimedNote whose end is past
    /// `duration_beats` (i.e. crosses the loop boundary) — the ring
    /// carries the entry across `reset_for_loop` because end times
    /// are stored in absolute beats, and fires it at the right
    /// absolute moment.
    #[test]
    fn timed_note_whose_end_crosses_loop_boundary_still_fires_off() {
        // 4-beat fugue, note starts at beat 3.5 with duration 1.0 so
        // it ends at beat 4.5 — past the loop boundary (= dur).
        let def = make_timed_notes_fugue(&[(3.5, 60, 1.0)], 4.0);
        let mut seq = sequencer_with_fugue(def);
        let events = collect_notes_over(&mut seq, 0.0, 8.0);

        // We expect at least one NoteOn + one NoteOff for pitch 60.
        // Under phase-based iteration (default), the first iteration
        // plays the note at beat 3.5 and its NoteOff should land at
        // beat 4.5 — inside iteration 2 but still correctly emitted
        // from the ring.
        let on = events.iter().find(|(_, _, n, on)| *on && *n == 60).expect("NoteOn missing");
        let off = events.iter().find(|(_, _, n, on)| !*on && *n == 60).expect("NoteOff missing");
        assert!(
            (on.0 - 3.5).abs() < buffer_beats(),
            "NoteOn should land ≈ beat 3.5, got {:.3}",
            on.0
        );
        assert!(
            (off.0 - 4.5).abs() < buffer_beats(),
            "NoteOff (cross-boundary) should land ≈ beat 4.5, got {:.3}",
            off.0
        );
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::types::QuantizeMode;

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
