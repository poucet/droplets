//! Known-ugly audio-thread workarounds, parked together so the main
//! scheduler code stays pristine. Each entry here documents *what*,
//! *why*, and what the "real fix" would look like — the goal isn't
//! to hide these, it's to make the list of hacks auditable at a
//! glance when new people read the scheduler.
//!
//! Current residents:
//!
//! - [`note_off_skewed_sample`] — subtract one sample from a NoteOff's
//!   `sample_offset` so same-sample OFF/ON pairs emitted by MIDI-import
//!   content (raw NoteOn + NoteOff in the event list at the same beat)
//!   resolve cleanly at the synth. TimedNote-originated notes don't go
//!   through this path — they handle retrigger via the ring's
//!   same-pitch-eviction in `Fugue::process_event`.
//!
//! Previously here but since retired:
//!
//! - `deconflict_same_sample_note_retriggers` (buffer-wide post-pass
//!   that shifted NoteOns +1 when they collided with a NoteOff at the
//!   same sample). Replaced by the targeted phase_lock fix in
//!   `send_note_offs_for_fugue_skipping_beat0_retriggers` — phase_lock
//!   no longer emits NoteOffs that would collide with the new
//!   iteration's beat-0 retriggers, so the collision never occurs.

/// Shift a NoteOff's `sample_offset` one sample earlier so that if
/// the same buffer carries a NoteOn at the same sample for the same
/// (channel, pitch) — typically from imported MIDI that has
/// back-to-back note events — the synth sees an unambiguous OFF → ON
/// ordering rather than two simultaneous events. Clamps to 0 via
/// `saturating_sub(1)`: at sample 0 the skew collapses and we fall
/// back to buffer-order ordering, which is rare enough to ignore.
///
/// **Scope**: this is only for raw `FugueEvent::NoteOff` events in a
/// fugue's event list. TimedNote-driven NoteOffs emitted from the
/// `PendingNoteOff` ring get their sample_offset computed directly
/// from `end_absolute_beat` and don't need the skew — the ring
/// scheduler's retrigger-eviction places NoteOffs at
/// `new_on_sample - 1` explicitly.
///
/// **If we retired this**: we'd need to either (a) drop MIDI-import
/// of tightly back-to-back same-pitch notes (lossy), or (b) merge
/// them to TimedNote at import time (duplicates emit_notes logic in
/// the import path). Neither is free, so we live with a one-sample
/// shift.
#[inline]
pub fn note_off_skewed_sample(sample_offset: u32) -> u32 {
    sample_offset.saturating_sub(1)
}
