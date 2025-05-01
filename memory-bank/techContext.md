# Technical Context: Simply Droplets

## Technologies Used
- **Language:** Rust (latest stable)
- **Plugin Framework:** `nih-plug` for VST3/CLAP integration
- **GUI Toolkit:** `iced` (via `nih-plug-iced`) for curve editors and 3D visualization
- **Build System:** `cargo` with custom `xtask` for bundling
- **Target OS:** macOS (initial target)
- **Audio Processing:** Custom droplet-based granular synthesis engine
- **3D Audio:** Custom spatial rendering system with stereo output

## Development Setup
- Standard Rust toolchain (`rustup`, `cargo`)
- A Digital Audio Workstation (DAW) capable of loading VST3 or CLAP plugins on macOS (e.g., Reaper, Bitwig Studio, Ableton Live)
- `nih-plug`'s `xtask` for bundling the plugin
- Testing environment for real-time audio processing performance

## Technical Constraints
- Real-time audio processing requirements demand efficient code
  - Each sample must be processed within its time slice
  - Droplet management must scale efficiently with up to 32 concurrent droplets
  - Buffer operations need to be optimized for minimal overhead
- GUI updates must be handled carefully to avoid impacting the audio thread
  - Curve editors for time-warping and distributions need efficient serialization
  - 3D visualizations must be lightweight enough not to impact performance
- Memory management considerations for real-time audio
  - Avoiding allocations in the audio thread
  - Efficient circular buffer implementation for audio storage
  - Pre-allocation of resources when possible
- Compatibility with `nih-plug`'s API and conventions
- VST3 plugins built with `nih-plug`'s default bindings are subject to GPLv3 licensing due to the `vst3-sys` crate. CLAP plugins do not have this restriction.

## Dependencies
- **Core Framework:**
  - `nih-plug` (audio plugin framework)
  - `nih-plug-iced` (Iced GUI integration)
  - `iced` (GUI toolkit)

- **Audio Processing:**
  - Custom droplet-based granular synthesis implementation
  - Circular buffer implementation for efficient audio storage
  - Time-warping algorithms for non-linear playback

- **Mathematics and DSP:**
  - Vector mathematics for 3D positioning
  - Interpolation algorithms (linear, cubic, spline-based)
  - Curve implementations (exponential, logarithmic, power functions)
  - Random number generation for stochastic processes

- **Future Extensions (Optional):**
  - HRTF processing for more realistic 3D audio (potentially)
  - Ambisonics encoding for multi-speaker reproduction (potentially)
  - More advanced DSP for spectral processing (potentially)

## Technical Implementation Considerations

### Buffer Management
- Circular buffer implementation for efficient sample access
- Memory optimization to avoid audio thread allocations
- Optimal read/write patterns for concurrent droplet processing

### Distribution System
- Flexible implementation supporting different approaches:
  - Simple parametric distributions (min, max, curve)
  - Curve-based distributions (series of points with interpolation)
  - Mathematical function-based distributions

### Time Warping
- Power function approach for exponential/logarithmic warping
- Support for both direction control and curve-based warping
- Optimization for real-time sample-level processing

### 3D Positioning and Movement
- Vector mathematics for position calculations
- Path interpolation algorithms (B-spline, Bezier, Catmull-Rom)
- Stereo rendering techniques for 3D->2D conversion
  - Pan: gain adjustments between channels
  - Depth: volume reduction (up to 70% at maximum depth)
  - Height: creative stereo mixing techniques

### Parameter System
- Integration with nih-plug's parameter system
- Custom parameter types for complex data (curves, distributions)
- Efficient parameter serialization/deserialization
