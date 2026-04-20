//! GUI Server - HTTP + WebSocket server for standalone UI
//!
//! Provides:
//! - HTTP serving of frontend assets
//! - REST API endpoints with instance selection
//! - WebSocket endpoint for real-time transport/fugue updates
//!
//! This allows running the UI in a browser connecting to the VST running in a DAW.

use std::net::SocketAddr;
use std::sync::OnceLock;
use std::time::Duration;

use axum::{
    Json, Router,
    extract::{Path, Query, ws::{Message, WebSocket, WebSocketUpgrade}},
    http::{header, StatusCode},
    response::{Html, IntoResponse},
    routing::{get, post},
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::fugue::{FugueBridge, TransportState};
use crate::mcp::{CcBridge, project::ProjectLayout};
use super::api;

/// Broadcast channel for immediate project-layout updates to all connected
/// GUI WebSocket sessions. Pushed to from the `/api/project_layout` POST
/// handler and subscribed to by each `ws_handler` session, so a layout
/// change in the host extension reaches every open UI tab in one hop (no
/// polling).
///
/// Capacity is low because the payload is cheap and bursts coalesce at the
/// extension side — we don't need large backlogs. Slow consumers that lag
/// drop the oldest messages via broadcast's `Lagged` error, which is fine:
/// we only care about the *latest* layout.
const PROJECT_LAYOUT_CHANNEL_CAPACITY: usize = 16;
static PROJECT_LAYOUT_TX: OnceLock<broadcast::Sender<ProjectLayout>> = OnceLock::new();

fn project_layout_tx() -> &'static broadcast::Sender<ProjectLayout> {
    PROJECT_LAYOUT_TX.get_or_init(|| broadcast::channel(PROJECT_LAYOUT_CHANNEL_CAPACITY).0)
}

/// Broadcast a new project layout to every connected GUI WebSocket client.
///
/// Called from `POST /api/project_layout` (GUI server) and from the MCP
/// server's `POST /project_layout` handler — both land in Droplets via
/// the host controller extension, and both need to reach the UI. Silent
/// no-op when no clients are connected.
pub fn broadcast_project_layout(layout: ProjectLayout) {
    let track_count = layout.tracks.len();
    match project_layout_tx().send(layout) {
        Ok(n) => log::info!(
            "broadcast_project_layout: sent {} tracks to {} WS subscriber(s)",
            track_count, n
        ),
        Err(_) => log::info!(
            "broadcast_project_layout: {} tracks stored, 0 WS subscribers (plugin-mode webview uses IPC instead)",
            track_count
        ),
    }
}

/// Default port for the GUI server (plugin)
pub const DEFAULT_GUI_PORT: u16 = 9998;

/// Default port for the GUI server (standalone)
pub const STANDALONE_GUI_PORT: u16 = 9996;

/// Singleton flag to ensure only one server starts
static SERVER_STARTED: OnceLock<()> = OnceLock::new();

/// Query parameter for instance selection
#[derive(Debug, Deserialize)]
pub struct InstanceQuery {
    #[serde(default = "default_instance")]
    instance: String,
}

fn default_instance() -> String {
    "default".to_string()
}

/// WebSocket message types sent to the frontend.
///
/// Transport + fugue updates are tagged with `instance_id` so a single WS
/// subscription covers every connected instance. The frontend keeps a
/// per-instance map and renders whichever one the user has selected —
/// no reconnect needed when the instance dropdown changes.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsMessage {
    /// Transport state update (sent frequently for smooth playhead)
    Transport {
        instance_id: String,
        transport: crate::fugue::TransportState,
    },
    /// Fugue list update (sent when fugues change)
    Fugues {
        instance_id: String,
        infos: Vec<crate::fugue::FugueInfo>,
        definitions: Vec<crate::fugue::FugueDefinition>,
    },
    /// Project layout update from the host controller extension. Pushed
    /// immediately on every `POST /api/project_layout` — no polling, so
    /// the UI reflects DAW changes in ~one WebSocket round-trip.
    ProjectLayout(ProjectLayout),
}

/// Interval for transport updates (client-side interpolation handles smooth animation)
/// Only need updates for sync/state changes, not every frame
const TRANSPORT_UPDATE_INTERVAL: Duration = Duration::from_millis(500);

/// Interval for fugue list updates
const FUGUE_UPDATE_INTERVAL: Duration = Duration::from_millis(100);

/// Start the singleton GUI server.
pub fn start_server(port: u16) {
    SERVER_STARTED.get_or_init(|| {
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .thread_name("gui-server")
                .build()
                .expect("Failed to create tokio runtime for GUI server");

            runtime.block_on(run_server(port));
        });
        log::info!("GUI server thread spawned on port {}", port);
    });
}

/// Run the GUI server
async fn run_server(port: u16) {
    let addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();
    log::info!("GUI server starting on http://{}", addr);

    let app = Router::new()
        // Serve frontend
        .route("/", get(serve_index))
        .route("/index.html", get(serve_index))
        // WebSocket for real-time updates
        .route("/ws", get(ws_handler))
        // REST API endpoints with instance query param support
        .route("/api/instances", get(api_instances))
        .route("/api/rename_instance", post(api_rename_instance))
        .route("/api/slots", get(api_slots).post(api_add_slot))
        .route("/api/slots/:slot", axum::routing::delete(api_remove_slot))
        .route("/api/slots/:slot/cc", post(api_set_slot_cc))
        .route("/api/slots/:slot/name", post(api_rename_slot))
        .route("/api/slots/:slot/channel", post(api_set_slot_channel))
        .route("/api/activity", get(api_activity))
        .route("/api/fugues", get(api_fugues))
        .route("/api/transport", get(api_transport))
        .route("/api/fugue/:id", get(api_fugue_by_id))
        // Project layout from the host controller extension. GET returns
        // the current snapshot for a tab opening fresh; POST is an
        // alternate ingress (the MCP server on :9999 is the primary).
        .route("/api/project_layout", get(api_get_project_layout).post(api_post_project_layout))
        // Actions
        .route("/api/start_learn/:slot", get(api_start_learn))
        .route("/api/cancel_learn", get(api_cancel_learn))
        .route("/api/wiggle/:slot", get(api_wiggle))
        .route("/api/note_on/:note", get(api_note_on))
        .route("/api/note_on/:note/:velocity", get(api_note_on_velocity))
        .route("/api/note_off/:note", get(api_note_off))
        // Fugue queue/cancel (POST)
        .route("/api/queue_fugue", post(api_queue_fugue))
        .route("/api/cancel_fugue", post(api_cancel_fugue))
        .route("/api/cancel_fugues_by_tag", post(api_cancel_fugues_by_tag))
        .route("/api/clear_fugues", get(api_clear_fugues));

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => {
            log::info!("GUI server: Successfully bound to {}", addr);
            l
        }
        Err(e) => {
            log::error!("Failed to bind GUI server on {}: {}", addr, e);
            return;
        }
    };

    if let Err(e) = axum::serve(listener, app).await {
        log::error!("GUI server error: {}", e);
    }

    log::info!("GUI server stopped");
}

/// Serve the index.html
async fn serve_index() -> impl IntoResponse {
    Html(include_str!("../../frontend/dist/index.html"))
}

// =============================================================================
// REST API endpoints using shared handlers
// =============================================================================

async fn api_instances() -> impl IntoResponse {
    let response = api::get_instances();
    json_response(response)
}

/// Request body for renaming an instance
#[derive(Debug, Deserialize)]
pub struct RenameInstanceRequest {
    instance: String,
    name: String,
}

async fn api_rename_instance(Json(body): Json<RenameInstanceRequest>) -> impl IntoResponse {
    match api::rename_instance(&body.instance, &body.name) {
        Ok(response) => json_response(response),
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e),
    }
}

async fn api_slots(Query(query): Query<InstanceQuery>) -> impl IntoResponse {
    match api::get_slots(&query.instance) {
        Ok(response) => json_response(response),
        Err(e) => error_response(StatusCode::NOT_FOUND, &e),
    }
}

async fn api_activity() -> impl IntoResponse {
    let response = api::get_activity();
    json_response(response)
}

async fn api_fugues(Query(query): Query<InstanceQuery>) -> impl IntoResponse {
    let response = api::get_fugues(&query.instance);
    json_response(response)
}

async fn api_transport(Query(query): Query<InstanceQuery>) -> impl IntoResponse {
    let response = api::get_transport(&query.instance);
    json_response(response)
}

/// `GET /api/project_layout` — return the current layout snapshot, or an
/// empty layout if no host controller extension has pushed yet. The UI
/// uses this for its initial render; subsequent updates arrive via the
/// WebSocket's `project_layout` message.
async fn api_get_project_layout() -> impl IntoResponse {
    let layout = CcBridge::get_project_layout().unwrap_or_default();
    json_response(layout)
}

/// `POST /api/project_layout` — same shape as the MCP server's
/// `/project_layout` endpoint (port 9999). Mirrored here so the Bitwig
/// extension can push to either port; storage and broadcast are shared.
async fn api_post_project_layout(body: axum::body::Bytes) -> impl IntoResponse {
    match serde_json::from_slice::<ProjectLayout>(&body) {
        Ok(layout) => {
            log::info!("GUI /project_layout received: {} tracks", layout.tracks.len());
            CcBridge::set_project_layout(layout.clone());
            broadcast_project_layout(layout);
            (StatusCode::OK, "ok").into_response()
        }
        Err(e) => {
            log::warn!("GUI /project_layout parse error: {}", e);
            (StatusCode::BAD_REQUEST, format!("parse error: {}", e)).into_response()
        }
    }
}

async fn api_fugue_by_id(
    Path(id): Path<u64>,
    Query(query): Query<InstanceQuery>,
) -> impl IntoResponse {
    match api::get_fugue_by_id(&query.instance, id) {
        Ok(response) => json_response(response),
        Err(e) => error_response(StatusCode::NOT_FOUND, &e),
    }
}

async fn api_start_learn(
    Path(slot): Path<usize>,
    Query(query): Query<InstanceQuery>,
) -> impl IntoResponse {
    match api::start_learn(&query.instance, slot) {
        Ok(response) => json_response(response),
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e),
    }
}

async fn api_cancel_learn(Query(query): Query<InstanceQuery>) -> impl IntoResponse {
    match api::cancel_learn(&query.instance) {
        Ok(response) => json_response(response),
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e),
    }
}

#[derive(serde::Deserialize)]
struct AddSlotBody { cc: u8, name: String }

async fn api_add_slot(
    Query(query): Query<InstanceQuery>,
    Json(body): Json<AddSlotBody>,
) -> impl IntoResponse {
    match CcBridge::add_slot(&query.instance, body.cc, &body.name) {
        Ok(index) => json_response(serde_json::json!({ "ok": true, "index": index })),
        Err(e) => error_response(StatusCode::BAD_REQUEST, e),
    }
}

async fn api_remove_slot(
    Path(slot): Path<usize>,
    Query(query): Query<InstanceQuery>,
) -> impl IntoResponse {
    match CcBridge::remove_slot(&query.instance, slot) {
        Ok(()) => json_response(serde_json::json!({ "ok": true })),
        Err(e) => error_response(StatusCode::BAD_REQUEST, e),
    }
}

#[derive(serde::Deserialize)]
struct SetCcBody { cc: u8 }

async fn api_set_slot_cc(
    Path(slot): Path<usize>,
    Query(query): Query<InstanceQuery>,
    Json(body): Json<SetCcBody>,
) -> impl IntoResponse {
    // Preserve the current channel; only the CC number is changing.
    let channel = CcBridge::get_slots(&query.instance)
        .ok()
        .and_then(|v| v.into_iter().find(|s| s.index == slot).map(|s| s.channel))
        .unwrap_or(0);
    match CcBridge::map_slot(&query.instance, slot, body.cc, channel) {
        Ok(()) => json_response(serde_json::json!({ "ok": true })),
        Err(e) => error_response(StatusCode::BAD_REQUEST, e),
    }
}

#[derive(serde::Deserialize)]
struct RenameBody { name: String }

async fn api_rename_slot(
    Path(slot): Path<usize>,
    Query(query): Query<InstanceQuery>,
    Json(body): Json<RenameBody>,
) -> impl IntoResponse {
    match CcBridge::rename_slot(&query.instance, slot, &body.name) {
        Ok(_) => json_response(serde_json::json!({ "ok": true })),
        Err(e) => error_response(StatusCode::BAD_REQUEST, e),
    }
}

#[derive(serde::Deserialize)]
struct SetChannelBody { channel: u8 }

async fn api_set_slot_channel(
    Path(slot): Path<usize>,
    Query(query): Query<InstanceQuery>,
    Json(body): Json<SetChannelBody>,
) -> impl IntoResponse {
    let cc = CcBridge::get_slots(&query.instance)
        .ok()
        .and_then(|v| v.into_iter().find(|s| s.index == slot).and_then(|s| s.cc))
        .unwrap_or(0);
    match CcBridge::map_slot(&query.instance, slot, cc, body.channel) {
        Ok(()) => json_response(serde_json::json!({ "ok": true })),
        Err(e) => error_response(StatusCode::BAD_REQUEST, e),
    }
}

async fn api_wiggle(
    Path(slot): Path<usize>,
    Query(query): Query<InstanceQuery>,
) -> impl IntoResponse {
    match api::wiggle_slot(&query.instance, slot) {
        Ok(response) => json_response(response),
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e),
    }
}

async fn api_note_on(
    Path(note): Path<u8>,
    Query(query): Query<InstanceQuery>,
) -> impl IntoResponse {
    match api::note_on(&query.instance, note, 100) {
        Ok(response) => json_response(response),
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e),
    }
}

async fn api_note_on_velocity(
    Path((note, velocity)): Path<(u8, u8)>,
    Query(query): Query<InstanceQuery>,
) -> impl IntoResponse {
    match api::note_on(&query.instance, note, velocity) {
        Ok(response) => json_response(response),
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e),
    }
}

async fn api_note_off(
    Path(note): Path<u8>,
    Query(query): Query<InstanceQuery>,
) -> impl IntoResponse {
    match api::note_off(&query.instance, note) {
        Ok(response) => json_response(response),
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e),
    }
}

async fn api_queue_fugue(
    Query(query): Query<InstanceQuery>,
    Json(body): Json<api::QueueFugueRequest>,
) -> impl IntoResponse {
    log::info!(
        "GUI queue_fugue: instance='{}' tag={:?} events={} duration={} quantize={:?}",
        query.instance, body.tag, body.events.len(), body.duration_beats, body.quantize
    );
    let response = api::queue_fugue(&query.instance, body);
    if !response.ok {
        log::warn!("GUI queue_fugue failed: {:?}", response.error);
    }
    json_response(response)
}

async fn api_cancel_fugue(
    Query(query): Query<InstanceQuery>,
    Json(body): Json<api::CancelFugueRequest>,
) -> impl IntoResponse {
    match api::cancel_fugue(&query.instance, body.id) {
        Ok(response) => json_response(response),
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e),
    }
}

async fn api_cancel_fugues_by_tag(
    Query(query): Query<InstanceQuery>,
    Json(body): Json<api::CancelByTagRequest>,
) -> impl IntoResponse {
    match api::cancel_fugues_by_tag(&query.instance, &body.tag) {
        Ok(response) => json_response(response),
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e),
    }
}

async fn api_clear_fugues(Query(query): Query<InstanceQuery>) -> impl IntoResponse {
    match api::clear_fugues(&query.instance) {
        Ok(response) => json_response(response),
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e),
    }
}

// =============================================================================
// Response helpers
// =============================================================================

fn json_response<T: Serialize>(data: T) -> (StatusCode, [(header::HeaderName, &'static str); 1], String) {
    let json = serde_json::to_string(&data).unwrap_or_else(|_| r#"{"error":"serialize failed"}"#.to_string());
    (StatusCode::OK, [(header::CONTENT_TYPE, "application/json")], json)
}

fn error_response(status: StatusCode, error: &str) -> (StatusCode, [(header::HeaderName, &'static str); 1], String) {
    let response = api::ErrorResponse { error: error.to_string() };
    let json = serde_json::to_string(&response).unwrap_or_else(|_| format!(r#"{{"error":"{}"}}"#, error));
    (status, [(header::CONTENT_TYPE, "application/json")], json)
}

// =============================================================================
// WebSocket handler
// =============================================================================

async fn ws_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    log::info!("GUI WebSocket: New connection request (all instances)");
    ws.on_upgrade(handle_socket)
}

async fn handle_socket(socket: WebSocket) {
    log::info!("GUI WebSocket: Connection established (all instances)");

    let (mut sender, mut receiver) = socket.split();

    // Spawn task to send updates to the client.
    //
    // One WS subscription covers every connected instance — each push is
    // tagged with its `instance_id`, and the frontend decides which one to
    // render. This lets the user switch instances instantly without a
    // reconnect (previously every selector change dropped and reopened
    // the WS, which was both janky and error-prone).
    let send_task = tokio::spawn(async move {
        let mut transport_interval = tokio::time::interval(TRANSPORT_UPDATE_INTERVAL);
        let mut fugue_interval = tokio::time::interval(FUGUE_UPDATE_INTERVAL);
        let mut layout_rx = project_layout_tx().subscribe();

        // Per-instance caches so we only push on actual change. Keyed by
        // stable instance ID, which survives rename.
        use std::collections::HashMap;
        let mut last_transport: HashMap<String, TransportState> = HashMap::new();
        let mut last_fugues: HashMap<String, (Vec<u64>, Vec<bool>)> = HashMap::new();

        loop {
            tokio::select! {
                _ = transport_interval.tick() => {
                    for (id, _name) in CcBridge::list_instances() {
                        let Ok(transport) = FugueBridge::get_transport(&id) else { continue };
                        let should_send = last_transport
                            .get(&id)
                            .map(|last: &TransportState| {
                                (transport.beat - last.beat).abs() > 0.001
                                    || transport.playing != last.playing
                                    || transport.tempo != last.tempo
                            })
                            .unwrap_or(true);
                        if !should_send { continue; }

                        let msg = WsMessage::Transport {
                            instance_id: id.clone(),
                            transport,
                        };
                        let Ok(json) = serde_json::to_string(&msg) else { continue };
                        if sender.send(Message::Text(json)).await.is_err() { return; }
                        last_transport.insert(id, transport);
                    }
                }
                _ = fugue_interval.tick() => {
                    for (id, _name) in CcBridge::list_instances() {
                        let response = api::get_fugues(&id);
                        let current_ids: Vec<u64> = response.infos.iter().map(|f| f.id).collect();
                        let current_waiting: Vec<bool> = response.infos.iter().map(|f| f.is_waiting).collect();
                        let changed = last_fugues
                            .get(&id)
                            .map(|(ids, waiting)| *ids != current_ids || *waiting != current_waiting)
                            .unwrap_or(true);
                        if !changed { continue; }

                        let msg = WsMessage::Fugues {
                            instance_id: id.clone(),
                            infos: response.infos,
                            definitions: response.definitions,
                        };
                        let Ok(json) = serde_json::to_string(&msg) else { continue };
                        if sender.send(Message::Text(json)).await.is_err() { return; }
                        last_fugues.insert(id, (current_ids, current_waiting));
                    }
                }
                layout_update = layout_rx.recv() => {
                    match layout_update {
                        Ok(layout) => {
                            let msg = WsMessage::ProjectLayout(layout);
                            if let Ok(json) = serde_json::to_string(&msg) {
                                if sender.send(Message::Text(json)).await.is_err() {
                                    break;
                                }
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            log::debug!("GUI WS: dropped {} stale layouts", n);
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }
    });

    // Handle incoming messages
    let recv_task = tokio::spawn(async move {
        while let Some(msg) = receiver.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    log::debug!("GUI WebSocket: Received: {}", text);
                    // Future: Handle commands from UI
                }
                Ok(Message::Close(_)) => break,
                Err(_) => break,
                _ => {}
            }
        }
    });

    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
    }

    log::info!("GUI WebSocket: Connection closed");
}

/// Check if the GUI server has been started
pub fn is_server_running() -> bool {
    SERVER_STARTED.get().is_some()
}
