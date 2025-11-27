//! MCP Server module for Simply Droplets
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
mod server;

pub use bridge::{ActivityEvent, CcBridge, CcMessage};
pub use server::DropletsMcp;

use rmcp::transport::streamable_http_server::{
    StreamableHttpService, StreamableHttpServerConfig,
    session::local::LocalSessionManager,
};
use axum::Router;
use std::sync::{Arc, OnceLock};
use std::net::SocketAddr;

/// Singleton flag to ensure only one server starts
static SERVER_STARTED: OnceLock<()> = OnceLock::new();

/// Default port for the MCP server
pub const DEFAULT_MCP_PORT: u16 = 9999;

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
        }));

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
