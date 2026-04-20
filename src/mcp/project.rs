//! Project-layout types describing the host DAW's track tree.
//!
//! The host controller extension (Bitwig today; Ableton later) POSTs a
//! [`ProjectLayout`] to the plugin over `/project_layout`. The plugin exposes
//! a single tier-1 view over MCP via `get_project_state`: per Droplets
//! instance, the track name and the track's primary sound source — a synth or
//! a drum machine with its pad map.
//!
//! The wire format and the MCP response share the same Rust types.
//! [`PrimaryDevice`] + [`PadSummary`] are both `Serialize + Deserialize`, with
//! note values rendered as DAW pitch notation (`"C1"`, C3=60) on the wire so
//! the extension does the conversion once and the plugin doesn't need to
//! re-parse / re-format.

use serde::{Deserialize, Serialize};
use ts_rs::TS;


/// Project-wide snapshot pushed by the host controller extension.
///
/// Keyed by track name. Tracks without a Droplets device are included too —
/// the LLM benefits from seeing the full project context ("there's a Vocals
/// track I can't play, but the user may ask about it").
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProjectLayout {
    pub tracks: Vec<TrackContext>,
}

/// One track's worth of context. The extension picks the track's primary
/// sound source (first instrument on the chain, or a drum machine) and
/// encodes only that — nested racks, chain selectors, effects aren't
/// surfaced because the LLM never actually used them.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TrackContext {
    pub track_name: String,
    /// Instance ID of the Droplets device on this track, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub droplets_instance_id: Option<String>,
    /// Primary sound source on this track, or `None` for effects-only tracks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_device: Option<PrimaryDevice>,
    /// Primary-instrument Remote Controls Page 1, as seen by the host controller
    /// extension. Only populated when a Droplets instance is on this track.
    /// Each entry describes one of the 8 remote-control parameters by index and
    /// human-readable name — a hint the plugin/UI can use for slot naming.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remote_controls: Vec<RemoteControlInfo>,
}

/// One parameter on the primary instrument's Remote Controls Page 1.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RemoteControlInfo {
    pub index: u8,
    pub name: String,
}

/// The track's primary sound source. Either a single instrument (synth or
/// sampler) or a drum machine whose pads we enumerate so the LLM can target
/// the right MIDI note for each sound.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PrimaryDevice {
    Instrument {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        vendor: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        preset_name: Option<String>,
    },
    DrumMachine {
        name: String,
        pads: Vec<PadSummary>,
    },
}

/// One pad on a drum machine. `note` is DAW pitch notation (`"C1"`, C3=60)
/// — the extension renders it so the LLM reads the same format it already
/// uses in fugue notes.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PadSummary {
    pub note: String,
    pub name: String,
    /// Preset / sample name surfaced from the pad's nested instrument when
    /// available. For Bitwig's Sampler this is the loaded audio file name,
    /// so the LLM sees `kick_808.wav` next to `C1 — Kick`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_name: Option<String>,
}

// =============================================================================
// `get_project_state` response
// =============================================================================

/// Result of `get_project_state`. Instances listed in registration order.
/// `other_tracks` covers tracks without a Droplets device, so the LLM has
/// full project context even for tracks it can't directly play.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ProjectState {
    pub instances: Vec<InstanceSummary>,
    pub other_tracks: Vec<OtherTrackSummary>,
    /// True when a host controller extension has pushed a layout; false when
    /// the LLM should fall back to asking the user / using GM conventions.
    pub layout_available: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct InstanceSummary {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_name: Option<String>,
    /// The track's primary sound source, or `None` when no layout data is
    /// available or the track is effects-only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_device: Option<PrimaryDevice>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct OtherTrackSummary {
    pub name: String,
}

impl ProjectState {
    /// Build the response from a layout + the live instance registry. The
    /// registry provides `(id, name)` pairs; the layout provides per-track
    /// info keyed by instance ID.
    pub fn build(
        layout: Option<&ProjectLayout>,
        registered_instances: &[(String, String)],
    ) -> Self {
        let layout_available = layout.is_some();
        let empty = ProjectLayout::default();
        let layout = layout.unwrap_or(&empty);

        let instances = registered_instances
            .iter()
            .map(|(id, name)| {
                let track = layout.tracks.iter().find(|t| {
                    t.droplets_instance_id.as_deref() == Some(id.as_str())
                });
                InstanceSummary {
                    id: id.clone(),
                    name: name.clone(),
                    track_name: track.map(|t| t.track_name.clone()),
                    primary_device: track.and_then(|t| t.primary_device.clone()),
                }
            })
            .collect();

        let other_tracks = layout
            .tracks
            .iter()
            .filter(|t| t.droplets_instance_id.is_none())
            .map(|t| OtherTrackSummary { name: t.track_name.clone() })
            .collect();

        Self { instances, other_tracks, layout_available }
    }
}

// =============================================================================
// Controller command stream — plugin → host-extension messages.
// =============================================================================

/// Command the plugin emits to the host controller extension over
/// `/ws/controller`. No variants beyond a no-op keepalive yet; when MCP tools
/// start enqueuing real commands, add variants here without changing the
/// wire framing.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControllerCommand {
    /// No-op — included so the enum is non-empty and so the transport can
    /// be exercised without a real command variant existing yet.
    Noop,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_layout() -> ProjectLayout {
        ProjectLayout {
            tracks: vec![
                TrackContext {
                    track_name: "Drums".into(),
                    droplets_instance_id: Some("droplets-aaaa".into()),
                    remote_controls: vec![],
                    primary_device: Some(PrimaryDevice::DrumMachine {
                        name: "Drum Machine".into(),
                        pads: vec![
                            PadSummary {
                                note: "C1".into(),
                                name: "Kick".into(),
                                sample_name: Some("kick_808.wav".into()),
                            },
                            PadSummary {
                                note: "D1".into(),
                                name: "Snare".into(),
                                sample_name: Some("snare.wav".into()),
                            },
                        ],
                    }),
                },
                TrackContext {
                    track_name: "Bass".into(),
                    droplets_instance_id: Some("droplets-bbbb".into()),
                    remote_controls: vec![],
                    primary_device: Some(PrimaryDevice::Instrument {
                        name: "Serum".into(),
                        vendor: Some("Xfer".into()),
                        preset_name: Some("LD Screamer".into()),
                    }),
                },
                TrackContext {
                    track_name: "Vocals".into(),
                    droplets_instance_id: None,
                    remote_controls: vec![],
                    primary_device: None,
                },
            ],
        }
    }

    fn sample_instances() -> Vec<(String, String)> {
        vec![
            ("droplets-aaaa".into(), "drums".into()),
            ("droplets-bbbb".into(), "bass".into()),
        ]
    }

    #[test]
    fn drum_primary_flows_pads_through() {
        let layout = sample_layout();
        let state = ProjectState::build(Some(&layout), &sample_instances());
        let drums = &state.instances[0];
        assert_eq!(drums.track_name.as_deref(), Some("Drums"));
        let Some(PrimaryDevice::DrumMachine { pads, .. }) = &drums.primary_device else {
            panic!("expected drum machine primary");
        };
        assert_eq!(pads[0].note, "C1");
        assert_eq!(pads[0].sample_name.as_deref(), Some("kick_808.wav"));
        assert_eq!(pads[1].note, "D1");
    }

    #[test]
    fn instrument_primary_carries_preset() {
        let layout = sample_layout();
        let state = ProjectState::build(Some(&layout), &sample_instances());
        let bass = &state.instances[1];
        let Some(PrimaryDevice::Instrument { name, preset_name, .. }) = &bass.primary_device else {
            panic!("expected instrument primary");
        };
        assert_eq!(name, "Serum");
        assert_eq!(preset_name.as_deref(), Some("LD Screamer"));
    }

    #[test]
    fn includes_tracks_without_droplets() {
        let layout = sample_layout();
        let state = ProjectState::build(Some(&layout), &sample_instances());
        assert_eq!(state.other_tracks.len(), 1);
        assert_eq!(state.other_tracks[0].name, "Vocals");
    }

    #[test]
    fn without_layout_still_lists_instances() {
        let state = ProjectState::build(None, &sample_instances());
        assert!(!state.layout_available);
        assert_eq!(state.instances.len(), 2);
        assert!(state.instances[0].primary_device.is_none());
        assert!(state.instances[0].track_name.is_none());
        assert_eq!(state.other_tracks.len(), 0);
    }

    #[test]
    fn round_trip_serde() {
        let layout = sample_layout();
        let json = serde_json::to_string(&layout).unwrap();
        let back: ProjectLayout = serde_json::from_str(&json).unwrap();
        assert_eq!(back.tracks.len(), 3);
    }
}
