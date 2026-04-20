# Droplets — Bitwig Controller Extension

Pushes the current project's track/device layout to the Simply Droplets
plugin so the LLM driving it over MCP can see what's on each track:
device names, preset names, drum-pad maps, per-track Remote Controls,
and which track each Droplets instance sits on. Auto-renames Droplets
instances to match their track name.

Wire format: JSON POSTed to `http://127.0.0.1:9999/project_layout` on
every relevant project change. Defined in `docs/droplets/0.1/TASKS.md`
§14a.2. A `ws://127.0.0.1:9999/ws/controller` WebSocket is reserved
for future plugin→extension commands.

## Build & install

Requires macOS, Bitwig 6 (API 18+), and Homebrew `openjdk` + `kotlin`.
The build script auto-discovers Bitwig at `/Applications/Bitwig Studio.app`
and OpenJDK at `/opt/homebrew/opt/openjdk` (override with `JAVA_HOME`).

```bash
cd extensions/bitwig
./install.sh   # builds and copies to ~/Documents/Bitwig Studio/Extensions/
```

Enable in Bitwig: **Settings → Controllers → Add Controller → Simply Chris → Droplets**.
Confirm `[droplets] Droplets extension initialized` appears in **View → Controller Console**.

## Known v1 limitations

- **Remote Controls only** — device-level parameter introspection
  (ranges, display strings) isn't wired; the LLM gets the track's
  Remote Controls page as parameter targets.
- **No container walk** — Chain Selector / Instrument Layer devices
  emit as `unknown` instead of recursing into nested chains.
- **Drum pad MIDI note = `36 + padIndex`** — assumes C1 root. Shifted
  Drum Machines will report wrong MIDI notes.
- **Fixed bank sizes** — 32 tracks, 16 devices per track, 128 drum
  pads, 8 devices per pad. Larger projects get truncated.

## Troubleshooting

- **POST always fails** — plugin not running, or MCP server not on
  `:9999`. Check for `MCP server listening on 9999` in the plugin log.
- **"Unable to locate a Java Runtime"** — `brew install openjdk`.
