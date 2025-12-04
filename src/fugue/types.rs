//! Fugue types - all serializable types for fugue system
//!
//! These types are exported to TypeScript via ts-rs for frontend use.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// How a fugue should loop
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum LoopMode {
    /// Play once and finish
    Once,
    /// Play a specific number of times
    Times(u32),
    /// Loop forever until cancelled
    Forever,
}

impl Default for LoopMode {
    fn default() -> Self {
        Self::Once
    }
}

/// Quantization interval for fugue playback
///
/// Defines a grid that the fugue aligns to. The fugue will start when the
/// transport reaches a grid line (where current_beat % interval == 0).
/// Beat 0 is always a valid grid line for all intervals.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum QuantizeMode {
    /// No quantization - start immediately at current position
    Immediate,
    /// Quantize to beat boundaries (interval = 1 beat)
    Beat,
    /// Quantize to bar boundaries (interval = time_sig_numerator beats)
    Bar,
    /// Quantize to N-bar boundaries (interval = N * time_sig_numerator beats)
    Bars(u32),
}

impl QuantizeMode {
    /// Get the quantization interval in beats
    ///
    /// Returns None for Immediate (no grid), otherwise returns the interval size.
    pub fn interval_beats(&self, time_sig_numerator: u32) -> Option<f64> {
        match self {
            QuantizeMode::Immediate => None,
            QuantizeMode::Beat => Some(1.0),
            QuantizeMode::Bar => Some(time_sig_numerator as f64),
            QuantizeMode::Bars(n) => Some(*n as f64 * time_sig_numerator as f64),
        }
    }

    /// Check if a given beat position is on a grid line
    ///
    /// Uses a tolerance of ~3% of the interval (minimum 0.01 beats) to account
    /// for floating point imprecision and DAW timing variations.
    pub fn is_on_grid(&self, beat: f64, time_sig_numerator: u32) -> bool {
        match self.interval_beats(time_sig_numerator) {
            None => true, // Immediate - always on grid
            Some(interval) => {
                // Use a tolerance proportional to the interval, but at least 0.01 beats
                // This handles both small intervals (1 beat) and large ones (16+ beats)
                let tolerance = (interval * 0.03).max(0.01);
                let remainder = beat % interval;
                remainder < tolerance || (interval - remainder) < tolerance
            }
        }
    }

    /// Get the next grid line at or after the given beat
    ///
    /// If the beat is within tolerance of a grid line, snaps to that grid line.
    /// Otherwise returns the next grid line.
    pub fn next_grid_line(&self, beat: f64, time_sig_numerator: u32) -> f64 {
        match self.interval_beats(time_sig_numerator) {
            None => beat, // Immediate - current position is the grid line
            Some(interval) => {
                let tolerance = (interval * 0.03).max(0.01);
                let remainder = beat % interval;

                // If we're close to the current grid line, snap to it
                if remainder < tolerance {
                    beat - remainder // Snap back to exact grid line
                } else if (interval - remainder) < tolerance {
                    // We're close to the next grid line, snap forward
                    beat + (interval - remainder)
                } else {
                    // We're between grid lines, go to next one
                    ((beat / interval).floor() + 1.0) * interval
                }
            }
        }
    }
}

impl Default for QuantizeMode {
    fn default() -> Self {
        Self::Bar
    }
}

/// What to cancel when this fugue starts
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum CancelMode {
    /// Don't cancel anything (layer with existing fugues)
    None,
    /// Cancel all fugues with matching tag
    CancelByTag(String),
    /// Cancel all active fugues
    CancelAll,
}

impl Default for CancelMode {
    fn default() -> Self {
        Self::None
    }
}

/// A single musical event in a fugue
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FugueEvent {
    /// MIDI Note On
    NoteOn {
        channel: u8,
        note: u8,
        velocity: u8,
    },
    /// MIDI Note Off
    NoteOff {
        channel: u8,
        note: u8,
    },
    /// MIDI Control Change
    Cc {
        channel: u8,
        cc: u8,
        value: u8,
    },
    /// Per-note pitch bend (MIDI 2.0)
    PerNotePitchBend {
        channel: u8,
        note: u8,
        /// Pitch bend in semitones (-64.0 to +64.0)
        semitones: f32,
    },
    /// Per-note pressure/aftertouch (MIDI 2.0)
    PerNotePressure {
        channel: u8,
        note: u8,
        /// Pressure value 0.0-1.0
        pressure: f32,
    },
}

impl FugueEvent {
    /// Create a Note On event
    pub fn note_on(channel: u8, note: u8, velocity: u8) -> Self {
        Self::NoteOn { channel, note, velocity }
    }

    /// Create a Note Off event
    pub fn note_off(channel: u8, note: u8) -> Self {
        Self::NoteOff { channel, note }
    }

    /// Create a CC event
    pub fn cc(channel: u8, cc: u8, value: u8) -> Self {
        Self::Cc { channel, cc, value }
    }

    /// Create a per-note pitch bend event
    pub fn per_note_pitch_bend(channel: u8, note: u8, semitones: f32) -> Self {
        Self::PerNotePitchBend { channel, note, semitones }
    }

    /// Create a per-note pressure event
    pub fn per_note_pressure(channel: u8, note: u8, pressure: f32) -> Self {
        Self::PerNotePressure { channel, note, pressure }
    }

    /// Get the channel for this event
    pub fn channel(&self) -> u8 {
        match self {
            Self::NoteOn { channel, .. } => *channel,
            Self::NoteOff { channel, .. } => *channel,
            Self::Cc { channel, .. } => *channel,
            Self::PerNotePitchBend { channel, .. } => *channel,
            Self::PerNotePressure { channel, .. } => *channel,
        }
    }

    /// Get the note number for note events, if applicable
    pub fn note(&self) -> Option<u8> {
        match self {
            Self::NoteOn { note, .. } => Some(*note),
            Self::NoteOff { note, .. } => Some(*note),
            Self::PerNotePitchBend { note, .. } => Some(*note),
            Self::PerNotePressure { note, .. } => Some(*note),
            Self::Cc { .. } => None,
        }
    }
}

/// A fugue event with timing information
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TimedFugueEvent {
    /// Beat offset relative to fugue start (0.0 = start of fugue)
    pub beat_offset: f64,
    /// The event to trigger
    pub event: FugueEvent,
}

impl TimedFugueEvent {
    /// Create a new timed event
    pub fn new(beat_offset: f64, event: FugueEvent) -> Self {
        Self { beat_offset, event }
    }

    /// Create a note on event at the given beat
    pub fn note_on(beat: f64, channel: u8, note: u8, velocity: u8) -> Self {
        Self::new(beat, FugueEvent::note_on(channel, note, velocity))
    }

    /// Create a note off event at the given beat
    pub fn note_off(beat: f64, channel: u8, note: u8) -> Self {
        Self::new(beat, FugueEvent::note_off(channel, note))
    }

    /// Create a CC event at the given beat
    pub fn cc(beat: f64, channel: u8, cc: u8, value: u8) -> Self {
        Self::new(beat, FugueEvent::cc(channel, cc, value))
    }
}

/// A complete fugue definition ready for playback
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FugueDefinition {
    /// Unique ID for this fugue instance
    pub id: u64,
    /// Optional tag for grouping/cancellation
    pub tag: Option<String>,
    /// Events sorted by beat_offset
    pub events: Vec<TimedFugueEvent>,
    /// Total duration in beats
    pub duration_beats: f64,
    /// Looping behavior
    pub loop_mode: LoopMode,
    /// When to start relative to transport
    pub quantize: QuantizeMode,
    /// What to cancel when this starts
    pub cancel_mode: CancelMode,
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
            quantize: QuantizeMode::Immediate,
            cancel_mode: CancelMode::None,
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
}

/// Generate a unique fugue ID using time + random
fn generate_fugue_id() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let time_part = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0);
    let random_part = fastrand::u64(..) & 0xFFFF;
    (time_part << 16) | random_part
}

/// Information about an active fugue for listing
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct FugueInfo {
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
#[derive(Debug, Clone, Copy, Serialize, TS)]
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
}

impl Default for TransportState {
    fn default() -> Self {
        Self {
            beat: 0.0,
            tempo: 120.0,
            playing: false,
            time_sig_numerator: 4,
        }
    }
}
