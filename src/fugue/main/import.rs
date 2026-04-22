//! MIDI file import — the inverse of [`super::export`].
//!
//! Takes SMF bytes (format 0 or format 1) and produces `FugueDefinition`s
//! that can be fed directly to `FugueBridge::queue`. Used by:
//!
//! - `POST /api/import/fugue` (frontend drop-zone flow)
//! - the `import_fugue` MCP tool (LLM hands back a DAW-edited clip)
//!
//! ## Two-stage pipeline
//!
//! Mirrors the export side's shape (merge → build → serialize):
//!
//! ```text
//! bytes
//!    ↓  ImportedMidi::parse(bytes, strict)  ← decode SMF into an event-stream
//!    ↓                                         intermediate, no fugue policy
//! ImportedMidi { tracks: [...] }
//!    ↓  .into_fugues(&opts)                  ← attach loop/quantize/cancel/tag
//!    ↓                                         and produce queueable definitions
//! Vec<FugueDefinition>
//! ```
//!
//! Callers that only want one step (inspect raw tracks without queuing, or
//! re-tag later with different options) use the two methods directly;
//! `smf_to_fugues` is the thin wrapper for the common full-pipeline case.
//!
//! ## Fidelity
//!
//! Round-trip is best-effort. Notes + CC survive exactly; per-note
//! expression (pitch bend, polyphonic aftertouch) is dropped because MIDI
//! 1.0 carries no per-note target and mapping channel-level bend back to a
//! specific note would be guesswork. DAW-edited clips may also come back
//! quantized/humanized in ways that diverge from the original fugue, which
//! is fine — "good enough" is explicitly the goal for this leg.

use std::collections::HashMap;

use midly::{MetaMessage, MidiMessage, Smf, Timing, TrackEventKind};

use super::super::types::{
    CancelMode, FugueDefinition, InterpolationMode, LoopMode, QuantizeMode, TimedFugueEvent,
    generate_fugue_id,
};

/// Caller-provided knobs controlling how the imported MIDI is turned into
/// fugues. All fields are independent of the file content — the file
/// provides notes + CC + timing; these control playback semantics.
#[derive(Debug, Clone)]
pub struct ImportOptions {
    /// Loop policy for every fugue produced from this import.
    pub loop_mode: LoopMode,
    /// Quantize-to-grid policy (when the fugue should start relative to
    /// the transport). Defaults to `Bar`, matching the LLM-facing default.
    pub quantize: QuantizeMode,
    /// Cancel policy applied to each imported fugue. Usually `None` —
    /// imports layer onto whatever's currently playing.
    pub cancel_mode: CancelMode,
    /// If set, prepended to every output fugue's tag. Useful for
    /// namespacing a batch (e.g. `"edited-"` → `"edited-bass"`,
    /// `"edited-melody"`) so the user can later cancel the whole batch
    /// without collecting ids individually.
    pub tag_prefix: Option<String>,
    /// If true, reject files containing events we can't represent
    /// (pitch bend, polyphonic aftertouch). Default false: silently drop
    /// those events and keep the rest. Strict mode is useful when the
    /// caller wants a hard guarantee that the re-imported fugue matches
    /// the file they dropped in.
    pub strict: bool,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            loop_mode: LoopMode::Forever,
            quantize: QuantizeMode::Bar,
            cancel_mode: CancelMode::None,
            tag_prefix: None,
            strict: false,
        }
    }
}

/// Owned intermediate — one entry per SMF track that carried playable MIDI
/// events, with beat-offsets already computed (PPQ baked in). Carries no
/// fugue-level policy (loop, quantize, cancel) — that's attached in
/// [`ImportedMidi::into_fugues`].
#[derive(Debug, Clone)]
pub struct ImportedMidi {
    /// Ticks per quarter note from the parsed file header. Preserved so
    /// callers can re-derive tick positions for debugging; events are
    /// already in beat units so most callers ignore this.
    pub ppq: u16,
    /// One entry per non-empty track. Conductor / pure-meta tracks are
    /// filtered out here — if `tracks` ends up empty, the file had
    /// nothing playable.
    pub tracks: Vec<ImportedTrack>,
}

/// One track's worth of parsed content. `name` is the SMF TrackName meta
/// if present (raw — no synthetic fallback). `duration_beats` comes from
/// the track's EndOfTrack meta when present, otherwise the max event tick
/// ceilinged to a beat boundary so loops land on a musical grid.
#[derive(Debug, Clone)]
pub struct ImportedTrack {
    pub name: Option<String>,
    pub events: Vec<TimedFugueEvent>,
    pub duration_beats: f64,
}

impl ImportedMidi {
    /// Decode SMF bytes into the intermediate form. Does NOT apply any
    /// fugue-level policy — callers then pass the result to
    /// [`Self::into_fugues`] to attach loop/quantize/tag/cancel.
    ///
    /// `strict` controls what happens on encountering events we can't
    /// represent in the fugue schema (pitch bend, polyphonic aftertouch):
    /// `true` → return an `Err`, `false` → drop the event and keep parsing.
    pub fn parse(bytes: &[u8], strict: bool) -> Result<Self, String> {
        let smf = Smf::parse(bytes).map_err(|e| format!("failed to parse MIDI file: {}", e))?;
        let ppq_u16 = match smf.header.timing {
            Timing::Metrical(n) => n.as_int(),
            Timing::Timecode(_, _) => {
                return Err(
                    "SMPTE-timed MIDI files are not supported (only PPQ/metrical)".into(),
                );
            }
        };
        if ppq_u16 == 0 {
            return Err("MIDI file declares zero PPQ".into());
        }
        let ppq = ppq_u16 as f64;

        let mut tracks: Vec<ImportedTrack> = Vec::new();
        for track in &smf.tracks {
            let parsed = parse_track(track, ppq, strict)?;
            if !parsed.events.is_empty() {
                tracks.push(parsed);
            }
        }
        Ok(ImportedMidi { ppq: ppq_u16, tracks })
    }

    /// Attach fugue-level policy (loop, quantize, cancel, tag prefix) and
    /// produce queueable `FugueDefinition`s — one per parsed track. Tracks
    /// without a `TrackName` meta get a synthetic `imported-N` tag, in
    /// first-seen order across `self.tracks`. `opts.strict` is ignored
    /// here (it's a parse-time concern).
    pub fn into_fugues(self, opts: &ImportOptions) -> Vec<FugueDefinition> {
        let mut out: Vec<FugueDefinition> = Vec::with_capacity(self.tracks.len());
        let mut synth_idx = 0usize;
        for t in self.tracks {
            let base_name = t.name.unwrap_or_else(|| {
                synth_idx += 1;
                format!("imported-{}", synth_idx)
            });
            let tag = match &opts.tag_prefix {
                Some(prefix) => Some(format!("{}{}", prefix, base_name)),
                None => Some(base_name),
            };
            out.push(FugueDefinition {
                id: generate_fugue_id(),
                tag,
                events: t.events,
                duration_beats: t.duration_beats,
                loop_mode: opts.loop_mode,
                quantize: opts.quantize,
                cancel_mode: opts.cancel_mode.clone(),
                cc_interpolation: InterpolationMode::Linear,
                start_mode: crate::fugue::StartMode::default(),
            });
        }
        out
    }
}

/// Full pipeline: parse + convert. Thin wrapper for callers that don't
/// need to inspect the parsed intermediate — typical for HTTP handlers
/// and MCP tools. Callers that want the intermediate (debugging, custom
/// policy application) should call [`ImportedMidi::parse`] +
/// [`ImportedMidi::into_fugues`] directly.
pub fn smf_to_fugues(bytes: &[u8], opts: &ImportOptions) -> Result<Vec<FugueDefinition>, String> {
    Ok(ImportedMidi::parse(bytes, opts.strict)?.into_fugues(opts))
}

fn parse_track(
    track: &[midly::TrackEvent<'_>],
    ppq: f64,
    strict: bool,
) -> Result<ImportedTrack, String> {
    let mut name: Option<String> = None;
    let mut events: Vec<TimedFugueEvent> = Vec::new();
    // Queue of unmatched NoteOns per (channel, note), in FIFO order. Each
    // entry is `(onset_beat, velocity)` so the matching NoteOff can emit
    // a fully-formed `FugueEvent::TimedNote` carrying the onset, velocity,
    // and duration in one event — no raw NoteOn / NoteOff pair ever
    // reaches the scheduler's event list from the import path.
    //
    // The queue (not a single slot) is load-bearing: rapid retriggers of
    // the same pitch are legal MIDI — pairs with the oldest unmatched on.
    let mut active: HashMap<(u8, u8), Vec<(f64, u8)>> = HashMap::new();
    let mut abs_ticks: u64 = 0;
    let mut max_tick: u64 = 0;
    let mut end_tick: Option<u64> = None;

    for event in track {
        abs_ticks = abs_ticks.saturating_add(event.delta.as_int() as u64);
        if abs_ticks > max_tick {
            max_tick = abs_ticks;
        }
        let beat = abs_ticks as f64 / ppq;

        match event.kind {
            TrackEventKind::Meta(MetaMessage::TrackName(bytes)) => {
                let s = String::from_utf8_lossy(bytes).trim().to_string();
                if !s.is_empty() {
                    name = Some(s);
                }
            }
            TrackEventKind::Meta(MetaMessage::EndOfTrack) => {
                end_tick = Some(abs_ticks);
            }
            TrackEventKind::Meta(_) => {
                // Tempo / TimeSignature / KeySignature etc. belong in the
                // conductor track; fugues don't own tempo.
            }
            TrackEventKind::Midi { channel, message } => {
                let channel = channel.as_int();
                match message {
                    MidiMessage::NoteOn { key, vel } => {
                        let note = key.as_int();
                        let velocity = vel.as_int();
                        if velocity == 0 {
                            // Running-status note-off convention (DAW-emitted
                            // files often use this — Bitwig, Logic, Ableton).
                            close_note(&mut active, &mut events, channel, note, beat);
                        } else {
                            // Just queue it — we emit the TimedNote later
                            // when the matching NoteOff arrives, so we
                            // know the duration.
                            active
                                .entry((channel, note))
                                .or_default()
                                .push((beat, velocity));
                        }
                    }
                    MidiMessage::NoteOff { key, .. } => {
                        close_note(&mut active, &mut events, channel, key.as_int(), beat);
                    }
                    MidiMessage::Controller { controller, value } => {
                        events.push(TimedFugueEvent::cc(
                            beat,
                            channel,
                            controller.as_int(),
                            value.as_int(),
                        ));
                    }
                    MidiMessage::PitchBend { .. } | MidiMessage::Aftertouch { .. } => {
                        if strict {
                            return Err(
                                "file contains per-note expression (pitch bend / aftertouch) which this importer cannot represent; disable strict mode to drop those events".into()
                            );
                        }
                    }
                    _ => {
                        // ProgramChange, ChannelAftertouch — not representable
                        // in the fugue schema today. Dropped silently.
                    }
                }
            }
            _ => {
                // SysEx, Escape — ignore.
            }
        }
    }

    // Any unmatched NoteOn gets capped at end-of-track (or max event tick
    // as fallback for files without an explicit EndOfTrack meta). Emitting
    // an explicit TimedNote — with a finite duration — means the scheduler
    // never sees a stuck note at loop boundaries.
    let close_tick = end_tick.unwrap_or(max_tick);
    let close_beat = close_tick as f64 / ppq;
    // Sort for determinism — HashMap iteration order is nondeterministic,
    // and we want round-trip tests to see a stable event stream.
    let mut dangling: Vec<((u8, u8), Vec<(f64, u8)>)> = active.drain().collect();
    dangling.sort_by_key(|(k, _)| *k);
    for ((channel, note), queue) in dangling {
        for (onset, velocity) in queue {
            let duration = (close_beat - onset).max(0.0);
            events.push(TimedFugueEvent::timed_note(
                onset, channel, note, velocity, duration,
            ));
        }
    }

    // Sort by onset beat — emit_notes-produced fugues already have this
    // invariant (events sorted by beat_offset). partial_cmp only fails on
    // NaN beats, which we never produce — finite PPQ × integer ticks.
    events.sort_by(|a, b| {
        a.beat_offset
            .partial_cmp(&b.beat_offset)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Duration: prefer the explicit end-of-track position (what the file's
    // author declared the clip length to be). Fall back to the last event's
    // tick ceilinged to a beat boundary so looping lands on a musical grid.
    let duration_beats = if let Some(t) = end_tick {
        t as f64 / ppq
    } else {
        (max_tick as f64 / ppq).max(1.0).ceil()
    };

    Ok(ImportedTrack {
        name,
        events,
        duration_beats,
    })
}

fn close_note(
    active: &mut HashMap<(u8, u8), Vec<(f64, u8)>>,
    events: &mut Vec<TimedFugueEvent>,
    channel: u8,
    note: u8,
    off_beat: f64,
) {
    if let Some(bucket) = active.get_mut(&(channel, note)) {
        // Pop the most recently started NoteOn that hasn't been closed
        // — LIFO gives typical DAWs the behavior users expect when
        // legitimate same-pitch overlap exists (outer note ends last).
        if let Some((onset_beat, velocity)) = bucket.pop() {
            let duration = (off_beat - onset_beat).max(0.0);
            events.push(TimedFugueEvent::timed_note(
                onset_beat,
                channel,
                note,
                velocity,
                duration,
            ));
            if bucket.is_empty() {
                active.remove(&(channel, note));
            }
        }
    }
    // Orphan NoteOffs (no matching NoteOn) are legal-but-weird MIDI;
    // they refer to no real note, so drop silently rather than
    // inventing a phantom event.
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fugue::export::fugues_to_smf;
    use crate::fugue::FugueEvent;

    /// Count logical notes in the imported definition. Post Feature 28
    /// the importer emits `TimedNote` rather than paired NoteOn/NoteOff
    /// events, so one TimedNote counts as both a NoteOn *and* a
    /// NoteOff for assertion purposes — callers of this helper
    /// historically checked `on == off` to verify pairing.
    fn count_notes(def: &FugueDefinition) -> (usize, usize) {
        let mut on = 0;
        let mut off = 0;
        for ev in &def.events {
            match ev.event {
                FugueEvent::NoteOn { .. } => on += 1,
                FugueEvent::NoteOff { .. } => off += 1,
                FugueEvent::TimedNote { .. } => {
                    on += 1;
                    off += 1;
                }
                _ => {}
            }
        }
        (on, off)
    }

    #[test]
    fn rejects_malformed_bytes() {
        let result = smf_to_fugues(b"not a MIDI file", &ImportOptions::default());
        assert!(result.is_err());
    }

    #[test]
    fn empty_track_produces_no_fugue() {
        // Build a minimal SMF with just a conductor track — should round-trip
        // into zero fugues because there's nothing to play.
        let bytes = fugues_to_smf(&[], 120.0);
        let fugues = smf_to_fugues(&bytes, &ImportOptions::default()).unwrap();
        assert!(fugues.is_empty());
    }

    #[test]
    fn round_trip_single_tagged_fugue() {
        // Export one fugue, import, confirm the notes come back with the
        // same pitches, channels, and relative ordering. Beat positions are
        // allowed to round (PPQ is discrete).
        let original = FugueDefinition::new(
            vec![
                TimedFugueEvent::note_on(0.0, 0, 60, 100),
                TimedFugueEvent::note_off(1.0, 0, 60),
                TimedFugueEvent::note_on(1.0, 0, 64, 90),
                TimedFugueEvent::note_off(2.0, 0, 64),
            ],
            4.0,
        )
        .with_tag("melody");

        let bytes = fugues_to_smf(&[original.clone()], 120.0);
        let imported = smf_to_fugues(&bytes, &ImportOptions::default()).unwrap();

        assert_eq!(imported.len(), 1);
        let imp = &imported[0];
        assert_eq!(imp.tag.as_deref(), Some("melody"));
        let (on, off) = count_notes(imp);
        assert_eq!(on, 2);
        assert_eq!(off, 2);
    }

    #[test]
    fn round_trip_multi_track() {
        let bass = FugueDefinition::new(
            vec![
                TimedFugueEvent::note_on(0.0, 0, 36, 100),
                TimedFugueEvent::note_off(1.0, 0, 36),
            ],
            4.0,
        )
        .with_tag("bass");
        let lead = FugueDefinition::new(
            vec![
                TimedFugueEvent::note_on(0.0, 1, 60, 100),
                TimedFugueEvent::note_off(0.5, 1, 60),
            ],
            4.0,
        )
        .with_tag("lead");

        let bytes = fugues_to_smf(&[bass, lead], 120.0);
        let imported = smf_to_fugues(&bytes, &ImportOptions::default()).unwrap();
        assert_eq!(imported.len(), 2);
        let tags: Vec<_> = imported.iter().filter_map(|d| d.tag.clone()).collect();
        assert!(tags.iter().any(|t| t == "bass"));
        assert!(tags.iter().any(|t| t == "lead"));
    }

    #[test]
    fn tag_prefix_namespaces_imports() {
        let a = FugueDefinition::new(
            vec![
                TimedFugueEvent::note_on(0.0, 0, 60, 100),
                TimedFugueEvent::note_off(1.0, 0, 60),
            ],
            4.0,
        )
        .with_tag("melody");

        let bytes = fugues_to_smf(&[a], 120.0);
        let opts = ImportOptions {
            tag_prefix: Some("edited-".into()),
            ..Default::default()
        };
        let imported = smf_to_fugues(&bytes, &opts).unwrap();
        assert_eq!(imported[0].tag.as_deref(), Some("edited-melody"));
    }

    #[test]
    fn synthetic_tag_when_track_name_missing() {
        // Hand-craft an SMF with one track that has a note but no TrackName.
        use midly::{Format, Header, Track, TrackEvent, num::{u15, u28, u4, u7}};
        let mut track: Track = Vec::new();
        track.push(TrackEvent {
            delta: u28::new(0),
            kind: TrackEventKind::Midi {
                channel: u4::new(0),
                message: MidiMessage::NoteOn { key: u7::new(60), vel: u7::new(100) },
            },
        });
        track.push(TrackEvent {
            delta: u28::new(480),
            kind: TrackEventKind::Midi {
                channel: u4::new(0),
                message: MidiMessage::NoteOff { key: u7::new(60), vel: u7::new(0) },
            },
        });
        track.push(TrackEvent {
            delta: u28::new(0),
            kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
        });

        let smf = Smf {
            header: Header { format: Format::SingleTrack, timing: Timing::Metrical(u15::new(480)) },
            tracks: vec![track],
        };
        let mut bytes = Vec::new();
        smf.write_std(&mut bytes).unwrap();

        let imported = smf_to_fugues(&bytes, &ImportOptions::default()).unwrap();
        assert_eq!(imported.len(), 1);
        assert_eq!(imported[0].tag.as_deref(), Some("imported-1"));
    }

    #[test]
    fn note_on_velocity_zero_treated_as_off() {
        // Many DAWs emit note-off as NoteOn vel=0 (running status). The
        // importer must pair that with the prior on and close the note.
        use midly::{Format, Header, Track, TrackEvent, num::{u15, u28, u4, u7}};
        let mut track: Track = Vec::new();
        track.push(TrackEvent {
            delta: u28::new(0),
            kind: TrackEventKind::Midi {
                channel: u4::new(0),
                message: MidiMessage::NoteOn { key: u7::new(60), vel: u7::new(100) },
            },
        });
        track.push(TrackEvent {
            delta: u28::new(480),
            kind: TrackEventKind::Midi {
                channel: u4::new(0),
                message: MidiMessage::NoteOn { key: u7::new(60), vel: u7::new(0) },
            },
        });
        track.push(TrackEvent {
            delta: u28::new(0),
            kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
        });

        let smf = Smf {
            header: Header { format: Format::SingleTrack, timing: Timing::Metrical(u15::new(480)) },
            tracks: vec![track],
        };
        let mut bytes = Vec::new();
        smf.write_std(&mut bytes).unwrap();

        let imported = smf_to_fugues(&bytes, &ImportOptions::default()).unwrap();
        let (on, off) = count_notes(&imported[0]);
        assert_eq!(on, 1);
        assert_eq!(off, 1);
    }

    #[test]
    fn dangling_note_on_closes_at_eot() {
        // Note-on without matching off should still produce a balanced
        // on/off pair, closed at end-of-track, so the fugue doesn't leave
        // stuck notes in the scheduler.
        use midly::{Format, Header, Track, TrackEvent, num::{u15, u28, u4, u7}};
        let mut track: Track = Vec::new();
        track.push(TrackEvent {
            delta: u28::new(0),
            kind: TrackEventKind::Midi {
                channel: u4::new(0),
                message: MidiMessage::NoteOn { key: u7::new(60), vel: u7::new(100) },
            },
        });
        track.push(TrackEvent {
            delta: u28::new(960),
            kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
        });

        let smf = Smf {
            header: Header { format: Format::SingleTrack, timing: Timing::Metrical(u15::new(480)) },
            tracks: vec![track],
        };
        let mut bytes = Vec::new();
        smf.write_std(&mut bytes).unwrap();

        let imported = smf_to_fugues(&bytes, &ImportOptions::default()).unwrap();
        let (on, off) = count_notes(&imported[0]);
        assert_eq!(on, 1);
        assert_eq!(off, 1);
        // The emitted TimedNote should close at end-of-track (960/480 = 2.0)
        // — i.e. duration = 2.0 beats, since the NoteOn was at beat 0.
        let (onset, duration) = imported[0].events.iter().find_map(|e| match e.event {
            FugueEvent::TimedNote { duration_beats, .. } => Some((e.beat_offset, duration_beats)),
            _ => None,
        }).expect("dangling NoteOn should have been closed as a TimedNote");
        assert!((onset - 0.0).abs() < 1e-6);
        assert!((duration - 2.0).abs() < 1e-6);
    }

    #[test]
    fn cc_passthrough() {
        use midly::{Format, Header, Track, TrackEvent, num::{u15, u28, u4, u7}};
        let mut track: Track = Vec::new();
        for (delta, val) in [(0u32, 0u8), (120, 64), (120, 127)] {
            track.push(TrackEvent {
                delta: u28::new(delta),
                kind: TrackEventKind::Midi {
                    channel: u4::new(0),
                    message: MidiMessage::Controller {
                        controller: u7::new(74),
                        value: u7::new(val),
                    },
                },
            });
        }
        track.push(TrackEvent {
            delta: u28::new(0),
            kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
        });

        let smf = Smf {
            header: Header { format: Format::SingleTrack, timing: Timing::Metrical(u15::new(480)) },
            tracks: vec![track],
        };
        let mut bytes = Vec::new();
        smf.write_std(&mut bytes).unwrap();

        let imported = smf_to_fugues(&bytes, &ImportOptions::default()).unwrap();
        let cc_events: Vec<_> = imported[0].events.iter().filter_map(|e| match e.event {
            FugueEvent::Cc { cc, value, .. } => Some((cc, value)),
            _ => None,
        }).collect();
        assert_eq!(cc_events, vec![(74, 0), (74, 64), (74, 127)]);
    }

    #[test]
    fn strict_mode_rejects_pitch_bend() {
        use midly::{Format, Header, PitchBend, Track, TrackEvent, num::{u14, u15, u28, u4}};
        let mut track: Track = Vec::new();
        track.push(TrackEvent {
            delta: u28::new(0),
            kind: TrackEventKind::Midi {
                channel: u4::new(0),
                message: MidiMessage::PitchBend { bend: PitchBend(u14::new(8192)) },
            },
        });
        track.push(TrackEvent {
            delta: u28::new(0),
            kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
        });

        let smf = Smf {
            header: Header { format: Format::SingleTrack, timing: Timing::Metrical(u15::new(480)) },
            tracks: vec![track],
        };
        let mut bytes = Vec::new();
        smf.write_std(&mut bytes).unwrap();

        let opts = ImportOptions { strict: true, ..Default::default() };
        assert!(smf_to_fugues(&bytes, &opts).is_err());

        // Non-strict: the pitch bend is dropped and the track becomes empty,
        // so no fugues are produced.
        let imported = smf_to_fugues(&bytes, &ImportOptions::default()).unwrap();
        assert!(imported.is_empty());
    }

    #[test]
    fn parse_produces_intermediate_without_policy() {
        // The parse stage is policy-free: tracks keep their raw names,
        // and no loop_mode / quantize / tag_prefix have been applied yet.
        // Callers can inspect and/or repeat into_fugues() with different
        // options against one parse result.
        let a = FugueDefinition::new(
            vec![
                TimedFugueEvent::note_on(0.0, 0, 60, 100),
                TimedFugueEvent::note_off(1.0, 0, 60),
            ],
            4.0,
        )
        .with_tag("bass");
        let bytes = fugues_to_smf(&[a], 120.0);

        let parsed = ImportedMidi::parse(&bytes, false).unwrap();
        assert_eq!(parsed.ppq, 480);
        assert_eq!(parsed.tracks.len(), 1);
        assert_eq!(parsed.tracks[0].name.as_deref(), Some("bass"));

        // Re-convert the same intermediate twice with different policies.
        let opts_a = ImportOptions { tag_prefix: Some("a-".into()), ..Default::default() };
        let fugues_a = parsed.clone().into_fugues(&opts_a);
        assert_eq!(fugues_a[0].tag.as_deref(), Some("a-bass"));

        let opts_b = ImportOptions { loop_mode: LoopMode::Once, ..Default::default() };
        let fugues_b = parsed.into_fugues(&opts_b);
        assert_eq!(fugues_b[0].loop_mode, LoopMode::Once);
        assert_eq!(fugues_b[0].tag.as_deref(), Some("bass"));
    }

    #[test]
    fn imported_options_carry_through_to_fugue() {
        let a = FugueDefinition::new(
            vec![
                TimedFugueEvent::note_on(0.0, 0, 60, 100),
                TimedFugueEvent::note_off(1.0, 0, 60),
            ],
            4.0,
        )
        .with_tag("x");
        let bytes = fugues_to_smf(&[a], 120.0);

        let opts = ImportOptions {
            loop_mode: LoopMode::Once,
            quantize: QuantizeMode::Immediate,
            cancel_mode: CancelMode::CancelAll,
            tag_prefix: None,
            strict: false,
        };
        let imported = smf_to_fugues(&bytes, &opts).unwrap();
        assert_eq!(imported[0].loop_mode, LoopMode::Once);
        assert_eq!(imported[0].quantize, QuantizeMode::Immediate);
        assert_eq!(imported[0].cancel_mode, CancelMode::CancelAll);
    }

    /// End-to-end round-trip for the TimedNote representation: build a
    /// FugueDefinition containing TimedNote events (the shape
    /// `emit_notes` produces), export to SMF, re-import, and verify
    /// every LLM-facing note field survives unchanged.
    ///
    /// This locks in the key invariant Feature 28 targets: notes stay
    /// notes across the SMF boundary — onset, pitch, velocity, and
    /// duration round-trip faithfully, even though MIDI 1.0 itself
    /// has no "note with duration" event type and the wire format
    /// splits them into NoteOn + NoteOff pairs. Import pairs the
    /// SMF events back into a single TimedNote per note so the
    /// round-trip doesn't leave raw NoteOn/NoteOff pairs in the
    /// event list.
    #[test]
    fn round_trip_timed_notes_through_smf_preserves_note_fields() {
        use crate::fugue::{FugueDefinition, TimedFugueEvent};

        // Three notes, distinct onsets / pitches / velocities / durations
        // so any round-trip drift shows up as a field mismatch.
        let original = FugueDefinition::new(
            vec![
                TimedFugueEvent::timed_note(0.0, 0, 60, 100, 0.5),
                TimedFugueEvent::timed_note(1.0, 0, 64, 80, 1.5),
                TimedFugueEvent::timed_note(2.5, 0, 67, 120, 1.0),
            ],
            4.0,
        )
        .with_tag("roundtrip");

        let bytes = fugues_to_smf(&[original.clone()], 120.0);
        let imported = smf_to_fugues(&bytes, &ImportOptions::default()).unwrap();
        assert_eq!(imported.len(), 1, "single-track round-trip");
        let got = &imported[0];
        assert_eq!(got.tag.as_deref(), Some("roundtrip"));

        // Every event in the re-imported fugue should be a TimedNote —
        // raw NoteOn/NoteOff pairs should NOT appear. The import path
        // pairs them back into TimedNotes during `close_note`.
        let timed_notes: Vec<(f64, u8, u8, u8, f64)> = got
            .events
            .iter()
            .filter_map(|e| match e.event {
                FugueEvent::TimedNote { channel, note, velocity, duration_beats } => {
                    Some((e.beat_offset, channel, note, velocity, duration_beats))
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            timed_notes.len(),
            3,
            "expected 3 TimedNotes, got events: {:?}",
            got.events
        );
        // No leftover raw NoteOn / NoteOff events.
        let raw_note_events = got
            .events
            .iter()
            .filter(|e| matches!(
                e.event,
                FugueEvent::NoteOn { .. } | FugueEvent::NoteOff { .. }
            ))
            .count();
        assert_eq!(
            raw_note_events, 0,
            "round-tripped fugue should contain zero raw NoteOn/NoteOff — import pairs them back into TimedNotes. events: {:?}",
            got.events
        );

        // Compare each round-tripped note against its original. Order
        // should match because both sides are sorted by onset beat.
        let expected = [
            (0.0, 0u8, 60u8, 100u8, 0.5),
            (1.0, 0, 64, 80, 1.5),
            (2.5, 0, 67, 120, 1.0),
        ];
        for (i, (got_beat, got_ch, got_note, got_vel, got_dur)) in timed_notes.iter().enumerate() {
            let (exp_beat, exp_ch, exp_note, exp_vel, exp_dur) = expected[i];
            assert!((got_beat - exp_beat).abs() < 1e-6, "note {}: beat {:.6} vs {:.6}", i, got_beat, exp_beat);
            assert_eq!(*got_ch, exp_ch, "note {}: channel", i);
            assert_eq!(*got_note, exp_note, "note {}: pitch", i);
            assert_eq!(*got_vel, exp_vel, "note {}: velocity", i);
            assert!((got_dur - exp_dur).abs() < 1e-6, "note {}: duration {:.6} vs {:.6}", i, got_dur, exp_dur);
        }
    }

    /// Same-pitch retrigger survives the round-trip: two TimedNotes on
    /// the same (channel, pitch) at different onsets stay as two
    /// distinct TimedNotes — SMF writes them as NoteOn → NoteOff →
    /// NoteOn → NoteOff, and the importer pairs them back correctly
    /// (LIFO per-pitch bucket, so the inner pair is matched first).
    #[test]
    fn round_trip_same_pitch_retrigger_pairs_correctly() {
        use crate::fugue::{FugueDefinition, TimedFugueEvent};

        let original = FugueDefinition::new(
            vec![
                TimedFugueEvent::timed_note(0.0, 0, 60, 100, 0.5),
                TimedFugueEvent::timed_note(1.0, 0, 60, 80, 0.5), // same pitch, retrigger
            ],
            4.0,
        );

        let bytes = fugues_to_smf(&[original], 120.0);
        let imported = smf_to_fugues(&bytes, &ImportOptions::default()).unwrap();
        let got = &imported[0];

        let mut timed_notes: Vec<(f64, u8, f64)> = got
            .events
            .iter()
            .filter_map(|e| match e.event {
                FugueEvent::TimedNote { velocity, duration_beats, .. } => {
                    Some((e.beat_offset, velocity, duration_beats))
                }
                _ => None,
            })
            .collect();
        timed_notes.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

        assert_eq!(timed_notes.len(), 2);
        assert!((timed_notes[0].0 - 0.0).abs() < 1e-6);
        assert_eq!(timed_notes[0].1, 100);
        assert!((timed_notes[0].2 - 0.5).abs() < 1e-6);
        assert!((timed_notes[1].0 - 1.0).abs() < 1e-6);
        assert_eq!(timed_notes[1].1, 80);
        assert!((timed_notes[1].2 - 0.5).abs() < 1e-6);
    }
}
