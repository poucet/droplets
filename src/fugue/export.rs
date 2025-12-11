//! MIDI file export for fugues
//!
//! Converts FugueDefinition to Standard MIDI File (SMF) format.

use midly::{
    Format, Header, MetaMessage, MidiMessage, Smf, Track, TrackEvent, TrackEventKind,
    num::{u14, u15, u24, u28, u4, u7},
};
use std::collections::HashMap;

use super::types::{FugueDefinition, FugueEvent, InterpolationMode, TimedFugueEvent};

/// Pulses per quarter note (standard MIDI resolution)
const PPQ: u16 = 480;

/// Resolution for CC interpolation (1/32 note = PPQ/8 ticks)
const CC_INTERP_RESOLUTION_TICKS: u32 = (PPQ as u32) / 8;

/// Convert a FugueDefinition to a Standard MIDI File
///
/// # Arguments
/// * `definition` - The fugue to export
/// * `tempo_bpm` - Tempo in beats per minute
///
/// # Returns
/// Raw bytes of the MIDI file
pub fn fugue_to_smf(definition: &FugueDefinition, tempo_bpm: f64) -> Vec<u8> {
    let mut track_events: Vec<(u32, TrackEventKind<'static>)> = Vec::new();

    // Add tempo meta event at tick 0
    let tempo_microseconds = (60_000_000.0 / tempo_bpm) as u32;
    track_events.push((0, TrackEventKind::Meta(MetaMessage::Tempo(u24::new(tempo_microseconds)))));

    // Collect all events with absolute tick positions
    let events = if definition.cc_interpolation == InterpolationMode::Linear {
        interpolate_cc_events(&definition.events, definition.duration_beats)
    } else {
        definition.events.clone()
    };

    for timed_event in &events {
        let tick = beat_to_tick(timed_event.beat_offset);
        let kind = fugue_event_to_midi(&timed_event.event);
        if let Some(k) = kind {
            track_events.push((tick, k));
        }
    }

    // Sort by tick position
    track_events.sort_by_key(|(tick, _)| *tick);

    // Convert absolute ticks to delta times
    let mut track: Track<'static> = Vec::new();
    let mut last_tick = 0u32;

    for (tick, kind) in track_events {
        let delta = tick.saturating_sub(last_tick);
        track.push(TrackEvent {
            delta: u28::new(delta),
            kind,
        });
        last_tick = tick;
    }

    // Add end of track
    track.push(TrackEvent {
        delta: u28::new(0),
        kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
    });

    // Build SMF
    let smf = Smf {
        header: Header {
            format: Format::SingleTrack,
            timing: midly::Timing::Metrical(u15::new(PPQ)),
        },
        tracks: vec![track],
    };

    // Write to bytes
    let mut buffer = Vec::new();
    smf.write_std(&mut buffer).expect("Failed to write MIDI file");
    buffer
}

/// Convert beat offset to MIDI tick
fn beat_to_tick(beat: f64) -> u32 {
    (beat * PPQ as f64).round() as u32
}

/// Convert a FugueEvent to a MIDI TrackEventKind
fn fugue_event_to_midi(event: &FugueEvent) -> Option<TrackEventKind<'static>> {
    match event {
        FugueEvent::NoteOn { channel, note, velocity } => {
            Some(TrackEventKind::Midi {
                channel: u4::new(*channel & 0x0F),
                message: MidiMessage::NoteOn {
                    key: u7::new(*note & 0x7F),
                    vel: u7::new(*velocity & 0x7F),
                },
            })
        }
        FugueEvent::NoteOff { channel, note } => {
            Some(TrackEventKind::Midi {
                channel: u4::new(*channel & 0x0F),
                message: MidiMessage::NoteOff {
                    key: u7::new(*note & 0x7F),
                    vel: u7::new(0),
                },
            })
        }
        FugueEvent::Cc { channel, cc, value } => {
            Some(TrackEventKind::Midi {
                channel: u4::new(*channel & 0x0F),
                message: MidiMessage::Controller {
                    controller: u7::new(*cc & 0x7F),
                    value: u7::new(*value & 0x7F),
                },
            })
        }
        FugueEvent::PerNotePitchBend { channel, semitones, .. } => {
            // Convert semitones to pitch bend value (assuming ±48 semitone range)
            // Center is 8192, range is 0-16383
            let bend = (8192.0 + (semitones / 48.0) * 8192.0).clamp(0.0, 16383.0) as u16;
            Some(TrackEventKind::Midi {
                channel: u4::new(*channel & 0x0F),
                message: MidiMessage::PitchBend {
                    bend: midly::PitchBend(u14::new(bend)),
                },
            })
        }
        FugueEvent::PerNotePressure { channel, note, pressure } => {
            // Convert to polyphonic aftertouch (per-note pressure)
            let value = (pressure * 127.0).clamp(0.0, 127.0) as u8;
            Some(TrackEventKind::Midi {
                channel: u4::new(*channel & 0x0F),
                message: MidiMessage::Aftertouch {
                    key: u7::new(*note & 0x7F),
                    vel: u7::new(value),
                },
            })
        }
    }
}

/// Interpolate CC events for smooth automation export
///
/// For Linear interpolation mode, generates intermediate CC values between
/// consecutive CC events on the same channel/CC number.
fn interpolate_cc_events(events: &[TimedFugueEvent], _duration_beats: f64) -> Vec<TimedFugueEvent> {
    let mut result: Vec<TimedFugueEvent> = Vec::new();

    // Group CC events by (channel, cc) for interpolation
    let mut cc_events: HashMap<(u8, u8), Vec<(f64, u8)>> = HashMap::new();
    let mut non_cc_events: Vec<TimedFugueEvent> = Vec::new();

    for event in events {
        match &event.event {
            FugueEvent::Cc { channel, cc, value } => {
                cc_events
                    .entry((*channel, *cc))
                    .or_default()
                    .push((event.beat_offset, *value));
            }
            _ => {
                non_cc_events.push(event.clone());
            }
        }
    }

    // Add non-CC events as-is
    result.extend(non_cc_events);

    // Interpolate each CC lane
    let resolution_beats = CC_INTERP_RESOLUTION_TICKS as f64 / PPQ as f64;

    for ((channel, cc), mut points) in cc_events {
        // Sort by beat offset
        points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

        if points.is_empty() {
            continue;
        }

        // Always include first point
        result.push(TimedFugueEvent::cc(points[0].0, channel, cc, points[0].1));

        // Interpolate between consecutive points
        for window in points.windows(2) {
            let (start_beat, start_val) = window[0];
            let (end_beat, end_val) = window[1];

            if start_val == end_val {
                // No interpolation needed, just add end point
                result.push(TimedFugueEvent::cc(end_beat, channel, cc, end_val));
                continue;
            }

            // Generate intermediate points
            let mut current_beat = start_beat + resolution_beats;
            while current_beat < end_beat {
                let t = (current_beat - start_beat) / (end_beat - start_beat);
                let value = lerp(start_val as f64, end_val as f64, t) as u8;
                result.push(TimedFugueEvent::cc(current_beat, channel, cc, value));
                current_beat += resolution_beats;
            }

            // Add end point
            result.push(TimedFugueEvent::cc(end_beat, channel, cc, end_val));
        }
    }

    // Sort all events by beat offset
    result.sort_by(|a, b| a.beat_offset.partial_cmp(&b.beat_offset).unwrap());

    result
}

/// Linear interpolation
fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// Generate a filename for the exported MIDI file
pub fn generate_filename(definition: &FugueDefinition) -> String {
    let tag = definition.tag.as_deref().unwrap_or("fugue");
    // Sanitize tag for filename
    let safe_tag: String = tag
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{}_{}.mid", safe_tag, timestamp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_export() {
        let events = vec![
            TimedFugueEvent::note_on(0.0, 0, 60, 100),
            TimedFugueEvent::note_off(1.0, 0, 60),
        ];
        let definition = FugueDefinition::new(events, 1.0);
        let midi_bytes = fugue_to_smf(&definition, 120.0);
        assert!(!midi_bytes.is_empty());
        // Check MIDI header magic bytes
        assert_eq!(&midi_bytes[0..4], b"MThd");
    }

    #[test]
    fn test_cc_interpolation() {
        let events = vec![
            TimedFugueEvent::cc(0.0, 0, 1, 0),
            TimedFugueEvent::cc(1.0, 0, 1, 127),
        ];
        let interpolated = interpolate_cc_events(&events, 1.0);
        // Should have more than 2 events due to interpolation
        assert!(interpolated.len() > 2);
    }
}
