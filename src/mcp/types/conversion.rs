//! Bidirectional conversion between the compact LLM-facing schema and the
//! scheduler-facing `FugueDefinition`:
//!
//! - `compact_to_definition` — what the MCP `queue_fugue` tool uses to turn
//!   a batch of `CompactFugue`s into `FugueDefinition`s the audio thread
//!   understands.
//! - `definition_to_compact` — inverse; the read-back path for `get_fugue`
//!   so the LLM can read-modify-write through the same shape it wrote.
//!
//! Also lives here: the LLM-facing string parsers (`once`/`forever`,
//! `bar`/`bars:4`, `tag:xxx`) and their reverse tagging functions — they're
//! part of the same MCP-wire-format concern.

use std::collections::{HashMap, VecDeque};

use crate::fugue::{
    CancelMode, FugueDefinition, FugueEvent, InterpolationMode, LoopMode, QuantizeMode,
    StartMode, TimedFugueEvent,
};

use super::compact::{
    CompactCc, CompactFugue, CompactNote, CompactPitchBend, CompactPressure, FugueContent,
};
use super::emit::{emit_cc_lane, emit_notes, emit_per_note_pitch_bend, emit_per_note_pressure};
use super::note::Note;
use super::point::Point;
use super::request::QueueFugueData;

// =============================================================================
// String parsers (LLM wire format → typed enum)
// =============================================================================

/// Parse an optional interpolation-mode string into the typed enum.
/// Unknown / missing values default to Linear to match the CC default and
/// keep "not specified" producing smooth output.
pub fn parse_interpolation_mode(s: Option<&str>) -> InterpolationMode {
    match s {
        Some("none") => InterpolationMode::None,
        Some("exp") => InterpolationMode::Exp,
        Some("log") => InterpolationMode::Log,
        _ => InterpolationMode::Linear,
    }
}

/// Parse the LLM-facing loop-mode string (`once` | `forever` | `<n>`).
/// `<n>` decodes to `Times(n)` so the LLM can write `"loop_mode": "4"`.
/// Returns `None` on empty/unrecognized input so the caller decides the
/// fallback (usually `Forever` at the tool level).
pub fn parse_loop_mode_str(s: &str) -> Option<LoopMode> {
    match s.trim().to_lowercase().as_str() {
        "" => None,
        "once" => Some(LoopMode::Once),
        "forever" => Some(LoopMode::Forever),
        other => other.parse::<u32>().ok().map(LoopMode::Times),
    }
}

/// Parse the LLM-facing quantize-mode string (`immediate` | `beat` | `bar`
/// | `bars:<n>`). Returns `None` on unrecognized input; callers typically
/// fall back to `Bar`.
pub fn parse_quantize_str(s: &str) -> Option<QuantizeMode> {
    let lower = s.trim().to_lowercase();
    match lower.as_str() {
        "" => None,
        "immediate" => Some(QuantizeMode::Immediate),
        "beat" => Some(QuantizeMode::Beat),
        "bar" => Some(QuantizeMode::Bar),
        _ => lower
            .strip_prefix("bars:")
            .and_then(|n| n.parse::<u32>().ok())
            .map(QuantizeMode::Bars),
    }
}

/// Parse the LLM-facing start-mode string. Unknown / missing input
/// falls back to [`StartMode::Phase`] (the default — immediate-ish
/// "join the grid" behaviour).
pub fn parse_start_mode_str(s: Option<&str>) -> StartMode {
    match s.map(str::trim).map(str::to_lowercase).as_deref() {
        Some("boundary") => StartMode::Boundary,
        _ => StartMode::Phase,
    }
}

/// Parse the LLM-facing cancel-mode string. `tag:<name>` captures the
/// inner tag; unknown input silently falls back to `None` (layer onto
/// existing fugues).
pub fn parse_cancel_mode_str(s: &str) -> CancelMode {
    match s.trim().to_lowercase().as_str() {
        "all" => CancelMode::CancelAll,
        s if s.starts_with("tag:") => {
            let tag = s.strip_prefix("tag:").unwrap_or("").to_string();
            CancelMode::CancelByTag(tag)
        }
        _ => CancelMode::None,
    }
}

// =============================================================================
// Reverse: typed enum → LLM wire-format tag
// =============================================================================

/// Stable lowercase tag for an interpolation mode. Matches the snake_case
/// the input parser accepts, so a round-tripped value re-parses cleanly.
fn interp_tag(mode: InterpolationMode) -> &'static str {
    match mode {
        InterpolationMode::Linear => "linear",
        InterpolationMode::Exp => "exp",
        InterpolationMode::Log => "log",
        InterpolationMode::None => "none",
    }
}

/// Stable lowercase tag for a `LoopMode`. `Times(n)` serializes as the
/// raw decimal — matches what `parse_loop_mode_str` accepts.
fn loop_mode_tag(mode: LoopMode) -> String {
    match mode {
        LoopMode::Once => "once".to_string(),
        LoopMode::Forever => "forever".to_string(),
        LoopMode::Times(n) => n.to_string(),
    }
}

/// Stable lowercase tag for a `QuantizeMode`, matching `parse_quantize_str`.
fn quantize_tag(mode: QuantizeMode) -> String {
    match mode {
        QuantizeMode::Immediate => "immediate".to_string(),
        QuantizeMode::Beat => "beat".to_string(),
        QuantizeMode::Bar => "bar".to_string(),
        QuantizeMode::Bars(n) => format!("bars:{}", n),
    }
}

/// Stable lowercase tag for a [`StartMode`], matching `parse_start_mode_str`.
fn start_mode_tag(mode: StartMode) -> &'static str {
    match mode {
        StartMode::Phase => "phase",
        StartMode::Boundary => "boundary",
    }
}

/// Stable lowercase tag for a `CancelMode`.
fn cancel_mode_tag(mode: &CancelMode) -> String {
    match mode {
        CancelMode::None => "none".to_string(),
        CancelMode::CancelAll => "all".to_string(),
        CancelMode::CancelByTag(tag) => format!("tag:{}", tag),
    }
}

/// Build a `Point` with an optional per-segment curve. The curve is elided
/// when it matches the lane default so a typical ramp with one shared
/// curve serializes compactly.
fn point_with_curve(
    beat: f64,
    value: f64,
    curve: Option<InterpolationMode>,
    lane_default: InterpolationMode,
) -> Point {
    let curve = match curve {
        Some(c) if c != lane_default => Some(interp_tag(c).to_string()),
        _ => None,
    };
    Point { beat, value, curve }
}

// =============================================================================
// compact_to_definition — request ingest path
// =============================================================================

/// Shared defaults (top-level on `QueueFugueData`) that fill in when a
/// `CompactFugue` doesn't override them. Precomputed once per batch so the
/// per-fugue path stays a pure fn that doesn't re-parse the shared strings.
///
/// `duration_beats` is `Option` rather than a concrete f64 so the per-fugue
/// resolver can tell "unset" (→ auto-size from content) from "explicitly 4".
pub struct QueueFugueDefaults {
    pub quantize_str: String,
    pub duration_beats: Option<f64>,
    pub loop_mode_str: String,
    pub start_mode_str: Option<String>,
}

impl QueueFugueDefaults {
    /// Build from the outer `QueueFugueData`. Missing fields get the
    /// documented defaults (`bar`, auto-sized, `forever`). `start_mode`
    /// stays `None` at this layer so the per-fugue resolver can tell
    /// "LLM didn't specify" from "LLM picked phase explicitly" — the
    /// final fallback to `StartMode::Phase` happens in
    /// `compact_to_definition`.
    pub fn from_data(data: &QueueFugueData) -> Self {
        Self {
            quantize_str: data.quantize.as_deref().unwrap_or("bar").to_string(),
            duration_beats: data.duration_beats,
            loop_mode_str: data.loop_mode.as_deref().unwrap_or("forever").to_string(),
            start_mode_str: data.start_mode.clone(),
        }
    }
}

/// Beats per bar assumed when auto-sizing `duration_beats`. Time signature
/// isn't known at conversion time (lives on the transport), so we use 4/4 —
/// matches the default quantize grid and the dominant case.
const AUTO_DURATION_BAR_BEATS: f64 = 4.0;

/// Pick a `duration_beats` that cleanly contains every event in `events`.
///
/// Rounds up to the smallest whole bar (`AUTO_DURATION_BAR_BEATS`-beat
/// multiple) that fits the latest event's `beat_offset` — for notes that's
/// `beat + duration` (the auto-emitted NoteOff beat), for CC / per-note
/// expression it's the last point. Empty content falls back to one bar so
/// a content-less fugue still has a sensible loop length.
///
/// `explicit` takes precedence when it's already long enough; when it's
/// shorter than the content the content wins. Silently truncating notes
/// to honour a too-short explicit value is almost always an LLM mistake,
/// not a feature — the only cost of extending is a bit of trailing silence.
fn resolve_duration_beats(explicit: Option<f64>, events: &[TimedFugueEvent]) -> f64 {
    let required = {
        let max_end = events
            .iter()
            .map(|e| e.beat_offset)
            .fold(0.0_f64, f64::max);
        let rounded = (max_end / AUTO_DURATION_BAR_BEATS).ceil() * AUTO_DURATION_BAR_BEATS;
        rounded.max(AUTO_DURATION_BAR_BEATS)
    };
    match explicit {
        Some(v) if v >= required => v,
        _ => required,
    }
}

/// Convert one `CompactFugue` into the scheduler-facing `FugueDefinition`.
/// Pure function: reads the compact input, resolves per-fugue overrides
/// against `defaults`, expands every `FugueContent` variant through the
/// existing `emit_*` helpers, sorts events by beat, and stamps tag /
/// loop / quantize / cancel. Call site just needs to queue the result.
pub fn compact_to_definition(
    compact: &CompactFugue,
    defaults: &QueueFugueDefaults,
) -> FugueDefinition {
    let quantize_str = compact.quantize.as_deref().unwrap_or(&defaults.quantize_str);
    let explicit_duration = compact.duration_beats.or(defaults.duration_beats);
    let loop_mode_str = compact.loop_mode.as_deref().unwrap_or(&defaults.loop_mode_str);
    let fugue_channel = compact.channel.unwrap_or(1).saturating_sub(1).min(15);

    let loop_mode = parse_loop_mode_str(loop_mode_str).unwrap_or(LoopMode::Forever);
    let quantize = parse_quantize_str(quantize_str).unwrap_or(QuantizeMode::Bar);
    let cancel_mode_str = compact.cancel_mode.as_deref().unwrap_or("none");
    let cancel_mode = parse_cancel_mode_str(cancel_mode_str);
    let start_mode = parse_start_mode_str(
        compact
            .start_mode
            .as_deref()
            .or(defaults.start_mode_str.as_deref()),
    );

    let mut events: Vec<TimedFugueEvent> = Vec::new();
    // Fugue-level CC interpolation defaults to Linear; single-lane CC
    // variants override to match the lane's own interpolation. Composite
    // fugues stamp curves per-event (see `emit_cc_lane`) so keeping this
    // at Linear is fine — the per-event curves win at export time.
    let mut cc_interpolation = InterpolationMode::Linear;

    match &compact.content {
        FugueContent::Notes { notes } => {
            emit_notes(notes, fugue_channel, &mut events);
        }
        FugueContent::Cc { cc, points, interpolation } => {
            let lane_mode = parse_interpolation_mode(interpolation.as_deref());
            cc_interpolation = lane_mode;
            emit_cc_lane(*cc, points, lane_mode, fugue_channel, &mut events);
        }
        FugueContent::PerNotePitchBend { note, points, interpolation } => {
            let default_mode = parse_interpolation_mode(interpolation.as_deref());
            emit_per_note_pitch_bend(note.0, points, default_mode, fugue_channel, &mut events);
        }
        FugueContent::PerNotePressure { note, points, interpolation } => {
            let default_mode = parse_interpolation_mode(interpolation.as_deref());
            emit_per_note_pressure(note.0, points, default_mode, fugue_channel, &mut events);
        }
        FugueContent::Composite { notes, cc, pitch_bends, pressures } => {
            // One fugue, multiple concerns. Each sub-lane carries its own
            // interpolation mode and gets its curve stamped onto every
            // emitted event so different CC lanes can use different curves
            // without fighting over the single cc_interpolation slot above.
            emit_notes(notes, fugue_channel, &mut events);
            for lane in cc {
                let lane_mode = parse_interpolation_mode(lane.interpolation.as_deref());
                emit_cc_lane(lane.cc, &lane.points, lane_mode, fugue_channel, &mut events);
            }
            for lane in pitch_bends {
                let lane_mode = parse_interpolation_mode(lane.interpolation.as_deref());
                emit_per_note_pitch_bend(lane.note.0, &lane.points, lane_mode, fugue_channel, &mut events);
            }
            for lane in pressures {
                let lane_mode = parse_interpolation_mode(lane.interpolation.as_deref());
                emit_per_note_pressure(lane.note.0, &lane.points, lane_mode, fugue_channel, &mut events);
            }
        }
    }

    events.sort_by(|a, b| {
        a.beat_offset.partial_cmp(&b.beat_offset).unwrap_or(std::cmp::Ordering::Equal)
    });

    let duration_beats = resolve_duration_beats(explicit_duration, &events);

    let mut definition = FugueDefinition::new(events, duration_beats)
        .with_loop_mode(loop_mode)
        .with_quantize(quantize)
        .with_cancel_mode(cancel_mode)
        .with_cc_interpolation(cc_interpolation)
        .with_start_mode(start_mode);
    if let Some(tag) = compact.tag.clone() {
        definition = definition.with_tag(tag);
    }
    definition
}

// =============================================================================
// definition_to_compact — read-back path (inverse of compact_to_definition)
//
// Returns a typed `CompactFugue` with content shaped as
// `FugueContent::Composite` — so the value can round-trip straight back
// through `queue_fugue` if the LLM mutates it and re-submits. Caller
// serializes via `serde_json::to_value` at the tool boundary.
//
// Lossy on per-note expression: the original input used sparse anchors
// (2–4 points), but the fugue scheduler expands them server-side to ~32
// events/beat. The dense stream is what shows up here. That's fine for
// "what is this fugue currently doing"; not a full round-trip of the LLM's
// original input.
// =============================================================================

/// Convert a `FugueDefinition` back into a `CompactFugue` — the same type
/// `queue_fugue` accepts on input. Notes pair by `(channel, note)`
/// matching the next NoteOff; CC events bucket by `(channel, cc)`; per-note
/// expression lanes bucket by `(channel, note)`. Lane order is stable
/// (first-seen). Resolved metadata (loop / quantize / cancel / duration)
/// always populates the CompactFugue's override slots so the read-back
/// carries the concrete state, not the "inherit from batch" emptiness.
pub fn definition_to_compact(def: &FugueDefinition) -> CompactFugue {
    let mut open_notes: HashMap<(u8, u8), VecDeque<(f64, u8)>> = HashMap::new();
    let mut notes: Vec<CompactNote> = Vec::new();
    let mut cc_lanes: HashMap<(u8, u8), Vec<Point>> = HashMap::new();
    let mut bend_lanes: HashMap<(u8, u8), Vec<Point>> = HashMap::new();
    let mut pressure_lanes: HashMap<(u8, u8), Vec<Point>> = HashMap::new();
    let mut cc_order: Vec<(u8, u8)> = Vec::new();
    let mut bend_order: Vec<(u8, u8)> = Vec::new();
    let mut pressure_order: Vec<(u8, u8)> = Vec::new();

    let push_compact_note = |notes: &mut Vec<CompactNote>,
                             on_beat: f64,
                             off_beat: f64,
                             channel: u8,
                             note: u8,
                             velocity: u8| {
        notes.push(CompactNote {
            beat: on_beat,
            note: Note(note),
            duration: (off_beat - on_beat).max(0.0),
            velocity: Some(velocity),
            // 1-indexed on the wire to match the LLM's input schema.
            channel: Some(channel.saturating_add(1).min(16)),
        });
    };

    for timed in &def.events {
        let beat = timed.beat_offset;
        match timed.event {
            FugueEvent::NoteOn { channel, note, velocity } => {
                open_notes.entry((channel, note)).or_default().push_back((beat, velocity));
            }
            FugueEvent::NoteOff { channel, note } => {
                if let Some(q) = open_notes.get_mut(&(channel, note)) {
                    if let Some((on_beat, vel)) = q.pop_front() {
                        push_compact_note(&mut notes, on_beat, beat, channel, note, vel);
                    }
                }
            }
            FugueEvent::Cc { channel, cc, value, curve } => {
                let key = (channel, cc);
                if !cc_lanes.contains_key(&key) {
                    cc_order.push(key);
                }
                cc_lanes
                    .entry(key)
                    .or_default()
                    .push(point_with_curve(beat, value as f64, curve, def.cc_interpolation));
            }
            FugueEvent::PerNotePitchBend { channel, note, semitones } => {
                let key = (channel, note);
                if !bend_lanes.contains_key(&key) {
                    bend_order.push(key);
                }
                bend_lanes
                    .entry(key)
                    .or_default()
                    .push(Point { beat, value: semitones as f64, curve: None });
            }
            FugueEvent::PerNotePressure { channel, note, pressure } => {
                let key = (channel, note);
                if !pressure_lanes.contains_key(&key) {
                    pressure_order.push(key);
                }
                pressure_lanes
                    .entry(key)
                    .or_default()
                    .push(Point { beat, value: pressure as f64, curve: None });
            }
        }
    }

    // Flush dangling note-ons against the fugue's end so held notes don't
    // disappear silently from the read-back.
    for ((channel, note), q) in open_notes {
        for (on_beat, vel) in q {
            push_compact_note(&mut notes, on_beat, def.duration_beats, channel, note, vel);
        }
    }

    let cc: Vec<CompactCc> = cc_order
        .into_iter()
        .map(|key| CompactCc {
            cc: key.1,
            points: cc_lanes.remove(&key).unwrap_or_default(),
            interpolation: Some(interp_tag(def.cc_interpolation).to_string()),
        })
        .collect();

    let pitch_bends: Vec<CompactPitchBend> = bend_order
        .into_iter()
        .map(|key| CompactPitchBend {
            note: Note(key.1),
            points: bend_lanes.remove(&key).unwrap_or_default(),
            interpolation: None,
        })
        .collect();

    let pressures: Vec<CompactPressure> = pressure_order
        .into_iter()
        .map(|key| CompactPressure {
            note: Note(key.1),
            points: pressure_lanes.remove(&key).unwrap_or_default(),
            interpolation: None,
        })
        .collect();

    CompactFugue {
        tag: def.tag.clone(),
        cancel_mode: Some(cancel_mode_tag(&def.cancel_mode)),
        channel: None,
        quantize: Some(quantize_tag(def.quantize)),
        duration_beats: Some(def.duration_beats),
        loop_mode: Some(loop_mode_tag(def.loop_mode)),
        start_mode: Some(start_mode_tag(def.start_mode).to_string()),
        content: FugueContent::Composite {
            notes,
            cc,
            pitch_bends,
            pressures,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // parse_interpolation_mode
    // ------------------------------------------------------------------

    #[test]
    fn interpolation_parser_covers_all_modes() {
        assert_eq!(parse_interpolation_mode(None), InterpolationMode::Linear);
        assert_eq!(parse_interpolation_mode(Some("linear")), InterpolationMode::Linear);
        assert_eq!(parse_interpolation_mode(Some("exp")), InterpolationMode::Exp);
        assert_eq!(parse_interpolation_mode(Some("log")), InterpolationMode::Log);
        assert_eq!(parse_interpolation_mode(Some("none")), InterpolationMode::None);
    }

    #[test]
    fn interpolation_parser_unknown_falls_back_to_linear() {
        assert_eq!(parse_interpolation_mode(Some("bogus")), InterpolationMode::Linear);
        assert_eq!(parse_interpolation_mode(Some("")), InterpolationMode::Linear);
        assert_eq!(parse_interpolation_mode(Some("Linear")), InterpolationMode::Linear);
    }

    // ------------------------------------------------------------------
    // Round-trip: CompactFugue → FugueDefinition → CompactFugue
    //
    // Verifies that a fugue the LLM writes on the input side survives the
    // full round trip through the scheduler's event stream. Tripwire for
    // silent drift between the two conversion sides — if either gets
    // refactored without updating the other, these break.
    // ------------------------------------------------------------------

    fn compact_from_json(json: serde_json::Value) -> CompactFugue {
        serde_json::from_value(json).expect("fixture must deserialize into CompactFugue")
    }

    fn defaults_bar_forever() -> QueueFugueDefaults {
        QueueFugueDefaults {
            quantize_str: "bar".into(),
            duration_beats: Some(4.0),
            loop_mode_str: "forever".into(),
            start_mode_str: None,
        }
    }

    #[test]
    fn round_trip_notes_fugue_preserves_note_count_and_tag() {
        let input = compact_from_json(serde_json::json!({
            "type": "notes",
            "tag": "melody",
            "notes": [
                { "beat": 0.0, "note": 60, "duration": 1.0 },
                { "beat": 1.0, "note": 62, "duration": 1.0 },
                { "beat": 2.0, "note": 64, "duration": 1.0 },
            ],
        }));
        let def = compact_to_definition(&input, &defaults_bar_forever());
        let out = definition_to_compact(&def);
        assert_eq!(out.tag.as_deref(), Some("melody"));
        match out.content {
            FugueContent::Composite { ref notes, .. } => {
                assert_eq!(notes.len(), 3);
                // Notes come back in beat order — same order they went in.
                assert_eq!(notes[0].note.0, 60);
                assert_eq!(notes[1].note.0, 62);
                assert_eq!(notes[2].note.0, 64);
            }
            _ => panic!("expected Composite read-back"),
        }
    }

    #[test]
    fn round_trip_cc_fugue_preserves_anchors() {
        let input = compact_from_json(serde_json::json!({
            "type": "cc",
            "tag": "filter",
            "cc": 74,
            "points": [[0.0, 30], [4.0, 110]],
        }));
        let def = compact_to_definition(&input, &defaults_bar_forever());
        let out = definition_to_compact(&def);
        match out.content {
            FugueContent::Composite { ref cc, .. } => {
                assert_eq!(cc.len(), 1);
                assert_eq!(cc[0].cc, 74);
                assert_eq!(cc[0].points.len(), 2, "expected both anchors back");
                assert_eq!(cc[0].points[0].beat, 0.0);
                assert_eq!(cc[0].points[0].value, 30.0);
                assert_eq!(cc[0].points[1].beat, 4.0);
                assert_eq!(cc[0].points[1].value, 110.0);
            }
            _ => panic!("expected Composite read-back"),
        }
    }

    #[test]
    fn round_trip_composite_populates_all_four_lanes() {
        let input = compact_from_json(serde_json::json!({
            "type": "composite",
            "tag": "pad",
            "notes": [{ "beat": 0.0, "note": 60, "duration": 4.0 }],
            "cc": [{ "cc": 74, "points": [[0.0, 30], [4.0, 100]] }],
            "pitch_bends": [{ "note": 60, "points": [[0.0, 0], [2.0, 1]] }],
            "pressures": [{ "note": 60, "points": [[0.0, 0.0], [4.0, 1.0]] }],
        }));
        let def = compact_to_definition(&input, &defaults_bar_forever());
        let out = definition_to_compact(&def);
        match out.content {
            FugueContent::Composite { notes, cc, pitch_bends, pressures } => {
                assert!(!notes.is_empty(), "notes lane lost in round-trip");
                assert!(!cc.is_empty(), "cc lane lost in round-trip");
                assert!(!pitch_bends.is_empty(), "pitch_bends lane lost in round-trip");
                assert!(!pressures.is_empty(), "pressures lane lost in round-trip");
            }
            _ => panic!("expected Composite read-back"),
        }
    }

    #[test]
    fn round_trip_duration_and_loop_mode_defaults_applied() {
        // No per-fugue overrides → batch defaults should flow through to
        // the FugueDefinition and end up tagged on the read-back.
        let input = compact_from_json(serde_json::json!({
            "type": "notes",
            "notes": [{ "beat": 0.0, "note": 60, "duration": 1.0 }],
        }));
        let defaults = QueueFugueDefaults {
            quantize_str: "bar".into(),
            duration_beats: Some(8.0),
            loop_mode_str: "once".into(),
            start_mode_str: None,
        };
        let def = compact_to_definition(&input, &defaults);
        let out = definition_to_compact(&def);
        assert_eq!(out.duration_beats, Some(8.0));
        assert_eq!(out.loop_mode.as_deref(), Some("once"));
    }

    fn defaults_unset_duration() -> QueueFugueDefaults {
        QueueFugueDefaults {
            quantize_str: "bar".into(),
            duration_beats: None,
            loop_mode_str: "forever".into(),
            start_mode_str: None,
        }
    }

    #[test]
    fn auto_duration_rounds_up_to_whole_bar() {
        // 6-beat melody → 8-beat (2-bar) pattern, not the prior 4-beat default.
        let input = compact_from_json(serde_json::json!({
            "type": "notes",
            "notes": [
                { "beat": 0.0, "note": 60, "duration": 1.0 },
                { "beat": 5.5, "note": 62, "duration": 0.5 },
            ],
        }));
        let def = compact_to_definition(&input, &defaults_unset_duration());
        assert_eq!(def.duration_beats, 8.0);
    }

    #[test]
    fn auto_duration_uses_note_off_end_not_note_on_beat() {
        // A single 5-beat note must size the pattern to 8 (2 bars), since
        // the NoteOff lands at beat 5 — not 4 (its NoteOn beat).
        let input = compact_from_json(serde_json::json!({
            "type": "notes",
            "notes": [{ "beat": 0.0, "note": 60, "duration": 5.0 }],
        }));
        let def = compact_to_definition(&input, &defaults_unset_duration());
        assert_eq!(def.duration_beats, 8.0);
    }

    #[test]
    fn auto_duration_empty_content_falls_back_to_one_bar() {
        let input = compact_from_json(serde_json::json!({
            "type": "notes",
            "notes": [],
        }));
        let def = compact_to_definition(&input, &defaults_unset_duration());
        assert_eq!(def.duration_beats, 4.0);
    }

    #[test]
    fn auto_duration_snaps_exact_bar_end_up_unchanged() {
        // A note ending exactly on a bar boundary fits in that bar — no extra
        // slack added.
        let input = compact_from_json(serde_json::json!({
            "type": "notes",
            "notes": [{ "beat": 0.0, "note": 60, "duration": 4.0 }],
        }));
        let def = compact_to_definition(&input, &defaults_unset_duration());
        assert_eq!(def.duration_beats, 4.0);
    }

    #[test]
    fn auto_duration_considers_cc_lane_last_point() {
        // No notes, but CC automation runs to beat 12 → 3-bar pattern.
        let input = compact_from_json(serde_json::json!({
            "type": "cc",
            "cc": 74,
            "points": [[0, 0], [12, 127]],
        }));
        let def = compact_to_definition(&input, &defaults_unset_duration());
        assert_eq!(def.duration_beats, 12.0);
    }

    #[test]
    fn explicit_duration_longer_than_content_is_respected() {
        // User asks for trailing silence — don't shrink.
        let input = compact_from_json(serde_json::json!({
            "type": "notes",
            "duration_beats": 16.0,
            "notes": [{ "beat": 0.0, "note": 60, "duration": 1.0 }],
        }));
        let def = compact_to_definition(&input, &defaults_unset_duration());
        assert_eq!(def.duration_beats, 16.0);
    }

    #[test]
    fn explicit_duration_too_short_is_extended_to_fit() {
        // The "LLM forgot to set duration and its default=4 truncates a
        // 6-beat melody" case this fix is meant to cover. Even when the value
        // was explicit, a too-short duration is treated as a mistake and
        // extended to the smallest whole bar that fits.
        let input = compact_from_json(serde_json::json!({
            "type": "notes",
            "duration_beats": 4.0,
            "notes": [
                { "beat": 0.0, "note": 60, "duration": 1.0 },
                { "beat": 6.0, "note": 62, "duration": 1.0 },
            ],
        }));
        let def = compact_to_definition(&input, &defaults_unset_duration());
        assert_eq!(def.duration_beats, 8.0, "7-beat content must live in an 8-beat pattern, not the explicit 4");
    }

    #[test]
    fn batch_default_duration_applies_when_per_fugue_unset() {
        // Batch-level override still works — the per-fugue resolver only
        // auto-sizes when neither layer supplied a value.
        let input = compact_from_json(serde_json::json!({
            "type": "notes",
            "notes": [{ "beat": 0.0, "note": 60, "duration": 1.0 }],
        }));
        let defaults = QueueFugueDefaults {
            quantize_str: "bar".into(),
            duration_beats: Some(16.0),
            loop_mode_str: "forever".into(),
            start_mode_str: None,
        };
        let def = compact_to_definition(&input, &defaults);
        assert_eq!(def.duration_beats, 16.0);
    }

    #[test]
    fn round_trip_through_json_produces_valid_compact_input() {
        // Output should be consumable by the input side — an LLM could
        // read a fugue, tweak it, and re-queue without schema translation.
        let input = compact_from_json(serde_json::json!({
            "type": "notes",
            "tag": "bass",
            "notes": [{ "beat": 0.0, "note": 36, "duration": 1.0 }],
        }));
        let def = compact_to_definition(&input, &defaults_bar_forever());
        let out = definition_to_compact(&def);
        let json = serde_json::to_value(&out).expect("serialize typed compact");
        // Reparse the emitted JSON as a CompactFugue — round-trip symmetry.
        let reparsed: CompactFugue = serde_json::from_value(json).expect("re-parse through CompactFugue");
        match reparsed.content {
            FugueContent::Composite { notes, .. } => {
                assert_eq!(notes.len(), 1);
                assert_eq!(notes[0].note.0, 36);
            }
            _ => panic!("expected Composite after re-parse"),
        }
    }
}
