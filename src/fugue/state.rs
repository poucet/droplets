//! Fugue playback state - tracks active notes and playback position

use super::types::{FugueDefinition, FugueInfo, LoopMode};

/// Runtime state for an active fugue
pub struct FugueState {
    /// The fugue definition being played
    pub definition: FugueDefinition,
    /// Absolute beat when this fugue started (after quantization)
    pub start_beat: f64,
    /// Current loop iteration (0-indexed)
    pub current_loop: u32,
    /// Index of the next event to process
    pub next_event_index: usize,
    /// Bitset tracking active notes: 16 channels × 128 notes
    /// Each u64 covers 64 notes, so 2 u64s cover all 128 notes per channel
    pub active_notes: [[u64; 2]; 16],
    /// Whether this fugue is waiting for its quantization point
    pub waiting_for_start: bool,
    /// Target beat to start at (calculated from quantization)
    pub target_start_beat: Option<f64>,
}

impl FugueState {
    /// Create a new fugue state from a definition
    pub fn new(definition: FugueDefinition) -> Self {
        Self {
            definition,
            start_beat: 0.0,
            current_loop: 0,
            next_event_index: 0,
            active_notes: [[0; 2]; 16],
            waiting_for_start: true,
            target_start_beat: None,
        }
    }

    /// Mark a note as active (being played)
    pub fn set_note_active(&mut self, channel: u8, note: u8) {
        let ch = (channel & 0x0F) as usize;
        let n = note as usize;
        let idx = n / 64;
        let bit = n % 64;
        self.active_notes[ch][idx] |= 1u64 << bit;
    }

    /// Mark a note as inactive (released)
    pub fn set_note_inactive(&mut self, channel: u8, note: u8) {
        let ch = (channel & 0x0F) as usize;
        let n = note as usize;
        let idx = n / 64;
        let bit = n % 64;
        self.active_notes[ch][idx] &= !(1u64 << bit);
    }

    /// Check if a note is currently active
    pub fn is_note_active(&self, channel: u8, note: u8) -> bool {
        let ch = (channel & 0x0F) as usize;
        let n = note as usize;
        let idx = n / 64;
        let bit = n % 64;
        (self.active_notes[ch][idx] & (1u64 << bit)) != 0
    }

    /// Get all active notes as (channel, note) pairs
    pub fn get_active_notes(&self) -> Vec<(u8, u8)> {
        let mut notes = Vec::new();
        for (ch, channel_bits) in self.active_notes.iter().enumerate() {
            for (idx, &bits) in channel_bits.iter().enumerate() {
                if bits == 0 {
                    continue;
                }
                let base = idx * 64;
                for bit in 0..64 {
                    if (bits & (1u64 << bit)) != 0 {
                        notes.push((ch as u8, (base + bit) as u8));
                    }
                }
            }
        }
        notes
    }

    /// Clear all active notes
    pub fn clear_active_notes(&mut self) {
        self.active_notes = [[0; 2]; 16];
    }

    /// Check if this fugue has completed all loops
    pub fn is_finished(&self) -> bool {
        match self.definition.loop_mode {
            LoopMode::Once => self.current_loop >= 1,
            LoopMode::Times(n) => self.current_loop >= n,
            LoopMode::Forever => false,
        }
    }

    /// Reset for next loop iteration
    pub fn reset_for_loop(&mut self) {
        self.next_event_index = 0;
        self.current_loop += 1;
        self.start_beat += self.definition.duration_beats;
    }

    /// Get info about this fugue for listing
    pub fn info(&self, current_beat: f64) -> FugueInfo {
        let progress = if self.waiting_for_start {
            0.0
        } else {
            let local_beat = current_beat - self.start_beat;
            local_beat.clamp(0.0, self.definition.duration_beats)
        };

        FugueInfo {
            id: self.definition.id,
            tag: self.definition.tag.clone(),
            current_loop: self.current_loop,
            total_loops: match self.definition.loop_mode {
                LoopMode::Once => Some(1),
                LoopMode::Times(n) => Some(n),
                LoopMode::Forever => None,
            },
            is_waiting: self.waiting_for_start,
            progress_beats: progress,
            duration_beats: self.definition.duration_beats,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_fugue() -> FugueDefinition {
        FugueDefinition::new(vec![], 4.0)
    }

    #[test]
    fn test_note_tracking() {
        let def = make_test_fugue();
        let mut state = FugueState::new(def);

        // Initially no notes active
        assert!(!state.is_note_active(0, 60));
        assert!(state.get_active_notes().is_empty());

        // Set note active
        state.set_note_active(0, 60);
        assert!(state.is_note_active(0, 60));
        assert_eq!(state.get_active_notes(), vec![(0, 60)]);

        // Set another note on different channel
        state.set_note_active(1, 64);
        assert!(state.is_note_active(1, 64));
        assert_eq!(state.get_active_notes(), vec![(0, 60), (1, 64)]);

        // Clear one note
        state.set_note_inactive(0, 60);
        assert!(!state.is_note_active(0, 60));
        assert!(state.is_note_active(1, 64));

        // Clear all
        state.clear_active_notes();
        assert!(state.get_active_notes().is_empty());
    }

    #[test]
    fn test_note_tracking_high_notes() {
        let def = make_test_fugue();
        let mut state = FugueState::new(def);

        // Test notes in the high range (64-127)
        state.set_note_active(0, 100);
        state.set_note_active(0, 127);
        assert!(state.is_note_active(0, 100));
        assert!(state.is_note_active(0, 127));
        assert!(!state.is_note_active(0, 60));
    }
}
