# Fugue Queue System - Implementation Plan

## Overview

Add a transport-synchronized sequencing system where LLMs can queue pre-composed musical sequences ("fugues") that play back with sample-accurate timing, support looping, layering, and tag-based cancellation.

## Design Decisions

1. **Layering & Cancellation**: Fugues have optional `tag`. Cancel modes: `None` (layer), `CancelByTag(String)`, `CancelAll`
2. **Transport dependency**: Only play when DAW is playing. Pause when stopped, resume when playing.
3. **Note-off handling**: Track active notes per fugue via bitset. Send note-offs on cancel.
4. **Fugue IDs**: Auto-generated, returned to LLM for later cancellation.

---

## File Structure

```
src/fugue/
  mod.rs        - FugueDefinition, LoopMode, QuantizeMode, CancelMode
  event.rs      - FugueEvent, TimedFugueEvent
  state.rs      - FugueState with note tracking bitset
  command.rs    - FugueCommand enum for ring buffer
  sequencer.rs  - FugueSequencer (audio thread logic)
  bridge.rs     - FugueBridge (MCP→audio thread)
```

---

## Core Data Structures

### FugueEvent (src/fugue/event.rs)
```rust
pub enum FugueEvent {
    NoteOn { channel: u8, note: u8, velocity: u8 },
    NoteOff { channel: u8, note: u8 },
    Cc { channel: u8, cc: u8, value: u8 },
    PerNotePitchBend { channel: u8, note: u8, semitones: f32 },
    PerNotePressure { channel: u8, note: u8, pressure: f32 },
}

pub struct TimedFugueEvent {
    pub beat_offset: f64,  // Relative to fugue start
    pub event: FugueEvent,
}
```

### FugueDefinition (src/fugue/mod.rs)
```rust
pub struct FugueDefinition {
    pub id: u64,
    pub tag: Option<String>,
    pub events: Vec<TimedFugueEvent>,  // Sorted by beat_offset
    pub duration_beats: f64,
    pub loop_mode: LoopMode,           // Once, Times(n), Forever
    pub quantize: QuantizeMode,        // Immediate, NextBeat, NextBar, NextBars(n)
    pub cancel_mode: CancelMode,       // None, CancelByTag(s), CancelAll
}
```

### FugueState (src/fugue/state.rs)
```rust
pub struct FugueState {
    pub definition: FugueDefinition,
    pub start_beat: f64,               // Absolute beat when started
    pub current_loop: u32,
    pub next_event_index: usize,
    pub active_notes: [[u64; 2]; 16],  // Bitset: 16 channels x 128 notes
    pub waiting_for_start: bool,
    pub target_start_beat: Option<f64>,
}
```

### FugueCommand (src/fugue/command.rs)
```rust
pub enum FugueCommand {
    Queue(FugueDefinition),
    Cancel { id: u64 },
    CancelByTag { tag: String },
    ClearAll,
}
```

---

## MCP Tools

### queue_fugue
```json
{
  "instance": "lead",
  "tag": "melody",
  "duration_beats": 4.0,
  "loop_mode": "forever",
  "quantize": "bar",
  "cancel_mode": "tag:melody",
  "events": [
    {"beat": 0.0, "type": "note_on", "note": 60, "velocity": 100},
    {"beat": 0.5, "type": "note_off", "note": 60},
    {"beat": 1.0, "type": "note_on", "note": 64, "velocity": 100},
    {"beat": 1.5, "type": "note_off", "note": 64}
  ]
}
```
Returns: `{ "fugue_id": 12345 }`

### list_fugues
List all active/pending fugues with IDs, tags, status.

### cancel_fugue
Cancel by ID. Sends note-offs for active notes.

### cancel_fugues_by_tag
Cancel all fugues with matching tag.

### clear_fugues
Emergency stop - cancel all fugues on instance.

---

## Audio Thread Sequencer Logic

### Beat-to-Sample Conversion
```rust
fn beat_to_sample_offset(target_beat: f64, transport: &TransportEvent, frames: u32) -> Option<u32> {
    let current_beat = transport.song_pos_beats.to_float();
    let beats_per_sample = transport.tempo / (60.0 * sample_rate);
    let sample_offset = ((target_beat - current_beat) / beats_per_sample).round() as i64;

    if sample_offset >= 0 && sample_offset < frames as i64 {
        Some(sample_offset as u32)
    } else {
        None
    }
}
```

### Quantization Target
```rust
fn calculate_quantize_target(quantize: &QuantizeMode, transport: &TransportEvent) -> f64 {
    match quantize {
        Immediate => current_beat,
        NextBeat => current_beat.ceil(),
        NextBar => next_bar_beat(transport),
        NextBars(n) => next_n_bar_boundary(transport, n),
    }
}
```

### Process Loop (per audio buffer)
1. Pop commands from ring buffer -> queue/cancel fugues
2. Check if transport IS_PLAYING; if not, return early
3. Detect transport jumps -> cancel all notes, reset positions
4. Check pending fugues -> start those whose quantization point is reached
5. For each active fugue:
   - Find events in current buffer range
   - Output with correct sample offset
   - Handle loop boundary
6. Remove completed fugues, send note-offs

---

## Thread Safety

- **Separate ring buffer** for fugue commands (alongside existing MIDI ring buffer)
- **ID generation** happens in MCP thread before ring buffer push (sync return)
- **FugueSequencer** lives entirely in audio thread (no locks)
- **FugueBridge** follows same pattern as CcBridge
- **Instance routing at MCP layer only** - `FugueDefinition` and `FugueCommand` don't contain instance info; routing happens in `FugueBridge::queue(instance, def)` which pushes to the correct instance's ring buffer

---

## Files to Modify

| File | Changes |
|------|---------|
| `src/lib.rs` | Add `mod fugue;`, update shared state |
| `src/audio/mod.rs` | Integrate FugueSequencer, use `process.transport` |
| `src/mcp/bridge.rs` | Add fugue producer to InstanceEntry |
| `src/mcp/server.rs` | Add 5 new MCP tools |

## New Files

| File | Purpose |
|------|---------|
| `src/fugue/mod.rs` | Module root, FugueDefinition, enums |
| `src/fugue/event.rs` | FugueEvent, TimedFugueEvent |
| `src/fugue/state.rs` | FugueState, note tracking |
| `src/fugue/command.rs` | FugueCommand |
| `src/fugue/sequencer.rs` | FugueSequencer |
| `src/fugue/bridge.rs` | FugueBridge |

---

## Implementation Order

### Phase 1: Core Types (no runtime changes)
- [ ] Create `src/fugue/mod.rs` with enums and FugueDefinition
- [ ] Create `src/fugue/event.rs` with FugueEvent, TimedFugueEvent
- [ ] Create `src/fugue/state.rs` with FugueState and note tracking
- [ ] Create `src/fugue/command.rs` with FugueCommand

### Phase 2: Sequencer
- [ ] Create `src/fugue/sequencer.rs` with FugueSequencer
- [ ] Implement beat-to-sample conversion
- [ ] Implement quantization calculation
- [ ] Implement event processing loop with looping
- [ ] Implement cancellation with note-off cleanup

### Phase 3: Bridge
- [ ] Create `src/fugue/bridge.rs` with FugueBridge
- [ ] Modify `src/mcp/bridge.rs` to add fugue producer
- [ ] Update instance registration

### Phase 4: MCP Tools
- [ ] Add request types to `src/mcp/server.rs`
- [ ] Implement queue_fugue
- [ ] Implement list_fugues
- [ ] Implement cancel_fugue, cancel_fugues_by_tag, clear_fugues

### Phase 5: Integration
- [ ] Update `src/lib.rs` with fugue module
- [ ] Modify `src/audio/mod.rs` to integrate sequencer
- [ ] Use `process.transport` for timing

### Phase 6: Testing
- [ ] Test with DAW transport (play/pause/seek)
- [ ] Test quantization modes
- [ ] Test looping
- [ ] Test concurrent fugues and cancellation

---

## Potential Challenges

1. **Buffer boundaries**: Events at exact boundaries need care to avoid double-trigger
2. **Transport jumps**: Seeking should reset fugue positions or cancel
3. **Ring buffer size**: FugueDefinition can be large; may need larger buffer (512+)
4. **Tempo changes**: Document that tempo changes mid-fugue cause drift
