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

Defaults across the batch, per-fugue can override: `duration_beats`, `quantize` (`"immediate"|"beat"|"bar"|"bars:N"`), `loop_mode` (`"once"|"forever"|"N"` — default `"forever"`).

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
