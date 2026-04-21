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

use crate::fugue::FugueInfo;

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
