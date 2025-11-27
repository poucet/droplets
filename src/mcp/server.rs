//! MCP Server tool definitions for Simply Droplets
//!
//! Exposes tools for AI to send MIDI CC through plugin instances.

use rmcp::{
    ServerHandler,
    model::ServerInfo,
    schemars, tool,
};
use serde::Deserialize;

use super::bridge::{CcBridge, CcMessage};

/// MCP Server for Simply Droplets
#[derive(Clone)]
pub struct DropletsMcp;

impl DropletsMcp {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DropletsMcp {
    fn default() -> Self {
        Self::new()
    }
}

/// Request to send a MIDI CC message
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SendCcRequest {
    /// Target plugin instance name or "default" for first available
    #[serde(default = "default_instance")]
    #[schemars(description = "Target plugin instance name or 'default' for first available")]
    pub instance: String,

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

/// Request to rename an instance
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RenameRequest {
    /// Current instance name or ID
    #[schemars(description = "Current instance name or ID")]
    pub instance: String,

    /// New display name for the instance
    #[schemars(description = "New display name for the instance")]
    pub name: String,
}

fn default_instance() -> String {
    "default".to_string()
}

fn default_channel() -> u8 {
    1
}

#[tool(tool_box)]
impl DropletsMcp {
    /// Send a MIDI CC message through a plugin instance.
    #[tool(description = "Send a MIDI CC message through a Simply Droplets plugin instance. The plugin outputs MIDI CC that your DAW can route to control any other plugin's parameters. Use 'default' for instance to target the first available plugin.")]
    fn send_cc(&self, #[tool(aggr)] req: SendCcRequest) -> String {
        let msg = CcMessage {
            // Convert 1-16 to 0-15, clamping to valid range
            channel: req.channel.saturating_sub(1).min(15),
            cc: req.cc.min(127),
            value: req.value.min(127),
        };

        match CcBridge::send(&req.instance, msg) {
            Ok(()) => format!(
                "Sent CC{} = {} on channel {} via instance '{}'",
                req.cc, req.value, req.channel, req.instance
            ),
            Err(e) => format!("Error: {}", e),
        }
    }

    /// List all connected Simply Droplets plugin instances.
    #[tool(description = "List all connected Simply Droplets plugin instances. Returns the names that can be used with send_cc.")]
    fn list_instances(&self) -> String {
        let instances = CcBridge::list_instances();

        if instances.is_empty() {
            "No plugin instances connected. Load Simply Droplets in your DAW first.".to_string()
        } else {
            serde_json::to_string_pretty(&instances)
                .unwrap_or_else(|_| format!("{:?}", instances))
        }
    }

    /// Rename a plugin instance for easier reference.
    #[tool(description = "Rename a Simply Droplets instance for easier reference. Use names like 'bass', 'pad', 'lead' to make targeting clearer.")]
    fn set_instance_name(&self, #[tool(aggr)] req: RenameRequest) -> String {
        match CcBridge::rename(&req.instance, &req.name) {
            Ok(old_name) => format!("Renamed '{}' to '{}'", old_name, req.name),
            Err(e) => format!("Error: {}", e),
        }
    }

    /// Get recent CC activity for debugging/visualization.
    #[tool(description = "Get recent MIDI CC activity sent through Simply Droplets instances. Useful for debugging and seeing what was sent.")]
    fn get_activity(&self) -> String {
        let activity = CcBridge::recent_activity();

        if activity.is_empty() {
            "No recent activity.".to_string()
        } else {
            let formatted: Vec<String> = activity
                .iter()
                .map(|e| {
                    format!(
                        "[{}] {} -> CC{} = {} (ch{})",
                        e.timestamp_ms, e.instance, e.cc, e.value, e.channel + 1
                    )
                })
                .collect();

            formatted.join("\n")
        }
    }
}

#[tool(tool_box)]
impl ServerHandler for DropletsMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Simply Droplets MCP Server - Control any DAW plugin via MIDI CC.\n\n\
                 1. Load Simply Droplets plugin on a track in your DAW\n\
                 2. Route its MIDI output to the plugin you want to control\n\
                 3. Use send_cc() to send MIDI CC messages\n\
                 4. The target plugin receives the CC for parameter control\n\n\
                 Use list_instances() to see available plugin instances.\n\
                 Use set_instance_name() to give them memorable names."
                    .to_string(),
            ),
            ..Default::default()
        }
    }
}
