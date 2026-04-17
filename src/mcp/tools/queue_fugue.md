Queue one or more fugues for transport-synchronized playback. Each fugue is atomic — replaced together, cancelled together, one UI row.

## Fugue types (use the `type` field)

- **`composite`** — PREFERRED for single-instrument moments. Bundles notes + cc lanes + per-note bend lanes + per-note pressure lanes in ONE fugue. Fields: `notes` (required), `cc` (array of `{cc, points, interpolation?}`), `pitch_bends` (array of `{note, points, interpolation?}`), `pressures` (array of `{note, points, interpolation?}`). Everything optional except `notes`.
- **`notes`** — single-concern: just notes. Use when this part needs to be replaceable independently (bass swap while melody plays).
- **`cc`** — single-concern CC lane. Fields: `cc`, `points`, `interpolation?`.
- **`per_note_pitch_bend`** / **`per_note_pressure`** — single-concern per-note expression on one held note. Fields: `note`, `points`, `interpolation?`.

## When to pick which

- **Composite:** parts belong to one musical moment on one instrument (pad with chord tones + filter sweep + pressure swells).
- **Single-concern:** parts need independent replacement via tag swap (bass vs melody vs pads on a shared instrument).

## Points shape (unified across cc / pitch_bends / pressures)

Each point is `[beat, value]` or `[beat, value, curve]`.

Curves: `linear` (default), `exp` (ease-in), `log` (ease-out), `none` (step). Per-point curves win over the lane's `interpolation`; lane `interpolation` wins over fugue defaults.

## Notes

Accept names (preferred) or numbers: `"C4"` (middle C = 60), `"F#3"`, `"Bb5"`, `"C-1"` (lowest), or `0`–`127`.

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
        {"beat": 0, "note": "C2", "duration": 16},
        {"beat": 0, "note": "G3", "duration": 8},
        {"beat": 8, "note": "F3", "duration": 8}
      ],
      "cc": [
        {"cc": 74, "points": [[0,30], [8,100,"exp"], [16,40,"log"]]},
        {"cc": 11, "points": [[0,50], [16,115]], "interpolation": "exp"}
      ],
      "pitch_bends": [
        {"note": "G3", "points": [[0,0], [4,2,"exp"], [8,0,"log"]]}
      ],
      "pressures": [
        {"note": "C2", "points": [[0,0], [8,0.8,"exp"], [16,0,"log"]]}
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
