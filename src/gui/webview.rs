//! Shared WebView configuration and builder
//!
//! Provides common WebView setup logic used by both the plugin GUI and standalone binary.

use crossbeam::channel::Sender;
use std::sync::Arc;
use wry::http::{header::CONTENT_TYPE, Response};
use wry::WebViewBuilder;

use crate::params::DropletParams;

use super::drag::{self, DragState};
use super::routes;

/// The custom protocol scheme for serving app content
const APP_PROTOCOL: &str = "droplets";
/// The origin URL for the app (enables secure context)
const APP_ORIGIN: &str = "droplets://localhost";

/// Decode a percent-encoded query-string value. Minimal implementation —
/// we only use this to parse `?instance=…` where the values are
/// `droplets-xxxxxxxx` (no special chars) or user-chosen track names
/// (encodeURIComponent'd, commonly containing `%20`). Not a full URL
/// decoder — good enough and avoids pulling in `percent-encoding` as a
/// direct dep.
fn simple_percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = &s[i + 1..i + 3];
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        // `+` is not URL-encoded by encodeURIComponent, so we leave it.
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Configuration for creating a WebView
pub struct WebViewConfig {
    /// Whether to enable devtools (usually debug builds only)
    pub enable_devtools: bool,
    /// Whether to use dev mode (load HTML from filesystem for hot reload)
    pub dev_mode: bool,
    /// Optional IPC message sender for handling incoming IPC messages
    pub ipc_sender: Option<Sender<serde_json::Value>>,
    /// Instance ID of this plugin window (used by /self endpoint)
    pub instance_id: String,
    /// Parent-window handle slot used by native drag-out (Feature 15).
    /// `None` in standalone mode (we don't own the surrounding window
    /// and drag-out isn't wired up there yet).
    pub drag_state: Option<DragState>,
}

impl WebViewConfig {
    /// Create config for standalone mode
    pub fn standalone(instance_id: impl Into<String>) -> Self {
        Self {
            enable_devtools: cfg!(debug_assertions) || cfg!(feature = "dev-gui"),
            dev_mode: cfg!(debug_assertions) || cfg!(feature = "dev-gui"),
            ipc_sender: None,
            instance_id: instance_id.into(),
            drag_state: None,
        }
    }

    /// Create config for plugin mode with IPC sender
    pub fn plugin(
        ipc_sender: Sender<serde_json::Value>,
        instance_id: impl Into<String>,
        drag_state: DragState,
    ) -> Self {
        Self {
            enable_devtools: cfg!(debug_assertions) || cfg!(feature = "dev-gui"),
            dev_mode: cfg!(debug_assertions) || cfg!(feature = "dev-gui"),
            ipc_sender: Some(ipc_sender),
            instance_id: instance_id.into(),
            drag_state: Some(drag_state),
        }
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
    let instance_id_for_protocol = config.instance_id.clone();
    let builder = builder
        .with_asynchronous_custom_protocol(
            APP_PROTOCOL.to_string(),
            move |_webview_id, request, responder| {
                let params = Arc::clone(&params_for_protocol);
                let own_instance_id = instance_id_for_protocol.clone();
                let uri = request.uri();
                let path = uri.path();
                let method = request.method().as_str();
                let body = request.body();

                // Parse ?instance=... from the query string. The frontend
                // sends this on every API call so the UI can target any
                // connected instance (not just the webview's owner).
                // Falls back to the webview's own instance when missing.
                let instance_id = uri
                    .query()
                    .and_then(|q| {
                        q.split('&')
                            .find_map(|pair| pair.strip_prefix("instance="))
                    })
                    .map(simple_percent_decode)
                    .unwrap_or_else(|| own_instance_id.clone());

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
                let response_body = routes::handle_request(path, method, body, &params, &instance_id);
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

    // Add IPC handler if sender provided.
    //
    // Drag-start messages get special treatment: they run **synchronously**
    // on this thread (the WebView's UI thread on macOS/Windows), because
    // `drag::start_drag` has to be called during the same mouse-down
    // gesture that triggered the IPC. Every other message keeps the
    // original thread-spawn + channel-send path.
    let drag_state = config.drag_state.clone();
    let instance_id_for_ipc = config.instance_id.clone();
    let builder = if let Some(sender) = config.ipc_sender {
        builder.with_ipc_handler(move |request: wry::http::Request<String>| {
            let message = request.body().clone();

            #[cfg(any(debug_assertions, feature = "dev-gui"))]
            crate::logger::log_ipc_message_received(&message);

            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&message) {
                // Route drag-start inline; everything else goes through
                // the channel so the existing consumer logic sees it.
                if parsed.get("type").and_then(|v| v.as_str()) == Some("start_drag") {
                    if let Some(state) = &drag_state {
                        handle_start_drag(state, &instance_id_for_ipc, &parsed);
                    } else {
                        log::warn!("start_drag IPC received without drag_state configured");
                    }
                    return;
                }
            }

            let sender = sender.clone();
            std::thread::spawn(move || {
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

/// Handle a `start_drag` IPC message from the frontend. Expected shape:
///
/// ```json
/// { "type": "start_drag",
///   "instance": "<id or name>",      // optional — defaults to own instance
///   "fugue_ids": ["123", "456"],     // optional — pick specific fugues
///   "active": true,                  // optional — export all active fugues
///   "tempo_bpm": 120 }               // optional — override session tempo
/// ```
///
/// Resolves the active fugues, writes a `.mid` file to the OS temp dir,
/// starts a native OS drag with that file. On Linux (drag unsupported) or
/// drag failure, reveals the file in the user's file manager so the user
/// can drag it from there (Feature 15b.5 fallback).
fn handle_start_drag(
    drag_state: &DragState,
    own_instance_id: &str,
    payload: &serde_json::Value,
) {
    use crate::fugue::{export, FugueBridge};

    // Which instance to export from? Defaults to the webview's own.
    let instance = payload
        .get("instance")
        .and_then(|v| v.as_str())
        .unwrap_or(own_instance_id);

    // Pick the fugue set: explicit ids > active = true > nothing.
    let definitions: Vec<_> = match FugueBridge::get_definitions(instance) {
        Ok(defs) => defs,
        Err(e) => {
            log::warn!("start_drag: get_definitions('{}') failed: {}", instance, e);
            return;
        }
    };

    let selected: Vec<_> = if let Some(ids) = payload.get("fugue_ids").and_then(|v| v.as_array()) {
        let wanted: std::collections::HashSet<u64> = ids
            .iter()
            .filter_map(|v| v.as_str())
            .filter_map(|s| s.parse::<u64>().ok())
            .collect();
        definitions
            .into_iter()
            .filter(|d| wanted.contains(&d.id))
            .collect()
    } else {
        // "active: true" or anything else → everything currently queued.
        definitions
    };

    if selected.is_empty() {
        log::warn!("start_drag: nothing to export on instance '{}'", instance);
        return;
    }

    // Tempo: payload override if present, else default 120. Future
    // improvement: ask the FugueBridge for the current transport tempo.
    let tempo_bpm = payload
        .get("tempo_bpm")
        .and_then(|v| v.as_f64())
        .unwrap_or(120.0);

    let bytes = export::fugues_to_smf(&selected, tempo_bpm);

    // Materialize in the OS temp dir so the DAW can copy it before we
    // clean up. Filename includes a timestamp so concurrent drags don't
    // collide.
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let path = std::env::temp_dir().join(format!("droplets-{}.mid", ts));
    if let Err(e) = std::fs::write(&path, &bytes) {
        log::warn!("start_drag: failed to write temp .mid {}: {}", path.display(), e);
        return;
    }

    log::info!(
        "start_drag: wrote {} fugues ({} bytes) to {}",
        selected.len(),
        bytes.len(),
        path.display()
    );

    // Drag, with file-manager reveal as fallback.
    match drag::start_file_drag(drag_state, path.clone()) {
        drag::DragStart::Started => {
            log::info!("start_drag: native drag started for {}", path.display());
        }
        drag::DragStart::Unsupported => {
            log::info!(
                "start_drag: native drag unsupported on this platform — revealing {} in file manager",
                path.display()
            );
            drag::reveal_fallback(&path);
        }
        drag::DragStart::NoWindow => {
            log::warn!(
                "start_drag: no parent window stashed — revealing {} in file manager as fallback",
                path.display()
            );
            drag::reveal_fallback(&path);
        }
        drag::DragStart::Failed(e) => {
            log::warn!(
                "start_drag: drag crate returned error '{}' — revealing {} in file manager as fallback",
                e,
                path.display()
            );
            drag::reveal_fallback(&path);
        }
    }

    // Temp file cleanup: spawn a detached thread that deletes after 60s.
    // Gives the DAW time to copy the file into its own storage before we
    // pull the rug out. Best-effort; a missing file at cleanup time is
    // fine (the user might have moved it).
    let cleanup_path = path;
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(60));
        let _ = std::fs::remove_file(&cleanup_path);
    });
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
