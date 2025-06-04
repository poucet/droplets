# 🎵 Simply Droplets - Installation Guide

## Quick Install for macOS + Ableton Live

### Method 1: One-Command Install
```bash
# Build and install in one command
cargo run --manifest-path xtask/Cargo.toml -- bundle simply_droplets && ./install_macos.sh
```

### Method 2: Step-by-Step
```bash
# 1. Build the plugin
cargo run --manifest-path xtask/Cargo.toml -- bundle simply_droplets

# 2. Run the installer
./install_macos.sh
```

## What the installer does:
- ✅ Copies VST3 to `~/Library/Audio/Plug-Ins/VST3/`
- ✅ Copies CLAP to `~/Library/Audio/Plug-Ins/CLAP/` (if available)
- ✅ Creates directories if they don't exist
- ✅ Removes old versions automatically

## In Ableton Live:
1. **Open Ableton Live**
2. **Go to:** Live > Preferences > Plug-ins
3. **Enable:** "Use VST3 Plug-in System Folders"
4. **Click:** "Rescan" button
5. **Find:** "Simply Droplets" in Audio Effects

## Plugin Locations:
- **VST3:** `~/Library/Audio/Plug-Ins/VST3/simply_droplets.vst3`
- **CLAP:** `~/Library/Audio/Plug-Ins/CLAP/simply_droplets.clap`

## Troubleshooting:

### Plugin doesn't appear in Ableton:
- Restart Ableton Live completely
- Check that VST3 folders are enabled in preferences
- Try rescanning plugins again

### UI issues:
- The React UI may have compatibility issues on macOS
- Audio processing still works fully
- Use parameter automation from Ableton's interface

### Check for errors:
```bash
# View system logs for plugin errors
open /Applications/Utilities/Console.app
# Filter for "simply_droplets" or "VST3"
```

### Manual Installation:
If the script doesn't work, manually copy:
```bash
cp -R target/bundled/simply_droplets.vst3 ~/Library/Audio/Plug-Ins/VST3/
```

## Other DAWs:

### Logic Pro:
Same VST3 location works automatically

### Pro Tools:
Requires AAX format (not currently supported)

### Reaper:
Will automatically find VST3 in the standard location

---

🎉 **Ready to make some droplet music!** 🎵