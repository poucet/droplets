use crate::{DropletParams, logger};
use nih_plug::prelude::*;
use std::sync::Arc;
use wry::{WebView, WebViewBuilder};
use std::ffi::c_void;

pub struct DropletEditor {
    params: Arc<DropletParams>,
    context: Arc<dyn GuiContext>,
}

// Wrapper to make WebView Send-safe
struct WebViewWrapper {
    webview: Option<WebView>,
}

unsafe impl Send for WebViewWrapper {}

impl WebViewWrapper {
    fn new(webview: WebView) -> Self {
        Self {
            webview: Some(webview),
        }
    }
}

impl DropletEditor {
    fn create_webview_for_parent(
        &self,
        parent: &ParentWindowHandle,
        url: &str,
    ) -> Result<WebView, Box<dyn std::error::Error>> {
        // Create webview as child of the parent window provided by the plugin host
        // This avoids creating our own event loop which would crash DAWs
        
        match parent {
            #[cfg(target_os = "windows")]
            ParentWindowHandle::Win32Hwnd(hwnd) => {
                self.create_windows_webview(*hwnd as *mut c_void, url)
            }
            #[cfg(target_os = "macos")]
            ParentWindowHandle::AppKitNsView(ns_view) => {
                self.create_macos_webview(*ns_view as *mut c_void, url)
            }
            _ => Err("Webview not supported on this platform yet".into()),
        }
    }
    
    #[cfg(target_os = "windows")]
    fn create_windows_webview(&self, hwnd: *mut c_void, url: &str) -> Result<WebView, Box<dyn std::error::Error>> {
        // Create a wrapper that implements HasWindowHandle for Windows
        struct WindowWrapper(pub *mut c_void);
        
        impl raw_window_handle::HasWindowHandle for WindowWrapper {
            fn window_handle(&self) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
                use raw_window_handle::{Win32WindowHandle, RawWindowHandle};
                let mut handle = Win32WindowHandle::new(std::num::NonZeroIsize::new(self.0 as isize).ok_or(raw_window_handle::HandleError::Unavailable)?);
                unsafe {
                    Ok(raw_window_handle::WindowHandle::borrow_raw(RawWindowHandle::Win32(handle)))
                }
            }
        }
        
        let wrapper = WindowWrapper(hwnd);
        let webview = WebViewBuilder::new()
            .with_url(url)
            .with_devtools(true)
            .build_as_child(&wrapper)?;
        Ok(webview)
    }
    
    #[cfg(target_os = "macos")]
    fn create_macos_webview(&self, ns_view: *mut c_void, url: &str) -> Result<WebView, Box<dyn std::error::Error>> {
        // Create a wrapper that implements HasWindowHandle for macOS
        struct WindowWrapper(pub *mut c_void);
        
        impl raw_window_handle::HasWindowHandle for WindowWrapper {
            fn window_handle(&self) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
                use raw_window_handle::{AppKitWindowHandle, RawWindowHandle};
                let handle = AppKitWindowHandle::new(std::ptr::NonNull::new(self.0).ok_or(raw_window_handle::HandleError::Unavailable)?);
                unsafe {
                    Ok(raw_window_handle::WindowHandle::borrow_raw(RawWindowHandle::AppKit(handle)))
                }
            }
        }
        
        let wrapper = WindowWrapper(ns_view);
        let webview = WebViewBuilder::new()
            .with_url(url)
            .with_devtools(true)
            .build_as_child(&wrapper)?;
        Ok(webview)
    }
}

impl DropletEditor {
    pub fn new(params: Arc<DropletParams>, context: Arc<dyn GuiContext>) -> Self {
        Self { params, context }
    }

    fn get_frontend_path() -> std::path::PathBuf {
        // Direct path to frontend/dist/index.html in project directory
        std::path::PathBuf::from("/Users/simplychris/projects/simply/droplets")
            .join("frontend")
            .join("dist")
            .join("index.html")
    }
}

impl Editor for DropletEditor {
    fn spawn(
        &self,
        parent: ParentWindowHandle,
        _context: Arc<dyn GuiContext>,
    ) -> Box<dyn std::any::Any + Send> {
        let frontend_path = Self::get_frontend_path();
        let url = format!("file://{}", frontend_path.display());
        
        logger::log_info(&format!("Loading webview from: {}", url));
        
        // Create webview as child of the parent window
        match self.create_webview_for_parent(&parent, &url) {
            Ok(webview) => {
                logger::log_info("Webview created successfully as child window");
                Box::new(WebViewWrapper::new(webview))
            }
            Err(e) => {
                logger::log_error(&format!("Failed to create webview: {}", e));
                logger::log_info("Falling back to placeholder UI");
                Box::new(())
            }
        }
    }

    fn size(&self) -> (u32, u32) {
        (800, 600)
    }

    fn set_scale_factor(&self, _factor: f32) -> bool {
        false
    }

    fn param_value_changed(&self, id: &str, normalized_value: f32) {
        logger::log_parameter_change(id, normalized_value, normalized_value);
    }

    fn param_modulation_changed(&self, _id: &str, _modulation_offset: f32) {
        // TODO: Handle modulation changes
    }

    fn param_values_changed(&self) {
        // TODO: Handle bulk parameter updates
    }
}