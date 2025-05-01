# Active Context: Simply Droplets

## Current Focus
- Completed comprehensive architecture design for Simply Droplets
- Defined core types with detailed implementation options
- Established approach for 3D spatial positioning and time-warping
- Designed the Rain Catcher system for droplet generation
- Ready to begin implementation of core components

## Recent Changes
- Renamed project from "Granular VST" to "Simply Droplets" to reflect the distinctive droplet-based approach
- Created detailed architecture design document in memory-bank/design/architecture.md
- Defined core components with Rust struct definitions:
  - Distribution - for probabilistic or deterministic property distribution
  - TimeWarpCurve - for non-linear time manipulation within droplets
  - Position3D - for 3D spatial positioning with distributions for each axis
  - Droplet - representing active audio snippets with position and time-warp
  - RainCatcher - generator system for creating droplets with configured properties
  - DropletProcessor - main audio processing component
- Established detailed 3D audio positioning concept using radial coordinates:
  - Radius: Distance from the center (intensity/presence)
  - Azimuth: Horizontal angle around the listener (0-360°)
  - Elevation: Vertical angle (-90° to +90°)
  - This coordinate system makes it easy to compute polar opposites for audio balance
  - Options for both static positioning and dynamic movement along paths
- Designed time-warping system using power functions:
  - Exponential: `position.powf(1.0 + warp_curve * 3.0) * time_warp`
  - Logarithmic: `position.powf(1.0 / (1.0 + warp_curve.abs() * 3.0)) * time_warp`
  - Random variation (±20%) for organic sound textures
- Added support for dynamic droplet movement along paths with multiple interpolation options
- Created component relationship diagrams in the architecture document

## Implementation Insights
From architecture exploration:
- Even with basic stereo rendering, compelling spatial effects are achievable
- Pan: Simple gain adjustments between channels creates effective horizontal positioning
- Depth: Volume reduction (up to 70% at maximum depth) provides convincing distance cues
- Height: Creative stereo mixing adds vertical dimension to the stereo field
- Randomization: Applying controlled randomness creates diffuse, organic soundscapes
- Density-based timing: Formula `spawn_interval = sample_rate / (10.0 * density)` provides intuitive control
- Limiting to 32 concurrent droplets balances rich sound with CPU efficiency
- Normalization based on the square root of active droplet count effectively controls volume

## Development Approach

We will be breaking down the implementation into distinct building blocks, each following a two-step process:

1. **SuperCollider prototype** - To validate the concept, sound design, and functionality
2. **Rust implementation** - To integrate into the final VST/CLAP plugin

This approach allows us to rapidly test audio concepts before dedicating resources to plugin implementation.

## Building Blocks

1. **Distribution System**
   - SC Prototype: Create different distribution models (uniform, gaussian, etc.)
   - Rust Implementation: Implement the Distribution trait with various strategies

2. **Time Warping**
   - SC Prototype: Test power function warping, experiment with other curve shapes
   - Rust Implementation: Build the TimeWarpCurve implementation

3. **3D Radial Positioning**
   - SC Prototype: Test radial coordinate positioning and spatialization to stereo
   - Rust Implementation: Implement Position3D and RadialCoordinate structs

4. **Droplet Generation**
   - SC Prototype: Experiment with stochastic generation algorithms
   - Rust Implementation: Create the RainCatcher system

5. **Envelope Processing**
   - SC Prototype: Test different envelope shapes and their sonic impact
   - Rust Implementation: Add envelope support to the Droplet struct

6. **Audio Engine Core**
   - SC Prototype: Integrate all components in SuperCollider
   - Rust Implementation: Build the DropletProcessor and buffer management

7. **UI Components**
   - SC Prototype: Mock UIs in SuperCollider for testing parameter ranges
   - Rust Implementation: Create nih-plug-iced components

## Next Steps

1. Set up SuperCollider project for prototyping
2. Begin with Distribution System prototype in SuperCollider
3. Test and refine the concept
4. Implement Distribution system in Rust
5. Continue with incremental development of each building block

## Design Decisions

### Architectural Decisions
- Using power functions for time-warping (simple yet effective)
- Limiting to 32 concurrent droplets by default (balances richness and performance)
- Basic stereo rendering for initial implementation (postponing HRTF or Ambisonics)
- Sample-level processing rather than block-level (for fine-grained control)
- Circular buffer implementation for efficient audio storage
- Independent distributions per axis with option for joint distribution

### Implementation Priorities
1. Core droplet processing engine (time-warping, basic positioning)
2. Basic parameters and simple distributions
3. Simple UI controls
4. Enhanced 3D positioning
5. Advanced time-warping
6. Full UI with curve editors
7. Path-based movement
8. Advanced features (HRTF, multiple input streams, etc.)

### Parameter Design Philosophy
Organizing parameters into logical categories:
- Droplet Generation Parameters (duration, density, source selection)
- Envelope Parameters (shape, attack/decay/sustain/release)
- Time Manipulation Parameters (speed, curve type, warp amount, direction)
- Spatial Parameters (pan, depth, height, animation)
- Global Processing (dry/wet mix, output level, filtering)
