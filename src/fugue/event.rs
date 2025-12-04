//! Fugue events - the atomic musical events within a fugue

use serde::{Deserialize, Serialize};

/// A single musical event in a fugue
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
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
