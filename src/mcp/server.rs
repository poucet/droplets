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
    handler::server::wrapper::{Json, Parameters},
    model::*,
    tool, tool_router,
    service::RequestContext,
};

use std::collections::HashMap;

use super::bridge::CcBridge;
use super::requests::{
    CancelFugueRequest, CancelFuguesByTagRequest, FugueContent, GetFugueRequest, GetSlotsRequest, ImportFugueRequest,
    QueueFugueRequest, RenameInstanceRequest,
    emit_cc_lane, emit_notes, emit_per_note_pitch_bend, emit_per_note_pressure,
    parse_interpolation_mode,
};
use crate::fugue::{
    CancelMode, FugueBridge, FugueDefinition, FugueEvent, InterpolationMode, LoopMode, QuantizeMode,
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

/// Per-instance row returned by the `list_instances` MCP tool. Kept
/// deliberately minimal — id + name — since this tool exists to enable
/// the LLM to pick a target before calling `queue_fugue`. Richer
/// per-instance data (track name, primary device) comes from
/// `get_project_state`'s [`project::InstanceSummary`].
#[derive(Debug, Clone, serde::Serialize, schemars::JsonSchema)]
pub struct InstanceHandle {
    pub id: String,
    pub name: String,
}

/// `queue_fugue` result summary. `fugue_ids` are stringified to match the
/// JS-side convention (u64 doesn't round-trip through JSON numbers); the
/// redundant `count` saves the LLM from counting ids back.
#[derive(Debug, Clone, serde::Serialize, schemars::JsonSchema)]
pub struct QueueFugueSummary {
    pub fugue_ids: Vec<String>,
    pub count: usize,
    pub duration_beats: f64,
    pub quantize: String,
    pub loop_mode: String,
}

#[tool_router]
impl DropletsMcp {
    /// List all connected Simply Droplets plugin instances.
    #[tool(description = "List all connected Simply Droplets plugin instances. Returns the names that can be used to target a specific instance when queueing fugues.")]
    fn list_instances(&self) -> Json<Vec<InstanceHandle>> {
        Json(
            CcBridge::list_instances()
                .into_iter()
                .map(|(id, name)| InstanceHandle { id, name })
                .collect(),
        )
    }

    /// Rename a plugin instance for easier reference.
    #[tool(description = "Rename a Simply Droplets instance for easier reference. Use names like 'bass', 'pad', 'lead' to make targeting clearer.")]
    fn set_instance_name(
        &self,
        Parameters(req): Parameters<RenameInstanceRequest>,
    ) -> Result<String, String> {
        CcBridge::rename(&req.instance, &req.name)
            .map(|old_name| format!("Renamed '{}' to '{}'", old_name, req.name))
            .map_err(|e| e.to_string())
    }

    /// List all parameter slots for an instance with their names, CC mappings, and current values.
    #[tool(description = "List all parameter slots for an instance with their names, CC mappings, and values. Shows which slots are mapped to MIDI CC and can output CC when set.")]
    fn list_slots(
        &self,
        Parameters(req): Parameters<GetSlotsRequest>,
    ) -> Result<Json<Vec<crate::params::SlotInfo>>, String> {
        CcBridge::get_slots(&req.instance)
            .map(Json)
            .map_err(|e| e.to_string())
    }

    // =========================================================================
    // Fugue Sequencing Tools
    // =========================================================================

    /// Queue one or more fugues for transport-synchronized playback.
    #[tool(description = "Queue fugues for transport-synced playback. See tool description for the full spec (content types, points shape, curves, worked examples).")]
    fn queue_fugue(
        &self,
        Parameters(req): Parameters<QueueFugueRequest>,
    ) -> Result<Json<QueueFugueSummary>, String> {
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

            let loop_mode = parse_loop_mode_str(loop_mode_str).unwrap_or(LoopMode::Forever);
            let quantize = parse_quantize_str(quantize_str).unwrap_or(QuantizeMode::Bar);

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

        if !errors.is_empty() {
            return Err(errors.join("; "));
        }
        Ok(Json(QueueFugueSummary {
            fugue_ids: fugue_ids.iter().map(|id| id.to_string()).collect(),
            count: fugue_ids.len(),
            duration_beats: default_duration,
            quantize: default_quantize_str.to_string(),
            loop_mode: default_loop_mode_str.to_string(),
        }))
    }

    /// List all active and pending fugues.
    #[tool(description = "List all active and pending fugues on a plugin instance. Shows fugue IDs, tags, loop progress, and timing information.")]
    fn list_fugues(
        &self,
        Parameters(req): Parameters<GetSlotsRequest>,
    ) -> Json<Vec<crate::fugue::FugueInfo>> {
        // Empty vec is the right "nothing queued" signal — no need to
        // branch on the Err path since that also means "nothing to list".
        Json(FugueBridge::get_fugue_info(&req.instance).unwrap_or_default())
    }

    /// Fetch one fugue's current definition — notes, CC lanes, per-note
    /// expression lanes, loop/tag/quantize metadata. Enables a read-modify-write
    /// pattern: read a fugue, mutate one lane, re-queue with the same tag.
    #[tool(description = "Fetch a single fugue's full content by ID. Returns the same compact lane-grouped shape the LLM writes to queue_fugue (notes + cc + pitch_bends + pressures), so you can read a fugue, mutate a lane, and re-queue with the same tag + cancel_mode:'tag:...' to replace it. Use `compact: false` to get the raw event stream instead (useful for debugging or inspecting server-side expansion of per-note expression).")]
    fn get_fugue(
        &self,
        Parameters(req): Parameters<GetFugueRequest>,
    ) -> Result<Json<serde_json::Value>, String> {
        // Compact/raw produce differently-shaped JSON, so the return type
        // stays `Value`. A typed CompactFugueView would pin the schema
        // tighter but the compact shape's lane sets vary per fugue —
        // Value is the honest ceiling on what can be schema'd here.
        let def = FugueBridge::get_definition(&req.instance, req.data.id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!(
                "No fugue with id {} on instance '{}'. Call list_fugues to see active IDs.",
                req.data.id, req.instance
            ))?;
        let value = if req.data.compact {
            compact_fugue_view(&def)
        } else {
            serde_json::to_value(&def).map_err(|e| e.to_string())?
        };
        Ok(Json(value))
    }

    /// Import a `.mid` file as one or more fugues queued on an instance.
    #[tool(description = "Import a Standard MIDI File as fugues on an instance. The body must be base64-encoded SMF bytes. Each MIDI track becomes one fugue (format 0 files produce one fugue). Track names become fugue tags (with optional `tag_prefix` prepended); tracks without names get synthetic `imported-N` tags. Notes + CC are preserved exactly; per-note pitch bend and polyphonic aftertouch are dropped because MIDI 1.0 can't represent them per-note (set `strict: true` to error instead). Returns the queued fugue IDs. Typical workflow: user drags a .mid out of a DAW, edits it in the piano roll, and hands it back to the LLM who imports it as the new canonical version of a part.")]
    fn import_fugue(
        &self,
        Parameters(req): Parameters<ImportFugueRequest>,
    ) -> Result<Json<crate::gui::api::ImportFugueResponse>, String> {
        use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
        let bytes = B64
            .decode(req.data.base64_mid.as_bytes())
            .map_err(|e| format!("failed to decode base64 payload: {}", e))?;

        let query = crate::gui::api::ImportFugueQuery {
            instance: req.instance.clone(),
            tag_prefix: req.data.tag_prefix,
            loop_mode: req.data.loop_mode.as_deref().and_then(parse_loop_mode_str),
            quantize: req.data.quantize.as_deref().and_then(parse_quantize_str),
            cancel_mode: None,
            strict: req.data.strict,
        };
        Ok(Json(crate::gui::api::import_fugue(
            &req.instance,
            &bytes,
            query,
        )))
    }

    /// Cancel a specific fugue by ID.
    #[tool(description = "Cancel a specific fugue by its ID. Sends note-offs for any active notes and stops playback. Use the fugue_id returned by queue_fugue.")]
    fn cancel_fugue(
        &self,
        Parameters(req): Parameters<CancelFugueRequest>,
    ) -> Result<String, String> {
        FugueBridge::cancel(&req.instance, req.data.id).map_err(|e| e.to_string())?;
        // Wait for the audio thread to process the cancel so a follow-up
        // list_fugues / UI fetch reflects the removal.
        FugueBridge::wait_for_fugue_gone(&req.instance, req.data.id, 100);
        Ok(format!("Cancelled fugue {}", req.data.id))
    }

    /// Cancel all fugues with a specific tag.
    #[tool(description = "Cancel all fugues with a matching tag. Sends note-offs for any active notes. Use this to stop all instances of a pattern, like all 'melody' fugues.")]
    fn cancel_fugues_by_tag(
        &self,
        Parameters(req): Parameters<CancelFuguesByTagRequest>,
    ) -> Result<String, String> {
        FugueBridge::cancel_by_tag(&req.instance, &req.data.tag).map_err(|e| e.to_string())?;
        FugueBridge::wait_for_tag_gone(&req.instance, &req.data.tag, 100);
        Ok(format!("Cancelled all fugues with tag '{}'", req.data.tag))
    }

    /// Emergency stop - clear all fugues.
    #[tool(description = "Emergency stop: cancel all fugues on a plugin instance. Sends note-offs for all active notes and clears the queue. Use when you need to stop everything immediately.")]
    fn clear_fugues(
        &self,
        Parameters(req): Parameters<GetSlotsRequest>,
    ) -> Result<String, String> {
        FugueBridge::clear_all(&req.instance).map_err(|e| e.to_string())?;
        FugueBridge::wait_for_no_fugues(&req.instance, 100);
        Ok("Cleared all fugues".into())
    }

    /// Get the current DAW transport state for an instance.
    #[tool(description = "Get the current DAW transport state for an instance: {beat, tempo, playing, time_sig_numerator, is_looping, loop_start_beat, loop_end_beat}. Use this to reason about where we are in the song before scheduling — e.g. 'queue starting at bar 8, we're on bar 6 now'. In standalone mode, reports the simulated 120 BPM always-playing transport.")]
    fn get_transport(
        &self,
        Parameters(req): Parameters<GetSlotsRequest>,
    ) -> Result<Json<crate::fugue::TransportState>, String> {
        FugueBridge::get_transport(&req.instance)
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Get the minimal project-state summary — which Droplets instances are
    /// connected and what each track's primary sound source is.
    #[tool(description = "Call this FIRST when composing. Returns a compact summary of which Droplets instances are connected, their track names, each track's primary device, and per-instance slot hints. For drum tracks, includes pad notes (as pitch notation like 'C2') with pad names and loaded sample names — so you can write a drum pattern with correct note mapping instead of guessing GM conventions. For synth tracks, includes the instrument name and preset. The `slots` field on each instance lists which slot params have been user-configured and what each controls. If `layout_available` is false, no host controller extension is running (e.g. Ableton without the script); fall back to asking the user or GM conventions.")]
    fn get_project_state(&self) -> Json<super::project::ProjectState> {
        let layout = CcBridge::get_project_layout();
        let instances = CcBridge::list_instances();
        Json(super::project::ProjectState::build(layout.as_ref(), &instances))
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

// =============================================================================
// Compact read-back view for get_fugue
// =============================================================================

/// Parse the LLM-facing loop-mode string (`once` | `forever` | `<n>`) into
/// a `LoopMode`. Returns `None` for the empty/unrecognized case so callers
/// can apply their own default; `<n>` decodes to `Times(n)` so the LLM
/// can write `"loop_mode": "4"` for a 4-repeat pattern.
fn parse_loop_mode_str(s: &str) -> Option<LoopMode> {
    match s.trim().to_lowercase().as_str() {
        "" => None,
        "once" => Some(LoopMode::Once),
        "forever" => Some(LoopMode::Forever),
        other => other.parse::<u32>().ok().map(LoopMode::Times),
    }
}

/// Parse the LLM-facing quantize-mode string (`immediate` | `beat` | `bar`
/// | `bars:<n>`) into a `QuantizeMode`. Returns `None` on unrecognized
/// input so the caller decides the fallback (usually `Bar`).
fn parse_quantize_str(s: &str) -> Option<QuantizeMode> {
    let lower = s.trim().to_lowercase();
    match lower.as_str() {
        "" => None,
        "immediate" => Some(QuantizeMode::Immediate),
        "beat" => Some(QuantizeMode::Beat),
        "bar" => Some(QuantizeMode::Bar),
        _ => lower
            .strip_prefix("bars:")
            .and_then(|n| n.parse::<u32>().ok())
            .map(QuantizeMode::Bars),
    }
}

/// Serialize an interpolation mode using its serde tag so round-tripping
/// through queue_fugue hits the same parser. `InterpolationMode` derives
/// snake_case serde, which matches what the LLM writes on input.
fn interp_tag(mode: InterpolationMode) -> &'static str {
    match mode {
        InterpolationMode::Linear => "linear",
        InterpolationMode::Exp => "exp",
        InterpolationMode::Log => "log",
        InterpolationMode::None => "none",
    }
}

/// Render a point tuple with the curve tag optional — matches the
/// `[beat, value]` / `[beat, value, curve]` form the input side accepts.
/// `curve` is only emitted when present AND not the fugue-level default,
/// keeping the serialization compact on typical ramps.
fn point_json(beat: f64, value: serde_json::Value, curve: Option<InterpolationMode>, lane_default: InterpolationMode) -> serde_json::Value {
    match curve {
        Some(c) if c != lane_default => serde_json::json!([beat, value, interp_tag(c)]),
        _ => serde_json::json!([beat, value]),
    }
}

/// Fold a FugueDefinition's event stream back into the compact lane-grouped
/// shape the LLM wrote on input to queue_fugue. Notes are paired by
/// (channel, note) matching the next NoteOff; CC events bucket by
/// (channel, cc); per-note expression lanes bucket by (channel, note).
///
/// Lossy on per-note expression: the original input used sparse anchors
/// (2–4 points), but the fugue scheduler expands them server-side to ~32
/// events/beat. The dense stream is what shows up here. That's fine for
/// the "what is this fugue currently doing" use case; not a full round-trip
/// of the LLM's original input.
fn compact_fugue_view(def: &FugueDefinition) -> serde_json::Value {
    use std::collections::VecDeque;

    // Pair NoteOn with the next NoteOff on the same (channel, note). A FIFO
    // per key handles re-triggers cleanly. Any notes still held at the end
    // of the fugue get duration = duration_beats - on_beat (fugue boundary).
    let mut open_notes: HashMap<(u8, u8), VecDeque<(f64, u8)>> = HashMap::new();
    let mut notes: Vec<serde_json::Value> = Vec::new();
    let mut cc_lanes: HashMap<(u8, u8), Vec<serde_json::Value>> = HashMap::new();
    let mut bend_lanes: HashMap<(u8, u8), Vec<serde_json::Value>> = HashMap::new();
    let mut pressure_lanes: HashMap<(u8, u8), Vec<serde_json::Value>> = HashMap::new();
    // Keep insertion order of lanes stable — matches the order they first
    // appeared in the event stream, which is deterministic across reads.
    let mut cc_order: Vec<(u8, u8)> = Vec::new();
    let mut bend_order: Vec<(u8, u8)> = Vec::new();
    let mut pressure_order: Vec<(u8, u8)> = Vec::new();

    let emit_note = |on_beat: f64, off_beat: f64, channel: u8, note: u8, velocity: u8| -> serde_json::Value {
        serde_json::json!({
            "beat": on_beat,
            "note": note,
            "duration": (off_beat - on_beat).max(0.0),
            "velocity": velocity,
            // 1-indexed to match the LLM input schema (channel 1..=16).
            "channel": channel.saturating_add(1).min(16),
        })
    };

    for timed in &def.events {
        let beat = timed.beat_offset;
        match timed.event {
            FugueEvent::NoteOn { channel, note, velocity } => {
                open_notes.entry((channel, note)).or_default().push_back((beat, velocity));
            }
            FugueEvent::NoteOff { channel, note } => {
                if let Some(q) = open_notes.get_mut(&(channel, note)) {
                    if let Some((on_beat, vel)) = q.pop_front() {
                        notes.push(emit_note(on_beat, beat, channel, note, vel));
                    }
                }
            }
            FugueEvent::Cc { channel, cc, value, curve } => {
                let key = (channel, cc);
                if !cc_lanes.contains_key(&key) {
                    cc_order.push(key);
                }
                cc_lanes.entry(key).or_default().push(
                    point_json(beat, serde_json::json!(value), curve, def.cc_interpolation),
                );
            }
            FugueEvent::PerNotePitchBend { channel, note, semitones } => {
                let key = (channel, note);
                if !bend_lanes.contains_key(&key) {
                    bend_order.push(key);
                }
                bend_lanes.entry(key).or_default().push(
                    serde_json::json!([beat, semitones]),
                );
            }
            FugueEvent::PerNotePressure { channel, note, pressure } => {
                let key = (channel, note);
                if !pressure_lanes.contains_key(&key) {
                    pressure_order.push(key);
                }
                pressure_lanes.entry(key).or_default().push(
                    serde_json::json!([beat, pressure]),
                );
            }
        }
    }

    // Flush dangling note-ons against the fugue's end.
    for ((channel, note), q) in open_notes {
        for (on_beat, vel) in q {
            notes.push(emit_note(on_beat, def.duration_beats, channel, note, vel));
        }
    }

    let cc: Vec<serde_json::Value> = cc_order.into_iter().map(|(channel, cc)| {
        serde_json::json!({
            "cc": cc,
            "channel": channel.saturating_add(1).min(16),
            "points": cc_lanes.remove(&(channel, cc)).unwrap_or_default(),
        })
    }).collect();

    let pitch_bends: Vec<serde_json::Value> = bend_order.into_iter().map(|(channel, note)| {
        serde_json::json!({
            "note": note,
            "channel": channel.saturating_add(1).min(16),
            "points": bend_lanes.remove(&(channel, note)).unwrap_or_default(),
        })
    }).collect();

    let pressures: Vec<serde_json::Value> = pressure_order.into_iter().map(|(channel, note)| {
        serde_json::json!({
            "note": note,
            "channel": channel.saturating_add(1).min(16),
            "points": pressure_lanes.remove(&(channel, note)).unwrap_or_default(),
        })
    }).collect();

    let mut out = serde_json::Map::new();
    out.insert("id".into(), serde_json::json!(def.id.to_string()));
    if let Some(tag) = &def.tag {
        out.insert("tag".into(), serde_json::json!(tag));
    }
    out.insert("duration_beats".into(), serde_json::json!(def.duration_beats));
    out.insert("loop_mode".into(), serde_json::to_value(def.loop_mode).unwrap_or(serde_json::Value::Null));
    out.insert("quantize".into(), serde_json::to_value(def.quantize).unwrap_or(serde_json::Value::Null));
    out.insert("cancel_mode".into(), serde_json::to_value(&def.cancel_mode).unwrap_or(serde_json::Value::Null));
    if def.cc_interpolation != InterpolationMode::Linear {
        out.insert("cc_interpolation".into(), serde_json::json!(interp_tag(def.cc_interpolation)));
    }
    out.insert("type".into(), serde_json::json!("composite"));
    out.insert("notes".into(), serde_json::Value::Array(notes));
    out.insert("cc".into(), serde_json::Value::Array(cc));
    out.insert("pitch_bends".into(), serde_json::Value::Array(pitch_bends));
    out.insert("pressures".into(), serde_json::Value::Array(pressures));
    serde_json::Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fugue::FugueDefinition;

    fn def_with_events(events: Vec<TimedFugueEvent>, duration: f64) -> FugueDefinition {
        FugueDefinition::new(events, duration).with_tag("test")
    }

    #[test]
    fn compact_view_pairs_note_on_off() {
        let events = vec![
            TimedFugueEvent::note_on(0.0, 0, 60, 100),
            TimedFugueEvent::note_off(1.0, 0, 60),
        ];
        let v = compact_fugue_view(&def_with_events(events, 4.0));
        let notes = v.get("notes").and_then(|n| n.as_array()).unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].get("note").and_then(|n| n.as_u64()), Some(60));
        assert_eq!(notes[0].get("duration").and_then(|n| n.as_f64()), Some(1.0));
        assert_eq!(notes[0].get("velocity").and_then(|n| n.as_u64()), Some(100));
        // 0-indexed internally → 1-indexed on wire.
        assert_eq!(notes[0].get("channel").and_then(|n| n.as_u64()), Some(1));
    }

    #[test]
    fn compact_view_closes_dangling_note_at_fugue_end() {
        // A note-on with no matching note-off — duration runs to the fugue's
        // declared end. Covers notes that happen to be still held when the
        // fugue finishes its cycle.
        let events = vec![TimedFugueEvent::note_on(0.0, 0, 60, 100)];
        let v = compact_fugue_view(&def_with_events(events, 4.0));
        let notes = v.get("notes").and_then(|n| n.as_array()).unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].get("duration").and_then(|n| n.as_f64()), Some(4.0));
    }

    #[test]
    fn compact_view_buckets_cc_by_channel_and_cc() {
        let events = vec![
            TimedFugueEvent::cc(0.0, 0, 74, 30),
            TimedFugueEvent::cc(2.0, 0, 74, 110),
            TimedFugueEvent::cc(0.0, 0, 1, 0),
            TimedFugueEvent::cc(2.0, 0, 1, 127),
        ];
        let v = compact_fugue_view(&def_with_events(events, 4.0));
        let cc_lanes = v.get("cc").and_then(|n| n.as_array()).unwrap();
        assert_eq!(cc_lanes.len(), 2);
        // Lane order matches first-seen order in the event stream.
        assert_eq!(cc_lanes[0].get("cc").and_then(|c| c.as_u64()), Some(74));
        assert_eq!(cc_lanes[1].get("cc").and_then(|c| c.as_u64()), Some(1));
        let pts74 = cc_lanes[0].get("points").and_then(|p| p.as_array()).unwrap();
        assert_eq!(pts74.len(), 2);
    }

    #[test]
    fn compact_view_includes_tag_and_duration() {
        let v = compact_fugue_view(&def_with_events(vec![], 8.0));
        assert_eq!(v.get("tag").and_then(|t| t.as_str()), Some("test"));
        assert_eq!(v.get("duration_beats").and_then(|d| d.as_f64()), Some(8.0));
        assert_eq!(v.get("type").and_then(|t| t.as_str()), Some("composite"));
    }

    #[test]
    fn compact_view_reports_empty_lanes_as_empty_arrays() {
        let v = compact_fugue_view(&def_with_events(vec![], 4.0));
        for field in ["notes", "cc", "pitch_bends", "pressures"] {
            let arr = v.get(field).and_then(|a| a.as_array()).unwrap_or_else(|| panic!("{} missing", field));
            assert!(arr.is_empty(), "{} should be empty", field);
        }
    }
}
