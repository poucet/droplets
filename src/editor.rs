use crate::{DropletParams, logger};
use nih_plug::prelude::*;
use nih_plug_webview::*;
use serde::Deserialize;
use serde_json::json;
use std::sync::atomic::Ordering;
use std::sync::Arc;

#[derive(Deserialize)]
#[serde(tag = "type")]
pub enum Action {
    Init,
    SetSize { width: u32, height: u32 },
    SetGain { value: f32 },
    SetDryWet { value: f32 },
}

pub fn create_droplet_editor(
    params: Arc<DropletParams>,
) -> Box<dyn Editor> {
    let params_clone = params.clone();
    let gain_changed_clone = params.gain_value_changed.clone();
    let dry_wet_changed_clone = params.dry_wet_value_changed.clone();

    let html_content = include_str!("../frontend/dist/index.html");

    logger::log_info("Loading webview from embedded HTML");

    let editor = WebViewEditor::new(HTMLSource::String(&html_content), (800, 600))
        .with_background_color((40, 40, 40, 255))
        .with_developer_mode(true)
        .with_keyboard_handler(move |event| {
            logger::log_info(&format!("Keyboard event: {:?}", event.key));
            event.key == Key::Escape
        })
        .with_mouse_handler(|event| match event {
            MouseEvent::DragEntered { .. } => {
                logger::log_info("Drag entered");
                EventStatus::AcceptDrop(DropEffect::Copy)
            }
            MouseEvent::DragMoved { .. } => EventStatus::AcceptDrop(DropEffect::Copy),
            MouseEvent::DragLeft => EventStatus::Ignored,
            MouseEvent::DragDropped { data, .. } => {
                if let DropData::Files(files) = data {
                    logger::log_info(&format!("Files dropped: {:?}", files));
                }
                EventStatus::AcceptDrop(DropEffect::Copy)
            }
            _ => EventStatus::Ignored,
        })
        .with_event_loop(move |ctx, setter, window| {
            // Handle incoming messages from frontend
            while let Ok(value) = ctx.next_event() {
                if let Ok(action) = serde_json::from_value(value) {
                    match action {
                        Action::SetGain { value } => {
                            setter.begin_set_parameter(&params_clone.gain);
                            setter.set_parameter_normalized(&params_clone.gain, value);
                            setter.end_set_parameter(&params_clone.gain);
                            logger::log_parameter_change("gain", value, value);
                        }
                        Action::SetDryWet { value } => {
                            setter.begin_set_parameter(&params_clone.dry_wet);
                            setter.set_parameter_normalized(&params_clone.dry_wet, value);
                            setter.end_set_parameter(&params_clone.dry_wet);
                            logger::log_parameter_change("dry_wet", value, value);
                        }
                        Action::SetSize { width, height } => {
                            ctx.resize(window, width, height);
                        }
                        Action::Init => {
                            // Send initial state to frontend
                            ctx.send_json(json!({
                                "type": "init_response",
                                "width": ctx.width.load(Ordering::Relaxed),
                                "height": ctx.height.load(Ordering::Relaxed),
                                "params": {
                                    "gain": {
                                        "value": params_clone.gain.unmodulated_normalized_value(),
                                        "text": params_clone.gain.to_string()
                                    },
                                    "dry_wet": {
                                        "value": params_clone.dry_wet.unmodulated_normalized_value(),
                                        "text": params_clone.dry_wet.to_string()
                                    }
                                }
                            }));
                        }
                    }
                } else {
                    logger::log_error("Invalid action received from web UI");
                }
            }

            // Send parameter updates to frontend when they change
            if gain_changed_clone.swap(false, Ordering::Relaxed) {
                ctx.send_json(json!({
                    "type": "param_change",
                    "param": "gain",
                    "value": params_clone.gain.unmodulated_normalized_value(),
                    "text": params_clone.gain.to_string()
                }));
            }

            if dry_wet_changed_clone.swap(false, Ordering::Relaxed) {
                ctx.send_json(json!({
                    "type": "param_change",
                    "param": "dry_wet",
                    "value": params_clone.dry_wet.unmodulated_normalized_value(),
                    "text": params_clone.dry_wet.to_string()
                }));
            }
        });

    Box::new(editor)
}


