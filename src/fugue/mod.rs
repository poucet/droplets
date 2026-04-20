//! Fugue Queue System - Transport-synchronized sequencing
//!
//! Allows LLMs to queue pre-composed musical sequences ("fugues") that play back
//! with sample-accurate timing, support looping, layering, and tag-based cancellation.

mod bridge;
mod command;
mod fugue;
mod sequencer;
mod types;

pub mod export;
pub mod import;
pub mod settings;

pub use bridge::{FugueBridge, FugueInfoHandle};
pub use command::FugueCommand;
pub use fugue::Fugue;
pub use sequencer::FugueSequencer;
pub use types::{
    CancelMode, FugueDefinition, FugueEvent, FugueInfo, InterpolationMode, LoopMode,
    ProcessedEvent, QuantizeMode, TimedFugueEvent, TransportState,
};
