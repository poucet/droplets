# System Patterns: Simply Droplets

## Architecture Overview
Simply Droplets is built around a system that generates and processes "droplets" in 3D space, with time-warping characteristics:

- The plugin follows the structure dictated by the `nih-plug` framework
- A core `DropletProcessor` struct handles the main audio processing
- The `RainCatcher` system generates droplets according to configured distributions
- Individual `Droplet` instances process audio independently with their own properties
- Distribution, positioning, and time-warping are handled by specialized components
- The UI is managed by `nih-plug-iced` with custom curve and distribution editors

## Core Types

### Distribution
Defines probabilistic or deterministic distributions for various droplet properties:
```rust
struct Distribution {
    // Implementation options: parametric, drawable curve, or mathematical function
    fn sample(&self, context: &SampleContext) -> f32;
}
```

### TimeWarpCurve
Controls how time flows within a droplet:
```rust
struct TimeWarpCurve {
    // Implementation options: predefined curves, user-drawn curves, splines
    fn warp(&self, t: f32) -> f32;
}
```

### Position3D
Manages 3D positioning for droplets:
```rust
struct Position3D {
    x_distribution: Distribution,
    y_distribution: Distribution,
    z_distribution: Distribution,
    joint_distribution: Option<JointDistribution3D>,
    
    fn sample(&self, context: &SampleContext) -> Vector3D;
}
```

### Droplet
Represents an active droplet playing in 3D space:
```rust
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
    movement: Option<MovementTrajectory>,
    
    fn is_active(&self) -> bool;
    fn process(&mut self, buffers: &AudioBuffers) -> Sample3D;
}
```

### RainCatcher
Generator system that creates droplets based on configured distributions:
```rust
struct RainCatcher {
    // Probability distribution for spawning droplets
    spawn_probability: Distribution,
    
    // Distribution for droplet properties
    duration_distribution: Distribution,
    position_distribution: Position3D,
    time_warp_distribution: Distribution,
    
    fn update(&mut self, 
              sample_idx: usize, 
              num_active_droplets: usize, 
              buffers: &AudioBuffers) -> Option<Droplet>;
}
```

### DropletProcessor
Main processor handling the overall audio processing:
```rust
struct DropletProcessor {
    // Audio buffers
    input_buffers: Vec<CircularBuffer>,
    
    // Active droplets
    droplets: Vec<Droplet>,
    
    // Droplet generators
    rain_catchers: Vec<RainCatcher>,
    
    // Processing logic
    fn process_block(&mut self, input: &AudioBlock, output: &mut AudioBlock);
    fn spatialize(&self, sample_3d: Sample3D) -> StereoSample;
}
```

## Component Relationships

```mermaid
graph TD
    DAW --> PluginInstance[Simply Droplets Plugin Instance]
    PluginInstance -- Manages --> State[Plugin State]
    PluginInstance -- Manages --> Params[Parameters]
    PluginInstance -- Handles --> AudioIO[Audio Processing]
    PluginInstance -- Uses --> Processor[DropletProcessor]
    
    Processor -- Contains --> Buffers[Input Buffers]
    Processor -- Manages --> ActiveDroplets[Active Droplets]
    Processor -- Uses --> RainCatchers[Rain Catchers]
    Processor -- Produces --> Spatialization[3D Spatialization]
    
    RainCatchers -- Generate --> NewDroplets[New Droplets]
    RainCatchers -- Use --> Distributions[Property Distributions]
    
    Distributions -- Include --> DurationDist[Duration Distribution]
    Distributions -- Include --> PositionDist[Position Distribution]
    Distributions -- Include --> TimewarpDist[Time Warp Distribution]
    
    PositionDist -- Contains --> XDist[X-axis Distribution]
    PositionDist -- Contains --> YDist[Y-axis Distribution]
    PositionDist -- Contains --> ZDist[Z-axis Distribution]
    PositionDist -- May Use --> JointDist[Joint Distribution]
    
    ActiveDroplets -- Process --> Audio[Audio Samples]
    ActiveDroplets -- Apply --> TimeWarp[Time Warping]
    ActiveDroplets -- Apply --> Movement[Movement Trajectories]
    
    PluginInstance -- Communicates --> GUI[Iced GUI (nih-plug-iced)]
    GUI -- Contains --> CurveEditors[Curve Editors]
    GUI -- Contains --> DistEditors[Distribution Editors]
    GUI -- Contains --> Visualizations[3D Visualizations]
    GUI -- Modifies --> Params
```

## Key Technical Decisions

### Audio Processing
- Circular buffer for input audio storage
- Resource management to limit active droplet count (default max: 32)
- Normalization based on square root of active droplet count to manage volume
- Processing one sample at a time for fine-grained control

### 3D Positioning
- Basic stereo rendering with depth and height simulation
- Optional path for future advanced spatialization (HRTF, Ambisonics)
- Support for static positions and dynamic movement along paths

### Time Warping
- Power function approach for exponential/logarithmic warping
- Random variation (±20%) applied to time warp factor for organic sound
- Support for both direction control and curve-based warping

### Distribution System
- Flexible distribution framework supporting different implementation approaches
- Support for joint distributions with correlation between axes
- Geometric primitives for spatial distribution (sphere, path, etc.)

### Path-Based Movement
- Multiple interpolation options: Linear, B-spline, Bezier, Catmull-Rom
- Customizable speed and position curves along paths
- Optional looping for continuous movement
