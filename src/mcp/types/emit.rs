//! Event emitters — turn compact-schema lane objects into the
//! `TimedFugueEvent` stream the scheduler consumes.
//!
//! Pure functions that push into a `&mut Vec<TimedFugueEvent>`. Kept
//! self-contained so the single-concern `FugueContent` variants and the
//! `Composite` variant both use the same code paths, and so a Composite's
//! four lanes can emit independently without borrow-checker gymnastics.

use crate::fugue::{FugueEvent, InterpolationMode, TimedFugueEvent};

use super::compact::CompactNote;
use super::conversion::parse_interpolation_mode;
use super::point::Point;

/// Density used when expanding per-note curves into discrete events.
/// ~200 Hz at 120 BPM — smooth enough for MPE-style testing while keeping
/// the event list small. If audio-thread ramps land (post-demo Feature 10)
/// this constant goes away.
pub const PER_NOTE_EXPANSION_DENSITY: f64 = 32.0;

/// Expand a per-note trajectory into discrete events using per-segment curves.
///
/// The first point is emitted as-is. For each subsequent point, the curve
/// stored on *that* point drives interpolation from the previous point's
/// value to this point's value — the "curve to this point" convention. If a
/// point doesn't specify its own curve, `default_mode` applies. Non-monotonic
/// or zero-length segments emit the endpoint only.
///
/// Intermediate values are generated at [`PER_NOTE_EXPANSION_DENSITY`]
/// events/beat and routed through [`InterpolationMode::apply_curve`] so new
/// curves added to that function pick up automatically.
pub fn expand_per_note_points(
    points: &[Point],
    default_mode: InterpolationMode,
    mut emit: impl FnMut(f64, f64),
) {
    if points.is_empty() {
        return;
    }
    emit(points[0].beat, points[0].value);
    for i in 1..points.len() {
        let p0 = &points[i - 1];
        let p1 = &points[i];
        let mode = p1.curve.as_deref()
            .map(|c| parse_interpolation_mode(Some(c)))
            .unwrap_or(default_mode);
        let segment_beats = p1.beat - p0.beat;
        if segment_beats <= 0.0 || mode == InterpolationMode::None {
            emit(p1.beat, p1.value);
            continue;
        }
        let steps = ((segment_beats * PER_NOTE_EXPANSION_DENSITY).ceil() as usize).max(1);
        for j in 1..=steps {
            let t = j as f64 / steps as f64;
            let tc = mode.apply_curve(t);
            let beat = p0.beat + t * segment_beats;
            let value = p0.value + tc * (p1.value - p0.value);
            emit(beat, value);
        }
    }
}

/// Expand a Notes array into `NoteOn` + auto-generated `NoteOff` events.
///
/// When two notes share the same `(channel, pitch)` and the later one's
/// onset falls inside the earlier's nominal sustain, the earlier note's
/// `NoteOff` is truncated to the later note's onset. MIDI has no concept
/// of two simultaneous same-pitch voices on one channel — the second
/// NoteOn retriggers and the first note effectively ends there anyway.
/// Leaving the stale `NoteOff` to fire later would kill the new voice,
/// which is the observable bug. Only triggers on real overlap, so
/// non-overlapping notes round-trip through `definition_to_compact`
/// with their original durations intact.
///
/// Zero- or negative-duration notes (either on input or after overlap
/// truncation) are dropped entirely: they produce a NoteOn + NoteOff at
/// the same beat, and the audio-thread's sample-level left-skew of
/// NoteOffs (which guards the same-sample NoteOff/NoteOn race at loop
/// and beat boundaries) would push the NoteOff strictly *before* its
/// own NoteOn. Rather than emit a malformed pair, just skip the note —
/// a zero-length note was never going to make sound anyway.
pub fn emit_notes(
    notes: &[CompactNote],
    fugue_channel: u8,
    events: &mut Vec<TimedFugueEvent>,
) {
    let resolve_channel = |n: &CompactNote| -> u8 {
        n.channel
            .map(|c| c.saturating_sub(1).min(15))
            .unwrap_or(fugue_channel)
    };
    for (i, note) in notes.iter().enumerate() {
        if note.duration <= 0.0 {
            continue;
        }
        let channel = resolve_channel(note);
        let velocity = note.velocity.unwrap_or(100).clamp(1, 127);
        let note_num = note.note.0.min(127);

        let mut off_beat = note.beat + note.duration;
        for (j, other) in notes.iter().enumerate() {
            if i == j {
                continue;
            }
            if resolve_channel(other) == channel
                && other.note.0.min(127) == note_num
                && other.beat > note.beat
                && other.beat < off_beat
            {
                off_beat = other.beat;
            }
        }

        if off_beat <= note.beat {
            continue;
        }

        events.push(TimedFugueEvent::new(
            note.beat,
            FugueEvent::NoteOn { channel, note: note_num, velocity },
        ));
        events.push(TimedFugueEvent::new(
            off_beat,
            FugueEvent::NoteOff { channel, note: note_num },
        ));
    }
}

/// Emit CC events for one lane, attaching each point's curve to the event.
///
/// `lane_default_mode` fills in when a point has no per-point curve. CC events
/// always carry an explicit `Some(curve)` so multi-lane composites can run
/// different curves per lane — the single `FugueDefinition::cc_interpolation`
/// slot only holds one mode, so we bypass it and stamp each event directly.
pub fn emit_cc_lane(
    cc_num: u8,
    points: &[Point],
    lane_default_mode: InterpolationMode,
    fugue_channel: u8,
    events: &mut Vec<TimedFugueEvent>,
) {
    let cc_num = cc_num.min(127);
    for point in points {
        let value = (point.value as u8).min(127);
        let resolved = point.curve.as_deref()
            .map(|c| parse_interpolation_mode(Some(c)))
            .unwrap_or(lane_default_mode);
        events.push(TimedFugueEvent::new(
            point.beat,
            FugueEvent::Cc {
                channel: fugue_channel,
                cc: cc_num,
                value,
                curve: Some(resolved),
            },
        ));
    }
}

/// Expand a per-note pitch-bend trajectory into dense discrete events.
pub fn emit_per_note_pitch_bend(
    note: u8,
    points: &[Point],
    default_mode: InterpolationMode,
    fugue_channel: u8,
    events: &mut Vec<TimedFugueEvent>,
) {
    let n = note.min(127);
    expand_per_note_points(points, default_mode, |beat, value| {
        let semitones = (value as f32).clamp(-64.0, 64.0);
        events.push(TimedFugueEvent::new(
            beat,
            FugueEvent::PerNotePitchBend { channel: fugue_channel, note: n, semitones },
        ));
    });
}

/// Expand a per-note pressure trajectory into dense discrete events.
pub fn emit_per_note_pressure(
    note: u8,
    points: &[Point],
    default_mode: InterpolationMode,
    fugue_channel: u8,
    events: &mut Vec<TimedFugueEvent>,
) {
    let n = note.min(127);
    expand_per_note_points(points, default_mode, |beat, value| {
        let pressure = (value as f32).clamp(0.0, 1.0);
        events.push(TimedFugueEvent::new(
            beat,
            FugueEvent::PerNotePressure { channel: fugue_channel, note: n, pressure },
        ));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::types::note::Note;

    fn collect_expansion(pts: &[Point]) -> Vec<(f64, f64)> {
        collect_expansion_with(pts, InterpolationMode::Linear)
    }

    fn collect_expansion_with(pts: &[Point], default_mode: InterpolationMode) -> Vec<(f64, f64)> {
        let mut out = Vec::new();
        expand_per_note_points(pts, default_mode, |b, v| out.push((b, v)));
        out
    }

    fn pt(beat: f64, value: f64, curve: Option<&str>) -> Point {
        Point { beat, value, curve: curve.map(|s| s.to_string()) }
    }

    #[test]
    fn expand_empty_emits_nothing() {
        let out = collect_expansion(&[]);
        assert!(out.is_empty());
    }

    #[test]
    fn expand_single_point_emits_that_point() {
        let out = collect_expansion(&[pt(0.5, 0.7, None)]);
        assert_eq!(out, vec![(0.5, 0.7)]);
    }

    #[test]
    fn expand_linear_ramp_hits_anchor_endpoints() {
        let pts = [pt(0.0, 0.0, None), pt(1.0, 1.0, Some("linear"))];
        let out = collect_expansion(&pts);
        // 1-beat ramp at 32/beat = 32 segment steps + 1 anchor point.
        assert_eq!(out.len(), 33);
        assert_eq!(out[0], (0.0, 0.0));
        let (last_b, last_v) = *out.last().unwrap();
        assert!((last_b - 1.0).abs() < 1e-9);
        assert!((last_v - 1.0).abs() < 1e-9);
    }

    #[test]
    fn expand_linear_midpoint_is_half() {
        let pts = [pt(0.0, 0.0, None), pt(1.0, 1.0, Some("linear"))];
        let out = collect_expansion(&pts);
        let (mid_b, mid_v) = out[16];
        assert!((mid_b - 0.5).abs() < 1e-9);
        assert!((mid_v - 0.5).abs() < 1e-9);
    }

    #[test]
    fn expand_exp_midpoint_is_quadratic() {
        // Exp curve: t² → midpoint (t=0.5) yields 0.25.
        let pts = [pt(0.0, 0.0, None), pt(1.0, 1.0, Some("exp"))];
        let out = collect_expansion(&pts);
        let (_, mid_v) = out[16];
        assert!((mid_v - 0.25).abs() < 1e-9);
    }

    #[test]
    fn expand_log_midpoint_is_complementary_quadratic() {
        // Log curve: 1-(1-t)² → midpoint (t=0.5) yields 0.75.
        let pts = [pt(0.0, 0.0, None), pt(1.0, 1.0, Some("log"))];
        let out = collect_expansion(&pts);
        let (_, mid_v) = out[16];
        assert!((mid_v - 0.75).abs() < 1e-9);
    }

    #[test]
    fn expand_none_is_stepped() {
        let pts = [pt(0.0, 0.0, None), pt(1.0, 1.0, Some("none"))];
        let out = collect_expansion(&pts);
        assert_eq!(out, vec![(0.0, 0.0), (1.0, 1.0)]);
    }

    #[test]
    fn expand_default_curve_is_linear() {
        let implicit = [pt(0.0, 0.0, None), pt(1.0, 1.0, None)];
        let explicit = [pt(0.0, 0.0, None), pt(1.0, 1.0, Some("linear"))];
        assert_eq!(collect_expansion(&implicit), collect_expansion(&explicit));
    }

    #[test]
    fn expand_zero_length_segment_emits_endpoint_only() {
        let pts = [pt(1.0, 0.0, None), pt(1.0, 1.0, Some("linear"))];
        let out = collect_expansion(&pts);
        assert_eq!(out.len(), 2);
        assert_eq!(out[1], (1.0, 1.0));
    }

    #[test]
    fn expand_negative_length_segment_emits_endpoint_only() {
        let pts = [pt(2.0, 0.0, None), pt(1.0, 1.0, Some("linear"))];
        let out = collect_expansion(&pts);
        assert_eq!(out.len(), 2);
        assert_eq!(out[1], (1.0, 1.0));
    }

    #[test]
    fn expand_per_segment_curves_compose() {
        let pts = [
            pt(0.0, 0.0, None),
            pt(2.0, 1.0, Some("exp")),
            pt(4.0, 0.0, Some("log")),
        ];
        let out = collect_expansion(&pts);
        let at_beat_1 = out.iter().find(|(b, _)| (b - 1.0).abs() < 1e-6).unwrap();
        assert!(
            (at_beat_1.1 - 0.25).abs() < 1e-6,
            "exp midpoint of 0→1 should be 0.25, got {}", at_beat_1.1
        );
        let at_beat_3 = out.iter().find(|(b, _)| (b - 3.0).abs() < 1e-6).unwrap();
        assert!(
            (at_beat_3.1 - 0.25).abs() < 1e-6,
            "log midpoint of 1→0 should be 0.25 (fast fall early), got {}", at_beat_3.1
        );
    }

    #[test]
    fn expand_long_segment_scales_step_count() {
        let pts = [pt(0.0, 0.0, None), pt(4.0, 1.0, Some("linear"))];
        let out = collect_expansion(&pts);
        assert_eq!(out.len(), 129); // 1 anchor + 128 steps.
    }

    #[test]
    fn expand_unknown_curve_falls_back_to_linear() {
        let bogus = [pt(0.0, 0.0, None), pt(1.0, 1.0, Some("wobble"))];
        let linear = [pt(0.0, 0.0, None), pt(1.0, 1.0, Some("linear"))];
        assert_eq!(collect_expansion(&bogus), collect_expansion(&linear));
    }

    #[test]
    fn expand_first_point_curve_is_ignored() {
        let without = [pt(0.0, 0.0, None), pt(1.0, 1.0, Some("linear"))];
        let with_first_curve = [pt(0.0, 0.0, Some("exp")), pt(1.0, 1.0, Some("linear"))];
        assert_eq!(collect_expansion(&without), collect_expansion(&with_first_curve));
    }

    #[test]
    fn expand_uses_default_mode_when_point_has_no_curve() {
        let pts = [pt(0.0, 0.0, None), pt(1.0, 1.0, None)];
        let with_default_linear = collect_expansion_with(&pts, InterpolationMode::Linear);
        let with_default_exp = collect_expansion_with(&pts, InterpolationMode::Exp);
        assert!((with_default_linear[16].1 - 0.5).abs() < 1e-9);
        assert!((with_default_exp[16].1 - 0.25).abs() < 1e-9);
    }

    #[test]
    fn expand_per_point_curve_overrides_default() {
        let pts = [pt(0.0, 0.0, None), pt(1.0, 1.0, Some("exp"))];
        let out = collect_expansion_with(&pts, InterpolationMode::Log);
        assert!((out[16].1 - 0.25).abs() < 1e-9);
    }

    // ------------------------------------------------------------------
    // Emit helpers (MCP request → TimedFugueEvent)
    // ------------------------------------------------------------------

    #[test]
    fn emit_notes_generates_paired_on_off_events() {
        let notes = vec![CompactNote {
            beat: 0.0,
            note: Note(60),
            duration: 1.0,
            velocity: Some(100),
            channel: None,
        }];
        let mut events = Vec::new();
        emit_notes(&notes, 0, &mut events);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].beat_offset, 0.0);
        assert_eq!(events[1].beat_offset, 1.0);
        assert!(matches!(events[0].event, FugueEvent::NoteOn { note: 60, .. }));
        assert!(matches!(events[1].event, FugueEvent::NoteOff { note: 60, .. }));
    }

    #[test]
    fn emit_notes_truncates_overlap_between_same_pitch_notes() {
        // Note A covers beats [1, 3); note B starts at beat 2 on the same
        // pitch. A's NoteOff must be pulled in to 2 so it doesn't kill B.
        let notes = vec![
            CompactNote { beat: 1.0, note: Note(60), duration: 2.0, velocity: None, channel: None },
            CompactNote { beat: 2.0, note: Note(60), duration: 1.0, velocity: None, channel: None },
        ];
        let mut events = Vec::new();
        emit_notes(&notes, 0, &mut events);
        // Find A's NoteOff (the one originating from the note at beat 1).
        let a_off = events.iter().find(|e| {
            matches!(e.event, FugueEvent::NoteOff { note: 60, .. })
                && e.beat_offset <= 2.0 + f64::EPSILON
        });
        let a_off = a_off.expect("note A's NoteOff missing");
        assert_eq!(a_off.beat_offset, 2.0, "A's NoteOff should truncate to B's onset, not its nominal 3.0");
    }

    #[test]
    fn emit_notes_preserves_duration_for_non_overlapping_same_pitch_notes() {
        // Note A: [1, 2) — ends before B begins at 2.5. Different timing,
        // no overlap, nothing to truncate.
        let notes = vec![
            CompactNote { beat: 1.0, note: Note(60), duration: 1.0, velocity: None, channel: None },
            CompactNote { beat: 2.5, note: Note(60), duration: 1.0, velocity: None, channel: None },
        ];
        let mut events = Vec::new();
        emit_notes(&notes, 0, &mut events);
        let offs: Vec<f64> = events
            .iter()
            .filter(|e| matches!(e.event, FugueEvent::NoteOff { note: 60, .. }))
            .map(|e| e.beat_offset)
            .collect();
        // Both NoteOffs intact: A at 2.0, B at 3.5.
        assert!(offs.contains(&2.0), "A's NoteOff at nominal 2.0, got {:?}", offs);
        assert!(offs.contains(&3.5), "B's NoteOff at nominal 3.5, got {:?}", offs);
    }

    #[test]
    fn emit_notes_overlap_on_different_pitches_is_not_truncated() {
        // Chord — A on C3, B on E3 starting mid-A. Independent pitches
        // coexist; neither NoteOff shifts.
        let notes = vec![
            CompactNote { beat: 0.0, note: Note(60), duration: 4.0, velocity: None, channel: None },
            CompactNote { beat: 2.0, note: Note(64), duration: 2.0, velocity: None, channel: None },
        ];
        let mut events = Vec::new();
        emit_notes(&notes, 0, &mut events);
        let off_60 = events.iter().find(|e| matches!(e.event, FugueEvent::NoteOff { note: 60, .. })).unwrap();
        assert_eq!(off_60.beat_offset, 4.0);
    }

    #[test]
    fn emit_notes_drops_zero_duration_notes() {
        // A zero-duration note would produce NoteOn + NoteOff at the same
        // beat; the audio-thread's sample-level NoteOff left-skew would
        // then push the OFF before its own ON. Drop the pair instead.
        let notes = vec![
            CompactNote { beat: 0.0, note: Note(60), duration: 0.0, velocity: None, channel: None },
            CompactNote { beat: 1.0, note: Note(62), duration: 1.0, velocity: None, channel: None },
        ];
        let mut events = Vec::new();
        emit_notes(&notes, 0, &mut events);
        // Only the valid note round-trips; the zero-duration note is gone.
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0].event, FugueEvent::NoteOn { note: 62, .. }));
    }

    #[test]
    fn emit_notes_drops_notes_whose_overlap_truncation_reaches_zero() {
        // Two same-pitch notes starting at the same beat — truncation
        // would pull the first's NoteOff onto its own NoteOn. Drop it
        // rather than emit a zero-length pair.
        let notes = vec![
            CompactNote { beat: 1.0, note: Note(60), duration: 2.0, velocity: None, channel: None },
            // Later in the list, same beat and pitch — logically a dup.
            CompactNote { beat: 1.5, note: Note(60), duration: 1.0, velocity: None, channel: None },
            CompactNote { beat: 1.0, note: Note(60), duration: 3.0, velocity: None, channel: None },
        ];
        let mut events = Vec::new();
        emit_notes(&notes, 0, &mut events);
        // Emitted NoteOns should not include a phantom pair for the third
        // note (beat=1.0, duration=3.0) whose NoteOff got truncated to 1.5
        // but whose sibling at beat=1.0 also got truncated to 1.5. Both
        // valid; neither should produce a zero-length pair.
        let on_count = events
            .iter()
            .filter(|e| matches!(e.event, FugueEvent::NoteOn { .. }))
            .count();
        let off_count = events
            .iter()
            .filter(|e| matches!(e.event, FugueEvent::NoteOff { .. }))
            .count();
        assert_eq!(on_count, off_count, "every NoteOn needs a matching NoteOff");
    }

    #[test]
    fn emit_notes_respects_channel_override() {
        // Note-level channel override wins over fugue-level channel.
        let notes = vec![CompactNote {
            beat: 0.0,
            note: Note(60),
            duration: 1.0,
            velocity: Some(100),
            channel: Some(5),  // 1-indexed → channel 4 internally
        }];
        let mut events = Vec::new();
        emit_notes(&notes, 0, &mut events);
        match events[0].event {
            FugueEvent::NoteOn { channel, .. } => assert_eq!(channel, 4),
            _ => panic!("expected NoteOn"),
        }
    }

    #[test]
    fn emit_cc_lane_stamps_curve_on_every_event() {
        let points = vec![
            Point { beat: 0.0, value: 0.0, curve: None },
            Point { beat: 4.0, value: 127.0, curve: Some("exp".into()) },
        ];
        let mut events = Vec::new();
        emit_cc_lane(74, &points, InterpolationMode::Log, 0, &mut events);
        assert_eq!(events.len(), 2);
        match events[0].event {
            FugueEvent::Cc { curve, .. } => {
                assert_eq!(curve, Some(InterpolationMode::Log), "bare point uses lane default");
            }
            _ => panic!("expected Cc"),
        }
        match events[1].event {
            FugueEvent::Cc { curve, .. } => {
                assert_eq!(curve, Some(InterpolationMode::Exp), "per-point curve wins");
            }
            _ => panic!("expected Cc"),
        }
    }

    #[test]
    fn emit_cc_lane_clamps_value_and_cc_number() {
        let points = vec![
            Point { beat: 0.0, value: 200.0, curve: None },
            Point { beat: 1.0, value: -5.0, curve: None },
        ];
        let mut events = Vec::new();
        emit_cc_lane(200, &points, InterpolationMode::Linear, 0, &mut events);
        match events[0].event {
            FugueEvent::Cc { cc, value, .. } => {
                assert_eq!(cc, 127, "cc number clamped");
                assert_eq!(value, 127, "value clamped");
            }
            _ => panic!("expected Cc"),
        }
    }

    #[test]
    fn emit_per_note_pitch_bend_expands_segments() {
        let points = vec![
            Point { beat: 0.0, value: 0.0, curve: None },
            Point { beat: 1.0, value: 2.0, curve: None },
        ];
        let mut events = Vec::new();
        emit_per_note_pitch_bend(60, &points, InterpolationMode::Linear, 0, &mut events);
        assert_eq!(events.len(), 33);
        let last = events.last().unwrap();
        match last.event {
            FugueEvent::PerNotePitchBend { note, semitones, .. } => {
                assert_eq!(note, 60);
                assert!((semitones - 2.0).abs() < 1e-5);
            }
            _ => panic!("expected PerNotePitchBend"),
        }
    }

    #[test]
    fn emit_per_note_pressure_clamps_to_unit_range() {
        let points = vec![
            Point { beat: 0.0, value: -0.5, curve: None },
            Point { beat: 1.0, value: 1.5, curve: None },
        ];
        let mut events = Vec::new();
        emit_per_note_pressure(60, &points, InterpolationMode::Linear, 0, &mut events);
        for e in &events {
            if let FugueEvent::PerNotePressure { pressure, .. } = e.event {
                assert!((0.0..=1.0).contains(&pressure),
                    "pressure {} out of range", pressure);
            }
        }
    }
}
