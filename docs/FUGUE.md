# Fugue System

The Fugue system lets AI assistants queue pre-composed musical sequences for transport-synchronized playback with sample-accurate timing.

## Why Fugues?

LLMs are slow (~1–10 seconds per response), but music needs precise timing. Issuing per-event MCP commands like `send_note_on` from an LLM produces unpredictable latency and sloppy phrasing.

**Fugues solve this by:**

1. Accepting a whole musical sequence — or a batch of sequences — in one request.
2. Scheduling playback to start at a quantized transport position (next bar, next 4 bars, etc.).
3. Playing events back from the audio thread with sample-accurate timing.

You compose in musical terms (beats, notes, curves) and Droplets turns that into MIDI at the right samples.

## Quick Start

### A simple looping melody

```json
{
  "tool": "queue_fugue",
  "arguments": {
    "instance": "lead",
    "duration_beats": 4,
    "quantize": "bar",
    "loop_mode": "forever",
    "fugues": [
      {
        "tag": "melody",
        "cancel_mode": "tag:melody",
        "type": "notes",
        "notes": [
          {"beat": 0.0, "note": "C4", "duration": 0.5},
          {"beat": 1.0, "note": "E4", "duration": 0.5},
          {"beat": 2.0, "note": "G4", "duration": 0.5},
          {"beat": 3.0, "note": "C5", "duration": 0.5}
        ]
      }
    ]
  }
}
```

**Response:**

```json
{
  "fugue_ids": [12345],
  "count": 1,
  "duration_beats": 4.0,
  "quantize": "bar",
  "loop_mode": "forever"
}
```

### Swap just the melody

Re-queue with the same tag and `cancel_mode: "tag:melody"`:

```json
{
  "tool": "queue_fugue",
  "arguments": {
    "instance": "lead",
    "duration_beats": 4,
    "quantize": "bar",
    "loop_mode": "forever",
    "fugues": [
      {
        "tag": "melody",
        "cancel_mode": "tag:melody",
        "type": "notes",
        "notes": [
          {"beat": 0.0, "note": "D4", "duration": 1.0},
          {"beat": 2.0, "note": "F4", "duration": 1.0}
        ]
      }
    ]
  }
}
```

Any other tagged fugues (bass, pads, automation) keep playing untouched.

### Stop everything on an instance

```json
{ "tool": "clear_fugues", "arguments": { "instance": "lead" } }
```

## MCP Tools

### `queue_fugue`

Queue a **batch** of fugues for playback.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `instance` | string | `"default"` | Target plugin instance (name from `list_instances`, or a display name set via `set_instance_name`). |
| `fugues` | array | required | One or more fugues. See [Fugue Content Types](#fugue-content-types). |
| `duration_beats` | number | `4.0` | Shared default length in beats — per-fugue `duration_beats` overrides. |
| `quantize` | string | `"bar"` | Shared default start alignment: `"immediate"`, `"beat"`, `"bar"`, `"bars:N"`. |
| `loop_mode` | string | `"forever"` | Shared default looping: `"once"`, `"forever"`, or an integer string `"N"` (N loops). |

Each fugue in the batch can also carry `tag`, `cancel_mode`, `channel`, and per-fugue overrides for `duration_beats` / `quantize` / `loop_mode`.

**Returns:** `{ "fugue_ids": [u64, ...], "count": N, "duration_beats": ..., "quantize": ..., "loop_mode": ... }`.

### `list_fugues`

List active and pending fugues on an instance. Shows IDs, tags, loop progress, and whether each is waiting for its quantized start.

### `cancel_fugue`

Cancel one fugue by ID. Sends note-offs for any active notes.

### `cancel_fugues_by_tag`

Cancel every fugue whose `tag` matches. Sends note-offs. Use this to stop all instances of a pattern — e.g. every `"melody"` fugue.

### `clear_fugues`

Emergency stop: cancel all fugues on an instance and send note-offs for anything held.

### `get_transport`

Return the current transport state for an instance: `{beat, tempo, playing, time_sig_numerator, is_looping, loop_start_beat, loop_end_beat}`. Useful for reasoning about where the playhead is before scheduling (e.g. "queue at the next 4-bar boundary — we're on bar 6 now, so schedule at bar 8"). In standalone mode, reports the simulated 120 BPM transport.

### Instance tools

- `list_instances` — see connected plugin instances (returns IDs like `droplets-a1b2c3d4`).
- `set_instance_name` — rename an instance to something musical (`"lead"`, `"bass"`, `"pad"`). Subsequent calls target that name via the `instance` field.

## Fugue Content Types

Every fugue carries a `type` discriminator. **Keep different musical concerns in different fugues** so they can be updated (tag-swapped) independently.

> **Note values in all examples.** Every `note` field accepts either a scientific-pitch-notation name (`"C4"` = middle C, `"F#3"`, `"Bb5"`, `"C-1"`) or an integer `0`–`127`. Examples below use names for clarity; numbers work identically. Letter case doesn't matter, `#` = sharp, `b`/`B` = flat.

### `notes` — MIDI notes with auto note-off

```json
{
  "type": "notes",
  "notes": [
    {"beat": 0.0, "note": "C4", "duration": 0.5, "velocity": 100},
    {"beat": 1.0, "note": "E4", "duration": 0.5}
  ]
}
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `beat` | number | required | Beat offset from fugue start. |
| `note` | string or number | required | MIDI note. Name like `"C4"`, `"F#3"`, `"Bb5"` (preferred) or integer 0–127 (60 = C4). |
| `duration` | number | required | Length in beats — a note-off is generated at `beat + duration`. |
| `velocity` | number | `100` | 1–127. |
| `channel` | number | fugue's channel | Override channel (1–16). |

### `cc` — CC automation with smooth interpolation

```json
{
  "type": "cc",
  "cc": 74,
  "points": [[0, 30], [2, 110], [4, 30]],
  "interpolation": "exp"
}
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `cc` | number | required | CC number (0–127). |
| `points` | array of `[beat, value]` | required | Keyframes. Values 0–127. |
| `interpolation` | string | `"linear"` | `"linear"`, `"exp"`, `"log"`, or `"none"` (stepped). |

The audio thread interpolates per-sample between keyframes — you write sparse points and get smooth CC output.

### `per_note_pitch_bend` — MIDI 2.0 per-note bend

Bends one specific held note over time. **Requires a concurrent `notes` fugue holding that note on the same channel** — otherwise the bend has no target.

```json
{
  "type": "per_note_pitch_bend",
  "note": "C4",
  "points": [[0, 0], [2, 2, "exp"], [4, 0, "log"]]
}
```

| Field | Type | Description |
|-------|------|-------------|
| `note` | string or number | MIDI note being bent. Name or integer 0–127. Must match a note held by a concurrent `notes` fugue on the same channel. |
| `points` | array | Each point is `[beat, semitones]` or `[beat, semitones, curve]`. Semitones: `-64.0` to `+64.0` (0 = no bend). |

### `per_note_pressure` — MIDI 2.0 per-note pressure

Modulates pressure on one specific held note.

```json
{
  "type": "per_note_pressure",
  "note": "C4",
  "points": [[0, 0.0], [2, 1.0, "exp"], [4, 0.0, "log"]]
}
```

| Field | Type | Description |
|-------|------|-------------|
| `note` | string or number | MIDI note receiving pressure. Name or integer 0–127. Same held-note requirement. |
| `points` | array | Each point is `[beat, pressure]` or `[beat, pressure, curve]`. Pressure range `0.0`–`1.0`. |

## Curves

Valid on the `cc` fugue's `interpolation` field and on each per-note point's optional third element:

| Name | Shape | Musical feel |
|------|-------|--------------|
| `linear` (default) | straight line | neutral |
| `exp` | `t²` | ease-in, accelerating |
| `log` | `1−(1−t)²` | ease-out, decelerating |
| `none` | stepped | value jumps at each point |

### Per-fugue vs per-segment curves

- **`cc`** uses a single `interpolation` per fugue — the same curve for every segment.
- **`per_note_pitch_bend` / `per_note_pressure`** use per-segment curves. Each point's optional third element is the curve for the segment **arriving at** that point (ignored on the first point). This lets one fugue combine shapes:

```json
// Crescendo-then-release on a held note — exp rise, log fall:
{"type":"per_note_pressure","note":"C4",
 "points":[[0,0],[2,1,"exp"],[4,0,"log"]]}
```

Per-note trajectories are expanded server-side into dense discrete events (~32/beat). Audio-thread per-note ramps are tracked as post-demo work for sample-accurate smoothness.

## Quantize Modes

When a fugue starts playing relative to the DAW transport:

| Mode | Description |
|------|-------------|
| `immediate` | Start on the next audio buffer (minimal latency). |
| `beat` | Snap to the next beat boundary. |
| `bar` | Snap to the next bar/measure boundary. |
| `bars:N` | Snap to the next N-bar boundary (e.g. `bars:4` for a 4-bar phrase grid). |

## Loop Modes

| Mode | Description |
|------|-------------|
| `once` | Play once and stop. |
| `N` (integer string) | Play N times, e.g. `"4"`. |
| `forever` | Loop until cancelled. |

## Cancel Modes

`cancel_mode` on a fugue controls what happens to *existing* fugues when this one starts:

| Mode | Description |
|------|-------------|
| `none` (default) | Layer with existing fugues. |
| `tag:NAME` | Cancel every currently-playing fugue with this tag before starting. |
| `all` | Cancel every fugue on this instance before starting. |

## Layering and Tags

Tags are the primary mechanism for updating one part without disturbing others. The pattern is simple: every fugue gets a `tag`, and every update uses `cancel_mode: "tag:<same-name>"`.

```json
// Start bass, melody, and CC automation on an instance — layered, independent.
{
  "tool": "queue_fugue",
  "arguments": {
    "instance": "lead",
    "duration_beats": 4,
    "quantize": "bar",
    "loop_mode": "forever",
    "fugues": [
      {"tag":"bass","cancel_mode":"tag:bass","type":"notes","notes":[
        {"beat":0,"note":"C2","duration":0.5},
        {"beat":1,"note":"C2","duration":0.5},
        {"beat":2,"note":"G2","duration":0.5},
        {"beat":3,"note":"C2","duration":0.5}
      ]},
      {"tag":"melody","cancel_mode":"tag:melody","type":"notes","notes":[
        {"beat":0,"note":"C4","duration":4,"velocity":90}
      ]},
      {"tag":"filter","cancel_mode":"tag:filter","type":"cc","cc":74,
       "points":[[0,30],[2,110],[4,30]],"interpolation":"exp"},
      {"tag":"bend","cancel_mode":"tag:bend","type":"per_note_pitch_bend","note":"C4",
       "points":[[0,0],[2,2,"exp"],[4,0,"log"]]}
    ]
  }
}
```

To replace only the melody, queue a new `tag:"melody"` fugue with `cancel_mode:"tag:melody"`. Bass, filter, and the bend keep playing.

## Multi-Instance Routing

When Droplets is loaded on multiple tracks in a DAW, each instance self-registers with a random ID (`droplets-a1b2c3d4`). Give them musical names first, then target by name:

```json
// 1. See what's connected.
{ "tool": "list_instances" }

// 2. Rename for clarity.
{ "tool": "set_instance_name",
  "arguments": { "instance": "droplets-a1b2c3d4", "name": "lead" } }
{ "tool": "set_instance_name",
  "arguments": { "instance": "droplets-5f6e7d8c", "name": "bass" } }

// 3. Route fugues to each.
{ "tool": "queue_fugue",
  "arguments": { "instance": "lead", "fugues": [ /* ... */ ] } }
{ "tool": "queue_fugue",
  "arguments": { "instance": "bass", "fugues": [ /* ... */ ] } }
```

All of this is served by a single MCP server. Each plugin instance owns its own lock-free ring buffer, and routing is just a registry lookup — there's no IPC overhead.

## Transport Behavior

- **Fugues only play while the DAW transport is running.** A fugue queued while stopped sits in the `waiting_for_start` state until transport plays and its quantize point is reached.
- **Pause preserves position.** When transport stops, fugues pause; when it resumes, they continue from where they were.
- **Seeking cancels active notes.** If the transport jumps, the scheduler emits note-offs for everything currently held so nothing is left hanging. Pending fugues reset to their waiting state.

In standalone mode (`cargo standalone`), there is no DAW — a simulated 120 BPM always-playing transport drives the scheduler so you can test fugues against a virtual MIDI port.

## Note Tracking

The scheduler tracks which notes are active per fugue. When a fugue is cancelled or the transport stops, note-offs are emitted for every held note on that fugue — no hanging notes.

## Worked Examples

### Arpeggio with a CC modulation

```json
{
  "tool": "queue_fugue",
  "arguments": {
    "instance": "lead",
    "duration_beats": 2,
    "quantize": "bar",
    "loop_mode": "forever",
    "fugues": [
      {
        "tag": "arp", "cancel_mode": "tag:arp",
        "type": "notes",
        "notes": [
          {"beat": 0.0,  "note": "C3", "duration": 0.25, "velocity": 100},
          {"beat": 0.25, "note": "E3", "duration": 0.25, "velocity": 90},
          {"beat": 0.5,  "note": "G3", "duration": 0.25, "velocity": 80},
          {"beat": 0.75, "note": "C4", "duration": 0.25, "velocity": 70},
          {"beat": 1.0,  "note": "G3", "duration": 0.25, "velocity": 80},
          {"beat": 1.25, "note": "E3", "duration": 0.25, "velocity": 90},
          {"beat": 1.5,  "note": "C3", "duration": 0.25, "velocity": 100},
          {"beat": 1.75, "note": "E3", "duration": 0.25, "velocity": 90}
        ]
      },
      {
        "tag": "mod", "cancel_mode": "tag:mod",
        "type": "cc", "cc": 1,
        "points": [[0, 0], [1, 64], [2, 0]],
        "interpolation": "linear"
      }
    ]
  }
}
```

### Chord progression, 4 bars, play twice

```json
{
  "tool": "queue_fugue",
  "arguments": {
    "instance": "pad",
    "duration_beats": 16,
    "quantize": "bars:4",
    "loop_mode": "2",
    "fugues": [
      {
        "tag": "chords", "cancel_mode": "tag:chords",
        "type": "notes", "channel": 1,
        "notes": [
          {"beat":  0, "note": "C3", "duration": 3.9, "velocity": 80},
          {"beat":  0, "note": "E3", "duration": 3.9, "velocity": 80},
          {"beat":  0, "note": "G3", "duration": 3.9, "velocity": 80},
          {"beat":  4, "note": "F3", "duration": 3.9, "velocity": 80},
          {"beat":  4, "note": "A3", "duration": 3.9, "velocity": 80},
          {"beat":  4, "note": "C4", "duration": 3.9, "velocity": 80},
          {"beat":  8, "note": "G3", "duration": 3.9, "velocity": 80},
          {"beat":  8, "note": "B3", "duration": 3.9, "velocity": 80},
          {"beat":  8, "note": "D4", "duration": 3.9, "velocity": 80},
          {"beat": 12, "note": "F3", "duration": 3.9, "velocity": 80},
          {"beat": 12, "note": "A3", "duration": 3.9, "velocity": 80},
          {"beat": 12, "note": "C4", "duration": 3.9, "velocity": 80}
        ]
      }
    ]
  }
}
```

### Crescendo-then-release pressure on a sustained note

```json
{
  "tool": "queue_fugue",
  "arguments": {
    "instance": "lead",
    "duration_beats": 4,
    "quantize": "bar",
    "loop_mode": "forever",
    "fugues": [
      {"tag":"hold","cancel_mode":"tag:hold","type":"notes","notes":[
        {"beat": 0, "note": "E4", "duration": 4, "velocity": 70}
      ]},
      {"tag":"swell","cancel_mode":"tag:swell","type":"per_note_pressure","note":"E4",
       "points":[[0, 0.0], [2, 1.0, "exp"], [4, 0.0, "log"]]}
    ]
  }
}
```

Exp on the way up (slow start, fast finish) and log on the way down (fast release, slow tail) — a natural breath-like swell from two points per segment.

## Limitations

- **Tempo changes drift.** If the DAW tempo changes mid-fugue, event timing is computed against the current tempo, so phrases originally composed against a different tempo will shift.
- **No in-flight modification.** Once a fugue is queued you can't edit its events — cancel (by ID or tag) and re-queue.
- **Transport must be playing.** A fugue queued while stopped waits.
- **MIDI 2.0 per-note expression needs a MIDI 2.0-capable host/instrument.** In MIDI 1.0–only paths (e.g. standalone's virtual MIDI port), per-note messages are skipped — use the plugin path in a DAW for that testing.
- **Per-note curves are currently expanded server-side** at ~32 events/beat. Sample-accurate audio-thread per-note ramps are planned post-demo.
