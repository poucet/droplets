# System Patterns: Granular VST

## Architecture Overview
- The plugin will follow the structure dictated by the `nih-plug` framework.
- A core `GranularPlugin` struct will hold the plugin's state and processing logic.
- A separate `Params` struct, derived using `#[derive(Params)]`, will manage user-controllable parameters.
- The UI will be handled by `nih-plug-iced`, communicating with the main plugin state.

## Key Technical Decisions (Initial)
- Use `nih-plug` for the core plugin framework (VST3/CLAP).
- Use `iced` via `nih-plug-iced` for the GUI.
- Parameter management via `nih-plug`'s declarative system.

## Component Relationships (Initial)
```mermaid
graph TD
    DAW --> PluginInstance[Granular VST Instance]
    PluginInstance -- Manages --> State[Plugin State (GranularPlugin)]
    PluginInstance -- Manages --> Params[Parameters (Params Struct)]
    PluginInstance -- Handles --> AudioIO[Audio Processing]
    PluginInstance -- Communicates --> GUI[Iced GUI (nih-plug-iced)]
    GUI -- Modifies --> Params
    State -- Contains --> AudioBuffer[Internal Audio Buffer (for grains)]
