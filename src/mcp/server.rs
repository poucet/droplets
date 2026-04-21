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

use super::bridge::CcBridge;
use super::types::{
    CancelFugueRequest, CancelFuguesByTagRequest, GetFugueRequest, GetSlotsRequest,
    ImportFugueRequest, InstanceHandle, QueueFugueDefaults, QueueFugueRequest, QueueFugueSummary,
    RenameInstanceRequest, compact_to_definition, definition_to_compact, parse_loop_mode_str,
    parse_quantize_str,
};
use crate::fugue::FugueBridge;

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
        let defaults = QueueFugueDefaults::from_data(&req.data);
        let mut fugue_ids: Vec<u64> = Vec::new();
        let mut errors: Vec<String> = Vec::new();

        for compact in &req.data.fugues {
            let definition = compact_to_definition(compact, &defaults);
            match FugueBridge::queue(&req.instance, definition) {
                Ok(id) => fugue_ids.push(id),
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
            duration_beats: defaults.duration_beats,
            quantize: defaults.quantize_str,
            loop_mode: defaults.loop_mode_str,
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
            // Fold events back into the same CompactFugue shape the LLM
            // writes on input, then drop the fugue's id onto the object
            // so clients can cancel / replace it without a second call.
            let compact = definition_to_compact(&def);
            let mut v = serde_json::to_value(&compact).map_err(|e| e.to_string())?;
            if let Some(obj) = v.as_object_mut() {
                obj.insert("id".into(), def.id.to_string().into());
            }
            v
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
    #[tool(description = "Call this FIRST when composing. Returns a compact summary of which Droplets instances are connected, their track names, each track's primary device, per-instance slot hints, and the user's current `custom_instructions` from the Settings tab (refreshed on each call — edits land here without needing to restart the session). For drum tracks, includes pad notes (as pitch notation like 'C2') with pad names and loaded sample names — so you can write a drum pattern with correct note mapping instead of guessing GM conventions. For synth tracks, includes the instrument name and preset. If `layout_available` is false, no host controller extension is running (e.g. Ableton without the script); fall back to asking the user or GM conventions.")]
    fn get_project_state(&self) -> Json<super::project::ProjectState> {
        let layout = CcBridge::get_project_layout();
        let instances = CcBridge::list_instances();
        let custom_instructions = crate::fugue::settings::get_settings().custom_instructions;
        Json(super::project::ProjectState::build(
            layout.as_ref(),
            &instances,
            custom_instructions,
        ))
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
            // The MCP system prompt base lives in instructions.md next to
            // this file — edit there, not here. User-authored custom context
            // from the Settings tab is delivered via the `custom_instructions`
            // field of `get_project_state` (refreshed on every call) rather
            // than baked in at initialize time; settings edits land without
            // the session needing to restart.
            instructions: Some(include_str!("instructions.md").to_string()),
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
            // Some LLM providers (notably Google Gemini) only accept an
            // OpenAPI 3.0 subset of JSON Schema — no `const`, no
            // `prefixItems`. schemars 1.x emits both for tagged enums and
            // custom tuple types, so we rewrite the schemas to the
            // widely-supported forms here before handing them to MCP
            // clients. `const: X` becomes `enum: [X]`; `prefixItems` is
            // dropped (losing positional validation, keeping shape).
            let mut input_val = serde_json::Value::Object((*tool.input_schema).clone());
            sanitize_schema_in_place(&mut input_val);
            if let serde_json::Value::Object(obj) = input_val {
                tool.input_schema = std::sync::Arc::new(obj);
            }
            if let Some(out) = tool.output_schema.as_ref() {
                let mut out_val = serde_json::Value::Object((**out).clone());
                sanitize_schema_in_place(&mut out_val);
                if let serde_json::Value::Object(obj) = out_val {
                    tool.output_schema = Some(std::sync::Arc::new(obj));
                }
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

/// Recursively rewrite a JSON Schema emitted by `schemars` into a form the
/// narrower OpenAPI-3.0 subset (Gemini, some other LLM APIs) accepts.
///
/// Concretely:
/// - `{ "const": X }` → `{ "enum": [X] }`. Gemini's schema validator rejects
///   `const` (emitted by schemars for tagged-enum discriminators like
///   `FugueContent`'s `"type": "notes"`).
/// - `prefixItems: [...]` is dropped and a generic `items` left in place.
///   Gemini doesn't know `prefixItems` (JSON Schema Draft 2020-12); dropping
///   it loses positional validation but keeps the overall array shape.
/// - Descends into `properties`, `items`, `oneOf`, `anyOf`, `allOf`, and the
///   per-element entries of `prefixItems` itself so nested violations get
///   sanitized too.
///
/// No-op on anything else. Leaves `Value::Array`, `Value::Number`, etc.
/// untouched apart from recursing through them.
fn sanitize_schema_in_place(value: &mut serde_json::Value) {
    use serde_json::Value;
    match value {
        Value::Object(map) => {
            // const → enum: [const_value]
            if let Some(const_val) = map.remove("const") {
                map.insert("enum".into(), Value::Array(vec![const_val]));
            }
            // prefixItems → drop entirely (we lose positional validation
            // but Gemini doesn't support it). Recurse into it first so any
            // nested const rewrites get applied in case something else in
            // the ecosystem does support it.
            if let Some(prefix) = map.get_mut("prefixItems") {
                sanitize_schema_in_place(prefix);
            }
            map.remove("prefixItems");

            for (_, v) in map.iter_mut() {
                sanitize_schema_in_place(v);
            }
        }
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                sanitize_schema_in_place(v);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod sanitizer_tests {
    use super::sanitize_schema_in_place;
    use serde_json::json;

    #[test]
    fn rewrites_const_to_single_enum() {
        let mut v = json!({ "const": "notes" });
        sanitize_schema_in_place(&mut v);
        assert_eq!(v, json!({ "enum": ["notes"] }));
    }

    #[test]
    fn rewrites_const_nested_in_properties() {
        // Matches schemars output for tagged-enum discriminators.
        let mut v = json!({
            "type": "object",
            "properties": {
                "type": { "const": "notes" },
                "notes": { "type": "array" }
            }
        });
        sanitize_schema_in_place(&mut v);
        assert_eq!(v["properties"]["type"], json!({ "enum": ["notes"] }));
    }

    #[test]
    fn rewrites_const_deep_in_oneof_branches() {
        let mut v = json!({
            "oneOf": [
                { "properties": { "type": { "const": "a" } } },
                { "properties": { "type": { "const": "b" } } }
            ]
        });
        sanitize_schema_in_place(&mut v);
        assert_eq!(v["oneOf"][0]["properties"]["type"], json!({ "enum": ["a"] }));
        assert_eq!(v["oneOf"][1]["properties"]["type"], json!({ "enum": ["b"] }));
    }

    #[test]
    fn drops_prefix_items() {
        let mut v = json!({
            "type": "array",
            "prefixItems": [{ "type": "number" }, { "type": "string" }]
        });
        sanitize_schema_in_place(&mut v);
        assert!(v.get("prefixItems").is_none(), "prefixItems should be dropped");
        assert_eq!(v["type"], "array");
    }

    #[test]
    fn leaves_ordinary_enum_alone() {
        let mut v = json!({ "enum": ["linear", "exp", "log", "none"] });
        let original = v.clone();
        sanitize_schema_in_place(&mut v);
        assert_eq!(v, original, "plain enum arrays should pass through unchanged");
    }

    #[test]
    fn handles_non_object_values() {
        // Sanity: calling on a plain string / number / bool should not panic.
        let mut s = json!("hello");
        sanitize_schema_in_place(&mut s);
        let mut n = json!(42);
        sanitize_schema_in_place(&mut n);
        let mut b = json!(true);
        sanitize_schema_in_place(&mut b);
    }
}

