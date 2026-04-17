Simply Droplets — AI-controlled MIDI 1.0/2.0 out of a DAW plugin.

## Multi-instance setup (do first when >1 plugin is loaded)
1. `list_instances` — shows connected IDs like 'droplets-a1b2c3d4'.
2. `set_instance_name` on each to give musical names: 'lead', 'bass', 'pad'. All subsequent calls target these names via the `instance` field.
3. The DAW transport MUST be PLAYING for fugues to produce sound.

## queue_fugue — primary composition tool
Batches multiple fugues into one call. Each fugue is atomic; swap one musical part by queueing a new fugue with the same tag + cancel_mode:'tag:<name>'. The other parts keep playing untouched.

Fugue content types (each in its own fugue for independent control):
- 'notes'               — MIDI notes with auto note-off at beat+duration.
- 'cc'                  — CC automation with smooth per-fugue interpolation.
- 'per_note_pitch_bend' — MIDI 2.0 per-note bend on a held note.
- 'per_note_pressure'   — MIDI 2.0 per-note pressure on a held note.

Shared fields (override per-fugue): duration_beats, quantize ('immediate'|'beat'|'bar'|'bars:N'), loop_mode ('once'|'forever'|N).

Curves (on CC interpolation, and per-segment on per-note point tuples):
- 'linear' (default)  smooth straight line
- 'exp'               ease-in, accelerating (t²)
- 'log'               ease-out, decelerating (1-(1-t)²)
- 'none'              stepped/discrete

See the queue_fugue tool description for a full worked example.

## Musical defaults that actually sound good
- Velocities 60-110 — save 110-120 for hits that need to cut through. Avoid 127 unless aggressive is the point.
- Use `quantize: 'bar'` so updates land on musical boundaries.
- Short fugues (2-8 bars) + loop_mode: 'forever'; replace via tag swap.
- Separate notes and automation into different fugues — update independently.
- Per-note bend/pressure REQUIRE a concurrent notes fugue holding the target note on the same channel; otherwise the expression has nothing to modulate.

## Other tools
- get_transport — current {beat, tempo, playing, time_sig, loop bounds}; use before scheduling if you need to know where the playhead is.
- list_fugues / cancel_fugue / cancel_fugues_by_tag / clear_fugues
- send_note_on / send_note_off / send_cc — ONE-SHOT only, not for composition
- send_per_note_pitch_bend / send_per_note_pressure — MIDI 2.0 expression (one-shot)
- set_param / rename_slot / list_slots — parameter-slot automation
- get_activity — recent MIDI event log (debugging)

## Gotchas
- Fugues do not play while transport is stopped (check with get_transport).
- Tempo changes mid-fugue drift the timing.
- MIDI 2.0 per-note expressions require a MIDI 2.0-capable host/instrument.
