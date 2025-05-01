# Simply Droplets - Architecture Design Document

## Overview

Simply Droplets is a 3D droplet-based granular synthesis audio plugin built in Rust using the `nih-plug` framework. Unlike traditional granular synthesis which focuses on "grains," this plugin uses the concept of "droplets" that can be positioned in 3D space and have configurable time-warping characteristics.

## Audio Processing Parameters

### Parameter Design Philosophy

The Simply Droplets architecture is designed to provide a flexible parameter system that can evolve beyond the current implementation. Parameters fall into several categories:

#### Droplet Generation Parameters
Controls how droplets are created and their basic characteristics:
- **Duration Range**: Configurable min/max duration (currently 5-500ms)
- **Density**: Controls droplet spawn probability/frequency
- **Source Selection**: Where in the buffer droplets are sampled from (could be expanded to multiple buffers/sources)
- **Randomization**: Controllable variance applied to various droplet properties

#### Envelope Parameters
Configurable amplitude shaping over a droplet's lifetime:
- **Envelope Type**: ADSR, custom shape, drawable curve, etc.
- **Attack/Decay/Sustain/Release**: Either fixed proportions or independent parameters
- **Envelope Curves**: Linear, exponential, or custom shapes for each envelope segment

#### Time Manipulation Parameters
Controls how time flows within droplets:
- **Playback Speed**: Base time-warp factor (slower/faster)
- **Warp Curve Type**: Linear, exponential, logarithmic, or custom shapes
- **Warp Amount**: Intensity of the warping effect
- **Directional Control**: Forward, reverse, alternating, or randomized playback

#### Spatial Parameters
Controls positioning in a 3D sound field:
- **Pan Distribution**: Horizontal (L-R) positioning and spread
- **Depth Control**: Front-back positioning with optional distance simulation
- **Height Parameters**: Vertical positioning with variable stereo mix
- **Spatial Animation**: Optional movement paths or patterns

#### Global Processing
Overall sound shaping:
- **Dry/Wet Mix**: Balance between processed and original signal
- **Output Level**: Volume control with possible limiting/compression
- **Filter Controls**: Optional frequency filtering

### Learnings from the Current Implementation

The prototype implementation revealed several valuable insights:
1. **Dry/Wet Mix** is essential for practical use cases
2. **Envelope Shaping** significantly impacts the perceived character of droplets
3. **3D Positioning** creates engaging spatial effects even when collapsed to stereo
4. **Random Variation** is crucial for organic sound, preventing mechanical artifacts
5. **Normalization** based on active droplet count prevents unexpected volume spikes

## Core Concepts

### Droplets vs. Grains

Traditional granular synthesis uses "grains" - small snippets of audio that are played back with various modifications. Simply Droplets extends this concept with "droplets":

- Droplets capture a slice of input audio (like grains)
- Droplets have a random duration (configurable between 20-250ms by default)
- Droplets are positioned in 3D space 
- Droplets have a time-warping characteristic that affects playback

### 3D Audio Positioning

Each droplet exists in a 3D audio space with three primary axes:
- **X-axis**: Left-right panning (stereo field)
- **Y-axis**: Front-back positioning (depth/distance)
- **Z-axis**: Up-down positioning (height)

This 3D positioning system supports multiple rendering approaches:

#### Architectural Options

1. **Basic Stereo Rendering**
   - Pan values translate directly to stereo channel gains
   - Depth simulated through volume reduction and optional filtering
   - Height dimension creatively mixed into stereo field

2. **Advanced Spatialization**
   - HRTF (Head-Related Transfer Function) for realistic 3D perception
   - Ambisonics encoding for multi-speaker reproduction
   - Binaural rendering for headphone optimization

3. **Dynamic Positioning**
   - Static droplet positions vs. moving droplets
   - Path-based movement with configurable trajectories
   - Reactive movement based on audio characteristics

#### Implementation Insights

The prototype implementation demonstrated that even with basic stereo rendering techniques, compelling spatial effects can be achieved:

- **Pan**: Simple gain adjustments between channels creates effective horizontal positioning
- **Depth**: Volume reduction (up to 70% at maximum depth) provides convincing distance cues
- **Height**: Creative stereo mixing of height information adds vertical dimension to stereo field
- **Randomization**: Applying controlled randomness to positions creates diffuse, organic soundscapes

The 3D spatial system works especially well when each droplet has unique spatial characteristics within configurable distributions, rather than fixed positions.

### Time Warping

A core concept in Simply Droplets is the non-linear manipulation of time within each droplet, enabling unique sonic textures beyond traditional granular synthesis.

#### Architectural Approach

Time warping can be approached in multiple ways:

1. **Curve-Based Warping**
   - Pre-defined curves (linear, exponential, logarithmic)
   - User-drawn custom curves via UI
   - Mathematical formulas (power, sinusoidal, etc.)

2. **Playback Direction Control**
   - Forward, reverse, or bidirectional playback
   - Palindromic (forward then reverse) playback
   - Random direction decisions

3. **Advanced Time Manipulation**
   - Time-stretching with phase vocoder techniques
   - Pitch-shifting independent of duration
   - Freeze points where time temporarily stops

4. **Modulation Sources**
   - Per-droplet unique warping
   - Time-based modulation (LFOs)
   - Audio-reactive warping based on signal characteristics

#### Implementation Insights

The prototype implementation demonstrated effective results with relatively simple power-function warping:

- **Linear warping** (when warp curve parameter is near zero) provides clean, predictable playback
- **Exponential curves** (warp_curve > 0.0) create acceleration effects, useful for percussive sounds
- **Logarithmic curves** (warp_curve < 0.0) create deceleration effects, useful for creating suspense or emphasis on attack

The specific implementation used power functions:
- Exponential: `position.powf(1.0 + warp_curve * 3.0) * time_warp`
- Logarithmic: `position.powf(1.0 / (1.0 + warp_curve.abs() * 3.0)) * time_warp`

Adding random variation (±20%) to the time warp factor creates more organic sound textures, preventing the mechanical quality often associated with granular synthesis.

### Rain Catcher System

The Rain Catcher represents a generator system that creates droplets according to configurable rules and distributions. This system forms the heart of the droplet generation process.

#### Architectural Approach

The Rain Catcher concept can be implemented in various ways:

1. **Probabilistic Generators**
   - Stochastic processes based on density parameters
   - Random or pseudo-random timing of droplet creation
   - Distribution-based property assignment

2. **Pattern-Based Generators**
   - Rhythmic/sequence-based droplet creation
   - Musical time division (beat-synced) spawning
   - Algorithmic composition techniques

3. **Reactive Generators**
   - Audio-reactive droplet creation based on input analysis
   - Envelope following to determine droplet timing
   - Frequency/spectral content-based spawning

4. **Multiple Generator Types**
   - Different rain catcher types with different characteristics
   - Layering multiple rain catchers for complex textures
   - Independent control of different droplet populations

#### Core Responsibilities

Regardless of implementation details, any Rain Catcher system handles:
- **Creation Timing**: When to spawn new droplets
- **Property Assignment**: Setting initial values for all droplet parameters
- **Population Management**: Enforcing limits on concurrent droplets
- **Resource Optimization**: Pruning inactive droplets and efficient allocation

#### Implementation Insights

The prototype implementation provided valuable insights:

- **Density-Based Timing**: Converting a density parameter to spawn interval works well
  - Formula: `spawn_interval = sample_rate / (10.0 * density)` provides intuitive control
  - This approach naturally adapts to different sample rates

- **Randomized Properties**: Random variation within set ranges creates organic results
  - Duration, position, and time warp all benefit from controlled randomization
  - Limited randomization (e.g., ±20% for time warp) maintains musical coherence

- **Resource Management**: 
  - Limiting to 32 concurrent droplets balances rich sound with CPU efficiency
  - Automatic pruning of inactive droplets prevents memory bloat
  - Normalization based on the square root of active droplet count controls volume effectively

These techniques produce evolving, organic textures that avoid the mechanical qualities sometimes associated with granular synthesis, while maintaining predictable performance characteristics.

## Architecture

### Core Types

#### Distribution

```rust
/// Distribution that can be defined by knobs or curves
struct Distribution {
    // Implementation variations:
    // 1. Simple parametric (min, max, curve shape)
    // 2. Drawable curve (series of points with interpolation)
    // 3. Mathematical function
    
    fn sample(&self, context: &SampleContext) -> f32;
}
```

#### TimeWarpCurve

```rust
/// Time warping curve - defines how time flows within a droplet
struct TimeWarpCurve {
    // Implementation options:
    // 1. Predefined curve types (linear, exponential, etc.)
    // 2. User-drawn curve via UI
    // 3. Bezier or spline curve
    
    fn warp(&self, t: f32) -> f32;
}
```

#### Position3D

```rust
/// 3D position with distributions for each axis
struct Position3D {
    // Option 1: Independent distributions per axis (original approach)
    x_distribution: Distribution,
    y_distribution: Distribution,
    z_distribution: Distribution,
    
    // Option 2: Joint distribution for correlated coordinates
    joint_distribution: Option<JointDistribution3D>,
    
    fn sample(&self, context: &SampleContext) -> Vector3D;
}

/// Joint distribution for sampling correlated 3D coordinates
struct JointDistribution3D {
    // Various implementation options:
    // 1. Correlation matrix between axes
    // 2. Geometric primitives (sphere, cylinder, plane, etc.)
    // 3. Path-based distributions (following trajectories)
    // 4. Cluster-based approach (multiple centroids with spread)
    
    fn sample(&self, context: &SampleContext) -> Vector3D;
}

/// Geometric shape-based distributions
enum GeometricDistribution {
    // Distributes points within a sphere
    Sphere {
        center: Vector3D,
        radius: Distribution,
        // Optional: non-uniform distribution within the sphere
        distance_from_center_bias: Option<Distribution>,
    },
    
    // Distributes points along a path
    Path {
        points: Vec<Vector3D>,
        spread: Distribution,     // How far from the path points can be
        progression: Distribution, // How to move along the path
    },
    
    // Other shapes: Cube, Plane, Cylinder, etc.
    // Each with appropriate parameters
}

/// Defines how a droplet moves through 3D space over time along a user-defined path
struct MovementTrajectory {
    // Control points defining the path
    control_points: Vec<Vector3D>,
    
    // How the droplet moves along the path
    // 0.0 = start of path, 1.0 = end of path
    position_curve: TimeWarpCurve,
    
    // Movement speed factor
    speed: f32,
    
    // Whether to loop when reaching the end of the path
    loop_path: bool,
    
    // B-spline or Bezier interpolation settings
    interpolation_type: PathInterpolation,
}

enum PathInterpolation {
    // Linear interpolation between points (simplest)
    Linear,
    
    // Cubic B-spline for smooth curved paths
    BSpline {
        tension: f32,  // Controls how tightly the curve follows control points
    },
    
    // Bezier curves for precise control
    Bezier,
    
    // Catmull-Rom splines (passes through all control points)
    CatmullRom {
        alpha: f32,  // Controls curve tightness (0.0 = uniform, 0.5 = centripetal)
    },
}
```

#### Droplet

```rust
/// An active droplet playing in 3D space
struct Droplet {
    // Capture information
    source_buffer: AudioBufferId,
    start_position: usize,
    duration: usize,
    
    // Playback control
    age: usize,
    time_warp: TimeWarpCurve,
    
    // 3D positioning with optional dynamic movement
    position: Vector3D,
    movement: Option<MovementTrajectory>, // For moving droplets in 3D space
    
    fn is_active(&self) -> bool;
    fn process(&mut self, buffers: &AudioBuffers) -> Sample3D;
}
```

### Rain Catcher System

```rust
struct RainCatcher {
    // Probability distribution for spawning droplets
    spawn_probability: Distribution,
    
    // Distribution for droplet properties
    duration_distribution: Distribution,
    position_distribution: Position3D,
    time_warp_distribution: Distribution,
    
    // Constraints
    max_droplets: usize,
    
    fn update(&mut self, 
              sample_idx: usize, 
              num_active_droplets: usize, 
              buffers: &AudioBuffers) -> Option<Droplet>;
}
```

### Main Processor

```rust
struct DropletProcessor {
    // Audio buffers
    input_buffers: Vec<CircularBuffer>,
    
    // Active droplets
    droplets: Vec<Droplet>,
    
    // Droplet generators
    rain_catchers: Vec<RainCatcher>,
    
    // User parameters
    time_warp_curve_editor: TimeWarpCurveEditor,
    spatial_distribution_editor: SpatialDistributionEditor,
    
    fn process_block(&mut self, input: &AudioBlock, output: &mut AudioBlock) {
        // For each sample in the block:
        for i in 0..block_size {
            // 1. Write input to circular buffers
            
            // 2. Update rain catchers - may generate new droplets
            for catcher in &mut self.rain_catchers {
                if let Some(droplet) = catcher.update(current_sample_idx, self.droplets.len(), &self.input_buffers) {
                    self.droplets.push(droplet);
                }
            }
            
            // 3. Process all active droplets
            let mut output_3d = Sample3D::ZERO;
            for droplet in &mut self.droplets {
                if droplet.is_active() {
                    output_3d += droplet.process(&self.input_buffers);
                }
            }
            
            // 4. Apply 3D spatialization to stereo/multichannel output
            let stereo_out = self.spatialize(output_3d);
            
            // 5. Write to output
            output[i] = stereo_out;
            
            // 6. Clean up inactive droplets
            self.droplets.retain(|d| d.is_active());
        }
    }
    
    fn spatialize(&self, sample_3d: Sample3D) -> StereoSample {
        // Convert 3D audio to stereo (or multichannel if supported)
        // This could use HRTF or simpler panning/depth algorithms
    }
}
```

### UI Components

```rust
struct TimeWarpCurveEditor {
    // UI for drawing or selecting time warp curves
    
    fn get_curve(&self) -> TimeWarpCurve;
    fn update_from_ui(&mut self, ui_event: &UiEvent);
}

struct SpatialDistributionEditor {
    // UI for configuring 3D spatial distribution
    
    fn get_distribution(&self) -> Position3D;
    fn update_from_ui(&mut self, ui_event: &UiEvent);
}
```

## NIH-plug Integration

### Parameter Definitions

The plugin will expose parameters through NIH-plug's parameter system:

- Basic parameters (min/max duration, density, etc.) will use `FloatParam`
- More complex parameters (curves, distributions) will need custom serialization

### UI with nih-plug-iced

The UI will use the `nih-plug-iced` library to create:
1. Curve editors for time-warping functions
2. 3D visualizations for spatial positioning
3. Distribution editors for probability controls

### Audio Processing

The plugin will integrate with NIH-plug's audio processing APIs:
1. Buffer handling
2. Sample rate adaptation
3. Context management

## Future Extensions

### Multiple Input Streams

The architecture is designed to potentially support multiple input audio streams:
- Each stream could have its own spatial characteristics
- Droplets from different streams could be positioned in different regions of 3D space
- Streams could have different processing characteristics

### Advanced Spatialization

Future versions could incorporate more sophisticated 3D audio:
- HRTF (Head-Related Transfer Function) processing
- Ambisonics support
- Integration with spatial audio frameworks

### More Complex Distribution Controls

- Multi-modal distributions
- Contextual/reactive distributions (responding to audio characteristics)
- Machine learning for intelligent droplet generation

## Implementation Priorities

1. Core droplet processing engine
2. Basic parameters and simple distributions
3. Simple UI controls
4. 3D positioning
5. Advanced time-warping
6. Full UI with curve editors
7. Future extensions
