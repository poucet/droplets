# Active Context: Simply Droplets

## Current Focus
- Defined core architecture for Simply Droplets, a 3D droplet-based granular synthesis VST/CLAP plugin
- Created design document outlining the key components and their interactions
- Established conceptual differences between traditional "grains" and our "droplets" approach
- Planning implementation of core droplet processing engine and 3D audio positioning

## Recent Changes
- Renamed project from "Granular VST" to "Simply Droplets" to reflect the distinctive approach
- Created architecture design document in memory-bank/design/architecture.md
- Defined key components: Distribution, TimeWarpCurve, Position3D, Droplet, RainCatcher, and DropletProcessor
- Outlined 3D audio positioning concept with x (left-right), y (front-back), and z (height) axes
- Established time-warping approach for non-linear playback of droplets

## Next Steps
1. Review and refine the architecture design document
2. Implement core types (Distribution, TimeWarpCurve, etc.)
3. Build basic droplet processing engine
4. Implement the Rain Catcher system for generating droplets
5. Add 3D positioning and time-warping capabilities
6. Integrate with nih-plug's parameter system
7. Develop basic UI controls using nih-plug-iced
8. Create advanced curve editors for time-warping and spatial distributions

## Decision Points
- Need to decide on the specific implementation approach for distributions (parametric vs. curve-based)
- Consider the most efficient way to handle 3D audio positioning with stereo output
- Determine how to structure the buffer management for optimal performance
- Evaluate the complexity/feasibility of user-drawable curves in the UI
- Consider how to make the plugin performant while managing potentially many droplets
- Decide on initial parameter set for MVP vs. more advanced features for later versions
