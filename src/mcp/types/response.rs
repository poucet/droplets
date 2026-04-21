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

/// `queue_fugue` result summary. `fugue_ids` are stringified to match the
/// JS-side convention (u64 doesn't round-trip through JSON numbers); the
/// redundant `count` saves the LLM from counting ids back. `duration_beats`
/// / `quantize` / `loop_mode` echo the batch-level defaults so the LLM
/// sees what actually got applied when per-fugue overrides were omitted.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct QueueFugueSummary {
    pub fugue_ids: Vec<String>,
    pub count: usize,
    pub duration_beats: f64,
    pub quantize: String,
    pub loop_mode: String,
}
