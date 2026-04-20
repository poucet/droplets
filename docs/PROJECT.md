---
project: droplets
version: "0.1"
phase: "01-demo-prep"
architecture_updated: 2026-04-17
---

# Droplets

**Problem:** Droplets is a CLAP/VST3 plugin that exposes MIDI 1.0/2.0 output to AI assistants via MCP, so LLMs can compose music inside a DAW. Direct per-note MCP calls are too slow for musical timing, so the core primitive is the **fugue** — a batched, tempo-quantized sequence scheduled from the audio thread with sample-accurate timing.

**Current focus:** Phase 01 (demo prep) — shipping a live MCP-music demo on **2026-04-23** (4 days). Priorities: correct docs, a system prompt that actually describes the shipping schema, multi-instance validation, and UI stability on the demo machine. Stretch: per-note expressivity inside fugues.

**Notes:**
- Uses `jj` (Jujutsu), not git. Commits: `jj commit -m "..."`.
- MCP server runs once per plugin process (`OnceLock` guard), port 9999. GUI server on 9998.
- Multi-instance: every plugin self-registers into a global `RwLock<HashMap>` with a random ID, and routes commands via lock-free `rtrb` ring buffers. `set_instance_name` renames instances for LLM reference ("lead", "bass", etc.).
- `FUGUE.md` is currently **stale** — documents a flat events array that does not match the shipping compact schema (`fugues: [{type: "notes"|"cc", ...}]`). Fixing this is Phase 01 P0.

---

## Architecture

> TBD. Regenerate from source before the next architectural review. Key entry points below for orientation:

- **Plugin entry:** [src/lib.rs](src/lib.rs) — `DropletPlugin` self-registers with `CcBridge` and `FugueBridge` on load.
- **MCP server:** [src/mcp/server.rs](src/mcp/server.rs) — tool definitions, request schemas, and `ServerInfo::instructions`.
- **MCP instance routing:** [src/mcp/bridge.rs](src/mcp/bridge.rs) — global registry, per-instance ring buffers.
- **Fugue scheduler:** [src/fugue/](src/fugue/) — audio-thread sample-accurate event dispatch.
- **GUI HTTP/WS server:** [src/gui/server.rs](src/gui/server.rs) — REST + WebSocket on :9998.
- **Frontend:** [frontend/src/](frontend/src/) — React SPA; `TimingManager.ts` handles client-side beat interpolation.
