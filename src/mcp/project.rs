//! Project-layout types describing the host DAW's track and device tree.
//!
//! Pushed to the plugin process via `POST /project_layout` by a host
//! controller extension (Bitwig for v1; Ableton later). The plugin exposes
//! three tiered views to the LLM via MCP:
//!
//! - `get_project_state` — minimal per-instance summary: track name +
//!   primary device. For drum machines, pads include note + name +
//!   sample_name so the LLM can write a drum pattern with correct mapping.
//! - `get_track_info` — full recursive device chain for one instance.
//! - `get_device_parameters` — param detail for one addressed device.
//!
//! All three views serialize the same underlying [`ProjectLayout`] through
//! different wrapper views. The extension sends MIDI note numbers on drum
//! pads; we render them as DAW-convention pitch notation (`"C1"`, C3=60) on the
//! way out so the LLM sees the same note format it already uses elsewhere.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::bridge::CcBridge;
use super::requests::midi_to_name;

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

/// One track's worth of context.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TrackContext {
    pub track_name: String,
    /// Instance ID of the Droplets device on this track, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub droplets_instance_id: Option<String>,
    pub devices: Vec<Device>,
    /// Primary-instrument Remote Controls Page 1, as seen by the host controller
    /// extension. Only populated when a Droplets instance is on this track.
    /// Each entry describes one of the 8 remote-control parameters by index and
    /// human-readable name — the plugin mirrors these onto slots 0–7 so the LLM
    /// can automate named instrument parameters without the user hand-mapping.
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

/// A device on a track's main chain. No recursion beyond drum-machine pads —
/// nested racks / chain selectors / instrument layers would require the
/// extension to walk arbitrary chain depth, which we've intentionally skipped:
/// the LLM makes all of its decisions off the primary-device summary in
/// `get_project_state`, not off deep chain detail.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Device {
    Instrument {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        vendor: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        preset_name: Option<String>,
        /// For samplers with a single loaded sample (Bitwig's Sampler,
        /// Ableton's Simpler). `None` for synths.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sample_name: Option<String>,
    },
    Effect {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        vendor: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        preset_name: Option<String>,
    },
    DrumMachine {
        name: String,
        pads: Vec<DrumPad>,
    },
    /// Fallback for device types the extension doesn't recognize.
    Unknown {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        vendor: Option<String>,
    },
}

/// One pad in a drum machine.
///
/// `note` is a raw MIDI number on the wire (what the extension can easily
/// read). Outgoing serializations to the LLM convert to pitch notation.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DrumPad {
    pub note: u8,
    pub name: String,
    pub devices: Vec<Device>,
}

// =============================================================================
// Tier 1 — `get_project_state`: minimal "what's loaded" per instance.
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
    /// The first instrument/drum-machine on the track's chain, or `None`
    /// when no layout data is available or the chain has only effects.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_device: Option<PrimaryDevice>,
    /// Per-slot hints: what is each of the 16 plugin slots currently named? The
    /// LLM reads this to decide which slot to target when automating a synth
    /// parameter — e.g. if slot 0's name is "Filter Cutoff", a `slot` fugue
    /// targeting slot 0 will drive that mapped parameter in the DAW. Only
    /// non-default names are included (generic "Slot N" entries are elided to
    /// keep the hint surface tight).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub slots: Vec<SlotHint>,
}

/// One slot's name hint for the LLM. `cc` lists the slot's backing MIDI CC
/// number, kept for parity with the standalone hardware path — for host-param
/// automation in a DAW, the LLM should use a `slot` fugue lane keyed by
/// `index`, not emit raw CC.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct SlotHint {
    pub index: u8,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cc: Option<u8>,
}

/// A compact view of the track's primary sound source for tier 1.
/// Drum machines include pad summaries because "what's on each pad" is
/// precisely the reason tier 1 exists for drum tracks.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PrimaryDevice {
    Instrument {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        vendor: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        preset_name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        sample_name: Option<String>,
    },
    DrumMachine {
        name: String,
        pads: Vec<PadSummary>,
    },
}

/// Tier 1 view of a drum pad — just what the LLM needs to pick the right
/// note. Note rendered as DAW pitch notation (`"C1"`, C3=60) not an integer.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct PadSummary {
    /// Pitch notation — `"C1"`, `"F#2"` (DAW convention, C3=60). Comes from converting the raw MIDI
    /// number in [`DrumPad::note`].
    pub note: String,
    pub name: String,
    /// If a single sampler is loaded on the pad, its sample file name.
    /// Surfaced at tier 1 because the whole point of tier 1 for drums is
    /// "which sample sits on which note."
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct OtherTrackSummary {
    pub name: String,
}

impl ProjectState {
    /// Build tier 1 from a layout + the live instance registry. The
    /// registry provides `(id, name)` pairs; the layout provides
    /// per-track device info keyed by instance ID.
    ///
    /// `registered_instances` should be the full list from
    /// `CcBridge::list_instances()` so we still report instances even when
    /// the extension hasn't sent a layout yet.
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
                // Slot hints come from the running plugin instance's slot
                // store (per-instance — each Droplets has its own 16 slots,
                // names, CC mappings). Filter out slots still on the generic
                // default "Slot N" name: until the user or extension has set
                // a meaningful label, there's nothing useful to hint to the
                // LLM and the noise would drown the real hints.
                let slots = CcBridge::get_slots(id)
                    .map(|slot_infos| {
                        slot_infos
                            .into_iter()
                            .filter(|s| !is_default_slot_name(s.index, &s.name))
                            .map(|s| SlotHint {
                                index: s.index as u8,
                                name: s.name,
                                cc: s.cc,
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                InstanceSummary {
                    id: id.clone(),
                    name: name.clone(),
                    track_name: track.map(|t| t.track_name.clone()),
                    primary_device: track.and_then(|t| pick_primary_device(&t.devices)),
                    slots,
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

/// Is this slot still on its factory default name? Used to hide unconfigured
/// slots from `get_project_state` — the defaults advertise standard-CC names
/// ("Filter Cutoff", "Mod Wheel") that don't actually control anything until
/// the user maps the slot, so surfacing them as hints would mislead the LLM.
/// Once the user renames a slot (directly via `rename_slot` MCP tool, the
/// plugin UI, or the Bitwig extension mirroring a Remote Controls Page 1
/// name), this returns false and the hint becomes visible.
fn is_default_slot_name(index: usize, name: &str) -> bool {
    let (_, default_name) = crate::params::default_cc_config(index);
    name == default_name
}

/// Pick the primary sound source on a chain: first instrument or drum
/// machine, ignoring effects. `None` when the chain is effects-only.
fn pick_primary_device(devices: &[Device]) -> Option<PrimaryDevice> {
    for device in devices {
        match device {
            Device::Instrument { name, vendor, preset_name, sample_name, .. } => {
                return Some(PrimaryDevice::Instrument {
                    name: name.clone(),
                    vendor: vendor.clone(),
                    preset_name: preset_name.clone(),
                    sample_name: sample_name.clone(),
                });
            }
            Device::DrumMachine { name, pads } => {
                return Some(PrimaryDevice::DrumMachine {
                    name: name.clone(),
                    pads: pads.iter().map(summarize_pad).collect(),
                });
            }
            Device::Effect { .. } | Device::Unknown { .. } => {
                continue;
            }
        }
    }
    None
}

fn summarize_pad(pad: &DrumPad) -> PadSummary {
    // A pad typically wraps a single Sampler/Simpler in practice. Pull its
    // sample_name up to the pad summary so the LLM sees `kick_808.wav` on
    // `C2` without having to recurse.
    let sample_name = pad.devices.iter().find_map(|d| match d {
        Device::Instrument { sample_name, .. } => sample_name.clone(),
        _ => None,
    });
    PadSummary {
        note: midi_to_name(pad.note),
        name: pad.name.clone(),
        sample_name,
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
                    devices: vec![Device::DrumMachine {
                        name: "Drum Machine".into(),
                        pads: vec![
                            DrumPad {
                                note: 36, // C1 (DAW convention; C3=60)
                                name: "Kick".into(),
                                devices: vec![Device::Instrument {
                                    name: "Sampler".into(),
                                    vendor: Some("Bitwig".into()),
                                    preset_name: None,
                                    sample_name: Some("kick_808.wav".into()),
                                }],
                            },
                            DrumPad {
                                note: 38, // D1 (DAW convention; C3=60)
                                name: "Snare".into(),
                                devices: vec![Device::Instrument {
                                    name: "Sampler".into(),
                                    vendor: Some("Bitwig".into()),
                                    preset_name: None,
                                    sample_name: Some("snare.wav".into()),
                                }],
                            },
                        ],
                    }],
                },
                TrackContext {
                    track_name: "Bass".into(),
                    droplets_instance_id: Some("droplets-bbbb".into()),
                    remote_controls: vec![],
                    devices: vec![
                        Device::Instrument {
                            name: "Serum".into(),
                            vendor: Some("Xfer".into()),
                            preset_name: Some("LD Screamer".into()),
                            sample_name: None,
                        },
                        Device::Effect {
                            name: "Delay".into(),
                            vendor: Some("Bitwig".into()),
                            preset_name: None,
                        },
                    ],
                },
                TrackContext {
                    track_name: "Vocals".into(),
                    droplets_instance_id: None,
                    remote_controls: vec![],
                    devices: vec![Device::Effect {
                        name: "Reverb".into(),
                        vendor: None,
                        preset_name: None,
                    }],
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
    fn tier1_produces_pitch_notation_notes() {
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
    fn tier1_picks_instrument_for_synth_track() {
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
    fn tier1_includes_other_tracks() {
        let layout = sample_layout();
        let state = ProjectState::build(Some(&layout), &sample_instances());
        assert_eq!(state.other_tracks.len(), 1);
        assert_eq!(state.other_tracks[0].name, "Vocals");
    }

    #[test]
    fn tier1_without_layout_still_lists_instances() {
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
