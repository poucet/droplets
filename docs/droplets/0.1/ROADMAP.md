# Droplets 0.1 Roadmap

## Summary

Droplets 0.1 is the **MCP-music demo release** — the version that goes on stage on **2026-04-21** to show an LLM composing multi-track music live through a DAW plugin over MCP.

The backend and UI are ~95% compliant with [FUGUE.md](../../FUGUE.md) and [FUGUE_UI.md](../../FUGUE_UI.md). Remaining work is about **polishing the LLM-facing surface** (prompt, tool descriptions, docs), **validating the multi-instance path** under demo conditions, and adding one missing primitive (`get_transport`). Per-note expressivity inside fugues is the stretch goal that would differentiate this demo from "LLM spams notes." A native UI rewrite (egui) is explicitly post-demo — the motivation is eliminating transport-to-render skew, not aesthetics.

---

## Feature Overview

### Phase 01: Demo Prep (must ship by 2026-04-21)

| Done | Pri | # | Feature | Complexity | Impact |
|------|-----|---|---------|------------|--------|
| [x] | P0 | 1 | Per-note expressivity inside fugues | M | High — reshapes schema; must land before docs/prompt |
| [x] | P0 | 2 | Rewrite `ServerInfo::instructions` system prompt | S | Very High — shapes every LLM call |
| [x] | P0 | 3 | Add a worked example to `queue_fugue` tool description | S | Very High — LLMs imitate examples |
| [x] | P0 | 4 | Rewrite FUGUE.md to match the shipping compact schema | S | High — live demo reference |
| [x] | P1 | 5 | Add `get_transport` MCP tool | S | Medium-High — lets LLM reason about timing |
| [x] | P0 | 6 | Multi-instance end-to-end validation | M | Critical — central demo claim |
| [x] | P0 | 7 | macOS UI verification on demo machine | S | Critical — risk mitigation |
| [x] | P0 | 11 | `composite` fugue type (notes + cc + bends + pressures in one fugue) | M | High — LLMs currently emit 10 fugues for one instrument; this is the biggest LLM-ergonomics fix left |
| [x] | P1 | 12 | UI lanes for per-note bend/pressure | M | Medium — composite fugues aren't useful if the UI can't render half their content |
| [x] | P0 | 13 | Transport phase-locking (fugues resume from correct phase on stop/play/relocate) | S | High — without this, every stop/play kills the demo |
| [~] | P0 | 14 | DAW track context (drum maps, device names) via host extension | L | Very High — AI currently picks random notes for drums because it has no way to know which sample is on which pad. **Rust side + Bitwig extension shipped 2026-04-20; demo-machine walkthrough pending.** |

### Phase 02: Post-Demo Polish

| Done | Pri | # | Feature | Complexity | Impact |
|------|-----|---|---------|------------|--------|
| [ ] | P2 | 8 | Migrate UI from React/HTTP to egui (in-process) | XL | Correctness — removes transport skew |
| [ ] | P3 | 9 | Evaluate stateful MCP mode for server→client push | S | Medium — unlocks event notifications (fugue-finished, instance-changed) |
| [ ] | P2 | 10 | Audio-thread ramps for per-note expression (pitch bend, pressure) | M | Quality — sample-accurate per-note curves instead of server-side discrete-event expansion |
| [x] | P1 | 17 | MCP tool: `get_fugue(id)` returning current FugueDefinition | S | High — small surface, immediately unlocks LLM read-modify-write; ships before the drag features because it's independent and its compact serializer is reusable downstream |
| [ ] | P1 | 15 | Native drag-out of fugues → DAW clip (`.mid` file) | M | High — lets users hand AI-generated patterns to the DAW's piano roll for editing; removes the need for an in-app editor |
| [ ] | P2 | 16 | Native drag-in of `.mid` → new fugue on an instance | M | Medium — round-trip workflow: edit in the DAW, drop back as a fugue |
| [ ] | P3 | 18 | Pause instead of delete on tag replacement | M | Medium — lets the user walk back to a prior version of a part instead of losing it forever when the LLM queues a new fugue with the same tag |
| [ ] | P2 | 19 | Unify GUI + MCP on a single port with three top-level paths | S | Medium — halves port consumption per plugin process; simpler firewall / sandbox story. Primary port stays **9999** (agents already configured). Top-level layout collapses to just `/api` (HTTP calls), `/mcp` (MCP protocol), and `/ws` (WebSocket upgrade). Extension POSTs move under `/api/*`; the MCP-side bare `/project_layout` + `/rename_instance` go away. |
| [ ] | P2 | 20 | Dynamic port selection on bind conflict | S | Medium — plugin currently dies if :9998/:9999 are in use. Walk a range, bind the first free port, surface the chosen port to the extension + UI |

---

## Phase Details

### Phase 01: Demo Prep

Everything here is about the **LLM's view of the system** and **demo-day reliability**. One new musical primitive (per-note expressivity in fugues) lands first because it reshapes the schema that the system prompt, worked example, and FUGUE.md all describe — sequencing it first means writing those once, not three times.

#### Feature 1: Per-note expressivity inside fugues

**Problem:** `FugueContent` has only `Notes` and `Cc` variants. Per-note pitch bend and per-note pressure exist as immediate `send_per_note_*` MCP tools but cannot be scheduled on a beat offset — which defeats the whole "pre-schedule because LLMs are slow" premise for any expressive/MPE-flavored demo moment.

**Solution:** Add two `FugueContent` variants: `PerNotePitchBend { note, points: [[beat, semitones]] }` and `PerNotePressure { note, points: [[beat, value_0_1]] }`. The fugue scheduler already handles per-note events via the one-shot dispatch path, so the audio-thread side is mostly reuse. This must land **before** features 2-4 so the system prompt, worked example, and FUGUE.md can describe the final schema in one pass.

**Status (2026-04-17 — shipped):**
- ✅ Per-note variants added to `FugueContent` + parsing.
- ✅ `InterpolationMode` extended with `Exp` / `Log` via unified `apply_curve`. Both CC interpolator sites (`interpolate_value`, `output_cc_ramp`) route through it — adding a new curve to `apply_curve` propagates to every site automatically.
- ✅ Per-note trajectories use flat `[beat, value]` / `[beat, value, curve]` tuples with per-segment curves (custom `Deserialize` + manual `JsonSchema`), expanded server-side into discrete events at ~32/beat. LLMs write e.g. `[[0,0],[2,1,"exp"],[4,0,"log"]]` for a crescendo-then-release on a single held note.
- ✅ CC's fugue-level interpolation parser now shares `parse_interpolation_mode` with per-note, which fixes an adjacent bug where CC `"exp"` / `"log"` silently fell through to Linear.
- ✅ Request types + parsing moved out of `server.rs` into `src/mcp/requests.rs` (server.rs is now ~50% of its former size and focused on MCP tool routing).
- ✅ Extensive test coverage: 32 tests on the custom deserializer, `parse_interpolation_mode`, and `expand_per_note_points` (including multi-segment curve composition, zero-length segments, unknown-curve fallback, JSON forward-compat with extra tail elements).

Audio-thread ramps for per-note remain tracked as Phase 02 Feature 10 — sample-accurate smoothness replacing the current server-side expansion.

**Files:** [src/mcp/server.rs](../../../src/mcp/server.rs) (FugueContent enum + parsing), [src/fugue/types.rs](../../../src/fugue/types.rs) (InterpolationMode + apply_curve), [src/fugue/fugue.rs](../../../src/fugue/fugue.rs) + [src/midi/mod.rs](../../../src/midi/mod.rs) (interpolator sites).

---

#### Feature 2: Rewrite `ServerInfo::instructions` system prompt

**Problem:** Current prompt ([src/mcp/server.rs:964-977](../../../src/mcp/server.rs#L964-L977)) shows a wrong `queue_fugue(events, duration_beats, ...)` signature, doesn't mention `set_instance_name` (central to the demo), omits the one-shot MIDI tools and per-note expression tools, and provides zero musical guidance.

**Solution:** Replace with a prompt that (a) describes the real `fugues: [...]` schema **including the new per-note variants from Feature 1**, (b) spells out the multi-instance workflow (`list_instances` → `set_instance_name` → use names), (c) states musical defaults (velocities 60-110, `quantize: "bar"`, tag+cancel_mode per part), (d) flags gotchas (transport must be playing, tempo drift, no `get_transport` yet — removed if Feature 5 ships).

**Files:** [src/mcp/server.rs](../../../src/mcp/server.rs) — the `instructions` field of `ServerInfo`.

---

#### Feature 3: Add worked example to `queue_fugue` tool description

**Problem:** The description is 3 sentences of rules. LLMs follow examples much more reliably than rules. An engineer-facing technical description is not a good model-facing prompt.

**Solution:** Append a fully-worked JSON example to the `#[tool(description = ...)]` attribute: two or three tagged fugues showing `notes`, `cc`, and **at least one per-note expression** (pitch bend or pressure) from Feature 1 — so the LLM sees the full vocabulary. Show `tag`, `cancel_mode: "tag:..."`, `quantize: "bar"`, and linear-interpolated CC points. Keep the existing sentences as preamble.

**Files:** [src/mcp/server.rs:768](../../../src/mcp/server.rs#L768).

---

#### Feature 4: Rewrite FUGUE.md

**Problem:** FUGUE.md documents a flat `events: [{type: "note_on"|"note_off"|"cc"|...}]` array, but the shipping schema is a batched `fugues: [{type: "notes" | "cc", ...}]` with auto note-off generation and interpolated CC points. Reading the current doc during the talk will produce confusion live.

**Solution:** Rewrite the doc against the actual `CompactFugue` / `FugueContent` types **as extended by Feature 1**. Lead with one tagged-layering example (bass + melody + CC sweep + per-note expression). Keep the constraints section (tempo changes cause drift, transport required).

**Files:** [docs/FUGUE.md](../../FUGUE.md), cross-referenced against [src/mcp/server.rs:250-390](../../../src/mcp/server.rs#L250-L390).

---

#### Feature 5: Add `get_transport` MCP tool

**Problem:** The LLM cannot ask "where are we?" — there is no MCP tool that returns `{beat, tempo, playing}`. The GUI has `/api/transport`, but the LLM side does not. This blocks patterns like "queue starting at bar 8, we're on bar 6 now" without the user narrating the state.

**Solution:** Thin MCP tool wrapping the existing `FugueBridge` transport cache (`Arc<ArcSwap<TransportState>>`). Takes `instance: String`, returns JSON. No audio-thread changes needed — cache is already populated.

**Files:** [src/mcp/server.rs](../../../src/mcp/server.rs) (add tool method), [src/fugue/bridge.rs](../../../src/fugue/bridge.rs) (expose getter if not public).

---

#### Feature 6: Multi-instance end-to-end validation

**Problem:** The architecture supports multi-instance (global registry, per-instance ring buffers, `set_instance_name`), but there is no test that exercises the full loop under demo conditions — plugin on 2+ tracks, renamed via MCP, fugues routed to each. If this breaks on stage, the whole demo premise breaks.

**Solution:** Manual scripted walkthrough on the demo machine: load plugin on 3 tracks → `list_instances` → `set_instance_name` × 3 → `queue_fugue` to each with different tags → `list_fugues` per instance → `cancel_fugues_by_tag` per instance → `clear_fugues` on one. Capture output and any quirks. Fix issues discovered.

**Files:** No code files — validation exercise. May produce fixes in [src/mcp/bridge.rs](../../../src/mcp/bridge.rs) if routing edge cases surface.

---

#### Feature 7: macOS UI verification on demo machine

**Problem:** [INSTALL.md](../../../INSTALL.md) flags "React UI may have compatibility issues" on macOS. Unknown whether this affects the demo machine specifically.

**Solution:** Run the full UI flow (viewer + composer + playhead animation) on the exact demo hardware. If broken, commit to the browser fallback at `http://localhost:9998` and rehearse opening it. Don't attempt a fix — time budget is tight.

**Files:** None — verification. Possibly [INSTALL.md](../../../INSTALL.md) update.

---

#### Feature 11: `composite` fugue type

**Problem:** Today a musically-coherent moment on one instrument (e.g. a pad with held chord tones, a filter sweep, an expression curve, and pressure swells) requires the LLM to emit one fugue PER concern. Observed on 2026-04-17: asked for "a meaningful fugue," the LLM produced 10 separate fugues for a single pad part (drone / mid / upper notes + 4 pressure swells + 3 CC lanes). Each gets its own row in the UI, its own id, its own cancel. Editing the "pad moment" means juggling 10 tagged cancels. The one-fugue-per-concern pattern is right when parts are *independently editable* (bass + melody), but wrong when they're one atomic musical idea.

The audio-thread already treats a fugue as a single flat `events: Vec<TimedFugueEvent>` list — the per-concern split exists only in the MCP surface. So bundling is a parser-layer change.

**Solution:** Add a new `FugueContent::Composite` variant that carries all four continuous-signal types in one object:

```json
{
  "tag": "pad-moment",
  "cancel_mode": "tag:pad-moment",
  "type": "composite",
  "notes":       [{"beat":0,"note":"C2","duration":16}, {"beat":0,"note":"G3","duration":8}, ...],
  "cc":          [{"cc":74,"points":[[0,30],[8,100,"exp"],[16,40,"log"]]}, {"cc":11,"points":[...]}],
  "pitch_bends": [{"note":"C4","points":[[0,0],[2,2,"exp"],[4,0,"log"]]}],
  "pressures":   [{"note":"C2","points":[[0,0],[8,0.8,"exp"],[16,0,"log"]]}, ...]
}
```

All fields except `notes` are optional. One fugue, one tag, one id, one UI row, one cancel. Keep the single-concern variants (`notes`, `cc`, `per_note_pitch_bend`, `per_note_pressure`) for when parts legitimately belong to different musical concerns that should be cancellable independently (bass vs melody).

**Parser:** each sub-array expands using the same helpers as the single-concern variants (CompactNote → note_on/note_off pairs; `cc` entries → per-event CC with curve; `pressures`/`pitch_bends` entries → expanded per-note events via `expand_per_note_points`). No audio-thread changes.

**System prompt:** steer the LLM toward composite by default for single-instrument moments, with a one-line rule: *"use `composite` when the parts belong to one musical moment on one instrument; use single-concern fugues only when parts need independent replacement (e.g. bass swap while melody keeps playing)"*.

**Files:** [src/mcp/requests.rs](../../../src/mcp/requests.rs) (new variant + sub-type structs `CompactCc`, `CompactPitchBend`, `CompactPressure`), [src/mcp/server.rs](../../../src/mcp/server.rs) (parser match arm), [src/mcp/instructions.md](../../../src/mcp/instructions.md), [docs/FUGUE.md](../../FUGUE.md), `queue_fugue` tool description.

---

#### Feature 12: UI lanes for per-note bend and pressure

**Problem:** `FugueGrid` currently renders two lane types: a piano-roll grid for `notes` and line graphs for `cc`. Per-note pitch bend and per-note pressure have neither representation — they silently don't show. Feature 11 makes this worse: a composite fugue may carry 4 pressure swells and 2 bends that the UI can't visualize, so the user sees a fugue panel that looks empty below the notes.

**Solution:** Two new lane types in `FugueGrid`:

- **Pressure lane** (one per `(note, channel)` tuple that has pressure data): line graph in its own horizontal lane below the CC lanes, labelled with the target note name. Same rendering as CC lanes structurally, but y-axis is 0.0–1.0.
- **Pitch-bend overlay** on the piano-roll note cells: color-tint the cell along its duration based on the bend trajectory (semitones), with a small numeric label at peaks. Unlike pressure, bends belong spatially *on* the held note, so an overlay is more intuitive than a separate lane.

Composite fugues from Feature 11 render as a vertical stack: piano-roll (with bend overlays) on top → N CC lanes → N pressure lanes. Single-concern fugues still render as today.

**Files:** [frontend/src/components/FugueGrid.tsx](../../../frontend/src/components/FugueGrid.tsx), new sub-components for PressureLane and BendOverlay, possibly adjust FugueViewer layout for the taller composite panel.

---

#### Feature 13: Transport phase-locking

**Problem:** Fugues were anchored to an absolute `start_beat` captured at quantization time, with loop iterations monotonically advancing `start_beat += duration_beats`. Consequences: pressing stop and then play in Bitwig (from beat 0 or from anywhere else) left looping fugues silent — their events were all in the past relative to the new transport position, and `next_event_index` was past the end. Pressing play from bar 2 also broke alignment because `current_beat` didn't line up with `start_beat + k*duration`.

**Solution:** Treat looping fugues as **phase-locked to the DAW's transport timeline.** On transport start and on jump/relocate, re-anchor each looping fugue so its phase matches the current transport beat:

```
phase = (transport_beat - original_start_beat) mod duration_beats
new_start_beat = transport_beat - phase
next_event_index = first event with beat_offset >= phase
```

Mid-flight one-shots (`LoopMode::Once` / `Times(n)`) can't be meaningfully resumed — their events are in the past — so they're cancelled on jump. Pending (waiting) fugues re-quantize normally on the next buffer. Note-offs emitted for all held notes before re-anchoring so the synth doesn't get stuck.

The existing jump-detector heuristic in `FugueSequencer::process` (comparing `current_beat` delta to expected advance) now drives phase-locking instead of "reset to waiting." The previous `just_started` fast-path that only synced `last_beat` was replaced by a real re-anchor call.

**Status (2026-04-20 — shipped):** `Fugue::phase_lock(transport_beat)` on the fugue type; `FugueSequencer::phase_lock_all` replaces `handle_transport_jump`. All 114 tests pass.

**Files:** [src/fugue/fugue.rs](../../../src/fugue/fugue.rs) (phase_lock method), [src/fugue/sequencer.rs](../../../src/fugue/sequencer.rs) (transport-start + jump handling).

---

#### Feature 14: DAW track context (drum maps, device names)

**Problem:** When asked to write a drum pattern, the LLM has no way to know which sample sits on which drum pad — it guesses notes based on General MIDI conventions (kick = C1/C2, snare = D1/D2), which is often wrong for user-loaded kits. Same problem for "add a filter sweep to the Serum bass" — the model can't see what synth is on the track. The plugin has no API to introspect its host's device tree (CLAP and VST3 both isolate plugins from neighbouring devices), so the host must push this info to us.

**Solution:** A small host extension (Bitwig for v1, Ableton in a follow-up) reads the project's track/device tree and pushes a `ProjectLayout` snapshot to the plugin process over HTTP. The plugin exposes three MCP tools providing tiered views:

- **`get_project_state`** — minimal "what's loaded" view. For each Droplets instance: track name, primary instrument device (with preset), or drum machine with pad list including **pad name, note (as pitch-notation string), and sample_name**. This is enough for the LLM to write a drum pattern with correct note mapping.
- **`get_track_info(instance)`** — full recursive device chain for one track. Drum pads with nested device chains, effects chain with device names + presets, no parameter lists yet.
- **`get_device_parameters(instance, device_path)`** — parameter detail for one addressed device. For the future "map CC to Delay > Amount" flow.

**Identity handshake:** Droplets exposes a read-only plugin parameter whose displayed-value string is the instance ID (`droplets-a1b2c3d4`). The host extension reads that via the Bitwig API's direct-parameter path to correlate a device-on-track with a plugin instance in the MCP server. The extension also auto-renames each Droplets instance to match its track name, so the LLM sees meaningful names (`drums`, `bass`) without user intervention.

**Project-wide payload:** The extension POSTs the full `ProjectLayout` in one call (not per-instance), keyed by track name. Tracks without Droplets on them are still included, so the LLM has complete context ("there's a Synth track I can't play, but the user may ask about it").

**Command channel (future):** A WebSocket endpoint (`/ws/controller`) streams `ControllerCommand`s from the plugin process to the extension — for things like "map CC 74 to Delay > Amount on track X." v1 ships the endpoint and the command queue, but no commands are emitted yet. The shape is reserved so later MCP tools (`map_cc_to_parameter`, etc.) can enqueue commands without protocol changes.

**Extension language:** Kotlin `.bwextension` (JVM is the only option for Bitwig extensions; Kotlin is more compact than Java and doesn't require Gradle — a two-line `kotlinc` + `jar` script produces the artifact). Lives in a sibling directory (`bitwig-extension/`) with its own README and build script, intentionally isolated from the Rust/TypeScript build.

**Graceful degradation:** In DAWs without the extension (Ableton, Logic, Live pre-Python-script), `get_project_state` returns an empty `ProjectLayout` and the LLM falls back to asking the user or to GM conventions.

**Status (2026-04-20 — code shipped, demo-machine walkthrough pending):**
- ✅ Phase 14a (Rust, DAW-agnostic): all 11 tasks done. Instance-ID plugin parameter, `ProjectLayout` types with three tiered serialization views, `POST /project_layout` HTTP endpoint, `WS /ws/controller` command stream, three MCP tools (`get_project_state` / `get_track_info` / `get_device_parameters`), updated system instructions telling the LLM to call `get_project_state` first and trust the returned drum map over GM conventions. 145 tests pass (11 new for the ProjectLayout types).
- ✅ Phase 14b (Bitwig extension): Kotlin `.bwextension` at [extensions/bitwig/](../../../extensions/bitwig/) (~380 LOC, Kotlin stdlib bundled, no Gradle/Maven — just `kotlinc` + `jar`). Walks `TrackBank(32) → DeviceBank(16) → DrumPadBank(128) → DeviceBank(8)`, detects Droplets via the direct-parameter display pattern (no hard-coded VST3 UID), auto-renames instances on first sight, debounces rebuilds at 150ms, POSTs the layout + connects the WS command channel. v1 gaps (no container chain walk, no native-Sampler `sampleName`, no param introspection, C1 root assumed for drum pads) captured in the extension README. End-to-end walkthrough on the demo machine tracked as 14b.10.
- ⏳ Phase 14c (Ableton): deferred, post-demo.

**Files (Rust side, shipped):** new [src/instance_param.rs](../../../src/instance_param.rs) (read-only param exposing the instance ID), [src/lib.rs](../../../src/lib.rs) (register `PluginParams`), new [src/mcp/project.rs](../../../src/mcp/project.rs) (all types + tiered views + 11 tests), [src/mcp/bridge.rs](../../../src/mcp/bridge.rs) (ProjectLayout storage + `resolve_instance_id`), [src/mcp/mod.rs](../../../src/mcp/mod.rs) (`POST /project_layout` + `WS /ws/controller` routes + broadcast channel), [src/mcp/server.rs](../../../src/mcp/server.rs) + [src/mcp/requests.rs](../../../src/mcp/requests.rs) (three MCP tools + request types), [src/mcp/instructions.md](../../../src/mcp/instructions.md).

**Files (Bitwig extension, shipped):** [extensions/bitwig/DropletsExtension.kt](../../../extensions/bitwig/src/main/kotlin/com/simply/droplets/DropletsExtension.kt) (observer wiring + rebuild), [DropletsClient.kt](../../../extensions/bitwig/src/main/kotlin/com/simply/droplets/DropletsClient.kt) (HTTP + WS), [DropletsExtensionDefinition.kt](../../../extensions/bitwig/src/main/kotlin/com/simply/droplets/DropletsExtensionDefinition.kt), [Json.kt](../../../extensions/bitwig/src/main/kotlin/com/simply/droplets/Json.kt), [build.sh](../../../extensions/bitwig/build.sh) + [install.sh](../../../extensions/bitwig/install.sh) + [README.md](../../../extensions/bitwig/README.md).

See [TASKS.md](TASKS.md) for the detailed task breakdown and resolved design decisions.

---

### Phase 02: Post-Demo Polish

#### Feature 8: Migrate UI to egui (in-process)

**Problem:** The transport-to-render pipeline has visible skew. Chain: audio thread → `ArcSwap<TransportState>` → GUI HTTP server → JSON → WebSocket (500ms) → browser → React → SVG. `TimingManager.ts` papers over the gap with client-side wall-clock interpolation, but drifts under JS event-loop jank or tempo changes between syncs. This is structural, not tuneable.

**Solution:** Replace the React frontend with an in-process egui UI. Collapses the chain to: audio thread → `ArcSwap` → UI thread → paint. Removes JSON serialization, WebSocket polling, and interpolation guesswork. Residual latency is ~one audio block (few ms). Current UI already does direct DOM manipulation (`progressRefs`, SVG playhead) to dodge React re-renders — that pattern maps cleanly onto immediate-mode GUI, so the port is structurally natural.

**Files:** New `src/ui/` module, wire into plugin window. Deprecate [frontend/](../../../frontend/) and [src/gui/](../../../src/gui/).

---

#### Feature 9: Evaluate stateful MCP mode

**Problem:** MCP server currently runs in stateless mode ([src/mcp/mod.rs:75](../../../src/mcp/mod.rs#L75)). Every request is independent — no sessions, no SSE, no server-initiated messages. The LLM cannot be told "fugue X just finished" or "instance 'lead' disconnected" without polling. GET `/mcp` returns 405 because streamable HTTP reserves GET for session-scoped SSE streams.

**Solution:** Switch `stateful_mode: true` and add session cleanup. Emit notifications from the fugue scheduler (on completion/cancellation) and bridge registry (on instance add/remove) through the rmcp server handle. Decide whether stateless fallback is kept for health probes.

**Files:** [src/mcp/mod.rs](../../../src/mcp/mod.rs), [src/mcp/server.rs](../../../src/mcp/server.rs), [src/fugue/bridge.rs](../../../src/fugue/bridge.rs).

---

#### Feature 10: Audio-thread ramps for per-note expression

**Problem:** Per-note pitch bend and pressure inside fugues currently use server-side expansion: the MCP handler materializes N discrete events per beat at parse time, and the audio thread dispatches them as `Instant` events. This is ergonomically identical for the LLM ("two points + curve") but limits resolution to the expansion density (typically 32/beat ≈ 64 Hz at 120 BPM). CC already has proper sample-accurate ramps via `ProcessedEvent::CcRamp` and cross-buffer state in `ActiveCcRamp`. Per-note should too, for MPE-style smoothness during slow sweeps.

**Solution:** Generalize `ProcessedEvent::CcRamp` to carry a `RampTarget` enum (`Cc { channel, cc }` | `PerNotePitchBend { channel, note }` | `PerNotePressure { channel, note }`). Unify `ActiveCcRamp` into an `ActiveRamp` with per-target state. Teach the MIDI processor ([src/midi/mod.rs](../../../src/midi/mod.rs)) to emit MIDI 2.0 per-note expression messages at interpolated values per sample block. Curve remapping continues to flow through `InterpolationMode::apply_curve`, so Exp/Log already work the moment the plumbing lands.

**Files:** [src/fugue/types.rs](../../../src/fugue/types.rs) (ProcessedEvent + RampTarget), [src/fugue/fugue.rs](../../../src/fugue/fugue.rs) (active ramp state), [src/midi/mod.rs](../../../src/midi/mod.rs) (per-sample per-note output), [src/mcp/server.rs](../../../src/mcp/server.rs) (stop doing server-side expansion for per-note).

---

#### Feature 15: Native drag-out of fugues → DAW clip (`.mid`)

**Problem:** Without drag-out, a user who wants to edit an AI-generated pattern has to either (a) rebuild it by hand in the DAW's piano roll, or (b) use an in-app editor we don't want to maintain. The previous "sequencer tab" in the frontend tried to be an editor and was broken; it was removed. We need a path from "fugues are playing in Droplets" to "this pattern is a clip in the DAW" so the DAW's native editor takes over.

**Solution:** Webview-initiated native OS drag. On `mousedown` over a drag zone the frontend sends an IPC message to the Rust side; the Rust side materializes a `.mid` file to the OS temp dir and calls the `drag` crate (v2.1, cross-platform wrapper around `NSFilePromiseProvider` / Windows `IDropSource` / XDND) using the wry webview's raw window handle as the drag source. Bitwig / Ableton / Finder / Logic all accept the drop. Exporter reuses [src/fugue/export.rs](../../../src/fugue/export.rs) (single-fugue MIDI writer already shipped); new code path merges all active fugues on an instance for the "drag all active" affordance.

**Scope breakdown:**
- **15.1 — Exporter: merge active fugues.** Extend [src/fugue/export.rs](../../../src/fugue/export.rs) to take `Vec<FugueDefinition>`, merge events per channel, optionally convert each slot/CC lane to a single CC track in the file. ~2 hours.
- **15.2 — `drag` crate + IPC wiring.** Add `drag = "2.1"` dependency, new IPC handler in [src/gui/webview.rs](../../../src/gui/webview.rs) that writes a temp `.mid` and calls `drag::start_drag(window_handle, path)`. Temp file cleaned up via drag-result callback. ~2 hours.
- **15.3 — Drag zones in UI.** Drag handle per-fugue in [FugueViewer.tsx](../../../frontend/src/components/FugueViewer.tsx); "Drag all active" button in [FugueList.tsx](../../../frontend/src/components/FugueList.tsx). Hook `dragstart` / `mousedown` → IPC. ~1 hour.
- **15.4 — HTTP endpoints for LLM/scripted use.** `GET /api/export/fugue/:id?instance=` and `GET /api/export/active?instance=` return `.mid` blobs directly — useful for MCP-driven flows ("give me a file I can email") and as a test harness. ~1 hour.

**Non-goals:** automation in the exported file is notes + CC only. Slot / host-param automation doesn't have a portable MIDI representation. Documented — to capture slot automation, arm automation lanes in the DAW and replay the fugue (the plugin emits `ParamValueEvent` so the DAW records it natively).

**Files:** [src/fugue/export.rs](../../../src/fugue/export.rs), [src/gui/webview.rs](../../../src/gui/webview.rs), [src/gui/routes.rs](../../../src/gui/routes.rs), [frontend/src/components/FugueViewer.tsx](../../../frontend/src/components/FugueViewer.tsx), [frontend/src/components/FugueList.tsx](../../../frontend/src/components/FugueList.tsx), `Cargo.toml` (add `drag = "2.1"`).

---

#### Feature 16: Native drag-in of `.mid` → fugue on an instance

**Problem:** After dragging out + editing in the DAW, there's no way back. Round-trip workflows (LLM sketches → user refines in piano roll → import back as the new canonical version) require drop-to-import.

**Solution:** Accept dropped `.mid` files on a drop zone in the per-instance UI. Frontend reads the file as `ArrayBuffer`, POSTs to `POST /api/import/fugue?instance=` (binary body, `audio/midi`). Rust side parses with [`midly`](https://crates.io/crates/midly) (already a dependency via the exporter), converts MIDI tracks to `FugueDefinition` events (inverse of `fugue_event_to_midi` in [src/fugue/export.rs](../../../src/fugue/export.rs)), and queues via `FugueBridge::queue`. Time resolution: use the `.mid`'s PPQ to convert ticks → beats at parse time.

**Scope breakdown:**
- **16.1 — MIDI → FugueEvent parser.** Inverse of [src/fugue/export.rs:92-143](../../../src/fugue/export.rs#L92-L143). Handle Note On / Off pairing (emit note events with duration), CC events (straight mapping). Skip per-note expression (not in MIDI 1.0). ~2 hours.
- **16.2 — HTTP endpoint.** `POST /api/import/fugue?instance=&tag=&loop_mode=&quantize=` body = `audio/midi`. Returns queued fugue ID. ~30 min.
- **16.3 — Drop zone in UI.** Per-instance area accepts `.mid` drop, uploads, refreshes fugue list. ~30 min.
- **16.4 — MCP tool.** `import_fugue(instance, base64_mid)` for LLM-initiated imports. ~30 min.

**Non-goals:** bidirectional fidelity — the exported `.mid` from Feature 15 isn't guaranteed to re-import exactly. DAW-edited clips in particular may differ (quantized grid, different curves). Good enough.

**Files:** [src/mcp/requests.rs](../../../src/mcp/requests.rs) (new `ImportRequest`), [src/fugue/import.rs](../../../src/fugue/import.rs) (new), [src/gui/routes.rs](../../../src/gui/routes.rs), frontend drop zone component.

---

#### Feature 17: MCP tool `get_fugue(id)` — read back a FugueDefinition

**Problem:** `list_fugues` today returns `FugueInfo` (id, tag, loop progress, timing) but not the actual event content. Composing a "verse 2" that picks up where verse 1 left off — or modifying a fugue the user edited via drag-in (Feature 16) — requires reading the notes, CC points, and curves back. The LLM currently has no path to inspect its own output; it can only emit fresh fugues.

**Priority:** bumped to P1. Even without the drag-in return-leg, `get_fugue` unlocks a read-modify-write loop that's valuable on its own: LLM queues v1 of a pattern, user listens, asks for an edit, LLM reads v1 and emits v2 as a diff rather than rewriting from memory. Small implementation footprint (~1 tool + thin bridge lookup) for outsized LLM ergonomic gain.

**Solution:** New MCP tool `get_fugue(instance, fugue_id)` returning the full `FugueDefinition` — or an error when the id isn't active on that instance. The scheduler already keeps definitions alive for the duration a fugue is queued/playing ([src/fugue/bridge.rs](../../../src/fugue/bridge.rs) `get_definitions`), so this is a thin wrapper: look up by id, serialize through the same path the UI uses.

**LLM-facing format:** same compact schema as `queue_fugue` accepts on input — notes as `[beat, note, duration?, velocity?, channel?]` arrays, CC / per-note lanes as `[beat, value, curve]` point lists. Symmetry with the input side means the LLM can read a fugue, mutate it, and re-queue it without schema translation.

**Scope breakdown:**
- **17.1 — Internal:** add `FugueBridge::get_definition(instance, id) -> Option<FugueDefinition>` beside the existing `get_definitions`. ~15 min.
- **17.2 — MCP tool:** new `#[tool]` fn in [src/mcp/server.rs](../../../src/mcp/server.rs) with the thin lookup + serde_json render. ~30 min.
- **17.3 — Compact-schema serializer:** add a `FugueDefinition → CompactFugue` converter that mirrors the inverse of the parser path — flatten `TimedFugueEvent` streams back into `FugueContent::Composite { notes, cc, pitch_bends, pressures }`. Groups NoteOn/NoteOff pairs by matching beat+note+channel. ~2 hours — non-trivial because the internal representation is event-stream, not lane-grouped. For v1 we can ship the raw `FugueDefinition` and add the compact view only if the LLM actually struggles with the event stream.
- **17.4 — Docs:** update `instructions.md` with "read-modify-write" pattern.

**Files:** [src/fugue/bridge.rs](../../../src/fugue/bridge.rs) (new getter), [src/mcp/server.rs](../../../src/mcp/server.rs) (tool), [src/mcp/requests.rs](../../../src/mcp/requests.rs) (optional compact serializer), [src/mcp/instructions.md](../../../src/mcp/instructions.md).

---

#### Feature 18: Pause instead of delete on tag replacement

**Problem:** When a fugue is cancelled by a same-tagged replacement (`cancel_mode: "tag:<name>"`), it's gone. If the user liked the old version better than the new one, the only way back is to ask the LLM to regenerate — which may produce a meaningfully different output each time. There's no "undo" at the musical level.

**Solution:** When a tag-based cancel fires, stash the cancelled fugue into a per-instance paused-fugue ring buffer instead of discarding it. Expose:

- **MCP tool `list_paused(instance, tag?)`** — show what's in the pause buffer. Default returns the most recent 10 entries; `tag` filters to one lineage.
- **MCP tool `restore_fugue(instance, fugue_id)`** — re-queue a paused fugue as if `queue_fugue` had just been called with it. The restored fugue gets a fresh live id; the original stays in the pause buffer for chain-undo.
- **UI affordance** in the sequencer tab — per-tag "history" list showing stashed versions with one-click restore.

**Scope + caveats:**
- **Bounded retention**: keep the last 8 entries per tag per instance. Pause buffer is session-scoped (cleared on plugin unload) — this is a working undo, not a persistent archive.
- **Complements Feature 17**, doesn't overlap: `get_fugue` is for the LLM's read-modify-write workflow; pause-stash is for the user's undo workflow. Both touch the same data from different angles.
- **Compact state**: store only the `FugueDefinition` (notes + CC + expression lanes), not runtime state (playhead, loop count). On restore, playback starts fresh from beat 0 — the "undo" is musical content, not timing position.

**Files:** [src/fugue/bridge.rs](../../../src/fugue/bridge.rs) (registry gains `paused: HashMap<String, VecDeque<FugueDefinition>>`), [src/fugue/sequencer.rs](../../../src/fugue/sequencer.rs) (intercept tag-cancel, stash instead of drop), [src/mcp/server.rs](../../../src/mcp/server.rs) (new tools), frontend sequencer-tab history panel.

---

#### Feature 19: Unify GUI + MCP on a single port with three top-level paths

**Problem:** Droplets currently binds **two** TCP ports per plugin process — `:9998` for the GUI HTTP server + WebSocket, and `:9999` for the MCP server's `/mcp` + `/ws/controller` + bare `/project_layout` + bare `/rename_instance`. Two ports doubles firewall/sandbox coordination cost, doubles bind-time race risk, and splits the URL layout: `/api/*` routes live on `:9998` while the extension-facing routes sit bare at root on `:9999`.

**Solution:** One port, three clean top-level paths. Primary port stays **9999** (external MCP agents are already configured for it — preserves compatibility). Everything collapses onto 9999 under:

```
http://localhost:9999/
  /api/*          ← HTTP API: frontend + extension POSTs
                    (includes /api/project_layout, /api/rename_instance)
  /mcp            ← MCP protocol (unchanged)
  /ws             ← WebSocket: GUI frontend (unchanged)
  /ws/controller  ← WebSocket: extension command stream (unchanged)
```

The MCP-side bare `/project_layout` and `/rename_instance` routes go away — both already have `/api/*` equivalents the frontend uses, and the extension just switches to hitting those.

**Scope:**
- Merge the MCP router into the GUI axum app via `.merge()`. Drop the `:9998` listener entirely.
- Change the unified default from `DEFAULT_GUI_PORT` (9998) to `DEFAULT_MCP_PORT` (9999).
- Delete the duplicate bare extension routes in [src/mcp/mod.rs](../../../src/mcp/mod.rs) — frontend's `/api/project_layout` + `/api/rename_instance` handlers remain authoritative.
- Update the Bitwig extension to hit `/api/project_layout` and `/api/rename_instance` at port 9999 ([extensions/bitwig/src/main/kotlin/com/simply/droplets/DropletsClient.kt](../../../extensions/bitwig/src/main/kotlin/com/simply/droplets/DropletsClient.kt)).
- Collapse the standalone binary's two ports (9996 GUI + 9997 MCP) to one (9997).
- Settings UI + header MCP-URL display reflect the single port.

**Caveat:** any external tool pointing at `:9998` breaks. Internal consumers (frontend, Bitwig extension, standalone binary) are all updated in the same change.

**Files:** [src/mcp/mod.rs](../../../src/mcp/mod.rs), [src/gui/server.rs](../../../src/gui/server.rs), [src/bin/standalone.rs](../../../src/bin/standalone.rs), [src/lib.rs](../../../src/lib.rs) (start one server, not two), [extensions/bitwig/src/main/kotlin/com/simply/droplets/DropletsClient.kt](../../../extensions/bitwig/src/main/kotlin/com/simply/droplets/DropletsClient.kt).

---

#### Feature 20: Dynamic port selection on bind conflict

**Problem:** The plugin hard-codes port 9998 (and 9999 until Feature 19 lands). If that port is in use (previous plugin instance not fully torn down, unrelated process, test harness), the plugin silently fails to bind and the UI/MCP server is simply missing. No user-visible error and no recovery.

**Solution:** On startup, try the canonical port first; if bind fails with `AddrInUse`, walk a bounded range (e.g., 9998, 10000, 10001, … up to 9998 + 20) until a bind succeeds. Surface the chosen port:

- **Into the plugin state** so the frontend can fetch it (`getSettings().mcp_url` already returns the current URL — just make sure this path reflects the actual bound port, not the default).
- **Into the Bitwig extension** via the existing Droplets-instance direct-parameter display. Add a second param slot carrying the port number as a display string; the extension reads it the same way it reads the instance ID.

**Caveats:**
- The Bitwig extension can no longer assume `:9998` — it reads the port from the instance's param-display. Backwards compat: fall back to 9998 when no port display is available (old plugin, new extension).
- Port changes across plugin reloads are a minor nuisance (cached MCP client connections may go stale). Mitigated by short-lived connections on the plugin side.

**Files:** [src/gui/server.rs](../../../src/gui/server.rs) (bind loop), [src/mcp/mod.rs](../../../src/mcp/mod.rs) (same if Feature 19 not landed yet), [src/instance_param.rs](../../../src/instance_param.rs) (new port-display param), [extensions/bitwig/src/main/kotlin/com/simply/droplets/DropletsExtension.kt](../../../extensions/bitwig/src/main/kotlin/com/simply/droplets/DropletsExtension.kt) (read the port display alongside the instance ID).

---

## Implementation Order

```
Phase 01:
  1 (per-note in fugues)                       ← reshapes schema, do first  [done]
  ↓
  2 (system prompt) + 3 (queue_fugue example)  ← describe full vocabulary in one pass  [done]
  ↓
  4 (FUGUE.md rewrite)                         ← mirrors the new prompt  [done]
  ↓
  5 (get_transport)                                                       [done]
  ↓
  11 (composite fugue type)                    ← biggest remaining LLM-ergonomics fix;
                                                 touches prompt + FUGUE.md + tool desc,
                                                 so do BEFORE 12 which depends on it
  ↓
  12 (UI lanes for bend/pressure)              ← depends on 11: composite fugues aren't
                                                 useful if the UI can't render them
  ↓
  13 (transport phase-locking)                ← stop/play reliability  [done]
  ↓
  14 (DAW track context)                       ← AI can finally see drums and synths;
                                                 14a Rust side [done 2026-04-20],
                                                 14b Bitwig extension [done 2026-04-20],
                                                 14c Ableton deferred post-demo
  ↓
  6 (multi-instance validation) + 7 (UI verify) ← both on demo machine; final
  ↓
  2026-04-21: DEMO
  ↓
Phase 02: 8 (egui migration) + 9 (stateful MCP) + 10 (audio-thread per-note ramps)
          17 (get_fugue) → 15 (drag-out) → 16 (drag-in) [done/in progress]
          ↑ 17 first — tiny surface, independent of drag plumbing, and its
            compact read-back serializer gets reused by 15 when bundling
            multiple fugues into a .mid. 15 then lands the primary hand-off
            flow. 16 closes the full round-trip last.

          Infra cleanups, order independent of the features above:
          19 (single port) → 20 (dynamic port)
          ↑ 19 first — halving the port count simplifies the bind-retry
            logic 20 has to implement. Doing them in this order means 20
            only walks one port range, not two.

          Ergonomic polish:
          18 (pause-instead-of-delete) — post-drag-out because it layers
            on top of the existing tag-swap mechanic and doesn't block
            anything else.
```

---

## Key Files

| Phase | Files |
|-------|-------|
| 01 | [src/mcp/server.rs](../../../src/mcp/server.rs), [src/fugue/types.rs](../../../src/fugue/types.rs), [src/fugue/](../../../src/fugue/), [src/fugue/bridge.rs](../../../src/fugue/bridge.rs), [docs/FUGUE.md](../../FUGUE.md), [INSTALL.md](../../../INSTALL.md) |
| 02 | New `src/ui/`, retire [frontend/](../../../frontend/) and [src/gui/](../../../src/gui/) |
