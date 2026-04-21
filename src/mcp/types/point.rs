//! `Point` — a beat-anchored value in a continuous-signal trajectory
//! (CC, per-note pitch bend, per-note pressure).
//!
//! Serialized as a flat tuple on the wire (`[beat, value]` or
//! `[beat, value, curve]`) to save LLM tokens. `curve` is the
//! "arriving-at" interpolation tag for the segment ending at this point.

use rmcp::schemars;

/// A single point in a per-note continuous-signal trajectory.
///
/// Serialized as a flat 2- or 3-element array `[beat, value]` or
/// `[beat, value, curve]`. The tuple form keeps LLM output tokens low
/// (no repeated field names) while still allowing per-segment curves.
///
/// `curve` controls interpolation for the segment **arriving at** this point —
/// the "curve to this point" convention. It is ignored on the first point.
/// This lets one fugue combine distinct curves, e.g. `exp` on the way up and
/// `log` on the way down for a crescendo–release shape.
#[derive(Debug, Clone)]
pub struct Point {
    pub beat: f64,
    pub value: f64,
    pub curve: Option<String>,
}

impl<'de> serde::Deserialize<'de> for Point {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct PointVisitor;
        impl<'de> serde::de::Visitor<'de> for PointVisitor {
            type Value = Point;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("array [beat, value] or [beat, value, curve]")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let beat: f64 = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(0, &self))?;
                let value: f64 = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(1, &self))?;
                let curve: Option<String> = seq.next_element()?;
                // Drain any extra elements silently — forward-compat.
                while seq.next_element::<serde::de::IgnoredAny>()?.is_some() {}
                Ok(Point { beat, value, curve })
            }
        }
        deserializer.deserialize_seq(PointVisitor)
    }
}

impl serde::Serialize for Point {
    /// Serialize as the same flat tuple the Deserialize side accepts:
    /// `[beat, value]` when `curve` is absent, `[beat, value, curve]`
    /// when present. Keeps a round-tripped Point structurally equal to
    /// what the LLM wrote on input.
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeSeq;
        let len = if self.curve.is_some() { 3 } else { 2 };
        let mut seq = serializer.serialize_seq(Some(len))?;
        seq.serialize_element(&self.beat)?;
        seq.serialize_element(&self.value)?;
        if let Some(curve) = &self.curve {
            seq.serialize_element(curve)?;
        }
        seq.end()
    }
}

impl schemars::JsonSchema for Point {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Point".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        // Hand-rolled schema: a tuple of 2-3 items where the optional third is
        // a curve enum. prefixItems + items is JSON Schema 2020-12 idiomatic;
        // LLMs also read the description which carries the full vocabulary.
        let value = serde_json::json!({
            "type": "array",
            "description": "Point in a per-note trajectory. Array form [beat, value] or [beat, value, curve]. \
                beat: fugue-relative beat offset. \
                value: semitones (-64 to +64) for pitch bend, or 0.0-1.0 for pressure. \
                curve: optional interpolation for the segment ARRIVING at this point (ignored on first point). \
                One of: 'linear' (default, smooth ramp), 'exp' (ease-in, accelerating), 'log' (ease-out, decelerating), 'none' (step). \
                Example crescendo-then-release on pressure: [[0,0],[2,1,\"exp\"],[4,0,\"log\"]].",
            "minItems": 2,
            "maxItems": 3,
            "prefixItems": [
                { "type": "number", "description": "beat offset" },
                { "type": "number", "description": "target value" }
            ],
            "items": {
                "type": "string",
                "enum": ["linear", "exp", "log", "none"]
            }
        });
        let map = match value {
            serde_json::Value::Object(m) => m,
            _ => unreachable!("json! literal is an object"),
        };
        schemars::Schema::from(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn per_note_point_2_tuple_no_curve() {
        let p: Point = serde_json::from_str("[1.5, 0.8]").unwrap();
        assert_eq!(p.beat, 1.5);
        assert_eq!(p.value, 0.8);
        assert!(p.curve.is_none());
    }

    #[test]
    fn per_note_point_3_tuple_with_curve() {
        let p: Point = serde_json::from_str(r#"[2.0, -1.5, "exp"]"#).unwrap();
        assert_eq!(p.beat, 2.0);
        assert_eq!(p.value, -1.5);
        assert_eq!(p.curve.as_deref(), Some("exp"));
    }

    #[test]
    fn per_note_point_accepts_all_curve_names() {
        for curve in ["linear", "exp", "log", "none"] {
            let json = format!(r#"[0, 0, "{}"]"#, curve);
            let p: Point = serde_json::from_str(&json).unwrap();
            assert_eq!(p.curve.as_deref(), Some(curve));
        }
    }

    #[test]
    fn per_note_point_accepts_unknown_curve_string() {
        // Parse succeeds (we don't validate here); expansion falls back to Linear.
        let p: Point = serde_json::from_str(r#"[0, 0, "bogus"]"#).unwrap();
        assert_eq!(p.curve.as_deref(), Some("bogus"));
    }

    #[test]
    fn per_note_point_extra_elements_are_dropped() {
        let p: Point = serde_json::from_str(r#"[0, 1, "log", 42, "future"]"#).unwrap();
        assert_eq!(p.curve.as_deref(), Some("log"));
    }

    #[test]
    fn per_note_point_integer_values_deserialize() {
        let p: Point = serde_json::from_str("[2, 1]").unwrap();
        assert_eq!(p.beat, 2.0);
        assert_eq!(p.value, 1.0);
    }

    #[test]
    fn per_note_point_negative_values() {
        let p: Point = serde_json::from_str("[0.5, -12.0]").unwrap();
        assert_eq!(p.value, -12.0);
    }

    #[test]
    fn per_note_point_missing_value_fails() {
        let err: Result<Point, _> = serde_json::from_str("[1.0]");
        assert!(err.is_err(), "single-element array should fail");
    }

    #[test]
    fn per_note_point_empty_array_fails() {
        let err: Result<Point, _> = serde_json::from_str("[]");
        assert!(err.is_err(), "empty array should fail");
    }

    #[test]
    fn per_note_point_object_form_fails() {
        let err: Result<Point, _> =
            serde_json::from_str(r#"{"beat": 0, "value": 1}"#);
        assert!(err.is_err(), "object form should not deserialize");
    }

    #[test]
    fn per_note_point_trajectory_parses() {
        let pts: Vec<Point> =
            serde_json::from_str(r#"[[0,0],[2,1,"exp"],[4,0,"log"]]"#).unwrap();
        assert_eq!(pts.len(), 3);
        assert!(pts[0].curve.is_none());
        assert_eq!(pts[1].curve.as_deref(), Some("exp"));
        assert_eq!(pts[2].curve.as_deref(), Some("log"));
    }

    #[test]
    fn per_note_point_trajectory_mixed_tuple_lengths() {
        let pts: Vec<Point> =
            serde_json::from_str(r#"[[0,0],[1,0.5],[2,1,"exp"],[3,0.5],[4,0,"log"]]"#).unwrap();
        assert_eq!(pts.len(), 5);
        assert!(pts[0].curve.is_none());
        assert!(pts[1].curve.is_none());
        assert_eq!(pts[2].curve.as_deref(), Some("exp"));
        assert!(pts[3].curve.is_none());
        assert_eq!(pts[4].curve.as_deref(), Some("log"));
    }

    #[test]
    fn per_note_point_serialize_two_tuple() {
        let p = Point { beat: 1.0, value: 0.5, curve: None };
        let json = serde_json::to_string(&p).unwrap();
        assert_eq!(json, "[1.0,0.5]");
    }

    #[test]
    fn per_note_point_serialize_three_tuple() {
        let p = Point { beat: 1.0, value: 0.5, curve: Some("exp".into()) };
        let json = serde_json::to_string(&p).unwrap();
        assert_eq!(json, "[1.0,0.5,\"exp\"]");
    }
}
