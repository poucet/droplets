use std::num::{NonZeroIsize, NonZeroU32};
use std::ptr::NonNull;

use clack_extensions::gui::*;
use clack_plugin::prelude::*;
use wry::{Rect, WebViewBuilder};
use wry::dpi::{LogicalSize, PhysicalPosition, Position};
use wry::raw_window_handle::{
    AppKitWindowHandle, WindowHandle, RawWindowHandle, Win32WindowHandle, XcbWindowHandle,
};

mod dpi;
use crate::DropletMainThread;
use crate::gui::dpi::{GuiSizeExtensions, LogicalSizeExtensions};



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
            // no known host supports floating mode at this time
            is_floating: false,
        })
    }

    fn create(&mut self, configuration: GuiConfiguration) -> Result<(), PluginError> {
        if !self.is_api_supported(configuration) {
            crate::logger::log_gui_event("create_failed", "Unsupported GUI configuration");
            return Err(PluginError::Message("Unsupported GUI configuration"));
        }
        crate::logger::log_gui_event("created", "GUI created successfully");
        Ok(())
    }

    fn destroy(&mut self) {
        self.gui.web_view.take();
        crate::logger::log_gui_event("destroyed", "GUI destroyed");
    }

    fn set_scale(&mut self, scale: f64) -> Result<(), PluginError> {
        self.gui.scale_factor = scale;
        crate::logger::log_gui_event("scale_changed", &format!("Scale set to: {}", scale));
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
        Ok(())
    }

    fn set_parent(&mut self, parent: Window) -> Result<(), PluginError> {
        crate::logger::log_gui_event("set_parent", "Setting parent window");
        
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


        crate::logger::log_gui_event("webview_building", "Starting WebView creation");
        
        match WebViewBuilder::new()
            // Load HTML from simplified GUI file
            .with_html(include_str!("../../gui.html"))
            .with_devtools(cfg!(debug_assertions) || cfg!(feature = "dev-gui"))
            .with_bounds(Rect {
                position: Position::Physical(PhysicalPosition::new(0, 0)),
                size: self.gui.size.to_webview_size(self.gui.scale_factor),
            })
            // Handle IPC messages from the web view
            .with_initialization_script(include_str!("script.js"))
            .with_ipc_handler({
                let sender = self.shared.ipc_sender.clone();
                move |request: wry::http::Request<String>| {
                    let message = request.body();
                    crate::logger::log_ipc_message_received(message);
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(message) {
                        crate::logger::log_ipc_message_parsed(&parsed);
                        if let Err(e) = sender.send(parsed) {
                            crate::logger::log_ipc_send_error(&e.to_string());
                        }
                    } else {
                        crate::logger::log_ipc_parse_error(message);
                    }
                }
            })
            .with_navigation_handler(|url| {
                crate::logger::log_gui_event("navigation", &format!("Navigation to: {}", url));
                if url.starts_with("http") {
                    if let Err(e) = open::that(url) {
                        crate::logger::log_error(&format!("Failed to open URL: {}", e));
                    }
                    false
                } else {
                    true
                }
            })
            .build_as_child(&parent_handle) {
                Ok(webview) => {
                    crate::logger::log_gui_event("webview_created", "WebView created successfully");
                    self.gui.web_view = Some(webview);
                },
                Err(e) => {
                    crate::logger::log_error(&format!("Failed to create WebView: {}", e));
                    return Err(PluginError::Message("Failed to create WebView"));
                }
            }

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
