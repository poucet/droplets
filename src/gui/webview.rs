//! Shared WebView configuration and builder
//!
//! Provides common WebView setup logic used by both the plugin GUI and standalone binary.

use crossbeam::channel::Sender;
use std::sync::Arc;
use wry::http::{header::CONTENT_TYPE, Response};
use wry::WebViewBuilder;

use crate::params::DropletParams;

use super::routes;

/// The custom protocol scheme for serving app content
const APP_PROTOCOL: &str = "droplets";
/// The origin URL for the app (enables secure context)
const APP_ORIGIN: &str = "droplets://localhost";

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
/// - Custom protocol handler for droplets:// (serves HTML, assets, and API)
/// - Initialization script
/// - IPC handler (if sender provided)
/// - Navigation handler (opens external links in browser)
/// - Devtools setting
pub fn configure_webview<'a>(
    builder: WebViewBuilder<'a>,
    params: Arc<DropletParams>,
    config: WebViewConfig,
) -> WebViewBuilder<'a> {
    // Add devtools
    let builder = builder.with_devtools(config.enable_devtools);

    // Add initialization script
    let builder = builder.with_initialization_script(include_str!("script.js"));

    // Add custom protocol handler that serves both HTML content and API
    let params_for_protocol = Arc::clone(&params);
    let dev_mode = config.dev_mode;
    let builder = builder
        .with_asynchronous_custom_protocol(
            APP_PROTOCOL.to_string(),
            move |_webview_id, request, responder| {
                let params = Arc::clone(&params_for_protocol);
                let uri = request.uri();
                let path = uri.path();
                let method = request.method().as_str();
                let body = request.body();

                #[cfg(any(debug_assertions, feature = "dev-gui"))]
                crate::logger::log_gui_event(
                    "protocol_request",
                    &format!("method={} uri={} path={:?}", method, uri, path),
                );

                // Serve HTML content for root path
                if path == "/" || path.is_empty() {
                    let html = get_html_content(dev_mode);
                    let response = Response::builder()
                        .header(CONTENT_TYPE, "text/html")
                        .body(html.into_bytes())
                        .unwrap();
                    responder.respond(response);
                    return;
                }

                // Serve static assets
                if let Some(response) = serve_static_asset(path, dev_mode) {
                    responder.respond(response);
                    return;
                }

                // Handle API requests
                let response_body = routes::handle_request(path, method, body, &params);
                let response = Response::builder()
                    .header(CONTENT_TYPE, "application/json")
                    .header("Access-Control-Allow-Origin", "*")
                    .body(response_body.into_bytes())
                    .unwrap();
                responder.respond(response);
            },
        )
        // Load content via custom protocol for secure context
        .with_url(APP_ORIGIN);

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

/// Get HTML content (from filesystem in dev mode, bundled otherwise)
fn get_html_content(dev_mode: bool) -> String {
    if dev_mode {
        let dev_path =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("frontend/dist/index.html");
        if dev_path.exists() {
            #[cfg(any(debug_assertions, feature = "dev-gui"))]
            crate::logger::log_gui_event(
                "webview_dev_mode",
                &format!("Loading HTML from: {:?}", dev_path),
            );
            if let Ok(html) = std::fs::read_to_string(&dev_path) {
                return html;
            }
        }
    }
    include_str!("../../frontend/dist/index.html").to_string()
}

/// Serve static assets (JS, CSS) from the frontend dist
fn serve_static_asset(path: &str, dev_mode: bool) -> Option<Response<Vec<u8>>> {
    // Determine content type from extension
    let content_type = if path.ends_with(".js") {
        "application/javascript"
    } else if path.ends_with(".css") {
        "text/css"
    } else if path.ends_with(".svg") {
        "image/svg+xml"
    } else if path.ends_with(".png") {
        "image/png"
    } else if path.ends_with(".woff2") {
        "font/woff2"
    } else if path.ends_with(".woff") {
        "font/woff"
    } else {
        return None;
    };

    // Try to load from filesystem in dev mode
    if dev_mode {
        let asset_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("frontend/dist")
            .join(path.trim_start_matches('/'));
        if asset_path.exists() {
            if let Ok(content) = std::fs::read(&asset_path) {
                #[cfg(any(debug_assertions, feature = "dev-gui"))]
                crate::logger::log_gui_event("serve_asset", &format!("Serving: {:?}", asset_path));
                return Some(
                    Response::builder()
                        .header(CONTENT_TYPE, content_type)
                        .body(content)
                        .unwrap(),
                );
            }
        }
    }

    // For bundled assets, we need to handle common asset paths
    // The bundled HTML references assets like /assets/index-xxx.js
    let bundled_content: Option<&[u8]> = match path {
        // Add bundled assets here if needed for production builds
        // For now, we rely on dev mode or inline assets
        _ => None,
    };

    bundled_content.map(|content| {
        Response::builder()
            .header(CONTENT_TYPE, content_type)
            .body(content.to_vec())
            .unwrap()
    })
}
