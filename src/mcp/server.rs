//! MCP Server tool definitions for Droplets.
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
    CancelFugueRequest, CancelFuguesByTagRequest, ClearFuguesRequest, GetFugueRequest,
    GetFugueResponse, GetSlotsRequest, ImportFugueRequest, InstanceHandle, ListFuguesRequest,
    ListFuguesResponse, ListInstancesResponse, ListSlotsResponse, ListedFugue,
    QueueFugueDefaults, QueueFugueRequest, QueueFugueSummary, RenameInstanceRequest,
    compact_to_definition, definition_to_compact, parse_loop_mode_str, parse_quantize_str,
};
use crate::fugue::FugueBridge;

/// MCP Server for Droplets
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
    /// List all connected Droplets plugin instances.
    #[tool(description = "List all connected Droplets plugin instances. Returns `{ instances: [{ id, name }, …] }`. Use the names or ids to target a specific instance when queueing fugues.")]
    fn list_instances(&self) -> Json<ListInstancesResponse> {
        Json(ListInstancesResponse {
            instances: CcBridge::list_instances()
                .into_iter()
                .map(|(id, name)| InstanceHandle { id, name })
                .collect(),
        })
    }

    /// Rename a plugin instance for easier reference.
    #[tool(description = "Rename a Droplets instance for easier reference. Use names like 'bass', 'pad', 'lead' to make targeting clearer.")]
    fn set_instance_name(
        &self,
        Parameters(req): Parameters<RenameInstanceRequest>,
    ) -> Result<String, String> {
        CcBridge::rename(&req.instance, &req.name)
            .map(|old_name| format!("Renamed '{}' to '{}'", old_name, req.name))
            .map_err(|e| e.to_string())
    }

    /// List all parameter slots for an instance with their names, CC mappings, and current values.
    #[tool(description = "List all parameter slots for an instance with their names, CC mappings, and values. Returns `{ slots: [...] }`. Shows which slots are mapped to MIDI CC and can output CC when set.")]
    fn list_slots(
        &self,
        Parameters(req): Parameters<GetSlotsRequest>,
    ) -> Result<Json<ListSlotsResponse>, String> {
        CcBridge::get_slots(&req.instance)
            .map(|slots| Json(ListSlotsResponse { slots }))
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

    /// List all active and pending fugues across one or every instance.
    #[tool(description = "List active and pending fugues. Returns `{ fugues: [...] }`. Omit `instance` to list across every connected Droplets instance; pass a name or id to scope to one. Each row carries its own `instance_id` + `instance_name`, so follow-up calls (cancel_fugue, get_fugue) don't need a second list_instances hop.")]
    fn list_fugues(
        &self,
        Parameters(req): Parameters<ListFuguesRequest>,
    ) -> Json<ListFuguesResponse> {
        // Fan-out on None: walk every registered instance and concat their
        // infos with the instance metadata stapled on. A single-instance
        // scope uses the same code path, just with a one-element iterator,
        // so the output shape is uniform.
        let targets: Vec<(String, String)> = match req.instance.as_deref() {
            None => FugueBridge::list_all_instances(),
            Some(name) => CcBridge::list_instances()
                .into_iter()
                .find(|(id, nm)| id == name || nm == name)
                .into_iter()
                .collect(),
        };
        let mut fugues = Vec::new();
        for (instance_id, instance_name) in targets {
            let infos = FugueBridge::get_fugue_info(&instance_id).unwrap_or_default();
            for info in infos {
                fugues.push(ListedFugue {
                    instance_id: instance_id.clone(),
                    instance_name: instance_name.clone(),
                    info,
                });
            }
        }
        Json(ListFuguesResponse { fugues })
    }

    /// Fetch one fugue's current definition — notes, CC lanes, per-note
    /// expression lanes, loop/tag/quantize metadata. Enables a read-modify-write
    /// pattern: read a fugue, mutate one lane, re-queue with the same tag.
    #[tool(description = "Fetch a single fugue's full content by ID. Returns the fugue in the same compact lane-grouped form the LLM writes to queue_fugue (notes + cc + pitch_bends + pressures), with the `id` included — read a fugue, mutate one lane, and re-queue with the same tag + cancel_mode:'tag:...' to replace it.")]
    fn get_fugue(
        &self,
        Parameters(req): Parameters<GetFugueRequest>,
    ) -> Result<Json<GetFugueResponse>, String> {
        let def = FugueBridge::get_definition(&req.instance, req.data.id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!(
                "No fugue with id {} on instance '{}'. Call list_fugues to see active IDs.",
                req.data.id, req.instance
            ))?;
        Ok(Json(GetFugueResponse {
            id: def.id.to_string(),
            fugue: definition_to_compact(&def),
        }))
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

    /// Cancel a specific fugue by ID, searching every instance if unscoped.
    #[tool(description = "Cancel a fugue by ID. Omit `instance` to search every connected instance for the id (fugue ids are globally unique); pass a name or id to scope. Sends note-offs for any active notes and stops playback.")]
    fn cancel_fugue(
        &self,
        Parameters(req): Parameters<CancelFugueRequest>,
    ) -> Result<String, String> {
        let id = req.data.id;
        // Scoped: pass through to the existing per-instance cancel.
        if let Some(name) = req.instance.as_deref() {
            FugueBridge::cancel(name, id).map_err(|e| e.to_string())?;
            FugueBridge::wait_for_fugue_gone(name, id, 100);
            return Ok(format!("Cancelled fugue {} on '{}'", id, name));
        }
        // Unscoped: try every instance. Fugue ids are random u64 so at
        // most one hit is expected; stop at the first that succeeds.
        // "Not found" is rolled up to a single error so the LLM gets a
        // clean actionable message.
        let mut matched_instance: Option<String> = None;
        for (instance_id, _) in FugueBridge::list_all_instances() {
            let infos = FugueBridge::get_fugue_info(&instance_id).unwrap_or_default();
            if infos.iter().any(|i| i.id == id) {
                FugueBridge::cancel(&instance_id, id).map_err(|e| e.to_string())?;
                FugueBridge::wait_for_fugue_gone(&instance_id, id, 100);
                matched_instance = Some(instance_id);
                break;
            }
        }
        match matched_instance {
            Some(name) => Ok(format!("Cancelled fugue {} on '{}'", id, name)),
            None => Err(format!(
                "No fugue with id {} across any connected instance. Call list_fugues to see active ids.",
                id
            )),
        }
    }

    /// Cancel fugues by tag across one or every instance.
    #[tool(description = "Cancel all fugues with a matching tag. Omit `instance` to cancel across every connected instance; pass a name or id to scope. Sends note-offs for any active notes.")]
    fn cancel_fugues_by_tag(
        &self,
        Parameters(req): Parameters<CancelFuguesByTagRequest>,
    ) -> Result<String, String> {
        let tag = &req.data.tag;
        let targets: Vec<String> = match req.instance.as_deref() {
            Some(name) => vec![name.to_string()],
            None => FugueBridge::list_all_instances()
                .into_iter()
                .map(|(id, _)| id)
                .collect(),
        };
        // Collect errors but still process every instance — one bad
        // entry shouldn't block the rest of the fan-out.
        let mut errors: Vec<String> = Vec::new();
        for instance_id in &targets {
            if let Err(e) = FugueBridge::cancel_by_tag(instance_id, tag) {
                errors.push(format!("{}: {}", instance_id, e));
                continue;
            }
            FugueBridge::wait_for_tag_gone(instance_id, tag, 100);
        }
        if !errors.is_empty() {
            return Err(errors.join("; "));
        }
        Ok(format!(
            "Cancelled all fugues with tag '{}' across {} instance(s)",
            tag,
            targets.len()
        ))
    }

    /// Emergency stop — clear all fugues on one or every instance.
    #[tool(description = "Emergency stop: cancel every fugue. Omit `instance` to clear across every connected instance; pass a name or id to scope. Sends note-offs for all active notes and empties the queue.")]
    fn clear_fugues(
        &self,
        Parameters(req): Parameters<ClearFuguesRequest>,
    ) -> Result<String, String> {
        let targets: Vec<String> = match req.instance.as_deref() {
            Some(name) => vec![name.to_string()],
            None => FugueBridge::list_all_instances()
                .into_iter()
                .map(|(id, _)| id)
                .collect(),
        };
        let mut errors: Vec<String> = Vec::new();
        for instance_id in &targets {
            if let Err(e) = FugueBridge::clear_all(instance_id) {
                errors.push(format!("{}: {}", instance_id, e));
                continue;
            }
            FugueBridge::wait_for_no_fugues(instance_id, 100);
        }
        if !errors.is_empty() {
            return Err(errors.join("; "));
        }
        Ok(format!("Cleared all fugues across {} instance(s)", targets.len()))
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

