# 🎹 Simply Droplets

> **Let AI make music in your DAW** — Connect Claude, GPT, or any AI to Ableton, Bitwig, Logic, and more!

Simply Droplets is a VST3/CLAP plugin that bridges AI assistants and your DAW. Ask Claude to play a melody, create a drum pattern, or automate your synth — and hear it instantly in your project.

---

## ✨ What Can It Do?

🎵 **Real-time MIDI** — AI sends notes and CC messages directly to your instruments
🔁 **Looping Patterns** — Create "fugues" that sync to your DAW's transport
🎛️ **Parameter Control** — Automate any plugin via MIDI CC or parameter slots
📤 **MIDI Export** — Drag AI-created patterns into your arrangement as clips
🎚️ **MIDI 2.0** — Per-note pitch bend, pressure, and 16-bit velocity

---

## 🏗️ Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         YOUR DAW                                 │
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
│                        AI ASSISTANT                              │
│                                                                  │
│   🤖 Claude, GPT, or any MCP-compatible AI                      │
│                                                                  │
│   "Play a jazz chord progression"                                │
│   "Create a 4-bar drum loop"                                     │
│   "Sweep the filter cutoff from 20% to 80%"                     │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

---

## 🚀 Quick Start

### 1️⃣ Install the Plugin

Download from [Releases](https://github.com/simply-chris/simply-droplets/releases) and copy to your plugin folder:

| Platform | VST3 Location | CLAP Location |
|----------|---------------|---------------|
| 🍎 macOS | `~/Library/Audio/Plug-Ins/VST3/` | `~/Library/Audio/Plug-Ins/CLAP/` |
| 🪟 Windows | `C:\Program Files\Common Files\VST3\` | `C:\Program Files\Common Files\CLAP\` |
| 🐧 Linux | `~/.vst3/` | `~/.clap/` |

### 2️⃣ Load in Your DAW

Add Simply Droplets as a **MIDI effect** and route its output to your instruments.

### 3️⃣ Connect Your AI

Add the MCP server to Claude's config:

```json
{
  "mcpServers": {
    "simply-droplets": {
      "url": "http://localhost:9999/mcp"
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

Fugues use a simple, efficient format:

```
note_on@0:60,100 | note_off@1:60 | cc@0.5:1,64
```

This plays:
- Note 60 (middle C) at beat 0 with velocity 100
- Releases it at beat 1
- Sets CC1 to 64 at beat 0.5

---

## 🛠️ MCP Tools

AI can use these tools to control your music:

### 🎹 Notes
| Tool | Description |
|------|-------------|
| `send_note_on` | Trigger a MIDI note |
| `send_note_off` | Release a MIDI note |
| `send_note_on_hires` | 16-bit velocity (MIDI 2.0) |

### 🎛️ Control
| Tool | Description |
|------|-------------|
| `send_cc` | Send MIDI CC message |
| `set_param` | Set automatable parameter slot |
| `send_per_note_pitch_bend` | Per-note pitch bend (MIDI 2.0) |
| `send_per_note_pressure` | Polyphonic aftertouch (MIDI 2.0) |

### 🎼 Fugues
| Tool | Description |
|------|-------------|
| `queue_fugue` | Schedule a musical sequence |
| `cancel_fugue` | Stop a specific fugue |
| `cancel_fugues_by_tag` | Stop all fugues with a tag |
| `clear_fugues` | Emergency stop — cancel everything |
| `list_fugues` | See what's playing |

### ℹ️ Info
| Tool | Description |
|------|-------------|
| `list_instances` | List connected plugins |
| `list_slots` | Show parameter slots |
| `get_activity` | Recent MIDI activity log |

---

## 🖥️ Plugin UI

The plugin has three tabs:

### 🎼 Sequencer
- See active fugues with real-time progress
- Export fugues as MIDI files (drag to DAW!)
- Cancel patterns individually

### 📊 Monitor
- Parameter slots with values and mappings
- Recent MIDI activity
- Test keyboard for manual notes

### ⚙️ Settings
- MCP server URL (copy for AI config)
- Export folder location
- Server port info

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

### Build Steps

```bash
# Clone
git clone https://github.com/poucet/simply-droplets.git
cd simply-droplets

# Build frontend
cd frontend && npm install && npm run build && cd ..

# Bundle plugin
cargo xtask bundle --release

# Find your plugins in target/bundle/
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
  Made with 🎵 by <a href="http://www.simplychris.ai">Simply Chris</a>
</p>
