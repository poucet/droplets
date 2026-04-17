//! Request types and input parsing for the Simply Droplets MCP server.
//!
//! Everything here is about turning MCP-wire JSON into typed Rust values:
//! data structs with `#[derive(Deserialize, JsonSchema)]`, the
//! [`InstanceRequest`] wrapper, the flat-tuple [`PerNotePoint`] with its
//! custom Deserialize + JsonSchema, and the helpers that turn LLM-friendly
//! compact inputs into dense event streams.
//!
//! [`super::server`] stays focused on MCP tool routing and imports from here.

use rmcp::schemars;
use serde::Deserialize;

use crate::fugue::InterpolationMode;

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

/// MIDI Note On data (7-bit velocity)
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct NoteOnData {
    /// MIDI channel (1-16, default: 1)
    #[serde(default = "default_channel")]
    #[schemars(description = "MIDI channel (1-16, default: 1)")]
    pub channel: u8,

    /// MIDI note number (0-127, where 60 = C4/middle C)
    #[schemars(description = "MIDI note number (0-127, where 60 = C4/middle C)")]
    pub note: u8,

    /// Note velocity (1-127, default: 100)
    #[serde(default = "default_velocity")]
    #[schemars(description = "Note velocity (1-127, default: 100)")]
    pub velocity: u8,
}

/// MIDI 2.0 Note On data with 16-bit velocity
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct NoteOnHiresData {
    /// MIDI channel (1-16, default: 1)
    #[serde(default = "default_channel")]
    #[schemars(description = "MIDI channel (1-16, default: 1)")]
    pub channel: u8,

    /// MIDI note number (0-127, where 60 = C4/middle C)
    #[schemars(description = "MIDI note number (0-127, where 60 = C4/middle C)")]
    pub note: u8,

    /// 16-bit velocity (1-65535, default: 32768). MIDI 2.0 high-resolution.
    #[serde(default = "default_velocity_16bit")]
    #[schemars(description = "16-bit velocity (1-65535, default: 32768). MIDI 2.0 high-resolution.")]
    pub velocity: u16,
}

/// MIDI Note Off data
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct NoteOffData {
    /// MIDI channel (1-16, default: 1)
    #[serde(default = "default_channel")]
    #[schemars(description = "MIDI channel (1-16, default: 1)")]
    pub channel: u8,

    /// MIDI note number (0-127, where 60 = C4/middle C)
    #[schemars(description = "MIDI note number (0-127, where 60 = C4/middle C)")]
    pub note: u8,

    /// Release velocity (0-127, default: 0)
    #[serde(default)]
    #[schemars(description = "Release velocity (0-127, default: 0)")]
    pub velocity: u8,
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
    pub note: u8,

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

    /// MIDI note number (0-127, where 60 = C4/middle C)
    #[schemars(description = "MIDI note number (0-127, where 60 = C4/middle C)")]
    pub note: u8,

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

    /// MIDI note number (0-127, where 60 = C4/middle C)
    #[schemars(description = "MIDI note number (0-127, where 60 = C4/middle C)")]
    pub note: u8,

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

    /// MIDI note number (0-127, where 60 = C4/middle C)
    #[schemars(description = "MIDI note number (0-127, where 60 = C4/middle C)")]
    pub note: u8,

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
    /// CC automation with automatic linear interpolation between points
    Cc {
        /// CC number (0-127)
        #[schemars(description = "CC number (0-127)")]
        cc: u8,
        /// Array of [beat, value] pairs. Linear interpolation is automatic between consecutive points.
        #[schemars(description = "Array of [beat, value] pairs. Values 0-127. Linear interpolation happens automatically between points - just specify keyframes (e.g., [[0,0],[4,127]] ramps smoothly over 4 beats).")]
        points: Vec<[f64; 2]>,
        /// Interpolation mode: "linear" (default), "exp", "log", or "none"
        #[serde(default)]
        #[schemars(description = "Interpolation: 'linear' (default), 'exp' (ease-in, accelerating), 'log' (ease-out, decelerating), or 'none' (stepped).")]
        interpolation: Option<String>,
    },
    /// Per-note pitch bend over time (MIDI 2.0). Bends a single held note.
    /// Requires a concurrent Notes fugue holding the target note.
    /// Each point carries its own curve for the segment arriving at it, so
    /// a single fugue can combine different shapes (e.g. exp on a crescendo
    /// followed by log on a release). Server-side expansion at ~32 events/beat.
    PerNotePitchBend {
        /// MIDI note number being bent. Must match a note currently held by a concurrent Notes fugue (0-127).
        #[schemars(description = "MIDI note number to bend. Must be held by a concurrent notes fugue on the same channel (0-127, 60 = C4).")]
        note: u8,
        /// Points forming the bend trajectory. Tuple form [beat, semitones] or [beat, semitones, curve].
        #[schemars(description = "Array of trajectory points. Each point is [beat, semitones] or [beat, semitones, curve]. Semitones -64.0 to +64.0 (0 = no bend). curve on each point controls interpolation for the segment arriving at it (ignored on first point); one of 'linear' (default), 'exp', 'log', 'none'. Example vibrato: [[0,0],[0.5,1,\"linear\"],[1,-1,\"linear\"],[1.5,0,\"linear\"]].")]
        points: Vec<PerNotePoint>,
    },
    /// Per-note pressure/aftertouch over time (MIDI 2.0). Modulates a single held note.
    /// Requires a concurrent Notes fugue holding the target note.
    /// Same per-segment curve shape as PerNotePitchBend.
    PerNotePressure {
        /// MIDI note number receiving pressure. Must match a note currently held by a concurrent Notes fugue (0-127).
        #[schemars(description = "MIDI note number for pressure. Must be held by a concurrent notes fugue on the same channel (0-127, 60 = C4).")]
        note: u8,
        /// Points forming the pressure trajectory. Tuple form [beat, pressure] or [beat, pressure, curve].
        #[schemars(description = "Array of trajectory points. Each point is [beat, pressure] or [beat, pressure, curve]. Pressure 0.0-1.0. Example crescendo-then-release: [[0,0],[2,1,\"exp\"],[4,0,\"log\"]]. Valid curves: 'linear' (default), 'exp' (accelerating), 'log' (decelerating), 'none' (step, ignored on first point).")]
        points: Vec<PerNotePoint>,
    },
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
pub struct PerNotePoint {
    pub beat: f64,
    pub value: f64,
    pub curve: Option<String>,
}

impl<'de> serde::Deserialize<'de> for PerNotePoint {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct PointVisitor;
        impl<'de> serde::de::Visitor<'de> for PointVisitor {
            type Value = PerNotePoint;

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
                Ok(PerNotePoint { beat, value, curve })
            }
        }
        deserializer.deserialize_seq(PointVisitor)
    }
}

impl schemars::JsonSchema for PerNotePoint {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "PerNotePoint".into()
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
    /// MIDI note number (0-127)
    #[schemars(description = "MIDI note number (0-127)")]
    pub note: u8,
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
pub type SendNoteOnRequest = InstanceRequest<NoteOnData>;
pub type SendNoteOnHiresRequest = InstanceRequest<NoteOnHiresData>;
pub type SendNoteOffRequest = InstanceRequest<NoteOffData>;
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

fn default_velocity() -> u8 {
    100
}

fn default_velocity_16bit() -> u16 {
    32768 // Mid-point of 16-bit range
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
/// value to this point's value — the "curve to this point" convention.
/// Non-monotonic or zero-length segments emit the endpoint only.
///
/// Intermediate values are generated at [`PER_NOTE_EXPANSION_DENSITY`]
/// events/beat and routed through [`InterpolationMode::apply_curve`] so new
/// curves added to that function pick up automatically.
pub fn expand_per_note_points(points: &[PerNotePoint], mut emit: impl FnMut(f64, f64)) {
    if points.is_empty() {
        return;
    }
    emit(points[0].beat, points[0].value);
    for i in 1..points.len() {
        let p0 = &points[i - 1];
        let p1 = &points[i];
        let mode = parse_interpolation_mode(p1.curve.as_deref());
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
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // PerNotePoint custom Deserialize
    // ------------------------------------------------------------------

    #[test]
    fn per_note_point_2_tuple_no_curve() {
        let p: PerNotePoint = serde_json::from_str("[1.5, 0.8]").unwrap();
        assert_eq!(p.beat, 1.5);
        assert_eq!(p.value, 0.8);
        assert!(p.curve.is_none());
    }

    #[test]
    fn per_note_point_3_tuple_with_curve() {
        let p: PerNotePoint = serde_json::from_str(r#"[2.0, -1.5, "exp"]"#).unwrap();
        assert_eq!(p.beat, 2.0);
        assert_eq!(p.value, -1.5);
        assert_eq!(p.curve.as_deref(), Some("exp"));
    }

    #[test]
    fn per_note_point_accepts_all_curve_names() {
        for curve in ["linear", "exp", "log", "none"] {
            let json = format!(r#"[0, 0, "{}"]"#, curve);
            let p: PerNotePoint = serde_json::from_str(&json).unwrap();
            assert_eq!(p.curve.as_deref(), Some(curve));
        }
    }

    #[test]
    fn per_note_point_accepts_unknown_curve_string() {
        // Parse succeeds (we don't validate here); expansion falls back to Linear.
        let p: PerNotePoint = serde_json::from_str(r#"[0, 0, "bogus"]"#).unwrap();
        assert_eq!(p.curve.as_deref(), Some("bogus"));
    }

    #[test]
    fn per_note_point_extra_elements_are_dropped() {
        // Forward-compat: extra tail elements are silently ignored.
        let p: PerNotePoint = serde_json::from_str(r#"[0, 1, "log", 42, "future"]"#).unwrap();
        assert_eq!(p.curve.as_deref(), Some("log"));
    }

    #[test]
    fn per_note_point_integer_values_deserialize() {
        // JSON integers should parse as f64.
        let p: PerNotePoint = serde_json::from_str("[2, 1]").unwrap();
        assert_eq!(p.beat, 2.0);
        assert_eq!(p.value, 1.0);
    }

    #[test]
    fn per_note_point_negative_values() {
        let p: PerNotePoint = serde_json::from_str("[0.5, -12.0]").unwrap();
        assert_eq!(p.value, -12.0);
    }

    #[test]
    fn per_note_point_missing_value_fails() {
        let err: Result<PerNotePoint, _> = serde_json::from_str("[1.0]");
        assert!(err.is_err(), "single-element array should fail");
    }

    #[test]
    fn per_note_point_empty_array_fails() {
        let err: Result<PerNotePoint, _> = serde_json::from_str("[]");
        assert!(err.is_err(), "empty array should fail");
    }

    #[test]
    fn per_note_point_object_form_fails() {
        // Object form is explicitly not supported — tuple only.
        let err: Result<PerNotePoint, _> =
            serde_json::from_str(r#"{"beat": 0, "value": 1}"#);
        assert!(err.is_err(), "object form should not deserialize");
    }

    #[test]
    fn per_note_point_trajectory_parses() {
        // Crescendo-then-release: exp rise, log fall.
        let pts: Vec<PerNotePoint> =
            serde_json::from_str(r#"[[0,0],[2,1,"exp"],[4,0,"log"]]"#).unwrap();
        assert_eq!(pts.len(), 3);
        assert!(pts[0].curve.is_none());
        assert_eq!(pts[1].curve.as_deref(), Some("exp"));
        assert_eq!(pts[2].curve.as_deref(), Some("log"));
    }

    #[test]
    fn per_note_point_trajectory_mixed_tuple_lengths() {
        // Array of mixed-arity tuples should all deserialize.
        let pts: Vec<PerNotePoint> =
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

    fn collect_expansion(pts: &[PerNotePoint]) -> Vec<(f64, f64)> {
        let mut out = Vec::new();
        expand_per_note_points(pts, |b, v| out.push((b, v)));
        out
    }

    fn pt(beat: f64, value: f64, curve: Option<&str>) -> PerNotePoint {
        PerNotePoint { beat, value, curve: curve.map(|s| s.to_string()) }
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
            FugueContent::PerNotePitchBend { note, points } => {
                assert_eq!(note, 60);
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
            FugueContent::PerNotePressure { note, points } => {
                assert_eq!(note, 72);
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
}
