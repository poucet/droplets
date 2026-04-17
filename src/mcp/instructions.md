Simply Droplets — AI-controlled MIDI 1.0/2.0 out of a DAW plugin.

## Multi-instance setup (do first when >1 plugin is loaded)
1. `list_instances` — shows connected IDs like 'droplets-a1b2c3d4'.
2. `set_instance_name` on each to give musical names: 'lead', 'bass', 'pad'. All subsequent calls target these names via the `instance` field.
3. The DAW transport MUST be PLAYING for fugues to produce sound. Use `get_transport` to check.

## queue_fugue — primary composition tool
Batches multiple fugues into one call. Each fugue is atomic; swap one musical part by queueing a new fugue with the same tag + `cancel_mode: "tag:<name>"`. The other parts keep playing untouched.

Fugue content types (each in its own fugue for independent control):
- `notes`               — MIDI notes with auto note-off at beat+duration.
- `cc`                  — CC automation with smooth per-fugue interpolation.
- `per_note_pitch_bend` — MIDI 2.0 per-note bend on a held note.
- `per_note_pressure`   — MIDI 2.0 per-note pressure on a held note.

Shared fields (override per-fugue): `duration_beats`, `quantize` (`"immediate"|"beat"|"bar"|"bars:N"`), `loop_mode` (`"once"|"forever"|"N"`).

Curves (on CC interpolation, and per-segment on per-note point tuples):
- `linear` (default)  smooth straight line
- `exp`               ease-in, accelerating (t²)
- `log`               ease-out, decelerating (1-(1-t)²)
- `none`              stepped/discrete

See the `queue_fugue` tool description for a full worked example.

## MIDI reference

### Notes — always accept name OR number
Every `note` field accepts either scientific pitch notation (`"C4"`, `"F#3"`, `"Bb5"`, `"C-1"` for the lowest MIDI note) or an integer 0–127. **Prefer names** — they're clearer for both you and the user reading the output. Letter case doesn't matter (`c4` = `C4`), `#` = sharp, `b` or `B` after the letter = flat.

Reference (when you need to think in numbers):
- C0 = 12,  C1 = 24,  C2 = 36,  C3 = 48,  C4 = 60 (middle C),  C5 = 72,  C6 = 84,  C7 = 96
- Within an octave: C, C#, D, D#, E, F, F#, G, G#, A, A#, B → offsets 0..11
- So D4 = 62, G4 = 67, A4 = 69 (concert pitch), Bb3 = 58, etc.

### Typical musical ranges
- Kick / sub-bass:        24–36  (C1–C2)
- Bass line:              36–55  (C2–G3)
- Chords / pad:           48–72  (C3–C5)
- Melody / lead:          60–84  (C4–C6)
- Lead / top-line hooks:  72–96  (C5–C7)

### Common CC numbers (widely supported, but individual synths may remap)
- CC 1   — Modulation wheel (typically adds vibrato / depth)
- CC 7   — Channel volume (fader level)
- CC 10  — Pan (0 = left, 64 = center, 127 = right)
- CC 11  — Expression (for dynamics — preferred over volume for swells)
- CC 64  — Sustain pedal (0–63 = off, 64–127 = on)
- CC 71  — Resonance / filter Q
- CC 72  — Release time
- CC 73  — Attack time
- CC 74  — Filter cutoff / brightness  ← the classic filter sweep CC
- CC 91  — Reverb send amount
- CC 93  — Chorus / mod-FX send amount
- CC 120 — All sound off (panic)
- CC 123 — All notes off

When the user asks for a "filter sweep", default to CC 74. For "volume swell" prefer CC 11. For "mod wheel" it's CC 1.

## Musical defaults that actually sound good
- Velocities 60–110. Save 110–120 for hits that need to cut through. 127 is shouting — use only if aggressive is the point.
- Use `quantize: "bar"` so updates land on musical boundaries.
- Short fugues (2–8 bars) + `loop_mode: "forever"`; replace via tag swap.
- Separate notes and automation into different fugues — update independently.
- Per-note bend/pressure REQUIRE a concurrent notes fugue holding the target note on the same channel; otherwise the expression has nothing to modulate.

## Timing quick reference (4/4 time)
- 1 beat = a quarter note. 1 bar = 4 beats.
- 16th note = 0.25 beats, 8th = 0.5, quarter = 1, half = 2, whole = 4.
- A "2-bar phrase" in 4/4 → `duration_beats: 8`.
- Triplet eighth = 1/3 beat ≈ 0.333.
- Beat 0 is the downbeat; beats 1, 2, 3 are the "and" positions of a bar.

## Pattern cookbook (compact)

### Four-on-the-floor kick
Fugue `type:"notes"`, channel mapped to a drum: kick on C2 every beat.
```
notes: [{beat:0,note:"C2",duration:0.2},{beat:1,note:"C2",duration:0.2},
        {beat:2,note:"C2",duration:0.2},{beat:3,note:"C2",duration:0.2}]
```

### Held drone + breath-like pressure swell
Two fugues on the same channel/note. Pressure is per-segment exp/log for a breath shape.
```
{type:"notes", notes:[{beat:0,note:"C4",duration:4,velocity:70}]}
{type:"per_note_pressure", note:"C4",
 points:[[0,0.0],[2,1.0,"exp"],[4,0.0,"log"]]}
```

### Filter sweep up and back
CC 74 ramping 30→110→30 over a bar, with exp curve.
```
{type:"cc", cc:74, points:[[0,30],[2,110],[4,30]], interpolation:"exp"}
```

### 16th-note arp, velocity tapering
Ascending triad with each step softer — classic plucked-arp feel.
```
notes:[{beat:0.00,note:"C3",duration:0.2,velocity:100},
       {beat:0.25,note:"E3",duration:0.2,velocity:90},
       {beat:0.50,note:"G3",duration:0.2,velocity:80},
       {beat:0.75,note:"C4",duration:0.2,velocity:70}]
```

## Other tools
- `get_transport` — current {beat, tempo, playing, time_sig, loop bounds}; use before scheduling if you need to know where the playhead is.
- `list_fugues` / `cancel_fugue` / `cancel_fugues_by_tag` / `clear_fugues`
- `send_note_on` / `send_note_off` / `send_cc` — ONE-SHOT only, not for composition
- `send_per_note_pitch_bend` / `send_per_note_pressure` — MIDI 2.0 expression (one-shot)
- `set_param` / `rename_slot` / `list_slots` — parameter-slot automation
- `get_activity` — recent MIDI event log (debugging)

## Gotchas
- Fugues do not play while transport is stopped (check with `get_transport`).
- Tempo changes mid-fugue drift the timing.
- MIDI 2.0 per-note expressions require a MIDI 2.0-capable host/instrument.
- CC numbers above are conventions — individual synths may map them differently. When in doubt, the user's DAW session is the source of truth.
