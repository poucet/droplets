# Droplets 0.1 — TASKS

Active task breakdown for work in flight. See [ROADMAP.md](ROADMAP.md) for the full feature list.

---

## Feature 14: DAW track context

Split into two phases: the **Rust side** is DAW-agnostic and unblocks everything else. The **Bitwig extension** follows, then Ableton (M4L or Remote Script) as a follow-up.

### Phase 14a — Rust side (DAW-agnostic)

| Done | # | Task | Notes |
|------|---|------|-------|
| [ ] | 14a.1 | Expose instance ID as a read-only plugin parameter | Register `PluginParams` extension in [src/lib.rs](../../../src/lib.rs) (currently intentionally off). Param's displayed-value string = `droplets-a1b2c3d4`. Encode underlying numeric as the u32 instance-ID bits. Host extensions read the displayed string via direct-parameter API. |
| [ ] | 14a.2 | Define `ProjectLayout` types | New module `src/mcp/project.rs`. Recursive `Device` enum with variants `Instrument { name, vendor, preset_name, sample_name, parameters }`, `Effect { name, vendor, preset_name, parameters }`, `DrumMachine { name, pads }`, `Container { name, kind, chains }`, `Unknown { name, vendor }`. `DrumPad { note: u8 (serialized as pitch notation), name, devices: Vec<Device> }`. `ParameterInfo { name, index, displayed_value }`. `#[serde(tag = "type", rename_all = "snake_case")]`. ts-rs export for future TS use. |
| [ ] | 14a.3 | Tiered serialization views | Three views over the same rich internal type: tier 1 (minimal — primary device per track + drum pad note/name/sample_name, no params, no effect chain), tier 2 (full device chain + drum pads + nested chains, no params), tier 3 (params for one addressed device). Implement via wrapper structs or a `SerializationTier` enum that selects field inclusion. |
| [ ] | 14a.4 | `ProjectLayout` storage in bridge | Add `project_layout: Arc<ArcSwap<Option<ProjectLayout>>>` (process-wide, not per-instance — extension pushes the whole project). Module-level in [src/mcp/bridge.rs](../../../src/mcp/bridge.rs). |
| [ ] | 14a.5 | HTTP `POST /project_layout` endpoint | Accepts JSON body, deserializes into `ProjectLayout`, stores into the `ArcSwap`. Returns 200. Reuses existing HTTP server in [src/mcp/server.rs](../../../src/mcp/server.rs). |
| [ ] | 14a.6 | MCP tool: `get_project_state` | Tier 1 view. Returns `{ instances: [{ id, name, track_name, primary_device }], other_tracks: [{ name }] }`. `primary_device` = first `Instrument` or `DrumMachine` in the top-level chain, or `null` for effects-only. Drum machine primary device includes `pads: [{ note: "C2", name, sample_name }]`. |
| [ ] | 14a.7 | MCP tool: `get_track_info` | Tier 2 view. Takes `instance: String`. Returns full recursive device chain for that instance's track. No params. |
| [ ] | 14a.8 | MCP tool: `get_device_parameters` | Tier 3 view. Takes `instance: String, device_path: String` (e.g. `"device:0/pad:36/device:1"`). Returns the param list for that single device. Defer implementation until extension populates `parameters` arrays — stub OK for v1. |
| [ ] | 14a.9 | WebSocket `/ws/controller` endpoint | Internal command queue (bounded, drop-oldest). Extension connects and receives `ControllerCommand`s. v1 emits nothing; endpoint + queue + reconnection handling exist so future MCP tools can enqueue without protocol changes. `enum ControllerCommand { /* variants TBD */ }` with `#[serde(tag = "type")]`. |
| [ ] | 14a.10 | Update [instructions.md](../../../src/mcp/instructions.md) | Add "## DAW context" section. Direct LLM to call `get_project_state` at the start of every session. Explain graceful degradation (empty → DAW without host extension). Clarify tier model: state → track → device. Note: when available, the primary device tells you what kind of sound is on each track; drum maps give correct note-to-sample mapping so you don't have to guess. |
| [ ] | 14a.11 | Unit tests for `ProjectLayout` serialization | Verify tier 1/2/3 views, pitch-notation serialization on pad notes, round-trip through `POST /project_layout`. |

### Phase 14b — Bitwig extension

| Done | # | Task | Notes |
|------|---|------|-------|
| [ ] | 14b.1 | Scaffold `bitwig-extension/` directory | Sibling of `src/`. README with build instructions and install path. `build.sh` calling `kotlinc -cp bitwig-api.jar src/*.kt -d build/` + `jar cf Droplets.bwextension -C build/ .`. Commit Bitwig's API jar or script its extraction from `/Applications/Bitwig Studio.app`. |
| [ ] | 14b.2 | Extension entry + controller definition | `DropletsExtension` class extending `ControllerExtension`. `DropletsExtensionDefinition` with UUID, name, vendor. Register in `META-INF/services/com.bitwig.extension.controller.ControllerExtensionDefinition`. |
| [ ] | 14b.3 | Track + device enumeration | `TrackBank` with reasonable size (32). Per track, `Track.createDeviceBank(16)`. Filter-by-matcher variant using `host.createVST3DeviceMatcher(<droplets-vst3-uid>)` for fast Droplets detection. |
| [ ] | 14b.4 | Read instance ID from Droplets params | `device.addDirectParameterIdObserver` + `setObservedParameterIds([<instance-id-param-id>])` + `addDirectParameterValueDisplayObserver` to read the displayed string. |
| [ ] | 14b.5 | Drum Machine pad enumeration | `device.hasDrumPads()` detection → `device.createDrumPadBank(128)` → per pad: `name()`, `addNoteObserver`, nested `pad.createDeviceBank(8)` for chain, `sampleName()` on the nested Sampler. |
| [ ] | 14b.6 | Device chain walk + preset names | Per device: `name()`, `presetName()`, `vendor()` where available, `isPlugin()`. Recurse into `DrumPad.createDeviceBank`. Parameter reading deferred (tier 3). |
| [ ] | 14b.7 | Auto-rename Droplets instance | On first sight of a Droplets device on track X, POST `/rename_instance` to the plugin with `{ instance: <instance_id>, name: <track_name> }`. Use existing MCP rename route. |
| [ ] | 14b.8 | Build ProjectLayout JSON + POST | Schedule work off the observer thread. Use `java.net.http.HttpClient` to POST to `http://127.0.0.1:9999/project_layout`. Re-POST on any relevant observer change (device added/removed, preset/sample swap, pad name change, track name change). |
| [ ] | 14b.9 | WebSocket client for `/ws/controller` | Connect, auto-reconnect on failure. Handle `ControllerCommand` variants as they land (v1: none to handle, just connect + log). |
| [ ] | 14b.10 | Bitwig-demo walkthrough | With extension installed: load Droplets on a drum track + a synth track → verify `get_project_state` shows the right primary devices and pad names → verify auto-rename → verify drum-pattern demo picks correct notes from pad names. |

### Phase 14c — Ableton (follow-up, not demo-critical)

Remote Script (Python) or M4L device — decide based on Ableton version of demo machine. Remote Scripts have no build step; M4L is drag-and-drop. Same MCP surface, same `/project_layout` wire format, so zero Rust-side changes.

| Done | # | Task |
|------|---|------|
| [ ] | 14c.1 | Decide Remote Script vs M4L based on install footprint + LOM coverage |
| [ ] | 14c.2 | Port the Bitwig walk to the chosen path |
| [ ] | 14c.3 | Verify Drum Rack introspection (pad names, notes, Simpler sample paths) |

---

## Open questions

- **Instance ID param encoding:** best way to make the CLAP parameter's display string equal the instance ID? CLAP params are floats; we need the displayed-value mapping to return the ID string regardless of the numeric value. Investigate `clack`'s `Param::display` hooks in [src/params.rs](../../../src/params.rs) when implementing 14a.1.
- **Ableton/Logic degradation messaging:** should `get_project_state` return `{ supported: false, reason: "no host extension running" }` or just an empty payload? Former is more informative for the LLM; latter is simpler. Lean toward former — one extra bool field, much clearer for the model.
- **`device_path` addressing format:** `"device:0/pad:36/device:1"` (using pad's MIDI note) vs `"device:0/pad:3/device:1"` (using pad bank index). Note-based is more stable across reorderings. Decide when implementing 14a.3.
