//! Compact fugue schema — the LLM-facing shape of a single fugue.
//!
//! `FugueContent` is the polymorphic body (notes / cc / per-note expression /
//! composite); the `Compact*` lane types are what sit inside a `Composite`.
//! `CompactFugue` wraps one body with its tag / quantize / loop metadata.
//!
//! The whole module is optimized for terse LLM output: notes can be written
//! as `[beat, note, duration]` tuples, points as `[beat, value]`, and so on.
//! Custom Deserialize / Serialize impls handle the flat-tuple ↔ typed-struct
//! bridge so the rest of the code never sees JSON.

use rmcp::schemars;
use serde::Deserialize;

use super::note::Note;
use super::point::Point;

/// Content type for a fugue — notes, CC automation, or per-note MIDI 2.0 expression.
#[derive(Debug, serde::Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FugueContent {
    /// MIDI notes with automatic note-off generation
    Notes {
        /// Array of notes to play
        #[schemars(description = "Array of notes. Each note auto-generates a note_off at beat + duration.")]
        notes: Vec<CompactNote>,
    },
    /// CC automation with smooth audio-thread interpolation between points
    Cc {
        /// CC number (0-127)
        #[schemars(description = "CC number (0-127)")]
        cc: u8,
        /// Array of points. Same tuple shape as per-note: [beat, value] or [beat, value, curve].
        #[schemars(description = "Array of [beat, value] or [beat, value, curve] points. Values 0-127. The audio thread smoothly interpolates between consecutive points. NOTE: CC uses ONE curve per fugue today — the `interpolation` field wins if set; otherwise the first per-point curve is used. Per-segment curves are planned post-demo (Feature 10). Example: [[0,0],[4,127]] ramps smoothly over 4 beats.")]
        points: Vec<Point>,
        /// Interpolation mode for the entire fugue: "linear" (default), "exp", "log", or "none"
        #[serde(default)]
        #[schemars(description = "Fugue-level interpolation: 'linear' (default), 'exp' (ease-in, accelerating), 'log' (ease-out, decelerating), or 'none' (stepped). If set, overrides any per-point curves.")]
        interpolation: Option<String>,
    },
    /// Per-note pitch bend over time (MIDI 2.0). Bends a single held note.
    /// Requires a concurrent Notes fugue holding the target note.
    /// Same per-segment curve shape as CC and per-note pressure — each point's
    /// curve drives the segment arriving at it, so one fugue can combine
    /// shapes (e.g. exp up, log down). Server-side expansion at ~32 events/beat.
    PerNotePitchBend {
        /// MIDI note being bent. Must match a note currently held by a concurrent Notes fugue. Accepts name ('C4') or number (60).
        #[schemars(description = "MIDI note to bend. Must be held by a concurrent notes fugue on the same channel. Accepts name like 'C4' or 'F#3', or number 0-127.")]
        note: Note,
        /// Points forming the bend trajectory. Tuple form [beat, semitones] or [beat, semitones, curve].
        #[schemars(description = "Array of trajectory points. Each point is [beat, semitones] or [beat, semitones, curve]. Semitones -64.0 to +64.0 (0 = no bend). Per-point curve controls the segment arriving at it (ignored on first point). Example vibrato: [[0,0],[0.5,1],[1,-1],[1.5,0]].")]
        points: Vec<Point>,
        /// Fugue-level default curve for segments without a per-point curve.
        #[serde(default)]
        #[schemars(description = "Fugue-level default interpolation for segments whose points don't specify a curve. 'linear' (default), 'exp', 'log', or 'none'.")]
        interpolation: Option<String>,
    },
    /// Per-note pressure/aftertouch over time (MIDI 2.0). Modulates a single held note.
    /// Requires a concurrent Notes fugue holding the target note.
    /// Same per-segment curve shape as CC and per-note pitch bend.
    PerNotePressure {
        /// MIDI note receiving pressure. Must match a note currently held by a concurrent Notes fugue. Accepts name ('C4') or number (60).
        #[schemars(description = "MIDI note for pressure. Must be held by a concurrent notes fugue on the same channel. Accepts name like 'C4' or 'F#3', or number 0-127.")]
        note: Note,
        /// Points forming the pressure trajectory. Tuple form [beat, pressure] or [beat, pressure, curve].
        #[schemars(description = "Array of trajectory points. Each point is [beat, pressure] or [beat, pressure, curve]. Pressure 0.0-1.0. Per-point curve controls the segment arriving at it (ignored on first point). Example crescendo-then-release: [[0,0],[2,1,\"exp\"],[4,0,\"log\"]].")]
        points: Vec<Point>,
        /// Fugue-level default curve for segments without a per-point curve.
        #[serde(default)]
        #[schemars(description = "Fugue-level default interpolation for segments whose points don't specify a curve. 'linear' (default), 'exp', 'log', or 'none'.")]
        interpolation: Option<String>,
    },
    /// Composite fugue — bundle notes + cc + per-note bends + per-note pressures
    /// into a single atomic musical moment. Use this when all the parts belong
    /// to one instrument / one musical idea (e.g. a pad with held chord tones,
    /// a filter sweep, expression curves, and pressure swells). One tag, one
    /// cancel, one UI row. Prefer over multiple single-concern fugues unless
    /// the parts need independent replacement (bass swap while melody keeps
    /// playing → keep those separate).
    Composite {
        /// Notes to play. Required — at least provide an empty array if
        /// there are no notes, though a composite with zero notes is unusual.
        #[schemars(description = "Array of notes. Each auto-generates a note_off at beat + duration.")]
        notes: Vec<CompactNote>,
        /// CC lanes. One entry per CC number you want to automate.
        #[serde(default)]
        #[schemars(description = "Array of CC automation lanes. Each lane has its own cc number, points, and optional interpolation. Omit or empty if no CC automation.")]
        cc: Vec<CompactCc>,
        /// Per-note pitch-bend lanes. One entry per target note.
        #[serde(default)]
        #[schemars(description = "Array of per-note pitch-bend lanes. Each targets one held note. Omit or empty if no bends.")]
        pitch_bends: Vec<CompactPitchBend>,
        /// Per-note pressure lanes. One entry per target note.
        #[serde(default)]
        #[schemars(description = "Array of per-note pressure lanes. Each targets one held note. Omit or empty if no pressure.")]
        pressures: Vec<CompactPressure>,
    },
}

/// One CC automation lane inside a [`FugueContent::Composite`].
/// Mirrors the fields of the single-concern `Cc` variant minus the `type` tag.
#[derive(Debug, serde::Serialize, Deserialize, schemars::JsonSchema)]
pub struct CompactCc {
    /// CC number (0-127).
    #[schemars(description = "CC number (0-127). Common: 1=mod, 7=volume, 11=expression, 71=resonance, 74=filter cutoff, 91=reverb.")]
    pub cc: u8,
    /// Array of [beat, value] or [beat, value, curve] points. Values 0-127.
    #[schemars(description = "Array of [beat, value] or [beat, value, curve] points. Values 0-127.")]
    pub points: Vec<Point>,
    /// Default curve for segments without a per-point curve.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Lane-level default interpolation: 'linear' (default), 'exp', 'log', or 'none'. Per-point curves override.")]
    pub interpolation: Option<String>,
}

/// One per-note pitch-bend lane inside a [`FugueContent::Composite`].
#[derive(Debug, serde::Serialize, Deserialize, schemars::JsonSchema)]
pub struct CompactPitchBend {
    /// Target note. Must match a note held in the composite's `notes` array on the same channel.
    #[schemars(description = "Target note (name like 'C4' or number 0-127). Must match a note held in the composite's notes array on the same channel.")]
    pub note: Note,
    /// Array of [beat, semitones] or [beat, semitones, curve] points. Semitones -64.0 to +64.0.
    #[schemars(description = "Array of [beat, semitones] or [beat, semitones, curve] points. Semitones -64.0 to +64.0.")]
    pub points: Vec<Point>,
    /// Lane-level default curve.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Lane-level default interpolation: 'linear' (default), 'exp', 'log', or 'none'. Per-point curves override.")]
    pub interpolation: Option<String>,
}

/// One per-note pressure lane inside a [`FugueContent::Composite`].
#[derive(Debug, serde::Serialize, Deserialize, schemars::JsonSchema)]
pub struct CompactPressure {
    /// Target note. Must match a note held in the composite's `notes` array on the same channel.
    #[schemars(description = "Target note (name like 'C4' or number 0-127). Must match a note held in the composite's notes array on the same channel.")]
    pub note: Note,
    /// Array of [beat, pressure] or [beat, pressure, curve] points. Pressure 0.0-1.0.
    #[schemars(description = "Array of [beat, pressure] or [beat, pressure, curve] points. Pressure 0.0-1.0.")]
    pub points: Vec<Point>,
    /// Lane-level default curve.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Lane-level default interpolation: 'linear' (default), 'exp', 'log', or 'none'. Per-point curves override.")]
    pub interpolation: Option<String>,
}

/// A single note in a compact fugue.
///
/// Accepts two wire forms:
/// - **Object** (self-documenting, backward-compat):
///   `{"beat": 0, "note": "C1", "duration": 0.75, "velocity": 120}`
/// - **Array** (terse, preferred for LLMs to save tokens):
///   `[beat, note, duration?, velocity?, channel?]` — e.g. `[0, "C1", 0.75, 120]`
///
/// Trailing fields in the array form are optional with sensible defaults
/// (`duration=1`, `velocity=100`, `channel` = fugue default). A 16-note
/// drum pattern that would be ~1 KB as objects collapses to ~200 B as
/// arrays — meaningful tokens saved when the LLM iterates on big patterns.
#[derive(Debug)]
pub struct CompactNote {
    pub beat: f64,
    pub note: Note,
    pub duration: f64,
    pub velocity: Option<u8>,
    pub channel: Option<u8>,
}

impl serde::Serialize for CompactNote {
    /// Emit as an object (`{beat, note, duration, velocity?, channel?}`).
    /// The tuple form is valid input but the object form is less
    /// ambiguous on read-back and carries the field names the LLM
    /// expects to see.
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let field_count = 3
            + self.velocity.map_or(0, |_| 1)
            + self.channel.map_or(0, |_| 1);
        let mut s = serializer.serialize_struct("CompactNote", field_count)?;
        s.serialize_field("beat", &self.beat)?;
        s.serialize_field("note", &self.note)?;
        s.serialize_field("duration", &self.duration)?;
        if let Some(v) = self.velocity {
            s.serialize_field("velocity", &v)?;
        }
        if let Some(c) = self.channel {
            s.serialize_field("channel", &c)?;
        }
        s.end()
    }
}

impl<'de> serde::Deserialize<'de> for CompactNote {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::{Error, IgnoredAny, MapAccess, SeqAccess, Visitor};
        use std::fmt;

        struct CompactNoteVisitor;

        impl<'de> Visitor<'de> for CompactNoteVisitor {
            type Value = CompactNote;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str(
                    "CompactNote as either an object \
                    {beat, note, duration, velocity?, channel?} \
                    or a tuple [beat, note, duration?, velocity?, channel?]",
                )
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<CompactNote, A::Error> {
                let beat: f64 = seq
                    .next_element()?
                    .ok_or_else(|| A::Error::custom("note tuple: missing beat"))?;
                let note: Note = seq
                    .next_element()?
                    .ok_or_else(|| A::Error::custom("note tuple: missing note"))?;
                // Defaults match the previous field-level defaults so
                // [0, "C3"] behaves identically to {beat:0, note:"C3", duration:1, velocity:100}.
                let duration: f64 = seq.next_element()?.unwrap_or(1.0);
                let velocity: Option<u8> = match seq.next_element::<serde_json::Value>()? {
                    Some(serde_json::Value::Null) | None => None,
                    Some(v) => Some(serde_json::from_value(v).map_err(A::Error::custom)?),
                };
                let channel: Option<u8> = match seq.next_element::<serde_json::Value>()? {
                    Some(serde_json::Value::Null) | None => None,
                    Some(v) => Some(serde_json::from_value(v).map_err(A::Error::custom)?),
                };
                // Drain any extra elements for forward compatibility.
                while seq.next_element::<IgnoredAny>()?.is_some() {}
                Ok(CompactNote { beat, note, duration, velocity, channel })
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<CompactNote, A::Error> {
                let mut beat: Option<f64> = None;
                let mut note: Option<Note> = None;
                let mut duration: Option<f64> = None;
                let mut velocity: Option<u8> = None;
                let mut channel: Option<u8> = None;
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "beat" => beat = Some(map.next_value()?),
                        "note" => note = Some(map.next_value()?),
                        "duration" => duration = Some(map.next_value()?),
                        "velocity" => velocity = map.next_value()?,
                        "channel" => channel = map.next_value()?,
                        _ => {
                            let _: IgnoredAny = map.next_value()?;
                        }
                    }
                }
                // duration now defaults to 1.0 in both forms — the object
                // form drops its required-field requirement for symmetry
                // with the new array form. Existing callers always passed
                // duration, so this is a compatible loosening.
                Ok(CompactNote {
                    beat: beat.ok_or_else(|| A::Error::custom("note: missing beat"))?,
                    note: note.ok_or_else(|| A::Error::custom("note: missing note"))?,
                    duration: duration.unwrap_or(1.0),
                    velocity,
                    channel,
                })
            }
        }

        d.deserialize_any(CompactNoteVisitor)
    }
}

impl schemars::JsonSchema for CompactNote {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "CompactNote".into()
    }

    fn json_schema(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
        // Both wire forms are advertised to the LLM. The schema avoids
        // the JSON-Schema-draft-07 positional `items: [...]` tuple form
        // (Anthropic's API rejects it — they validate against Draft
        // 2020-12 where `items` must be a single schema). Positional
        // meaning is conveyed in the description field instead, which
        // the LLM picks up just fine.
        let value = serde_json::json!({
            "description": "A note in a fugue. PREFERRED form (terse): array [beat, note, duration?, velocity?, channel?] — e.g. [0, \"C1\", 0.75, 120]. Positional: beat (number, beat offset from fugue start), note (integer 0-127 or pitch-notation string like \"C3\", \"F#2\"), duration (number, beats — default 1), velocity (integer 1-127 or null — default 100), channel (integer 1-16 or null — default = fugue channel). Use null to skip a middle field while specifying a later one. Object form {beat, note, duration?, velocity?, channel?} is also accepted for readability.",
            "oneOf": [
                {
                    "type": "array",
                    "description": "Terse form [beat, note, duration?, velocity?, channel?]. See parent description for positional meaning.",
                    "minItems": 2,
                    "maxItems": 5,
                    "items": {
                        "anyOf": [
                            { "type": "number" },
                            { "type": "string" },
                            { "type": "null" }
                        ]
                    }
                },
                {
                    "type": "object",
                    "properties": {
                        "beat": { "type": "number", "description": "Beat offset from fugue start" },
                        "note": {
                            "oneOf": [
                                { "type": "integer", "minimum": 0, "maximum": 127 },
                                { "type": "string", "pattern": "^[A-Ga-g][#b]?-?[0-9]+$" }
                            ],
                            "description": "MIDI note (0-127) or name ('C3', 'F#2')"
                        },
                        "duration": { "type": "number", "description": "Duration in beats (default 1)" },
                        "velocity": { "type": "integer", "minimum": 1, "maximum": 127, "description": "Velocity 1-127 (default 100)" },
                        "channel": { "type": "integer", "minimum": 1, "maximum": 16, "description": "Channel 1-16 (default = fugue channel)" }
                    },
                    "required": ["beat", "note"]
                }
            ]
        });
        let map = match value {
            serde_json::Value::Object(m) => m,
            _ => unreachable!("json! literal is an object"),
        };
        schemars::Schema::from(map)
    }
}

/// A single fugue definition within a batch
#[derive(Debug, serde::Serialize, Deserialize, schemars::JsonSchema)]
pub struct CompactFugue {
    /// Tag for grouping/cancellation (e.g., "melody", "filter")
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Tag for grouping/cancellation. Use with cancel_mode:'tag:NAME' to replace specific fugues.")]
    pub tag: Option<String>,
    /// What to cancel when this fugue starts: "none", "tag:NAME", or "all"
    #[serde(default = "super::request::default_cancel_mode", skip_serializing_if = "Option::is_none")]
    #[schemars(description = "What to cancel when starting: 'none', 'tag:NAME' (cancels matching tag), or 'all'")]
    pub cancel_mode: Option<String>,
    /// Default MIDI channel for this fugue (1-16, defaults to 1)
    #[serde(default = "super::request::default_fugue_channel", skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Default MIDI channel (1-16, defaults to 1)")]
    pub channel: Option<u8>,

    // Per-fugue overrides for shared settings. On the read-back path these
    // carry the resolved concrete values (not `None` for "inherit"), so
    // the LLM sees exactly what the fugue is set to when re-queuing.
    /// Override quantize mode for this fugue
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Override quantize: 'immediate', 'beat', 'bar', or 'bars:N'")]
    pub quantize: Option<String>,
    /// Override duration for this fugue
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Override duration in beats. Omit to auto-size to the smallest whole bar that fits the content; explicit values shorter than the content are extended to fit.")]
    pub duration_beats: Option<f64>,
    /// Override loop mode for this fugue
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Override loop mode: 'once', 'forever', or a number")]
    pub loop_mode: Option<String>,
    /// How the fugue lands on the song-grid when `quantize` resolves to a
    /// target beat that isn't a multiple of `duration_beats`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Start mode: 'phase' (default — fugue joins the implicit always-running grid at the current song-phase, may start mid-pattern) or 'boundary' (wait for the next multiple of duration_beats, then play from pattern-beat-0). Iteration boundaries always sit on multiples of duration_beats from song-beat-0 regardless of this mode; it only decides what happens between queue time and the first full iteration boundary.")]
    pub start_mode: Option<String>,

    /// The fugue content (notes / CC / per-note expression)
    #[serde(flatten)]
    pub content: FugueContent,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // Note-in-context Deserialize tests (through the compact envelope)
    // ------------------------------------------------------------------

    #[test]
    fn note_deserialize_in_nested_context() {
        // Real-world: note lives inside a CompactNote lives inside a FugueContent.
        let json = r#"{
            "type": "notes",
            "notes": [
                {"beat": 0, "note": "C3",  "duration": 1},
                {"beat": 1, "note": "E3",  "duration": 1},
                {"beat": 2, "note": "G3",  "duration": 1},
                {"beat": 3, "note": 72,    "duration": 1}
            ]
        }"#;
        let content: FugueContent = serde_json::from_str(json).unwrap();
        match content {
            FugueContent::Notes { notes } => {
                assert_eq!(notes[0].note.0, 60);
                assert_eq!(notes[1].note.0, 64);
                assert_eq!(notes[2].note.0, 67);
                assert_eq!(notes[3].note.0, 72, "numeric note still works");
            }
            _ => panic!("expected Notes variant"),
        }
    }

    #[test]
    fn note_deserialize_in_per_note_variant() {
        let json = r#"{
            "type": "per_note_pitch_bend",
            "note": "C3",
            "points": [[0, 0], [1, 2]]
        }"#;
        let content: FugueContent = serde_json::from_str(json).unwrap();
        match content {
            FugueContent::PerNotePitchBend { note, .. } => assert_eq!(note.0, 60),
            _ => panic!("expected PerNotePitchBend"),
        }
    }

    // ------------------------------------------------------------------
    // FugueContent deserialization
    // ------------------------------------------------------------------

    #[test]
    fn fugue_content_per_note_pitch_bend_parses() {
        let json = r#"{
            "type": "per_note_pitch_bend",
            "note": 60,
            "points": [[0, 0], [2, 2, "exp"], [4, 0, "log"]]
        }"#;
        let content: FugueContent = serde_json::from_str(json).unwrap();
        match content {
            FugueContent::PerNotePitchBend { note, points, .. } => {
                assert_eq!(note.0, 60);
                assert_eq!(points.len(), 3);
                assert_eq!(points[1].curve.as_deref(), Some("exp"));
            }
            _ => panic!("expected PerNotePitchBend variant"),
        }
    }

    #[test]
    fn fugue_content_per_note_pressure_parses() {
        let json = r#"{
            "type": "per_note_pressure",
            "note": 72,
            "points": [[0, 0.0], [2, 1.0, "exp"], [4, 0.0, "log"]]
        }"#;
        let content: FugueContent = serde_json::from_str(json).unwrap();
        match content {
            FugueContent::PerNotePressure { note, points, .. } => {
                assert_eq!(note.0, 72);
                assert_eq!(points.len(), 3);
            }
            _ => panic!("expected PerNotePressure variant"),
        }
    }

    #[test]
    fn fugue_content_notes_still_parses() {
        let json = r#"{
            "type": "notes",
            "notes": [{"beat": 0, "note": 60, "duration": 1}]
        }"#;
        let content: FugueContent = serde_json::from_str(json).unwrap();
        matches!(content, FugueContent::Notes { .. });
    }

    #[test]
    fn fugue_content_cc_with_curves() {
        let json = r#"{
            "type": "cc",
            "cc": 74,
            "points": [[0, 0], [4, 127]],
            "interpolation": "exp"
        }"#;
        let content: FugueContent = serde_json::from_str(json).unwrap();
        match content {
            FugueContent::Cc { cc, interpolation, .. } => {
                assert_eq!(cc, 74);
                assert_eq!(interpolation.as_deref(), Some("exp"));
            }
            _ => panic!("expected Cc variant"),
        }
    }

    #[test]
    fn fugue_content_cc_accepts_3_element_point_tuples() {
        let json = r#"{
            "type": "cc",
            "cc": 74,
            "points": [[0, 40], [4, 100, "exp"], [8, 60, "log"]],
            "interpolation": "linear"
        }"#;
        let content: FugueContent = serde_json::from_str(json).unwrap();
        match content {
            FugueContent::Cc { cc, points, interpolation } => {
                assert_eq!(cc, 74);
                assert_eq!(points.len(), 3);
                assert_eq!(points[0].curve, None);
                assert_eq!(points[1].curve.as_deref(), Some("exp"));
                assert_eq!(points[2].curve.as_deref(), Some("log"));
                assert_eq!(interpolation.as_deref(), Some("linear"));
            }
            _ => panic!("expected Cc variant"),
        }
    }

    #[test]
    fn fugue_content_cc_mixed_tuple_arities_parse() {
        let json = r#"{
            "type": "cc",
            "cc": 1,
            "points": [[0, 0], [2, 64, "exp"], [4, 127]]
        }"#;
        let content: FugueContent = serde_json::from_str(json).unwrap();
        match content {
            FugueContent::Cc { points, .. } => {
                assert_eq!(points.len(), 3);
                assert_eq!(points[0].curve, None);
                assert_eq!(points[1].curve.as_deref(), Some("exp"));
                assert_eq!(points[2].curve, None);
            }
            _ => panic!("expected Cc variant"),
        }
    }

    #[test]
    fn fugue_content_per_note_interpolation_field_parses() {
        let json = r#"{
            "type": "per_note_pitch_bend",
            "note": "C3",
            "points": [[0, 0], [2, 2], [4, 0]],
            "interpolation": "exp"
        }"#;
        let content: FugueContent = serde_json::from_str(json).unwrap();
        match content {
            FugueContent::PerNotePitchBend { interpolation, .. } => {
                assert_eq!(interpolation.as_deref(), Some("exp"));
            }
            _ => panic!("expected PerNotePitchBend"),
        }
    }

    #[test]
    fn fugue_content_per_note_interpolation_optional() {
        let json = r#"{
            "type": "per_note_pressure",
            "note": "C3",
            "points": [[0, 0], [2, 1]]
        }"#;
        let content: FugueContent = serde_json::from_str(json).unwrap();
        match content {
            FugueContent::PerNotePressure { interpolation, .. } => {
                assert!(interpolation.is_none());
            }
            _ => panic!("expected PerNotePressure"),
        }
    }

    // ------------------------------------------------------------------
    // Composite fugue (notes + cc + bends + pressures in one fugue)
    // ------------------------------------------------------------------

    #[test]
    fn fugue_content_composite_full() {
        let json = r#"{
            "type": "composite",
            "notes": [
                {"beat": 0, "note": "C1", "duration": 16},
                {"beat": 0, "note": "G2", "duration": 8},
                {"beat": 8, "note": "F2", "duration": 8}
            ],
            "cc": [
                {"cc": 74, "points": [[0,30],[8,100,"exp"],[16,40,"log"]]},
                {"cc": 11, "points": [[0,50],[16,115]], "interpolation": "exp"}
            ],
            "pitch_bends": [
                {"note": "G2", "points": [[0,0],[4,2,"exp"],[8,0,"log"]]}
            ],
            "pressures": [
                {"note": "C1", "points": [[0,0],[8,0.8,"exp"],[16,0,"log"]]},
                {"note": "G2", "points": [[0,0],[4,0.6],[8,0]]}
            ]
        }"#;
        let content: FugueContent = serde_json::from_str(json).unwrap();
        match content {
            FugueContent::Composite { notes, cc, pitch_bends, pressures } => {
                assert_eq!(notes.len(), 3);
                assert_eq!(notes[0].note.0, 36);  // C1 in DAW convention (C3=60)
                assert_eq!(cc.len(), 2);
                assert_eq!(cc[0].cc, 74);
                assert_eq!(cc[1].interpolation.as_deref(), Some("exp"));
                assert_eq!(pitch_bends.len(), 1);
                assert_eq!(pitch_bends[0].note.0, 55);  // G2 in DAW convention
                assert_eq!(pressures.len(), 2);
            }
            _ => panic!("expected Composite variant"),
        }
    }

    #[test]
    fn fugue_content_composite_optional_fields_default_empty() {
        let json = r#"{
            "type": "composite",
            "notes": [{"beat": 0, "note": "C3", "duration": 1}]
        }"#;
        let content: FugueContent = serde_json::from_str(json).unwrap();
        match content {
            FugueContent::Composite { notes, cc, pitch_bends, pressures } => {
                assert_eq!(notes.len(), 1);
                assert!(cc.is_empty());
                assert!(pitch_bends.is_empty());
                assert!(pressures.is_empty());
            }
            _ => panic!("expected Composite variant"),
        }
    }

    #[test]
    fn fugue_content_composite_empty_notes_still_parses() {
        let json = r#"{
            "type": "composite",
            "notes": [],
            "cc": [{"cc": 1, "points": [[0,0],[4,64]]}]
        }"#;
        let content: FugueContent = serde_json::from_str(json).unwrap();
        match content {
            FugueContent::Composite { notes, cc, .. } => {
                assert!(notes.is_empty());
                assert_eq!(cc.len(), 1);
            }
            _ => panic!("expected Composite variant"),
        }
    }

    #[test]
    fn fugue_content_single_concern_still_works() {
        let json = r#"{
            "type": "notes",
            "notes": [{"beat": 0, "note": "C3", "duration": 1}]
        }"#;
        assert!(matches!(
            serde_json::from_str::<FugueContent>(json).unwrap(),
            FugueContent::Notes { .. }
        ));
    }

    // ------------------------------------------------------------------
    // CompactNote — array (terse) and object forms
    // ------------------------------------------------------------------

    #[test]
    fn compact_note_array_form_full() {
        let n: CompactNote = serde_json::from_str(r#"[0.5, "C1", 0.75, 120, 10]"#).unwrap();
        assert_eq!(n.beat, 0.5);
        assert_eq!(n.note.0, 36, "C1 in DAW convention = MIDI 36");
        assert_eq!(n.duration, 0.75);
        assert_eq!(n.velocity, Some(120));
        assert_eq!(n.channel, Some(10));
    }

    #[test]
    fn compact_note_array_form_defaults() {
        let n: CompactNote = serde_json::from_str(r#"[2, "D3"]"#).unwrap();
        assert_eq!(n.beat, 2.0);
        assert_eq!(n.note.0, 62);
        assert_eq!(n.duration, 1.0, "default duration");
        assert_eq!(n.velocity, None, "None surfaces as 100 in emit_notes");
        assert_eq!(n.channel, None);
    }

    #[test]
    fn compact_note_array_form_partial() {
        let n: CompactNote = serde_json::from_str(r#"[0, "E3", 0.25]"#).unwrap();
        assert_eq!(n.duration, 0.25);
        assert_eq!(n.velocity, None);

        let n: CompactNote = serde_json::from_str(r#"[1, "F3", 2, 80]"#).unwrap();
        assert_eq!(n.duration, 2.0);
        assert_eq!(n.velocity, Some(80));
        assert_eq!(n.channel, None);
    }

    #[test]
    fn compact_note_array_accepts_numeric_note() {
        let n: CompactNote = serde_json::from_str(r#"[0, 60, 1]"#).unwrap();
        assert_eq!(n.note.0, 60);
    }

    #[test]
    fn compact_note_array_explicit_null_for_velocity_keeps_channel() {
        let n: CompactNote = serde_json::from_str(r#"[0, "C3", 1, null, 5]"#).unwrap();
        assert_eq!(n.velocity, None);
        assert_eq!(n.channel, Some(5));
    }

    #[test]
    fn compact_note_object_form_still_works() {
        let n: CompactNote =
            serde_json::from_str(r#"{"beat": 0, "note": "C3", "duration": 1, "velocity": 100}"#)
                .unwrap();
        assert_eq!(n.beat, 0.0);
        assert_eq!(n.note.0, 60);
        assert_eq!(n.duration, 1.0);
        assert_eq!(n.velocity, Some(100));
    }

    #[test]
    fn compact_note_object_form_duration_now_optional() {
        let n: CompactNote =
            serde_json::from_str(r#"{"beat": 0, "note": "C3"}"#).unwrap();
        assert_eq!(n.duration, 1.0);
    }

    #[test]
    fn compact_note_array_form_in_fugue_content() {
        let json = r#"{
            "type": "notes",
            "notes": [
                [0,  "C1", 0.2, 120],
                [2,  "C1", 0.2, 115],
                [4,  "C1", 0.2, 120],
                [6,  "C1", 0.2, 110]
            ]
        }"#;
        let content: FugueContent = serde_json::from_str(json).unwrap();
        match content {
            FugueContent::Notes { notes } => {
                assert_eq!(notes.len(), 4);
                assert_eq!(notes[0].note.0, 36, "C1");
                assert_eq!(notes[3].velocity, Some(110));
            }
            _ => panic!("expected Notes variant"),
        }
    }

    #[test]
    fn compact_note_rejects_empty_array() {
        assert!(serde_json::from_str::<CompactNote>(r#"[]"#).is_err());
    }

    #[test]
    fn compact_note_rejects_missing_note_in_array() {
        assert!(serde_json::from_str::<CompactNote>(r#"[0]"#).is_err());
    }
}
