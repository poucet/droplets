//! Fugue types - all serializable types for fugue system
//!
//! Uses canonical definitions from `simply-fugue` shared across the Simply ecosystem.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

// Re-export canonical types from simply-fugue
pub use simply_fugue::{
    CancelTarget, CompiledFugue, CompositeFugue, FugueEvent, InterpolationMode, LoopMode,
    QuantizeMode, TimedFugueEvent,
};

/// What to cancel when a fugue starts.
///
/// Aliased directly to [`simply_fugue::CancelTarget`]: `None`, `Tag(String)`, `All`.
pub type CancelMode = CancelTarget;

/// Trait providing quantization helper methods on [`QuantizeMode`].
pub trait QuantizeExt {
    /// Get the quantization interval in beats
    fn interval_beats(&self, time_sig_numerator: u32) -> Option<f64>;
    /// Check if a given beat position is on a grid line
    fn is_on_grid(&self, beat: f64, time_sig_numerator: u32) -> bool;
    /// Get the next grid line at or after the given beat
    fn next_grid_line(&self, beat: f64, time_sig_numerator: u32) -> f64;
}

impl QuantizeExt for QuantizeMode {
    fn interval_beats(&self, time_sig_numerator: u32) -> Option<f64> {
        match self {
            QuantizeMode::None => None,
            QuantizeMode::Beat => Some(1.0),
            QuantizeMode::Bar => Some(time_sig_numerator as f64),
            QuantizeMode::Bars(n) => Some(*n as f64 * time_sig_numerator as f64),
            QuantizeMode::Eighth => Some(0.5),
            QuantizeMode::Sixteenth => Some(0.25),
        }
    }

    fn is_on_grid(&self, beat: f64, time_sig_numerator: u32) -> bool {
        match self.interval_beats(time_sig_numerator) {
            None => true, // None / Immediate - always on grid
            Some(interval) => {
                let tolerance = (interval * 0.03).max(0.01);
                let remainder = beat % interval;
                remainder < tolerance || (interval - remainder) < tolerance
            }
        }
    }

    fn next_grid_line(&self, beat: f64, time_sig_numerator: u32) -> f64 {
        match self.interval_beats(time_sig_numerator) {
            None => beat, // None / Immediate - current position is the grid line
            Some(interval) => {
                let tolerance = (interval * 0.03).max(0.01);
                let remainder = beat % interval;

                if remainder < tolerance {
                    beat - remainder // Snap back to exact grid line
                } else if (interval - remainder) < tolerance {
                    beat + (interval - remainder)
                } else {
                    ((beat / interval).floor() + 1.0) * interval
                }
            }
        }
    }
}

/// Trait providing curve application on [`InterpolationMode`].
pub trait InterpolationExt {
    /// Remap a normalized t ∈ [0,1] through this curve.
    fn apply_curve(&self, t: f64) -> f64;
}

impl InterpolationExt for InterpolationMode {
    fn apply_curve(&self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::None => if t >= 1.0 { 1.0 } else { 0.0 },
            Self::Linear => t,
            Self::Exp => t * t,
            Self::Log => 1.0 - (1.0 - t) * (1.0 - t),
        }
    }
}

/// Extension methods on [`FugueEvent`].
pub trait FugueEventExt {
    /// Create a Note On event
    fn note_on(channel: u8, note: u8, velocity: u8) -> FugueEvent;
    /// Create a Note Off event
    fn note_off(channel: u8, note: u8) -> FugueEvent;
    /// Create a CC event
    fn cc(channel: u8, cc: u8, value: u8) -> FugueEvent;
    /// Create a per-note pitch bend event
    fn per_note_pitch_bend(channel: u8, note: u8, semitones: f32) -> FugueEvent;
    /// Create a per-note pressure event
    fn per_note_pressure(channel: u8, note: u8, pressure: f32) -> FugueEvent;
    /// Create a TimedNote event
    fn timed_note(channel: u8, note: u8, velocity: u8, duration_beats: f64) -> FugueEvent;
    /// Get the channel for this event
    fn channel(&self) -> u8;
    /// Get the note number for note events, if applicable
    fn note(&self) -> Option<u8>;
}

impl FugueEventExt for FugueEvent {
    fn note_on(channel: u8, note: u8, velocity: u8) -> FugueEvent {
        Self::NoteOn { channel, note, velocity }
    }

    fn note_off(channel: u8, note: u8) -> FugueEvent {
        Self::NoteOff { channel, note, velocity: 0 }
    }

    fn cc(channel: u8, cc: u8, value: u8) -> FugueEvent {
        Self::Cc { channel, controller: cc, value, interpolation: InterpolationMode::Linear }
    }

    fn per_note_pitch_bend(channel: u8, note: u8, semitones: f32) -> FugueEvent {
        Self::PerNotePitchBend { channel, note, semitones, interpolation: InterpolationMode::Linear }
    }

    fn per_note_pressure(channel: u8, note: u8, pressure: f32) -> FugueEvent {
        Self::PerNotePressure { channel, note, pressure, interpolation: InterpolationMode::Linear }
    }

    fn timed_note(channel: u8, note: u8, velocity: u8, duration_beats: f64) -> FugueEvent {
        Self::TimedNote { channel, note, velocity, duration_beats }
    }

    fn channel(&self) -> u8 {
        match self {
            Self::NoteOn { channel, .. } => *channel,
            Self::NoteOff { channel, .. } => *channel,
            Self::TimedNote { channel, .. } => *channel,
            Self::Cc { channel, .. } => *channel,
            Self::Param { .. } => 0,
            Self::PitchBend { channel, .. } => *channel,
            Self::Pressure { channel, .. } => *channel,
            Self::PolyPressure { channel, .. } => *channel,
            Self::PerNotePitchBend { channel, .. } => *channel,
            Self::PerNotePressure { channel, .. } => *channel,
        }
    }

    fn note(&self) -> Option<u8> {
        match self {
            Self::NoteOn { note, .. } => Some(*note),
            Self::NoteOff { note, .. } => Some(*note),
            Self::TimedNote { note, .. } => Some(*note),
            Self::PolyPressure { note, .. } => Some(*note),
            Self::PerNotePitchBend { note, .. } => Some(*note),
            Self::PerNotePressure { note, .. } => Some(*note),
            _ => None,
        }
    }
}

/// Extension methods on [`TimedFugueEvent`].
pub trait TimedFugueEventExt {
    /// Create a new timed event
    #[allow(clippy::new_ret_no_self)]
    fn new(beat_offset: f64, event: FugueEvent) -> TimedFugueEvent;
    /// Create a note on event at the given beat
    fn note_on(beat: f64, channel: u8, note: u8, velocity: u8) -> TimedFugueEvent;
    /// Create a note off event at the given beat
    fn note_off(beat: f64, channel: u8, note: u8) -> TimedFugueEvent;
    /// Create a CC event at the given beat
    fn cc(beat: f64, channel: u8, cc: u8, value: u8) -> TimedFugueEvent;
    /// Create a CC event with an explicit curve at the given beat
    fn cc_with_curve(
        beat: f64,
        channel: u8,
        cc: u8,
        value: u8,
        curve: InterpolationMode,
    ) -> TimedFugueEvent;
    /// Create a TimedNote event at the given beat
    fn timed_note(beat: f64, channel: u8, note: u8, velocity: u8, duration_beats: f64) -> TimedFugueEvent;
}

impl TimedFugueEventExt for TimedFugueEvent {
    fn new(beat_offset: f64, event: FugueEvent) -> TimedFugueEvent {
        Self { beat_offset, event }
    }

    fn note_on(beat: f64, channel: u8, note: u8, velocity: u8) -> TimedFugueEvent {
        Self::new(beat, FugueEvent::note_on(channel, note, velocity))
    }

    fn note_off(beat: f64, channel: u8, note: u8) -> TimedFugueEvent {
        Self::new(beat, FugueEvent::note_off(channel, note))
    }

    fn cc(beat: f64, channel: u8, cc: u8, value: u8) -> TimedFugueEvent {
        Self::new(beat, FugueEvent::cc(channel, cc, value))
    }

    fn cc_with_curve(
        beat: f64,
        channel: u8,
        cc: u8,
        value: u8,
        curve: InterpolationMode,
    ) -> TimedFugueEvent {
        Self::new(
            beat,
            FugueEvent::Cc {
                channel,
                controller: cc,
                value,
                interpolation: curve,
            },
        )
    }

    fn timed_note(beat: f64, channel: u8, note: u8, velocity: u8, duration_beats: f64) -> TimedFugueEvent {
        Self::new(beat, FugueEvent::timed_note(channel, note, velocity, duration_beats))
    }
}

/// How a promoted fugue places itself on the song-grid when its quantize
/// target isn't already aligned to its `duration_beats`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum StartMode {
    /// Fugue joins the implicit always-running grid at its current phase.
    Phase,
    /// Fugue waits for the next iteration boundary, then plays from pattern-beat-0.
    Boundary,
}

impl Default for StartMode {
    fn default() -> Self {
        Self::Phase
    }
}

/// A complete fugue definition ready for playback in Droplets' sequencer.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct FugueDefinition {
    /// Unique ID for this fugue instance (serialized as string for JS compatibility)
    #[serde(with = "crate::serde_u64_string")]
    #[ts(type = "string")]
    #[schemars(with = "String")]
    pub id: u64,
    /// Optional tag for grouping/cancellation
    pub tag: Option<String>,
    /// Events sorted by beat_offset
    #[ts(type = "Array<import('./TimedFugueEvent').TimedFugueEvent>")]
    pub events: Vec<TimedFugueEvent>,
    /// Total duration in beats
    pub duration_beats: f64,
    /// Looping behavior
    #[ts(type = "import('./LoopMode').LoopMode")]
    pub loop_mode: LoopMode,
    /// When to start relative to transport
    #[ts(type = "import('./QuantizeMode').QuantizeMode")]
    pub quantize: QuantizeMode,
    /// What to cancel when this starts
    #[ts(type = "import('./CancelMode').CancelMode")]
    pub cancel_mode: CancelMode,
    /// Interpolation mode for CC automation
    #[serde(default)]
    #[ts(type = "import('./InterpolationMode').InterpolationMode")]
    pub cc_interpolation: InterpolationMode,
    /// How the fugue places itself on the song-grid
    #[serde(default)]
    pub start_mode: StartMode,
}

impl FugueDefinition {
    /// Create a new fugue definition with auto-generated ID
    pub fn new(events: Vec<TimedFugueEvent>, duration_beats: f64) -> Self {
        Self {
            id: generate_fugue_id(),
            tag: None,
            events,
            duration_beats,
            loop_mode: LoopMode::Once,
            quantize: QuantizeMode::None,
            cancel_mode: CancelMode::None,
            cc_interpolation: InterpolationMode::Linear,
            start_mode: StartMode::default(),
        }
    }

    /// Set the tag for this fugue
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = Some(tag.into());
        self
    }

    /// Set the loop mode
    pub fn with_loop_mode(mut self, mode: LoopMode) -> Self {
        self.loop_mode = mode;
        self
    }

    /// Set the quantize mode
    pub fn with_quantize(mut self, mode: QuantizeMode) -> Self {
        self.quantize = mode;
        self
    }

    /// Set the cancel mode
    pub fn with_cancel_mode(mut self, mode: CancelMode) -> Self {
        self.cancel_mode = mode;
        self
    }

    /// Set the CC interpolation mode
    pub fn with_cc_interpolation(mut self, mode: InterpolationMode) -> Self {
        self.cc_interpolation = mode;
        self
    }

    /// Set the start mode (phase vs boundary alignment).
    pub fn with_start_mode(mut self, mode: StartMode) -> Self {
        self.start_mode = mode;
        self
    }
}

impl From<CompositeFugue> for FugueDefinition {
    fn from(cf: CompositeFugue) -> Self {
        let compiled = cf.compile();
        Self {
            id: generate_fugue_id(),
            tag: cf.tag,
            events: compiled.events,
            duration_beats: compiled.duration_beats,
            loop_mode: compiled.loop_mode,
            quantize: QuantizeMode::Bar,
            cancel_mode: cf.cancel.unwrap_or(CancelTarget::None),
            cc_interpolation: InterpolationMode::Linear,
            start_mode: StartMode::Phase,
        }
    }
}

/// Generate a unique fugue ID using time + random
pub fn generate_fugue_id() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let time_part = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0);
    let random_part = fastrand::u64(..) & 0xFFFF;
    (time_part << 16) | random_part
}

/// Information about an active fugue for listing
#[derive(Debug, Clone, Serialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct FugueInfo {
    /// Serialized as string for JS compatibility
    #[serde(with = "crate::serde_u64_string")]
    #[ts(type = "string")]
    #[schemars(with = "String")]
    pub id: u64,
    pub tag: Option<String>,
    pub current_loop: u32,
    pub total_loops: Option<u32>,
    pub is_waiting: bool,
    pub progress_beats: f64,
    pub duration_beats: f64,
    /// Absolute beat when this fugue started (for UI position calculation)
    pub start_beat: f64,
    /// Quantization interval in beats (for UI playhead: transport_beat % interval)
    /// None means Immediate mode (no grid alignment)
    pub quantize_interval_beats: Option<f64>,
}

/// Transport state for UI synchronization
#[derive(Debug, Clone, Copy, Serialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct TransportState {
    /// Current beat position in the song
    pub beat: f64,
    /// Current tempo in BPM
    pub tempo: f64,
    /// Whether playback is active
    pub playing: bool,
    /// Time signature numerator (beats per bar)
    pub time_sig_numerator: u32,
    /// Whether DAW loop is active
    pub is_looping: bool,
    /// Loop start beat (only valid if is_looping is true)
    pub loop_start_beat: f64,
    /// Loop end beat (only valid if is_looping is true)
    pub loop_end_beat: f64,
}

impl Default for TransportState {
    fn default() -> Self {
        Self {
            beat: 0.0,
            tempo: 120.0,
            playing: false,
            time_sig_numerator: 4,
            is_looping: false,
            loop_start_beat: 0.0,
            loop_end_beat: 0.0,
        }
    }
}

// =============================================================================
// Sequencer output types (internal, not part of LLM API)
// =============================================================================

use crate::mcp::MidiMessage;

/// Event output from the fugue sequencer
#[derive(Debug, Clone, Copy)]
pub enum ProcessedEvent {
    /// An instant MIDI event at a specific sample offset
    Instant {
        sample_offset: u32,
        message: MidiMessage,
    },
    /// A CC ramp to interpolate over a sample range
    CcRamp {
        channel: u8,
        cc: u8,
        start_value: u8,
        end_value: u8,
        start_sample: u32,
        end_sample: u32,
        interpolation: InterpolationMode,
    },
}
