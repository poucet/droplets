//! Request types for every MCP tool — the JSON shapes `rmcp` deserializes
//! into before calling our tool impls.
//!
//! `InstanceRequest<T>` is the generic wrapper that carries `{instance, ...}`;
//! every per-tool `*Data` struct flattens inside it. The `*Request` type
//! aliases at the bottom are what the `#[tool]` signatures actually declare.

use rmcp::schemars;
use serde::Deserialize;

use super::compact::CompactFugue;

/// Wrapper for requests that target a specific plugin instance.
/// This enables future batching of multiple operations for the same instance.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct InstanceRequest<T> {
    /// Target plugin instance name or "default" for first available
    #[serde(default = "default_instance")]
    #[schemars(description = "Target plugin instance name or 'default' for first available")]
    pub instance: String,

    /// The actual request data
    #[serde(flatten)]
    pub data: T,
}

/// Queue one or more fugues for transport-synchronized playback
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct QueueFugueData {
    /// Array of fugues to queue. Each fugue is atomic.
    #[schemars(description = "Array of fugues. Each is atomic - use separate fugues for notes, CC, and per-note expression so they can be updated independently.")]
    pub fugues: Vec<CompactFugue>,

    // Shared defaults (can be overridden per-fugue)
    /// Default quantize mode: "immediate", "beat", "bar", or "bars:N"
    #[serde(default = "default_quantize")]
    #[schemars(description = "Default quantize: 'immediate', 'beat', 'bar', or 'bars:N' (default: 'bar')")]
    pub quantize: Option<String>,
    /// Default duration in beats
    #[serde(default = "default_duration")]
    #[schemars(description = "Default duration in beats. If omitted, duration is auto-sized to the smallest whole number of bars (4/4) that fits the content. Explicit values shorter than the content are also extended to fit — a duration that would truncate notes is almost always a bug.")]
    pub duration_beats: Option<f64>,
    /// Default loop mode: "once", "forever", or a number
    #[serde(default = "default_loop_mode_opt")]
    #[schemars(description = "Default loop mode: 'once', 'forever', or a number (default: 'forever')")]
    pub loop_mode: Option<String>,
}

/// Cancel a specific fugue by ID
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CancelFugueData {
    /// Fugue ID to cancel (returned by queue_fugue)
    #[schemars(description = "Fugue ID to cancel (returned by queue_fugue)")]
    pub id: u64,
}

/// Cancel fugues by tag
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CancelFuguesByTagData {
    /// Tag to match for cancellation
    #[schemars(description = "Tag to match for cancellation (e.g., 'melody')")]
    pub tag: String,
}

/// Request to fetch one fugue's full definition by ID.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetFugueData {
    /// Fugue ID to fetch (returned by queue_fugue / list_fugues).
    #[schemars(description = "Fugue ID to fetch (the same id list_fugues and queue_fugue return).")]
    pub id: u64,

    /// When true (default), return the compact lane-grouped view — same
    /// shape the LLM writes on input to `queue_fugue`. When false, return
    /// the raw event stream for debugging.
    #[serde(default = "default_compact")]
    #[schemars(description = "Return compact lane-grouped view (default true). Set false to get raw event stream for debugging.")]
    pub compact: bool,
}

/// Request body for `import_fugue` — accepts a base64-encoded `.mid` blob
/// and a small set of queue-time knobs. Mirrors the options used by the
/// frontend drop-zone HTTP handler so the MCP and HTTP paths can share
/// the underlying `api::import_fugue` function.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ImportFugueData {
    /// Base64-encoded Standard MIDI File. The LLM typically gets this from
    /// a user message ("here is a .mid I edited") or from a fetch tool.
    #[schemars(description = "Base64-encoded Standard MIDI File bytes (SMF format 0 or 1).")]
    pub base64_mid: String,

    /// Optional prefix prepended to each imported fugue's tag. Useful for
    /// namespacing a batch so it can be cancelled later via
    /// `cancel_fugues_by_tag` with the same prefix.
    #[serde(default)]
    #[schemars(description = "Optional prefix prepended to every imported fugue's tag (e.g. 'edited-' → 'edited-bass').")]
    pub tag_prefix: Option<String>,

    /// Loop policy applied to every imported fugue. Defaults to `forever`
    /// to match the LLM-facing queue_fugue default.
    #[serde(default)]
    #[schemars(description = "Loop mode for the imported fugues. One of: once | times | forever. Default: forever.")]
    pub loop_mode: Option<String>,

    /// Quantize policy applied to every imported fugue. Defaults to `bar`.
    #[serde(default)]
    #[schemars(description = "Quantize mode for imported fugues: immediate | beat | bar. Default: bar.")]
    pub quantize: Option<String>,

    /// When true, reject files containing per-note expression (pitch bend,
    /// polyphonic aftertouch) instead of silently dropping those events.
    /// Useful when the caller wants a hard guarantee of round-trip fidelity.
    #[serde(default)]
    #[schemars(description = "Reject files containing per-note expression instead of dropping those events. Default: false.")]
    pub strict: Option<bool>,
}

// =============================================================================
// Instance management types (these don't use the wrapper since instance is the subject)
// =============================================================================

/// Request to rename an instance
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RenameInstanceRequest {
    /// Current instance name or ID
    #[schemars(description = "Current instance name or ID")]
    pub instance: String,

    /// New display name for the instance
    #[schemars(description = "New display name for the instance")]
    pub name: String,
}

/// Request to get slots for an instance (no additional data needed)
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetSlotsRequest {
    /// Target plugin instance name or "default" for first available
    #[serde(default = "default_instance")]
    #[schemars(description = "Target plugin instance name or 'default' for first available")]
    pub instance: String,
}

// =============================================================================
// Type aliases for the MCP tool interface
// =============================================================================

pub type QueueFugueRequest = InstanceRequest<QueueFugueData>;
pub type CancelFugueRequest = InstanceRequest<CancelFugueData>;
pub type CancelFuguesByTagRequest = InstanceRequest<CancelFuguesByTagData>;
pub type GetFugueRequest = InstanceRequest<GetFugueData>;
pub type ImportFugueRequest = InstanceRequest<ImportFugueData>;

// =============================================================================
// Default value functions
//
// Kept `pub(super)` because `compact::CompactFugue` pulls `default_cancel_mode`
// and `default_fugue_channel` across module boundaries. The rest stay
// package-private.
// =============================================================================

pub(super) fn default_instance() -> String {
    "default".to_string()
}

pub(super) fn default_fugue_channel() -> Option<u8> {
    Some(1)
}

pub(super) fn default_duration() -> Option<f64> {
    None
}

pub(super) fn default_loop_mode_opt() -> Option<String> {
    Some("forever".to_string())
}

pub(super) fn default_quantize() -> Option<String> {
    Some("bar".to_string())
}

pub(super) fn default_cancel_mode() -> Option<String> {
    Some("none".to_string())
}

pub(super) fn default_compact() -> bool {
    true
}
