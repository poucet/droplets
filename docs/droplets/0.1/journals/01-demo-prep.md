# Phase 01 — Demo Prep Journal

Chronological log of notable decisions, blockers, and shipped work during
the 2026-04-23 demo push. Append entries at the end; most recent at the
bottom.

---

## 2026-04-20 — Feature 16 (drag-in) shipped

Closed the round-trip: users can now drag `.mid` files out of the DAW
(Feature 15), edit them in the piano roll, and drop them back into the
sequencer to re-queue as fugues.

**Structural decision: mirror the export shape.** First pass put decoding
and policy application in one function. Split on review into:

```
bytes
  → ImportedMidi::parse(bytes, strict)   ← decode to event-stream
  → .into_fugues(&opts)                   ← attach loop/quantize/cancel/tag
  → Vec<FugueDefinition>
```

Matches `merge_fugues_by_tag → ExportedMidi::from_merged → .to_bytes()`
on the export side. The payoff is callers can parse once and re-convert
with different policies (tested it), and the parse stage is independently
testable without any fugue-level knobs to fake.

**Three surfaces, one core.** HTTP (`POST /api/import_fugue`), wry IPC
(`/import_fugue` custom-protocol route), and MCP (`import_fugue` tool)
all delegate to `api::import_fugue`. Extracted `parse_loop_mode_str` and
`parse_quantize_str` so the existing `queue_fugue` path and the new
import path parse the same strings the same way — prevents drift.

**Per-note expression is lossy by default.** MIDI 1.0 channel-level
pitch bend and polyphonic aftertouch can't be reattributed to a specific
note on import — we'd be guessing. Default behavior drops them; `strict:
true` errors instead. Got into a conversation about whether to just
use MPE end-to-end; concluded MPE is the right fix but not demo-critical,
tracked as Feature 21. SMF2 / MIDI 2.0 is the eventual destination once
DAW support matures — tracked as Feature 22.

**Drag-out crash fix (earlier in the day).** User reported the plugin
crashing when dragging a fugue. Root cause: `drag::Image::Raw(Vec::new())`
passed zero bytes to `NSImage::initWithData_`, which returns nil; the
drag-frame setup then dereferenced nil. Fixed by bundling a 96×72 piano-
roll-styled PNG via `include_bytes!`. User asked about live-rendering from
MIDI content — agreed as a nicer follow-up; hardcoded fallback ships now
to stop the crash.

**Tests:** 170/170 lib tests pass. Added 12 to `src/fugue/import.rs`
covering malformed bytes, conductor-only files, tagged round-trip,
multi-track round-trip, `tag_prefix` namespacing, synthetic tag fallback,
NoteOn-vel-0 pairing, dangling-note-on closure at EOT, CC passthrough,
strict-mode rejection of pitch bend, option passthrough, and the
parse-intermediate-without-policy inspection pattern.
