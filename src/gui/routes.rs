use std::sync::Arc;
use crate::params::DropletParams;

/// Route an API request to the appropriate handler
pub fn handle_request(path: &str, params: &Arc<DropletParams>) -> String {
    match path {
        "/slots" => get_slots(params),
        "/activity" => get_activity(),
        "/cancel_learn" => cancel_learn(params),
        _ => handle_dynamic_route(path, params),
    }
}

fn handle_dynamic_route(path: &str, params: &Arc<DropletParams>) -> String {
    if let Some(slot_str) = path.strip_prefix("/start_learn/") {
        return start_learn(slot_str, params);
    }

    if let Some(slot_str) = path.strip_prefix("/wiggle/") {
        return wiggle(slot_str, params);
    }

    // /note_on/{note}/{velocity} - trigger a note
    if let Some(rest) = path.strip_prefix("/note_on/") {
        return note_on(rest);
    }

    // /note_off/{note} - release a note
    if let Some(note_str) = path.strip_prefix("/note_off/") {
        return note_off(note_str);
    }

    r#"{"error":"not found"}"#.to_string()
}

fn get_slots(params: &Arc<DropletParams>) -> String {
    let slots = params.get_all_slots();
    let json = serde_json::json!({
        "type": "slots",
        "data": slots
    });
    let result = serde_json::to_string(&json)
        .unwrap_or_else(|_| r#"{"error":"serialize failed"}"#.to_string());
    crate::logger::log_gui_event("slots_response", &format!("{} slots, len={}", slots.len(), result.len()));
    result
}

fn get_activity() -> String {
    let activity = crate::mcp::CcBridge::recent_activity();
    serde_json::to_string(&serde_json::json!({
        "type": "activity",
        "data": activity.iter().map(|e| {
            serde_json::json!({
                "timestamp": e.timestamp_ms,
                "instance": e.instance,
                "channel": e.channel + 1,
                "cc": e.cc,
                "value": e.value
            })
        }).collect::<Vec<_>>()
    })).unwrap_or_else(|_| r#"{"error":"serialize failed"}"#.to_string())
}

fn start_learn(slot_str: &str, params: &Arc<DropletParams>) -> String {
    let Ok(slot) = slot_str.parse::<usize>() else {
        return r#"{"error":"invalid slot"}"#.to_string();
    };

    params.start_learning(slot);
    crate::logger::log_gui_event("learn_started", &format!("Slot {}", slot));
    r#"{"ok":true}"#.to_string()
}

fn cancel_learn(params: &Arc<DropletParams>) -> String {
    params.cancel_learning();
    crate::logger::log_gui_event("learn_cancelled", "All slots");
    r#"{"ok":true}"#.to_string()
}

fn wiggle(slot_str: &str, params: &Arc<DropletParams>) -> String {
    let Ok(slot) = slot_str.parse::<usize>() else {
        return r#"{"error":"invalid slot"}"#.to_string();
    };

    let Some(info) = params.get_all_slots().into_iter().find(|s| s.index == slot) else {
        return r#"{"error":"slot not found"}"#.to_string();
    };

    let Some(cc) = info.cc else {
        return r#"{"error":"slot not mapped to CC"}"#.to_string();
    };

    // Spawn thread to wiggle for ~1 second
    let channel = info.channel;
    std::thread::spawn(move || {
        for i in 0..6 {
            let value = if i % 2 == 0 { 127u8 } else { 0u8 };
            let msg = crate::mcp::CcMessage::new(channel, cc, value);
            let _ = crate::mcp::CcBridge::send("default", msg);
            std::thread::sleep(std::time::Duration::from_millis(150));
        }
        // Return to center
        let msg = crate::mcp::CcMessage::new(channel, cc, 64);
        let _ = crate::mcp::CcBridge::send("default", msg);
    });

    crate::logger::log_gui_event("wiggle_started", &format!("Slot {} CC{}", slot, cc));
    format!(r#"{{"ok":true,"cc":{}}}"#, cc)
}

fn note_on(rest: &str) -> String {
    // Parse note/velocity from path
    let parts: Vec<&str> = rest.split('/').collect();
    let (note, velocity) = match parts.as_slice() {
        [note_str, vel_str] => {
            let Ok(note) = note_str.parse::<u8>() else {
                return r#"{"error":"invalid note"}"#.to_string();
            };
            let Ok(vel) = vel_str.parse::<u8>() else {
                return r#"{"error":"invalid velocity"}"#.to_string();
            };
            (note.min(127), vel.min(127))
        }
        [note_str] => {
            let Ok(note) = note_str.parse::<u8>() else {
                return r#"{"error":"invalid note"}"#.to_string();
            };
            (note.min(127), 100u8) // default velocity
        }
        _ => return r#"{"error":"invalid path"}"#.to_string(),
    };

    let msg = crate::mcp::NoteMessage::new(0, note, velocity, true);
    match crate::mcp::CcBridge::send_note("default", msg) {
        Ok(()) => {
            crate::logger::log_gui_event("note_on", &format!("Note {} vel {}", note, velocity));
            format!(r#"{{"ok":true,"note":{},"velocity":{}}}"#, note, velocity)
        }
        Err(e) => format!(r#"{{"error":"{}"}}"#, e),
    }
}

fn note_off(note_str: &str) -> String {
    let Ok(note) = note_str.parse::<u8>() else {
        return r#"{"error":"invalid note"}"#.to_string();
    };
    let note = note.min(127);

    let msg = crate::mcp::NoteMessage::new(0, note, 0, false);
    match crate::mcp::CcBridge::send_note("default", msg) {
        Ok(()) => {
            crate::logger::log_gui_event("note_off", &format!("Note {}", note));
            format!(r#"{{"ok":true,"note":{}}}"#, note)
        }
        Err(e) => format!(r#"{{"error":"{}"}}"#, e),
    }
}
