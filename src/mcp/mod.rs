//! MCP Server module for Simply Droplets
//!
//! Provides AI control of MIDI CC output via the Model Context Protocol.
//!
//! ## Architecture
//!
//! AI Assistant
//!     |
//!     | HTTP (MCP over SSE)
//!     v
//! MCP Server (singleton, localhost:9999/sse)
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

use rmcp::transport::sse_server::SseServer;
use rmcp::serve_server;
use std::sync::OnceLock;
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
    log::info!("MCP server starting on http://{}/sse", addr);

    match SseServer::serve(addr).await {
        Ok(mut server) => {
            log::info!("MCP server listening on http://{}/sse", addr);

            // Accept connections and serve them
            while let Some(transport) = server.next_transport().await {
                let service = DropletsMcp::new();
                tokio::spawn(async move {
                    if let Err(e) = serve_server(service, transport).await {
                        log::error!("MCP session error: {}", e);
                    }
                });
            }

            log::info!("MCP server stopped");
        }
        Err(e) => {
            log::error!("Failed to start MCP server on {}: {}", addr, e);
            log::error!("Another instance may already be running, or the port is in use");
        }
    }
}

/// Check if the MCP server has been started
pub fn is_server_running() -> bool {
    SERVER_STARTED.get().is_some()
}
