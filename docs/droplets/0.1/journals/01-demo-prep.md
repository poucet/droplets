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

---

## 2026-04-20 (later) — demo-prep triage + drag docs polish

Bitwig's clip launcher won't emit `.mid` on drag. Verified not a
Droplets bug: even Finder can't accept a clip-launcher drag from
Bitwig. "Save Launcher Clip to Library" emits `.bwclip`, which is
Bitwig's clip-level subset of the open **dawproject** spec. Added
Feature 23 to the roadmap to support `.bwclip` export + import
alongside `.mid`, gated by a settings toggle. Estimated 2–3 days
when picked up; the zip + quick-xml stack is well-trodden and the
dawproject schema is published.

With 3 days to demo (2026-04-23), triaged open items. Kept the
"should land" list small:

- **15c.4** — FUGUE_UI.md gained a Drag-out/drag-in section and a
  Platform caveats block (macOS Gatekeeper, Linux GTK fallback,
  Bitwig launcher-clip limitation with forward-reference to
  Feature 23).
- **15e.2 + a symmetric drag-in entry** — instructions.md now
  frames drag-out as the user-facing hand-off and drag-in /
  `import_fugue` as the return leg, including the per-note
  expression caveat. Also added `import_fugue` to the Other
  tools list so the LLM doesn't need to rediscover it.
- **Summary-table fix** — Feature 15 was still `[ ]` in
  ROADMAP.md despite shipping; marked `[x]`.

**Deferred past demo** with rationale captured: Feature 8 (egui
migration, XL), 10 (audio-thread per-note ramps, risky), 18
(pause-instead-of-delete, untested in hot path), 19 + 20 (port
changes, breaks connectivity on stage), 9 (stateful MCP,
notifications not demo-critical), 21 / 22 / 23 (all format work).
Only remaining critical demo item is **14b.10** — Bitwig
extension walkthrough on the actual demo machine. Everything
else is post-demo.
