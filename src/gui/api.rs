//! Shared API handlers and types for GUI endpoints
//!
//! Used by both the wry custom protocol (routes.rs) and HTTP server (server.rs).
//! All response types are exported to TypeScript via ts-rs.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::fugue::{
    CancelMode, FugueBridge, FugueDefinition, FugueInfo, InterpolationMode, LoopMode, QuantizeMode,
    TimedFugueEvent, TransportState,
    export::{fugue_to_smf, generate_filename},
    settings::{self, SettingsResponse, UpdateSettingsRequest},
};
use crate::mcp::CcBridge;
use crate::params;

/// Slot info for API serialization
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct SlotInfo {
    pub index: usize,
    pub name: String,
    pub cc: Option<u8>,
    pub channel: u8,
    pub value: f64,
    pub learning: bool,
}

impl From<params::SlotInfo> for SlotInfo {
    fn from(s: params::SlotInfo) -> Self {
        Self {
            index: s.index,
            name: s.name,
            cc: s.cc,
            channel: s.channel,
            value: s.value,
            learning: s.learning,
        }
    }
}

// =============================================================================
// API Response Types - TypeScript exported
// =============================================================================

/// Response containing all CC slots
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct SlotsResponse {
    pub slots: Vec<SlotInfo>,
}

/// A single activity event for the UI
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ActivityEventDto {
    pub timestamp: u64,
    pub instance: String,
    pub channel: u8,
    pub cc: Option<u8>,
    pub value: u8,
    pub note: Option<u8>,
    pub is_note_on: Option<bool>,
    pub expression_type: Option<String>,
}

/// Response containing recent MIDI activity
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ActivityResponse {
    pub events: Vec<ActivityEventDto>,
}

/// Response containing fugue state
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct FuguesResponse {
    pub infos: Vec<FugueInfo>,
    pub definitions: Vec<FugueDefinition>,
}

/// Response containing transport state
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct TransportResponse {
    pub transport: TransportState,
}

/// Response for a single fugue definition
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct FugueResponse {
    pub fugue: Option<FugueDefinition>,
}

/// List of available instances
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct InstancesResponse {
    pub instances: Vec<InstanceInfo>,
}

/// Information about a plugin instance
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct InstanceInfo {
    pub id: String,
    pub name: String,
}

/// Generic success response
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct OkResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Generic error response
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ErrorResponse {
    pub error: String,
}

// =============================================================================
// API Handlers - shared logic
// =============================================================================

/// Get all slots for an instance
pub fn get_slots(instance: &str) -> Result<SlotsResponse, String> {
    match CcBridge::get_slots(instance) {
        Ok(slots) => Ok(SlotsResponse {
            slots: slots.into_iter().map(SlotInfo::from).collect(),
        }),
        Err(e) => Err(e.to_string()),
    }
}

/// Get recent activity for display
pub fn get_activity() -> ActivityResponse {
    let activity = CcBridge::recent_activity();
    let events = activity
        .into_iter()
        .map(|e| ActivityEventDto {
            timestamp: e.timestamp_ms,
            instance: e.instance,
            channel: e.channel + 1, // 1-indexed for display
            cc: e.cc,
            value: e.value,
            note: e.note,
            is_note_on: e.is_note_on,
            expression_type: e.expression_type,
        })
        .collect();
    ActivityResponse { events }
}

/// Get all fugue state for an instance
pub fn get_fugues(instance: &str) -> FuguesResponse {
    let infos = FugueBridge::get_fugue_info(instance).unwrap_or_default();
    let definitions = FugueBridge::get_definitions(instance).unwrap_or_default();
    FuguesResponse { infos, definitions }
}

/// Get transport state for an instance
pub fn get_transport(instance: &str) -> TransportResponse {
    let transport = FugueBridge::get_transport(instance).unwrap_or_default();
    TransportResponse { transport }
}

/// Get a single fugue by ID
pub fn get_fugue_by_id(instance: &str, id: u64) -> Result<FugueResponse, String> {
    match FugueBridge::get_definition(instance, id) {
        Ok(fugue) => Ok(FugueResponse { fugue }),
        Err(e) => Err(e.to_string()),
    }
}

/// Get list of available instances
pub fn get_instances() -> InstancesResponse {
    let instances = CcBridge::list_instances()
        .into_iter()
        .map(|(id, name)| InstanceInfo { id, name })
        .collect();
    InstancesResponse { instances }
}

/// Rename an instance
pub fn rename_instance(instance: &str, new_name: &str) -> Result<OkResponse, String> {
    CcBridge::rename(instance, new_name)
        .map(|old_name| OkResponse {
            ok: true,
            message: Some(format!("Renamed '{}' to '{}'", old_name, new_name)),
        })
        .map_err(|e| e.to_string())
}

/// Start learning mode for a slot
pub fn start_learn(instance: &str, slot: usize) -> Result<OkResponse, String> {
    CcBridge::start_learn(instance, slot)
        .map(|_| OkResponse { ok: true, message: None })
        .map_err(|e| e.to_string())
}

/// Cancel learning mode
pub fn cancel_learn(instance: &str) -> Result<OkResponse, String> {
    CcBridge::cancel_learn(instance)
        .map(|_| OkResponse { ok: true, message: None })
        .map_err(|e| e.to_string())
}

/// Trigger wiggle for a slot (visual feedback for MIDI learn)
pub fn wiggle_slot(instance: &str, slot: usize) -> Result<OkResponse, String> {
    let slots = CcBridge::get_slots(instance).map_err(|e| e.to_string())?;

    let slot_info = slots
        .into_iter()
        .find(|s| s.index == slot)
        .ok_or_else(|| "slot not found".to_string())?;

    let cc = slot_info.cc.ok_or_else(|| "slot not mapped to CC".to_string())?;
    let channel = slot_info.channel;
    let instance_owned = instance.to_string();

    // Spawn thread to wiggle
    std::thread::spawn(move || {
        for i in 0..6 {
            let value = if i % 2 == 0 { 127u8 } else { 0u8 };
            let msg = crate::mcp::CcMessage::new(channel, cc, value);
            let _ = CcBridge::send(&instance_owned, msg);
            std::thread::sleep(std::time::Duration::from_millis(150));
        }
        // Return to center
        let msg = crate::mcp::CcMessage::new(channel, cc, 64);
        let _ = CcBridge::send(&instance_owned, msg);
    });

    Ok(OkResponse {
        ok: true,
        message: Some(format!("wiggling CC{}", cc)),
    })
}

/// Send a note on message
pub fn note_on(instance: &str, note: u8, velocity: u8) -> Result<OkResponse, String> {
    let msg = crate::mcp::NoteMessage::new(0, note.min(127), velocity.min(127), true);
    CcBridge::send_note(instance, msg)
        .map(|_| OkResponse { ok: true, message: None })
        .map_err(|e| e.to_string())
}

/// Send a note off message
pub fn note_off(instance: &str, note: u8) -> Result<OkResponse, String> {
    let msg = crate::mcp::NoteMessage::new(0, note.min(127), 0, false);
    CcBridge::send_note(instance, msg)
        .map(|_| OkResponse { ok: true, message: None })
        .map_err(|e| e.to_string())
}

// =============================================================================
// Fugue Queue/Cancel API
// =============================================================================

/// Request to queue a new fugue
#[derive(Debug, Clone, Deserialize)]
pub struct QueueFugueRequest {
    pub tag: Option<String>,
    pub events: Vec<TimedFugueEvent>,
    pub duration_beats: f64,
    pub loop_mode: LoopMode,
    pub quantize: QuantizeMode,
    pub cancel_mode: CancelMode,
    #[serde(default)]
    pub cc_interpolation: InterpolationMode,
}

/// Response from queuing a fugue
#[derive(Debug, Clone, Serialize)]
pub struct QueueFugueResponse {
    pub ok: bool,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "serialize_option_u64_string"
    )]
    pub fugue_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn serialize_option_u64_string<S>(value: &Option<u64>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match value {
        Some(v) => serializer.serialize_str(&v.to_string()),
        None => serializer.serialize_none(),
    }
}

/// Request to cancel a fugue by ID
#[derive(Debug, Clone, Deserialize)]
pub struct CancelFugueRequest {
    #[serde(with = "crate::serde_u64_string")]
    pub id: u64,
}

/// Request to cancel fugues by tag
#[derive(Debug, Clone, Deserialize)]
pub struct CancelByTagRequest {
    pub tag: String,
}

/// Queue a new fugue for playback
pub fn queue_fugue(instance: &str, req: QueueFugueRequest) -> QueueFugueResponse {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);

    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);

    let definition = FugueDefinition {
        id,
        tag: req.tag,
        events: req.events,
        duration_beats: req.duration_beats,
        loop_mode: req.loop_mode,
        quantize: req.quantize,
        cancel_mode: req.cancel_mode,
        cc_interpolation: req.cc_interpolation,
    };

    match FugueBridge::queue(instance, definition) {
        Ok(fugue_id) => {
            // Wait briefly for the audio thread to pick up the command so
            // the UI's immediate re-fetch shows the new fugue. Caps at
            // 100ms; the audio thread's info-cache tick is ~5ms in plugin
            // and standalone modes.
            FugueBridge::wait_for_fugue_visible(instance, fugue_id, 100);
            QueueFugueResponse {
                ok: true,
                fugue_id: Some(fugue_id),
                error: None,
            }
        }
        Err(e) => QueueFugueResponse {
            ok: false,
            fugue_id: None,
            error: Some(e.to_string()),
        },
    }
}

/// Cancel a specific fugue by ID.
/// Waits briefly for the audio thread to process the cancel so the caller's
/// next read of the info cache reflects the removal (without this, the UI
/// sees the cancelled fugue stuck in the list).
pub fn cancel_fugue(instance: &str, id: u64) -> Result<OkResponse, String> {
    FugueBridge::cancel(instance, id)
        .map(|_| {
            FugueBridge::wait_for_fugue_gone(instance, id, 100);
            OkResponse {
                ok: true,
                message: Some(format!("Cancelled fugue {}", id)),
            }
        })
        .map_err(|e| e.to_string())
}

/// Cancel all fugues with a specific tag. Waits for the audio thread to
/// process the command so the follow-up read is clean.
pub fn cancel_fugues_by_tag(instance: &str, tag: &str) -> Result<OkResponse, String> {
    FugueBridge::cancel_by_tag(instance, tag)
        .map(|_| {
            FugueBridge::wait_for_tag_gone(instance, tag, 100);
            OkResponse {
                ok: true,
                message: Some(format!("Cancelled fugues with tag '{}'", tag)),
            }
        })
        .map_err(|e| e.to_string())
}

/// Clear all fugues on an instance. Waits for the audio thread to empty
/// the info cache so the UI's next fetch returns [].
pub fn clear_fugues(instance: &str) -> Result<OkResponse, String> {
    FugueBridge::clear_all(instance)
        .map(|_| {
            FugueBridge::wait_for_no_fugues(instance, 100);
            OkResponse {
                ok: true,
                message: Some("Cleared all fugues".to_string()),
            }
        })
        .map_err(|e| e.to_string())
}

// =============================================================================
// Settings API
// =============================================================================

/// Get current settings
pub fn get_settings() -> SettingsResponse {
    let settings = settings::get_settings();
    SettingsResponse::new(&settings, crate::mcp::DEFAULT_MCP_PORT)
}

/// Update settings. Each field is Option<...> on the request; leaving a field
/// out keeps the current value. Empty string explicitly clears the field.
pub fn update_settings(req: UpdateSettingsRequest) -> Result<OkResponse, String> {
    let mut current = settings::get_settings();

    if let Some(path) = req.export_path {
        current.export_path = std::path::PathBuf::from(path);
    }
    if let Some(text) = req.custom_instructions {
        current.custom_instructions = text;
    }

    settings::update_settings(current)?;

    Ok(OkResponse {
        ok: true,
        message: Some("Settings updated".to_string()),
    })
}

/// Open the exports folder in the system file manager
pub fn reveal_exports() -> Result<OkResponse, String> {
    settings::reveal_export_dir()?;
    Ok(OkResponse {
        ok: true,
        message: Some("Opened exports folder".to_string()),
    })
}

// =============================================================================
// Fugue Export API
// =============================================================================

/// Request to export a fugue as MIDI
#[derive(Debug, Clone, Deserialize)]
pub struct ExportFugueRequest {
    #[serde(with = "crate::serde_u64_string")]
    pub id: u64,
    /// Optional tempo override (defaults to current transport tempo)
    pub tempo: Option<f64>,
}

/// Response from exporting a fugue
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ExportFugueResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Export a fugue as a Standard MIDI File
pub fn export_fugue(instance: &str, req: ExportFugueRequest) -> ExportFugueResponse {
    // Get the fugue definition
    let definition = match FugueBridge::get_definition(instance, req.id) {
        Ok(Some(def)) => def,
        Ok(None) => {
            return ExportFugueResponse {
                ok: false,
                path: None,
                error: Some(format!("Fugue {} not found", req.id)),
            };
        }
        Err(e) => {
            return ExportFugueResponse {
                ok: false,
                path: None,
                error: Some(e.to_string()),
            };
        }
    };

    // Get tempo (from request or transport)
    let tempo = req.tempo.unwrap_or_else(|| {
        FugueBridge::get_transport(instance)
            .map(|t| t.tempo)
            .unwrap_or(120.0)
    });

    // Generate MIDI file
    let midi_bytes = fugue_to_smf(&definition, tempo);

    // Ensure export directory exists
    let export_dir = match settings::ensure_export_dir() {
        Ok(dir) => dir,
        Err(e) => {
            return ExportFugueResponse {
                ok: false,
                path: None,
                error: Some(e),
            };
        }
    };

    // Generate filename and full path
    let filename = generate_filename(&definition);
    let file_path = export_dir.join(&filename);

    // Write MIDI file
    if let Err(e) = std::fs::write(&file_path, midi_bytes) {
        return ExportFugueResponse {
            ok: false,
            path: None,
            error: Some(format!("Failed to write MIDI file: {}", e)),
        };
    }

    log::info!("Exported fugue {} to {:?}", req.id, file_path);

    // Reveal in file manager
    if let Err(e) = settings::reveal_file(&file_path) {
        log::warn!("Failed to reveal exported file: {}", e);
        // Don't fail the export, just log the warning
    }

    ExportFugueResponse {
        ok: true,
        path: Some(file_path.to_string_lossy().to_string()),
        error: None,
    }
}
