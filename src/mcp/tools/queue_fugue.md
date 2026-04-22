Queue one or more fugues for transport-synchronized playback. Each fugue is atomic — replaced together, cancelled together, one UI row.

**Required:** top-level `instance` — which Droplets plugin instance to target (from `get_project_state` / `list_instances`). One call = one instance; for multi-instrument arrangements, issue one `queue_fugue` call per instance (parallelizable). Avoid `"default"` when multiple instances are loaded.

## Fugue types (use the `type` field)

- **`composite`** — PREFERRED for single-instrument moments. Bundles notes + cc lanes + slot lanes + per-note bend lanes + per-note pressure lanes in ONE fugue. Fields: `notes` (required), `cc` (array of `{cc, points, interpolation?}`), `slots` (array of `{slot, points, interpolation?}`), `pitch_bends` (array of `{note, points, interpolation?}`), `pressures` (array of `{note, points, interpolation?}`). Everything optional except `notes`.
- **`notes`** — single-concern: just notes. Use when this part needs to be replaceable independently (bass swap while melody plays).
- **`slot`** — single-concern slot-param automation. Fields: `slot` (0–15), `points` (values 0.0–1.0), `interpolation?`. **Preferred over `cc` for automating soft-synth parameters inside a DAW** — the user maps slots to synth params natively; you drive slots and the DAW records real automation.
- **`cc`** — single-concern CC lane for hardware targets (external MIDI synths, CC-addressable mixers). Inside a DAW, prefer `slot`. Fields: `cc`, `points`, `interpolation?`.
- **`per_note_pitch_bend`** / **`per_note_pressure`** — single-concern per-note expression on one held note. Fields: `note`, `points`, `interpolation?`.

## When to pick which

- **Composite:** parts belong to one musical moment on one instrument (pad with chord tones + filter sweep + pressure swells).
- **Single-concern:** parts need independent replacement via tag swap (bass vs melody vs pads on a shared instrument).

## Points shape (unified across cc / pitch_bends / pressures)

Each point is `[beat, value]` or `[beat, value, curve]`.

Curves: `linear` (default), `exp` (ease-in), `log` (ease-out), `none` (step). Per-point curves win over the lane's `interpolation`; lane `interpolation` wins over fugue defaults.

## Notes

**Note shorthand — prefer the array form for ~4x fewer tokens.** Each entry in a `notes` lane accepts either:

- **Array (preferred):** `[beat, note, duration?, velocity?, channel?]` — e.g. `[0, "C1", 0.75, 120]`
- **Object:** `{beat, note, duration, velocity?, channel?}` — more readable but much longer

Defaults: `duration = 1`, `velocity = 100`, `channel = fugue default`. Omit trailing fields you don't need. Use `null` to skip a middle field (e.g. `[0, "C3", 1, null, 5]` to set channel only).

Note values accept names (preferred) or numbers: `"C3"` (middle C = 60, DAW convention matching Bitwig/Ableton/Logic), `"F#2"`, `"Bb4"`, `"C-2"` (lowest), or `0`–`127`.

## Shared top-level fields

Defaults across the batch, per-fugue can override: `duration_beats`, `quantize` (`"immediate"|"beat"|"bar"|"bars:N"`), `loop_mode` (`"once"|"forever"|"N"` — default `"forever"`), `start_mode` (`"phase"|"boundary"` — default `"phase"`).

**`duration_beats` is in BEATS, not bars.** In 4/4 (the default), multiply bars × 4: a 2-bar pattern is `duration_beats: 8`, 4 bars is `16`, 8 bars is `32`, 16 bars is `64`. Every `beat` field in notes and points uses the same unit — beat 4 is the *second* downbeat, not "4 bars in."

`duration_beats` is auto-sized when not set: the smallest whole bar (4/4) that fits every note's end-beat and every CC/expression point. Values that are shorter than the content are extended to fit — silently truncating notes is almost always a bug, not intent. Set it explicitly when you want trailing silence in the loop, or a specific odd-length pattern.

### Song-grid alignment — `quantize` vs `duration_beats`

Fugue iteration boundaries always land on multiples of `duration_beats` from song-beat-0, regardless of when the LLM called `queue_fugue` — network / inference latency can't shift musical alignment. `quantize` only decides the **earliest moment** the fugue can begin; `start_mode` decides what happens between that moment and the first full iteration boundary.

- `start_mode: "phase"` (default): fugue joins the always-running grid at the current song-phase. Example: `duration_beats: 16`, `quantize: "bar"`, queued when transport is at bar 2 (beat 5). The fugue begins at bar 3 (beat 8), plays pattern-beat-8 → pattern-beat-16 from transport 8 → 16 (second half of the pattern sounds immediately), then pattern-beat-0 at transport 16, 32, 48. Musical position is predictable regardless of queue latency.
- `start_mode: "boundary"`: fugue waits for the next multiple of `duration_beats` ≥ quantize target, then plays from pattern-beat-0. Same setup: silent from transport 8 → 16, then pattern-beat-0 at 16, 32, 48. Use when the first beat of the pattern is musically load-bearing (a drum fill's downbeat) and you'd rather have a brief silence than a mid-pattern start.

If you want the fugue to *always* start from pattern-beat-0 and iterate on the bar grid, set `quantize` to match `duration_beats` (e.g. `"bars:4"` for a 16-beat pattern) — then the quantize target is itself a multiple of `duration_beats` and both modes collapse to "start from the top on the next 4-bar boundary."

## Tag + cancel_mode

Update one part without disturbing others: `tag:"melody"` + `cancel_mode:"tag:melody"` replaces just the melody.

## Worked example — a pad moment on one instrument

```json
{
  "instance": "pad",
  "duration_beats": 16,
  "quantize": "bar",
  "loop_mode": "forever",
  "fugues": [
    {
      "tag": "pad-moment",
      "cancel_mode": "tag:pad-moment",
      "type": "composite",
      "notes": [
        [0, "C1", 16],
        [0, "G2", 8],
        [8, "F2", 8]
      ],
      "cc": [
        {"cc": 74, "points": [[0,30], [8,100,"exp"], [16,40,"log"]]},
        {"cc": 11, "points": [[0,50], [16,115]], "interpolation": "exp"}
      ],
      "pitch_bends": [
        {"note": "G2", "points": [[0,0], [4,2,"exp"], [8,0,"log"]]}
      ],
      "pressures": [
        {"note": "C1", "points": [[0,0], [8,0.8,"exp"], [16,0,"log"]]}
      ]
    }
  ]
}
```

## Contrast — use single-concern fugues when parts need independent replacement

```json
{
  "fugues": [
    {"tag": "bass",   "cancel_mode": "tag:bass",   "type": "notes", "notes": [...]},
    {"tag": "melody", "cancel_mode": "tag:melody", "type": "notes", "notes": [...]}
  ]
}
```

Later you can swap just the melody with a new `queue_fugue` call using `tag:"melody"` + `cancel_mode:"tag:melody"` — bass keeps playing.
