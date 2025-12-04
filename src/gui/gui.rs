//! DropletGui - GUI state and WebView management
//!
//! Contains the core GUI struct for managing webview state and IPC.

use wry::dpi::LogicalSize;

use crate::fugue::{FugueDefinition, FugueInfo, TransportState};

pub const DEFAULT_GUI_SIZE: LogicalSize<f64> = LogicalSize::new(1000.0, 800.0);
pub const MIN_GUI_SIZE: LogicalSize<f64> = LogicalSize::new(600.0, 500.0);
pub const MAX_GUI_SIZE: LogicalSize<f64> = LogicalSize::new(1600.0, 1200.0);

pub struct DropletGui {
    pub(crate) size: LogicalSize<f64>,
    pub(crate) scale_factor: f64,
    pub(crate) web_view: Option<wry::WebView>,
    /// Cache last transport state to avoid sending duplicate updates
    last_transport: Option<TransportState>,
    /// Cache last fugue count to detect changes
    last_fugue_count: usize,
}

impl DropletGui {
    pub fn new() -> Self {
        Self {
            size: DEFAULT_GUI_SIZE,
            scale_factor: 1.0,
            web_view: None,
            last_transport: None,
            last_fugue_count: 0,
        }
    }

    /// Push a transport update to the webview via IPC
    pub fn push_transport(&mut self, transport: &TransportState) {
        // Check if transport actually changed
        let should_send = self.last_transport
            .map(|last| {
                (transport.beat - last.beat).abs() > 0.001
                    || transport.playing != last.playing
                    || transport.tempo != last.tempo
            })
            .unwrap_or(true);

        if !should_send {
            return;
        }

        self.last_transport = Some(*transport);

        if let Some(webview) = &self.web_view {
            let js = format!(
                "window.simplyvst._pushTransport({{beat:{},tempo:{},playing:{},time_sig_numerator:{}}})",
                transport.beat,
                transport.tempo,
                transport.playing,
                transport.time_sig_numerator
            );
            let _ = webview.evaluate_script(&js);
        }
    }

    /// Push fugue updates to the webview via IPC
    pub fn push_fugues(&mut self, infos: &[FugueInfo], definitions: &[FugueDefinition]) {
        // Only send if count changed (simple heuristic)
        if infos.len() == self.last_fugue_count {
            return;
        }
        self.last_fugue_count = infos.len();

        if let Some(webview) = &self.web_view {
            // Serialize to JSON
            let infos_json = serde_json::to_string(infos).unwrap_or_else(|_| "[]".to_string());
            let defs_json = serde_json::to_string(definitions).unwrap_or_else(|_| "[]".to_string());

            let js = format!(
                "window.simplyvst._pushFugues({},{})",
                infos_json,
                defs_json
            );
            let _ = webview.evaluate_script(&js);
        }
    }

    /// Check if webview is active
    pub fn is_active(&self) -> bool {
        self.web_view.is_some()
    }
}

impl Default for DropletGui {
    fn default() -> Self {
        Self::new()
    }
}
