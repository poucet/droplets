# Fugue System

The Fugue system allows AI assistants to queue pre-composed musical sequences for transport-synchronized playback with sample-accurate timing.

## Why Fugues?

LLMs are slow (~1-10 seconds per response), but music needs precise timing. Direct MCP commands like `send_note_on` execute with unpredictable latency, making musical phrases sound sloppy.

**Fugues solve this by:**
1. Accepting a complete musical sequence in one request
2. Scheduling playback to start at a quantized position (next bar, next 4 bars, etc.)
3. Playing back events with sample-accurate timing from the audio thread

## Quick Start

### Queue a Simple Melody

```json
{
  "tool": "queue_fugue",
  "arguments": {
    "instance": "lead",
    "tag": "melody",
    "duration_beats": 4.0,
    "loop_mode": "forever",
    "quantize": "bar",
    "cancel_mode": "tag:melody",
    "events": [
      {"beat": 0.0,  "type": "note_on",  "note": 60, "velocity": 100},
      {"beat": 0.5,  "type": "note_off", "note": 60},
      {"beat": 1.0,  "type": "note_on",  "note": 64, "velocity": 100},
      {"beat": 1.5,  "type": "note_off", "note": 64},
      {"beat": 2.0,  "type": "note_on",  "note": 67, "velocity": 100},
      {"beat": 2.5,  "type": "note_off", "note": 67},
      {"beat": 3.0,  "type": "note_on",  "note": 72, "velocity": 100},
      {"beat": 3.5,  "type": "note_off", "note": 72}
    ]
  }
}
```

**Response:**
```json
{
  "fugue_id": 12345,
  "status": "queued",
  "start_at": "bar 5"
}
```

### Stop the Melody

```json
{
  "tool": "cancel_fugues_by_tag",
  "arguments": {
    "instance": "lead",
    "tag": "melody"
  }
}
```

## MCP Tools

### queue_fugue

Queue a musical sequence for playback.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `instance` | string | `"default"` | Target plugin instance |
| `tag` | string | `null` | Optional tag for cancellation grouping |
| `events` | array | required | Array of timed events |
| `duration_beats` | number | required | Total length in beats (for looping) |
| `loop_mode` | string | `"once"` | `"once"`, `"times:N"`, or `"forever"` |
| `quantize` | string | `"bar"` | When to start: `"immediate"`, `"beat"`, `"bar"`, `"bars:N"` |
| `cancel_mode` | string | `"none"` | `"none"`, `"tag:NAME"`, or `"all"` |

**Returns:** `{ "fugue_id": number }`

### list_fugues

List all active and pending fugues.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `instance` | string | `"default"` | Target plugin instance |

### cancel_fugue

Cancel a specific fugue by ID.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `instance` | string | `"default"` | Target plugin instance |
| `fugue_id` | number | required | The fugue ID to cancel |

### cancel_fugues_by_tag

Cancel all fugues with a specific tag.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `instance` | string | `"default"` | Target plugin instance |
| `tag` | string | required | Tag to match |

### clear_fugues

Emergency stop - cancel all fugues on an instance.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `instance` | string | `"default"` | Target plugin instance |

## Event Types

### note_on

```json
{"beat": 0.0, "type": "note_on", "note": 60, "velocity": 100, "channel": 1}
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `beat` | number | required | Beat offset from fugue start |
| `note` | number | required | MIDI note (0-127, 60 = C4) |
| `velocity` | number | `100` | Velocity (1-127) |
| `channel` | number | `1` | MIDI channel (1-16) |

### note_off

```json
{"beat": 0.5, "type": "note_off", "note": 60, "channel": 1}
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `beat` | number | required | Beat offset from fugue start |
| `note` | number | required | MIDI note to release |
| `channel` | number | `1` | MIDI channel (1-16) |

### cc

```json
{"beat": 0.0, "type": "cc", "cc": 1, "value": 64, "channel": 1}
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `beat` | number | required | Beat offset from fugue start |
| `cc` | number | required | CC number (0-127) |
| `value` | number | required | CC value (0-127) |
| `channel` | number | `1` | MIDI channel (1-16) |

### per_note_pitch_bend

```json
{"beat": 0.0, "type": "per_note_pitch_bend", "note": 60, "semitones": 2.0, "channel": 1}
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `beat` | number | required | Beat offset from fugue start |
| `note` | number | required | Note to bend |
| `semitones` | number | required | Bend amount (-64 to +64) |
| `channel` | number | `1` | MIDI channel (1-16) |

### per_note_pressure

```json
{"beat": 0.0, "type": "per_note_pressure", "note": 60, "pressure": 0.8, "channel": 1}
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `beat` | number | required | Beat offset from fugue start |
| `note` | number | required | Note to apply pressure to |
| `pressure` | number | required | Pressure (0.0-1.0) |
| `channel` | number | `1` | MIDI channel (1-16) |

## Quantization Modes

| Mode | Description |
|------|-------------|
| `immediate` | Start on next audio buffer (minimal latency) |
| `beat` | Start on next beat boundary |
| `bar` | Start on next bar/measure boundary |
| `bars:N` | Start on next N-bar boundary (e.g., `bars:4` for next 4-bar phrase) |

## Loop Modes

| Mode | Description |
|------|-------------|
| `once` | Play once and stop |
| `times:N` | Play N times (e.g., `times:4`) |
| `forever` | Loop indefinitely until cancelled |

## Cancel Modes

Cancel mode determines what happens to existing fugues when a new one is queued.

| Mode | Description |
|------|-------------|
| `none` | Layer with existing fugues (default) |
| `tag:NAME` | Cancel all fugues with matching tag before starting |
| `all` | Cancel all fugues on this instance before starting |

## Layering and Tags

Tags enable sophisticated layering patterns:

```json
// Queue a bass line (tagged "bass")
{"tool": "queue_fugue", "arguments": {"tag": "bass", "cancel_mode": "tag:bass", ...}}

// Queue a melody (tagged "melody") - bass continues
{"tool": "queue_fugue", "arguments": {"tag": "melody", "cancel_mode": "tag:melody", ...}}

// Queue CC automation (tagged "automation") - both continue
{"tool": "queue_fugue", "arguments": {"tag": "automation", "cancel_mode": "tag:automation", ...}}

// Replace just the melody
{"tool": "queue_fugue", "arguments": {"tag": "melody", "cancel_mode": "tag:melody", ...}}

// Stop everything
{"tool": "clear_fugues", "arguments": {"instance": "lead"}}
```

## Multi-Instance Routing

Different plugin instances can receive different fugues:

```json
// Rename instances for clarity
{"tool": "set_instance_name", "arguments": {"instance": "droplets-abc123", "name": "lead"}}
{"tool": "set_instance_name", "arguments": {"instance": "droplets-def456", "name": "bass"}}

// Route fugues to specific instances
{"tool": "queue_fugue", "arguments": {"instance": "lead", "events": [...]}}
{"tool": "queue_fugue", "arguments": {"instance": "bass", "events": [...]}}
```

## Transport Behavior

- **Playback requires transport**: Fugues only play when the DAW transport is running
- **Pause preserves position**: When transport stops, fugues pause and resume from the same position
- **Seeking cancels notes**: If the user seeks to a different position, all active notes receive note-off messages

## Note Tracking

The plugin tracks which notes are currently on for each fugue. When a fugue is cancelled:
1. Note-off messages are sent for all active notes
2. No hanging notes are left behind

## Examples

### Arpeggio with CC Modulation

```json
{
  "tool": "queue_fugue",
  "arguments": {
    "instance": "lead",
    "tag": "arp",
    "duration_beats": 2.0,
    "loop_mode": "forever",
    "quantize": "bar",
    "cancel_mode": "tag:arp",
    "events": [
      {"beat": 0.0,   "type": "cc", "cc": 1, "value": 0},
      {"beat": 0.0,   "type": "note_on",  "note": 48, "velocity": 100},
      {"beat": 0.25,  "type": "note_off", "note": 48},
      {"beat": 0.25,  "type": "note_on",  "note": 52, "velocity": 90},
      {"beat": 0.5,   "type": "cc", "cc": 1, "value": 32},
      {"beat": 0.5,   "type": "note_off", "note": 52},
      {"beat": 0.5,   "type": "note_on",  "note": 55, "velocity": 80},
      {"beat": 0.75,  "type": "note_off", "note": 55},
      {"beat": 0.75,  "type": "note_on",  "note": 60, "velocity": 70},
      {"beat": 1.0,   "type": "cc", "cc": 1, "value": 64},
      {"beat": 1.0,   "type": "note_off", "note": 60},
      {"beat": 1.0,   "type": "note_on",  "note": 55, "velocity": 80},
      {"beat": 1.25,  "type": "note_off", "note": 55},
      {"beat": 1.25,  "type": "note_on",  "note": 52, "velocity": 90},
      {"beat": 1.5,   "type": "cc", "cc": 1, "value": 32},
      {"beat": 1.5,   "type": "note_off", "note": 52},
      {"beat": 1.5,   "type": "note_on",  "note": 48, "velocity": 100},
      {"beat": 1.75,  "type": "note_off", "note": 48},
      {"beat": 2.0,   "type": "cc", "cc": 1, "value": 0}
    ]
  }
}
```

### Chord Progression (4 bars, play twice)

```json
{
  "tool": "queue_fugue",
  "arguments": {
    "instance": "pad",
    "tag": "chords",
    "duration_beats": 16.0,
    "loop_mode": "times:2",
    "quantize": "bars:4",
    "cancel_mode": "tag:chords",
    "events": [
      {"beat": 0.0,  "type": "note_on", "note": 48, "velocity": 80},
      {"beat": 0.0,  "type": "note_on", "note": 52, "velocity": 80},
      {"beat": 0.0,  "type": "note_on", "note": 55, "velocity": 80},
      {"beat": 3.9,  "type": "note_off", "note": 48},
      {"beat": 3.9,  "type": "note_off", "note": 52},
      {"beat": 3.9,  "type": "note_off", "note": 55},
      {"beat": 4.0,  "type": "note_on", "note": 53, "velocity": 80},
      {"beat": 4.0,  "type": "note_on", "note": 57, "velocity": 80},
      {"beat": 4.0,  "type": "note_on", "note": 60, "velocity": 80},
      {"beat": 7.9,  "type": "note_off", "note": 53},
      {"beat": 7.9,  "type": "note_off", "note": 57},
      {"beat": 7.9,  "type": "note_off", "note": 60},
      {"beat": 8.0,  "type": "note_on", "note": 55, "velocity": 80},
      {"beat": 8.0,  "type": "note_on", "note": 59, "velocity": 80},
      {"beat": 8.0,  "type": "note_on", "note": 62, "velocity": 80},
      {"beat": 11.9, "type": "note_off", "note": 55},
      {"beat": 11.9, "type": "note_off", "note": 59},
      {"beat": 11.9, "type": "note_off", "note": 62},
      {"beat": 12.0, "type": "note_on", "note": 53, "velocity": 80},
      {"beat": 12.0, "type": "note_on", "note": 57, "velocity": 80},
      {"beat": 12.0, "type": "note_on", "note": 60, "velocity": 80},
      {"beat": 15.9, "type": "note_off", "note": 53},
      {"beat": 15.9, "type": "note_off", "note": 57},
      {"beat": 15.9, "type": "note_off", "note": 60}
    ]
  }
}
```

## Limitations

- **Tempo changes**: If the DAW tempo changes during fugue playback, timing will drift from the original intent
- **No real-time modification**: Once queued, a fugue's events cannot be modified (cancel and re-queue instead)
- **Transport required**: Fugues won't play if the DAW transport is stopped
