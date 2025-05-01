# Simply Droplets - SuperCollider Prototypes

This directory contains SuperCollider prototype implementations of the various building blocks for the Simply Droplets plugin. Each prototype will validate core concepts before implementing them in Rust.

## SuperCollider Prototype Philosophy

SuperCollider offers several advantages for prototyping audio DSP concepts:

1. **Rapid Iteration**: Quick implementation and real-time parameter adjustment
2. **Immediate Feedback**: Audio and visual feedback on concepts
3. **Rich DSP Library**: Access to existing UGens for comparison
4. **Visualization Tools**: Plotting and spectral analysis
5. **Separable Components**: Each building block can be developed independently

## Building Blocks Organization

Each building block has its own `.scd` file:

1. `01_distributions.scd` - Various statistical distributions
2. `02_timewarping.scd` - Non-linear time manipulation 
3. `03_radial_positioning.scd` - 3D spatial positioning with radial coordinates
4. `04_droplet_generation.scd` - Stochastic generation and management
5. `05_envelope_processing.scd` - Amplitude shaping of droplets
6. `06_audio_engine.scd` - Core processing and buffer management
7. `07_ui_prototype.scd` - Parameter layout and UI mock

## Integration File

The `simply_droplets_integrated.scd` file will combine all building blocks into a complete prototype that approximates the full functionality of the plugin.

## Block Implementation Status

| Building Block | Status | Notes |
|----------------|--------|-------|
| Distributions | Not Started | |
| Time Warping | Not Started | |
| Radial Positioning | Not Started | |
| Droplet Generation | Not Started | |
| Envelope Processing | Not Started | |
| Audio Engine | Not Started | |
| UI Prototype | Not Started | |
| Integrated System | Not Started | |

## Usage Instructions

To use these prototypes:

1. Install SuperCollider (https://supercollider.github.io/)
2. Open the desired `.scd` file
3. Start the SuperCollider server (Ctrl+B or Cmd+B)
4. Execute code blocks by positioning cursor and pressing Shift+Enter

## Parameter Ranges and Mappings

We'll document optimal parameter ranges here as we discover them during prototyping. These ranges will inform the final Rust implementation.

## Learning Resources

For those new to SuperCollider:
- [SuperCollider Documentation](https://doc.sccode.org/)
- [Eli Fieldsteel's SuperCollider Tutorials](https://www.youtube.com/playlist?list=PLPYzvS8A_rTaNDweXe6PX4CXSGq4iEWYC)
- [Nick Collins' SuperCollider Tutorial](https://composerprogrammer.com/teaching/supercollider/sctutorial/tutorial.html)
