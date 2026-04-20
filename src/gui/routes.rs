//! Route handler for wry custom protocol (droplets://)
//!
//! This delegates to the shared API handlers in api.rs

use std::sync::Arc;
use crate::params::DropletParams;
use super::api;

/// Route an API request to the appropriate handler
pub fn handle_request(path: &str, method: &str, body: &[u8], params: &Arc<DropletParams>, instance_id: &str) -> String {
    match path {
        "/self" => handle_self(instance_id),
        "/slots" => handle_slots(instance_id),
        "/activity" => handle_activity(),
        "/fugues" => handle_fugues(instance_id),
        "/transport" => handle_transport(instance_id),
        "/instances" => handle_instances(),
        "/project_layout" if method == "GET" => handle_get_project_layout(),
        "/project_layout" if method == "POST" => handle_post_project_layout(body),
        "/cancel_learn" => handle_cancel_learn(instance_id, params),
        "/clear_fugues" => handle_clear_fugues(instance_id),
        "/settings" if method == "GET" => handle_get_settings(),
        "/reveal_exports" => handle_reveal_exports(),
        // POST endpoints
        "/queue_fugue" if method == "POST" => handle_queue_fugue(body, instance_id),
        "/cancel_fugue" if method == "POST" => handle_cancel_fugue(body, instance_id),
        "/cancel_fugues_by_tag" if method == "POST" => handle_cancel_fugues_by_tag(body, instance_id),
        "/rename_instance" if method == "POST" => handle_rename_instance(body),
        "/settings" if method == "POST" => handle_update_settings(body),
        "/export_fugue" if method == "POST" => handle_export_fugue(body, instance_id),
        "/import_fugue" if method == "POST" => handle_import_fugue(body, instance_id),
        "/slots" if method == "POST" => handle_add_slot(body, instance_id),
        _ => handle_dynamic_route(path, method, body, instance_id, params),
    }
}

fn handle_dynamic_route(path: &str, method: &str, body: &[u8], instance: &str, params: &Arc<DropletParams>) -> String {
    if let Some(slot_str) = path.strip_prefix("/start_learn/") {
        return handle_start_learn(slot_str, instance, params);
    }

    if let Some(slot_str) = path.strip_prefix("/wiggle/") {
        return handle_wiggle(slot_str, instance);
    }

    // /note_on/{note}/{velocity} - trigger a note
    if let Some(rest) = path.strip_prefix("/note_on/") {
        return handle_note_on(rest, instance);
    }

    // /note_off/{note} - release a note
    if let Some(note_str) = path.strip_prefix("/note_off/") {
        return handle_note_off(note_str, instance);
    }

    // /fugue/{id} - get a single fugue
    if let Some(id_str) = path.strip_prefix("/fugue/") {
        return handle_fugue_by_id(id_str, instance);
    }

    // /slots/{i}         DELETE → remove a slot
    // /slots/{i}/cc      POST   body={"cc":<u8>} → set CC number
    // /slots/{i}/name    POST   body={"name":"..."} → rename
    // /slots/{i}/channel POST   body={"channel":<u8>} → set channel
    if let Some(rest) = path.strip_prefix("/slots/") {
        if let Some(idx_str) = rest.strip_suffix("/cc") {
            return handle_set_slot_cc(idx_str, body, instance);
        }
        if let Some(idx_str) = rest.strip_suffix("/name") {
            return handle_rename_slot(idx_str, body, instance);
        }
        if let Some(idx_str) = rest.strip_suffix("/channel") {
            return handle_set_slot_channel(idx_str, body, instance);
        }
        if method == "DELETE" {
            return handle_remove_slot(rest, instance);
        }
    }

    serialize_error("not found")
}

// =============================================================================
// Handler implementations using shared API
// =============================================================================

fn handle_self(instance_id: &str) -> String {
    serde_json::json!({ "id": instance_id }).to_string()
}

fn handle_slots(instance: &str) -> String {
    match api::get_slots(instance) {
        Ok(response) => {
            let result = serde_json::to_string(&response).unwrap_or_else(|_| serialize_error("serialize failed"));
            result
        }
        Err(e) => serialize_error(&e),
    }
}

fn handle_activity() -> String {
    let response = api::get_activity();
    serde_json::to_string(&response).unwrap_or_else(|_| serialize_error("serialize failed"))
}

fn handle_fugues(instance: &str) -> String {
    let response = api::get_fugues(instance);
    serde_json::to_string(&response).unwrap_or_else(|_| serialize_error("serialize failed"))
}

fn handle_transport(instance: &str) -> String {
    let response = api::get_transport(instance);
    serde_json::to_string(&response).unwrap_or_else(|_| serialize_error("serialize failed"))
}

fn handle_instances() -> String {
    let response = api::get_instances();
    serde_json::to_string(&response).unwrap_or_else(|_| serialize_error("serialize failed"))
}

/// `GET /api/project_layout` via the wry protocol. Returns the last layout
/// pushed by the host controller extension, or an empty layout when nothing
/// has been pushed yet.
fn handle_get_project_layout() -> String {
    let layout = crate::mcp::CcBridge::get_project_layout().unwrap_or_default();
    log::info!(
        "routes /project_layout GET: returning {} tracks",
        layout.tracks.len()
    );
    serde_json::to_string(&layout)
        .unwrap_or_else(|_| serialize_error("serialize failed"))
}

/// `POST /api/project_layout` via the wry protocol. Mirrors the MCP server
/// endpoint on :9999 so the Bitwig extension can push to either path; both
/// end up in the same storage and trigger the same broadcast.
fn handle_post_project_layout(body: &[u8]) -> String {
    match serde_json::from_slice::<crate::mcp::project::ProjectLayout>(body) {
        Ok(layout) => {
            log::info!(
                "routes /project_layout POST received: {} tracks",
                layout.tracks.len()
            );
            crate::mcp::CcBridge::set_project_layout(layout.clone());
            crate::gui::server::broadcast_project_layout(layout);
            serde_json::json!({ "ok": true }).to_string()
        }
        Err(e) => {
            log::warn!("routes /project_layout POST parse error: {}", e);
            serialize_error(&format!("parse error: {}", e))
        }
    }
}

fn handle_start_learn(slot_str: &str, _instance: &str, params: &Arc<DropletParams>) -> String {
    let Ok(slot) = slot_str.parse::<usize>() else {
        return serialize_error("invalid slot");
    };

    // Use params directly for wry context (more efficient)
    params.start_learning(slot);
    crate::logger::log_gui_event("learn_started", &format!("Slot {}", slot));
    serialize_ok(None)
}

fn handle_cancel_learn(_instance: &str, params: &Arc<DropletParams>) -> String {
    params.cancel_learning();
    crate::logger::log_gui_event("learn_cancelled", "All slots");
    serialize_ok(None)
}

fn handle_wiggle(slot_str: &str, instance: &str) -> String {
    let Ok(slot) = slot_str.parse::<usize>() else {
        return serialize_error("invalid slot");
    };

    match api::wiggle_slot(instance, slot) {
        Ok(response) => {
            crate::logger::log_gui_event("wiggle_started", &format!("Slot {}", slot));
            serde_json::to_string(&response).unwrap_or_else(|_| serialize_error("serialize failed"))
        }
        Err(e) => serialize_error(&e),
    }
}

fn handle_note_on(rest: &str, instance: &str) -> String {
    let parts: Vec<&str> = rest.split('/').collect();
    let (note, velocity) = match parts.as_slice() {
        [note_str, vel_str] => {
            let Ok(note) = note_str.parse::<u8>() else {
                return serialize_error("invalid note");
            };
            let Ok(vel) = vel_str.parse::<u8>() else {
                return serialize_error("invalid velocity");
            };
            (note.min(127), vel.min(127))
        }
        [note_str] => {
            let Ok(note) = note_str.parse::<u8>() else {
                return serialize_error("invalid note");
            };
            (note.min(127), 100u8) // default velocity
        }
        _ => return serialize_error("invalid path"),
    };

    match api::note_on(instance, note, velocity) {
        Ok(response) => {
            crate::logger::log_gui_event("note_on", &format!("Note {} vel {}", note, velocity));
            serde_json::to_string(&response).unwrap_or_else(|_| serialize_error("serialize failed"))
        }
        Err(e) => serialize_error(&e),
    }
}

fn handle_note_off(note_str: &str, instance: &str) -> String {
    let Ok(note) = note_str.parse::<u8>() else {
        return serialize_error("invalid note");
    };
    let note = note.min(127);

    match api::note_off(instance, note) {
        Ok(response) => {
            crate::logger::log_gui_event("note_off", &format!("Note {}", note));
            serde_json::to_string(&response).unwrap_or_else(|_| serialize_error("serialize failed"))
        }
        Err(e) => serialize_error(&e),
    }
}

fn handle_fugue_by_id(id_str: &str, instance: &str) -> String {
    let Ok(id) = id_str.parse::<u64>() else {
        return serialize_error("invalid id");
    };

    match api::get_fugue_by_id(instance, id) {
        Ok(response) => serde_json::to_string(&response).unwrap_or_else(|_| serialize_error("serialize failed")),
        Err(e) => serialize_error(&e),
    }
}

// =============================================================================
// Fugue Queue/Cancel Handlers (POST)
// =============================================================================

fn handle_rename_instance(body: &[u8]) -> String {
    #[derive(serde::Deserialize)]
    struct Req { instance: String, name: String }
    let Ok(req) = serde_json::from_slice::<Req>(body) else {
        return serialize_error("invalid request body");
    };
    match api::rename_instance(&req.instance, &req.name) {
        Ok(response) => serde_json::to_string(&response).unwrap_or_else(|_| serialize_error("serialize failed")),
        Err(e) => serialize_error(&e),
    }
}

fn handle_queue_fugue(body: &[u8], instance: &str) -> String {
    let Ok(req) = serde_json::from_slice::<api::QueueFugueRequest>(body) else {
        return serialize_error("invalid request body");
    };

    let response = api::queue_fugue(instance, req);
    crate::logger::log_gui_event("queue_fugue", &format!("id={:?}", response.fugue_id));
    serde_json::to_string(&response).unwrap_or_else(|_| serialize_error("serialize failed"))
}

fn handle_cancel_fugue(body: &[u8], instance: &str) -> String {
    let Ok(req) = serde_json::from_slice::<api::CancelFugueRequest>(body) else {
        return serialize_error("invalid request body");
    };

    match api::cancel_fugue(instance, req.id) {
        Ok(response) => {
            crate::logger::log_gui_event("cancel_fugue", &format!("id={}", req.id));
            serde_json::to_string(&response).unwrap_or_else(|_| serialize_error("serialize failed"))
        }
        Err(e) => serialize_error(&e),
    }
}

fn handle_cancel_fugues_by_tag(body: &[u8], instance: &str) -> String {
    let Ok(req) = serde_json::from_slice::<api::CancelByTagRequest>(body) else {
        return serialize_error("invalid request body");
    };

    match api::cancel_fugues_by_tag(instance, &req.tag) {
        Ok(response) => {
            crate::logger::log_gui_event("cancel_fugues_by_tag", &format!("tag={}", req.tag));
            serde_json::to_string(&response).unwrap_or_else(|_| serialize_error("serialize failed"))
        }
        Err(e) => serialize_error(&e),
    }
}

fn handle_clear_fugues(instance: &str) -> String {
    match api::clear_fugues(instance) {
        Ok(response) => {
            crate::logger::log_gui_event("clear_fugues", "all");
            serde_json::to_string(&response).unwrap_or_else(|_| serialize_error("serialize failed"))
        }
        Err(e) => serialize_error(&e),
    }
}

// =============================================================================
// Helpers
// =============================================================================

/// POST /slots — add a new CC slot. Body: `{cc: u8, name: string}`.
fn handle_add_slot(body: &[u8], instance: &str) -> String {
    #[derive(serde::Deserialize)]
    struct Req {
        cc: u8,
        name: String,
    }
    let Ok(req) = serde_json::from_slice::<Req>(body) else {
        return serialize_error("invalid body");
    };
    match crate::mcp::CcBridge::add_slot(instance, req.cc, &req.name) {
        Ok(idx) => serde_json::json!({ "ok": true, "index": idx }).to_string(),
        Err(e) => serialize_error(e),
    }
}

/// DELETE /slots/{i} — remove a slot.
fn handle_remove_slot(idx_str: &str, instance: &str) -> String {
    let Ok(idx) = idx_str.parse::<usize>() else {
        return serialize_error("invalid slot index");
    };
    match crate::mcp::CcBridge::remove_slot(instance, idx) {
        Ok(()) => serialize_ok(None),
        Err(e) => serialize_error(e),
    }
}

/// POST /slots/{i}/cc — set the slot's CC number. Body `{cc: u8}`.
fn handle_set_slot_cc(idx_str: &str, body: &[u8], instance: &str) -> String {
    let Ok(idx) = idx_str.parse::<usize>() else {
        return serialize_error("invalid slot index");
    };
    #[derive(serde::Deserialize)]
    struct Req { cc: u8 }
    let Ok(req) = serde_json::from_slice::<Req>(body) else {
        return serialize_error("invalid body");
    };
    // Preserve channel — pulling it from the existing slot lets us reuse
    // `map_slot` without a dedicated setter.
    let channel = crate::mcp::CcBridge::get_slots(instance)
        .ok()
        .and_then(|v| v.into_iter().find(|s| s.index == idx).map(|s| s.channel))
        .unwrap_or(0);
    match crate::mcp::CcBridge::map_slot(instance, idx, req.cc, channel) {
        Ok(()) => serialize_ok(None),
        Err(e) => serialize_error(e),
    }
}

/// POST /slots/{i}/name — rename a slot. Body `{name: string}`.
fn handle_rename_slot(idx_str: &str, body: &[u8], instance: &str) -> String {
    let Ok(idx) = idx_str.parse::<usize>() else {
        return serialize_error("invalid slot index");
    };
    #[derive(serde::Deserialize)]
    struct Req { name: String }
    let Ok(req) = serde_json::from_slice::<Req>(body) else {
        return serialize_error("invalid body");
    };
    match crate::mcp::CcBridge::rename_slot(instance, idx, &req.name) {
        Ok(_) => serialize_ok(None),
        Err(e) => serialize_error(e),
    }
}

/// POST /slots/{i}/channel — set the slot's MIDI channel (0-15). Body `{channel: u8}`.
fn handle_set_slot_channel(idx_str: &str, body: &[u8], instance: &str) -> String {
    let Ok(idx) = idx_str.parse::<usize>() else {
        return serialize_error("invalid slot index");
    };
    #[derive(serde::Deserialize)]
    struct Req { channel: u8 }
    let Ok(req) = serde_json::from_slice::<Req>(body) else {
        return serialize_error("invalid body");
    };
    let cc = crate::mcp::CcBridge::get_slots(instance)
        .ok()
        .and_then(|v| v.into_iter().find(|s| s.index == idx).and_then(|s| s.cc))
        .unwrap_or(0);
    match crate::mcp::CcBridge::map_slot(instance, idx, cc, req.channel) {
        Ok(()) => serialize_ok(None),
        Err(e) => serialize_error(e),
    }
}

fn serialize_ok(message: Option<&str>) -> String {
    let response = api::OkResponse {
        ok: true,
        message: message.map(String::from),
    };
    serde_json::to_string(&response).unwrap_or_else(|_| r#"{"ok":true}"#.to_string())
}

fn serialize_error(error: &str) -> String {
    let response = api::ErrorResponse {
        error: error.to_string(),
    };
    serde_json::to_string(&response).unwrap_or_else(|_| format!(r#"{{"error":"{}"}}"#, error))
}

// =============================================================================
// Settings Handlers
// =============================================================================

fn handle_get_settings() -> String {
    let response = api::get_settings();
    serde_json::to_string(&response).unwrap_or_else(|_| serialize_error("serialize failed"))
}

fn handle_update_settings(body: &[u8]) -> String {
    let Ok(req) = serde_json::from_slice::<crate::fugue::settings::UpdateSettingsRequest>(body) else {
        return serialize_error("invalid request body");
    };

    match api::update_settings(req) {
        Ok(response) => {
            crate::logger::log_gui_event("settings_updated", "export path");
            serde_json::to_string(&response).unwrap_or_else(|_| serialize_error("serialize failed"))
        }
        Err(e) => serialize_error(&e),
    }
}

fn handle_reveal_exports() -> String {
    match api::reveal_exports() {
        Ok(response) => {
            crate::logger::log_gui_event("reveal_exports", "opened folder");
            serde_json::to_string(&response).unwrap_or_else(|_| serialize_error("serialize failed"))
        }
        Err(e) => serialize_error(&e),
    }
}

// =============================================================================
// Export Handlers
// =============================================================================

/// POST /api/import_fugue — body is raw `.mid` bytes (no JSON envelope).
///
/// The wry custom-protocol route takes the instance from the URL path, so
/// `handle_import_fugue` uses `instance_id` directly and the query struct
/// is constructed with defaults. The axum HTTP route adds a `Query<...>`
/// layer on top when we need per-request options (tag_prefix, loop_mode,
/// etc.); this wry path keeps the simple defaults since the webview's only
/// caller (the instance drop zone) is happy with "loop forever, bar-quantize".
fn handle_import_fugue(body: &[u8], instance: &str) -> String {
    let query = api::ImportFugueQuery {
        instance: instance.to_string(),
        ..Default::default()
    };
    let response = api::import_fugue(instance, body, query);
    if response.ok {
        crate::logger::log_gui_event(
            "import_fugue",
            &format!("{} fugue(s)", response.fugue_ids.len()),
        );
    } else {
        log::warn!("routes /import_fugue failed: {:?}", response.error);
    }
    serde_json::to_string(&response).unwrap_or_else(|_| serialize_error("serialize failed"))
}

fn handle_export_fugue(body: &[u8], instance: &str) -> String {
    let Ok(req) = serde_json::from_slice::<api::ExportFugueRequest>(body) else {
        return serialize_error("invalid request body");
    };

    let response = api::export_fugue(instance, req);
    if response.ok {
        crate::logger::log_gui_event("export_fugue", response.path.as_deref().unwrap_or("success"));
    }
    serde_json::to_string(&response).unwrap_or_else(|_| serialize_error("serialize failed"))
}
