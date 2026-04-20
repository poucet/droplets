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

**Status (2026-04-20 — code shipped, demo-machine walkthrough pending):** 14b.1–14b.9 done. Source at [extensions/bitwig/](../../../extensions/bitwig/) — ~380 LOC Kotlin, Kotlin stdlib bundled, no Gradle/Maven. `./install.sh` builds + copies into `~/Documents/Bitwig Studio/Extensions/`.

| Done | # | Task | Notes |
|------|---|------|-------|
| [x] | 14b.1 | Scaffold `extensions/bitwig/` directory | [build.sh](../../../extensions/bitwig/build.sh) runs `kotlinc -include-runtime` → jar, then `jar uf` for `META-INF/services`. [install.sh](../../../extensions/bitwig/install.sh) builds + copies to Bitwig's extensions dir. Auto-copies `bitwig.jar` out of the Bitwig app on first run. Pins `JAVA_HOME` to Homebrew's openjdk (macOS ships no JDK). `.gitignore` excludes `build/` + the 33 MB API jar. |
| [x] | 14b.2 | Extension entry + controller definition | [DropletsExtensionDefinition.kt](../../../extensions/bitwig/src/main/kotlin/com/simply/droplets/DropletsExtensionDefinition.kt) (API 18, stable UUID, 0 MIDI ports) + [DropletsExtension.kt](../../../extensions/bitwig/src/main/kotlin/com/simply/droplets/DropletsExtension.kt). Service registered via [META-INF/services/com.bitwig.extension.ExtensionDefinition](../../../extensions/bitwig/src/main/resources/META-INF/services/com.bitwig.extension.ExtensionDefinition). |
| [x] | 14b.3 | Track + device enumeration | `TrackBank(32, 0, 0)`, `track.createDeviceBank(16)`. No VST3 UID matcher — Droplets detection piggybacks on the direct-parameter display observer (see 14b.4), which works identically for CLAP and VST3 without hard-coding a wrapper-derived UID. |
| [x] | 14b.4 | Read instance ID from Droplets params | `device.addDirectParameterValueDisplayObserver(32, BiConsumer)` caches `(paramId, displayValue)` per device; `findInstanceId` scans for any value matching `^droplets-[0-9a-f]+$`. `setObservedParameterIds` turned out unnecessary — the read-only display string only emits once per load. |
| [x] | 14b.5 | Drum Machine pad enumeration | `device.hasDrumPads()` → `device.createDrumPadBank(128)` → per pad: `name()`, `createDeviceBank(8)`. Pad MIDI note = `36 + padIndex` (assumes C1 root, documented as a v1 limitation). `addNoteObserver` dropped (its signature is note-on/off events, not root-note observation). `sampleName()` deferred — needs `createSpecificBitwigDevice(SamplerUUID)`; `preset_name` currently carries the sample name for drag-and-drop samples. |
| [x] | 14b.6 | Device chain walk + preset names | Per device: `name()`, `presetName()`, `isPlugin()`, `deviceType()` ("instrument" / "audio_effect" / "note_effect"). Recurses into `DrumPad.createDeviceBank`. Container (Chain Selector, Instrument Layer) walk deferred — emits as `unknown` for v1. |
| [x] | 14b.7 | Auto-rename Droplets instance | `HashSet<String>` of already-renamed IDs; first sight POSTs `{ instance, name }` to `/rename_instance`. |
| [x] | 14b.8 | Build ProjectLayout JSON + POST | 150ms-debounced rebuild via `host.scheduleTask` coalesces observer bursts at load. Hand-written [Json.kt](../../../extensions/bitwig/src/main/kotlin/com/simply/droplets/Json.kt) (no deps). POST via `java.net.http.HttpClient.sendAsync` — I/O never hits the controller thread. |
| [x] | 14b.9 | WebSocket client for `/ws/controller` | [DropletsClient.kt](../../../extensions/bitwig/src/main/kotlin/com/simply/droplets/DropletsClient.kt): `HttpClient.newWebSocketBuilder().buildAsync(...)` on init. Exponential backoff 1s → 30s cap on connect fail / close / error. v1 logs received text frames. |
| [ ] | 14b.10 | Bitwig-demo walkthrough | Steps captured in [extensions/bitwig/README.md](../../../extensions/bitwig/README.md); needs execution on the demo machine. |

### Phase 14c — Ableton (follow-up, not demo-critical)

Remote Script (Python) or M4L device — decide based on Ableton version of demo machine. Remote Scripts have no build step; M4L is drag-and-drop. Same MCP surface, same `/project_layout` wire format, so zero Rust-side changes.

| Done | # | Task |
|------|---|------|
| [ ] | 14c.1 | Decide Remote Script vs M4L based on install footprint + LOM coverage |
| [ ] | 14c.2 | Port the Bitwig walk to the chosen path |
| [ ] | 14c.3 | Verify Drum Rack introspection (pad names, notes, Simpler sample paths) |

---

## Feature 15: Native drag-out of fugues → DAW clip (`.mid`)

See [ROADMAP.md §Feature 15](ROADMAP.md#feature-15-native-drag-out-of-fugues--daw-clip-mid) for the problem statement. Below is the implementation breakdown.

### Phase 15a — Exporter: merge active fugues

| Done | # | Task | Notes |
|------|---|------|-------|
| [ ] | 15a.1 | Extend [src/fugue/export.rs](../../../src/fugue/export.rs) with `fugues_to_smf(&[FugueDefinition], tempo, duration_beats)` | Single-fugue `fugue_to_smf` stays as a thin wrapper that forwards a one-element slice. Merge events from all inputs into one sorted timeline; each fugue maps to its own MIDI track (SMF format 1) so per-part editing in the DAW is clean. |
| [ ] | 15a.2 | Choose MIDI-file format: one track per fugue-tag vs one-merged-track | Decision: one track per tag. Tagged-fugue groups map to tracks; untagged fugues share a "misc" track. Reason: user opens the file in Bitwig's piano roll and sees "bass / lead / pad" as separate lanes instead of one confusing blob. |
| [ ] | 15a.3 | Loop-aware duration | When all inputs share the same loop length, export one cycle. When durations differ, export LCM of durations (capped at 32 bars) so each part ends on a musical boundary. Write a TimeSignature meta-event from `transport.time_sig_numerator`. |
| [ ] | 15a.4 | Tests | Fixtures covering: single fugue, multi-fugue with different tags, LCM duration, mixed interpolation curves round-tripping through the exporter. |

### Phase 15b — `drag` crate + IPC wiring

| Done | # | Task | Notes |
|------|---|------|-------|
| [ ] | 15b.1 | Add `drag = "2.1"` to Cargo.toml | Cross-platform wrapper around `NSFilePromiseProvider` / `IDropSource` / XDND. Small crate (~900 LOC, no heavyweight deps). |
| [ ] | 15b.2 | New wry IPC handler `start_drag` in [src/gui/webview.rs](../../../src/gui/webview.rs) | Receives `{ instance, fugue_ids?: string[], active?: bool }`. `fugue_ids` = specific fugues; `active` = "all currently playing." Writes `.mid` to `std::env::temp_dir()/droplets-<timestamp>.mid`. |
| [ ] | 15b.3 | Start native drag from the wry window handle | `drag::start_drag(window_handle, DragItem::Files(vec![path]), Image::Raw(...), on_result, options)`. Place-holder image (simple MIDI icon) acceptable for v1. |
| [ ] | 15b.4 | Temp file cleanup | Spawn a thread on drag-result callback to delete the file after 60s — gives the DAW time to copy/index. Alternative: use `tempfile::NamedTempFile` with manual drop-delay. |
| [ ] | 15b.5 | Fallback when drag isn't supported | If `drag::start_drag` errors (rare — mostly Linux distro mismatches), fall back to `open::that(&path)` to reveal in the file manager. Return the path to the frontend so it can show a toast with it. |

### Phase 15c — Drag zones in UI

| Done | # | Task | Notes |
|------|---|------|-------|
| [ ] | 15c.1 | Drag handle per-fugue in [FugueViewer.tsx](../../../frontend/src/components/FugueViewer.tsx) | Small 🎵-icon button on each fugue row. `onMouseDown` → IPC `start_drag` with `{ instance, fugue_ids: [id] }`. Crucially: use `mousedown` not `click` — native drag must start during the initial mouse-down gesture on Windows/macOS. |
| [ ] | 15c.2 | "Drag all active" affordance in [FugueList.tsx](../../../frontend/src/components/FugueList.tsx) | Single handle at the list header. IPC `start_drag` with `{ instance, active: true }`. |
| [ ] | 15c.3 | Visual feedback | Drag handle highlights while the user holds the button. Show a count badge (🎵 × N) on the "all active" handle so the user knows how many fugues will bundle. |
| [ ] | 15c.4 | Platform caveats doc | Short note in [FUGUE_UI.md](../../FUGUE_UI.md) about macOS Gatekeeper: first-time drag may require a quarantine-bypass dialog for the temp file. Users drop onto a Bitwig clip and it works. |

### Phase 15d — HTTP endpoints for LLM / scripted use

| Done | # | Task | Notes |
|------|---|------|-------|
| [ ] | 15d.1 | `GET /api/export/fugue/:id` | Serves the `.mid` for one fugue. Query param `tempo` overrides the session tempo. Response `Content-Type: audio/midi`. Useful as a test harness for 15a and for MCP-driven export flows. |
| [ ] | 15d.2 | `GET /api/export/active?instance=` | Serves a `.mid` bundling all currently-active fugues on the instance. Same format as the drag-out path (one track per tag). |
| [ ] | 15d.3 | New MCP tool `export_fugues(instance, fugue_ids?)` | Returns `{ path: "/path/to/file.mid", size_bytes }`. The file is written to the user-configured export dir (not the temp dir the drag-out uses). Useful when the LLM wants to "save this pattern somewhere I can find later." |

### Phase 15e — Caveats + docs

| Done | # | Task | Notes |
|------|---|------|-------|
| [ ] | 15e.1 | Document automation limitations | MIDI export carries notes + CC only. Slot param automation doesn't have a portable MIDI representation. Guidance in [FUGUE.md](../../FUGUE.md): to capture slot automation in the DAW, arm automation lanes and replay the fugue. |
| [ ] | 15e.2 | Update [instructions.md](../../../src/mcp/instructions.md) | Tell the LLM that `export_fugues` exists for hand-off workflows. |

---

## Feature 16: Native drag-in of `.mid` → fugue on an instance

See [ROADMAP.md §Feature 16](ROADMAP.md#feature-16-native-drag-in-of-mid--fugue-on-an-instance). Implements the return leg of the round-trip.

### Phase 16a — MIDI parser → FugueDefinition

| Done | # | Task | Notes |
|------|---|------|-------|
| [ ] | 16a.1 | New [src/fugue/import.rs](../../../src/fugue/import.rs) with `smf_to_fugues(&[u8], opts) -> Result<Vec<FugueDefinition>, String>` | Opts: default channel, loop_mode, quantize, tag-prefix. Parse via `midly::Smf::parse` (already a dep). Handle both SMF format 0 and format 1. |
| [ ] | 16a.2 | Note on/off pairing | Track active (channel, note) pairs across a single parse; emit a note event with `duration = off_tick - on_tick` scaled to beats via the file's PPQ. A dangling note-on at EOF closes at the last parsed tick. |
| [ ] | 16a.3 | CC events passthrough | Straight translation: `MidiMessage::Controller { cc, value }` → `FugueEvent::Cc { channel, cc, value, curve: None }`. Fugue-level `cc_interpolation: Linear` so ramps rebuild smoothly. |
| [ ] | 16a.4 | Drop per-note expression | `PitchBend` and `Aftertouch` ignored — MIDI 1.0 channel events don't carry the per-note target. Document the lossy conversion (16a.1 opts can include `strict: bool` that errors on unsupported events instead). |
| [ ] | 16a.5 | Multi-track handling | SMF format 1 → one `FugueDefinition` per track, each gets the track name as its tag (falls back to `imported-N`). Format 0 → single fugue. |
| [ ] | 16a.6 | Tests | Round-trip: a fugue exported via 15a re-imports to an equivalent event stream (modulo per-note expression). Fuzz: malformed SMF produces a clean `Err`, never panics. |

### Phase 16b — HTTP endpoint + drop zone

| Done | # | Task | Notes |
|------|---|------|-------|
| [ ] | 16b.1 | `POST /api/import/fugue?instance=&tag_prefix=&loop_mode=&quantize=` | Body: `Content-Type: audio/midi` raw bytes. Parse via 16a, queue each resulting fugue via `FugueBridge::queue`. Respond `{ ok, fugue_ids: [..] }`. |
| [ ] | 16b.2 | Drop zone in [App.tsx](../../../frontend/src/App.tsx) instance header | Accepts `.mid` drag events. `onDrop` reads the file as ArrayBuffer and POSTs. Visual feedback: the header highlights while a file is dragged over. |
| [ ] | 16b.3 | MCP tool `import_fugue(instance, base64_mid, options?)` | Accepts base64-encoded MIDI. Decodes, queues through the same path as 16b.1. Returns `fugue_ids`. |
| [ ] | 16b.4 | Handle drop onto a specific instance row | If the user's project has multiple Droplets instances, dropping on `instance "bass"` queues to that instance specifically (not `selectedInstance`). |

---

## Feature 17: MCP tool `get_fugue(id)` — read back a FugueDefinition

See [ROADMAP.md §Feature 17](ROADMAP.md#feature-17-mcp-tool-get_fugueid--read-back-a-fuguedefinition). Closes the loop: LLM reads user-edited fugues.

| Done | # | Task | Notes |
|------|---|------|-------|
| [ ] | 17.1 | `FugueBridge::get_definition(instance, id) -> Option<FugueDefinition>` | Thin lookup beside existing `get_definitions` (plural). ~15 min. |
| [ ] | 17.2 | New MCP tool `get_fugue` | `#[tool]` fn in [src/mcp/server.rs](../../../src/mcp/server.rs). Takes `{ instance, fugue_id }`, returns the raw `FugueDefinition` serialized as JSON. |
| [ ] | 17.3 | Compact-schema response (stretch) | Group NoteOn/NoteOff pairs back into `{beat, note, duration, velocity?, channel?}` tuples, collapse CC events into `{cc, points: [[beat, value]]}` lanes. Returns the same `FugueContent::Composite` shape the LLM accepts on input — symmetric read/modify/write. ~2 hours; not blocking 17.2. |
| [ ] | 17.4 | Docs: read-modify-write pattern | Paragraph in [instructions.md](../../../src/mcp/instructions.md): "To evolve a user-edited pattern, call `get_fugue`, mutate, then re-queue with the same `tag + cancel_mode: 'tag:…'`." |

---

## Resolved design decisions

- **Instance ID param encoding:** went with `ParamInfoFlags::IS_READONLY` + constant numeric value `0.0` + `value_to_text` returning the `instance_id` string. Host extensions read via `addDirectParameterValueDisplayObserver`. Numeric value is meaningless but that's fine — only the displayed string matters for this correlation.
- **Degradation messaging:** `get_project_state` returns `layout_available: bool` alongside the instance list. `false` = no host extension; the LLM falls back. Instructions.md steers the fallback explicitly.
- **`device_path` addressing:** note-based pad segment (`pad:36` = MIDI C2). Survives pad reorderings because it's addressed by trigger note rather than position. `parse_device_path` rejects out-of-range notes.
- **Storage primitive:** used `OnceLock<RwLock<Option<ProjectLayout>>>` to match the existing bridge pattern rather than pulling in `ArcSwap` for a single struct. Contention is negligible (writer runs only when the extension re-POSTs on change).
