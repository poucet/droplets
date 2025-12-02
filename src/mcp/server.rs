//! MCP Server tool definitions for Simply Droplets
//!
//! Exposes tools for AI to send MIDI CC and notes through plugin instances.

use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::router::tool::ToolRouter,
    handler::server::tool::ToolCallContext,
    handler::server::wrapper::Parameters,
    model::*,
    schemars, tool, tool_router,
    service::RequestContext,
};
use serde::Deserialize;

use super::bridge::{CcBridge, CcMessage, NoteMessage, PerNoteExpressionMessage};

/// MCP Server for Simply Droplets
#[derive(Clone)]
pub struct DropletsMcp {
    tool_router: ToolRouter<DropletsMcp>,
}

impl DropletsMcp {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

impl Default for DropletsMcp {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Wrapper type for instance-targeted requests
// =============================================================================

/// Wrapper for requests that target a specific plugin instance.
/// This enables future batching of multiple operations for the same instance.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct InstanceRequest<T> {
    /// Target plugin instance name or "default" for first available
    #[serde(default = "default_instance")]
    #[schemars(description = "Target plugin instance name or 'default' for first available")]
    pub instance: String,

    /// The actual request data
    #[serde(flatten)]
    pub data: T,
}

// =============================================================================
// MIDI message types (can be used standalone or in batches)
// =============================================================================

/// MIDI CC data
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CcData {
    /// MIDI channel (1-16, default: 1)
    #[serde(default = "default_channel")]
    #[schemars(description = "MIDI channel (1-16, default: 1)")]
    pub channel: u8,

    /// CC number (0-127)
    #[schemars(description = "CC number (0-127)")]
    pub cc: u8,

    /// CC value (0-127)
    #[schemars(description = "CC value (0-127)")]
    pub value: u8,
}

/// MIDI Note On data (7-bit velocity)
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct NoteOnData {
    /// MIDI channel (1-16, default: 1)
    #[serde(default = "default_channel")]
    #[schemars(description = "MIDI channel (1-16, default: 1)")]
    pub channel: u8,

    /// MIDI note number (0-127, where 60 = C4/middle C)
    #[schemars(description = "MIDI note number (0-127, where 60 = C4/middle C)")]
    pub note: u8,

    /// Note velocity (1-127, default: 100)
    #[serde(default = "default_velocity")]
    #[schemars(description = "Note velocity (1-127, default: 100)")]
    pub velocity: u8,
}

/// MIDI 2.0 Note On data with 16-bit velocity
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct NoteOnHiresData {
    /// MIDI channel (1-16, default: 1)
    #[serde(default = "default_channel")]
    #[schemars(description = "MIDI channel (1-16, default: 1)")]
    pub channel: u8,

    /// MIDI note number (0-127, where 60 = C4/middle C)
    #[schemars(description = "MIDI note number (0-127, where 60 = C4/middle C)")]
    pub note: u8,

    /// 16-bit velocity (1-65535, default: 32768). MIDI 2.0 high-resolution.
    #[serde(default = "default_velocity_16bit")]
    #[schemars(description = "16-bit velocity (1-65535, default: 32768). MIDI 2.0 high-resolution.")]
    pub velocity: u16,
}

/// MIDI Note Off data
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct NoteOffData {
    /// MIDI channel (1-16, default: 1)
    #[serde(default = "default_channel")]
    #[schemars(description = "MIDI channel (1-16, default: 1)")]
    pub channel: u8,

    /// MIDI note number (0-127, where 60 = C4/middle C)
    #[schemars(description = "MIDI note number (0-127, where 60 = C4/middle C)")]
    pub note: u8,

    /// Release velocity (0-127, default: 0)
    #[serde(default)]
    #[schemars(description = "Release velocity (0-127, default: 0)")]
    pub velocity: u8,
}

// =============================================================================
// Slot/parameter types
// =============================================================================

/// Set parameter slot value data
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SetParamData {
    /// Slot index (0-15)
    #[schemars(description = "Parameter slot index (0-15)")]
    pub slot: usize,

    /// Value (0.0-1.0 normalized)
    #[schemars(description = "Parameter value (0.0-1.0 normalized)")]
    pub value: f64,
}

/// Rename parameter slot data
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RenameSlotData {
    /// Slot index (0-15)
    #[schemars(description = "Parameter slot index (0-15)")]
    pub slot: usize,

    /// New name for the slot (e.g., "Vital Filter Cutoff")
    #[schemars(description = "New name for the slot (e.g., 'Vital Filter Cutoff')")]
    pub name: String,
}

// =============================================================================
// Per-note expression types (MIDI 2.0 only)
// =============================================================================

/// Per-note pitch bend data (MIDI 2.0)
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PerNotePitchBendData {
    /// MIDI channel (1-16, default: 1)
    #[serde(default = "default_channel")]
    #[schemars(description = "MIDI channel (1-16, default: 1)")]
    pub channel: u8,

    /// MIDI note number to bend (0-127, where 60 = C4/middle C)
    #[schemars(description = "MIDI note number to bend (0-127, where 60 = C4/middle C)")]
    pub note: u8,

    /// Pitch bend in semitones (-64.0 to +64.0, 0 = no bend)
    #[schemars(description = "Pitch bend in semitones (-64.0 to +64.0, 0 = no bend)")]
    pub semitones: f32,
}

/// Per-note pressure/aftertouch data (MIDI 2.0)
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PerNotePressureData {
    /// MIDI channel (1-16, default: 1)
    #[serde(default = "default_channel")]
    #[schemars(description = "MIDI channel (1-16, default: 1)")]
    pub channel: u8,

    /// MIDI note number (0-127, where 60 = C4/middle C)
    #[schemars(description = "MIDI note number (0-127, where 60 = C4/middle C)")]
    pub note: u8,

    /// Pressure value (0.0-1.0 normalized)
    #[schemars(description = "Pressure value (0.0-1.0 normalized)")]
    pub pressure: f32,
}

/// Per-note controller data (MIDI 2.0)
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PerNoteControllerData {
    /// MIDI channel (1-16, default: 1)
    #[serde(default = "default_channel")]
    #[schemars(description = "MIDI channel (1-16, default: 1)")]
    pub channel: u8,

    /// MIDI note number (0-127, where 60 = C4/middle C)
    #[schemars(description = "MIDI note number (0-127, where 60 = C4/middle C)")]
    pub note: u8,

    /// Controller index (0-255)
    #[schemars(description = "Controller index (0-255)")]
    pub index: u8,

    /// Controller value (0.0-1.0 normalized)
    #[schemars(description = "Controller value (0.0-1.0 normalized)")]
    pub value: f32,
}

/// Per-note management data (MIDI 2.0)
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PerNoteManagementData {
    /// MIDI channel (1-16, default: 1)
    #[serde(default = "default_channel")]
    #[schemars(description = "MIDI channel (1-16, default: 1)")]
    pub channel: u8,

    /// MIDI note number (0-127, where 60 = C4/middle C)
    #[schemars(description = "MIDI note number (0-127, where 60 = C4/middle C)")]
    pub note: u8,

    /// Detach this note from prior note-on (default: false)
    #[serde(default)]
    #[schemars(description = "Detach this note from prior note-on")]
    pub detach: bool,

    /// Reset all controllers on this note (default: false)
    #[serde(default)]
    #[schemars(description = "Reset all controllers on this note")]
    pub reset: bool,
}

// =============================================================================
// Instance management types (these don't use the wrapper since instance is the subject)
// =============================================================================

/// Request to rename an instance
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RenameInstanceRequest {
    /// Current instance name or ID
    #[schemars(description = "Current instance name or ID")]
    pub instance: String,

    /// New display name for the instance
    #[schemars(description = "New display name for the instance")]
    pub name: String,
}

// =============================================================================
// Type aliases for the MCP tool interface
// =============================================================================

pub type SendCcRequest = InstanceRequest<CcData>;
pub type SendNoteOnRequest = InstanceRequest<NoteOnData>;
pub type SendNoteOnHiresRequest = InstanceRequest<NoteOnHiresData>;
pub type SendNoteOffRequest = InstanceRequest<NoteOffData>;
pub type SetParamRequest = InstanceRequest<SetParamData>;
pub type RenameSlotRequest = InstanceRequest<RenameSlotData>;

// Per-note expression request types (MIDI 2.0)
pub type PerNotePitchBendRequest = InstanceRequest<PerNotePitchBendData>;
pub type PerNotePressureRequest = InstanceRequest<PerNotePressureData>;
pub type PerNoteControllerRequest = InstanceRequest<PerNoteControllerData>;
pub type PerNoteManagementRequest = InstanceRequest<PerNoteManagementData>;

/// Request to get slots for an instance (no additional data needed)
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetSlotsRequest {
    /// Target plugin instance name or "default" for first available
    #[serde(default = "default_instance")]
    #[schemars(description = "Target plugin instance name or 'default' for first available")]
    pub instance: String,
}

// =============================================================================
// Default value functions
// =============================================================================

fn default_instance() -> String {
    "default".to_string()
}

fn default_channel() -> u8 {
    1
}

fn default_velocity() -> u8 {
    100
}

fn default_velocity_16bit() -> u16 {
    32768 // Mid-point of 16-bit range
}

#[tool_router]
impl DropletsMcp {
    /// Send a MIDI CC message through a plugin instance.
    #[tool(description = "Send a MIDI CC message through a Simply Droplets plugin instance. The plugin outputs MIDI CC that your DAW can route to control any other plugin's parameters. Use 'default' for instance to target the first available plugin.")]
    fn send_cc(&self, Parameters(req): Parameters<SendCcRequest>) -> Result<CallToolResult, McpError> {
        let msg = CcMessage::new(
            req.data.channel.saturating_sub(1).min(15),
            req.data.cc.min(127),
            req.data.value.min(127),
        );

        let result = match CcBridge::send(&req.instance, msg) {
            Ok(()) => format!(
                "Sent CC{} = {} on channel {} via instance '{}'",
                req.data.cc, req.data.value, req.data.channel, req.instance
            ),
            Err(e) => format!("Error: {}", e),
        };
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    /// List all connected Simply Droplets plugin instances.
    #[tool(description = "List all connected Simply Droplets plugin instances. Returns the names that can be used with send_cc.")]
    fn list_instances(&self) -> Result<CallToolResult, McpError> {
        let instances = CcBridge::list_instances();

        let result = if instances.is_empty() {
            "No plugin instances connected. Load Simply Droplets in your DAW first.".to_string()
        } else {
            serde_json::to_string_pretty(&instances)
                .unwrap_or_else(|_| format!("{:?}", instances))
        };
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    /// Rename a plugin instance for easier reference.
    #[tool(description = "Rename a Simply Droplets instance for easier reference. Use names like 'bass', 'pad', 'lead' to make targeting clearer.")]
    fn set_instance_name(&self, Parameters(req): Parameters<RenameInstanceRequest>) -> Result<CallToolResult, McpError> {
        let result = match CcBridge::rename(&req.instance, &req.name) {
            Ok(old_name) => format!("Renamed '{}' to '{}'", old_name, req.name),
            Err(e) => format!("Error: {}", e),
        };
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    /// Get recent MIDI activity for debugging/visualization.
    #[tool(description = "Get recent MIDI activity (CC, notes, and per-note expressions) sent through Simply Droplets instances. Useful for debugging and seeing what was sent.")]
    fn get_activity(&self) -> Result<CallToolResult, McpError> {
        let activity = CcBridge::recent_activity();

        let result = if activity.is_empty() {
            "No recent activity.".to_string()
        } else {
            let formatted: Vec<String> = activity
                .iter()
                .map(|e| {
                    if let Some(cc) = e.cc {
                        format!(
                            "[{}] {} -> CC{} = {} (ch{})",
                            e.timestamp_ms, e.instance, cc, e.value, e.channel + 1
                        )
                    } else if let Some(expr_type) = &e.expression_type {
                        // Per-note expression
                        let note = e.note.unwrap_or(0);
                        format!(
                            "[{}] {} -> {} note={} val={} (ch{})",
                            e.timestamp_ms, e.instance, expr_type, note, e.value, e.channel + 1
                        )
                    } else if let Some(note) = e.note {
                        let note_type = if e.is_note_on.unwrap_or(false) { "NoteOn" } else { "NoteOff" };
                        format!(
                            "[{}] {} -> {} {} vel={} (ch{})",
                            e.timestamp_ms, e.instance, note_type, note, e.value, e.channel + 1
                        )
                    } else {
                        format!("[{}] {} -> unknown event", e.timestamp_ms, e.instance)
                    }
                })
                .collect();

            formatted.join("\n")
        };
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    /// Set a parameter slot value for DAW automation/modulation.
    #[tool(description = "Set a parameter slot value (0.0-1.0). These slots are automatable parameters that can be mapped via your DAW's modulation system (Ableton LFOs, Bitwig modulators) to control any plugin on the same track.")]
    fn set_param(&self, Parameters(req): Parameters<SetParamRequest>) -> Result<CallToolResult, McpError> {
        let result = match CcBridge::set_param(&req.instance, req.data.slot, req.data.value) {
            Ok(()) => format!(
                "Set slot {} = {:.2} ({:.0}%) on instance '{}'",
                req.data.slot, req.data.value, req.data.value * 100.0, req.instance
            ),
            Err(e) => format!("Error: {}", e),
        };
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    /// Rename a parameter slot for easier identification.
    #[tool(description = "Rename a parameter slot to describe what it controls. For example, if slot 0 is mapped to 'Vital Filter Cutoff' in your DAW, rename it so both the UI and AI can identify it clearly.")]
    fn rename_slot(&self, Parameters(req): Parameters<RenameSlotRequest>) -> Result<CallToolResult, McpError> {
        let result = match CcBridge::rename_slot(&req.instance, req.data.slot, &req.data.name) {
            Ok(old_name) => format!(
                "Renamed slot {} from '{}' to '{}' on instance '{}'",
                req.data.slot, old_name, req.data.name, req.instance
            ),
            Err(e) => format!("Error: {}", e),
        };
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    /// List all parameter slots for an instance with their names, CC mappings, and current values.
    #[tool(description = "List all parameter slots for an instance with their names, CC mappings, and values. Shows which slots are mapped to MIDI CC and can output CC when set.")]
    fn list_slots(&self, Parameters(req): Parameters<GetSlotsRequest>) -> Result<CallToolResult, McpError> {
        let result = match CcBridge::get_slots(&req.instance) {
            Ok(slots) => {
                if slots.is_empty() {
                    "No slots available.".to_string()
                } else {
                    let formatted: Vec<String> = slots
                        .iter()
                        .map(|s| {
                            let cc_info = match s.cc {
                                Some(cc) => format!("CC{} ch{}", cc, s.channel + 1),
                                None => "unmapped".to_string(),
                            };
                            format!("  [{}] {} ({}) = {:.2} ({:.0}%)", s.index, s.name, cc_info, s.value, s.value * 100.0)
                        })
                        .collect();
                    format!("Parameter slots on '{}':\n{}", req.instance, formatted.join("\n"))
                }
            }
            Err(e) => format!("Error: {}", e),
        };
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    /// Send a MIDI Note On message through a plugin instance.
    #[tool(description = "Send a MIDI Note On message through a Simply Droplets plugin instance. The plugin outputs MIDI notes that your DAW can route to trigger synths, samplers, or other instruments. Note 60 = C4 (middle C). Uses 7-bit velocity (0-127).")]
    fn send_note_on(&self, Parameters(req): Parameters<SendNoteOnRequest>) -> Result<CallToolResult, McpError> {
        let msg = NoteMessage::new(
            req.data.channel.saturating_sub(1).min(15),
            req.data.note.min(127),
            req.data.velocity.clamp(1, 127),
            true,
        );

        let result = match CcBridge::send_note(&req.instance, msg) {
            Ok(()) => format!(
                "Sent Note On {} vel={} on channel {} via instance '{}'",
                req.data.note, req.data.velocity, req.data.channel, req.instance
            ),
            Err(e) => format!("Error: {}", e),
        };
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    /// Send a MIDI 2.0 Note On with 16-bit high-resolution velocity.
    #[tool(description = "Send a MIDI 2.0 Note On message with 16-bit velocity (0-65535) for high-resolution dynamics. Use this when you need finer control than standard 7-bit velocity provides. Note 60 = C4 (middle C).")]
    fn send_note_on_hires(&self, Parameters(req): Parameters<SendNoteOnHiresRequest>) -> Result<CallToolResult, McpError> {
        let msg = NoteMessage::new_hires(
            req.data.channel.saturating_sub(1).min(15),
            req.data.note.min(127),
            req.data.velocity.max(1), // Ensure at least 1 for Note On
            true,
        );

        let result = match CcBridge::send_note(&req.instance, msg) {
            Ok(()) => format!(
                "Sent MIDI 2.0 Note On {} vel={} (16-bit) on channel {} via instance '{}'",
                req.data.note, req.data.velocity, req.data.channel, req.instance
            ),
            Err(e) => format!("Error: {}", e),
        };
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    /// Send a MIDI Note Off message through a plugin instance.
    #[tool(description = "Send a MIDI Note Off message through a Simply Droplets plugin instance. Use this to release a note that was previously triggered with send_note_on.")]
    fn send_note_off(&self, Parameters(req): Parameters<SendNoteOffRequest>) -> Result<CallToolResult, McpError> {
        let msg = NoteMessage::new(
            req.data.channel.saturating_sub(1).min(15),
            req.data.note.min(127),
            req.data.velocity.min(127),
            false,
        );

        let result = match CcBridge::send_note(&req.instance, msg) {
            Ok(()) => format!(
                "Sent Note Off {} on channel {} via instance '{}'",
                req.data.note, req.data.channel, req.instance
            ),
            Err(e) => format!("Error: {}", e),
        };
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    // =========================================================================
    // MIDI 2.0 Per-Note Expression Tools
    // =========================================================================

    /// Send per-note pitch bend (MIDI 2.0 only).
    #[tool(description = "Send MIDI 2.0 per-note pitch bend. Unlike channel pitch bend, this affects only a specific note that is currently playing. Range is -64 to +64 semitones. Requires MIDI 2.0 compatible host/instrument.")]
    fn send_per_note_pitch_bend(&self, Parameters(req): Parameters<PerNotePitchBendRequest>) -> Result<CallToolResult, McpError> {
        let msg = PerNoteExpressionMessage::pitch_bend_semitones(
            req.data.channel.saturating_sub(1).min(15),
            req.data.note.min(127),
            req.data.semitones.clamp(-64.0, 64.0),
        );

        let result = match CcBridge::send_per_note_expression(&req.instance, msg) {
            Ok(()) => format!(
                "Sent per-note pitch bend {:.2} semitones on note {} ch{} via '{}'",
                req.data.semitones, req.data.note, req.data.channel, req.instance
            ),
            Err(e) => format!("Error: {}", e),
        };
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    /// Send per-note pressure/aftertouch (MIDI 2.0 only).
    #[tool(description = "Send MIDI 2.0 per-note pressure (polyphonic aftertouch). Unlike channel aftertouch, this affects only a specific note. Use 0.0-1.0 for pressure intensity. Requires MIDI 2.0 compatible host/instrument.")]
    fn send_per_note_pressure(&self, Parameters(req): Parameters<PerNotePressureRequest>) -> Result<CallToolResult, McpError> {
        let msg = PerNoteExpressionMessage::pressure_normalized(
            req.data.channel.saturating_sub(1).min(15),
            req.data.note.min(127),
            req.data.pressure.clamp(0.0, 1.0),
        );

        let result = match CcBridge::send_per_note_expression(&req.instance, msg) {
            Ok(()) => format!(
                "Sent per-note pressure {:.2} on note {} ch{} via '{}'",
                req.data.pressure, req.data.note, req.data.channel, req.instance
            ),
            Err(e) => format!("Error: {}", e),
        };
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    /// Send per-note registered controller (MIDI 2.0 only).
    #[tool(description = "Send MIDI 2.0 registered per-note controller. These are standardized controllers that apply to individual notes. Index 7 is per-note pressure (use send_per_note_pressure instead). Requires MIDI 2.0 compatible host/instrument.")]
    fn send_per_note_registered_controller(&self, Parameters(req): Parameters<PerNoteControllerRequest>) -> Result<CallToolResult, McpError> {
        let value_32bit = (req.data.value.clamp(0.0, 1.0) * (u32::MAX as f32)) as u32;
        let msg = PerNoteExpressionMessage::registered_controller(
            req.data.channel.saturating_sub(1).min(15),
            req.data.note.min(127),
            req.data.index,
            value_32bit,
        );

        let result = match CcBridge::send_per_note_expression(&req.instance, msg) {
            Ok(()) => format!(
                "Sent per-note registered controller {} = {:.2} on note {} ch{} via '{}'",
                req.data.index, req.data.value, req.data.note, req.data.channel, req.instance
            ),
            Err(e) => format!("Error: {}", e),
        };
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    /// Send per-note assignable controller (MIDI 2.0 only).
    #[tool(description = "Send MIDI 2.0 assignable per-note controller. These are custom controllers that apply to individual notes, similar to registered controllers but vendor/implementation specific. Requires MIDI 2.0 compatible host/instrument.")]
    fn send_per_note_assignable_controller(&self, Parameters(req): Parameters<PerNoteControllerRequest>) -> Result<CallToolResult, McpError> {
        let value_32bit = (req.data.value.clamp(0.0, 1.0) * (u32::MAX as f32)) as u32;
        let msg = PerNoteExpressionMessage::assignable_controller(
            req.data.channel.saturating_sub(1).min(15),
            req.data.note.min(127),
            req.data.index,
            value_32bit,
        );

        let result = match CcBridge::send_per_note_expression(&req.instance, msg) {
            Ok(()) => format!(
                "Sent per-note assignable controller {} = {:.2} on note {} ch{} via '{}'",
                req.data.index, req.data.value, req.data.note, req.data.channel, req.instance
            ),
            Err(e) => format!("Error: {}", e),
        };
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    /// Send per-note management message (MIDI 2.0 only).
    #[tool(description = "Send MIDI 2.0 per-note management message. Use detach=true to separate this note from its Note On (for legato/portamento). Use reset=true to reset all controllers on this note. Requires MIDI 2.0 compatible host/instrument.")]
    fn send_per_note_management(&self, Parameters(req): Parameters<PerNoteManagementRequest>) -> Result<CallToolResult, McpError> {
        let msg = PerNoteExpressionMessage::management(
            req.data.channel.saturating_sub(1).min(15),
            req.data.note.min(127),
            req.data.detach,
            req.data.reset,
        );

        let flags_desc = match (req.data.detach, req.data.reset) {
            (true, true) => "detach+reset",
            (true, false) => "detach",
            (false, true) => "reset",
            (false, false) => "no-op",
        };

        let result = match CcBridge::send_per_note_expression(&req.instance, msg) {
            Ok(()) => format!(
                "Sent per-note management ({}) on note {} ch{} via '{}'",
                flags_desc, req.data.note, req.data.channel, req.instance
            ),
            Err(e) => format!("Error: {}", e),
        };
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }
}

impl ServerHandler for DropletsMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "Simply Droplets MCP Server - AI-controlled MIDI 1.0/2.0 output for DAW automation.\n\n\
                 MIDI Note/CC Tools:\n\
                 - send_note_on(note, velocity, channel): MIDI 1.0 Note On (7-bit velocity)\n\
                 - send_note_on_hires(note, velocity, channel): MIDI 2.0 Note On (16-bit velocity)\n\
                 - send_note_off(note, channel): Send MIDI Note Off\n\
                 - send_cc(cc, value, channel): Send MIDI CC message\n\n\
                 MIDI 2.0 Per-Note Expression Tools:\n\
                 - send_per_note_pitch_bend(note, semitones, channel): Pitch bend individual notes (-64 to +64 semitones)\n\
                 - send_per_note_pressure(note, pressure, channel): Per-note aftertouch (0.0-1.0)\n\
                 - send_per_note_registered_controller(note, index, value, channel): Registered per-note controller\n\
                 - send_per_note_assignable_controller(note, index, value, channel): Assignable per-note controller\n\
                 - send_per_note_management(note, detach, reset, channel): Note management (detach/reset)\n\n\
                 Instance Tools:\n\
                 - list_instances(): See connected plugin instances\n\
                 - list_slots(): See slots with names, CC mappings, and values\n\
                 - set_param(slot, value): Set slot value (0.0-1.0)\n\
                 - rename_slot(slot, name): Label a slot\n\
                 - get_activity(): See recent MIDI activity\n\n\
                 Note: Per-note expressions require MIDI 2.0 compatible host/instruments."
                    .to_string(),
            ),
        }
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        log::debug!("MCP: list_tools called");
        let tools = self.tool_router.list_all();
        log::info!("MCP: Returning {} tools", tools.len());
        Ok(ListToolsResult {
            tools,
            next_cursor: None,
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParam,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        log::info!("MCP: call_tool '{}' with args: {:?}", request.name, request.arguments);
        let tool_context = ToolCallContext::new(self, request, context);
        self.tool_router.call(tool_context).await
    }
}
