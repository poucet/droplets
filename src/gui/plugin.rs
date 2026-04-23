//! CLAP GUI extension implementation
//!
//! Implements PluginGuiImpl for DropletMainThread to provide embedded GUI support.

use std::num::{NonZeroIsize, NonZeroU32};
use std::ptr::NonNull;
use std::sync::Arc;
use log;

use clack_extensions::gui::*;
use clack_plugin::prelude::*;
use wry::dpi::{PhysicalPosition, Position};
use wry::raw_window_handle::{
    AppKitWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle, XcbWindowHandle,
};
use wry::{Rect, WebViewBuilder};

use super::dpi::{GuiSizeExtensions, LogicalSizeExtensions};
use super::gui::{MAX_GUI_SIZE, MIN_GUI_SIZE};
use super::webview::{configure_webview, WebViewConfig};
use crate::DropletMainThread;

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

        // Convert CLAP window to a raw handle we can reuse. Two consumers:
        // (1) wry's `build_as_child`, which takes a `WindowHandle<'_>`,
        // (2) `drag::start_drag` (Feature 15), which needs the handle to
        // stay accessible across IPC message processing.
        let raw_handle = if cfg!(target_os = "macos") {
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
        };
        // Stash for drag-out. The DAW owns the parent window for the
        // lifetime of the plugin, so the raw handle stays valid across
        // set_parent → drag → destroy.
        *self.shared.drag_state.lock().unwrap() = Some(super::drag::DragWindow::new(raw_handle));

        let parent_handle = unsafe { WindowHandle::borrow_raw(raw_handle) };

        crate::logger::log_gui_event("webview_building", "Starting WebView creation");

        // Use shared WebView configuration. Drag state shared with the
        // plugin-wide shared slot so the IPC handler inside the webview
        // can reach the parent window when a drag message comes in.
        let config = WebViewConfig::plugin(
            self.shared.ipc_sender.clone(),
            self.shared.instance_id.clone(),
            std::sync::Arc::clone(&self.shared.drag_state),
        );
        let builder = configure_webview(
            WebViewBuilder::new(),
            Arc::clone(&self.shared.params),
            config,
        );

        // Add plugin-specific bounds and build as child
        match builder
            .with_bounds(Rect {
                position: Position::Physical(PhysicalPosition::new(0, 0)),
                size: self.gui.size.to_webview_size(self.gui.scale_factor),
            })
            .build_as_child(&parent_handle)
        {
            Ok(webview) => {
                crate::logger::log_gui_event("webview_created", "WebView created successfully");
                self.gui.web_view = Some(webview);
            }
            Err(e) => {
                log::error!("Failed to create WebView: {}", e);
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