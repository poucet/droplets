//! Fugue Queue System - Transport-synchronized sequencing
//!
//! Allows LLMs to queue pre-composed musical sequences ("fugues") that play back
//! with sample-accurate timing, support looping, layering, and tag-based cancellation.

mod bridge;
mod command;
mod sequencer;
mod state;
mod types;

pub use bridge::{FugueBridge, FugueInfoHandle};
pub use command::FugueCommand;
pub use sequencer::FugueSequencer;
pub use state::FugueState;
pub use types::{
    CancelMode, FugueDefinition, FugueEvent, FugueInfo, LoopMode, QuantizeMode,
    TimedFugueEvent, TransportState,
};
