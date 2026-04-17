# Droplets 0.1 Roadmap

## Summary

Droplets 0.1 is the **MCP-music demo release** — the version that goes on stage on **2026-04-21** to show an LLM composing multi-track music live through a DAW plugin over MCP.

The backend and UI are ~95% compliant with [FUGUE.md](../../FUGUE.md) and [FUGUE_UI.md](../../FUGUE_UI.md). Remaining work is about **polishing the LLM-facing surface** (prompt, tool descriptions, docs), **validating the multi-instance path** under demo conditions, and adding one missing primitive (`get_transport`). Per-note expressivity inside fugues is the stretch goal that would differentiate this demo from "LLM spams notes." A native UI rewrite (egui) is explicitly post-demo — the motivation is eliminating transport-to-render skew, not aesthetics.

---

## Feature Overview

### Phase 01: Demo Prep (must ship by 2026-04-21)

| Done | Pri | # | Feature | Complexity | Impact |
|------|-----|---|---------|------------|--------|
| [🔄] | P0 | 1 | Per-note expressivity inside fugues | M | High — reshapes schema; must land before docs/prompt |
| [ ] | P0 | 2 | Rewrite `ServerInfo::instructions` system prompt | S | Very High — shapes every LLM call |
| [ ] | P0 | 3 | Add a worked example to `queue_fugue` tool description | S | Very High — LLMs imitate examples |
| [ ] | P0 | 4 | Rewrite FUGUE.md to match the shipping compact schema | S | High — live demo reference |
| [ ] | P1 | 5 | Add `get_transport` MCP tool | S | Medium-High — lets LLM reason about timing |
| [ ] | P0 | 6 | Multi-instance end-to-end validation | M | Critical — central demo claim |
| [ ] | P0 | 7 | macOS UI verification on demo machine | S | Critical — risk mitigation |

### Phase 02: Post-Demo Polish

| Done | Pri | # | Feature | Complexity | Impact |
|------|-----|---|---------|------------|--------|
| [ ] | P2 | 8 | Migrate UI from React/HTTP to egui (in-process) | XL | Correctness — removes transport skew |

---

## Phase Details

### Phase 01: Demo Prep

Everything here is about the **LLM's view of the system** and **demo-day reliability**. One new musical primitive (per-note expressivity in fugues) lands first because it reshapes the schema that the system prompt, worked example, and FUGUE.md all describe — sequencing it first means writing those once, not three times.

#### Feature 1: Per-note expressivity inside fugues

**Problem:** `FugueContent` has only `Notes` and `Cc` variants. Per-note pitch bend and per-note pressure exist as immediate `send_per_note_*` MCP tools but cannot be scheduled on a beat offset — which defeats the whole "pre-schedule because LLMs are slow" premise for any expressive/MPE-flavored demo moment.

**Solution:** Add two `FugueContent` variants: `PerNotePitchBend { note, points: [[beat, semitones]] }` and `PerNotePressure { note, points: [[beat, value_0_1]] }`. The fugue scheduler already handles per-note events via the one-shot dispatch path, so the audio-thread side is mostly reuse. This must land **before** features 2-4 so the system prompt, worked example, and FUGUE.md can describe the final schema in one pass.

**Files:** [src/mcp/server.rs](../../../src/mcp/server.rs) (FugueContent enum + parsing), [src/fugue/types.rs](../../../src/fugue/types.rs) (if event types need expansion), [src/fugue/](../../../src/fugue/) (scheduler dispatch).

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

### Phase 02: Post-Demo Polish

#### Feature 8: Migrate UI to egui (in-process)

**Problem:** The transport-to-render pipeline has visible skew. Chain: audio thread → `ArcSwap<TransportState>` → GUI HTTP server → JSON → WebSocket (500ms) → browser → React → SVG. `TimingManager.ts` papers over the gap with client-side wall-clock interpolation, but drifts under JS event-loop jank or tempo changes between syncs. This is structural, not tuneable.

**Solution:** Replace the React frontend with an in-process egui UI. Collapses the chain to: audio thread → `ArcSwap` → UI thread → paint. Removes JSON serialization, WebSocket polling, and interpolation guesswork. Residual latency is ~one audio block (few ms). Current UI already does direct DOM manipulation (`progressRefs`, SVG playhead) to dodge React re-renders — that pattern maps cleanly onto immediate-mode GUI, so the port is structurally natural.

**Files:** New `src/ui/` module, wire into plugin window. Deprecate [frontend/](../../../frontend/) and [src/gui/](../../../src/gui/).

---

## Implementation Order

```
Phase 01:
  1 (per-note in fugues)                       ← reshapes schema, do first
  ↓
  2 (system prompt) + 3 (queue_fugue example)  ← describe full vocabulary in one pass
  ↓
  4 (FUGUE.md rewrite)                         ← mirrors the new prompt
  ↓
  5 (get_transport)
  ↓
  6 (multi-instance validation) + 7 (UI verify) ← both on demo machine
  ↓
  2026-04-21: DEMO
  ↓
Phase 02: 8 (egui migration)
```

---

## Key Files

| Phase | Files |
|-------|-------|
| 01 | [src/mcp/server.rs](../../../src/mcp/server.rs), [src/fugue/types.rs](../../../src/fugue/types.rs), [src/fugue/](../../../src/fugue/), [src/fugue/bridge.rs](../../../src/fugue/bridge.rs), [docs/FUGUE.md](../../FUGUE.md), [INSTALL.md](../../../INSTALL.md) |
| 02 | New `src/ui/`, retire [frontend/](../../../frontend/) and [src/gui/](../../../src/gui/) |
