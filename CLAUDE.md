# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Simply Droplets is a 3D droplet-based granular synthesis audio plugin built in Rust using the `clack-plugin` framework. It transforms incoming audio into "droplets" positioned in 3D space with configurable time-warping, offering an innovative extension of traditional granular synthesis.

## Core Concepts

- **Droplets**: Extended grains with 3D positioning and time-warping capabilities
- **3D Audio Positioning**: Radial coordinate system (radius, azimuth, elevation) mixed down to stereo
- **Time Warping**: Non-linear time manipulation within droplets using curve-based warping
- **Rain Catcher System**: Generator system creating droplets according to configurable rules

## Build Commands

```bash
# Build the plugin in debug mode
cargo build

# Build optimized release version
cargo build --release

# Bundle the plugin for distribution (creates VST3/CLAP formats)
cargo xtask bundle

# Bundle release version
cargo xtask bundle --release
```

The bundler creates both CLAP and VST3 plugin files in the `target/bundle/` directory.

## Architecture

### Current Implementation Status
The project now has a fully functional droplet-based granular synthesis engine. The plugin supports both CLAP and VST3 formats and includes comprehensive 3D audio processing with time-warping capabilities.

### Target Architecture (from design docs)
- **Droplet Processing Engine**: Core audio processing with 3D positioning
- **Distribution System**: Configurable probability distributions for droplet properties  
- **Time Warp Curves**: Non-linear time manipulation using various curve types
- **Rain Catcher**: Stochastic droplet generation and management
- **3D Spatialization**: Conversion from 3D audio to stereo output

### Key Files
- `src/lib.rs`: Main plugin implementation (currently basic gain plugin)
- `Cargo.toml`: Primary package configuration for the plugin
- `bundler.toml`: Plugin bundling configuration
- `xtask/src/main.rs`: Build automation (bundle command)
- `memory-bank/`: Design documentation and SuperCollider prototypes

## Plugin Framework

The plugin uses `clack-plugin` framework with these key components:
- Plugin struct implementing `Plugin` trait with `DefaultPluginFactory`
- Parameter system using atomic values for thread-safe parameter access
- Audio processing in the `process()` method with CLAP audio and event handling
- Plugin exports using `clack_export_entry!()` and `clap_wrapper::export_vst3!()` macros

## Important Implementation Notes

- **Parameter Access During Activation**: Never call parameter getters (like `get_grain_size()`) during plugin activation as this breaks UI integration. Use hardcoded defaults instead.
- **Thread Safety**: All parameter access in audio processing uses atomic operations for thread safety.
- **Memory Management**: Complex data structures (VecDeque, Vec) are initialized during activation but used safely in the audio thread.

## Development Workflow

1. **Prototype in SuperCollider**: Use `memory-bank/supercollider/` files for rapid DSP concept validation
2. **Implement in Rust**: Translate proven concepts to the main plugin codebase
3. **Bundle and Test**: Use `cargo xtask bundle` to create plugin files for DAW testing
4. **Git Best Practices**: After each atomic task, commit changes with descriptive messages

### Version Control (jj)
**IMPORTANT**: This project uses `jj` (Jujutsu) for version control. Always use `jj commit` (not `jj describe` or `git commit`) after completing discrete tasks:

```bash
# After each logical unit of work:
jj commit -m "Descriptive commit message"
```

Examples of atomic commits:
- Add new audio processing feature
- Update UI component
- Fix specific bug
- Add documentation
- Update dependencies

**Never** batch unrelated changes into a single commit. Each commit should represent one logical change that could be safely reverted independently.

## Important Implementation Notes

- Plugin ID: "com.simply-chris.simply-droplets" (CLAP) / VST3 class ID uses "SimplyDroplets01"
- Audio I/O: Stereo input/output (2 channels)
- The project uses a workspace with `xtask` member for build automation
- Current implementation is a placeholder - the real droplet processing engine needs to be built

## Memory Bank Documentation

The `memory-bank/` directory contains comprehensive design documentation:
- `productContext.md`: Problem definition and core concepts
- `design/architecture.md`: Detailed technical architecture
- `supercollider/`: Prototype implementations for validation