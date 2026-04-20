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
| [x] | 15a.1 | Extend [src/fugue/export.rs](../../../src/fugue/export.rs) with a multi-fugue exporter | Shipped as a **three-stage decoupled pipeline** rather than a single function: `merge_fugues_by_tag(defs) → Vec<FugueDefinition>` (pure data merge) → `ExportedMidi::from_merged(...)` (named-track build) → `.to_smf()` / `.to_bytes()` (midly serialization). `fugues_to_smf(defs, tempo)` remains as the thin end-to-end wrapper for HTTP handlers. Each fugue carries its own `duration_beats` so the separate `duration_beats` parameter was unnecessary — dropped from the signature. |
| [x] | 15a.2 | One track per tag (SMF format 1) | Implemented in `ExportedMidi::from_merged`. Each tag group → one MIDI track with `TrackName` meta. Untagged fugues each get a unique synthetic name (`fugue-1`, `fugue-2`, …) — unlike the original spec, they do NOT share a "misc" track, because two untagged fugues represent independent user intent. Conductor track (tempo) sits at track 0. |
| [x] | 15a.3 | LCM-of-durations for merged fugues | Shipped in `merge_fugues_by_tag`. When a tag group has members with different durations (e.g. 4-beat bass + 16-bar melody), target duration becomes LCM(durations) and each member's events replicate to fill that span — so the merged definition loops cleanly in any DAW. **Oversized LCM handling:** if LCM > 128-beat cap (relatively prime durations, exotic time signatures), the duration clamps at the cap and every member keeps replicating as many cycles as fit; the last partial cycle's events past the cap are dropped. Quantizes to 960 ticks/beat before integer LCM so triplets (÷3), 16ths (÷4), and swung grids all snap cleanly. TimeSignature meta-event deferred — most DAWs infer tempo/signature from the tempo meta alone. |
| [x] | 15a.4 | Tests | 10 unit tests covering: empty input → valid but empty SMF, one-track-per-tag, TrackName meta matches tags, same-tag merge folds events, untagged fugues get unique tracks, `merge_fugues_by_tag` keeps untagged separate, folds same-tag into one definition, preserves first-seen order, assigns fresh id, `ExportedMidi` round-trips through `to_bytes` + parse. |

### Phase 15b — `drag` crate + IPC wiring

| Done | # | Task | Notes |
|------|---|------|-------|
| [x] | 15b.1 | Add `drag = "2.1"` to Cargo.toml | Done. |
| [x] | 15b.2 | New wry IPC handler `start_drag` in [src/gui/webview.rs](../../../src/gui/webview.rs) | `handle_start_drag` inline in `configure_webview`'s IPC closure. Runs synchronously on the WebView thread (drag must start during the same gesture). Accepts `{type: "start_drag", instance?, fugue_ids?, active?, tempo_bpm?}`. Pulls fugue definitions via `FugueBridge::get_definitions`, exports via `fugues_to_smf` (Phase 15a), writes to `env::temp_dir()/droplets-<timestamp_ms>.mid`. |
| [x] | 15b.3 | Start native drag from the wry window handle | New [src/gui/drag.rs](../../../src/gui/drag.rs) with `DragWindow` (wraps `RawWindowHandle`, impls `HasWindowHandle`) + shared `DragState` slot. [src/gui/plugin.rs](../../../src/gui/plugin.rs) `set_parent` stashes the handle; the IPC handler reads it when a drag is requested. `drag::start_drag` called with empty preview image (DAWs draw their own drag ghost). |
| [x] | 15b.4 | Temp file cleanup | Detached thread sleeps 60s then `remove_file`. Best-effort — a missing file at cleanup time is fine (user may have moved it). |
| [x] | 15b.5 | Fallback when drag isn't supported | `DragStart` enum with variants `Started / Unsupported / NoWindow / Failed`. Unsupported (Linux — drag crate wants a GTK window we don't have inside a CLAP plugin) falls through to `reveal_fallback` which uses `open::that(&path)` to open the file manager. NoWindow / Failed do the same. Standalone binary gets `NoWindow` (no DAW parent handle), so drag requests there reveal-to-file-manager. |

### Phase 15c — Drag zones in UI

| Done | # | Task | Notes |
|------|---|------|-------|
| [x] | 15c.1 | Whole-container drag source (scope widened) | Entire `.fugue-list-item` row and the `.viewer-header` in [FugueViewer.tsx](../../../frontend/src/components/FugueViewer.tsx) are drag sources, not small icon buttons — matches user's UX expectation ("why can't I drag the whole fugue"). Movement-threshold pattern (4px) on mousedown distinguishes click-to-select from press-and-drag: a global mousemove listener fires `startDrag` only once the cursor has actually moved; pure clicks still fire `onSelect`. Uses `mousedown` as the gesture anchor (not `click`) because macOS/Windows need the drag to start during the initial press. |
| [x] | 15c.2 | "Drag all active" affordance in [FugueList.tsx](../../../frontend/src/components/FugueList.tsx) | `⇣ Drag all <N>` button in the list header. IPC `start_drag` with `{ instance, active: true }`. Only rendered when 2+ fugues are live (dragging a single fugue is redundant with the row drag). |
| [x] | 15c.3 | Visual feedback | Cursor changes to `grab` on rows and the drag-all button, `grabbing` while pressed. Drag-all button shows live count. Further visual tweaks (drag-preview image, row highlight while drag is in-flight) deferred — drag-crate's v1 draws no preview; DAWs show their own drag ghost anyway. |
| [x] | 15c.4 | Platform caveats doc | New "Drag-out / drag-in" + "Platform caveats" sections in [FUGUE_UI.md](../../FUGUE_UI.md) covering macOS Gatekeeper first-drag prompt, Linux fallback (reveal-in-file-manager), and Bitwig launcher-clip limitation (Bitwig won't emit `.mid` from clip-launcher clips — must drag through arranger timeline first, tracked as Feature 23 for native `.bwclip` support). |

### ~~Phase 15d — HTTP endpoints for LLM / scripted use~~ (dropped)

Dropped — overlap with existing affordances made this mostly redundant:

- The `↓` button in FugueList already saves individual fugues to the user's export directory.
- Drag-out covers "all active fugues as one `.mid`" from the UI.
- Feature 17's `get_fugue` already gives the LLM read-back; an "export to disk" MCP tool would be a trivial follow-up if the LLM workflow actually demands it (not speculative demand).

### Phase 15e — Caveats + docs

| Done | # | Task | Notes |
|------|---|------|-------|
| [x] | 15e.2 | Update [instructions.md](../../../src/mcp/instructions.md) | Added "Drag round-trip with the DAW" section covering both drag-out and drag-in / `import_fugue` so the LLM frames DAW-edit workflows correctly. Also added `import_fugue` to the Other tools list. |

---

## Feature 16: Native drag-in of `.mid` → fugue on an instance

See [ROADMAP.md §Feature 16](ROADMAP.md#feature-16-native-drag-in-of-mid--fugue-on-an-instance). Implements the return leg of the round-trip.

### Phase 16a — MIDI parser → FugueDefinition

**Status (2026-04-20 — shipped):** all tasks complete. 12 unit tests pass.

| Done | # | Task | Notes |
|------|---|------|-------|
| [x] | 16a.1 | New [src/fugue/import.rs](../../../src/fugue/import.rs) with `smf_to_fugues(&[u8], opts) -> Result<Vec<FugueDefinition>, String>` | Shipped as a **two-stage decoupled pipeline**: `ImportedMidi::parse(bytes, strict)` (deserialize → intermediate) → `.into_fugues(&opts)` (convert with policy). `smf_to_fugues` is the thin wrapper for the common end-to-end case. Mirrors the export side's merge → build → serialize shape, so callers can inspect the parsed intermediate before applying policy. |
| [x] | 16a.2 | Note on/off pairing | FIFO queue of unmatched note-ons per `(channel, note)` so rapid re-triggers of the same pitch pair correctly. `NoteOn vel=0` treated as note-off (running-status convention). Dangling note-ons close at EndOfTrack tick (or max event tick fallback) so the fugue scheduler never sees a stuck note. |
| [x] | 16a.3 | CC events passthrough | Direct translation: `MidiMessage::Controller { cc, value }` → `FugueEvent::Cc { channel, cc, value, curve: None }`. Imported fugues default to `cc_interpolation: Linear` so DAW-emitted dense CC streams stay smooth on playback. |
| [x] | 16a.4 | Drop per-note expression | `PitchBend` and `Aftertouch` dropped silently by default. `ImportOptions { strict: true }` errors on those events instead — caller-facing guarantee for round-trip fidelity. `ProgramChange` and `ChannelAftertouch` also dropped (not representable in the fugue schema). |
| [x] | 16a.5 | Multi-track handling | SMF format 0 → one fugue. SMF format 1 → one fugue per track that carries playable MIDI events (pure-meta tracks including the conductor are skipped). Tag comes from the track's `TrackName` meta when present, otherwise a synthetic `imported-N` in first-seen order. `ImportOptions::tag_prefix` namespaces the whole batch. |
| [x] | 16a.6 | Tests | 12 tests: malformed bytes → `Err`, conductor-only file → no fugues, tagged single-fugue round-trip, multi-track round-trip, `tag_prefix` namespacing, synthetic tag fallback, NoteOn-vel-0 pairing, dangling note-on closes at EOT, CC passthrough, strict-mode rejects pitch-bend, option passthrough (loop_mode/quantize/cancel_mode), intermediate-without-policy inspection pattern. |

### Phase 16b — HTTP endpoint + drop zone

**Status (2026-04-20 — shipped):** all four tasks complete.

| Done | # | Task | Notes |
|------|---|------|-------|
| [x] | 16b.1 | `POST /api/import_fugue?instance=&tag_prefix=&loop_mode=&quantize=` | Body: raw SMF bytes (Content-Type `audio/midi` or `application/octet-stream` — axum's `Bytes` extractor accepts either). Wired into both [src/gui/server.rs](../../../src/gui/server.rs) (standalone HTTP) and [src/gui/routes.rs](../../../src/gui/routes.rs) (wry custom protocol). Shared core in [src/gui/api.rs](../../../src/gui/api.rs) `api::import_fugue` so MCP and HTTP land in the same code path. Response: `{ ok, fugue_ids: [..] }`. |
| [x] | 16b.2 | Drop zone in [App.tsx](../../../frontend/src/App.tsx) | Whole `.sequencer-view` is a drop target. `dragenter/leave` depth-counted to avoid child-element flicker. Dashed-outline highlight via `.sequencer-view.is-drop-target` + a header "drop hint" that only renders during an active drag. Dropped files are filtered by `.mid` / `.midi` extension; each is read as ArrayBuffer and POSTed sequentially so per-file errors don't cascade. Fugue list auto-refreshes after import. |
| [x] | 16b.3 | MCP tool `import_fugue(instance, base64_mid, options?)` | `#[tool]` fn in [src/mcp/server.rs](../../../src/mcp/server.rs) taking `ImportFugueRequest { instance, base64_mid, tag_prefix?, loop_mode?, quantize?, strict? }`. Base64 decode via `base64 = "0.22"` (new direct dep; was already transitively pulled by tokio-rustls). Delegates to `api::import_fugue` using the same `ImportFugueQuery` shape as the HTTP handler — one core function, three surfaces (wry, HTTP, MCP). Extracted `parse_loop_mode_str` + `parse_quantize_str` helpers and pointed the existing `queue_fugue` parser at them to keep the formats in sync. |
| [x] | 16b.4 | Per-instance drop | v1 routes drops to the currently-selected instance (`selectedInstance`). A future multi-instance sidebar would let the user drop on a specific row; deferred until the UI grows that affordance — the current single-instance view makes per-row dropping ambiguous. |

---

## Feature 17: MCP tool `get_fugue(id)` — read back a FugueDefinition

See [ROADMAP.md §Feature 17](ROADMAP.md#feature-17-mcp-tool-get_fugueid--read-back-a-fuguedefinition). Closes the loop: LLM reads user-edited fugues.

| Done | # | Task | Notes |
|------|---|------|-------|
| [x] | 17.1 | `FugueBridge::get_definition(instance, id) -> Option<FugueDefinition>` | Already shipped in [src/fugue/bridge.rs:231](../../../src/fugue/bridge.rs#L231). Thin wrapper around the existing plural `get_definitions`. |
| [x] | 17.2 | New MCP tool `get_fugue` | `#[tool]` fn in [src/mcp/server.rs](../../../src/mcp/server.rs). Takes `GetFugueRequest { instance, id, compact }`. Default `compact: true` returns the LLM-friendly shape; `compact: false` returns the raw `FugueDefinition` for debugging. Request type wired via [src/mcp/requests.rs](../../../src/mcp/requests.rs). |
| [x] | 17.3 | Compact-schema response | `compact_fugue_view(&FugueDefinition) -> serde_json::Value` in server.rs. Pairs NoteOn/NoteOff by `(channel, note)` FIFO (handles re-triggers), dangling-on closes at `duration_beats`. Groups CC by `(channel, cc)` preserving first-seen lane order. Per-note expression lanes come back as the dense server-expanded points (anchors not recovered) — doc'd in instructions.md as a caveat. Returns `type: "composite"` shape so round-tripping through `queue_fugue` works by construction. 5 unit tests cover note pairing, dangling notes, CC bucketing, metadata fields, empty-lane shape. |
| [x] | 17.4 | Docs: read-modify-write pattern | Added "Other tools" list + a dedicated "Read-modify-write with `get_fugue`" paragraph in [instructions.md](../../../src/mcp/instructions.md) explaining the tag-swap replacement workflow and the per-note expression caveat. |

---

## Resolved design decisions

- **Instance ID param encoding:** went with `ParamInfoFlags::IS_READONLY` + constant numeric value `0.0` + `value_to_text` returning the `instance_id` string. Host extensions read via `addDirectParameterValueDisplayObserver`. Numeric value is meaningless but that's fine — only the displayed string matters for this correlation.
- **Degradation messaging:** `get_project_state` returns `layout_available: bool` alongside the instance list. `false` = no host extension; the LLM falls back. Instructions.md steers the fallback explicitly.
- **`device_path` addressing:** note-based pad segment (`pad:36` = MIDI C2). Survives pad reorderings because it's addressed by trigger note rather than position. `parse_device_path` rejects out-of-range notes.
- **Storage primitive:** used `OnceLock<RwLock<Option<ProjectLayout>>>` to match the existing bridge pattern rather than pulling in `ArcSwap` for a single struct. Contention is negligible (writer runs only when the extension re-POSTs on change).
