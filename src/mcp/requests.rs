//! Request types and input parsing for the Simply Droplets MCP server.
//!
//! Everything here is about turning MCP-wire JSON into typed Rust values:
//! data structs with `#[derive(Deserialize, JsonSchema)]`, the
//! [`InstanceRequest`] wrapper, the flat-tuple [`Point`] with its
//! custom Deserialize + JsonSchema, and the helpers that turn LLM-friendly
//! compact inputs into dense event streams.
//!
//! [`super::server`] stays focused on MCP tool routing and imports from here.

use rmcp::schemars;
use serde::Deserialize;

use crate::fugue::{FugueEvent, InterpolationMode, TimedFugueEvent};

// =============================================================================
// Note — MIDI note accepting number (0-127) or name ("C4", "F#3", "Bb5")
// =============================================================================

/// A MIDI note. Deserializes from either an integer (0-127) or a note name
/// in scientific pitch notation (`C4` = middle C = 60, `F#3`, `Bb5`, `C-1`).
///
/// Internally a plain `u8` — parse at the MCP boundary, use `.0` everywhere
/// downstream. The LLM and humans both reason about notes by name far better
/// than by number; this type lets that ergonomics win happen without touching
/// any audio-thread code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Note(pub u8);

impl std::fmt::Display for Note {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&midi_to_name(self.0))
    }
}

impl From<Note> for u8 {
    fn from(n: Note) -> u8 { n.0 }
}

impl<'de> serde::Deserialize<'de> for Note {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct NoteVisitor;
        impl<'de> serde::de::Visitor<'de> for NoteVisitor {
            type Value = Note;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("MIDI note: integer 0-127 or name like 'C4', 'F#3', 'Bb5', 'C-1'")
            }

            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Note, E> {
                if v > 127 {
                    Err(E::custom(format!("MIDI note {} out of range 0-127", v)))
                } else {
                    Ok(Note(v as u8))
                }
            }

            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Note, E> {
                if !(0..=127).contains(&v) {
                    Err(E::custom(format!("MIDI note {} out of range 0-127", v)))
                } else {
                    Ok(Note(v as u8))
                }
            }

            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Note, E> {
                // Accept integer-valued floats (60.0) but reject fractional notes.
                let n = v.trunc();
                if (n - v).abs() > 1e-6 {
                    Err(E::custom(format!("MIDI note must be an integer, got {}", v)))
                } else {
                    self.visit_i64(n as i64)
                }
            }

            fn visit_str<E: serde::de::Error>(self, s: &str) -> Result<Note, E> {
                let t = s.trim();
                // Be permissive: if the string is a plain number, accept it
                // as the integer form. This is forgiving for callers who
                // serialize ints as strings ("60" → Note(60)).
                if !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit() || b == b'-' || b == b'+') {
                    if let Ok(n) = t.parse::<i32>() {
                        return self.visit_i64(n as i64);
                    }
                }
                parse_note_name(t).map(Note).map_err(E::custom)
            }

            fn visit_string<E: serde::de::Error>(self, s: String) -> Result<Note, E> {
                self.visit_str(&s)
            }
        }
        deserializer.deserialize_any(NoteVisitor)
    }
}

impl schemars::JsonSchema for Note {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Note".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let value = serde_json::json!({
            "description": "MIDI note. Accepts either an integer 0-127 or a note name in scientific pitch notation: 'C4' (middle C = 60), 'F#3', 'Bb5', 'C-1' (MIDI 0), 'G9' (MIDI 127). Letter is case-insensitive; '#' = sharp, 'b' = flat. Prefer names over numbers — they're clearer for both humans and LLMs.",
            "oneOf": [
                { "type": "integer", "minimum": 0, "maximum": 127 },
                { "type": "string", "pattern": "^[A-Ga-g][#b]?-?[0-9]+$" }
            ]
        });
        let map = match value {
            serde_json::Value::Object(m) => m,
            _ => unreachable!("json! literal is an object"),
        };
        schemars::Schema::from(map)
    }
}

/// Parse a scientific-pitch-notation note name into its MIDI note number.
///
/// Accepts `C4`, `F#3`, `Bb5`, `C-1` (lowest MIDI note), `G9` (highest).
/// Letter is case-insensitive. `#` means sharp, `b` means flat. Octaves
/// follow the convention where middle C (MIDI 60) is `C4` and MIDI 0 is `C-1`.
pub fn parse_note_name(s: &str) -> Result<u8, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("empty note name".into());
    }
    let bytes = s.as_bytes();

    let semitone: i32 = match bytes[0].to_ascii_uppercase() {
        b'C' => 0,
        b'D' => 2,
        b'E' => 4,
        b'F' => 5,
        b'G' => 7,
        b'A' => 9,
        b'B' => 11,
        _ => return Err(format!("invalid note letter in '{}'", s)),
    };

    // Accidental: '#' = sharp, 'b'/'B' = flat. Case-insensitive so that
    // "DB4" works as well as "Db4" and "db4". This creates a slight
    // ambiguity with the note letter B (e.g. "BB4" parses as B-flat 4,
    // not B-natural in octave B4), but in practice nobody writes a note
    // letter and accidental in two uppercase letters unless they mean a
    // flat, so the case-insensitive reading is the safer bet.
    let mut pos = 1;
    let accidental: i32 = match bytes.get(pos).copied() {
        Some(b'#') => { pos += 1; 1 }
        Some(b'b') | Some(b'B') => { pos += 1; -1 }
        _ => 0,
    };

    let octave_str = &s[pos..];
    let octave: i32 = octave_str
        .parse()
        .map_err(|_| format!("invalid octave in '{}'", s))?;

    // MIDI convention: C-1 = 0, C0 = 12, C4 = 60 (middle C), G9 = 127.
    let midi = (octave + 1) * 12 + semitone + accidental;
    if !(0..=127).contains(&midi) {
        return Err(format!("note '{}' maps to MIDI {} (out of range 0-127)", s, midi));
    }
    Ok(midi as u8)
}

/// Format a MIDI note number as a scientific-pitch-notation name.
/// Uses sharps for enharmonic names (C# rather than Db) — the common default.
pub fn midi_to_name(note: u8) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let octave = (note as i32) / 12 - 1;
    let name = NAMES[(note % 12) as usize];
    format!("{}{}", name, octave)
}

// =============================================================================
// Wrapper type for instance-targeted requests
// =============================================================================

/// Wrapper for requests that target a specific plugin instance.
/// This enables future batching of multiple operations for the same instance.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct InstanceRequest<T> {
    /// Target plugin instance name or "default" for first available
    #[serde(default = "default_instance")]
    #[schemars(description = "Target plugin instance name or 'default' for first available")]
    pub instance: String,

    /// The actual request data
    #[serde(flatten)]
    pub data: T,
}

// =============================================================================
// MIDI message types (can be used standalone or in batches)
// =============================================================================

/// MIDI CC data
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CcData {
    /// MIDI channel (1-16, default: 1)
    #[serde(default = "default_channel")]
    #[schemars(description = "MIDI channel (1-16, default: 1)")]
    pub channel: u8,

    /// CC number (0-127)
    #[schemars(description = "CC number (0-127)")]
    pub cc: u8,

    /// CC value (0-127)
    #[schemars(description = "CC value (0-127)")]
    pub value: u8,
}

// =============================================================================
// Slot/parameter types
// =============================================================================

/// Set parameter slot value data
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SetParamData {
    /// Slot index (0-15)
    #[schemars(description = "Parameter slot index (0-15)")]
    pub slot: usize,

    /// Value (0.0-1.0 normalized)
    #[schemars(description = "Parameter value (0.0-1.0 normalized)")]
    pub value: f64,
}

/// Rename parameter slot data
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RenameSlotData {
    /// Slot index (0-15)
    #[schemars(description = "Parameter slot index (0-15)")]
    pub slot: usize,

    /// New name for the slot (e.g., "Vital Filter Cutoff")
    #[schemars(description = "New name for the slot (e.g., 'Vital Filter Cutoff')")]
    pub name: String,
}

// =============================================================================
// Per-note expression types (MIDI 2.0 only) — immediate (non-fugue) variants
// =============================================================================

/// Per-note pitch bend data (MIDI 2.0)
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PerNotePitchBendData {
    /// MIDI channel (1-16, default: 1)
    #[serde(default = "default_channel")]
    #[schemars(description = "MIDI channel (1-16, default: 1)")]
    pub channel: u8,

    /// MIDI note number to bend (0-127, where 60 = C4/middle C)
    #[schemars(description = "MIDI note number to bend (0-127, where 60 = C4/middle C)")]
    pub note: Note,

    /// Pitch bend in semitones (-64.0 to +64.0, 0 = no bend)
    #[schemars(description = "Pitch bend in semitones (-64.0 to +64.0, 0 = no bend)")]
    pub semitones: f32,
}

/// Per-note pressure/aftertouch data (MIDI 2.0)
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PerNotePressureData {
    /// MIDI channel (1-16, default: 1)
    #[serde(default = "default_channel")]
    #[schemars(description = "MIDI channel (1-16, default: 1)")]
    pub channel: u8,

    /// MIDI note. Accepts name ('C4' = middle C = 60, 'F#3', 'Bb5') or number (0-127).
    #[schemars(description = "MIDI note. Prefer name ('C4', 'F#3', 'Bb5') — also accepts number 0-127 (60 = middle C).")]
    pub note: Note,

    /// Pressure value (0.0-1.0 normalized)
    #[schemars(description = "Pressure value (0.0-1.0 normalized)")]
    pub pressure: f32,
}

/// Per-note controller data (MIDI 2.0)
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PerNoteControllerData {
    /// MIDI channel (1-16, default: 1)
    #[serde(default = "default_channel")]
    #[schemars(description = "MIDI channel (1-16, default: 1)")]
    pub channel: u8,

    /// MIDI note. Accepts name ('C4' = middle C = 60, 'F#3', 'Bb5') or number (0-127).
    #[schemars(description = "MIDI note. Prefer name ('C4', 'F#3', 'Bb5') — also accepts number 0-127 (60 = middle C).")]
    pub note: Note,

    /// Controller index (0-255)
    #[schemars(description = "Controller index (0-255)")]
    pub index: u8,

    /// Controller value (0.0-1.0 normalized)
    #[schemars(description = "Controller value (0.0-1.0 normalized)")]
    pub value: f32,
}

/// Per-note management data (MIDI 2.0)
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PerNoteManagementData {
    /// MIDI channel (1-16, default: 1)
    #[serde(default = "default_channel")]
    #[schemars(description = "MIDI channel (1-16, default: 1)")]
    pub channel: u8,

    /// MIDI note. Accepts name ('C4' = middle C = 60, 'F#3', 'Bb5') or number (0-127).
    #[schemars(description = "MIDI note. Prefer name ('C4', 'F#3', 'Bb5') — also accepts number 0-127 (60 = middle C).")]
    pub note: Note,

    /// Detach this note from prior note-on (default: false)
    #[serde(default)]
    #[schemars(description = "Detach this note from prior note-on")]
    pub detach: bool,

    /// Reset all controllers on this note (default: false)
    #[serde(default)]
    #[schemars(description = "Reset all controllers on this note")]
    pub reset: bool,
}

// =============================================================================
// Fugue sequencing types — compact format for LLM efficiency
// =============================================================================

/// Content type for a fugue — notes, CC automation, or per-note MIDI 2.0 expression.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
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
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CompactCc {
    /// CC number (0-127).
    #[schemars(description = "CC number (0-127). Common: 1=mod, 7=volume, 11=expression, 71=resonance, 74=filter cutoff, 91=reverb.")]
    pub cc: u8,
    /// Array of [beat, value] or [beat, value, curve] points. Values 0-127.
    #[schemars(description = "Array of [beat, value] or [beat, value, curve] points. Values 0-127.")]
    pub points: Vec<Point>,
    /// Default curve for segments without a per-point curve.
    #[serde(default)]
    #[schemars(description = "Lane-level default interpolation: 'linear' (default), 'exp', 'log', or 'none'. Per-point curves override.")]
    pub interpolation: Option<String>,
}

/// One per-note pitch-bend lane inside a [`FugueContent::Composite`].
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CompactPitchBend {
    /// Target note. Must match a note held in the composite's `notes` array on the same channel.
    #[schemars(description = "Target note (name like 'C4' or number 0-127). Must match a note held in the composite's notes array on the same channel.")]
    pub note: Note,
    /// Array of [beat, semitones] or [beat, semitones, curve] points. Semitones -64.0 to +64.0.
    #[schemars(description = "Array of [beat, semitones] or [beat, semitones, curve] points. Semitones -64.0 to +64.0.")]
    pub points: Vec<Point>,
    /// Lane-level default curve.
    #[serde(default)]
    #[schemars(description = "Lane-level default interpolation: 'linear' (default), 'exp', 'log', or 'none'. Per-point curves override.")]
    pub interpolation: Option<String>,
}

/// One per-note pressure lane inside a [`FugueContent::Composite`].
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CompactPressure {
    /// Target note. Must match a note held in the composite's `notes` array on the same channel.
    #[schemars(description = "Target note (name like 'C4' or number 0-127). Must match a note held in the composite's notes array on the same channel.")]
    pub note: Note,
    /// Array of [beat, pressure] or [beat, pressure, curve] points. Pressure 0.0-1.0.
    #[schemars(description = "Array of [beat, pressure] or [beat, pressure, curve] points. Pressure 0.0-1.0.")]
    pub points: Vec<Point>,
    /// Lane-level default curve.
    #[serde(default)]
    #[schemars(description = "Lane-level default interpolation: 'linear' (default), 'exp', 'log', or 'none'. Per-point curves override.")]
    pub interpolation: Option<String>,
}

/// A single point in a per-note continuous-signal trajectory.
///
/// Serialized as a flat 2- or 3-element array `[beat, value]` or
/// `[beat, value, curve]`. The tuple form keeps LLM output tokens low
/// (no repeated field names) while still allowing per-segment curves.
///
/// `curve` controls interpolation for the segment **arriving at** this point —
/// the "curve to this point" convention. It is ignored on the first point.
/// This lets one fugue combine distinct curves, e.g. `exp` on the way up and
/// `log` on the way down for a crescendo–release shape.
#[derive(Debug, Clone)]
pub struct Point {
    pub beat: f64,
    pub value: f64,
    pub curve: Option<String>,
}

impl<'de> serde::Deserialize<'de> for Point {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct PointVisitor;
        impl<'de> serde::de::Visitor<'de> for PointVisitor {
            type Value = Point;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("array [beat, value] or [beat, value, curve]")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let beat: f64 = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(0, &self))?;
                let value: f64 = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(1, &self))?;
                let curve: Option<String> = seq.next_element()?;
                // Drain any extra elements silently — forward-compat.
                while seq.next_element::<serde::de::IgnoredAny>()?.is_some() {}
                Ok(Point { beat, value, curve })
            }
        }
        deserializer.deserialize_seq(PointVisitor)
    }
}

impl schemars::JsonSchema for Point {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Point".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        // Hand-rolled schema: a tuple of 2-3 items where the optional third is
        // a curve enum. prefixItems + items is JSON Schema 2020-12 idiomatic;
        // LLMs also read the description which carries the full vocabulary.
        let value = serde_json::json!({
            "type": "array",
            "description": "Point in a per-note trajectory. Array form [beat, value] or [beat, value, curve]. \
                beat: fugue-relative beat offset. \
                value: semitones (-64 to +64) for pitch bend, or 0.0-1.0 for pressure. \
                curve: optional interpolation for the segment ARRIVING at this point (ignored on first point). \
                One of: 'linear' (default, smooth ramp), 'exp' (ease-in, accelerating), 'log' (ease-out, decelerating), 'none' (step). \
                Example crescendo-then-release on pressure: [[0,0],[2,1,\"exp\"],[4,0,\"log\"]].",
            "minItems": 2,
            "maxItems": 3,
            "prefixItems": [
                { "type": "number", "description": "beat offset" },
                { "type": "number", "description": "target value" }
            ],
            "items": {
                "type": "string",
                "enum": ["linear", "exp", "log", "none"]
            }
        });
        let map = match value {
            serde_json::Value::Object(m) => m,
            _ => unreachable!("json! literal is an object"),
        };
        schemars::Schema::from(map)
    }
}

/// A single note in a compact fugue
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CompactNote {
    /// Beat offset from fugue start
    #[schemars(description = "Beat offset from fugue start (0.0 = start)")]
    pub beat: f64,
    /// MIDI note. Name ('C4', 'F#3') or number (0-127).
    #[schemars(description = "MIDI note. Name ('C4', 'F#3', 'Bb5') or number 0-127.")]
    pub note: Note,
    /// Duration in beats (note_off auto-generated at beat + duration)
    #[schemars(description = "Duration in beats. Note-off is automatically sent at beat + duration.")]
    pub duration: f64,
    /// Velocity (1-127, defaults to 100)
    #[serde(default = "default_note_velocity")]
    #[schemars(description = "Velocity (1-127, defaults to 100)")]
    pub velocity: Option<u8>,
    /// Channel override (1-16, defaults to fugue channel)
    #[schemars(description = "Channel override (1-16). If not set, uses fugue's channel.")]
    pub channel: Option<u8>,
}

/// A single fugue definition within a batch
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CompactFugue {
    /// Tag for grouping/cancellation (e.g., "melody", "filter")
    #[schemars(description = "Tag for grouping/cancellation. Use with cancel_mode:'tag:NAME' to replace specific fugues.")]
    pub tag: Option<String>,
    /// What to cancel when this fugue starts: "none", "tag:NAME", or "all"
    #[serde(default = "default_cancel_mode")]
    #[schemars(description = "What to cancel when starting: 'none', 'tag:NAME' (cancels matching tag), or 'all'")]
    pub cancel_mode: Option<String>,
    /// Default MIDI channel for this fugue (1-16, defaults to 1)
    #[serde(default = "default_fugue_channel")]
    #[schemars(description = "Default MIDI channel (1-16, defaults to 1)")]
    pub channel: Option<u8>,

    // Per-fugue overrides for shared settings
    /// Override quantize mode for this fugue
    #[schemars(description = "Override quantize: 'immediate', 'beat', 'bar', or 'bars:N'")]
    pub quantize: Option<String>,
    /// Override duration for this fugue
    #[schemars(description = "Override duration in beats")]
    pub duration_beats: Option<f64>,
    /// Override loop mode for this fugue
    #[schemars(description = "Override loop mode: 'once', 'forever', or a number")]
    pub loop_mode: Option<String>,

    /// The fugue content (notes / CC / per-note expression)
    #[serde(flatten)]
    pub content: FugueContent,
}

/// Queue one or more fugues for transport-synchronized playback
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct QueueFugueData {
    /// Array of fugues to queue. Each fugue is atomic.
    #[schemars(description = "Array of fugues. Each is atomic - use separate fugues for notes, CC, and per-note expression so they can be updated independently.")]
    pub fugues: Vec<CompactFugue>,

    // Shared defaults (can be overridden per-fugue)
    /// Default quantize mode: "immediate", "beat", "bar", or "bars:N"
    #[serde(default = "default_quantize")]
    #[schemars(description = "Default quantize: 'immediate', 'beat', 'bar', or 'bars:N' (default: 'bar')")]
    pub quantize: Option<String>,
    /// Default duration in beats
    #[serde(default = "default_duration")]
    #[schemars(description = "Default duration in beats (default: 4.0)")]
    pub duration_beats: Option<f64>,
    /// Default loop mode: "once", "forever", or a number
    #[serde(default = "default_loop_mode_opt")]
    #[schemars(description = "Default loop mode: 'once', 'forever', or a number (default: 'forever')")]
    pub loop_mode: Option<String>,
}

/// Cancel a specific fugue by ID
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CancelFugueData {
    /// Fugue ID to cancel (returned by queue_fugue)
    #[schemars(description = "Fugue ID to cancel (returned by queue_fugue)")]
    pub id: u64,
}

/// Cancel fugues by tag
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CancelFuguesByTagData {
    /// Tag to match for cancellation
    #[schemars(description = "Tag to match for cancellation (e.g., 'melody')")]
    pub tag: String,
}

// =============================================================================
// Instance management types (these don't use the wrapper since instance is the subject)
// =============================================================================

/// Request to rename an instance
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RenameInstanceRequest {
    /// Current instance name or ID
    #[schemars(description = "Current instance name or ID")]
    pub instance: String,

    /// New display name for the instance
    #[schemars(description = "New display name for the instance")]
    pub name: String,
}

/// Request to get slots for an instance (no additional data needed)
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetSlotsRequest {
    /// Target plugin instance name or "default" for first available
    #[serde(default = "default_instance")]
    #[schemars(description = "Target plugin instance name or 'default' for first available")]
    pub instance: String,
}

// =============================================================================
// Type aliases for the MCP tool interface
// =============================================================================

pub type SendCcRequest = InstanceRequest<CcData>;
pub type SetParamRequest = InstanceRequest<SetParamData>;
pub type RenameSlotRequest = InstanceRequest<RenameSlotData>;

// Per-note expression request types (MIDI 2.0)
pub type PerNotePitchBendRequest = InstanceRequest<PerNotePitchBendData>;
pub type PerNotePressureRequest = InstanceRequest<PerNotePressureData>;
pub type PerNoteControllerRequest = InstanceRequest<PerNoteControllerData>;
pub type PerNoteManagementRequest = InstanceRequest<PerNoteManagementData>;

// Fugue request types
pub type QueueFugueRequest = InstanceRequest<QueueFugueData>;
pub type CancelFugueRequest = InstanceRequest<CancelFugueData>;
pub type CancelFuguesByTagRequest = InstanceRequest<CancelFuguesByTagData>;

// =============================================================================
// Default value functions
// =============================================================================

fn default_instance() -> String {
    "default".to_string()
}

fn default_channel() -> u8 {
    1
}

fn default_note_velocity() -> Option<u8> {
    Some(100)
}

fn default_fugue_channel() -> Option<u8> {
    Some(1)
}

fn default_duration() -> Option<f64> {
    Some(4.0)
}

fn default_loop_mode_opt() -> Option<String> {
    Some("forever".to_string())
}

fn default_quantize() -> Option<String> {
    Some("bar".to_string())
}

fn default_cancel_mode() -> Option<String> {
    Some("none".to_string())
}

// =============================================================================
// Parsing helpers (string → typed)
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

// =============================================================================
// Event emitters (MCP request type → TimedFugueEvent stream)
//
// Each helper takes a `&mut Vec<TimedFugueEvent>` and pushes into it. Kept as
// pure functions (no `self`) so they compose freely: the single-concern
// FugueContent variants and the Composite variant both use the same code paths,
// and a Composite's four lanes can emit independently without borrow-checker
// gymnastics.
//
// `fugue_channel` is the fugue's default channel, already 0-indexed (0-15).
// =============================================================================

/// Expand a Notes array into `NoteOn` + auto-generated `NoteOff` events.
pub fn emit_notes(
    notes: &[CompactNote],
    fugue_channel: u8,
    events: &mut Vec<TimedFugueEvent>,
) {
    for note in notes {
        let channel = note.channel
            .map(|c| c.saturating_sub(1).min(15))
            .unwrap_or(fugue_channel);
        let velocity = note.velocity.unwrap_or(100).clamp(1, 127);
        let note_num = note.note.0.min(127);
        events.push(TimedFugueEvent::new(
            note.beat,
            FugueEvent::NoteOn { channel, note: note_num, velocity },
        ));
        events.push(TimedFugueEvent::new(
            note.beat + note.duration,
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

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // Note — parse_note_name
    // ------------------------------------------------------------------

    /// Shorthand for tests: parse and unwrap, or panic with the error.
    fn note(s: &str) -> u8 {
        parse_note_name(s).unwrap_or_else(|e| panic!("parse_note_name({:?}) failed: {}", s, e))
    }

    #[test]
    fn note_name_naturals_in_octave_4() {
        assert_eq!(note("C4"), 60, "middle C");
        assert_eq!(note("D4"), 62);
        assert_eq!(note("E4"), 64);
        assert_eq!(note("F4"), 65);
        assert_eq!(note("G4"), 67);
        assert_eq!(note("A4"), 69, "concert pitch");
        assert_eq!(note("B4"), 71);
    }

    #[test]
    fn note_name_c_across_octaves() {
        // C-1 is MIDI 0; each octave adds 12.
        assert_eq!(note("C-1"), 0);
        assert_eq!(note("C0"), 12);
        assert_eq!(note("C1"), 24);
        assert_eq!(note("C2"), 36);
        assert_eq!(note("C3"), 48);
        assert_eq!(note("C4"), 60);
        assert_eq!(note("C5"), 72);
        assert_eq!(note("C6"), 84);
        assert_eq!(note("C7"), 96);
        assert_eq!(note("C8"), 108);
        assert_eq!(note("C9"), 120);
    }

    #[test]
    fn note_name_extreme_range() {
        // MIDI 0 = lowest, MIDI 127 = highest.
        assert_eq!(note("C-1"), 0);
        assert_eq!(note("G9"), 127);
    }

    #[test]
    fn note_name_sharps() {
        assert_eq!(note("C#4"), 61);
        assert_eq!(note("D#4"), 63);
        assert_eq!(note("F#4"), 66);
        assert_eq!(note("G#4"), 68);
        assert_eq!(note("A#4"), 70);
    }

    #[test]
    fn note_name_flats_lowercase_b() {
        assert_eq!(note("Db4"), 61);
        assert_eq!(note("Eb4"), 63);
        assert_eq!(note("Gb4"), 66);
        assert_eq!(note("Ab4"), 68);
        assert_eq!(note("Bb4"), 70);
    }

    #[test]
    fn note_name_flats_uppercase_b_case_insensitive() {
        // Uppercase B after a note letter is also parsed as flat, so
        // callers who shout "DB4" get the same result as "Db4".
        assert_eq!(note("DB4"), 61);
        assert_eq!(note("EB4"), 63);
        assert_eq!(note("GB4"), 66);
        assert_eq!(note("AB4"), 68);
        assert_eq!(note("BB4"), 70);
    }

    #[test]
    fn note_name_case_insensitive_letter_every_natural() {
        // Every letter, lowercase and uppercase, matched against explicit MIDI.
        let pairs = [
            ("c4", 60), ("C4", 60),
            ("d4", 62), ("D4", 62),
            ("e4", 64), ("E4", 64),
            ("f4", 65), ("F4", 65),
            ("g4", 67), ("G4", 67),
            ("a4", 69), ("A4", 69),
            ("b4", 71), ("B4", 71),
        ];
        for (name, expected) in pairs {
            assert_eq!(note(name), expected, "{} should be MIDI {}", name, expected);
        }
    }

    #[test]
    fn note_name_case_insensitive_sharps() {
        // Sharp accidental is always '#', but the letter case varies.
        let pairs = [
            ("c#4", 61), ("C#4", 61),
            ("d#4", 63), ("D#4", 63),
            ("f#4", 66), ("F#4", 66),
            ("g#4", 68), ("G#4", 68),
            ("a#4", 70), ("A#4", 70),
        ];
        for (name, expected) in pairs {
            assert_eq!(note(name), expected, "{} should be MIDI {}", name, expected);
        }
    }

    #[test]
    fn note_name_case_insensitive_flats_full_matrix() {
        // Every combination of letter case × flat case.
        // Db4, dB4, DB4, db4 should all parse to 61.
        let pairs = [
            ("Db4", 61), ("dB4", 61), ("DB4", 61), ("db4", 61),
            ("Eb4", 63), ("eB4", 63), ("EB4", 63), ("eb4", 63),
            ("Gb4", 66), ("gB4", 66), ("GB4", 66), ("gb4", 66),
            ("Ab4", 68), ("aB4", 68), ("AB4", 68), ("ab4", 68),
            ("Bb4", 70), ("bB4", 70), ("BB4", 70), ("bb4", 70),
        ];
        for (name, expected) in pairs {
            assert_eq!(note(name), expected, "{} should be MIDI {}", name, expected);
        }
    }

    #[test]
    fn note_name_case_insensitive_across_octaves() {
        // Case-insensitivity holds regardless of octave.
        for octave in -1..=8 {
            let upper = format!("C{}", octave);
            let lower = format!("c{}", octave);
            assert_eq!(note(&upper), note(&lower), "C{} vs c{}", octave, octave);
        }
        // Same for flats with both cases of the flat symbol.
        let variants = ["Bb3", "bB3", "BB3", "bb3"];
        let first = note(variants[0]);
        for v in &variants[1..] {
            assert_eq!(note(v), first, "{} should equal {}", v, variants[0]);
        }
    }

    #[test]
    fn note_name_enharmonic_equivalents() {
        assert_eq!(note("C#4"), note("Db4"));
        assert_eq!(note("D#4"), note("Eb4"));
        assert_eq!(note("F#4"), note("Gb4"));
        assert_eq!(note("G#4"), note("Ab4"));
        assert_eq!(note("A#4"), note("Bb4"));
    }

    #[test]
    fn note_name_handles_surrounding_whitespace() {
        assert_eq!(note(" C4 "), 60);
        assert_eq!(note("\tF#3\n"), parse_note_name("F#3").unwrap());
    }

    #[test]
    fn note_name_negative_octaves() {
        // Only C-1 and above are in MIDI range, but the parser should
        // accept the syntax and reject out-of-range results cleanly.
        assert_eq!(note("C-1"), 0);
        assert_eq!(note("C#-1"), 1);
        assert_eq!(note("B-1"), 11);
        assert!(parse_note_name("C-2").is_err(), "C-2 is below MIDI range");
    }

    #[test]
    fn note_name_out_of_range_high() {
        // G9 = 127 is valid; G#9 / A9 would exceed 127.
        assert_eq!(note("G9"), 127);
        assert!(parse_note_name("G#9").is_err(), "G#9 = 128 should fail");
        assert!(parse_note_name("A9").is_err(), "A9 = 129 should fail");
        assert!(parse_note_name("C10").is_err(), "C10 = 132 should fail");
    }

    #[test]
    fn note_name_rejects_empty() {
        assert!(parse_note_name("").is_err());
        assert!(parse_note_name("   ").is_err(), "whitespace-only after trim");
    }

    #[test]
    fn note_name_rejects_invalid_letter() {
        assert!(parse_note_name("H4").is_err(), "H is not a note letter");
        assert!(parse_note_name("Z3").is_err());
        assert!(parse_note_name("04").is_err(), "digit in letter position");
    }

    #[test]
    fn note_name_rejects_missing_octave() {
        assert!(parse_note_name("C").is_err());
        assert!(parse_note_name("C#").is_err());
        assert!(parse_note_name("Bb").is_err());
    }

    #[test]
    fn note_name_rejects_bad_octave() {
        assert!(parse_note_name("Cx4").is_err(), "x is not a valid accidental");
        assert!(parse_note_name("C4x").is_err(), "trailing garbage");
        assert!(parse_note_name("C4.5").is_err(), "fractional octave");
        assert!(parse_note_name("Cfoo").is_err());
    }

    #[test]
    fn note_name_error_messages_are_helpful() {
        // Errors should name the input so a user (or LLM reading the
        // response) can diagnose what went wrong.
        let err = parse_note_name("H4").unwrap_err();
        assert!(err.contains("H4") || err.contains("note letter"),
            "error for 'H4' should mention input or note-letter issue; got: {}", err);
        let err = parse_note_name("C99").unwrap_err();
        assert!(err.contains("C99") || err.contains("range"),
            "range error should mention input or range; got: {}", err);
    }

    // ------------------------------------------------------------------
    // Note — Deserialize (accepts number or name via JSON)
    // ------------------------------------------------------------------

    #[test]
    fn note_deserialize_from_json_integer() {
        let n: Note = serde_json::from_str("60").unwrap();
        assert_eq!(n.0, 60);
        let n: Note = serde_json::from_str("0").unwrap();
        assert_eq!(n.0, 0);
        let n: Note = serde_json::from_str("127").unwrap();
        assert_eq!(n.0, 127);
    }

    #[test]
    fn note_deserialize_from_json_string_name() {
        let n: Note = serde_json::from_str("\"C4\"").unwrap();
        assert_eq!(n.0, 60);
        let n: Note = serde_json::from_str("\"F#3\"").unwrap();
        assert_eq!(n.0, 54);
        let n: Note = serde_json::from_str("\"Bb5\"").unwrap();
        assert_eq!(n.0, 82);
        let n: Note = serde_json::from_str("\"C-1\"").unwrap();
        assert_eq!(n.0, 0);
    }

    #[test]
    fn note_deserialize_from_json_string_number() {
        // Callers who serialize ints as strings shouldn't get punished.
        let n: Note = serde_json::from_str("\"60\"").unwrap();
        assert_eq!(n.0, 60);
        let n: Note = serde_json::from_str("\"127\"").unwrap();
        assert_eq!(n.0, 127);
    }

    #[test]
    fn note_deserialize_from_integer_float() {
        // 60.0 is OK — it's integer-valued.
        let n: Note = serde_json::from_str("60.0").unwrap();
        assert_eq!(n.0, 60);
    }

    #[test]
    fn note_deserialize_rejects_fractional_float() {
        assert!(serde_json::from_str::<Note>("60.5").is_err());
    }

    #[test]
    fn note_deserialize_rejects_out_of_range_integer() {
        assert!(serde_json::from_str::<Note>("128").is_err());
        assert!(serde_json::from_str::<Note>("-1").is_err());
        assert!(serde_json::from_str::<Note>("200").is_err());
    }

    #[test]
    fn note_deserialize_rejects_invalid_name() {
        assert!(serde_json::from_str::<Note>("\"H4\"").is_err());
        assert!(serde_json::from_str::<Note>("\"hello\"").is_err());
        assert!(serde_json::from_str::<Note>("\"\"").is_err());
    }

    #[test]
    fn note_deserialize_rejects_non_integer_strings_not_names() {
        assert!(serde_json::from_str::<Note>("\"abc\"").is_err());
    }

    #[test]
    fn note_deserialize_in_nested_context() {
        // Real-world: note lives inside a CompactNote lives inside a FugueContent.
        let json = r#"{
            "type": "notes",
            "notes": [
                {"beat": 0, "note": "C4",  "duration": 1},
                {"beat": 1, "note": "E4",  "duration": 1},
                {"beat": 2, "note": "G4",  "duration": 1},
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
            "note": "C4",
            "points": [[0, 0], [1, 2]]
        }"#;
        let content: FugueContent = serde_json::from_str(json).unwrap();
        match content {
            FugueContent::PerNotePitchBend { note, .. } => assert_eq!(note.0, 60),
            _ => panic!("expected PerNotePitchBend"),
        }
    }

    // ------------------------------------------------------------------
    // Note — Display (round-trip)
    // ------------------------------------------------------------------

    #[test]
    fn note_display_uses_sharps() {
        // Display chooses sharps for enharmonics — consistent convention.
        assert_eq!(Note(60).to_string(), "C4");
        assert_eq!(Note(61).to_string(), "C#4");
        assert_eq!(Note(70).to_string(), "A#4", "prefers A# over Bb");
        assert_eq!(Note(0).to_string(), "C-1");
        assert_eq!(Note(127).to_string(), "G9");
    }

    #[test]
    fn note_roundtrip_name_to_midi_to_name() {
        // Round-trip via Display: parse a name, format it back, parse again.
        // Names that don't go through enharmonic flips should survive.
        for name in ["C4", "C#4", "D4", "D#4", "E4", "F4", "F#4", "G4", "G#4", "A4", "A#4", "B4"] {
            let midi = parse_note_name(name).unwrap();
            let back = Note(midi).to_string();
            assert_eq!(back, name, "roundtrip failed for {}", name);
        }
    }

    #[test]
    fn note_roundtrip_all_midi_values() {
        // Every MIDI value 0..=127 should format to a valid name that
        // parses back to the same MIDI value.
        for midi in 0..=127u8 {
            let name = Note(midi).to_string();
            let parsed = parse_note_name(&name)
                .unwrap_or_else(|e| panic!("roundtrip failed for MIDI {}: name {:?} didn't parse: {}", midi, name, e));
            assert_eq!(parsed, midi, "MIDI {} → {:?} → MIDI {}", midi, name, parsed);
        }
    }

    // ------------------------------------------------------------------
    // Point custom Deserialize
    // ------------------------------------------------------------------

    #[test]
    fn per_note_point_2_tuple_no_curve() {
        let p: Point = serde_json::from_str("[1.5, 0.8]").unwrap();
        assert_eq!(p.beat, 1.5);
        assert_eq!(p.value, 0.8);
        assert!(p.curve.is_none());
    }

    #[test]
    fn per_note_point_3_tuple_with_curve() {
        let p: Point = serde_json::from_str(r#"[2.0, -1.5, "exp"]"#).unwrap();
        assert_eq!(p.beat, 2.0);
        assert_eq!(p.value, -1.5);
        assert_eq!(p.curve.as_deref(), Some("exp"));
    }

    #[test]
    fn per_note_point_accepts_all_curve_names() {
        for curve in ["linear", "exp", "log", "none"] {
            let json = format!(r#"[0, 0, "{}"]"#, curve);
            let p: Point = serde_json::from_str(&json).unwrap();
            assert_eq!(p.curve.as_deref(), Some(curve));
        }
    }

    #[test]
    fn per_note_point_accepts_unknown_curve_string() {
        // Parse succeeds (we don't validate here); expansion falls back to Linear.
        let p: Point = serde_json::from_str(r#"[0, 0, "bogus"]"#).unwrap();
        assert_eq!(p.curve.as_deref(), Some("bogus"));
    }

    #[test]
    fn per_note_point_extra_elements_are_dropped() {
        // Forward-compat: extra tail elements are silently ignored.
        let p: Point = serde_json::from_str(r#"[0, 1, "log", 42, "future"]"#).unwrap();
        assert_eq!(p.curve.as_deref(), Some("log"));
    }

    #[test]
    fn per_note_point_integer_values_deserialize() {
        // JSON integers should parse as f64.
        let p: Point = serde_json::from_str("[2, 1]").unwrap();
        assert_eq!(p.beat, 2.0);
        assert_eq!(p.value, 1.0);
    }

    #[test]
    fn per_note_point_negative_values() {
        let p: Point = serde_json::from_str("[0.5, -12.0]").unwrap();
        assert_eq!(p.value, -12.0);
    }

    #[test]
    fn per_note_point_missing_value_fails() {
        let err: Result<Point, _> = serde_json::from_str("[1.0]");
        assert!(err.is_err(), "single-element array should fail");
    }

    #[test]
    fn per_note_point_empty_array_fails() {
        let err: Result<Point, _> = serde_json::from_str("[]");
        assert!(err.is_err(), "empty array should fail");
    }

    #[test]
    fn per_note_point_object_form_fails() {
        // Object form is explicitly not supported — tuple only.
        let err: Result<Point, _> =
            serde_json::from_str(r#"{"beat": 0, "value": 1}"#);
        assert!(err.is_err(), "object form should not deserialize");
    }

    #[test]
    fn per_note_point_trajectory_parses() {
        // Crescendo-then-release: exp rise, log fall.
        let pts: Vec<Point> =
            serde_json::from_str(r#"[[0,0],[2,1,"exp"],[4,0,"log"]]"#).unwrap();
        assert_eq!(pts.len(), 3);
        assert!(pts[0].curve.is_none());
        assert_eq!(pts[1].curve.as_deref(), Some("exp"));
        assert_eq!(pts[2].curve.as_deref(), Some("log"));
    }

    #[test]
    fn per_note_point_trajectory_mixed_tuple_lengths() {
        // Array of mixed-arity tuples should all deserialize.
        let pts: Vec<Point> =
            serde_json::from_str(r#"[[0,0],[1,0.5],[2,1,"exp"],[3,0.5],[4,0,"log"]]"#).unwrap();
        assert_eq!(pts.len(), 5);
        assert!(pts[0].curve.is_none());
        assert!(pts[1].curve.is_none());
        assert_eq!(pts[2].curve.as_deref(), Some("exp"));
        assert!(pts[3].curve.is_none());
        assert_eq!(pts[4].curve.as_deref(), Some("log"));
    }

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
        // Unknown modes don't throw — they default to Linear, matching the
        // "not specified" behavior. This keeps bad inputs producing something
        // musically useful instead of failing the whole fugue.
        assert_eq!(parse_interpolation_mode(Some("bogus")), InterpolationMode::Linear);
        assert_eq!(parse_interpolation_mode(Some("")), InterpolationMode::Linear);
        assert_eq!(parse_interpolation_mode(Some("Linear")), InterpolationMode::Linear); // case-sensitive
    }

    // ------------------------------------------------------------------
    // expand_per_note_points
    // ------------------------------------------------------------------

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
        // Stepped: emit only the raw points — no intermediates.
        let pts = [pt(0.0, 0.0, None), pt(1.0, 1.0, Some("none"))];
        let out = collect_expansion(&pts);
        assert_eq!(out, vec![(0.0, 0.0), (1.0, 1.0)]);
    }

    #[test]
    fn expand_default_curve_is_linear() {
        // Missing curve on segment → Linear. Output matches explicit linear.
        let implicit = [pt(0.0, 0.0, None), pt(1.0, 1.0, None)];
        let explicit = [pt(0.0, 0.0, None), pt(1.0, 1.0, Some("linear"))];
        assert_eq!(collect_expansion(&implicit), collect_expansion(&explicit));
    }

    #[test]
    fn expand_zero_length_segment_emits_endpoint_only() {
        // Two points at the same beat: can't interpolate in time, emit once.
        let pts = [pt(1.0, 0.0, None), pt(1.0, 1.0, Some("linear"))];
        let out = collect_expansion(&pts);
        assert_eq!(out.len(), 2); // Anchor + endpoint.
        assert_eq!(out[1], (1.0, 1.0));
    }

    #[test]
    fn expand_negative_length_segment_emits_endpoint_only() {
        // Backward segment (decreasing beat): no interpolation, just the endpoint.
        let pts = [pt(2.0, 0.0, None), pt(1.0, 1.0, Some("linear"))];
        let out = collect_expansion(&pts);
        assert_eq!(out.len(), 2);
        assert_eq!(out[1], (1.0, 1.0));
    }

    #[test]
    fn expand_per_segment_curves_compose() {
        // The "crescendo-then-release" example: exp rise (0→1), log fall (1→0).
        //
        // Exp segment (0→1 over beats 0..2), midpoint t=0.5:
        //   t_curved = t² = 0.25; value = 0.0 + 0.25 * (1.0 - 0.0) = 0.25.
        //
        // Log segment (1→0 over beats 2..4), midpoint t=0.5:
        //   t_curved = 1 - (1-t)² = 0.75; value = 1.0 + 0.75 * (0.0 - 1.0) = 0.25.
        //
        // Both midpoints land at 0.25 — musical intuition: log on a falling
        // segment falls FAST early (more of the drop happens in the first half),
        // so we're already near the bottom at the midpoint.
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
        // 4-beat segment at density 32 should produce ~128 intermediate steps.
        let pts = [pt(0.0, 0.0, None), pt(4.0, 1.0, Some("linear"))];
        let out = collect_expansion(&pts);
        assert_eq!(out.len(), 129); // 1 anchor + 128 steps.
    }

    #[test]
    fn expand_unknown_curve_falls_back_to_linear() {
        // Bogus curve name → Linear behavior in the parser.
        let bogus = [pt(0.0, 0.0, None), pt(1.0, 1.0, Some("wobble"))];
        let linear = [pt(0.0, 0.0, None), pt(1.0, 1.0, Some("linear"))];
        assert_eq!(collect_expansion(&bogus), collect_expansion(&linear));
    }

    #[test]
    fn expand_first_point_curve_is_ignored() {
        // Curve on the first point has no segment to apply to — should be
        // irrelevant. Sanity-check: setting it to "exp" doesn't change output.
        let without = [pt(0.0, 0.0, None), pt(1.0, 1.0, Some("linear"))];
        let with_first_curve = [pt(0.0, 0.0, Some("exp")), pt(1.0, 1.0, Some("linear"))];
        assert_eq!(collect_expansion(&without), collect_expansion(&with_first_curve));
    }

    // ------------------------------------------------------------------
    // Full FugueContent deserialization (integration-ish)
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
        // Regression guard: adding per-note variants didn't break the notes variant.
        let json = r#"{
            "type": "notes",
            "notes": [{"beat": 0, "note": 60, "duration": 1}]
        }"#;
        let content: FugueContent = serde_json::from_str(json).unwrap();
        matches!(content, FugueContent::Notes { .. });
    }

    #[test]
    fn fugue_content_cc_with_curves() {
        // CC uses per-fugue interpolation (not per-segment) today.
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
        // Regression: previously CC points were Vec<[f64; 2]>, which rejected
        // 3-element tuples with an "invalid length 3, expected 2" error.
        // LLMs generalize from per-note's 3-tuple shape to CC, so CC now
        // accepts the unified Point form.
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
        // Some points with curves, some without — all must parse.
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

    // ------------------------------------------------------------------
    // Per-note interpolation field (fugue-level default)
    // ------------------------------------------------------------------

    #[test]
    fn expand_uses_default_mode_when_point_has_no_curve() {
        // Default mode is applied to segments whose points lack an explicit curve.
        let pts = [pt(0.0, 0.0, None), pt(1.0, 1.0, None)];
        let with_default_linear = collect_expansion_with(&pts, InterpolationMode::Linear);
        let with_default_exp = collect_expansion_with(&pts, InterpolationMode::Exp);
        // Midpoint differs because the curve differs.
        assert!((with_default_linear[16].1 - 0.5).abs() < 1e-9);
        assert!((with_default_exp[16].1 - 0.25).abs() < 1e-9);
    }

    #[test]
    fn expand_per_point_curve_overrides_default() {
        // Per-point curve wins over the fugue-level default.
        let pts = [pt(0.0, 0.0, None), pt(1.0, 1.0, Some("exp"))];
        let out = collect_expansion_with(&pts, InterpolationMode::Log);
        // Should be exp (quadratic ease-in), midpoint 0.25.
        assert!((out[16].1 - 0.25).abs() < 1e-9);
    }

    #[test]
    fn fugue_content_per_note_interpolation_field_parses() {
        // interpolation is now a field on the per-note variants.
        let json = r#"{
            "type": "per_note_pitch_bend",
            "note": "C4",
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
        // Omitting the field still parses (backward compat).
        let json = r#"{
            "type": "per_note_pressure",
            "note": "C4",
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
        // The motivating case: one instrument, many concerns in one fugue.
        let json = r#"{
            "type": "composite",
            "notes": [
                {"beat": 0, "note": "C2", "duration": 16},
                {"beat": 0, "note": "G3", "duration": 8},
                {"beat": 8, "note": "F3", "duration": 8}
            ],
            "cc": [
                {"cc": 74, "points": [[0,30],[8,100,"exp"],[16,40,"log"]]},
                {"cc": 11, "points": [[0,50],[16,115]], "interpolation": "exp"}
            ],
            "pitch_bends": [
                {"note": "G3", "points": [[0,0],[4,2,"exp"],[8,0,"log"]]}
            ],
            "pressures": [
                {"note": "C2", "points": [[0,0],[8,0.8,"exp"],[16,0,"log"]]},
                {"note": "G3", "points": [[0,0],[4,0.6],[8,0]]}
            ]
        }"#;
        let content: FugueContent = serde_json::from_str(json).unwrap();
        match content {
            FugueContent::Composite { notes, cc, pitch_bends, pressures } => {
                assert_eq!(notes.len(), 3);
                assert_eq!(notes[0].note.0, 36);  // C2
                assert_eq!(cc.len(), 2);
                assert_eq!(cc[0].cc, 74);
                assert_eq!(cc[1].interpolation.as_deref(), Some("exp"));
                assert_eq!(pitch_bends.len(), 1);
                assert_eq!(pitch_bends[0].note.0, 55);  // G3
                assert_eq!(pressures.len(), 2);
            }
            _ => panic!("expected Composite variant"),
        }
    }

    #[test]
    fn fugue_content_composite_optional_fields_default_empty() {
        // Only `notes` is required — the other lanes default to empty vecs.
        let json = r#"{
            "type": "composite",
            "notes": [{"beat": 0, "note": "C4", "duration": 1}]
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
        // Pure-automation fugue (no notes, just CC). Unusual but not invalid.
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
        // Regression guard: adding Composite didn't break the single-concern
        // variants. They must still parse exactly as before.
        let json = r#"{
            "type": "notes",
            "notes": [{"beat": 0, "note": "C4", "duration": 1}]
        }"#;
        assert!(matches!(
            serde_json::from_str::<FugueContent>(json).unwrap(),
            FugueContent::Notes { .. }
        ));
    }

    // ------------------------------------------------------------------
    // Emit helpers (MCP request → TimedFugueEvent)
    // ------------------------------------------------------------------

    #[test]
    fn emit_notes_generates_paired_on_off_events() {
        let notes = vec![
            CompactNote {
                beat: 0.0,
                note: Note(60),
                duration: 1.0,
                velocity: Some(100),
                channel: None,
            },
        ];
        let mut events = Vec::new();
        emit_notes(&notes, 0, &mut events);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].beat_offset, 0.0);
        assert_eq!(events[1].beat_offset, 1.0);
        assert!(matches!(events[0].event, FugueEvent::NoteOn { note: 60, .. }));
        assert!(matches!(events[1].event, FugueEvent::NoteOff { note: 60, .. }));
    }

    #[test]
    fn emit_notes_respects_channel_override() {
        // Note-level channel override wins over fugue-level channel.
        let notes = vec![
            CompactNote {
                beat: 0.0,
                note: Note(60),
                duration: 1.0,
                velocity: Some(100),
                channel: Some(5),  // 1-indexed → channel 4 internally
            },
        ];
        let mut events = Vec::new();
        emit_notes(&notes, 0, &mut events);
        match events[0].event {
            FugueEvent::NoteOn { channel, .. } => assert_eq!(channel, 4),
            _ => panic!("expected NoteOn"),
        }
    }

    #[test]
    fn emit_cc_lane_stamps_curve_on_every_event() {
        // Even bare points (no per-point curve) get Some(lane_default_mode)
        // on the event, so multi-lane composites can't collide.
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
            Point { beat: 0.0, value: 200.0, curve: None },   // clamp to 127
            Point { beat: 1.0, value: -5.0, curve: None },    // clamp to 0 via as u8 wraparound wraps high; see below
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
        // Anchor + 32 steps at default density.
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
        // Out-of-range pressure values get clamped to [0.0, 1.0].
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
