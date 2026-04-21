# 🎹 Simply Droplets

> **Let AI make music in your DAW** — Connect Claude, GPT, or any AI to Ableton, Bitwig, Logic, and more!

Simply Droplets is a VST3/CLAP plugin that bridges AI assistants and your DAW. Ask Claude to play a melody, create a drum pattern, or automate your synth — and hear it instantly in your project.

Built by [**Christophe Poucet**](https://www.simplychris.ai) (Simply Chris).

---

## ✨ What Can It Do?

 - 🎵 **Real-time MIDI** — AI sends notes and CC messages directly to your instruments
 - 🔁 **Looping Patterns** — Create "fugues" that sync to your DAW's transport
 - 🎛️ **Parameter Control** — Automate any plugin via MIDI CC or parameter slots
 - 📤 **MIDI Export** — Drag AI-created patterns into your arrangement as clips
 - 🎚️ **MIDI 2.0** — Per-note pitch bend, pressure, and 16-bit velocity

---

## 🏗️ Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         YOUR DAW                                │
│  ┌─────────────────┐    MIDI     ┌─────────────────────────┐    │
│  │                 │◄───────────►│    Your Instruments     │    │
│  │ Simply Droplets │             │                         │    │
│  │     Plugin      │             │  🎹 Synths              │    │
│  │                 │             │  🥁 Drums               │    │
│  └────────┬────────┘             │  🎸 Samplers            │    │
│           │                      └─────────────────────────┘    │
└───────────┼─────────────────────────────────────────────────────┘
            │ MCP (HTTP)
            ▼
┌─────────────────────────────────────────────────────────────────┐
│                        AI ASSISTANT                             │
│                                                                 │
│   🤖 Claude, GPT, or any MCP-compatible AI                      │
│                                                                 │
│   "Play a jazz chord progression"                               │
│   "Create a 4-bar drum loop"                                    │
│   "Sweep the filter cutoff from 20% to 80%"                     │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## 🚀 Quick Start

### 1️⃣ Install the Plugin

Build from source (see [Building from Source](#-building-from-source) below) and run `cargo xtask install` — it copies the plugin to the right location automatically.

| Platform | VST3 Location | CLAP Location |
|----------|---------------|---------------|
| 🍎 macOS | `~/Library/Audio/Plug-Ins/VST3/` | `~/Library/Audio/Plug-Ins/CLAP/` |
| 🪟 Windows | `C:\Program Files\Common Files\VST3\` | `C:\Program Files\Common Files\CLAP\` |
| 🐧 Linux | `~/.vst3/` | `~/.clap/` |

### 2️⃣ Load in Your DAW

Add Simply Droplets as a **MIDI effect** and route its output to your instruments.

**Bitwig:** drop Droplets on a Note Effects chain before any instrument; MIDI flows downstream automatically.

**Ableton Live:** the VST3 build is classified as an instrument (a clap-wrapper limitation — the wrapper can't emit a MIDI-effect-only VST3 category today). Use Ableton's two-track MIDI-routing pattern:

1. Put Droplets on its own MIDI track.
2. Create a second MIDI track with your real instrument (drum rack, synth, sampler, etc.).
3. On the second track, set **MIDI From** to the Droplets track and select **Droplets** in the plugin dropdown below it.
4. Arm the second track (or set monitor to `In`) so it hears the incoming MIDI.

Droplets still appears in the Plug-Ins browser under the Instruments list. Proper MIDI-effect classification is tracked as a post-demo fix.

### 3️⃣ Connect Your AI

Simply Droplets speaks MCP over **streamable HTTP** on `http://localhost:9999/mcp`. MCP clients that speak HTTP directly (e.g. Claude Code, Cursor) can use that URL as-is:

```json
{
  "mcpServers": {
    "simply-droplets": {
      "url": "http://localhost:9999/mcp"
    }
  }
}
```

**Claude Desktop is stdio-only** and does not support streamable HTTP. Bridge through [`mcp-remote`](https://www.npmjs.com/package/mcp-remote), passing `--allow-http` so it accepts the non-TLS loopback URL:

```json
{
  "mcpServers": {
    "simply-droplets": {
      "command": "npx",
      "args": [
        "-y",
        "mcp-remote",
        "http://localhost:9999/mcp",
        "--allow-http"
      ]
    }
  }
}
```

### 4️⃣ Make Music! 🎉

Ask Claude:
> "Play middle C for 2 beats, then E and G together"

Or get fancy:
> "Create a chill lo-fi beat with swing"

---

## 🎼 The Fugue System

**Fugues** are musical sequences that sync perfectly with your DAW's transport. Think of them as AI-generated MIDI clips that play in real-time.

### Features

| Feature | Description |
|---------|-------------|
| 🎯 **Quantized Start** | Begin on the next beat, bar, or 4-bar phrase |
| 🔄 **Looping** | Play once, N times, or forever |
| 🎹 **Layering** | Stack multiple patterns simultaneously |
| 🏷️ **Tags** | Group patterns (e.g., cancel all "drums" at once) |
| 📈 **CC Automation** | Smooth parameter sweeps with interpolation |

### Compact Format

Fugues use a compact JSON schema designed for LLM token efficiency. Each fugue is a typed block — `notes`, `cc`, `per_note_pitch_bend`, `per_note_pressure`, or a `composite` that bundles several together:

```json
{
  "fugues": [
    {
      "type": "notes",
      "tag": "melody",
      "notes": [
        { "beat": 0, "note": "C4", "duration": 1, "velocity": 100 },
        { "beat": 1, "note": "E4", "duration": 1 },
        { "beat": 2, "note": "G4", "duration": 2 }
      ]
    },
    {
      "type": "cc",
      "cc": 74,
      "points": [[0, 20], [4, 100]],
      "interpolation": "exp"
    }
  ],
  "quantize": "bar",
  "loop_mode": "forever"
}
```

Notes auto-generate their own note-offs at `beat + duration`. CC points interpolate smoothly on the audio thread.

---

## 🛠️ MCP Tools

> **MIDI is fugue-only by design.** LLM round-trip latency (~1–10s) is too high for musically-timed one-shot events — notes, CC, and MIDI 2.0 per-note expression all go through `queue_fugue`, which schedules events on the audio thread with sample-accurate timing. Use a `composite` fugue to bundle notes + CC automation + per-note pitch bend + pressure into one atomic musical moment.

### 🎼 Fugues
| Tool | Description |
|------|-------------|
| `queue_fugue` | Schedule one or more fugues (notes / cc / pitch_bends / pressures / composite) with tempo-quantized start |
| `list_fugues` | List active + pending fugues with timing and loop progress |
| `get_fugue` | Read a single fugue's full content back in the same compact shape `queue_fugue` accepts — enables read-modify-write |
| `import_fugue` | Import a base64-encoded `.mid` as one or more fugues (the return leg of drag-out → edit-in-DAW → hand back) |
| `cancel_fugue` | Stop a specific fugue by id |
| `cancel_fugues_by_tag` | Stop every fugue sharing a tag (e.g. `"melody"`) |
| `clear_fugues` | Emergency stop — cancel every fugue on an instance |

### 🎚️ Transport & context
| Tool | Description |
|------|-------------|
| `get_transport` | Current `{beat, tempo, playing, time_sig, loop bounds}` for reasoning about scheduling |
| `get_project_state` | **Call first when composing.** Summary of connected Droplets instances, each track's primary device, and drum-pad maps (pitch notation + sample names) so the LLM writes correct notes instead of GM conventions |

### 🏷️ Instances & slots
| Tool | Description |
|------|-------------|
| `list_instances` | Every connected Droplets plugin process |
| `set_instance_name` | Rename an instance (`bass`, `pad`, `lead`) for clearer targeting |
| `list_slots` | Parameter slots with their CC mappings and current values |

---

## 🖥️ Plugin UI

Four tabs in the plugin window:

### 🎼 Sequencer
- Every active fugue renders its own piano-roll grid with a live playhead.
- Drag a row (or the `⇣ Drag all` button) onto your DAW to drop a `.mid` clip of the fugue(s).
- Drop a `.mid` back onto the sequencer panel to re-queue it as fugues (drag-round-trip).
- Cancel, export, or export-all via the row controls.

### 🎛️ MIDI Mapping
- Per-instance list of CC slot parameters with their names and mapped CC numbers.
- Wiggle a slot to test the mapping; use MIDI-learn to bind a hardware controller.

### 🎹 DAW
- Live view of what the host controller extension (Bitwig today) has pushed: tracks, primary devices, drum-pad maps with sample names, per-track Remote Controls.
- Empty state shown when no extension is running (Ableton, Logic, older Bitwig).

### ⚙️ Settings
- MCP server URL (copy for your AI's config).
- Export folder for manually-saved `.mid` files.
- Custom instructions appended to the MCP system prompt.

---

## 📤 MIDI Export

Turn AI compositions into DAW clips:

1. Click **↓** on any fugue in the list
2. File saves to your exports folder
3. Finder/Explorer opens automatically
4. **Drag** the `.mid` file into your DAW!

---

## 🔧 Building from Source

### Prerequisites
- 🦀 Rust 1.70+
- 📦 Node.js 18+
- 🎛️ For VST3: a [VST3 SDK](https://github.com/steinbergmedia/vst3sdk) checkout, with `CLAP_WRAPPER_VST3_SDK` pointing at it

### Build + Install (macOS, Linux, Windows)

```bash
git clone https://github.com/poucet/droplets.git
cd simply-droplets

# Build bundles for your platform into target/bundle/
cargo xtask build

# …or build AND copy into your DAW's user plugin folders in one step
cargo xtask install
```

`cargo xtask install` detects your OS and copies the CLAP/VST3 bundles to the
right per-user location:

| Platform | CLAP | VST3 |
|----------|------|------|
| 🍎 macOS | `~/Library/Audio/Plug-Ins/CLAP/` | `~/Library/Audio/Plug-Ins/VST3/` |
| 🐧 Linux | `~/.clap/` | `~/.vst3/` |
| 🪟 Windows | `%LOCALAPPDATA%\Programs\Common\CLAP\` | `%LOCALAPPDATA%\Programs\Common\VST3\` |

Useful flags:

```bash
cargo xtask build --format clap            # skip VST3 if you don't have the SDK
cargo xtask build --profile debug --dev-gui # dev build with WebView devtools
cargo xtask install --skip-build           # just copy existing bundles
```

---

## 📁 Project Structure

```
simply-droplets/
├── 🦀 src/
│   ├── lib.rs              # Plugin entry point
│   ├── fugue/              # 🎼 Fugue sequencer
│   │   ├── sequencer.rs    # Transport-synced playback
│   │   ├── export.rs       # MIDI file export
│   │   └── settings.rs     # Persistent settings
│   ├── mcp/                # 🤖 AI communication
│   │   ├── server.rs       # MCP tool definitions
│   │   └── bridge.rs       # Plugin ↔ server bridge
│   ├── gui/                # 🖥️ UI backend
│   │   ├── api.rs          # REST endpoints
│   │   └── routes.rs       # Request routing
│   └── midi/               # 🎹 MIDI processing
│
├── ⚛️ frontend/            # React UI
│   ├── src/
│   │   ├── App.tsx         # Main app
│   │   └── components/     # UI components
│   └── package.json
│
└── 🔨 xtask/               # Build tools
```

---

## ⚙️ Configuration

### Ports
| Service | Port |
|---------|------|
| MCP Server | `9999` |
| GUI Server | `9998` |

### Settings Location
| Platform | Path |
|----------|------|
| 🍎 macOS | `~/.simply-droplets/settings.json` |
| 🪟 Windows | `%APPDATA%\Simply Droplets\settings.json` |
| 🐧 Linux | `~/.config/simply-droplets/settings.json` |

### Export Folder
| Platform | Default Path |
|----------|--------------|
| 🍎 macOS | `~/Music/Simply Droplets/Exports/` |
| 🪟 Windows | `Documents\Simply Droplets\Exports\` |
| 🐧 Linux | `~/Music/Simply Droplets/Exports/` |

---

## 🤝 Contributing

Contributions welcome! Feel free to:
- 🐛 Report bugs
- 💡 Suggest features
- 🔧 Submit pull requests

---

## 📜 License

MIT License — see [LICENSE](LICENSE) for details.

---

## 🙏 Credits

Built with love using:
- [clack-plugin](https://github.com/prokopyl/clack) — Rust CLAP framework
- [rmcp](https://crates.io/crates/rmcp) — MCP server
- [wry](https://github.com/tauri-apps/wry) — WebView
- [midly](https://crates.io/crates/midly) — MIDI files
- [React](https://react.dev/) — UI

---

<p align="center">
  Made with 🎵 by <a href="https://www.simplychris.ai">Christophe Poucet</a> (Simply Chris)
</p>
