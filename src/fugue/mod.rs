//! Fugue Queue System - Transport-synchronized sequencing
//!
//! Allows LLMs to queue pre-composed musical sequences ("fugues") that play back
//! with sample-accurate timing, support looping, layering, and tag-based cancellation.

mod bridge;
mod command;
mod event;
mod sequencer;
mod state;

pub use bridge::{FugueBridge, FugueInfoHandle};
pub use command::FugueCommand;
pub use event::{FugueEvent, TimedFugueEvent};
pub use sequencer::FugueSequencer;
pub use state::FugueState;

use serde::{Deserialize, Serialize};

/// How a fugue should loop
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

/// When to start a fugue relative to transport position
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum QuantizeMode {
    /// Start immediately
    Immediate,
    /// Start on the next beat
    NextBeat,
    /// Start on the next bar (assumes 4/4)
    NextBar,
    /// Start on next N-bar boundary
    NextBars(u32),
}

impl Default for QuantizeMode {
    fn default() -> Self {
        Self::Immediate
    }
}

/// What to cancel when this fugue starts
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// A complete fugue definition ready for playback
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize)]
pub struct FugueInfo {
    pub id: u64,
    pub tag: Option<String>,
    pub current_loop: u32,
    pub total_loops: Option<u32>,
    pub is_waiting: bool,
    pub progress_beats: f64,
    pub duration_beats: f64,
}
