//! Route handler for wry custom protocol (droplets://)
//!
//! This delegates to the shared API handlers in api.rs

use std::sync::Arc;
use crate::params::DropletParams;
use super::api;

/// Route an API request to the appropriate handler
/// Instance is determined from the plugin params (always "default" for wry context)
pub fn handle_request(path: &str, params: &Arc<DropletParams>) -> String {
    // In wry context, we use "default" instance since we're inside the plugin
    let instance = "default";

    match path {
        "/slots" => handle_slots(instance),
        "/activity" => handle_activity(),
        "/fugues" => handle_fugues(instance),
        "/transport" => handle_transport(instance),
        "/instances" => handle_instances(),
        "/cancel_learn" => handle_cancel_learn(instance, params),
        _ => handle_dynamic_route(path, instance, params),
    }
}

fn handle_dynamic_route(path: &str, instance: &str, params: &Arc<DropletParams>) -> String {
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

    serialize_error("not found")
}

// =============================================================================
// Handler implementations using shared API
// =============================================================================

fn handle_slots(instance: &str) -> String {
    match api::get_slots(instance) {
        Ok(response) => {
            let result = serde_json::to_string(&response).unwrap_or_else(|_| serialize_error("serialize failed"));
            crate::logger::log_gui_event("slots_response", &format!("{} slots", response.slots.len()));
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
// Helpers
// =============================================================================

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
