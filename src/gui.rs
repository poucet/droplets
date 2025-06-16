use std::num::{NonZeroIsize, NonZeroU32};
use std::ptr::NonNull;

use clack_extensions::gui::*;
use clack_plugin::prelude::*;
use wry::{Rect, WebViewBuilder};
use wry::dpi::{LogicalSize, PhysicalPosition, Position};
use wry::raw_window_handle::{
    AppKitWindowHandle, WindowHandle, RawWindowHandle, Win32WindowHandle, XcbWindowHandle,
};
use serde_json::json;

mod dpi;
use dpi::{GuiSizeExtensions, LogicalSizeExtensions};

use crate::DropletMainThread;
use crate::params::*;

// Include the React bundle generated at build time
include!(concat!(env!("OUT_DIR"), "/react_bundle.rs"));

pub const DEFAULT_GUI_SIZE: LogicalSize<f64> = LogicalSize::new(800.0, 600.0);
pub const MIN_GUI_SIZE: LogicalSize<f64> = LogicalSize::new(400.0, 300.0);
pub const MAX_GUI_SIZE: LogicalSize<f64> = LogicalSize::new(1200.0, 900.0);

pub struct DropletGui {
    size: LogicalSize<f64>,
    scale_factor: f64,
    web_view: Option<wry::WebView>,
}

impl DropletGui {
    pub fn new() -> Self {
        Self {
            size: DEFAULT_GUI_SIZE,
            scale_factor: 1.0,
            web_view: None,
        }
    }

    pub fn send_json(&mut self, message: serde_json::Value) -> Result<(), PluginError> {
        if let Some(web_view) = &mut self.web_view {
            let json_string = serde_json::to_string(&message)
                .map_err(|_e| PluginError::Message("Failed to serialize GUI message"))?;
            let script = format!("window.postMessage({}, '*');", json_string);
            web_view.evaluate_script(&script)?;
            Ok(())
        } else {
            Err(PluginError::Message("WebView not initialized"))
        }
    }

}

/// Implements the CLAP GUI extension
impl<'a> PluginGuiImpl for DropletMainThread<'a> {
    fn is_api_supported(&mut self, configuration: GuiConfiguration) -> bool {
        if let Some(preferred) = self.get_preferred_api() {
            configuration == preferred
        } else {
            false
        }
    }

    fn get_preferred_api(&mut self) -> Option<GuiConfiguration> {
        Some(GuiConfiguration {
            api_type: GuiApiType::default_for_current_platform()?,
            is_floating: false,
        })
    }

    fn create(&mut self, configuration: GuiConfiguration) -> Result<(), PluginError> {
        if !self.is_api_supported(configuration) {
            return Err(PluginError::Message("Unsupported GUI configuration"));
        }
        crate::logger::log_info("GUI created");
        Ok(())
    }

    fn destroy(&mut self) {
        self.gui.web_view.take();
        crate::logger::log_info("GUI destroyed");
    }

    fn set_scale(&mut self, scale: f64) -> Result<(), PluginError> {
        self.gui.scale_factor = scale;
        crate::logger::log_debug(&format!("GUI scale set to: {}", scale));
        Ok(())
    }

    fn get_size(&mut self) -> Option<GuiSize> {
        Some(self.gui.size.to_host_size(self.gui.scale_factor))
    }

    fn can_resize(&mut self) -> bool {
        true
    }

    fn get_resize_hints(&mut self) -> Option<GuiResizeHints> {
        Some(GuiResizeHints {
            can_resize_horizontally: true,
            can_resize_vertically: true,
            strategy: AspectRatioStrategy::Disregard,
        })
    }

    fn adjust_size(&mut self, size: GuiSize) -> Option<GuiSize> {
        let mut logical_size = size.to_logical(self.gui.scale_factor);

        // Constrain the size
        logical_size.width = logical_size.width.clamp(MIN_GUI_SIZE.width, MAX_GUI_SIZE.width);
        logical_size.height = logical_size.height.clamp(MIN_GUI_SIZE.height, MAX_GUI_SIZE.height);

        Some(logical_size.to_host_size(self.gui.scale_factor))
    }

    fn set_size(&mut self, size: GuiSize) -> Result<(), PluginError> {
        self.gui.size = size.to_logical(self.gui.scale_factor);
        if let Some(web_view) = &mut self.gui.web_view {
            web_view.set_bounds(Rect {
                position: Position::Physical(PhysicalPosition::new(0, 0)),
                size: self.gui.size.to_webview_size(self.gui.scale_factor),
            })?;
        }
        crate::logger::log_debug(&format!("GUI size set to: {}x{}", size.width, size.height));
        Ok(())
    }

    fn set_parent(&mut self, parent: Window) -> Result<(), PluginError> {
        crate::logger::log_info("Setting GUI parent window");

        // Convert CLAP window to WindowHandle expected by wry
        let parent_handle = unsafe {
            WindowHandle::borrow_raw(if cfg!(target_os = "macos") {
                RawWindowHandle::AppKit(AppKitWindowHandle::new(
                    NonNull::new(parent.as_cocoa_nsview().unwrap()).unwrap(),
                ))
            } else if cfg!(target_os = "windows") {
                RawWindowHandle::Win32(Win32WindowHandle::new(
                    NonZeroIsize::new(parent.as_win32_hwnd().unwrap() as isize).unwrap(),
                ))
            } else {
                RawWindowHandle::Xcb(XcbWindowHandle::new(
                    NonZeroU32::new(parent.as_x11_handle().unwrap() as u32).unwrap(),
                ))
            })
        };

        // Create a simple HTML container that will load React dynamically
        let html_content = r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Simply Droplets</title>
    <style>
        body {
            margin: 0;
            padding: 0;
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', 'Roboto', 'Oxygen',
                'Ubuntu', 'Cantarell', 'Fira Sans', 'Droid Sans', 'Helvetica Neue',
                sans-serif;
            -webkit-font-smoothing: antialiased;
            -moz-osx-font-smoothing: grayscale;
            background-color: #1a1a1a;
            color: #ffffff;
        }
        #loading {
            display: flex;
            justify-content: center;
            align-items: center;
            height: 100vh;
            font-size: 18px;
        }
    </style>
</head>
<body>
    <div id="loading">Loading Simply Droplets...</div>
    <div id="root"></div>
</body>
</html>"#;

        self.gui.web_view = Some(
            WebViewBuilder::new()
                .with_html(html_content)
                .with_devtools(cfg!(debug_assertions))
                .with_bounds(Rect {
                    position: Position::Physical(PhysicalPosition::new(0, 0)),
                    size: self.gui.size.to_webview_size(self.gui.scale_factor),
                })
                .with_initialization_script(&format!(r#"
                    // Setup IPC communication
                    window.ipc = {{
                        postMessage: function(message) {{
                            window.ipc.postMessage(message);
                        }}
                    }};
                    
                    // Load React bundle
                    function loadReactBundle() {{
                        try {{
                            // Create script element with the embedded React bundle
                            const script = document.createElement('script');
                            script.innerHTML = `{}`;
                            document.head.appendChild(script);
                            
                            console.log('React bundle loaded successfully');
                            
                            // Hide loading message after React is loaded
                            setTimeout(() => {{
                                const loading = document.getElementById('loading');
                                if (loading) loading.style.display = 'none';
                            }}, 100);
                        }} catch (error) {{
                            console.error('Error loading React bundle:', error);
                            const loading = document.getElementById('loading');
                            if (loading) {{
                                loading.innerHTML = 'Failed to load Simply Droplets UI';
                                loading.style.color = '#ff6b6b';
                            }}
                        }}
                    }}
                    
                    // Load bundle after DOM is ready
                    if (document.readyState === 'loading') {{
                        document.addEventListener('DOMContentLoaded', loadReactBundle);
                    }} else {{
                        loadReactBundle();
                    }}
                "#, REACT_BUNDLE.replace('`', r#"\`"#).replace("${", r#"\${"#)))
                .with_ipc_handler({
                    let sender = self.shared.ipc_sender.clone();
                    move |request: wry::http::Request<String>| {
                        let message = request.body();
                        crate::logger::log_debug(&format!("Received IPC message: {}", message));
                        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(message) {
                            if let Err(e) = sender.send(parsed) {
                                crate::logger::log_error(&format!("Failed to send IPC message: {}", e));
                            }
                        } else {
                            crate::logger::log_error(&format!("Failed to parse IPC message: {}", message));
                        }
                    }
                })
                .with_navigation_handler(|url| {
                    if url.starts_with("http") {
                        if let Err(e) = open::that(url) {
                            crate::logger::log_error(&format!("Failed to open URL: {}", e));
                        }
                        false
                    } else {
                        true
                    }
                })
                .build_as_child(&parent_handle)?,
        );

        // Send initial parameters to the frontend
        self.send_all_parameters_to_frontend()?;

        crate::logger::log_info("GUI webview created successfully");
        Ok(())
    }

    fn set_transient(&mut self, _window: Window) -> Result<(), PluginError> {
        Ok(())
    }

    fn show(&mut self) -> Result<(), PluginError> {
        Ok(())
    }

    fn hide(&mut self) -> Result<(), PluginError> {
        Ok(())
    }
}

impl<'a> DropletMainThread<'a> {
    pub fn send_all_parameters_to_frontend(&mut self) -> Result<(), PluginError> {
        let message = json!({
            "type": "AllParameters",
            "gain": self.shared.params.get_gain(),
            "grain_size": self.shared.params.get_grain_size(),
            "density": self.shared.params.get_density(),
            "time_warp": self.shared.params.get_time_warp(),
            "spatial_spread": self.shared.params.get_spatial_spread(),
            "dry_wet": self.shared.params.get_dry_wet()
        });
        self.gui.send_json(message)
    }

    pub fn send_parameter_change_to_frontend(&mut self, param_id: &str, value: f64) -> Result<(), PluginError> {
        let message = json!({
            "type": "ParameterChanged",
            "id": param_id,
            "value": value
        });
        self.gui.send_json(message)
    }

    pub fn handle_frontend_message(&mut self, message: &serde_json::Value) {
        if let Some(msg_type) = message.get("type").and_then(|t| t.as_str()) {
            match msg_type {
                "GetAllParameters" => {
                    if let Err(e) = self.send_all_parameters_to_frontend() {
                        crate::logger::log_error(&format!("Failed to send all parameters: {}", e));
                    }
                }
                "SetParameter" => {
                    if let (Some(param_id), Some(value)) = (
                        message.get("id").and_then(|id| id.as_str()),
                        message.get("value").and_then(|v| v.as_f64())
                    ) {
                        crate::logger::log_debug(&format!("Frontend setting parameter: {} = {}", param_id, value));
                        
                        // Convert frontend parameter names to IPC messages for the params module
                        let param_id_num = match param_id {
                            "gain" => PARAM_GAIN_ID.get() as u64,
                            "grain_size" => PARAM_GRAIN_SIZE_ID.get() as u64,
                            "density" => PARAM_DENSITY_ID.get() as u64,
                            "time_warp" => PARAM_TIME_WARP_ID.get() as u64,
                            "spatial_spread" => PARAM_SPATIAL_SPREAD_ID.get() as u64,
                            "dry_wet" => PARAM_DRY_WET_ID.get() as u64,
                            _ => {
                                crate::logger::log_warn(&format!("Unknown parameter from frontend: {}", param_id));
                                return;
                            }
                        };

                        let ipc_message = json!({
                            "type": "parameter_change",
                            "parameter_id": param_id_num,
                            "value": value
                        });

                        self.shared.params.handle_ipc_message(&ipc_message);
                    }
                }
                _ => {
                    crate::logger::log_warn(&format!("Unknown message type from frontend: {}", msg_type));
                }
            }
        }
    }
}