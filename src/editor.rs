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


fn create_fallback_html() -> String {
    r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <title>Simply Droplets</title>
    <style>
        body {
            font-family: 'Segoe UI', Tahoma, Geneva, Verdana, sans-serif;
            background: linear-gradient(135deg, #1e3c72, #2a5298);
            color: white;
            margin: 0;
            padding: 20px;
            display: flex;
            flex-direction: column;
            align-items: center;
            min-height: 100vh;
            box-sizing: border-box;
        }
        
        .container {
            background: rgba(255, 255, 255, 0.1);
            backdrop-filter: blur(10px);
            border: 1px solid rgba(255, 255, 255, 0.2);
            border-radius: 15px;
            padding: 30px;
            max-width: 600px;
            width: 100%;
            text-align: center;
        }
        
        h1 {
            margin: 0 0 20px 0;
            font-size: 2.5em;
            font-weight: 300;
            text-shadow: 0 2px 4px rgba(0,0,0,0.3);
        }
        
        .controls {
            display: flex;
            flex-direction: column;
            gap: 20px;
            margin-top: 30px;
        }
        
        .control {
            display: flex;
            align-items: center;
            justify-content: space-between;
            padding: 15px;
            background: rgba(255, 255, 255, 0.05);
            border-radius: 10px;
            border: 1px solid rgba(255, 255, 255, 0.1);
        }
        
        .control label {
            font-weight: 500;
            margin-right: 15px;
            min-width: 100px;
            text-align: left;
        }
        
        .control input[type="range"] {
            flex: 1;
            margin: 0 15px;
            height: 6px;
            background: rgba(255, 255, 255, 0.2);
            border-radius: 3px;
            outline: none;
            -webkit-appearance: none;
        }
        
        .control input[type="range"]::-webkit-slider-thumb {
            -webkit-appearance: none;
            width: 20px;
            height: 20px;
            background: #4CAF50;
            border-radius: 50%;
            cursor: pointer;
            box-shadow: 0 2px 4px rgba(0,0,0,0.3);
        }
        
        .control .value {
            min-width: 60px;
            text-align: right;
            font-weight: 500;
            font-family: 'Courier New', monospace;
        }
    </style>
</head>
<body>
    <div class="container">
        <h1>Simply Droplets</h1>
        <p>3D Droplet-Based Granular Synthesis</p>
        
        <div class="controls">
            <div class="control">
                <label>Gain:</label>
                <input type="range" id="gain" min="0" max="1" step="0.01" value="0.5">
                <span class="value" id="gain-value">0.5</span>
            </div>
            
            <div class="control">
                <label>Dry/Wet:</label>
                <input type="range" id="dry-wet" min="0" max="1" step="0.01" value="0.5">
                <span class="value" id="dry-wet-value">0.5</span>
            </div>
        </div>
    </div>

    <script>
        // Initialize communication with plugin
        function sendToPlugin(data) {
            if (window.ipc && window.ipc.postMessage) {
                window.ipc.postMessage(JSON.stringify(data));
            }
        }

        // Handle messages from plugin
        window.onPluginMessage = function(message) {
            try {
                const data = JSON.parse(message);
                handlePluginMessage(data);
            } catch (e) {
                console.error('Failed to parse plugin message:', e, message);
            }
        };

        function handlePluginMessage(data) {
            switch (data.type) {
                case 'init_response':
                    if (data.params) {
                        updateSlider('gain', data.params.gain);
                        updateSlider('dry-wet', data.params.dry_wet);
                    }
                    break;
                case 'param_change':
                    if (data.param === 'gain') {
                        updateSlider('gain', { value: data.value, text: data.text });
                    } else if (data.param === 'dry_wet') {
                        updateSlider('dry-wet', { value: data.value, text: data.text });
                    }
                    break;
            }
        }

        function updateSlider(id, paramData) {
            const slider = document.getElementById(id);
            const valueDisplay = document.getElementById(id + '-value');
            if (slider && valueDisplay && paramData) {
                slider.value = paramData.value;
                valueDisplay.textContent = paramData.text || paramData.value.toFixed(2);
            }
        }

        // Set up slider event handlers
        document.getElementById('gain').addEventListener('input', function(e) {
            const value = parseFloat(e.target.value);
            document.getElementById('gain-value').textContent = value.toFixed(2);
            sendToPlugin({ type: 'SetGain', value: value });
        });

        document.getElementById('dry-wet').addEventListener('input', function(e) {
            const value = parseFloat(e.target.value);
            document.getElementById('dry-wet-value').textContent = value.toFixed(2);
            sendToPlugin({ type: 'SetDryWet', value: value });
        });

        // Initialize plugin communication
        setTimeout(() => {
            sendToPlugin({ type: 'Init' });
        }, 100);
    </script>
</body>
</html>"#.to_string()
}