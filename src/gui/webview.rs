//! Shared WebView configuration and builder
//!
//! Provides common WebView setup logic used by both the plugin GUI and standalone binary.

use crossbeam::channel::Sender;
use std::sync::Arc;
use wry::http::{header::CONTENT_TYPE, Response};
use wry::WebViewBuilder;

use crate::params::DropletParams;

use super::routes;

/// Configuration for creating a WebView
pub struct WebViewConfig {
    /// Whether to enable devtools (usually debug builds only)
    pub enable_devtools: bool,
    /// Whether to use dev mode (load HTML from filesystem for hot reload)
    pub dev_mode: bool,
    /// Optional IPC message sender for handling incoming IPC messages
    pub ipc_sender: Option<Sender<serde_json::Value>>,
}

impl Default for WebViewConfig {
    fn default() -> Self {
        Self {
            enable_devtools: cfg!(debug_assertions) || cfg!(feature = "dev-gui"),
            dev_mode: cfg!(debug_assertions) || cfg!(feature = "dev-gui"),
            ipc_sender: None,
        }
    }
}

impl WebViewConfig {
    /// Create config for standalone mode
    pub fn standalone() -> Self {
        Self::default()
    }

    /// Create config for plugin mode with IPC sender
    pub fn plugin(ipc_sender: Sender<serde_json::Value>) -> Self {
        Self {
            ipc_sender: Some(ipc_sender),
            ..Self::default()
        }
    }

    /// Set IPC sender
    pub fn with_ipc_sender(mut self, sender: Sender<serde_json::Value>) -> Self {
        self.ipc_sender = Some(sender);
        self
    }
}

/// Configure a WebViewBuilder with shared settings
///
/// This applies the common configuration used by both plugin and standalone:
/// - HTML content (bundled or dev mode)
/// - Initialization script
/// - Custom protocol handler for droplets:// API
/// - IPC handler (if sender provided)
/// - Navigation handler (opens external links in browser)
/// - Devtools setting
pub fn configure_webview<'a>(
    builder: WebViewBuilder<'a>,
    params: Arc<DropletParams>,
    config: WebViewConfig,
) -> WebViewBuilder<'a> {
    // Start with HTML content
    let builder = configure_html(builder, config.dev_mode);

    // Add devtools
    let builder = builder.with_devtools(config.enable_devtools);

    // Add initialization script
    let builder = builder.with_initialization_script(include_str!("script.js"));

    // Add custom protocol handler
    let params_for_protocol = Arc::clone(&params);
    let builder = builder.with_asynchronous_custom_protocol(
        "droplets".to_string(),
        move |_webview_id, request, responder| {
            let params = Arc::clone(&params_for_protocol);
            let uri = request.uri();
            let path = uri.path();
            let method = request.method().as_str();
            let body = request.body();

            #[cfg(any(debug_assertions, feature = "dev-gui"))]
            crate::logger::log_gui_event(
                "api_request",
                &format!("method={} uri={} path={:?}", method, uri, path),
            );

            let response_body = routes::handle_request(path, method, body, &params);

            let response = Response::builder()
                .header(CONTENT_TYPE, "application/json")
                .header("Access-Control-Allow-Origin", "*")
                .body(response_body.into_bytes())
                .unwrap();
            responder.respond(response);
        },
    );

    // Add IPC handler if sender provided
    let builder = if let Some(sender) = config.ipc_sender {
        builder.with_ipc_handler(move |request: wry::http::Request<String>| {
            let sender = sender.clone();
            let message = request.body().clone();
            std::thread::spawn(move || {
                #[cfg(any(debug_assertions, feature = "dev-gui"))]
                crate::logger::log_ipc_message_received(&message);
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&message) {
                    let _ = sender.send(parsed);
                }
            });
        })
    } else {
        builder
    };

    // Add navigation handler
    builder.with_navigation_handler(|url| {
        #[cfg(any(debug_assertions, feature = "dev-gui"))]
        crate::logger::log_gui_event("navigation", &format!("Navigation to: {}", url));
        if url.starts_with("http") {
            if let Err(e) = open::that(url) {
                eprintln!("Failed to open URL: {}", e);
            }
            false
        } else {
            true
        }
    })
}

/// Configure HTML content for the WebView
fn configure_html(builder: WebViewBuilder<'_>, dev_mode: bool) -> WebViewBuilder<'_> {
    if dev_mode {
        // Try to load from file system for live editing
        let dev_path =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("frontend/dist/index.html");
        if dev_path.exists() {
            #[cfg(any(debug_assertions, feature = "dev-gui"))]
            crate::logger::log_gui_event(
                "webview_dev_mode",
                &format!("Loading from: {:?}", dev_path),
            );
            match std::fs::read_to_string(&dev_path) {
                Ok(html) => return builder.with_html(html),
                Err(e) => {
                    #[cfg(any(debug_assertions, feature = "dev-gui"))]
                    crate::logger::log_error(&format!("Failed to read dev HTML: {}", e));
                }
            }
        } else {
            #[cfg(any(debug_assertions, feature = "dev-gui"))]
            crate::logger::log_gui_event(
                "webview_dev_mode",
                "Dev path not found, using bundled HTML",
            );
        }
    }
    builder.with_html(include_str!("../../frontend/dist/index.html"))
}
