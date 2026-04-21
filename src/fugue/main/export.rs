//! MIDI file export for fugues
//!
//! Converts FugueDefinition to Standard MIDI File (SMF) format.

use midly::{
    Format, Header, MetaMessage, MidiMessage, Smf, Track, TrackEvent, TrackEventKind,
    num::{u14, u15, u24, u28, u4, u7},
};
use std::collections::HashMap;

use super::super::types::{FugueDefinition, FugueEvent, InterpolationMode, TimedFugueEvent};

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

    let events = densify_cc_events(&definition.events, definition.cc_interpolation);

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

/// Densify CC lanes for MIDI export: turn sparse anchor events into a
/// smooth event stream so DAW piano-roll automation curves match what
/// the fugue schema describes.
///
/// ## Curve selection per segment
/// Each CC event carries an optional per-point `curve` (the "curve
/// arriving at this point" convention — see `FugueEvent::Cc`). For each
/// segment between two anchors on the same `(channel, cc)` lane, the
/// arriving point's curve wins; absent that, the fugue-level
/// `cc_interpolation` applies.
///
/// ## When we emit intermediate points
/// - `InterpolationMode::None` on either the fugue or the segment →
///   stepped; emit only the anchors. DAW shows flat segments with jumps.
/// - Any other mode (`Linear`, `Exp`, `Log`) → emit ~8 intermediate
///   events per beat (`CC_INTERP_RESOLUTION_TICKS / PPQ`), each with
///   `t` remapped through `InterpolationMode::apply_curve`. The exported
///   `.mid` then carries a smooth ramp that Bitwig/Live render as a
///   proper automation curve instead of two levels with a jump.
///
/// Non-CC events pass through unchanged.
fn densify_cc_events(
    events: &[TimedFugueEvent],
    fugue_default_curve: InterpolationMode,
) -> Vec<TimedFugueEvent> {
    let mut cc_lanes: HashMap<(u8, u8), Vec<(f64, u8, Option<InterpolationMode>)>> = HashMap::new();
    let mut non_cc_events: Vec<TimedFugueEvent> = Vec::new();

    for event in events {
        match event.event {
            FugueEvent::Cc { channel, cc, value, curve } => {
                cc_lanes
                    .entry((channel, cc))
                    .or_default()
                    .push((event.beat_offset, value, curve));
            }
            _ => non_cc_events.push(*event),
        }
    }

    let mut result: Vec<TimedFugueEvent> = non_cc_events;
    let resolution_beats = CC_INTERP_RESOLUTION_TICKS as f64 / PPQ as f64;

    for ((channel, cc), mut points) in cc_lanes {
        points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        if points.is_empty() {
            continue;
        }

        // First anchor always ships — it sets the starting value.
        result.push(TimedFugueEvent::cc(points[0].0, channel, cc, points[0].1));

        for window in points.windows(2) {
            let (start_beat, start_val, _) = window[0];
            let (end_beat, end_val, arriving_curve) = window[1];

            // Per-segment curve: the arriving point's curve overrides the
            // fugue-level default. Matches the same convention per-note
            // expression uses.
            let curve = arriving_curve.unwrap_or(fugue_default_curve);

            let flat_segment =
                start_val == end_val || curve == InterpolationMode::None;
            if flat_segment {
                result.push(TimedFugueEvent::cc(end_beat, channel, cc, end_val));
                continue;
            }

            let mut current_beat = start_beat + resolution_beats;
            while current_beat < end_beat {
                let t = (current_beat - start_beat) / (end_beat - start_beat);
                let curved_t = curve.apply_curve(t);
                let value = lerp(start_val as f64, end_val as f64, curved_t)
                    .clamp(0.0, 127.0) as u8;
                result.push(TimedFugueEvent::cc(current_beat, channel, cc, value));
                current_beat += resolution_beats;
            }
            result.push(TimedFugueEvent::cc(end_beat, channel, cc, end_val));
        }
    }

    result.sort_by(|a, b| a.beat_offset.partial_cmp(&b.beat_offset).unwrap_or(std::cmp::Ordering::Equal));
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

/// Cap (in beats) applied when computing LCM durations in
/// `merge_fugues_by_tag`. 128 beats = 32 bars at 4/4. Above this, relatively
/// prime or otherwise pathological duration combinations would produce
/// exports too large to be musically useful; fall back to the max of the
/// input durations with a log warning. In practice the LLM writes fugues
/// with nice power-of-two durations (2, 4, 8, 16 beats), so this cap is
/// almost never hit.
const LCM_CAP_BEATS: f64 = 128.0;

/// Quantize beats to this resolution when computing LCM. 960 ticks/beat
/// matches the standard DAW PPQ and cleanly divides triplets (÷3), sixteenth
/// notes (÷4), swung grids, etc. — anything the LLM is likely to write.
const LCM_TICKS_PER_BEAT: u64 = 960;

/// Merge fugues that share a tag into one `FugueDefinition` per tag.
/// Untagged fugues pass through as-is (one output entry each, tag stays
/// `None`). Order is stable: the first-seen tag determines output order.
///
/// **Duration handling via LCM:** when a tag group's members have different
/// durations — a 4-bar bass line + a 16-bar melody, say — the merged fugue's
/// duration is the LCM of its members' durations, and each member's events
/// are replicated to fill that span. This matches the user's musical
/// expectation that dropping the merged clip into a DAW and looping it
/// produces the same result as playing the fugues together live. With only
/// `max`, the shorter pattern would stop mid-loop.
///
/// Example: bass (4 beats) + melody (16 beats) → LCM = 16, bass replicated 4×,
/// melody once, merged duration = 16.
///
/// **Oversized LCM — truncate instead of revert to `max`.** If the computed
/// LCM exceeds [`LCM_CAP_BEATS`] (relatively prime durations, exotic time
/// signatures), the merged duration is clamped to the cap and every member
/// keeps replicating as many cycles as fit; the final (partial) cycle's
/// events past the cap are dropped. This preserves the "everything plays
/// in parallel" feel even when a perfect loop isn't representable in the
/// export size budget.
///
/// Merged fields:
/// - **events**: union of replicated inputs, sorted by `beat_offset`
/// - **duration_beats**: LCM (or capped at `LCM_CAP_BEATS`)
/// - **loop_mode**, **quantize**, **cancel_mode**, **cc_interpolation**:
///   inherited from the first fugue of the group
/// - **tag**: the shared tag (`None` for untagged singletons)
/// - **id**: freshly generated so the merged fugue has a distinct identity
pub fn merge_fugues_by_tag(defs: &[FugueDefinition]) -> Vec<FugueDefinition> {
    // Group first, then merge — separating the two phases keeps the LCM +
    // replication logic on flat lists rather than tangled with tag tracking.
    let mut groups: Vec<(Option<String>, Vec<&FugueDefinition>)> = Vec::new();
    let mut tag_to_index: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for def in defs {
        match &def.tag {
            Some(tag) => {
                if let Some(&idx) = tag_to_index.get(tag) {
                    groups[idx].1.push(def);
                } else {
                    tag_to_index.insert(tag.clone(), groups.len());
                    groups.push((Some(tag.clone()), vec![def]));
                }
            }
            None => {
                // Each untagged fugue is its own group — we don't know if
                // two untagged queues were meant to represent the same part.
                groups.push((None, vec![def]));
            }
        }
    }

    let mut out: Vec<FugueDefinition> = Vec::with_capacity(groups.len());
    for (tag, members) in groups {
        // Single-member group: nothing to merge, just pass through with a
        // fresh id (or keep original id for untagged singletons since they're
        // unchanged). Avoids unnecessary event cloning for the common case.
        if members.len() == 1 {
            let mut base = members[0].clone();
            if tag.is_some() {
                base.id = crate::fugue::types::generate_fugue_id();
            }
            out.push(base);
            continue;
        }

        let durations: Vec<f64> = members.iter().map(|d| d.duration_beats).collect();
        let target_duration = lcm_beats_capped(&durations, LCM_CAP_BEATS);

        // Replicate each fugue's events across target_duration. When LCM fit
        // under the cap, every cycle completes exactly at the boundary. When
        // we capped, the last cycle of at least one member is partial —
        // events past `target_duration` are dropped ("cut them off at the end"
        // semantics) rather than leaving the tail silent.
        let mut merged_events: Vec<TimedFugueEvent> = Vec::new();
        for def in &members {
            let d = def.duration_beats;
            if d <= 0.0 {
                continue;
            }
            let mut cycle = 0usize;
            loop {
                let offset = cycle as f64 * d;
                if offset >= target_duration {
                    break;
                }
                for ev in &def.events {
                    let new_beat = ev.beat_offset + offset;
                    if new_beat < target_duration {
                        merged_events.push(TimedFugueEvent {
                            beat_offset: new_beat,
                            event: ev.event,
                        });
                    }
                }
                cycle += 1;
            }
        }

        merged_events.sort_by(|a, b| {
            a.beat_offset
                .partial_cmp(&b.beat_offset)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Inherit quantize/loop/interpolation from the first member. Groups
        // with >1 member are unusual enough that a smarter policy isn't
        // worth the complexity.
        let mut base = members[0].clone();
        base.id = crate::fugue::types::generate_fugue_id();
        base.tag = tag;
        base.events = merged_events;
        base.duration_beats = target_duration;
        out.push(base);
    }

    out
}

/// LCM over a slice of `f64` beat durations, clamped at `cap_beats`. Pairs
/// with the caller's truncation logic: when the true LCM would blow the cap
/// (relatively prime durations, exotic time signatures), return the cap and
/// let the caller replicate members until they run out of room.
///
/// Quantizes to [`LCM_TICKS_PER_BEAT`] ticks before computing LCM, so floats
/// that come in as (near-)rationals like `1.0 / 3.0` still snap cleanly.
fn lcm_beats_capped(durations: &[f64], cap_beats: f64) -> f64 {
    if durations.is_empty() {
        return 0.0;
    }
    let mut lcm_t: u64 = beats_to_ticks(durations[0]);
    let cap_ticks = beats_to_ticks(cap_beats);
    for &d in &durations[1..] {
        let d_t = beats_to_ticks(d);
        if d_t == 0 {
            continue;
        }
        lcm_t = lcm_u64(lcm_t, d_t);
        // Early bail on explosion — the numbers can grow fast with coprime
        // durations, and we'd rather return the cap than risk overflow on
        // the next iteration.
        if lcm_t > cap_ticks {
            log::warn!(
                "merge_fugues_by_tag: LCM of durations {:?} exceeds {} beat cap; \
                 clamping merged duration to cap and truncating member cycles at the end",
                durations, cap_beats
            );
            return cap_beats;
        }
    }
    ticks_to_beats(lcm_t)
}

fn beats_to_ticks(b: f64) -> u64 {
    (b * LCM_TICKS_PER_BEAT as f64).round() as u64
}

fn ticks_to_beats(t: u64) -> f64 {
    t as f64 / LCM_TICKS_PER_BEAT as f64
}

fn lcm_u64(a: u64, b: u64) -> u64 {
    if a == 0 || b == 0 {
        return 0;
    }
    a / gcd_u64(a, b) * b
}

fn gcd_u64(a: u64, b: u64) -> u64 {
    if b == 0 {
        a
    } else {
        gcd_u64(b, a % b)
    }
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

            let events = densify_cc_events(&def.events, def.cc_interpolation);

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

/// "Drag all" variant: every input fugue flattens into **one** MIDI
/// track, regardless of tag. Used when the user wants a single DAW clip
/// carrying every active fugue for an instance (e.g. notes + a CC filter
/// sweep meant to play on one channel).
///
/// Contrast with [`fugues_to_smf`], which preserves tag groupings as
/// separate tracks — that's the right behavior when the fugues are
/// independent parts (bass vs melody), but wrong when they're concerns
/// of a single instrument the DAW should treat as one clip.
///
/// LCM duration + replication logic runs the same as `merge_fugues_by_tag`
/// so a short CC sweep loops cleanly alongside a longer note pattern.
pub fn fugues_to_single_track_smf(
    definitions: &[FugueDefinition],
    tempo_bpm: f64,
) -> Vec<u8> {
    let Some(merged) = merge_all_fugues(definitions) else {
        return ExportedMidi { tempo_bpm, tracks: Vec::new() }.to_bytes();
    };
    let name = merged
        .tag
        .clone()
        .unwrap_or_else(|| "merged".to_string());
    let events = densify_cc_events(&merged.events, merged.cc_interpolation);
    ExportedMidi {
        tempo_bpm,
        tracks: vec![ExportedTrack { name, events }],
    }
    .to_bytes()
}

/// Flatten every input fugue into one `FugueDefinition`, ignoring tags.
/// Implementation shortcut: re-tag everything with a synthetic marker
/// and delegate to [`merge_fugues_by_tag`] so the LCM + replication
/// logic stays in one place. Returns `None` when input is empty.
pub fn merge_all_fugues(definitions: &[FugueDefinition]) -> Option<FugueDefinition> {
    if definitions.is_empty() {
        return None;
    }
    let retagged: Vec<FugueDefinition> = definitions
        .iter()
        .map(|d| {
            let mut clone = d.clone();
            clone.tag = Some("_merged".to_string());
            clone
        })
        .collect();
    let mut merged = merge_fugues_by_tag(&retagged);
    debug_assert_eq!(merged.len(), 1);
    merged.pop().map(|mut m| {
        // Drop the synthetic tag — callers decide how to name the track.
        m.tag = None;
        m
    })
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
        let densified = densify_cc_events(&events, InterpolationMode::Linear);
        // Should have more than 2 events due to densification
        assert!(densified.len() > 2);
    }

    #[test]
    fn densify_skips_interpolation_when_fugue_default_is_none() {
        // InterpolationMode::None = stepped; DAW should see the exact two
        // anchors and draw a flat-to-jump shape between them.
        let events = vec![
            TimedFugueEvent::cc(0.0, 0, 1, 0),
            TimedFugueEvent::cc(1.0, 0, 1, 127),
        ];
        let densified = densify_cc_events(&events, InterpolationMode::None);
        assert_eq!(densified.len(), 2);
    }

    #[test]
    fn densify_expands_exp_curve_via_apply_curve() {
        // Exponential ramp: intermediate values should follow t² shape
        // (slow then fast), not a straight line. Previously Exp mode fell
        // through to "don't densify" and exported only two levels.
        let events = vec![
            TimedFugueEvent::cc(0.0, 0, 1, 0),
            TimedFugueEvent::cc(1.0, 0, 1, 120),
        ];
        let densified = densify_cc_events(&events, InterpolationMode::Exp);
        assert!(densified.len() > 2, "Exp curve should densify, not just emit anchors");

        // Pick a mid-segment sample and confirm it matches Exp curve, not linear.
        // At t=0.5, linear would give ~60; exp (t²) gives ~30.
        let midpoint = densified
            .iter()
            .find(|e| (e.beat_offset - 0.5).abs() < 0.08)
            .expect("expected a sample near the midpoint");
        let midvalue = match midpoint.event {
            FugueEvent::Cc { value, .. } => value,
            _ => panic!(),
        };
        // t² at t=0.5 → 0.25 → value ≈ 30. Linear would be ~60.
        assert!(midvalue < 50, "Exp curve at t=0.5 should be <50, got {}", midvalue);
    }

    #[test]
    fn densify_per_point_curve_overrides_fugue_default() {
        // Fugue-level says Linear but this segment's arriving point
        // carries an explicit Exp curve tag — the segment should honour
        // the per-point override.
        let events = vec![
            TimedFugueEvent::new(0.0, FugueEvent::Cc { channel: 0, cc: 1, value: 0, curve: None }),
            TimedFugueEvent::new(1.0, FugueEvent::Cc { channel: 0, cc: 1, value: 120, curve: Some(InterpolationMode::Exp) }),
        ];
        let densified = densify_cc_events(&events, InterpolationMode::Linear);
        let midpoint = densified
            .iter()
            .find(|e| (e.beat_offset - 0.5).abs() < 0.08)
            .expect("expected sample near midpoint");
        let midvalue = match midpoint.event {
            FugueEvent::Cc { value, .. } => value,
            _ => panic!(),
        };
        // Should be low (Exp), not ~60 (Linear).
        assert!(midvalue < 50, "per-point Exp should override Linear default; got {}", midvalue);
    }

    #[test]
    fn densify_per_point_none_makes_segment_stepped() {
        // Fugue-level Linear but a segment explicitly tagged None should
        // stay stepped. Used when the LLM wants a filter-hold-then-jump.
        let events = vec![
            TimedFugueEvent::new(0.0, FugueEvent::Cc { channel: 0, cc: 1, value: 0, curve: None }),
            TimedFugueEvent::new(1.0, FugueEvent::Cc { channel: 0, cc: 1, value: 120, curve: Some(InterpolationMode::None) }),
        ];
        let densified = densify_cc_events(&events, InterpolationMode::Linear);
        assert_eq!(densified.len(), 2, "explicit None curve should not densify this segment");
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
        // LCM(4, 8) = 8. The 4-beat fugue replicates twice, the 8-beat
        // fugue plays once — merged duration 8, 6 total events.
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
        // a's 2 events × 2 cycles + b's 2 events × 1 cycle = 6 events.
        assert_eq!(combined.events.len(), 6);
        assert_eq!(combined.duration_beats, 8.0);
        // a's events at (0, 1) replicate to (4, 5); b's events at (2, 3) once.
        let beats: Vec<f64> = combined.events.iter().map(|e| e.beat_offset).collect();
        assert_eq!(beats, vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0]);
    }

    #[test]
    fn merge_lcm_expands_shorter_fugue_to_fill_longer() {
        // 4-bar bass + 16-bar melody → LCM 16. Bass plays 4×, melody 1×.
        // This is the "two loops at different resolutions" case the LLM
        // naturally writes and users expect to loop cleanly in the DAW.
        let bass_4_beats = FugueDefinition::new(
            vec![TimedFugueEvent::note_on(0.0, 0, 36, 100)],
            4.0,
        ).with_tag("bass");
        let melody_16_beats = FugueDefinition::new(
            vec![TimedFugueEvent::note_on(0.0, 0, 60, 100)],
            16.0,
        ).with_tag("bass");  // Same tag so they merge.

        let merged = merge_fugues_by_tag(&[bass_4_beats, melody_16_beats]);
        assert_eq!(merged.len(), 1);
        let combined = &merged[0];
        assert_eq!(combined.duration_beats, 16.0);
        // Bass replicates at offsets 0, 4, 8, 12 (4 cycles); melody once at 0.
        let bass_beats: Vec<f64> = combined
            .events
            .iter()
            .filter(|e| matches!(e.event, crate::fugue::types::FugueEvent::NoteOn { note: 36, .. }))
            .map(|e| e.beat_offset)
            .collect();
        assert_eq!(bass_beats, vec![0.0, 4.0, 8.0, 12.0]);
        let melody_beats: Vec<f64> = combined
            .events
            .iter()
            .filter(|e| matches!(e.event, crate::fugue::types::FugueEvent::NoteOn { note: 60, .. }))
            .map(|e| e.beat_offset)
            .collect();
        assert_eq!(melody_beats, vec![0.0]);
    }

    #[test]
    fn merge_lcm_coprime_durations_within_cap() {
        // 3 + 5 = LCM 15. Coprime but small enough to be under the cap.
        let a = FugueDefinition::new(
            vec![TimedFugueEvent::note_on(0.0, 0, 60, 100)],
            3.0,
        ).with_tag("x");
        let b = FugueDefinition::new(
            vec![TimedFugueEvent::note_on(0.0, 0, 62, 100)],
            5.0,
        ).with_tag("x");
        let merged = merge_fugues_by_tag(&[a, b]);
        assert_eq!(merged[0].duration_beats, 15.0);
        // a replicates 5×, b replicates 3×.
        assert_eq!(merged[0].events.len(), 5 + 3);
    }

    #[test]
    fn merge_lcm_oversized_truncates_at_cap() {
        // LCM(127, 128) = 16256 beats — far past the 128-beat cap. Both
        // members should replicate as many cycles as fit at 128 beats, with
        // events past the cap truncated. Neither silent-tails nor plays-once.
        let a = FugueDefinition::new(
            vec![TimedFugueEvent::note_on(0.0, 0, 60, 100)],
            127.0,
        ).with_tag("x");
        let b = FugueDefinition::new(
            vec![TimedFugueEvent::note_on(0.0, 0, 62, 100)],
            128.0,
        ).with_tag("x");
        let merged = merge_fugues_by_tag(&[a, b]);
        assert_eq!(merged[0].duration_beats, 128.0);

        // a (127-beat): cycles start at 0 and 127; the cycle at 127 has its
        // event at 127 which is < 128, so it plays. 2 events total for a.
        // b (128-beat): one cycle at offset 0, event at 0 < 128. 1 event.
        // Grand total: 3 events.
        assert_eq!(merged[0].events.len(), 3);

        // All events within the cap.
        for ev in &merged[0].events {
            assert!(ev.beat_offset < 128.0);
        }
    }

    #[test]
    fn merge_lcm_short_against_huge_truncates_cleanly() {
        // 1-beat pattern + 127-beat pattern → LCM = 127, under cap. All
        // fine. Harder case: 1-beat pattern + a 131-beat pattern. LCM jumps
        // to 131 (still under 128? no, 131 > 128). Actually LCM(1, 131) = 131.
        // That exceeds cap (128). Clamp to 128, replicate short 128× — short
        // should still show up many times, not be dropped. Exercises the
        // truncate-instead-of-silence semantics end to end.
        let short = FugueDefinition::new(
            vec![TimedFugueEvent::note_on(0.0, 0, 60, 100)],
            1.0,
        ).with_tag("x");
        let long = FugueDefinition::new(
            vec![TimedFugueEvent::note_on(0.0, 0, 62, 100)],
            131.0,
        ).with_tag("x");
        let merged = merge_fugues_by_tag(&[short, long]);
        assert_eq!(merged[0].duration_beats, 128.0);
        // short: 128 cycles at offsets 0..127, one event each = 128.
        // long: one cycle at offset 0 (next cycle would start at 131 > 128).
        //   Its event at beat 0 is within cap → 1 event.
        // Total 129.
        assert_eq!(merged[0].events.len(), 129);
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
    fn fugues_to_single_track_flattens_different_tags() {
        // Two fugues with different tags — drag-all should collapse both
        // into ONE MIDI track (plus the conductor). With the default
        // `fugues_to_smf` they would become two named tracks, and DAWs
        // that split multi-track SMFs into multiple clips (like Bitwig)
        // would produce two clips instead of one.
        let notes = FugueDefinition::new(
            vec![
                TimedFugueEvent::note_on(0.0, 0, 60, 100),
                TimedFugueEvent::note_off(1.0, 0, 60),
            ],
            4.0,
        )
        .with_tag("notes");
        let sweep = FugueDefinition::new(
            vec![
                TimedFugueEvent::cc(0.0, 0, 74, 30),
                TimedFugueEvent::cc(4.0, 0, 74, 100),
            ],
            4.0,
        )
        .with_tag("filter-sweep")
        .with_cc_interpolation(InterpolationMode::Linear);

        let bytes = fugues_to_single_track_smf(&[notes, sweep], 120.0);
        let smf = parse_smf(&bytes);
        // Conductor + exactly ONE musical track — the flatten guarantee.
        assert_eq!(smf.tracks.len(), 2, "expected conductor + 1 merged track");

        // That one track should carry both the notes and the (densified)
        // CC stream. Check at least one of each survived the merge.
        let musical = &smf.tracks[1];
        let has_note_on = musical.iter().any(|e| matches!(
            e.kind,
            TrackEventKind::Midi { message: MidiMessage::NoteOn { .. }, .. }
        ));
        let has_cc = musical.iter().any(|e| matches!(
            e.kind,
            TrackEventKind::Midi { message: MidiMessage::Controller { .. }, .. }
        ));
        assert!(has_note_on, "merged track missing note events");
        assert!(has_cc, "merged track missing CC events");
    }

    #[test]
    fn fugues_to_single_track_empty_input_still_valid() {
        let bytes = fugues_to_single_track_smf(&[], 120.0);
        let smf = parse_smf(&bytes);
        // Just the conductor track — parseable empty export.
        assert_eq!(smf.tracks.len(), 1);
    }

    #[test]
    fn merge_all_fugues_drops_synthetic_tag() {
        // merge_all_fugues internally re-tags everything to `_merged` to
        // reuse merge_fugues_by_tag's LCM logic. That synthetic tag
        // shouldn't leak out — callers expect a clean `tag: None`.
        let a = FugueDefinition::new(vec![], 4.0).with_tag("a");
        let b = FugueDefinition::new(vec![], 4.0).with_tag("b");
        let merged = merge_all_fugues(&[a, b]).expect("non-empty input");
        assert!(merged.tag.is_none(), "synthetic _merged tag leaked: {:?}", merged.tag);
    }

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
