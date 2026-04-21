//! MCP wire-format types and conversions.
//!
//! Split by concern:
//! - [`note`] — `Note` + MIDI note-name parse/format helpers.
//! - [`point`] — `Point` trajectory tuple.
//! - [`compact`] — `FugueContent` + `Compact*` lane types + `CompactFugue`.
//! - [`emit`] — compact lane → `TimedFugueEvent` stream helpers.
//! - [`conversion`] — compact ↔ `FugueDefinition` bidirectional conversion
//!   + LLM-facing string parsers (loop/quantize/cancel modes).
//! - [`request`] — request wrappers, `*Data` bodies, type aliases,
//!   default-value functions.
//!
//! Re-exports below keep external callers unchanged — server.rs writes
//! `use super::types::*` and everything just works.

pub mod compact;
pub mod conversion;
pub mod emit;
pub mod note;
pub mod point;
pub mod request;

pub use compact::{
    CompactCc, CompactFugue, CompactNote, CompactPitchBend, CompactPressure, FugueContent,
};
pub use conversion::{
    compact_to_definition, definition_to_compact, parse_cancel_mode_str, parse_interpolation_mode,
    parse_loop_mode_str, parse_quantize_str, QueueFugueDefaults,
};
pub use emit::{
    emit_cc_lane, emit_notes, emit_per_note_pitch_bend, emit_per_note_pressure,
    expand_per_note_points, PER_NOTE_EXPANSION_DENSITY,
};
pub use note::{midi_to_name, parse_note_name, Note};
pub use point::Point;
pub use request::{
    CancelFugueData, CancelFugueRequest, CancelFuguesByTagData, CancelFuguesByTagRequest,
    GetFugueData, GetFugueRequest, GetSlotsRequest, ImportFugueData, ImportFugueRequest,
    InstanceRequest, QueueFugueData, QueueFugueRequest, RenameInstanceRequest,
};
