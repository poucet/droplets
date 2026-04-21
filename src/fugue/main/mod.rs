//! **Main-thread code.** Everything in this module runs off the
//! audio callback — MCP handlers, GUI WebSocket handlers, background
//! drainers, file I/O. Allocations, logging, locks, and blocking
//! syscalls are all fine here.
//!
//! The boundary to the audio thread lives in [`bridge`]:
//! - `FugueBridge::queue` pushes a `FugueCommand::Queue(FugueDefinition)`
//!   into an `rtrb` ring buffer consumed by [`super::audio::FugueSequencer`].
//! - `FugueBridge::get_fugue_info` / `get_transport` / `get_definitions`
//!   read from `ArcSwap` caches the audio thread atomically swaps each
//!   buffer.
//!
//! MIDI import / export ([`import`], [`export`]) are pure main-thread —
//! they convert between SMF bytes and `FugueDefinition` values and never
//! touch the scheduler directly; the caller queues the parsed
//! definitions through `FugueBridge`.

pub mod bridge;
pub mod export;
pub mod import;
pub mod settings;

pub use bridge::{FugueBridge, FugueInfoHandle};
