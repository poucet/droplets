//! **Audio-thread code.** Everything in this module runs on the
//! host's realtime audio callback — no locks, no allocations in the
//! hot path, no blocking syscalls, no `println!` / `log::info!` /
//! other IO. Shared state crosses the thread boundary via `rtrb` ring
//! buffers (main → audio) and `ArcSwap` caches (audio → main); those
//! primitives live in [`super::main::bridge`] but are read / written
//! from here.
//!
//! If you add code here, check that you're not introducing any of:
//! - a `Mutex` / `RwLock` (lock contention stalls the audio thread),
//! - a `Vec::push` / `Vec::with_capacity` on the hot path (malloc),
//! - a `format!` / string allocation anywhere the buffer iterates,
//! - a `log::*!` macro (most loggers allocate and lock),
//! - a `thread::spawn` / blocking syscall.
//!
//! All timing arithmetic goes through integer ticks
//! ([`super::types::TICKS_PER_BEAT`]) once Feature 27 lands — the
//! goal is zero f64 comparisons in the scheduling hot path.

mod fugue;
mod hacks;
mod sequencer;

pub use fugue::Fugue;
pub use sequencer::FugueSequencer;
