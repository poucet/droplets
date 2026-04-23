#[cfg(any(debug_assertions, feature = "dev-gui"))]
use log::{error, LevelFilter};
#[cfg(any(debug_assertions, feature = "dev-gui"))]
use simplelog::*;
#[cfg(any(debug_assertions, feature = "dev-gui"))]
use std::fs::File;

use log::debug;

/// Initialize the file logger at `~/droplets_plugin.log`.
///
/// No-op in release builds without the `dev-gui` feature. Release
/// plugins should not be writing a debug log — every `log::*!` call
/// ends up holding `simplelog`'s internal writer Mutex while it
/// formats and writes bytes to disk, and we call those from tokio
/// async workers handling MCP requests. Under production load that's
/// both wasteful and a source of hard-to-reproduce stalls.
///
/// Opt in with `cargo build --features dev-gui` (or just a debug
/// build) when you actually need the log.
pub fn init_logger() {
    #[cfg(any(debug_assertions, feature = "dev-gui"))]
    {
        let home_dir = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let log_path = format!("{}/droplets_plugin.log", home_dir);

        // Drop rmcp's log output entirely. The loud line is
        // `Response(JsonRpcResponse { … })` which dumps the full ~16 KB
        // `InitializeResult` (the server's instructions.md) on every
        // handshake — wasteful on disk. Our wrapper in `src/mcp/mod.rs`
        // already logs method / URI / status for request/response, which
        // is what we actually want.
        let config = ConfigBuilder::new()
            .add_filter_ignore_str("rmcp")
            .build();

        let _ = WriteLogger::init(
            LevelFilter::Debug,
            config,
            File::create(&log_path).unwrap_or_else(|_| File::create("/tmp/droplets_plugin.log").unwrap()),
        );

        // Set up custom panic hook to log panics instead of aborting
        std::panic::set_hook(Box::new(|panic_info| {
            let location = panic_info.location().map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column())).unwrap_or_else(|| "unknown".to_string());
            let message = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
                s.clone()
            } else {
                "unknown panic".to_string()
            };
            error!("PANIC at {}: {}", location, message);
        }));
    }
}

pub fn log_ipc_message_received(message: &str) {
    #[cfg(any(debug_assertions, feature = "dev-gui"))]
    debug!("Received IPC message from web view: {}", message);
}

pub fn log_gui_event(event: &str, details: &str) {
    #[cfg(any(debug_assertions, feature = "dev-gui"))]
    debug!("GUI {}: {}", event, details);
}