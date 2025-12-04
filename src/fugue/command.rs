//! Fugue commands - messages sent from MCP thread to audio thread

use super::FugueDefinition;

/// Commands sent through the ring buffer from MCP to audio thread
#[derive(Debug)]
pub enum FugueCommand {
    /// Queue a new fugue for playback
    Queue(FugueDefinition),
    /// Cancel a specific fugue by ID
    Cancel { id: u64 },
    /// Cancel all fugues with a specific tag
    CancelByTag { tag: String },
    /// Clear all active and pending fugues
    ClearAll,
}

impl FugueCommand {
    /// Create a queue command
    pub fn queue(definition: FugueDefinition) -> Self {
        Self::Queue(definition)
    }

    /// Create a cancel command
    pub fn cancel(id: u64) -> Self {
        Self::Cancel { id }
    }

    /// Create a cancel-by-tag command
    pub fn cancel_by_tag(tag: impl Into<String>) -> Self {
        Self::CancelByTag { tag: tag.into() }
    }

    /// Create a clear-all command
    pub fn clear_all() -> Self {
        Self::ClearAll
    }
}
