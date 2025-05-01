# Product Context: Simply Droplets

## Problem Solved
Simply Droplets provides musicians and sound designers with a powerful tool to create immersive, spatially-rich textures and evolving soundscapes through an innovative extension of granular synthesis, integrated directly into their Digital Audio Workstation (DAW).

## How it Works (High-Level)
Unlike traditional granular synthesizers, Simply Droplets:

1. Transforms incoming audio into "droplets" (similar to grains but with extended capabilities)
2. Positions these droplets in 3D space with configurable distributions
3. Applies non-linear time-warping to control how time flows within each droplet
4. Allows droplets to move along paths through 3D space
5. The "Rain Catcher" system generates droplets according to user-configured rules

## Core Concepts

### Droplets vs. Traditional Grains
- Droplets capture slices of input audio (like grains)
- Droplets have configurable duration (5-500ms)
- Droplets exist in 3D space with position information
- Droplets use time-warping for unique playback characteristics

### 3D Audio Positioning
- X-axis: Left-right panning (stereo field)
- Y-axis: Front-back positioning (depth/distance)
- Z-axis: Up-down positioning (height)
- All three dimensions are mixed down to stereo output
- Options for static positioning or dynamic movement along paths

### Time Warping
- Non-linear manipulation of time within each droplet
- Curve-based warping (linear, exponential, logarithmic)
- Directionality control (forward, reverse, bidirectional)
- Random variation for organic sound textures

### Rain Catcher System
- Generator system that creates droplets according to configurable rules
- Controls creation timing, property assignment, and population management
- Manages droplet density and distribution of properties

## User Experience Goals
- Intuitive user interface with curve editors and 3D visualization
- Real-time manipulation of all droplet parameters
- Creative control over spatial positioning and time manipulation
- Organic, evolving textures that avoid the mechanical quality often associated with granular synthesis
- Stable performance within various DAWs on macOS
