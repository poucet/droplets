# Active Context: Granular VST

## Current Focus
- Basic project structure is now set up and ready for VST/CLAP plugin development
- The project compiles successfully without warnings
- We have a minimal working plugin skeleton with parameter definitions for basic granular functionality

## Recent Changes
- Created initial Memory Bank files for project documentation
- Set up the Rust project with cargo
- Added nih-plug and nih-plug-iced dependencies
- Implemented basic plugin structure with parameter definitions
- Set up VST3 and CLAP plugin exports
- Configured bundling system with xtask
- Added placeholder for audio buffer management

## Next Steps
1. Implement the audio buffer capture mechanism to store incoming audio for granulation
2. Create grain generation and scheduling logic
3. Implement the granular synthesis algorithm in the process function
4. Set up the Iced-based GUI
5. Add more parameters specific to granular synthesis
6. Test the plugin in different DAWs and fine-tune performance

## Decision Points
- We'll need to decide on the specific granular synthesis approach (e.g., overlap-add, windowing function)
- Consider whether to implement traditional or more experimental granular controls
- Determine the best buffer size and management strategy for real-time performance
