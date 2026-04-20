# Droplets 0.1 — TASKS

Active task breakdown for work in flight. See [ROADMAP.md](ROADMAP.md) for the full feature list.

---

## Feature 14: DAW track context

Split into two phases: the **Rust side** is DAW-agnostic and unblocks everything else. The **Bitwig extension** follows, then Ableton (M4L or Remote Script) as a follow-up.

### Phase 14a — Rust side (DAW-agnostic)

**Status (2026-04-20 — shipped):** all 11 tasks complete. 145 tests pass.

| Done | # | Task | Notes |
|------|---|------|-------|
| [x] | 14a.1 | Expose instance ID as a read-only plugin parameter | New [src/instance_param.rs](../../../src/instance_param.rs). `ClapId(1)`, name `"Instance"`, flag `IS_READONLY`. `value_to_text` renders the current `DropletShared::instance_id` — host extensions read it via `addDirectParameterValueDisplayObserver`. `PluginParams` extension registered in [src/lib.rs:62-67](../../../src/lib.rs#L62-L67). |
| [x] | 14a.2 | Define `ProjectLayout` types | New [src/mcp/project.rs](../../../src/mcp/project.rs). Recursive `Device` enum (Instrument / Effect / DrumMachine / Container / Unknown), `DrumPad { note: u8, name, devices }`, `Chain`, `ParameterInfo`. Drum pad notes are MIDI numbers on the wire; all outgoing views render via `midi_to_name`. Full serde + ts-rs exports generated. |
| [x] | 14a.3 | Tiered serialization views | Three wrapper structs over the same internal type: `ProjectState` (tier 1), `TrackInfo` + `DeviceTier2` + `DrumPadTier2` + `ChainTier2` (tier 2), `DeviceParameters` (tier 3). Each constructed via a `::build(layout, ...)` associated fn. Tier 2 types don't even carry a `parameters` field — compile-time guarantee that params don't leak at tier 2. |
| [x] | 14a.4 | `ProjectLayout` storage in bridge | Added `static PROJECT_LAYOUT: OnceLock<RwLock<Option<ProjectLayout>>>` in [src/mcp/bridge.rs](../../../src/mcp/bridge.rs) beside the existing `REGISTRY`. `CcBridge::set_project_layout` / `get_project_layout` wrap it. Chose `RwLock<Option<...>>` over `ArcSwap` to match the existing bridge pattern. |
| [x] | 14a.5 | HTTP `POST /project_layout` endpoint | Wired into the MCP server's axum router in [src/mcp/mod.rs](../../../src/mcp/mod.rs). Body is JSON `ProjectLayout`. Returns 200 on success, 400 with parse error on malformed JSON. |
| [x] | 14a.6 | MCP tool: `get_project_state` | Ships tier 1 with `{ instances, other_tracks, layout_available }`. `layout_available: false` when the host extension hasn't pushed yet — LLM falls back to GM/ask-the-user. `primary_device` is the first Instrument or DrumMachine on the chain; effects-only chains serialize as `None`. Drum machine primaries include pad summaries with pitch-notation notes + sample names. |
| [x] | 14a.7 | MCP tool: `get_track_info` | Tier 2. Takes `{ instance }`, resolves name→ID via `CcBridge::resolve_instance_id`, looks up the track in the layout, serializes via `DeviceTier2::from(&Device)`. Returns a helpful "no track info available" string when the instance isn't on any known track. |
| [x] | 14a.8 | MCP tool: `get_device_parameters` | Tier 3 fully implemented (not stubbed). `device_path` parsed via `parse_device_path` (accepts `device:N`, `pad:N_MIDI`, `chain:N`), resolved via `resolve_path`. Returns `{ device_name, parameters }`. Ready for when the extension starts populating `parameters` arrays. |
| [x] | 14a.9 | WebSocket `/ws/controller` endpoint | `tokio::sync::broadcast::Sender<ControllerCommand>` in a `OnceLock` (capacity 64, drop-oldest via broadcast's Lagged semantics). Handler subscribes, forwards commands as JSON text frames, drains inbound frames. `ControllerCommand::Noop` is the only variant for now — enough to exercise the pipe. `pub fn send_controller_command` is the push API for future MCP tools. |
| [x] | 14a.10 | Update [instructions.md](../../../src/mcp/instructions.md) | Replaced the "multi-instance setup" section with "session start" that leads with `get_project_state`. New "Using drum maps" section tells the LLM to use the returned pad notes rather than GM conventions. Explicit fallback when `layout_available: false`. |
| [x] | 14a.11 | Unit tests for `ProjectLayout` | 11 tests in [src/mcp/project.rs](../../../src/mcp/project.rs) covering: tier 1 drum-map pitch notation, tier 1 instrument-track summary, tier 1 other-tracks, tier 1 without layout, tier 2 parameters-stripped, tier 2 pad note naming, tier 2 missing instance, path parsing (valid/invalid), tier 3 resolution through pads, tier 3 error paths, round-trip serde. |

### Phase 14b — Bitwig extension

Built in parallel by a second agent. Source is at [extensions/bitwig/](../../../extensions/bitwig/) (uncommitted at time of writing). Track progress + final verification in this section.

| Done | # | Task | Notes |
|------|---|------|-------|
| [ ] | 14b.1 | Scaffold `extensions/bitwig/` directory | Kotlin + `kotlinc`/`jar` shell script, no Gradle. Sibling of `src/`. |
| [ ] | 14b.2 | Extension entry + controller definition | `DropletsExtension` + `DropletsExtensionDefinition`, service manifest registered. |
| [ ] | 14b.3 | Track + device enumeration | `TrackBank` with reasonable size, per-track `DeviceBank`, VST3 matcher for Droplets detection. |
| [ ] | 14b.4 | Read instance ID from Droplets params | Direct-parameter ID observer + display-value observer on the `Instance` param. |
| [ ] | 14b.5 | Drum Machine pad enumeration | `createDrumPadBank` + per-pad `name()` / `addNoteObserver` / nested `DeviceBank` / `sampleName()`. |
| [ ] | 14b.6 | Device chain walk + preset names | Per device: name, preset, vendor, recurse through containers. Params deferred for tier 3. |
| [ ] | 14b.7 | Auto-rename Droplets instance | POST `/rename_instance` on first sight, so the LLM sees track-matched names without user intervention. |
| [ ] | 14b.8 | Build ProjectLayout JSON + POST | Schedule HTTP off the observer thread; re-POST on relevant changes. |
| [ ] | 14b.9 | WebSocket client for `/ws/controller` | Connect + auto-reconnect. v1 has nothing to handle beyond `Noop`. |
| [ ] | 14b.10 | Bitwig-demo walkthrough | End-to-end: load on 2 tracks, verify primary devices + pad names via `get_project_state`, verify auto-rename, demo drum pattern uses correct notes. |

### Phase 14c — Ableton (follow-up, not demo-critical)

Remote Script (Python) or M4L device — decide based on Ableton version of demo machine. Remote Scripts have no build step; M4L is drag-and-drop. Same MCP surface, same `/project_layout` wire format, so zero Rust-side changes.

| Done | # | Task |
|------|---|------|
| [ ] | 14c.1 | Decide Remote Script vs M4L based on install footprint + LOM coverage |
| [ ] | 14c.2 | Port the Bitwig walk to the chosen path |
| [ ] | 14c.3 | Verify Drum Rack introspection (pad names, notes, Simpler sample paths) |

---

## Resolved design decisions

- **Instance ID param encoding:** went with `ParamInfoFlags::IS_READONLY` + constant numeric value `0.0` + `value_to_text` returning the `instance_id` string. Host extensions read via `addDirectParameterValueDisplayObserver`. Numeric value is meaningless but that's fine — only the displayed string matters for this correlation.
- **Degradation messaging:** `get_project_state` returns `layout_available: bool` alongside the instance list. `false` = no host extension; the LLM falls back. Instructions.md steers the fallback explicitly.
- **`device_path` addressing:** note-based pad segment (`pad:36` = MIDI C2). Survives pad reorderings because it's addressed by trigger note rather than position. `parse_device_path` rejects out-of-range notes.
- **Storage primitive:** used `OnceLock<RwLock<Option<ProjectLayout>>>` to match the existing bridge pattern rather than pulling in `ArcSwap` for a single struct. Contention is negligible (writer runs only when the extension re-POSTs on change).
