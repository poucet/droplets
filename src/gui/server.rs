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

use crate::fugue::{FugueBridge, TransportState};
use super::api;

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

/// WebSocket message types sent to the frontend
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsMessage {
    /// Transport state update (sent frequently for smooth playhead)
    Transport(api::TransportResponse),
    /// Fugue list update (sent when fugues change)
    Fugues(api::FuguesResponse),
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
        .route("/api/slots", get(api_slots))
        .route("/api/activity", get(api_activity))
        .route("/api/fugues", get(api_fugues))
        .route("/api/transport", get(api_transport))
        .route("/api/fugue/:id", get(api_fugue_by_id))
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
    let response = api::queue_fugue(&query.instance, body);
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

/// Query params for WebSocket connection
#[derive(Debug, Deserialize)]
pub struct WsQuery {
    #[serde(default = "default_instance")]
    instance: String,
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(query): Query<WsQuery>,
) -> impl IntoResponse {
    log::info!("GUI WebSocket: New connection request for instance '{}'", query.instance);
    ws.on_upgrade(move |socket| handle_socket(socket, query.instance))
}

async fn handle_socket(socket: WebSocket, instance: String) {
    log::info!("GUI WebSocket: Connection established for instance '{}'", instance);

    let (mut sender, mut receiver) = socket.split();

    // Spawn task to send updates to the client
    let instance_for_send = instance.clone();
    let send_task = tokio::spawn(async move {
        let mut transport_interval = tokio::time::interval(TRANSPORT_UPDATE_INTERVAL);
        let mut fugue_interval = tokio::time::interval(FUGUE_UPDATE_INTERVAL);

        let mut last_transport: Option<TransportState> = None;
        let mut last_fugue_ids: Vec<u64> = Vec::new();
        let mut last_waiting_states: Vec<bool> = Vec::new();

        loop {
            tokio::select! {
                _ = transport_interval.tick() => {
                    if let Ok(transport) = FugueBridge::get_transport(&instance_for_send) {
                        let should_send = last_transport
                            .map(|last| {
                                (transport.beat - last.beat).abs() > 0.001
                                    || transport.playing != last.playing
                                    || transport.tempo != last.tempo
                            })
                            .unwrap_or(true);

                        if should_send {
                            let msg = WsMessage::Transport(api::TransportResponse { transport });
                            if let Ok(json) = serde_json::to_string(&msg) {
                                if sender.send(Message::Text(json)).await.is_err() {
                                    break;
                                }
                            }
                            last_transport = Some(transport);
                        }
                    }
                }
                _ = fugue_interval.tick() => {
                    let response = api::get_fugues(&instance_for_send);

                    // Check if fugue list changed (IDs or waiting states)
                    let current_ids: Vec<u64> = response.infos.iter().map(|f| f.id).collect();
                    let current_waiting: Vec<bool> = response.infos.iter().map(|f| f.is_waiting).collect();
                    let changed = current_ids != last_fugue_ids || current_waiting != last_waiting_states;

                    if changed {
                        let msg = WsMessage::Fugues(response.clone());
                        if let Ok(json) = serde_json::to_string(&msg) {
                            if sender.send(Message::Text(json)).await.is_err() {
                                break;
                            }
                        }
                        last_fugue_ids = current_ids;
                        last_waiting_states = current_waiting;
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

    log::info!("GUI WebSocket: Connection closed for instance '{}'", instance);
}

/// Check if the GUI server has been started
pub fn is_server_running() -> bool {
    SERVER_STARTED.get().is_some()
}
