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
| [x] | P0 | 14 | DAW track context (drum maps, device names) via host extension | L | Very High — AI currently picks random notes for drums because it has no way to know which sample is on which pad. **Rust side + Bitwig extension shipped 2026-04-20; walkthrough verified same day.** |
| [ ] | P0 | 25 | Align fugue iteration boundaries to song-grid at promotion time | S | High — `start_pending_fugues` uses the quantize target as `start_beat`, so first iteration sits off-grid and `phase_lock` shifts it on any later transport jump. Symptom: "queue while transport plays" plays at one offset, then a DAW wrap snaps every subsequent bar to a different offset |
| [ ] | P0 | 26 | Widen `process_buffer` `local_start` tolerance to cover DAW wrap drift | S | High — DAWs report transport ~half a buffer into the new loop on wrap, dropping beat-0 events. Pairs with 25 to close the first-note-on-loop bug |
| [ ] | P1 | 27 | Integer-tick (PPQ 960) representation in sequencer hot path | M | Very High — eliminates the entire float-drift bug class in `phase_lock` / `process_buffer` / `reset_for_loop`. Replaces `beat_offset: f64` with `tick_offset: i64` inside the audio thread; LLM and UI surfaces stay in beats via conversion at boundaries |

### Phase 02: Post-Demo Polish

| Done | Pri | # | Feature | Complexity | Impact |
|------|-----|---|---------|------------|--------|
| [ ] | P2 | 8 | Migrate UI from React/HTTP to egui (in-process) | XL | Correctness — removes transport skew |
| [ ] | P3 | 9 | Evaluate stateful MCP mode for server→client push | S | Medium — unlocks event notifications (fugue-finished, instance-changed) |
| [ ] | P2 | 10 | Audio-thread ramps for per-note expression (pitch bend, pressure) | M | Quality — sample-accurate per-note curves instead of server-side discrete-event expansion |
| [x] | P1 | 17 | MCP tool: `get_fugue(id)` returning current FugueDefinition | S | High — small surface, immediately unlocks LLM read-modify-write; ships before the drag features because it's independent and its compact serializer is reusable downstream |
| [x] | P1 | 15 | Native drag-out of fugues → DAW clip (`.mid` file) | M | High — lets users hand AI-generated patterns to the DAW's piano roll for editing; removes the need for an in-app editor |
| [x] | P2 | 16 | Native drag-in of `.mid` → new fugue on an instance | M | Medium — round-trip workflow: edit in the DAW, drop back as a fugue |
| [ ] | P3 | 18 | Pause instead of delete on tag replacement | M | Medium — lets the user walk back to a prior version of a part instead of losing it forever when the LLM queues a new fugue with the same tag |
| [ ] | P2 | 19 | Unify GUI + MCP on a single port with three top-level paths | S | Medium — halves port consumption per plugin process; simpler firewall / sandbox story. Primary port stays **9999** (agents already configured). Top-level layout collapses to just `/api` (HTTP calls), `/mcp` (MCP protocol), and `/ws` (WebSocket upgrade). Extension POSTs move under `/api/*`; the MCP-side bare `/project_layout` + `/rename_instance` go away. |
| [ ] | P2 | 20 | Dynamic port selection on bind conflict | S | Medium — plugin currently dies if :9998/:9999 are in use. Walk a range, bind the first free port, surface the chosen port to the extension + UI |
| [ ] | P3 | 21 | MPE round-trip for per-note expression in drag-out/drag-in | L | Medium — today Features 15/16 drop pitch bend + pressure because MIDI 1.0 SMF has no per-note target. MPE (channel-per-note encoding) is understood by Logic, Bitwig, Live 12+, Cubase, so a `.mid` emitted in MPE shape round-trips the full expressivity. Cost: dynamic channel allocation on export, channel→note attribution on import |
| [ ] | P4 | 22 | Adopt MIDI 2.0 / SMF2 for native per-note support | L | Low (today) — SMF2 has true per-note bend/pressure in the format itself. DAW support is inconsistent as of 2026; revisit once Logic / Bitwig / Live all read SMF2 natively. Supersedes Feature 21 when that happens |
| [ ] | P3 | 23 | `.bwclip` (dawproject) export/import alongside `.mid` | M | Medium (Bitwig-only) — lets users round-trip launcher clips via Bitwig's "Save Launcher Clip to Library" (which Bitwig refuses to emit as `.mid`). Preserves per-note expression, tag, color, and timing metadata that MIDI 1.0 drops. Format is Bitwig's open dawproject spec (XML in a zip). Settings toggle chooses `.mid` vs `.bwclip` for drag-out |
| [ ] | P2 | 24a | Expose `CLAP_PLUGIN_AS_VST3` extension (Rust side) | S | Medium — the clap-wrapper already honours `vst3info->features` as a direct SubCategories override ([wrapasvst3_entry.cpp:269-276](https://github.com/free-audio/clap-wrapper/blob/main/src/wrapasvst3_entry.cpp#L269-L276)). Wire this on the Rust side (clack patch or raw FFI), emit `Fx\|Tools`, drop VST3 audio bus. Ableton stops treating Droplets as an instrument. Notes flow through fine; CC output stays broken until 24b |
| [ ] | P2 | 24b | clap-wrapper PR: translate `CLAP_EVENT_MIDI` out on VST3 | M | Medium — today [process.cpp:808-812](https://github.com/free-audio/clap-wrapper/blob/main/src/detail/vst3/process.cpp#L808-L812) silently swallows `CLAP_EVENT_MIDI` / `_SYSEX` / `_MIDI2` in `enqueueOutputEvent`. Fix is a surgical addition alongside the existing NOTE_ON/OFF cases: parse status byte, fan out to `kLegacyMIDICCOutEvent` / `kDataEvent`. Upstream PR against clap-wrapper (issue [#414](https://github.com/free-audio/clap-wrapper/issues/414)) |
| [ ] | P2 | 28 | Notes-with-duration representation in the audio thread | L | High — ships a single `TimedNote { tick_offset, duration_ticks, ... }` to the audio thread instead of pre-expanded NoteOn/NoteOff pairs. Absorbs `emit_notes` overlap truncation, zero-duration guard, NoteOff left-skew, and the same-sample deconflict pass. Requires a bounded `PendingNoteOff` buffer on the audio thread (no malloc), so polyphony-cap + pre-alloc design work up front |
| [ ] | P2 | 29 | Fold the parked `audio_debug` probe back in as a Cargo feature | S | Medium — audio-thread debug probe with lock-free ring buffer + background drainer, parked on bookmark `wip/audio-debug-probe`. Landing on trunk needs a `audio-debug` Cargo feature so call sites compile out entirely when disabled, and all DebugRecord construction moves inside `audio_debug.rs` so probes at call sites are single feature-gated lines. Keep for future sequencer debugging sessions |

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

#### Feature 25: Align fugue iteration boundaries to song-grid at promotion time

**Problem:** The fugue scheduler has two places that place a fugue on the transport timeline: `start_pending_fugues` in [src/fugue/sequencer.rs](../../../src/fugue/sequencer.rs) (when a waiting fugue is first promoted to active), and `phase_lock` in [src/fugue/fugue.rs](../../../src/fugue/fugue.rs) (on transport start / jump). They disagree:

- `start_pending_fugues` sets `start_beat = target`, where `target` is the quantize grid line the fugue naturally falls on (next bar, next beat, etc.). For a fugue with `dur = 16` queued at transport 5, `target = 8` (next bar in 4/4) — **not a multiple of 16**. Iteration boundaries land at 8, 24, 40, … — **off the song-grid**.
- `phase_lock` sets `start_beat = transport_beat - transport_beat.rem_euclid(dur)`, which forces a multiple of `dur` from song-zero. For the same fugue at the first transport jump, iteration boundaries snap to 0, 16, 32, …

Observable symptom (from an audio-thread debug trace of the real bug):
- User queues a 16-beat drum pattern. LLM latency puts `target = 8`. Fugue plays with kick on bars 3, 7, 11.
- DAW transport wraps back to 0. `phase_lock` fires, `old_start_beat=8 → new_start_beat=0`. Kick is now on bars 1, 5, 9 — **shifted by 8 beats (2 bars) for the rest of the session.**

The right fix is not "preserve the queue-time position across jumps" (the user is right to reject that) — LLM latency is arbitrary, so using the moment-of-queue as an anchor makes the fugue's musical position depend on call latency. The correct semantics: **iterations are always at `k · duration_beats` from song-start, regardless of when the fugue was queued.**

**Solution:** make `start_pending_fugues` do the same song-grid snap that `phase_lock` already does.

```rust
let target = fugue.target_start_beat.unwrap_or(current_beat);
let dur = fugue.definition.duration_beats;
let phase = target.rem_euclid(dur);
fugue.start_beat = target - phase;                           // virtual start, on song-grid
fugue.next_event_index = fugue.definition.events
    .partition_point(|e| e.beat_offset < phase);             // skip events before the current phase
```

Under these semantics, a fugue queued at `target = 8` with `dur = 16` plays pattern-beat-8 at transport 8 (the second half of the pattern plays "immediately"), and pattern-beat-0 fires at transport 16 — when the song-grid iteration actually begins. Every subsequent iteration is at 32, 48, 64 — consistent with `phase_lock`'s behavior under transport jumps. No shift on DAW wrap.

If the LLM wants pattern-beat-0 to fire at the queue moment, it sets `quantize` to match `duration_beats` (e.g. `bars:4` for a 16-beat pattern) — then `target` is guaranteed to be a multiple of `dur` and `phase = 0`.

**Tradeoff:** this changes the semantics of `quantize` for fugues whose `quantize interval < dur`. The LLM-facing docs need to state explicitly that the fugue plays "the slice of the pattern matching current song-phase" when queued off its own grid. Update [src/mcp/instructions.md](../../../src/mcp/instructions.md) and the `queue_fugue` tool description with one example.

**Obsoleted by Feature 27:** once timing is in integer ticks, the `rem_euclid` here is exact and this fix is a one-line change inside the tick-based code path. The fix lands in the beat-based world first so the demo has correct behavior regardless of whether Feature 27 ships in time.

**Files:** [src/fugue/sequencer.rs](../../../src/fugue/sequencer.rs) (`start_pending_fugues`), [src/mcp/instructions.md](../../../src/mcp/instructions.md), [src/mcp/tools/queue_fugue.md](../../../src/mcp/tools/queue_fugue.md).

---

#### Feature 26: Widen `process_buffer` `local_start` tolerance for DAW wrap drift

**Problem:** When a DAW transport wraps (loop end → loop start), the plugin is typically called with `current_beat` reporting *into* the new loop iteration rather than exactly on its start — observed in a real trace as `transport = 0.015` (≈7 ms / ≈350 samples) after a wrap. The previous buffer already consumed that slice of the new iteration's samples internally; the DAW just advances its reported transport.

`process_buffer` computes `local_start = current_beat - start_beat`. After `phase_lock` with the wrap's drift-shifted transport, `local_start ≈ 0.015` instead of `0`. The existing tolerance (`beats_per_sample ≈ 0.00002`) is ~1000× too small, so events at `beat_offset = 0` fail `0 >= local_start - tolerance` and are silently advanced past. Symptom: pattern-beat-0 NoteOns don't fire on DAW loop wrap.

**Solution:** Widen the `>= local_start` tolerance to one full buffer (`frames * beats_per_sample`). The `next_event_index`-forward-only invariant in `process_buffer` prevents double-firing: events already fired in a prior buffer have `next_event_index` past them and aren't re-checked, so widening the look-back gate only rescues events whose "scheduled time" falls in the slice the DAW ate during its wrap.

**Pairs with:** Feature 25. Together they close the "first note doesn't play on DAW loop" bug for both anchor-at-0 and anchor-mid-song cases.

**Note:** Feature 27 (integer ticks) makes this tolerance cleaner — it becomes a fixed number of ticks rather than a computed f64. The underlying cause (DAW mid-buffer wrap reporting) is independent of numeric representation.

**Files:** [src/fugue/fugue.rs](../../../src/fugue/fugue.rs) (`process_buffer` tolerance).

---

#### Feature 27: Integer-tick representation in sequencer hot path

**Problem:** Every bug this demo cycle has surfaced in the sequencer has been some shape of f64 drift: `rem_euclid` producing `1e-15` off, `local_start = 0.015` after DAW wrap, `partition_point` with `<` excluding beat-0 events, `.round()` pushing `sample_offset` past `frames`, tolerance-vs-strict-comparison juggling at every boundary check. Each one individually is ~5 lines to patch, but the class is unbounded because floating-point arithmetic isn't associative and the scheduler threads the transport through many comparison gates.

**Solution:** Represent all sequencer-internal timing as **integer ticks** at 960 PPQ (the MIDI standard). One f64→i64 conversion at the buffer boundary; everything inside is exact integer comparison.

- `TimedFugueEvent.beat_offset: f64` → `tick_offset: i64`
- `FugueDefinition.duration_beats: f64` → `duration_ticks: i64`
- `Fugue.start_beat: f64` → `start_tick: i64`
- `Fugue.anchor_start_beat: f64` (from Feature 25) → `anchor_tick: i64`
- Per-buffer: `current_tick = (transport_beat * 960.0).round() as i64` — bounded rounding error per buffer, never accumulates because the DAW's transport is re-read fresh each call.
- Sample-offset emission still does one f64 op per fired event (`(tick_delta as f64 / ticks_per_sample).floor()`), but the gate comparisons (`beat_offset >= local_end`, `beat_offset >= local_start - tolerance`, phase math, reset boundary) are all i64.

**Scope:**
- [src/fugue/types.rs](../../../src/fugue/types.rs): `TICKS_PER_BEAT: i64 = 960`, `beats_to_ticks` / `ticks_to_beats` helpers. Change `TimedFugueEvent` / `FugueDefinition` fields.
- [src/fugue/fugue.rs](../../../src/fugue/fugue.rs): `Fugue` state, `process_buffer` gates, `phase_lock`, `reset_for_loop`, `process_active_ramps`.
- [src/fugue/sequencer.rs](../../../src/fugue/sequencer.rs): `process`, `start_pending_fugues`, `process_active_fugues`, `tag_loop_boundaries`.
- [src/fugue/bridge.rs](../../../src/fugue/bridge.rs): `FugueInfo` derivation (converts ticks back to beats for UI).
- [src/mcp/types/conversion.rs](../../../src/mcp/types/conversion.rs) + [src/mcp/types/emit.rs](../../../src/mcp/types/emit.rs): convert LLM-input beats → ticks at queue time; inverse for `definition_to_compact`.
- Frontend: `TimedFugueEvent` has a new `tick_offset` field (ts-rs regenerates); `FugueGrid` converts via `tick / 960` for rendering. Or: custom serde to keep wire format as `beat_offset: f64` (decide based on UI-side churn budget).
- [src/fugue/import.rs](../../../src/fugue/import.rs) + [src/fugue/export.rs](../../../src/fugue/export.rs): MIDI PPQ → 960 ticks conversion.
- Tests: update `sequencer.rs::daw_loop_tests` harness, `conversion.rs` round-trips, `emit.rs` unit tests.

**Obsoletes:** the `partition_point` tolerance in `phase_lock`, the `.floor()` guard in `sample_offset` calc (becomes unambiguous in integer space), and the ad-hoc `beats_per_sample` tolerance in `process_buffer` (becomes a fixed integer tick budget).

**Non-goals:** integer-tick representation of CC ramp state (`ActiveCcRamp`) — those carry start/end *beats* which the audio thread interpolates per sample; refactoring them follows the same pattern but is independent of the event-scheduling fix.

**Files:** above.

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

#### Feature 21: MPE round-trip for per-note expression in drag-out/drag-in

**Problem:** Features 15 (drag-out) and 16 (drag-in) lose per-note pitch bend and per-note pressure at the MIDI file boundary. The export path drops the `note` field because MIDI 1.0's `PitchBend` / `Aftertouch` are channel-wide; the import path drops bend/aftertouch for the same reason (we cannot know which note a channel-level bend was meant for). For composite fugues with expressive trajectories this is a meaningful fidelity loss on the drag-out edit round-trip.

**Solution:** Adopt **MPE** (MIDI Polyphonic Expression) as the wire encoding for drag-out/drag-in. MPE is a convention over MIDI 1.0: notes in a zone get assigned one channel each (typically ch 2–16 around ch 1 as the global channel), so channel-level pitch bend and CC74 effectively carry per-note values. Every modern DAW that matters (Logic, Bitwig, Ableton Live 12+, Cubase) both writes and reads MPE natively — so a `.mid` emitted in MPE shape round-trips correctly.

**Scope:**
- **Export:** during `merge_fugues_by_tag`, detect fugues that contain `PerNotePitchBend` or `PerNotePressure` events and switch those tracks into MPE mode: allocate a channel per overlapping held note (round-robin within the master zone), emit the per-note bend/pressure as channel-level pitch bend / CC74 on the allocated channel. Emit an MPE configuration meta (RPN 6 / 7) at the top of the track so DAWs auto-enable MPE mode on import.
- **Import:** detect the MPE configuration meta (or pitch bend density that looks like MPE — every note-on uses a unique channel in a zone). When detected, attribute channel-level bend / CC74 / aftertouch to the currently-held note on that channel. Unknown-shape files stay on the current lossy path.
- **Fallback:** Rust-side `ExportOptions { mpe: bool, auto: bool }` and `ImportOptions { mpe: AutoMpe }`. Auto-detection is the default; explicit override is for debugging.

**Non-goals:** in-plugin MIDI I/O does not change — the plugin still emits MIDI 2.0-style per-note expression to the DAW's synth. MPE is purely a wire format for the drag lanes.

**Files:** [src/fugue/export.rs](../../../src/fugue/export.rs) (per-track channel allocation + MPE meta), [src/fugue/import.rs](../../../src/fugue/import.rs) (MPE detection + channel-to-note attribution), new `src/fugue/mpe.rs` for the shared channel-zone logic.

---

#### Feature 22: Adopt MIDI 2.0 / SMF2 for native per-note support

**Problem:** MPE (Feature 21) is a workable encoding but requires channel juggling on both ends. MIDI 2.0 resolves this with proper per-note pitch bend and per-note pressure messages in the format itself; SMF2 (MIDI Clip File) specifies the file container.

**Solution — deferred:** Re-evaluate once DAW support matures. As of 2026, Logic / Bitwig / Ableton Live / Cubase read SMF2 inconsistently (or not at all), so emitting SMF2 by default would break the drag hand-off for most users. Once support is broad enough that a SMF2 file opens correctly in ≥3 of the 4 major DAWs, Feature 22 supersedes Feature 21: the exporter emits SMF2 natively, the importer reads SMF2 in addition to SMF1/MPE.

**Scope (when we pick it up):**
- Add `midly`-equivalent crate (or extend `midly`) with SMF2 support.
- Drop MPE channel-juggling in favour of direct per-note event emission / parsing.
- Keep SMF1/MPE emit path alongside SMF2 for backwards compat — gated by an option, default follows the majority-DAW-support signal at the time.

**Tracking:** watch `midly`, DAW release notes, and the AMEI / MIDI Association's SMF2 adoption tracker for a signal to move.

**Files:** TBD when picked up.

---

#### Feature 23: `.bwclip` (dawproject) export/import alongside `.mid`

**Problem:** Bitwig's clip launcher refuses to emit `.mid` when the user drags a clip — the only export it exposes for launcher clips is "Save Launcher Clip to Library", which produces a `.bwclip` file (Bitwig's proprietary container from the open dawproject spec). So the most natural "edit in Bitwig → hand back to the LLM" flow *inside Bitwig* is currently blocked for launcher clips. Users have to bounce to the arranger timeline first, which disrupts the improvisation loop the launcher is designed for.

`.bwclip` also preserves things MIDI 1.0 drops: per-note pitch bend and pressure (losslessly, unlike MPE's channel-encoding), clip name, clip color, launcher slot, time signature, scene metadata. For a Bitwig-centric workflow that fidelity matters.

**Solution:** Add a `.bwclip` encoder and decoder alongside the existing `.mid` ones. Settings gains an export-format toggle (`Mid` default, `BwClip` opt-in). Drag-out produces the chosen format; drag-in sniffs the file type (zip magic → bwclip, "MThd" → mid) and routes to the right parser.

- **Format reference:** dawproject spec at https://github.com/bitwig/dawproject (open-source, Java reference impl). `.bwclip` is the clip-level subset of the full dawproject format.
- **Rust libraries:** `zip` for the container, `quick-xml` (serde) for the XML body. Both are widely used and small.
- **Scope of the XML we write/read:** Notes (with per-note bend + pressure as native dawproject elements), CC lanes, clip name, clip length, loop/launch metadata. Skip automation envelopes for parameters outside our schema — export as no-ops.
- **Settings UI:** add `export_format: "mid" | "bwclip"` to the settings pane. Import auto-detects; export defers to the toggle.

**Non-goals:** full dawproject round-trip (mixer, effects, automation). This is strictly clip-level. Also: no other-DAW support from this work — `.bwclip` is Bitwig-only by design; Logic's `.alc` / Ableton's `.alc` would each be separate features.

**Tradeoff vs. Feature 21 (MPE):** both address the same "per-note expression loss" symptom. MPE is portable (every modern DAW reads it as `.mid`); `.bwclip` is richer but Bitwig-only. Ship MPE first for cross-DAW reach, ship `.bwclip` second for Bitwig-specific fidelity. The two don't compete — users pick the format their DAW workflow rewards.

**Files:** new `src/fugue/bwclip.rs` (encoder + decoder), [src/gui/api.rs](../../../src/gui/api.rs) (format routing in import_fugue + export_fugue), [src/fugue/settings.rs](../../../src/fugue/settings.rs) (new `export_format` field), [frontend/src/components/Settings.tsx](../../../frontend/src/components/Settings.tsx) (UI toggle), `Cargo.toml` (`zip`, `quick-xml`).

---

#### Feature 24: Ableton Live VST3 MIDI-effect support (two upstream fixes)

**Problem:** In Ableton Live, Droplets currently lands in the Instruments bucket — adding it to a track replaces whatever instrument was there (drum rack, synth, sampler). This is Ableton's one-instrument-per-track rule kicking in. Bitwig is fine because CLAP note-effect plugins route correctly; Ableton's CLAP support is too recent / inconsistent for most of our target users, so VST3 is the practical path.

Full Ableton-VST3 MIDI-effect support needs **both** 24a and 24b. 24a alone lets Droplets coexist with a synth on the same track; 24b is additionally required for CC output to actually reach the downstream synth (without it, notes flow through but filter-sweep CCs etc. are silently dropped).

**Investigation (2026-04-20):** attempted two in-plugin fixes that both failed:
1. **Drop audio bus, use `[UTILITY]` category.** Ableton refused to instantiate — the clap-wrapper likely synthesizes an audio bus regardless of our `count()=0` report, and the mismatch confuses Ableton.
2. **Keep audio bus, drop INSTRUMENT token from features.** The wrapper's `NOTE_EFFECT → Instrument|Synth` mapping ([categories.cpp:62](https://github.com/free-audio/clap-wrapper/blob/main/src/detail/vst3/categories.cpp#L62)) forces `Instrument` into the category regardless of our features — no way around it via CLAP features alone.

---

**Feature 24a: expose `CLAP_PLUGIN_AS_VST3` on the Rust side (~1 day).**

The clap-wrapper already supports this ([wrapasvst3_entry.cpp:269-276](https://github.com/free-audio/clap-wrapper/blob/main/src/wrapasvst3_entry.cpp#L269-L276)):

```cpp
if (vst3info && vst3info->features)
  features = vst3info->features;        // direct SubCategories override
else
  features = clapCategoriesToVST3(clapdescr->features);
```

Returning a `clap_plugin_info_as_vst3_t { features = "Fx|Tools", ... }` from our plugin makes the wrapper emit `"Fx|Tools"` as the VST3 SubCategories — fully bypassing the NOTE_EFFECT→Instrument mapping. Combined with `count()=0` on audio ports, Ableton classifies as a MIDI effect.

Blocker: clack doesn't expose this wrapper-specific extension. Two paths:
- **Upstream contribution to clack** — cleanest. The extension is a tiny struct (vendor string + component ID + features string); mechanical addition alongside existing extension bindings.
- **Raw FFI in this repo** — add the struct + `get_extension` hook ourselves, parallel to clack. Keeps the change local but maintains a small FFI surface.

---

**Feature 24b: clap-wrapper PR for `CLAP_EVENT_MIDI` output (~2-3 days).**

Today ([process.cpp:808-812](https://github.com/free-audio/clap-wrapper/blob/main/src/detail/vst3/process.cpp#L808-L812)):

```cpp
case CLAP_EVENT_MIDI:
case CLAP_EVENT_MIDI_SYSEX:
case CLAP_EVENT_MIDI2:
  return true;        // silently swallowed
  break;
```

The fix is a surgical addition mirroring the existing NOTE_ON/OFF handling in the same function (~50 lines of C++ per case):

- `CLAP_EVENT_MIDI`: parse status byte, fan out to `Steinberg::Vst::Event::kLegacyMIDICCOutEvent` for CC / PitchBend / Aftertouch / ProgramChange.
- `CLAP_EVENT_MIDI_SYSEX`: translate to `Event::kDataEvent` with `DataEvent::kMidiSysEx` type.
- `CLAP_EVENT_MIDI2`: leave as commented-out TODO per wrapper upstream's deferral of MIDI 2 SDK work.

Upstream bug: [clap-wrapper #414](https://github.com/free-audio/clap-wrapper/issues/414). Maintainer response pending but the issue is open and has clear precedent in the same file.

---

**Temporary workaround (shipped, documented in [README.md](../../../README.md) §2):** Ableton users put Droplets on a separate MIDI track and route MIDI output into their instrument track via `MIDI From: <droplets track>`. Works today, zero plugin changes.

**Non-goals:** Logic and Cubase classification are already correct (they accept the current VST3 categorization as instrument without the destructive replace behavior). This feature is Ableton-specific.

**Files (when picked up):**
- 24a: [src/lib.rs](../../../src/lib.rs) (extension registration), [src/midi/ports.rs](../../../src/midi/ports.rs) (conditional zero audio buses for VST3), either a clack upstream patch or new `src/vst3_extension.rs` for raw FFI.
- 24b: upstream PR against `free-audio/clap-wrapper`, specifically `src/detail/vst3/process.cpp::ProcessAdapter::enqueueOutputEvent`.

---

#### Feature 28: Notes-with-duration representation in the audio thread

**Problem:** Today each `CompactNote` gets expanded at queue time into two `TimedFugueEvent`s — a `NoteOn` at `beat` and a `NoteOff` at `beat + duration`. The audio thread iterates them as independent, pre-sorted points. That independence is the source of a recurring bug family: same-sample NoteOff/NoteOn collisions at loop boundaries, same-pitch overlap needing post-hoc truncation, zero-duration notes inverting under the NoteOff left-skew, and ad-hoc deconflict passes over the output buffer.

All of these are derivative facts of "two independent events for one note." A note is logically a single thing — a pitch, scheduled at beat X, that rings for D ticks. The audio thread only needs that fact to emit correctly-ordered MIDI; pre-expanding into paired events loses information.

**Solution:** ship `TimedNote { tick_offset: i64, duration_ticks: i64, channel: u8, note: u8, velocity: u8 }` to the audio thread as a first-class scheduler primitive, alongside (not replacing) the existing CC / per-note expression event stream. The audio thread maintains a fixed-capacity ring of "notes currently playing" that it consults each buffer: any note whose scheduled NoteOn falls inside the buffer emits NoteOn; any currently-playing note whose `on_sample + duration_samples - 1` falls inside the buffer emits NoteOff at that exact sample.

Boundary properties fall out for free:
- **Same-pitch retrigger:** new NoteOn for a pitch that's already playing → audio thread emits NoteOff at `new_on_sample - 1` before the NoteOn. No `emit_notes` truncation pass needed.
- **Loop wrap + beat-0 retrigger:** NoteOff schedules at `loop_end_sample - 1` because duration expires there; next iteration's NoteOn starts at `loop_end_sample`. One-sample separation baked in.
- **Zero-duration notes:** audio thread skips notes with `duration_ticks <= 0`. Single guard replaces the current emit-time drop + audio-thread left-skew interaction.
- **Deconflict post-pass:** gone. The audio thread never emits a same-sample OFF/ON pair for the same pitch because it owns both scheduling decisions.

**Realtime design requirement:** no `Vec::push` on the audio thread. Use a fixed-capacity `[Option<ActiveNote>; N]` (N = polyphony cap, 128 is comfortable for music; 32 is tight but fine for demo content). New NoteOns find a free slot; if full, drop the oldest or the lowest-velocity voice (voice-stealing policy, TBD). Design explicitly addresses this before implementation — the malloc-on-audio-thread pitfall is the main reason this is Phase 02 rather than being folded into the pre-demo work.

**Subsumes:**
- `emit_notes` same-pitch overlap truncation (the current safety pass at compact-schema → definition).
- `emit_notes` zero-duration drop.
- `Fugue::process_buffer` NoteOff left-skew via `sample_offset.saturating_sub(1)`.
- `FugueSequencer::deconflict_same_sample_note_retriggers` post-pass.
- Loop-boundary NoteOff-at-`local_end`-is-dropped handling inside `process_buffer`.

Pairs with Feature 27 (integer ticks) cleanly — the `ActiveNote` state is keyed by ticks, and the per-buffer "does this note end here?" check is a single integer comparison.

**Non-goals:** CC and per-note expression stay as instant points — they're intrinsically point-in-time, not note-scoped. The audio thread sees a mixed queue: time-ordered `Note { tick, dur }` + `Cc { tick, value }` + `PerNoteExpr { tick, value }`.

**Files:** [src/fugue/types.rs](../../../src/fugue/types.rs) (new `TimedNote` variant or type; event enum gains a "Note" arm alongside the existing one-shot variants), [src/fugue/fugue.rs](../../../src/fugue/fugue.rs) (active-note ring, scheduling logic), [src/fugue/sequencer.rs](../../../src/fugue/sequencer.rs) (delete the deconflict post-pass, delete NoteOff left-skew), [src/mcp/types/emit.rs](../../../src/mcp/types/emit.rs) (delete overlap truncation + zero-dur drop — move to audio-thread scheduler), [src/mcp/types/conversion.rs](../../../src/mcp/types/conversion.rs) (emit `TimedNote`s instead of NoteOn/NoteOff pairs).

---

#### Feature 29: Fold the parked `audio_debug` probe back in as a Cargo feature

**Problem:** The audio-thread debug probe built for tracing the phase_lock / wrap-drift bugs is parked on bookmark `wip/audio-debug-probe`. It needs a clean landing so future sequencer debugging sessions don't rebuild it from scratch, but landing on trunk in its current shape (inline `DebugRecord` construction sprinkled through `FugueSequencer::process` and `Fugue::process_buffer`) pollutes the hot path and adds API noise (seq parameters threaded through internal methods).

**Solution:** gate the probe behind a Cargo feature `audio-debug`. When the feature is off, every call site compiles out entirely — zero cost in release builds, no API shape changes on `Fugue` / `FugueSequencer`. When on, a lock-free `mpsc::sync_channel`-backed drainer writes one record per event to `~/droplets_audio.log` via a background thread (not the audio thread).

Call-site design rule: **one feature-gated line per probe point.** All `DebugRecord` construction lives inside `audio_debug.rs` helper functions that take raw values (references to `Fugue`, `FugueEvent`, primitives). Sequence correlation (which records came from the same `process()` call) lives in a thread-local inside `audio_debug.rs`, set by `on_process_start` and read by subsequent helpers — call sites never pass a `seq` parameter.

Landing checklist:
- [ ] Add `audio-debug = []` to [Cargo.toml](../../../Cargo.toml) `[features]`.
- [ ] Feature-gate `pub mod audio_debug;` in [src/fugue/mod.rs](../../../src/fugue/mod.rs).
- [ ] Feature-gate `fugue::audio_debug::init_if_enabled()` in [src/lib.rs](../../../src/lib.rs).
- [ ] Replace each inline `audio_debug::log(DebugRecord::…)` in `sequencer.rs` / `fugue.rs` with a single-line call to a helper in `audio_debug.rs` (e.g. `audio_debug::on_phase_lock_fugue(fugue, transport_beat)`), gated by `#[cfg(feature = "audio-debug")]`.
- [ ] Delete `seq` parameters from `phase_lock_all`, `start_pending_fugues`, `process_active_fugues`, `Fugue::process_buffer` — `audio_debug.rs` owns the counter via thread-local.
- [ ] Default log path stays `~/droplets_audio.log` (home-relative for DAW sandbox compat).

**Why post-demo, not now:** not bug-fixing — refactor. The probe works as-is on its bookmark; reviving it means `jj new wip/audio-debug-probe` and building from there. Landing cleanly on trunk is worthwhile but not demo-blocking.

**Files:** [Cargo.toml](../../../Cargo.toml), [src/fugue/mod.rs](../../../src/fugue/mod.rs), [src/fugue/audio_debug.rs](../../../src/fugue/audio_debug.rs) (exists on bookmark), [src/fugue/sequencer.rs](../../../src/fugue/sequencer.rs), [src/fugue/fugue.rs](../../../src/fugue/fugue.rs), [src/lib.rs](../../../src/lib.rs).

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
