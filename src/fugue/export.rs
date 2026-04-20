//! MIDI file export for fugues
//!
//! Converts FugueDefinition to Standard MIDI File (SMF) format.

use midly::{
    Format, Header, MetaMessage, MidiMessage, Smf, Track, TrackEvent, TrackEventKind,
    num::{u14, u15, u24, u28, u4, u7},
};
use std::collections::HashMap;

use super::types::{FugueDefinition, FugueEvent, InterpolationMode, TimedFugueEvent};

/// Pulses per quarter note (standard MIDI resolution)
const PPQ: u16 = 480;

/// Resolution for CC interpolation (1/32 note = PPQ/8 ticks)
const CC_INTERP_RESOLUTION_TICKS: u32 = (PPQ as u32) / 8;

/// Convert a FugueDefinition to a Standard MIDI File
///
/// # Arguments
/// * `definition` - The fugue to export
/// * `tempo_bpm` - Tempo in beats per minute
///
/// # Returns
/// Raw bytes of the MIDI file
pub fn fugue_to_smf(definition: &FugueDefinition, tempo_bpm: f64) -> Vec<u8> {
    let mut track_events: Vec<(u32, TrackEventKind<'static>)> = Vec::new();

    // Add tempo meta event at tick 0
    let tempo_microseconds = (60_000_000.0 / tempo_bpm) as u32;
    track_events.push((0, TrackEventKind::Meta(MetaMessage::Tempo(u24::new(tempo_microseconds)))));

    // Collect all events with absolute tick positions
    let events = if definition.cc_interpolation == InterpolationMode::Linear {
        interpolate_cc_events(&definition.events, definition.duration_beats)
    } else {
        definition.events.clone()
    };

    for timed_event in &events {
        let tick = beat_to_tick(timed_event.beat_offset);
        let kind = fugue_event_to_midi(&timed_event.event);
        if let Some(k) = kind {
            track_events.push((tick, k));
        }
    }

    // Sort by tick position
    track_events.sort_by_key(|(tick, _)| *tick);

    // Convert absolute ticks to delta times
    let mut track: Track<'static> = Vec::new();
    let mut last_tick = 0u32;

    for (tick, kind) in track_events {
        let delta = tick.saturating_sub(last_tick);
        track.push(TrackEvent {
            delta: u28::new(delta),
            kind,
        });
        last_tick = tick;
    }

    // Add end of track
    track.push(TrackEvent {
        delta: u28::new(0),
        kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
    });

    // Build SMF
    let smf = Smf {
        header: Header {
            format: Format::SingleTrack,
            timing: midly::Timing::Metrical(u15::new(PPQ)),
        },
        tracks: vec![track],
    };

    // Write to bytes
    let mut buffer = Vec::new();
    smf.write_std(&mut buffer).expect("Failed to write MIDI file");
    buffer
}

/// Convert beat offset to MIDI tick
fn beat_to_tick(beat: f64) -> u32 {
    (beat * PPQ as f64).round() as u32
}

/// Convert a FugueEvent to a MIDI TrackEventKind
fn fugue_event_to_midi(event: &FugueEvent) -> Option<TrackEventKind<'static>> {
    match event {
        FugueEvent::NoteOn { channel, note, velocity } => {
            Some(TrackEventKind::Midi {
                channel: u4::new(*channel & 0x0F),
                message: MidiMessage::NoteOn {
                    key: u7::new(*note & 0x7F),
                    vel: u7::new(*velocity & 0x7F),
                },
            })
        }
        FugueEvent::NoteOff { channel, note } => {
            Some(TrackEventKind::Midi {
                channel: u4::new(*channel & 0x0F),
                message: MidiMessage::NoteOff {
                    key: u7::new(*note & 0x7F),
                    vel: u7::new(0),
                },
            })
        }
        FugueEvent::Cc { channel, cc, value, .. } => {
            Some(TrackEventKind::Midi {
                channel: u4::new(*channel & 0x0F),
                message: MidiMessage::Controller {
                    controller: u7::new(*cc & 0x7F),
                    value: u7::new(*value & 0x7F),
                },
            })
        }
        FugueEvent::PerNotePitchBend { channel, semitones, .. } => {
            // Convert semitones to pitch bend value (assuming ±48 semitone range)
            // Center is 8192, range is 0-16383
            let bend = (8192.0 + (semitones / 48.0) * 8192.0).clamp(0.0, 16383.0) as u16;
            Some(TrackEventKind::Midi {
                channel: u4::new(*channel & 0x0F),
                message: MidiMessage::PitchBend {
                    bend: midly::PitchBend(u14::new(bend)),
                },
            })
        }
        FugueEvent::PerNotePressure { channel, note, pressure } => {
            // Convert to polyphonic aftertouch (per-note pressure)
            let value = (pressure * 127.0).clamp(0.0, 127.0) as u8;
            Some(TrackEventKind::Midi {
                channel: u4::new(*channel & 0x0F),
                message: MidiMessage::Aftertouch {
                    key: u7::new(*note & 0x7F),
                    vel: u7::new(value),
                },
            })
        }
    }
}

/// Interpolate CC events for smooth automation export
///
/// For Linear interpolation mode, generates intermediate CC values between
/// consecutive CC events on the same channel/CC number.
fn interpolate_cc_events(events: &[TimedFugueEvent], _duration_beats: f64) -> Vec<TimedFugueEvent> {
    let mut result: Vec<TimedFugueEvent> = Vec::new();

    // Group CC events by (channel, cc) for interpolation
    let mut cc_events: HashMap<(u8, u8), Vec<(f64, u8)>> = HashMap::new();
    let mut non_cc_events: Vec<TimedFugueEvent> = Vec::new();

    for event in events {
        match &event.event {
            FugueEvent::Cc { channel, cc, value, .. } => {
                cc_events
                    .entry((*channel, *cc))
                    .or_default()
                    .push((event.beat_offset, *value));
            }
            _ => {
                non_cc_events.push(event.clone());
            }
        }
    }

    // Add non-CC events as-is
    result.extend(non_cc_events);

    // Interpolate each CC lane
    let resolution_beats = CC_INTERP_RESOLUTION_TICKS as f64 / PPQ as f64;

    for ((channel, cc), mut points) in cc_events {
        // Sort by beat offset
        points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

        if points.is_empty() {
            continue;
        }

        // Always include first point
        result.push(TimedFugueEvent::cc(points[0].0, channel, cc, points[0].1));

        // Interpolate between consecutive points
        for window in points.windows(2) {
            let (start_beat, start_val) = window[0];
            let (end_beat, end_val) = window[1];

            if start_val == end_val {
                // No interpolation needed, just add end point
                result.push(TimedFugueEvent::cc(end_beat, channel, cc, end_val));
                continue;
            }

            // Generate intermediate points
            let mut current_beat = start_beat + resolution_beats;
            while current_beat < end_beat {
                let t = (current_beat - start_beat) / (end_beat - start_beat);
                let value = lerp(start_val as f64, end_val as f64, t) as u8;
                result.push(TimedFugueEvent::cc(current_beat, channel, cc, value));
                current_beat += resolution_beats;
            }

            // Add end point
            result.push(TimedFugueEvent::cc(end_beat, channel, cc, end_val));
        }
    }

    // Sort all events by beat offset
    result.sort_by(|a, b| a.beat_offset.partial_cmp(&b.beat_offset).unwrap());

    result
}

/// Linear interpolation
fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

// =============================================================================
// Multi-fugue pipeline (Phase 15a)
//
// Three decoupled stages, each independently useful:
//
//   FugueDefinition[]
//        ↓  merge_fugues_by_tag()       ← groups by tag, produces one
//        ↓                                 FugueDefinition per group
//   FugueDefinition[]  (one per group)
//        ↓  ExportedMidi::from_merged()  ← names untagged groups for the
//        ↓                                 DAW's arranger view
//   ExportedMidi
//        ↓  .to_smf() / .to_bytes()      ← midly serialization
//   bytes
//
// The merge step is exposed separately because "give me one combined
// FugueDefinition for these N fugues" is a useful operation in its own
// right — composing a new layered fugue from existing parts, debugging,
// potential future MCP tool.
// =============================================================================

/// Merge fugues that share a tag into one `FugueDefinition` per tag.
/// Untagged fugues pass through as-is (one output entry each, tag stays
/// `None`). Order is stable: the first-seen tag (and untagged fugues in
/// input order) determines output order.
///
/// Merged fields:
/// - **events**: union of all inputs in the group, sorted by `beat_offset`
/// - **duration_beats**: max of input durations — the merged fugue needs to
///   be at least as long as its longest contributor
/// - **loop_mode**, **quantize**, **cancel_mode**, **cc_interpolation**: inherited
///   from the first fugue of the group. Groups usually have one fugue, so
///   this is a deterministic pick that matches the typical case
/// - **tag**: the shared tag for the group
/// - **id**: freshly generated so the merged fugue has a distinct identity
pub fn merge_fugues_by_tag(defs: &[FugueDefinition]) -> Vec<FugueDefinition> {
    let mut out: Vec<FugueDefinition> = Vec::new();
    let mut tag_to_index: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    for def in defs {
        match &def.tag {
            Some(tag) => {
                if let Some(&idx) = tag_to_index.get(tag) {
                    // Merge into the existing group entry.
                    let merged = &mut out[idx];
                    merged.events.extend(def.events.iter().cloned());
                    merged.duration_beats = merged.duration_beats.max(def.duration_beats);
                } else {
                    // First time we've seen this tag — clone the fugue so
                    // we don't mutate the caller's data, then give it a
                    // fresh id (the merged fugue is a new identity).
                    let mut base = def.clone();
                    base.id = crate::fugue::types::generate_fugue_id();
                    tag_to_index.insert(tag.clone(), out.len());
                    out.push(base);
                }
            }
            None => {
                // Untagged: pass through in place. Each untagged fugue is
                // its own group — we don't know if the user meant two
                // untagged queues to mean "the same part."
                out.push(def.clone());
            }
        }
    }

    // Sort events in every merged group. Passes-through untagged entries
    // already have sorted events, but a sort is O(n log n) and cheap here.
    for def in &mut out {
        def.events.sort_by(|a, b| {
            a.beat_offset
                .partial_cmp(&b.beat_offset)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    out
}

/// Owned intermediate representation of a multi-fugue MIDI export.
///
/// Built from `FugueDefinition[]` (already merged by tag) — each input
/// becomes one named track. Callers that only need the bytes (HTTP
/// handlers, drag-out) use `to_bytes()`; callers that want to inspect
/// the structure can walk `tracks[..]` directly.
#[derive(Debug, Clone)]
pub struct ExportedMidi {
    pub tempo_bpm: f64,
    pub tracks: Vec<ExportedTrack>,
}

/// One track's worth of exported content — a name (surfaced as the MIDI
/// TrackName meta), plus the event stream in beat-offset units.
#[derive(Debug, Clone)]
pub struct ExportedTrack {
    pub name: String,
    pub events: Vec<TimedFugueEvent>,
}

impl ExportedMidi {
    /// Build from a pre-merged list of fugues (one per intended track).
    /// Untagged fugues get synthetic names (`fugue-1`, …) so the DAW's
    /// track list is always labelled. CC lanes with `cc_interpolation: Linear`
    /// get densified for smooth-looking ramps in the piano roll.
    ///
    /// Typical pipeline: `merge_fugues_by_tag` → this.
    pub fn from_merged(merged: &[FugueDefinition], tempo_bpm: f64) -> Self {
        let mut tracks: Vec<ExportedTrack> = Vec::with_capacity(merged.len());
        let mut untagged_idx = 0;

        for def in merged {
            let name = match &def.tag {
                Some(t) => t.clone(),
                None => {
                    untagged_idx += 1;
                    format!("fugue-{}", untagged_idx)
                }
            };

            let events = if def.cc_interpolation == InterpolationMode::Linear {
                interpolate_cc_events(&def.events, def.duration_beats)
            } else {
                def.events.clone()
            };

            tracks.push(ExportedTrack { name, events });
        }

        ExportedMidi { tempo_bpm, tracks }
    }

    /// Convenience: merge + build in one step. Same as
    /// `ExportedMidi::from_merged(&merge_fugues_by_tag(defs), tempo_bpm)`.
    pub fn from_fugues(definitions: &[FugueDefinition], tempo_bpm: f64) -> Self {
        let merged = merge_fugues_by_tag(definitions);
        Self::from_merged(&merged, tempo_bpm)
    }

    /// Build the midly `Smf` structure. Borrows self — track-name bytes
    /// live in `self.tracks[i].name`. Caller keeps `self` alive until the
    /// returned Smf is serialized or dropped.
    pub fn to_smf(&self) -> Smf<'_> {
        let mut smf_tracks: Vec<Track<'_>> = Vec::with_capacity(self.tracks.len() + 1);
        smf_tracks.push(build_conductor_track(self.tempo_bpm));
        for track in &self.tracks {
            smf_tracks.push(build_named_track(&track.name, &track.events));
        }

        Smf {
            header: Header {
                // Parallel = SMF format 1: multi-track with shared timing.
                format: Format::Parallel,
                timing: midly::Timing::Metrical(u15::new(PPQ)),
            },
            tracks: smf_tracks,
        }
    }

    /// Convenience: serialize through midly into a byte buffer.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buffer = Vec::new();
        self.to_smf()
            .write_std(&mut buffer)
            .expect("midly::Smf::write_std failed — only fails on IO error, and our sink is an in-memory Vec<u8>");
        buffer
    }
}

/// Public entry point — runs the full merge → export → serialize pipeline.
/// Kept as a thin wrapper so HTTP handlers don't have to name the
/// intermediate types. Callers that want to inspect the structure should
/// use `ExportedMidi::from_fugues(...).to_smf()` directly, and callers
/// that want the merged fugues themselves should use
/// `merge_fugues_by_tag(...)`.
pub fn fugues_to_smf(definitions: &[FugueDefinition], tempo_bpm: f64) -> Vec<u8> {
    ExportedMidi::from_fugues(definitions, tempo_bpm).to_bytes()
}

/// Conductor track: tempo meta at tick 0 + EndOfTrack. Separated from the
/// musical tracks so DAWs reading tempo don't have to parse note data.
fn build_conductor_track<'a>(tempo_bpm: f64) -> Track<'a> {
    let tempo_us = (60_000_000.0 / tempo_bpm).clamp(1.0, u24::max_value().as_int() as f64) as u32;
    vec![
        TrackEvent {
            delta: u28::new(0),
            kind: TrackEventKind::Meta(MetaMessage::Tempo(u24::new(tempo_us))),
        },
        TrackEvent {
            delta: u28::new(0),
            kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
        },
    ]
}

/// Build one named MIDI track from a stream of already-merged, sorted
/// events. Emits a TrackName meta at tick 0 so the DAW arranger labels
/// the lane.
fn build_named_track<'a>(name: &'a str, events: &[TimedFugueEvent]) -> Track<'a> {
    let mut track: Track<'a> = Vec::new();
    track.push(TrackEvent {
        delta: u28::new(0),
        kind: TrackEventKind::Meta(MetaMessage::TrackName(name.as_bytes())),
    });

    // Convert events to (tick, midi_kind) pairs, stable-sort, delta-encode.
    let mut midi_events: Vec<(u32, TrackEventKind<'static>)> = Vec::new();
    for ev in events {
        let tick = beat_to_tick(ev.beat_offset);
        if let Some(kind) = fugue_event_to_midi(&ev.event) {
            midi_events.push((tick, kind));
        }
    }
    midi_events.sort_by_key(|(tick, _)| *tick);

    let mut last_tick = 0u32;
    for (tick, kind) in midi_events {
        let delta = tick.saturating_sub(last_tick);
        track.push(TrackEvent {
            delta: u28::new(delta),
            kind,
        });
        last_tick = tick;
    }

    track.push(TrackEvent {
        delta: u28::new(0),
        kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
    });
    track
}

/// Generate a filename for the exported MIDI file
pub fn generate_filename(definition: &FugueDefinition) -> String {
    let tag = definition.tag.as_deref().unwrap_or("fugue");
    // Sanitize tag for filename
    let safe_tag: String = tag
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{}_{}.mid", safe_tag, timestamp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_export() {
        let events = vec![
            TimedFugueEvent::note_on(0.0, 0, 60, 100),
            TimedFugueEvent::note_off(1.0, 0, 60),
        ];
        let definition = FugueDefinition::new(events, 1.0);
        let midi_bytes = fugue_to_smf(&definition, 120.0);
        assert!(!midi_bytes.is_empty());
        // Check MIDI header magic bytes
        assert_eq!(&midi_bytes[0..4], b"MThd");
    }

    #[test]
    fn test_cc_interpolation() {
        let events = vec![
            TimedFugueEvent::cc(0.0, 0, 1, 0),
            TimedFugueEvent::cc(1.0, 0, 1, 127),
        ];
        let interpolated = interpolate_cc_events(&events, 1.0);
        // Should have more than 2 events due to interpolation
        assert!(interpolated.len() > 2);
    }

    // ------------------------------------------------------------------
    // Multi-fugue export (Phase 15a)
    // ------------------------------------------------------------------

    fn parse_smf(bytes: &[u8]) -> midly::Smf<'_> {
        midly::Smf::parse(bytes).expect("exported bytes failed to parse back")
    }

    #[test]
    fn fugues_to_smf_empty_input_still_valid_file() {
        // "Drag all active" with nothing playing — should still hand back
        // a parseable file, not panic. Conductor track only.
        let bytes = fugues_to_smf(&[], 120.0);
        let smf = parse_smf(&bytes);
        assert_eq!(smf.tracks.len(), 1);
    }

    #[test]
    fn fugues_to_smf_one_track_per_tag() {
        let bass = FugueDefinition::new(
            vec![TimedFugueEvent::note_on(0.0, 0, 36, 100), TimedFugueEvent::note_off(1.0, 0, 36)],
            4.0,
        )
        .with_tag("bass");
        let lead = FugueDefinition::new(
            vec![TimedFugueEvent::note_on(0.0, 0, 60, 100), TimedFugueEvent::note_off(0.5, 0, 60)],
            4.0,
        )
        .with_tag("lead");

        let bytes = fugues_to_smf(&[bass, lead], 120.0);
        let smf = parse_smf(&bytes);

        // Conductor + 2 named tracks.
        assert_eq!(smf.tracks.len(), 3);

        // Track 0 is the conductor — should carry a tempo meta, no track name.
        let conductor = &smf.tracks[0];
        let has_tempo = conductor.iter().any(|e| matches!(
            e.kind,
            TrackEventKind::Meta(MetaMessage::Tempo(_))
        ));
        assert!(has_tempo, "conductor track missing tempo meta");
    }

    #[test]
    fn fugues_to_smf_track_names_match_tags() {
        let a = FugueDefinition::new(vec![], 1.0).with_tag("bass");
        let b = FugueDefinition::new(vec![], 1.0).with_tag("lead");

        let bytes = fugues_to_smf(&[a, b], 120.0);
        let smf = parse_smf(&bytes);

        // Tag names appear in first-seen order. Track 0 is conductor.
        let extract_name = |track: &midly::Track| -> Option<String> {
            track.iter().find_map(|e| match e.kind {
                TrackEventKind::Meta(MetaMessage::TrackName(name)) => {
                    Some(String::from_utf8_lossy(name).into_owned())
                }
                _ => None,
            })
        };
        assert_eq!(extract_name(&smf.tracks[1]).as_deref(), Some("bass"));
        assert_eq!(extract_name(&smf.tracks[2]).as_deref(), Some("lead"));
    }

    #[test]
    fn fugues_to_smf_same_tag_merges_events_into_one_track() {
        // Two fugues tagged "bass" — both should land in the same MIDI
        // track. Unusual live but possible if the user queued without
        // a cancel_mode, and the merge shouldn't panic or lose events.
        let a = FugueDefinition::new(
            vec![TimedFugueEvent::note_on(0.0, 0, 36, 100), TimedFugueEvent::note_off(1.0, 0, 36)],
            4.0,
        )
        .with_tag("bass");
        let b = FugueDefinition::new(
            vec![TimedFugueEvent::note_on(2.0, 0, 36, 100), TimedFugueEvent::note_off(3.0, 0, 36)],
            4.0,
        )
        .with_tag("bass");

        let bytes = fugues_to_smf(&[a, b], 120.0);
        let smf = parse_smf(&bytes);
        // Conductor + 1 merged "bass" track.
        assert_eq!(smf.tracks.len(), 2);

        // Both note-on events should be present in the merged track.
        let note_ons = smf.tracks[1]
            .iter()
            .filter(|e| matches!(e.kind, TrackEventKind::Midi { message: MidiMessage::NoteOn { .. }, .. }))
            .count();
        assert_eq!(note_ons, 2);
    }

    #[test]
    fn fugues_to_smf_untagged_fugues_get_unique_tracks() {
        let a = FugueDefinition::new(
            vec![TimedFugueEvent::note_on(0.0, 0, 60, 100)],
            1.0,
        );
        let b = FugueDefinition::new(
            vec![TimedFugueEvent::note_on(0.0, 0, 62, 100)],
            1.0,
        );
        let bytes = fugues_to_smf(&[a, b], 120.0);
        let smf = parse_smf(&bytes);
        // Conductor + two untagged tracks (fugue-1, fugue-2).
        assert_eq!(smf.tracks.len(), 3);
    }

    // ------------------------------------------------------------------
    // merge_fugues_by_tag — the pure-data merge stage, independently useful.
    // ------------------------------------------------------------------

    #[test]
    fn merge_keeps_untagged_fugues_separate() {
        let a = FugueDefinition::new(vec![TimedFugueEvent::note_on(0.0, 0, 60, 100)], 1.0);
        let b = FugueDefinition::new(vec![TimedFugueEvent::note_on(0.0, 0, 62, 100)], 1.0);
        let merged = merge_fugues_by_tag(&[a, b]);
        assert_eq!(merged.len(), 2);
        assert!(merged.iter().all(|d| d.tag.is_none()));
    }

    #[test]
    fn merge_folds_same_tag_into_one_definition() {
        let a = FugueDefinition::new(
            vec![TimedFugueEvent::note_on(0.0, 0, 36, 100), TimedFugueEvent::note_off(1.0, 0, 36)],
            4.0,
        ).with_tag("bass");
        let b = FugueDefinition::new(
            vec![TimedFugueEvent::note_on(2.0, 0, 36, 100), TimedFugueEvent::note_off(3.0, 0, 36)],
            8.0,
        ).with_tag("bass");

        let merged = merge_fugues_by_tag(&[a, b]);
        assert_eq!(merged.len(), 1);
        let combined = &merged[0];
        assert_eq!(combined.tag.as_deref(), Some("bass"));
        assert_eq!(combined.events.len(), 4);
        // Duration = max of inputs' durations.
        assert_eq!(combined.duration_beats, 8.0);
        // Events sorted by beat_offset (0, 1, 2, 3).
        let beats: Vec<f64> = combined.events.iter().map(|e| e.beat_offset).collect();
        assert_eq!(beats, vec![0.0, 1.0, 2.0, 3.0]);
    }

    #[test]
    fn merge_preserves_first_seen_order() {
        let lead = FugueDefinition::new(vec![], 1.0).with_tag("lead");
        let bass = FugueDefinition::new(vec![], 1.0).with_tag("bass");
        let merged = merge_fugues_by_tag(&[lead, bass]);
        // `lead` appeared first, so it comes out first.
        assert_eq!(merged[0].tag.as_deref(), Some("lead"));
        assert_eq!(merged[1].tag.as_deref(), Some("bass"));
    }

    #[test]
    fn merge_gives_merged_fugue_fresh_id() {
        let a = FugueDefinition::new(vec![], 1.0).with_tag("x");
        let a_id = a.id;
        let merged = merge_fugues_by_tag(&[a]);
        assert_ne!(merged[0].id, a_id, "merged fugue should have a new id");
    }

    // ------------------------------------------------------------------
    // ExportedMidi — the build-then-inspect-or-serialize stage.
    // ------------------------------------------------------------------

    #[test]
    fn exported_midi_structure_is_inspectable() {
        // Decoupled pipeline: build without serializing, check structure,
        // then serialize as a separate step.
        let a = FugueDefinition::new(vec![], 4.0).with_tag("bass");
        let b = FugueDefinition::new(vec![], 4.0).with_tag("lead");
        let exported = ExportedMidi::from_fugues(&[a, b], 120.0);
        assert_eq!(exported.tempo_bpm, 120.0);
        assert_eq!(exported.tracks.len(), 2);
        assert_eq!(exported.tracks[0].name, "bass");
        assert_eq!(exported.tracks[1].name, "lead");

        // Same value round-trips through to_bytes + parse.
        let bytes = exported.to_bytes();
        let smf = parse_smf(&bytes);
        assert_eq!(smf.tracks.len(), 3);
    }
}
