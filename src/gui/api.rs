//! Shared API handlers and types for GUI endpoints
//!
//! Used by both the wry custom protocol (routes.rs) and HTTP server (server.rs).
//! All response types are exported to TypeScript via ts-rs.

use serde::Serialize;
use ts_rs::TS;

use crate::fugue::{FugueBridge, FugueDefinition, FugueInfo, TransportState};
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
        .map(|name| InstanceInfo {
            id: name.clone(),
            name,
        })
        .collect();
    InstancesResponse { instances }
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

/// Trigger wiggle for a slot (visual feedback)
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
