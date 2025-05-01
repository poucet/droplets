# Active Context: Simply Droplets

## Current Focus
- Completed comprehensive architecture design for Simply Droplets
- Defined core types with detailed implementation options
- Established approach for 3D spatial positioning and time-warping
- Designed the Rain Catcher system for droplet generation
- Implemented all SuperCollider prototypes for each building block
- Ready to begin Rust implementation of core components

## Recent Changes
- Created and implemented all SuperCollider prototypes for the building blocks:
  - Distributions: Created various probabilistic distribution models with visualizations
  - Time Warping: Implemented power function warping with interactive testing tools
  - 3D Radial Positioning: Built radial coordinate positioning with stereo rendering
  - Droplet Generation: Created the Rain Catcher system with configurable distributions
  - Envelope Processing: Implemented various envelope shapes with parameter control
  - Audio Engine Core: Integrated all components in a complete audio engine
  - UI Prototype: Created parameter layout and controls with visualizations
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
From SuperCollider prototype implementation:

### Distribution System
- Gaussian distribution creates natural-sounding, focused droplet characteristics
- Clustered distribution with multiple centers creates more interesting spatial textures
- Bimodal distributions are effective for creating stereo opposition or focal points
- Interactive control of distribution shape is essential for sound design flexibility

### Time Warping
- Power function approach works effectively for both acceleration and deceleration effects
- Random variation of ±20% on the time warp factor creates more organic results
- Exponential warping (warp_curve > 0) creates forward momentum
- Logarithmic warping (warp_curve < 0) creates a sense of suspense or anticipation
- Most musically useful range is between -0.7 and 0.7 for the warp curve parameter

### 3D Radial Positioning
- Radial coordinate system simplifies many spatial audio calculations
- Even with basic stereo rendering, convincing spatial effects are achievable
- Elevation is the most challenging dimension to render convincingly in stereo
- Path-based movement creates more engaging spatial effects than static positioning
- The 'advanced' rendering mode provides the best balance between spatial clarity and stereo compatibility

### Droplet Generation
- The relationship between density parameter and spawning rate works intuitively
- Limiting to 32 concurrent droplets balances rich sound with CPU efficiency
- Clustered spatial distributions create more engaging and natural soundscapes
- Random variation in time warp and envelope parameters is essential for organic sound
- Duration distributions concentrated around 100-300ms provide the most musical results

### Envelope Processing
- Envelope shapes significantly impact the perceived character of droplets
- Envelope parameters should be adapted based on droplet duration
- Humanization (subtle random variations) creates more natural, organic textures
- Parameter ranges need careful constraining for musical results
- Envelope types can be parametrically distributed like other droplet properties

### Audio Engine Core
- Object-oriented design with clean separation between configuration and processing logic works well
- Processing per sample rather than per block provides more control but is more CPU intensive
- The square root normalization approach effectively manages volume as density changes
- Circular buffer implementation is critical for maintaining efficiency
- Hybrid approaches using native UGens for granular processing controlled by parameter logic provides the best performance

### UI Design
- Parameter organization into logical groups enhances usability
- Real-time visualization provides essential feedback for understanding complex parameters
- Interactive parameter relationships are essential for immediate user feedback
- Parameter ranges should be carefully constrained for musical results
- Consistent visual language improves learnability and reduces cognitive load

## Development Approach

We have completed the first step of our two-step development process:

1. **SuperCollider prototypes** - COMPLETED ✓
   - Created prototypes for all building blocks
   - Validated concepts, sound design, and functionality
   - Documented findings and parameter relationships

2. **Rust implementation** - NEXT PHASE
   - Implement the Distribution trait with various strategies
   - Build the TimeWarpCurve implementation
   - Implement Position3D and RadialCoordinate structs
   - Create the RainCatcher system
   - Add envelope support to the Droplet struct
   - Build the DropletProcessor and buffer management
   - Create nih-plug-iced UI components

## Next Steps

1. Begin Rust implementation of the Distribution system
   - Implement the Distribution trait with uniform, gaussian, and clustered distribution types
   - Create unit tests with validation against expected distributions
   - Implement parameter serialization for plugin state saving

2. Move to TimeWarpCurve implementation
   - Implement the power function approach validated in SuperCollider
   - Add support for randomization within specified bounds
   - Create unit tests with validation against expected time-warping behavior

3. Continue with incremental development of each building block
   - Position3D and related spatial components
   - RainCatcher droplet generation system
   - Droplet struct with envelope processing
   - Audio processing engine core
   - UI components with nih-plug-iced

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
