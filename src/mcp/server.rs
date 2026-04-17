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

use super::bridge::{CcBridge, CcMessage, NoteMessage, PerNoteExpressionMessage};
use super::requests::{
    CancelFugueRequest, CancelFuguesByTagRequest, FugueContent, GetSlotsRequest,
    PerNoteControllerRequest, PerNoteManagementRequest, PerNotePitchBendRequest,
    PerNotePressureRequest, QueueFugueRequest, RenameInstanceRequest, RenameSlotRequest,
    SendCcRequest, SendNoteOffRequest, SendNoteOnHiresRequest, SendNoteOnRequest, SetParamRequest,
    expand_per_note_points, parse_interpolation_mode,
};
use crate::fugue::{
    CancelMode, FugueBridge, FugueDefinition, FugueEvent, InterpolationMode, LoopMode,
    QuantizeMode, TimedFugueEvent,
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

    // =========================================================================
    // Fugue Sequencing Tools
    // =========================================================================

    /// Queue one or more fugues for transport-synchronized playback.
    #[tool(description = "Queue one or more fugues for transport-synchronized playback. Each fugue is atomic - use separate fugues for notes, CC automation, and per-note expression so they can be updated independently.\n\nFugue types:\n- 'notes': MIDI notes with auto note-off. Each note has beat, note (0-127), duration (beats).\n- 'cc': CC automation. Points are [beat, value] pairs (values 0-127). Interpolation: 'linear' (default), 'exp', 'log', or 'none'.\n- 'per_note_pitch_bend': MIDI 2.0 per-note pitch bend over time. Bends a single held note; points are [beat, semitones] or [beat, semitones, curve] tuples (-64.0 to +64.0). Requires a concurrent 'notes' fugue holding the target note on the same channel.\n- 'per_note_pressure': MIDI 2.0 per-note pressure/aftertouch over time. Points are [beat, pressure] or [beat, pressure, curve] tuples (0.0-1.0).\n\nPer-note trajectories support per-segment curves — each point's optional curve controls interpolation for the segment arriving at it (ignored on first point). Example crescendo-then-release: [[0,0],[2,1,\"exp\"],[4,0,\"log\"]].\n\nUse tags + cancel_mode for layering: tag:'melody' with cancel_mode:'tag:melody' replaces the previous melody while leaving other fugues untouched.")]
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
                    for note in notes {
                        let channel = note.channel.map(|c| c.saturating_sub(1).min(15)).unwrap_or(fugue_channel);
                        let velocity = note.velocity.unwrap_or(100).clamp(1, 127);

                        // Note on
                        events.push(TimedFugueEvent::new(
                            note.beat,
                            FugueEvent::NoteOn {
                                channel,
                                note: note.note.min(127),
                                velocity,
                            },
                        ));

                        // Auto-generated note off
                        events.push(TimedFugueEvent::new(
                            note.beat + note.duration,
                            FugueEvent::NoteOff {
                                channel,
                                note: note.note.min(127),
                            },
                        ));
                    }
                }
                FugueContent::Cc { cc, points, interpolation } => {
                    // CC has audio-thread ramps, so the mode is stored on the
                    // fugue definition and interpolation happens per-sample.
                    let cc_num = (*cc).min(127);
                    for point in points {
                        let beat = point[0];
                        let value = (point[1] as u8).min(127);
                        events.push(TimedFugueEvent::new(
                            beat,
                            FugueEvent::Cc { channel: fugue_channel, cc: cc_num, value },
                        ));
                    }
                    cc_interpolation = parse_interpolation_mode(interpolation.as_deref());
                }
                FugueContent::PerNotePitchBend { note, points } => {
                    // Per-note has no audio-thread ramp path yet, so expand
                    // per-segment curves into dense discrete events at parse
                    // time. The scheduler dispatches each as an Instant event.
                    let n = (*note).min(127);
                    expand_per_note_points(points, |beat, value| {
                        let semitones = (value as f32).clamp(-64.0, 64.0);
                        events.push(TimedFugueEvent::new(
                            beat,
                            FugueEvent::PerNotePitchBend {
                                channel: fugue_channel, note: n, semitones,
                            },
                        ));
                    });
                }
                FugueContent::PerNotePressure { note, points } => {
                    let n = (*note).min(127);
                    expand_per_note_points(points, |beat, value| {
                        let pressure = (value as f32).clamp(0.0, 1.0);
                        events.push(TimedFugueEvent::new(
                            beat,
                            FugueEvent::PerNotePressure {
                                channel: fugue_channel, note: n, pressure,
                            },
                        ));
                    });
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
            Ok(()) => format!("Cancelled fugue {}", req.data.id),
            Err(e) => format!("Error: {}", e),
        };
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    /// Cancel all fugues with a specific tag.
    #[tool(description = "Cancel all fugues with a matching tag. Sends note-offs for any active notes. Use this to stop all instances of a pattern, like all 'melody' fugues.")]
    fn cancel_fugues_by_tag(&self, Parameters(req): Parameters<CancelFuguesByTagRequest>) -> Result<CallToolResult, McpError> {
        let result = match FugueBridge::cancel_by_tag(&req.instance, &req.data.tag) {
            Ok(()) => format!("Cancelled all fugues with tag '{}'", req.data.tag),
            Err(e) => format!("Error: {}", e),
        };
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    /// Emergency stop - clear all fugues.
    #[tool(description = "Emergency stop: cancel all fugues on a plugin instance. Sends note-offs for all active notes and clears the queue. Use when you need to stop everything immediately.")]
    fn clear_fugues(&self, Parameters(req): Parameters<GetSlotsRequest>) -> Result<CallToolResult, McpError> {
        let result = match FugueBridge::clear_all(&req.instance) {
            Ok(()) => "Cleared all fugues".to_string(),
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
                 Fugue Sequencing Tools (transport-synchronized):\n\
                 - queue_fugue(events, duration_beats, ...): Queue a musical sequence for transport-synced playback\n\
                 - list_fugues(): List active/pending fugues with IDs and status\n\
                 - cancel_fugue(id): Cancel a specific fugue by ID\n\
                 - cancel_fugues_by_tag(tag): Cancel all fugues with matching tag\n\
                 - clear_fugues(): Emergency stop - cancel all fugues\n\n\
                 Instance Tools:\n\
                 - list_instances(): See connected plugin instances\n\
                 - list_slots(): See slots with names, CC mappings, and values\n\
                 - set_param(slot, value): Set slot value (0.0-1.0)\n\
                 - get_activity(): See recent MIDI activity\n\n\
                 Note: Fugues require DAW transport to be playing."
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
