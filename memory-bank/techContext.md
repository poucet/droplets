# Technical Context: Granular VST

## Technologies Used
- **Language:** Rust (latest stable)
- **Plugin Framework:** `nih-plug`
- **GUI Toolkit:** `iced` (via `nih-plug-iced`)
- **Build System:** `cargo`
- **Target OS:** macOS

## Development Setup
- Standard Rust toolchain (`rustup`, `cargo`).
- A Digital Audio Workstation (DAW) capable of loading VST3 or CLAP plugins on macOS (e.g., Reaper, Bitwig Studio, Ableton Live).
- `cargo-bundle` or `nih-plug`'s `xtask` for bundling.

## Technical Constraints
- Real-time audio processing requirements demand efficient code.
- GUI updates must be handled carefully to avoid impacting the audio thread.
- Compatibility with `nih-plug`'s API and conventions.
- VST3 plugins built with `nih-plug`'s default bindings are subject to GPLv3 licensing due to the `vst3-sys` crate. CLAP plugins do not have this restriction.

## Dependencies
- `nih-plug` (core framework)
- `nih-plug-iced` (Iced GUI integration)
- `iced` (GUI toolkit, pulled in by `nih-plug-iced`)
- Potentially other crates for DSP, random number generation, etc., as development progresses.
