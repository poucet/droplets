//! Response types returned by MCP tools.
//!
//! These pair one-to-one with a `#[tool]` function in `server.rs`. Each
//! derives `Serialize + JsonSchema` so `rmcp`'s `Json<T>` wrapper can
//! publish their shape in the tool's `output_schema` — that's what lets
//! MCP clients present the LLM with exact response shapes upfront.
//!
//! Richer output types (`TransportState`, `FugueInfo`, `ProjectState`,
//! etc.) live next to their producers elsewhere in the codebase and get
//! returned directly; only tool-specific wrappers live here.

use rmcp::schemars;
use serde::Serialize;

use super::compact::CompactFugue;
use crate::fugue::FugueInfo;
use crate::params::SlotInfo;

/// Per-instance row returned by the `list_instances` MCP tool. Kept
/// deliberately minimal — id + name — since this tool exists to enable
/// the LLM to pick a target before calling `queue_fugue`. Richer
/// per-instance data (track name, primary device) comes from
/// `get_project_state`'s [`super::super::project::InstanceSummary`].
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct InstanceHandle {
    pub id: String,
    pub name: String,
}

/// One row of `list_fugues` output — a [`FugueInfo`] with the instance
/// it's playing on stapled on. `list_fugues` returns a flat `Vec` of
/// these across every connected instance (or one, when scoped), so
/// follow-up calls like `cancel_fugue` / `get_fugue` get the id +
/// instance pair without a second `list_instances` round-trip.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct ListedFugue {
    /// Stable id of the instance the fugue is playing on. Use this as
    /// the `instance` argument when scoping follow-up calls.
    pub instance_id: String,
    /// Human-readable instance name (track name when the host controller
    /// extension is connected, otherwise the id prefix).
    pub instance_name: String,
    /// The fugue's info row.
    #[serde(flatten)]
    pub info: FugueInfo,
}

/// `queue_fugue` result summary. `fugue_ids` are stringified to match the
/// JS-side convention (u64 doesn't round-trip through JSON numbers); the
/// redundant `count` saves the LLM from counting ids back. `quantize` /
/// `loop_mode` echo the batch-level defaults so the LLM sees what actually
/// got applied when per-fugue overrides were omitted. `duration_beats` is
/// omitted when the batch didn't set one — each fugue is auto-sized from
/// its content in that case, so there's no single batch-level value to
/// echo; `get_fugue` / `list_fugues` carry the resolved per-fugue value.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct QueueFugueSummary {
    pub fugue_ids: Vec<String>,
    pub count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_beats: Option<f64>,
    pub quantize: String,
    pub loop_mode: String,
}

// ---------------------------------------------------------------------------
// Object wrappers for tools that naturally return arrays or untyped values.
//
// Gemini's MCP validator rejects any `outputSchema` whose top-level `type`
// isn't the string `"object"` — that's the JSON Schema shape used for
// "structured content" in the MCP spec. rmcp's schemars derive produces
// bare-array schemas for `Json<Vec<T>>` and a schema with no `type` for
// `Json<serde_json::Value>`, both of which trip the validator.
//
// Fix: make the tool methods return an object-shaped wrapper. Each wrapper
// is a single-field struct whose field carries the payload. Clients parse
// `response.<field>` instead of the raw response, which is a minor ergonomic
// cost but keeps the publish path honest — the schema we advertise matches
// what the tool actually returns.
// ---------------------------------------------------------------------------

/// Response wrapper for `list_instances` — one object containing the array.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct ListInstancesResponse {
    pub instances: Vec<InstanceHandle>,
}

/// Response wrapper for `list_slots`.
#[derive(Clone, Serialize, schemars::JsonSchema)]
pub struct ListSlotsResponse {
    pub slots: Vec<SlotInfo>,
}

/// Response wrapper for `list_fugues`.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct ListFuguesResponse {
    pub fugues: Vec<ListedFugue>,
}

/// Response shape for `get_fugue`: one `CompactFugue` (same lane-grouped
/// form the LLM writes to `queue_fugue`) with the fugue's id stapled on
/// so callers don't lose it on the round trip.
///
/// Single shape — no raw-event escape hatch. Compact is what a read-
/// modify-write workflow needs; the raw event stream was a debugging
/// mode that nobody outside the scheduler should be reasoning about,
/// and keeping both made `get_fugue` return an untyped JSON blob.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct GetFugueResponse {
    /// Fugue id as a string. u64 doesn't round-trip cleanly through JSON
    /// numbers on the JS side, so stringify at the boundary.
    pub id: String,
    /// The fugue's content in the same lane-grouped form `queue_fugue`
    /// accepts on input — read a fugue, mutate one lane, re-queue with
    /// the same tag + `cancel_mode:"tag:…"` to replace it.
    #[serde(flatten)]
    pub fugue: CompactFugue,
}
