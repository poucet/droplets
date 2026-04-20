# Droplets — Bitwig Controller Extension

A Bitwig Studio controller extension that gives the Simply Droplets plugin
(and the LLM driving it via MCP) visibility into what's on each track:
device names, preset names, drum-pad maps, and which track each Droplets
instance sits on.

On startup and on every relevant project change, the extension POSTs a
`ProjectLayout` JSON to `http://127.0.0.1:9999/project_layout` and keeps a
WebSocket open at `ws://127.0.0.1:9999/ws/controller` for future commands
from the plugin process. When it finds a Droplets device on a track, it
auto-renames that instance to the track name via `/rename_instance`.

## Requirements

- macOS with Homebrew
- Bitwig Studio 6 (uses extension API 18+)
- OpenJDK from Homebrew: `brew install openjdk`
- Kotlin compiler: `brew install kotlin`

The build script expects Bitwig at `/Applications/Bitwig Studio.app` and
OpenJDK at `/opt/homebrew/opt/openjdk`. Override by setting `JAVA_HOME`.

## Build

```bash
cd extensions/bitwig
./build.sh
```

This copies `bitwig.jar` out of `Bitwig Studio.app` into `libs/` on first
run, then produces `build/Droplets.bwextension` (~5 MB).

## Install

```bash
./install.sh
```

Builds (if needed) and copies to `~/Documents/Bitwig Studio/Extensions/`.
Override the destination with `BITWIG_EXTENSIONS_DIR=/some/path ./install.sh`.

Bitwig auto-reloads extensions when the file changes; no restart needed.

Enable it: **Settings → Controllers → Add Controller → Simply Chris → Droplets**.
You should see `[droplets] Droplets extension initialized` in Bitwig's console
(**View → Controller Console**).

## Manual test walkthrough

1. **Start the plugin process.** Open any Bitwig project, load Simply Droplets
   on one track. The plugin process must be running for HTTP POSTs to
   succeed — check the console for the instance ID (`droplets-a1b2c3d4`).
2. **Load the extension.** Controllers → Add → Droplets. Confirm the console
   prints `Droplets extension initialized — watching 32 tracks`.
3. **Drum track.** Add a Drum Machine on a new track named "Drums". Load a
   sample on the C1 pad. The extension should POST a layout with
   `type: "drum_machine"` and a pad at `note: 36`.
4. **Instrument track.** Add any synth on a track named "Bass". POST should
   carry `type: "instrument"` with the synth name and preset.
5. **Auto-rename.** Put Droplets on "Drums" and "Bass". Each Droplets
   instance should have its name updated to the track name — visible in
   the Droplets UI at `http://127.0.0.1:9998`.
6. **WebSocket.** The extension logs `WS connected` once the plugin's
   `/ws/controller` endpoint is up. If the plugin isn't running yet it will
   log retries with exponential backoff.

## What's in the JSON

Matches the wire format defined in `docs/droplets/0.1/TASKS.md` 14a.2.
Device variants: `instrument`, `effect`, `drum_machine`, `unknown`.
`container` (chain selector, instrument layer) is emitted as `unknown`
for v1 — walking nested chains is a follow-up.

## Known v1 limitations

- **No parameter introspection** — `parameters: []` always. Tier-3 param
  reads are a later phase.
- **No native Sampler sample-name read** — Bitwig's Sampler exposes the
  sample path only via `createSpecificBitwigDevice(UUID)`. Falls back to
  `preset_name`, which carries the sample name for drag-and-drop samples.
- **No container walk** — Chain Selector / Instrument Layer devices emit
  as `unknown` rather than recursing into their chains.
- **Fixed bank sizes** — 32 tracks, 16 devices per track, 128 drum pads,
  8 devices nested per pad. Projects exceeding these limits get truncated.
- **Drum pad MIDI note = `36 + padIndex`** — assumes the Drum Machine's
  root note is C1. Bitwig's Drum Machine allows shifting this; shifted
  pads will report the wrong MIDI note.

## Troubleshooting

- **"Unable to locate a Java Runtime"** — install OpenJDK:
  `brew install openjdk`.
- **"cp: libs/bitwig.jar: Permission denied"** — Bitwig's jar is inside the
  app bundle; make sure `/Applications/Bitwig Studio.app` exists and is
  readable.
- **POST always fails** — the plugin process isn't running, or its MCP
  server isn't listening on `:9999`. Load Droplets on at least one track
  and check for `MCP server listening on 9999` in the plugin log.
- **Extension doesn't load** — Bitwig's Controller Console (View menu) will
  print the loader error. Common cause: the UUID in
  `DropletsExtensionDefinition` collides with another installed extension.
