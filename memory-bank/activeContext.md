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
- Established detailed 3D audio positioning concept with:
  - X-axis: Left-right panning (stereo field)
  - Y-axis: Front-back positioning (depth/distance)
  - Z-axis: Up-down positioning (height)
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

## Next Steps
1. Implement core Distribution and TimeWarpCurve types
   - Start with simple implementations (parametric distributions, predefined curves)
   - Design with extensibility for later enhancements
   
2. Build the basic Droplet struct
   - Create time-warping implementation using power functions
   - Implement static 3D positioning (initially without path-based movement)
   - Add envelope processing for amplitude shaping
   
3. Implement RainCatcher system
   - Build probabilistic droplet generation based on density parameter
   - Implement property distribution sampling
   - Add resource management with concurrent droplet limiting

4. Create DropletProcessor
   - Implement circular buffer for input audio
   - Build sample-level processing loop
   - Add basic stereo spatialization

5. Integrate with nih-plug
   - Define parameter system
   - Create basic plugin structure
   - Implement audio processing callback

6. Add basic UI using nih-plug-iced
   - Start with simple parameter controls
   - Later add curve editors and spatial visualization

7. Enhance with advanced features
   - Path-based movement
   - Custom curve editors
   - More complex distributions

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
