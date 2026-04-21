//! `Note` — MIDI note that deserializes from either a number (0-127) or a
//! pitch-notation name (`"C4"`, `"F#2"`, `"Bb5"`).
//!
//! The LLM and humans both reason about notes better by name than by number,
//! so parsing accepts both. Internally it's a plain `u8`; everything
//! downstream of the MCP boundary uses `.0` directly.

use rmcp::schemars;

/// A MIDI note. Deserializes from either an integer (0-127) or a note name
/// in DAW pitch notation (`C3` = middle C = 60, `F#2`, `Bb4`, `C-2`).
/// Matches the convention used by Bitwig, Ableton, Logic, Reaper, Studio One.
///
/// Internally a plain `u8` — parse at the MCP boundary, use `.0` everywhere
/// downstream. The LLM and humans both reason about notes by name far better
/// than by number; this type lets that ergonomics win happen without touching
/// any audio-thread code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Note(pub u8);

impl std::fmt::Display for Note {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&midi_to_name(self.0))
    }
}

impl serde::Serialize for Note {
    /// Serialize as DAW pitch notation (`"C4"`, `"F#2"`, etc.) — the
    /// same format the Deserialize side accepts and what humans and
    /// LLMs read better than a raw MIDI number.
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&midi_to_name(self.0))
    }
}

impl From<Note> for u8 {
    fn from(n: Note) -> u8 { n.0 }
}

impl<'de> serde::Deserialize<'de> for Note {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct NoteVisitor;
        impl<'de> serde::de::Visitor<'de> for NoteVisitor {
            type Value = Note;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("MIDI note: integer 0-127 or name like 'C4', 'F#3', 'Bb5', 'C-1'")
            }

            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Note, E> {
                if v > 127 {
                    Err(E::custom(format!("MIDI note {} out of range 0-127", v)))
                } else {
                    Ok(Note(v as u8))
                }
            }

            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Note, E> {
                if !(0..=127).contains(&v) {
                    Err(E::custom(format!("MIDI note {} out of range 0-127", v)))
                } else {
                    Ok(Note(v as u8))
                }
            }

            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Note, E> {
                // Accept integer-valued floats (60.0) but reject fractional notes.
                let n = v.trunc();
                if (n - v).abs() > 1e-6 {
                    Err(E::custom(format!("MIDI note must be an integer, got {}", v)))
                } else {
                    self.visit_i64(n as i64)
                }
            }

            fn visit_str<E: serde::de::Error>(self, s: &str) -> Result<Note, E> {
                let t = s.trim();
                // Be permissive: if the string is a plain number, accept it
                // as the integer form. This is forgiving for callers who
                // serialize ints as strings ("60" → Note(60)).
                if !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit() || b == b'-' || b == b'+') {
                    if let Ok(n) = t.parse::<i32>() {
                        return self.visit_i64(n as i64);
                    }
                }
                parse_note_name(t).map(Note).map_err(E::custom)
            }

            fn visit_string<E: serde::de::Error>(self, s: String) -> Result<Note, E> {
                self.visit_str(&s)
            }
        }
        deserializer.deserialize_any(NoteVisitor)
    }
}

impl schemars::JsonSchema for Note {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Note".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let value = serde_json::json!({
            "description": "MIDI note. Accepts either an integer 0-127 or a note name in DAW pitch notation (C3 = middle C = 60, matching Bitwig/Ableton/Logic/Reaper/Studio One). Examples: 'C3' (middle C), 'F#2', 'Bb4', 'C-2' (MIDI 0), 'G8' (MIDI 127). Letter is case-insensitive; '#' = sharp, 'b' = flat. Prefer names over numbers — they're clearer for both humans and LLMs.",
            "oneOf": [
                { "type": "integer", "minimum": 0, "maximum": 127 },
                { "type": "string", "pattern": "^[A-Ga-g][#b]?-?[0-9]+$" }
            ]
        });
        let map = match value {
            serde_json::Value::Object(m) => m,
            _ => unreachable!("json! literal is an object"),
        };
        schemars::Schema::from(map)
    }
}

/// Parse a scientific-pitch-notation note name into its MIDI note number.
///
/// Accepts `C4`, `F#3`, `Bb5`, `C-1` (lowest MIDI note), `G9` (highest).
/// Letter is case-insensitive. `#` means sharp, `b` means flat. Octaves
/// follow the convention where middle C (MIDI 60) is `C4` and MIDI 0 is `C-1`.
pub fn parse_note_name(s: &str) -> Result<u8, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("empty note name".into());
    }
    let bytes = s.as_bytes();

    let semitone: i32 = match bytes[0].to_ascii_uppercase() {
        b'C' => 0,
        b'D' => 2,
        b'E' => 4,
        b'F' => 5,
        b'G' => 7,
        b'A' => 9,
        b'B' => 11,
        _ => return Err(format!("invalid note letter in '{}'", s)),
    };

    // Accidental: '#' = sharp, 'b'/'B' = flat. Case-insensitive so that
    // "DB4" works as well as "Db4" and "db4". This creates a slight
    // ambiguity with the note letter B (e.g. "BB4" parses as B-flat 4,
    // not B-natural in octave B4), but in practice nobody writes a note
    // letter and accidental in two uppercase letters unless they mean a
    // flat, so the case-insensitive reading is the safer bet.
    let mut pos = 1;
    let accidental: i32 = match bytes.get(pos).copied() {
        Some(b'#') => { pos += 1; 1 }
        Some(b'b') | Some(b'B') => { pos += 1; -1 }
        _ => 0,
    };

    let octave_str = &s[pos..];
    let octave: i32 = octave_str
        .parse()
        .map_err(|_| format!("invalid octave in '{}'", s))?;

    // DAW convention (Bitwig, Ableton, Logic, Reaper, Studio One):
    // C3 = MIDI 60 = middle C. MIDI 0 = C-2, MIDI 127 = G8.
    //
    // Not the "scientific pitch" C4=60 convention — the labels that appear
    // in the user's DAW are what matter, and every major DAW uses C3=60.
    let midi = (octave + 2) * 12 + semitone + accidental;
    if !(0..=127).contains(&midi) {
        return Err(format!("note '{}' maps to MIDI {} (out of range 0-127)", s, midi));
    }
    Ok(midi as u8)
}

/// Format a MIDI note number as a scientific-pitch-notation name.
/// Uses sharps for enharmonic names (C# rather than Db) — the common default.
///
/// Follows the DAW convention where C3 = MIDI 60 = middle C. Matches Bitwig,
/// Ableton, Logic, Reaper, Studio One.
pub fn midi_to_name(note: u8) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let octave = (note as i32) / 12 - 2;
    let name = NAMES[(note % 12) as usize];
    format!("{}{}", name, octave)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shorthand for tests: parse and unwrap, or panic with the error.
    fn note(s: &str) -> u8 {
        parse_note_name(s).unwrap_or_else(|e| panic!("parse_note_name({:?}) failed: {}", s, e))
    }

    #[test]
    fn note_name_naturals_in_octave_3() {
        // C3 = middle C = MIDI 60 (DAW convention, matching Bitwig/Ableton/Logic).
        assert_eq!(note("C3"), 60, "middle C");
        assert_eq!(note("D3"), 62);
        assert_eq!(note("E3"), 64);
        assert_eq!(note("F3"), 65);
        assert_eq!(note("G3"), 67);
        assert_eq!(note("A3"), 69, "concert pitch");
        assert_eq!(note("B3"), 71);
    }

    #[test]
    fn note_name_c_across_octaves() {
        // C-2 is MIDI 0; each octave adds 12. C3 = middle C = MIDI 60.
        assert_eq!(note("C-2"), 0);
        assert_eq!(note("C-1"), 12);
        assert_eq!(note("C0"), 24);
        assert_eq!(note("C1"), 36);
        assert_eq!(note("C2"), 48);
        assert_eq!(note("C3"), 60);
        assert_eq!(note("C4"), 72);
        assert_eq!(note("C5"), 84);
        assert_eq!(note("C6"), 96);
        assert_eq!(note("C7"), 108);
        assert_eq!(note("C8"), 120);
    }

    #[test]
    fn note_name_extreme_range() {
        // MIDI 0 = C-2 (lowest), MIDI 127 = G8 (highest).
        assert_eq!(note("C-2"), 0);
        assert_eq!(note("G8"), 127);
    }

    #[test]
    fn note_name_sharps() {
        assert_eq!(note("C#3"), 61);
        assert_eq!(note("D#3"), 63);
        assert_eq!(note("F#3"), 66);
        assert_eq!(note("G#3"), 68);
        assert_eq!(note("A#3"), 70);
    }

    #[test]
    fn note_name_flats_lowercase_b() {
        assert_eq!(note("Db3"), 61);
        assert_eq!(note("Eb3"), 63);
        assert_eq!(note("Gb3"), 66);
        assert_eq!(note("Ab3"), 68);
        assert_eq!(note("Bb3"), 70);
    }

    #[test]
    fn note_name_flats_uppercase_b_case_insensitive() {
        // Uppercase B after a note letter is also parsed as flat, so
        // callers who shout "DB3" get the same result as "Db3".
        assert_eq!(note("DB3"), 61);
        assert_eq!(note("EB3"), 63);
        assert_eq!(note("GB3"), 66);
        assert_eq!(note("AB3"), 68);
        assert_eq!(note("BB3"), 70);
    }

    #[test]
    fn note_name_case_insensitive_letter_every_natural() {
        let pairs = [
            ("c3", 60), ("C3", 60),
            ("d3", 62), ("D3", 62),
            ("e3", 64), ("E3", 64),
            ("f3", 65), ("F3", 65),
            ("g3", 67), ("G3", 67),
            ("a3", 69), ("A3", 69),
            ("b3", 71), ("B3", 71),
        ];
        for (name, expected) in pairs {
            assert_eq!(note(name), expected, "{} should be MIDI {}", name, expected);
        }
    }

    #[test]
    fn note_name_case_insensitive_sharps() {
        let pairs = [
            ("c#3", 61), ("C#3", 61),
            ("d#3", 63), ("D#3", 63),
            ("f#3", 66), ("F#3", 66),
            ("g#3", 68), ("G#3", 68),
            ("a#3", 70), ("A#3", 70),
        ];
        for (name, expected) in pairs {
            assert_eq!(note(name), expected, "{} should be MIDI {}", name, expected);
        }
    }

    #[test]
    fn note_name_case_insensitive_flats_full_matrix() {
        let pairs = [
            ("Db3", 61), ("dB3", 61), ("DB3", 61), ("db3", 61),
            ("Eb3", 63), ("eB3", 63), ("EB3", 63), ("eb3", 63),
            ("Gb3", 66), ("gB3", 66), ("GB3", 66), ("gb3", 66),
            ("Ab3", 68), ("aB3", 68), ("AB3", 68), ("ab3", 68),
            ("Bb3", 70), ("bB3", 70), ("BB3", 70), ("bb3", 70),
        ];
        for (name, expected) in pairs {
            assert_eq!(note(name), expected, "{} should be MIDI {}", name, expected);
        }
    }

    #[test]
    fn note_name_case_insensitive_across_octaves() {
        for octave in -2..=7 {
            let upper = format!("C{}", octave);
            let lower = format!("c{}", octave);
            assert_eq!(note(&upper), note(&lower), "C{} vs c{}", octave, octave);
        }
        let variants = ["Bb2", "bB2", "BB2", "bb2"];
        let first = note(variants[0]);
        for v in &variants[1..] {
            assert_eq!(note(v), first, "{} should equal {}", v, variants[0]);
        }
    }

    #[test]
    fn note_name_enharmonic_equivalents() {
        assert_eq!(note("C#3"), note("Db3"));
        assert_eq!(note("D#3"), note("Eb3"));
        assert_eq!(note("F#3"), note("Gb3"));
        assert_eq!(note("G#3"), note("Ab3"));
        assert_eq!(note("A#3"), note("Bb3"));
    }

    #[test]
    fn note_name_handles_surrounding_whitespace() {
        assert_eq!(note(" C3 "), 60);
        assert_eq!(note("\tF#2\n"), parse_note_name("F#2").unwrap());
    }

    #[test]
    fn note_name_negative_octaves() {
        assert_eq!(note("C-2"), 0);
        assert_eq!(note("C#-2"), 1);
        assert_eq!(note("B-2"), 11);
        assert!(parse_note_name("C-3").is_err(), "C-3 is below MIDI range");
    }

    #[test]
    fn note_name_out_of_range_high() {
        assert_eq!(note("G8"), 127);
        assert!(parse_note_name("G#8").is_err(), "G#8 = 128 should fail");
        assert!(parse_note_name("A8").is_err(), "A8 = 129 should fail");
        assert!(parse_note_name("C9").is_err(), "C9 = 132 should fail");
    }

    #[test]
    fn note_name_rejects_empty() {
        assert!(parse_note_name("").is_err());
        assert!(parse_note_name("   ").is_err(), "whitespace-only after trim");
    }

    #[test]
    fn note_name_rejects_invalid_letter() {
        assert!(parse_note_name("H4").is_err(), "H is not a note letter");
        assert!(parse_note_name("Z3").is_err());
        assert!(parse_note_name("04").is_err(), "digit in letter position");
    }

    #[test]
    fn note_name_rejects_missing_octave() {
        assert!(parse_note_name("C").is_err());
        assert!(parse_note_name("C#").is_err());
        assert!(parse_note_name("Bb").is_err());
    }

    #[test]
    fn note_name_rejects_bad_octave() {
        assert!(parse_note_name("Cx4").is_err(), "x is not a valid accidental");
        assert!(parse_note_name("C4x").is_err(), "trailing garbage");
        assert!(parse_note_name("C4.5").is_err(), "fractional octave");
        assert!(parse_note_name("Cfoo").is_err());
    }

    #[test]
    fn note_name_error_messages_are_helpful() {
        let err = parse_note_name("H4").unwrap_err();
        assert!(err.contains("H4") || err.contains("note letter"),
            "error for 'H4' should mention input or note-letter issue; got: {}", err);
        let err = parse_note_name("C98").unwrap_err();
        assert!(err.contains("C98") || err.contains("range"),
            "range error should mention input or range; got: {}", err);
    }

    // ------------------------------------------------------------------
    // Note — Deserialize (accepts number or name via JSON)
    // ------------------------------------------------------------------

    #[test]
    fn note_deserialize_from_json_integer() {
        let n: Note = serde_json::from_str("60").unwrap();
        assert_eq!(n.0, 60);
        let n: Note = serde_json::from_str("0").unwrap();
        assert_eq!(n.0, 0);
        let n: Note = serde_json::from_str("127").unwrap();
        assert_eq!(n.0, 127);
    }

    #[test]
    fn note_deserialize_from_json_string_name() {
        let n: Note = serde_json::from_str("\"C3\"").unwrap();
        assert_eq!(n.0, 60);
        let n: Note = serde_json::from_str("\"F#2\"").unwrap();
        assert_eq!(n.0, 54);
        let n: Note = serde_json::from_str("\"Bb4\"").unwrap();
        assert_eq!(n.0, 82);
        let n: Note = serde_json::from_str("\"C-2\"").unwrap();
        assert_eq!(n.0, 0);
    }

    #[test]
    fn note_deserialize_from_json_string_number() {
        let n: Note = serde_json::from_str("\"60\"").unwrap();
        assert_eq!(n.0, 60);
        let n: Note = serde_json::from_str("\"127\"").unwrap();
        assert_eq!(n.0, 127);
    }

    #[test]
    fn note_deserialize_from_integer_float() {
        let n: Note = serde_json::from_str("60.0").unwrap();
        assert_eq!(n.0, 60);
    }

    #[test]
    fn note_deserialize_rejects_fractional_float() {
        assert!(serde_json::from_str::<Note>("60.5").is_err());
    }

    #[test]
    fn note_deserialize_rejects_out_of_range_integer() {
        assert!(serde_json::from_str::<Note>("128").is_err());
        assert!(serde_json::from_str::<Note>("-1").is_err());
        assert!(serde_json::from_str::<Note>("200").is_err());
    }

    #[test]
    fn note_deserialize_rejects_invalid_name() {
        assert!(serde_json::from_str::<Note>("\"H4\"").is_err());
        assert!(serde_json::from_str::<Note>("\"hello\"").is_err());
        assert!(serde_json::from_str::<Note>("\"\"").is_err());
    }

    #[test]
    fn note_deserialize_rejects_non_integer_strings_not_names() {
        assert!(serde_json::from_str::<Note>("\"abc\"").is_err());
    }

    // ------------------------------------------------------------------
    // Note — Display (round-trip)
    // ------------------------------------------------------------------

    #[test]
    fn note_display_uses_sharps() {
        assert_eq!(Note(60).to_string(), "C3");
        assert_eq!(Note(61).to_string(), "C#3");
        assert_eq!(Note(70).to_string(), "A#3", "prefers A# over Bb");
        assert_eq!(Note(0).to_string(), "C-2");
        assert_eq!(Note(127).to_string(), "G8");
    }

    #[test]
    fn note_roundtrip_name_to_midi_to_name() {
        for name in ["C3", "C#3", "D3", "D#3", "E3", "F3", "F#3", "G3", "G#3", "A3", "A#3", "B3"] {
            let midi = parse_note_name(name).unwrap();
            let back = Note(midi).to_string();
            assert_eq!(back, name, "roundtrip failed for {}", name);
        }
    }

    #[test]
    fn note_roundtrip_all_midi_values() {
        for midi in 0..=127u8 {
            let name = Note(midi).to_string();
            let parsed = parse_note_name(&name)
                .unwrap_or_else(|e| panic!("roundtrip failed for MIDI {}: name {:?} didn't parse: {}", midi, name, e));
            assert_eq!(parsed, midi, "MIDI {} → {:?} → MIDI {}", midi, name, parsed);
        }
    }
}
