//! Fugue Queue System — Transport-synchronized sequencing.
//!
//! Allows LLMs to queue pre-composed musical sequences ("fugues") that
//! play back with sample-accurate timing, support looping, layering,
//! and tag-based cancellation.
//!
//! The module is split along the thread boundary so the audio-thread
//! constraints (no allocations, no locks, no IO) are visible at
//! import time — if a site `use fugue::audio::...`, it's hot-path
//! code and the usual realtime rules apply:
//!
//! - [`audio`] — runs on the host's audio callback. [`Fugue`],
//!   [`FugueSequencer`], event scheduling, sample-accurate emission.
//! - [`main`] — runs off the audio callback. [`FugueBridge`] (ring
//!   buffer producer + ArcSwap caches), MIDI import/export,
//!   settings.
//! - [`types`] / [`command`] — data shapes that cross the boundary.
//!   `FugueCommand` goes main → audio through an rtrb ring;
//!   `FugueInfo` / `TransportState` go audio → main through
//!   lock-free `ArcSwap` caches.

pub mod audio;
pub mod main;
mod command;
mod types;

pub use audio::{Fugue, FugueSequencer};
pub use command::FugueCommand;
pub use main::{FugueBridge, FugueInfoHandle};
// Flat re-exports for the existing `crate::fugue::import` /
// `crate::fugue::export` / `crate::fugue::settings` call sites — the
// files themselves live under `main::` now, but their content is
// main-thread-only data, not part of the thread-boundary concern.
pub use main::{export, import, settings};
pub use types::{
    CancelMode, FugueDefinition, FugueEvent, FugueInfo, InterpolationMode, LoopMode,
    ProcessedEvent, QuantizeMode, StartMode, TimedFugueEvent, TransportState,
};
