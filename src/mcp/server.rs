//! MCP Server tool definitions for Simply Droplets.
//!
//! This file owns the [`DropletsMcp`] service and the [`tool_router`] that
//! maps MCP tool calls to handler methods. Request data types, custom
//! deserializers, and parsing helpers live in [`super::requests`] — this
//! module only wires the tools to the bridges.

use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::router::tool::ToolRouter,
    handler::server::tool::ToolCallContext,
    handler::server::wrapper::Parameters,
    model::*,
    tool, tool_router,
    service::RequestContext,
};

use super::bridge::CcBridge;
use super::requests::{
    CancelFugueRequest, CancelFuguesByTagRequest, FugueContent, GetSlotsRequest,
    QueueFugueRequest, RenameInstanceRequest, RenameSlotRequest, SetParamRequest,
    emit_cc_lane, emit_notes, emit_per_note_pitch_bend, emit_per_note_pressure,
    parse_interpolation_mode,
};
use crate::fugue::{
    CancelMode, FugueBridge, FugueDefinition, InterpolationMode, LoopMode, QuantizeMode,
    TimedFugueEvent,
};

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

/// Build the system-instructions blob for this MCP session:
/// base markdown (compile-time) + the user's custom text from the GUI
/// Settings panel (runtime, if non-empty). Kept as a separate fn so the
/// ServerHandler::get_info caller stays short and the assembly is one place.
fn build_instructions() -> String {
    const BASE: &str = include_str!("instructions.md");
    let custom = crate::fugue::settings::get_settings().custom_instructions;
    let trimmed = custom.trim();
    if trimmed.is_empty() {
        return BASE.to_string();
    }
    format!(
        "{}\n\n## Custom context (set in Settings)\n\n{}\n",
        BASE, trimmed
    )
}

#[tool_router]
impl DropletsMcp {
    /// List all connected Simply Droplets plugin instances.
    #[tool(description = "List all connected Simply Droplets plugin instances. Returns the names that can be used to target a specific instance when queueing fugues.")]
    fn list_instances(&self) -> Result<CallToolResult, McpError> {
        let instances = CcBridge::list_instances();

        let result = if instances.is_empty() {
            "No plugin instances connected. Load Simply Droplets in your DAW first.".to_string()
        } else {
            let objs: Vec<serde_json::Value> = instances
                .into_iter()
                .map(|(id, name)| serde_json::json!({ "id": id, "name": name }))
                .collect();
            serde_json::to_string_pretty(&objs)
                .unwrap_or_else(|_| "error serializing instances".to_string())
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

    // =========================================================================
    // Fugue Sequencing Tools
    // =========================================================================

    /// Queue one or more fugues for transport-synchronized playback.
    #[tool(description = "Queue fugues for transport-synced playback. See tool description for the full spec (content types, points shape, curves, worked examples).")]
    fn queue_fugue(&self, Parameters(req): Parameters<QueueFugueRequest>) -> Result<CallToolResult, McpError> {
        // Get shared defaults
        let default_quantize_str = req.data.quantize.as_deref().unwrap_or("bar");
        let default_duration = req.data.duration_beats.unwrap_or(4.0);
        let default_loop_mode_str = req.data.loop_mode.as_deref().unwrap_or("forever");

        let mut fugue_ids: Vec<u64> = Vec::new();
        let mut errors: Vec<String> = Vec::new();

        for compact in &req.data.fugues {
            // Use per-fugue overrides or fall back to shared defaults
            let quantize_str = compact.quantize.as_deref().unwrap_or(default_quantize_str);
            let duration_beats = compact.duration_beats.unwrap_or(default_duration);
            let loop_mode_str = compact.loop_mode.as_deref().unwrap_or(default_loop_mode_str);
            let fugue_channel = compact.channel.unwrap_or(1).saturating_sub(1).min(15);

            // Parse loop mode
            let loop_mode = match loop_mode_str.to_lowercase().as_str() {
                "once" => LoopMode::Once,
                "forever" => LoopMode::Forever,
                s => {
                    if let Ok(n) = s.parse::<u32>() {
                        LoopMode::Times(n)
                    } else {
                        LoopMode::Forever
                    }
                }
            };

            // Parse quantize mode
            let quantize = match quantize_str.to_lowercase().as_str() {
                "immediate" => QuantizeMode::Immediate,
                "beat" => QuantizeMode::Beat,
                "bar" => QuantizeMode::Bar,
                s if s.starts_with("bars:") => {
                    let n = s.strip_prefix("bars:").and_then(|n| n.parse().ok()).unwrap_or(1);
                    QuantizeMode::Bars(n)
                }
                _ => QuantizeMode::Bar,
            };

            // Parse cancel mode
            let cancel_mode_str = compact.cancel_mode.as_deref().unwrap_or("none");
            let cancel_mode = match cancel_mode_str.to_lowercase().as_str() {
                "none" => CancelMode::None,
                "all" => CancelMode::CancelAll,
                s if s.starts_with("tag:") => {
                    let tag = s.strip_prefix("tag:").unwrap_or("").to_string();
                    CancelMode::CancelByTag(tag)
                }
                _ => CancelMode::None,
            };

            // Convert content to events
            let mut events: Vec<TimedFugueEvent> = Vec::new();
            let mut cc_interpolation = InterpolationMode::Linear; // Default

            match &compact.content {
                FugueContent::Notes { notes } => {
                    emit_notes(notes, fugue_channel, &mut events);
                }
                FugueContent::Cc { cc, points, interpolation } => {
                    let lane_mode = parse_interpolation_mode(interpolation.as_deref());
                    cc_interpolation = lane_mode;
                    emit_cc_lane(*cc, points, lane_mode, fugue_channel, &mut events);
                }
                FugueContent::PerNotePitchBend { note, points, interpolation } => {
                    let default_mode = parse_interpolation_mode(interpolation.as_deref());
                    emit_per_note_pitch_bend(note.0, points, default_mode, fugue_channel, &mut events);
                }
                FugueContent::PerNotePressure { note, points, interpolation } => {
                    let default_mode = parse_interpolation_mode(interpolation.as_deref());
                    emit_per_note_pressure(note.0, points, default_mode, fugue_channel, &mut events);
                }
                FugueContent::Composite { notes, cc, pitch_bends, pressures } => {
                    // One fugue, multiple concerns. Each lane carries its own
                    // interpolation mode; events get curves set explicitly
                    // per-lane so different CC lanes can use different curves
                    // without fighting over the single cc_interpolation slot.
                    emit_notes(notes, fugue_channel, &mut events);
                    for lane in cc {
                        let lane_mode = parse_interpolation_mode(lane.interpolation.as_deref());
                        emit_cc_lane(lane.cc, &lane.points, lane_mode, fugue_channel, &mut events);
                    }
                    for lane in pitch_bends {
                        let lane_mode = parse_interpolation_mode(lane.interpolation.as_deref());
                        emit_per_note_pitch_bend(lane.note.0, &lane.points, lane_mode, fugue_channel, &mut events);
                    }
                    for lane in pressures {
                        let lane_mode = parse_interpolation_mode(lane.interpolation.as_deref());
                        emit_per_note_pressure(lane.note.0, &lane.points, lane_mode, fugue_channel, &mut events);
                    }
                }
            }

            // Sort events by beat offset
            events.sort_by(|a, b| a.beat_offset.partial_cmp(&b.beat_offset).unwrap_or(std::cmp::Ordering::Equal));

            // Create fugue definition
            let mut definition = FugueDefinition::new(events, duration_beats)
                .with_loop_mode(loop_mode)
                .with_quantize(quantize)
                .with_cancel_mode(cancel_mode)
                .with_cc_interpolation(cc_interpolation);

            if let Some(tag) = compact.tag.clone() {
                definition = definition.with_tag(tag);
            }

            match FugueBridge::queue(&req.instance, definition) {
                Ok(fugue_id) => fugue_ids.push(fugue_id),
                Err(e) => errors.push(e.to_string()),
            }
        }

        // Wait for the LAST queued fugue to appear in the audio thread's
        // info cache so the UI and `list_fugues` see the new state by the
        // time this tool call returns. Commands are processed in order, so
        // waiting on the last covers the whole batch — much better than
        // paying the wait per-fugue on large batches.
        if let Some(&last_id) = fugue_ids.last() {
            FugueBridge::wait_for_fugue_visible(&req.instance, last_id, 100);
        }

        let result = if errors.is_empty() {
            let json = serde_json::json!({
                "fugue_ids": fugue_ids,
                "count": fugue_ids.len(),
                "duration_beats": default_duration,
                "quantize": default_quantize_str,
                "loop_mode": default_loop_mode_str,
            });
            serde_json::to_string_pretty(&json).unwrap_or_else(|_| format!("{{\"fugue_ids\": {:?}}}", fugue_ids))
        } else {
            format!("Errors: {:?}", errors)
        };
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    /// List all active and pending fugues.
    #[tool(description = "List all active and pending fugues on a plugin instance. Shows fugue IDs, tags, loop progress, and timing information.")]
    fn list_fugues(&self, Parameters(req): Parameters<GetSlotsRequest>) -> Result<CallToolResult, McpError> {
        let result = match FugueBridge::get_fugue_info(&req.instance) {
            Ok(infos) => {
                if infos.is_empty() {
                    "No active fugues.".to_string()
                } else {
                    serde_json::to_string_pretty(&infos).unwrap_or_else(|_| format!("{:?}", infos))
                }
            }
            Err(_) => "No active fugues.".to_string(),
        };
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    /// Cancel a specific fugue by ID.
    #[tool(description = "Cancel a specific fugue by its ID. Sends note-offs for any active notes and stops playback. Use the fugue_id returned by queue_fugue.")]
    fn cancel_fugue(&self, Parameters(req): Parameters<CancelFugueRequest>) -> Result<CallToolResult, McpError> {
        let result = match FugueBridge::cancel(&req.instance, req.data.id) {
            Ok(()) => {
                // Wait for the audio thread to process the cancel so a
                // follow-up list_fugues / UI fetch reflects the removal.
                FugueBridge::wait_for_fugue_gone(&req.instance, req.data.id, 100);
                format!("Cancelled fugue {}", req.data.id)
            }
            Err(e) => format!("Error: {}", e),
        };
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    /// Cancel all fugues with a specific tag.
    #[tool(description = "Cancel all fugues with a matching tag. Sends note-offs for any active notes. Use this to stop all instances of a pattern, like all 'melody' fugues.")]
    fn cancel_fugues_by_tag(&self, Parameters(req): Parameters<CancelFuguesByTagRequest>) -> Result<CallToolResult, McpError> {
        let result = match FugueBridge::cancel_by_tag(&req.instance, &req.data.tag) {
            Ok(()) => {
                FugueBridge::wait_for_tag_gone(&req.instance, &req.data.tag, 100);
                format!("Cancelled all fugues with tag '{}'", req.data.tag)
            }
            Err(e) => format!("Error: {}", e),
        };
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    /// Emergency stop - clear all fugues.
    #[tool(description = "Emergency stop: cancel all fugues on a plugin instance. Sends note-offs for all active notes and clears the queue. Use when you need to stop everything immediately.")]
    fn clear_fugues(&self, Parameters(req): Parameters<GetSlotsRequest>) -> Result<CallToolResult, McpError> {
        let result = match FugueBridge::clear_all(&req.instance) {
            Ok(()) => {
                FugueBridge::wait_for_no_fugues(&req.instance, 100);
                "Cleared all fugues".to_string()
            }
            Err(e) => format!("Error: {}", e),
        };
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    /// Get the current DAW transport state for an instance.
    #[tool(description = "Get the current DAW transport state for an instance: {beat, tempo, playing, time_sig_numerator, is_looping, loop_start_beat, loop_end_beat}. Use this to reason about where we are in the song before scheduling — e.g. 'queue starting at bar 8, we're on bar 6 now'. In standalone mode, reports the simulated 120 BPM always-playing transport.")]
    fn get_transport(&self, Parameters(req): Parameters<GetSlotsRequest>) -> Result<CallToolResult, McpError> {
        let result = match FugueBridge::get_transport(&req.instance) {
            Ok(state) => serde_json::to_string_pretty(&state)
                .unwrap_or_else(|_| format!("{:?}", state)),
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
            // The MCP system prompt base lives in instructions.md next to this
            // file — edit there, not here. If the user has set custom
            // instructions in the GUI Settings panel, append them so the LLM
            // gets per-setup context (synth CC mappings, stylistic constraints,
            // etc.) without anyone having to rebuild.
            instructions: Some(build_instructions()),
        }
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        log::debug!("MCP: list_tools called");
        let mut tools = self.tool_router.list_all();

        // Tool descriptions that outgrow a one-liner live as markdown files
        // under src/mcp/tools/ and are pulled in at compile time. rmcp's
        // #[tool(description = ...)] attribute only accepts string literals,
        // so we stamp the real markdown content over the stub description
        // here. Add a row to the table below when you promote a tool to a
        // markdown file.
        const TOOL_DESCRIPTIONS: &[(&str, &str)] = &[
            ("queue_fugue", include_str!("tools/queue_fugue.md")),
        ];
        for tool in &mut tools {
            if let Some(&(_, desc)) =
                TOOL_DESCRIPTIONS.iter().find(|(name, _)| *name == tool.name.as_ref())
            {
                tool.description = Some(desc.into());
            }
        }

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
