# Simply Droplets - Architecture Design Document

## Overview

Simply Droplets is a 3D droplet-based granular synthesis audio plugin built in Rust using the `nih-plug` framework. Unlike traditional granular synthesis which focuses on "grains," this plugin uses the concept of "droplets" that can be positioned in 3D space and have configurable time-warping characteristics.

## Core Concepts

### Droplets vs. Grains

Traditional granular synthesis uses "grains" - small snippets of audio that are played back with various modifications. Simply Droplets extends this concept with "droplets":

- Droplets capture a slice of input audio (like grains)
- Droplets have a random duration (configurable between 20-250ms by default)
- Droplets are positioned in 3D space 
- Droplets have a time-warping characteristic that affects playback

### 3D Audio Positioning

Each droplet exists in a 3D audio space with:
- X-axis: Left-right panning
- Y-axis: Front-back (depth)
- Z-axis: Up-down (height)

In stereo output, this 3D positioning is collapsed to create a sense of space and dimension.

### Time Warping

Each droplet can have its playback time-warped according to a configurable curve. This allows for:
- Speed variations
- Non-linear playback (accelerations, decelerations)
- Creative time-based effects

### Rain Catcher System

The "Rain Catcher" is responsible for creating new droplets based on probability distributions. This system:
- Decides when to spawn new droplets
- Determines droplet characteristics (duration, position, time-warp)
- Manages overall droplet density

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
    x_distribution: Distribution,
    y_distribution: Distribution,
    z_distribution: Distribution,
    
    fn sample(&self, context: &SampleContext) -> Vector3D;
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
    
    // 3D positioning
    position: Vector3D,
    
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
