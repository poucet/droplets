//! MCP Server module for Droplets
//!
//! Provides AI control of MIDI CC output via the Model Context Protocol.
//!
//! ## Architecture
//!
//! AI Assistant
//!     |
//!     | HTTP (MCP over Streamable HTTP)
//!     v
//! MCP Server (singleton, localhost:9999/mcp)
//!     |
//!     | CcBridge::send() -> rtrb ring buffer (lock-free)
//!     v
//! Plugin Instance (audio thread)
//!     |
//!     | MIDI CC output
//!     v
//! DAW routes to target plugin

mod bridge;
pub mod project;
pub mod types;
mod server;

pub use bridge::{ActivityEvent, CcBridge, CcMessage, MidiMessage, NoteMessage, PerNoteExpressionMessage, PerNoteExpressionType};
pub use server::DropletsMcp;

use rmcp::transport::streamable_http_server::{
    StreamableHttpService, StreamableHttpServerConfig,
    session::local::LocalSessionManager,
};
use axum::{
    Router,
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    http::StatusCode,
    response::IntoResponse,
};
use std::sync::{Arc, OnceLock};
use std::net::SocketAddr;
use tokio::sync::broadcast;

/// Singleton flag to ensure only one server starts
static SERVER_STARTED: OnceLock<()> = OnceLock::new();

/// Broadcast channel for `ControllerCommand`s streamed to the host
/// controller extension via `/ws/controller`. Process-wide: any MCP tool
/// that needs to push a command sends on this sender, and every connected
/// WebSocket subscriber receives a copy. Bounded capacity (drops oldest on
/// overflow — acceptable for DAW-config commands that don't need
/// latency-critical delivery).
const CONTROLLER_CHANNEL_CAPACITY: usize = 64;
static CONTROLLER_TX: OnceLock<broadcast::Sender<project::ControllerCommand>> = OnceLock::new();

fn controller_tx() -> &'static broadcast::Sender<project::ControllerCommand> {
    CONTROLLER_TX.get_or_init(|| broadcast::channel(CONTROLLER_CHANNEL_CAPACITY).0)
}

/// Send a command to any connected host controller extension. v1 doesn't
/// emit commands from MCP tools yet; callers that do can use this.
#[allow(dead_code)]
pub fn send_controller_command(cmd: project::ControllerCommand) {
    // Ignored if no subscribers — the broadcast API returns Err in that
    // case, which isn't an error for our semantics.
    let _ = controller_tx().send(cmd);
}

/// Default port for the MCP server (plugin)
pub const DEFAULT_MCP_PORT: u16 = 9999;

/// Default port for the MCP server (standalone)
pub const STANDALONE_MCP_PORT: u16 = 9997;

/// Start the singleton MCP server.
///
/// This is safe to call multiple times - only the first call actually starts the server.
/// Subsequent calls are no-ops.
pub fn start_server(port: u16) {
    SERVER_STARTED.get_or_init(|| {
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .thread_name("mcp-server")
                .build()
                .expect("Failed to create tokio runtime for MCP server");

            runtime.block_on(run_server(port));
        });
        log::info!("MCP server thread spawned");
    });
}

/// Run the MCP server (called from the spawned thread)
async fn run_server(port: u16) {
    let addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();
    log::info!("MCP server starting on http://{}/mcp", addr);

    // Create session manager for stateful connections
    let session_manager = Arc::new(LocalSessionManager::default());
    log::debug!("MCP: Created session manager");

    // Create the streamable HTTP service
    // Use stateless mode - each request creates a fresh service instance
    let config = StreamableHttpServerConfig {
        stateful_mode: false,
        sse_keep_alive: Some(std::time::Duration::from_secs(30)),
    };
    log::debug!("MCP: Created config with stateful_mode=false (stateless)");

    let mcp_service = StreamableHttpService::new(
        || {
            log::debug!("MCP: Creating new DropletsMcp service instance");
            Ok(DropletsMcp::new())
        },
        session_manager,
        config,
    );
    log::debug!("MCP: Created StreamableHttpService");

    // Build the axum router with logging
    let app = Router::new()
        .route("/mcp", axum::routing::any(move |req: axum::http::Request<axum::body::Body>| {
            let service = mcp_service.clone();
            async move {
                let method = req.method().clone();
                let uri = req.uri().clone();
                let headers = req.headers().clone();

                log::info!("MCP request: {} {}", method, uri);
                log::debug!("MCP request headers: {:?}", headers);

                let response = service.handle(req).await;

                log::info!("MCP response status: {}", response.status());
                log::debug!("MCP response headers: {:?}", response.headers());

                response
            }
        }))
        // Host controller extensions (Bitwig, Ableton) push project state here.
        // Treated as trusted local traffic — no auth because the server binds
        // only to 127.0.0.1.
        .route("/project_layout", axum::routing::post(handle_project_layout))
        // Extensions also POST /rename_instance on first sight of a Droplets
        // device so the plugin instance name matches the DAW track name.
        // Same semantics as the GUI server's /api/rename_instance — just
        // mirrored here so the extension only needs to know port 9999.
        .route("/rename_instance", axum::routing::post(handle_rename_instance))
        // WebSocket stream of ControllerCommands for the host extension.
        // v1 emits nothing; the endpoint exists so the extension can
        // establish its side of the pipe and so future MCP tools can enqueue
        // commands without a protocol change.
        .route("/ws/controller", axum::routing::any(handle_controller_ws));

    log::info!("MCP server listening on http://{}/mcp", addr);

    // Run the server
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => {
            log::info!("MCP: Successfully bound to {}", addr);
            l
        },
        Err(e) => {
            log::error!("Failed to bind MCP server on {}: {}", addr, e);
            log::error!("Another instance may already be running, or the port is in use");
            return;
        }
    };

    if let Err(e) = axum::serve(listener, app).await {
        log::error!("MCP server error: {}", e);
    }

    log::info!("MCP server stopped");
}

/// Check if the MCP server has been started
pub fn is_server_running() -> bool {
    SERVER_STARTED.get().is_some()
}

/// `GET /ws/controller` — upgrade to a WebSocket that streams
/// `ControllerCommand`s from the plugin process to a host controller
/// extension. Each command is serialized as a JSON text frame.
///
/// v1 sends nothing. The connection exists so the extension can open and
/// keep its side of the pipe; when MCP tools start emitting commands
/// (future MIDI-mapping tool etc.), no protocol change is needed.
async fn handle_controller_ws(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(controller_ws_loop)
}

async fn controller_ws_loop(mut socket: WebSocket) {
    let mut rx = controller_tx().subscribe();
    log::info!("controller WebSocket connected");

    loop {
        tokio::select! {
            // Forward commands to the extension as JSON text frames.
            cmd = rx.recv() => {
                match cmd {
                    Ok(cmd) => {
                        let Ok(json) = serde_json::to_string(&cmd) else {
                            log::warn!("controller WS: failed to serialize command");
                            continue;
                        };
                        if socket.send(Message::Text(json.into())).await.is_err() {
                            log::info!("controller WebSocket disconnected (send failed)");
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        log::warn!("controller WS: dropped {} commands due to backlog", n);
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            // Drain anything the client sends so close frames and pings are
            // handled; we don't process inbound content yet.
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => {
                        log::info!("controller WebSocket closed by peer");
                        break;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        log::info!("controller WebSocket recv error: {}", e);
                        break;
                    }
                }
            }
        }
    }
}

/// `POST /project_layout` — accept a full ProjectLayout from a host
/// controller extension and store it for the MCP tools to read.
///
/// Body is a JSON [`project::ProjectLayout`]. Replies 200 on success, 400 on
/// parse error. Failures are logged rather than surfaced to the extension
/// beyond a status code — the extension should retry on error with backoff.
/// `POST /rename_instance` — mirror of the GUI-server route on the MCP
/// port. Extensions POST `{ instance, name }`; we look up and rename via
/// `CcBridge::rename` (same code path as the `set_instance_name` MCP tool).
async fn handle_rename_instance(body: axum::body::Bytes) -> impl IntoResponse {
    #[derive(serde::Deserialize)]
    struct Req {
        instance: String,
        name: String,
    }
    let Ok(req) = serde_json::from_slice::<Req>(&body) else {
        log::warn!("rename_instance: invalid body");
        return (StatusCode::BAD_REQUEST, "invalid body").into_response();
    };
    match CcBridge::rename(&req.instance, &req.name) {
        Ok(old) => {
            log::info!(
                "rename_instance: '{}' -> '{}' (was '{}')",
                req.instance, req.name, old
            );
            (StatusCode::OK, "ok").into_response()
        }
        Err(e) => {
            log::warn!("rename_instance {}: {}", req.instance, e);
            (StatusCode::NOT_FOUND, e).into_response()
        }
    }
}

async fn handle_project_layout(
    body: axum::body::Bytes,
) -> impl IntoResponse {
    match serde_json::from_slice::<project::ProjectLayout>(&body) {
        Ok(layout) => {
            log::info!(
                "project_layout received: {} tracks",
                layout.tracks.len()
            );
            CcBridge::set_project_layout(layout.clone());
            // Also push to the GUI WebSocket broadcast so any open UI tab
            // updates without polling. Deliberately a direct call rather
            // than a cross-crate trait — the two servers live in the same
            // binary.
            crate::gui::server::broadcast_project_layout(layout);
            (StatusCode::OK, "ok").into_response()
        }
        Err(e) => {
            log::warn!("project_layout parse error: {}", e);
            (StatusCode::BAD_REQUEST, format!("parse error: {}", e)).into_response()
        }
    }
}
