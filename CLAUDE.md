# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Simply Droplets is a 3D droplet-based granular synthesis audio plugin built in Rust using the `nih-plug` framework. It transforms incoming audio into "droplets" positioned in 3D space with configurable time-warping, offering an innovative extension of traditional granular synthesis.

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

The bundler uses `nih_plug_xtask` internally and creates plugin files in the `target/` directory.

## Architecture

### Current Implementation Status
The project is in early development with a basic gain plugin implementation serving as the foundation. The current `src/lib.rs` provides a simple VST3/CLAP plugin with gain and dry/wet parameters.

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

The plugin uses `nih-plug` framework with these key components:
- Plugin struct implementing `Plugin`, `ClapPlugin`, and `Vst3Plugin` traits
- Parameter system using `FloatParam` for audio parameters
- Audio processing in the `process()` method with `Buffer` and `ProcessContext`
- Plugin exports using `nih_export_vst3!()` and `nih_export_clap!()` macros

## Development Workflow

1. **Prototype in SuperCollider**: Use `memory-bank/supercollider/` files for rapid DSP concept validation
2. **Implement in Rust**: Translate proven concepts to the main plugin codebase
3. **Bundle and Test**: Use `cargo xtask bundle` to create plugin files for DAW testing
4. **Git Best Practices**: After each atomic task, commit changes with descriptive messages

### Git Workflow
**IMPORTANT**: Always commit work atomically after completing discrete tasks:

```bash
# After each logical unit of work:
git add <relevant-files>
git commit -m "Descriptive commit message

🤖 Generated with [Claude Code](https://claude.ai/code)

Co-Authored-By: Claude <noreply@anthropic.com>"
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