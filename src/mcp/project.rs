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

/// A device somewhere in a chain. Recursive through `DrumMachine`/`Container`.
///
/// `Unknown` is the graceful fallback for device types the host extension
/// hasn't special-cased. The LLM can still reason about "there's something
/// called X here" without breaking when new device types appear.
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
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        parameters: Vec<ParameterInfo>,
    },
    Effect {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        vendor: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        preset_name: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        parameters: Vec<ParameterInfo>,
    },
    DrumMachine {
        name: String,
        pads: Vec<DrumPad>,
    },
    /// Anything holding nested chains: chain selectors, instrument layers,
    /// nested racks. `kind` is a free-form discriminator the extension picks
    /// (e.g. `"chain_selector"`, `"instrument_layer"`); the LLM just sees it
    /// as context.
    Container {
        name: String,
        kind: String,
        chains: Vec<Chain>,
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
/// read). Outgoing serializations rendered for the LLM convert to pitch
/// notation via [`PadView`].
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DrumPad {
    pub note: u8,
    pub name: String,
    pub devices: Vec<Device>,
}

/// A named chain inside a container device.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Chain {
    pub name: String,
    pub devices: Vec<Device>,
}

/// Parameter info for tier 3 (`get_device_parameters`).
///
/// Kept separate from the always-present Device fields so Tier 1/2 views can
/// skip them without an explicit schema change. The extension may still
/// populate `parameters` on every device — we just hide them in the tier 1/2
/// wrappers.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ParameterInfo {
    pub name: String,
    pub index: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub displayed_value: Option<String>,
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
                InstanceSummary {
                    id: id.clone(),
                    name: name.clone(),
                    track_name: track.map(|t| t.track_name.clone()),
                    primary_device: track.and_then(|t| pick_primary_device(&t.devices)),
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
            Device::Effect { .. } | Device::Container { .. } | Device::Unknown { .. } => {
                // Containers *could* wrap an instrument, but for tier 1 we
                // intentionally flatten — if the user has a complex layered
                // synth, tier 2 is the right level of detail. Tier 1 stays
                // at "effects-only track => no primary_device".
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
// Tier 2 — `get_track_info`: full chain for one instance, no params.
// =============================================================================

/// Tier 2 view of one track. Mirrors [`TrackContext`] but drops `parameters`
/// on every device via [`DeviceTier2`].
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct TrackInfo {
    pub track_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub droplets_instance_id: Option<String>,
    pub devices: Vec<DeviceTier2>,
}

/// Tier 2 Device — identical shape to [`Device`] but with `parameters`
/// stripped. Drum pads still recurse through this view so their inner
/// chains are visible without param clutter.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DeviceTier2 {
    Instrument {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        vendor: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        preset_name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        sample_name: Option<String>,
    },
    Effect {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        vendor: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        preset_name: Option<String>,
    },
    DrumMachine {
        name: String,
        pads: Vec<DrumPadTier2>,
    },
    Container {
        name: String,
        kind: String,
        chains: Vec<ChainTier2>,
    },
    Unknown {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        vendor: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct DrumPadTier2 {
    /// Pitch notation, same as tier 1.
    pub note: String,
    pub name: String,
    pub devices: Vec<DeviceTier2>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ChainTier2 {
    pub name: String,
    pub devices: Vec<DeviceTier2>,
}

impl TrackInfo {
    /// Build tier 2 for the track that has the given Droplets instance.
    /// Returns `None` when no layout is available or no track matches.
    pub fn build(layout: Option<&ProjectLayout>, instance_id: &str) -> Option<Self> {
        let layout = layout?;
        let track = layout
            .tracks
            .iter()
            .find(|t| t.droplets_instance_id.as_deref() == Some(instance_id))?;
        Some(Self {
            track_name: track.track_name.clone(),
            droplets_instance_id: track.droplets_instance_id.clone(),
            devices: track.devices.iter().map(DeviceTier2::from).collect(),
        })
    }
}

impl From<&Device> for DeviceTier2 {
    fn from(d: &Device) -> Self {
        match d {
            Device::Instrument { name, vendor, preset_name, sample_name, .. } => {
                Self::Instrument {
                    name: name.clone(),
                    vendor: vendor.clone(),
                    preset_name: preset_name.clone(),
                    sample_name: sample_name.clone(),
                }
            }
            Device::Effect { name, vendor, preset_name, .. } => Self::Effect {
                name: name.clone(),
                vendor: vendor.clone(),
                preset_name: preset_name.clone(),
            },
            Device::DrumMachine { name, pads } => Self::DrumMachine {
                name: name.clone(),
                pads: pads
                    .iter()
                    .map(|p| DrumPadTier2 {
                        note: midi_to_name(p.note),
                        name: p.name.clone(),
                        devices: p.devices.iter().map(DeviceTier2::from).collect(),
                    })
                    .collect(),
            },
            Device::Container { name, kind, chains } => Self::Container {
                name: name.clone(),
                kind: kind.clone(),
                chains: chains
                    .iter()
                    .map(|c| ChainTier2 {
                        name: c.name.clone(),
                        devices: c.devices.iter().map(DeviceTier2::from).collect(),
                    })
                    .collect(),
            },
            Device::Unknown { name, vendor } => Self::Unknown {
                name: name.clone(),
                vendor: vendor.clone(),
            },
        }
    }
}

// =============================================================================
// Tier 3 — `get_device_parameters`: param detail for one addressed device.
// =============================================================================

/// A path addressing a specific device within a track's chain. Form:
/// `"device:0/pad:36/device:1"` — alternating `device:<index>` for chain
/// positions and `pad:<midi_note>` when descending into a drum machine.
/// Note-based pad addressing is more stable than index-based across
/// reorderings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathSegment {
    /// Index into a `devices` list (top-level chain or a container chain).
    Device(usize),
    /// Descent into a drum machine by its pad's MIDI note number.
    Pad(u8),
    /// Index into a container's chains list. Paired with subsequent
    /// `Device(n)` segments to address devices within that chain.
    Chain(usize),
}

/// Parse a path string like `"device:0/pad:36/device:1"` into segments.
pub fn parse_device_path(path: &str) -> Result<Vec<PathSegment>, String> {
    path.split('/')
        .filter(|s| !s.is_empty())
        .map(|s| {
            let (kind, num) = s
                .split_once(':')
                .ok_or_else(|| format!("malformed path segment '{}'", s))?;
            let num: u64 = num
                .parse()
                .map_err(|_| format!("non-numeric index in segment '{}'", s))?;
            match kind {
                "device" => Ok(PathSegment::Device(num as usize)),
                "pad" => {
                    if num > 127 {
                        return Err(format!("pad note {} out of MIDI range", num));
                    }
                    Ok(PathSegment::Pad(num as u8))
                }
                "chain" => Ok(PathSegment::Chain(num as usize)),
                other => Err(format!("unknown path segment kind '{}'", other)),
            }
        })
        .collect()
}

/// Resolve a path through the given top-level devices list, returning the
/// addressed device. Returns `None` if any segment is out of bounds or the
/// path descends into a device type that doesn't support further descent.
pub fn resolve_path<'a>(
    mut devices: &'a [Device],
    path: &[PathSegment],
) -> Option<&'a Device> {
    let mut current: Option<&'a Device> = None;
    for seg in path {
        match seg {
            PathSegment::Device(i) => {
                let d = devices.get(*i)?;
                current = Some(d);
                // Pre-position `devices` for the next segment if it's a pad
                // or chain descent. For `Device`, `devices` stays pointing
                // at the same siblings — a chain of `device:1/device:2`
                // doesn't make sense; real chains alternate kinds. We'll
                // pick up the right subslice from `pad`/`chain` below.
            }
            PathSegment::Pad(note) => {
                let d = current?;
                if let Device::DrumMachine { pads, .. } = d {
                    let pad = pads.iter().find(|p| p.note == *note)?;
                    devices = &pad.devices;
                    current = None; // Pad isn't itself a Device; next segment resolves within.
                } else {
                    return None;
                }
            }
            PathSegment::Chain(i) => {
                let d = current?;
                if let Device::Container { chains, .. } = d {
                    let chain = chains.get(*i)?;
                    devices = &chain.devices;
                    current = None;
                } else {
                    return None;
                }
            }
        }
    }
    current
}

// =============================================================================
// Controller command stream — plugin → host-extension messages.
// =============================================================================

/// Command the plugin emits to the host controller extension over
/// `/ws/controller`. v1 defines no variants beyond a no-op keepalive; when
/// MCP tools start enqueuing real commands (MIDI map changes etc.), add
/// variants here without changing the wire framing.
///
/// Serialized with `#[serde(tag = "type")]` so variants appear as
/// `{"type": "...", ...}` on the wire.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControllerCommand {
    /// No-op — included so the enum is non-empty and so the transport can
    /// be exercised without a real command variant existing yet.
    Noop,
}

/// Tier 3 response — just the parameter list for one addressed device.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct DeviceParameters {
    pub device_name: String,
    pub parameters: Vec<ParameterInfo>,
}

impl DeviceParameters {
    pub fn build(
        layout: Option<&ProjectLayout>,
        instance_id: &str,
        device_path: &str,
    ) -> Result<Self, String> {
        let layout = layout.ok_or_else(|| "no project layout available".to_string())?;
        let track = layout
            .tracks
            .iter()
            .find(|t| t.droplets_instance_id.as_deref() == Some(instance_id))
            .ok_or_else(|| format!("no track found for instance '{}'", instance_id))?;

        let segments = parse_device_path(device_path)?;
        let device = resolve_path(&track.devices, &segments)
            .ok_or_else(|| format!("device_path '{}' did not resolve", device_path))?;

        let (name, parameters) = match device {
            Device::Instrument { name, parameters, .. }
            | Device::Effect { name, parameters, .. } => (name.clone(), parameters.clone()),
            Device::DrumMachine { name, .. } => (name.clone(), Vec::new()),
            Device::Container { name, .. } => (name.clone(), Vec::new()),
            Device::Unknown { name, .. } => (name.clone(), Vec::new()),
        };

        Ok(Self { device_name: name, parameters })
    }
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
                                    parameters: vec![],
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
                                    parameters: vec![],
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
                            parameters: vec![ParameterInfo {
                                name: "Cutoff".into(),
                                index: 0,
                                displayed_value: Some("440 Hz".into()),
                            }],
                        },
                        Device::Effect {
                            name: "Delay".into(),
                            vendor: Some("Bitwig".into()),
                            preset_name: None,
                            parameters: vec![ParameterInfo {
                                name: "Amount".into(),
                                index: 0,
                                displayed_value: Some("50%".into()),
                            }],
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
                        parameters: vec![],
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
    fn tier2_drops_parameters() {
        let layout = sample_layout();
        let info = TrackInfo::build(Some(&layout), "droplets-bbbb").unwrap();
        assert_eq!(info.track_name, "Bass");
        // Tier 2 types don't even have a parameters field, which is the whole
        // point — serialize and verify no param keys leak.
        let json = serde_json::to_value(&info).unwrap();
        let s = json.to_string();
        assert!(!s.contains("parameters"), "tier2 should not serialize parameters: {}", s);
        assert!(!s.contains("Cutoff"), "tier2 should not contain param names: {}", s);
    }

    #[test]
    fn tier2_renders_pad_notes_as_names() {
        let layout = sample_layout();
        let info = TrackInfo::build(Some(&layout), "droplets-aaaa").unwrap();
        let DeviceTier2::DrumMachine { pads, .. } = &info.devices[0] else {
            panic!("expected drum machine");
        };
        assert_eq!(pads[0].note, "C1");
    }

    #[test]
    fn tier2_missing_instance_returns_none() {
        let layout = sample_layout();
        assert!(TrackInfo::build(Some(&layout), "droplets-missing").is_none());
        assert!(TrackInfo::build(None, "droplets-aaaa").is_none());
    }

    #[test]
    fn parse_path_basic() {
        let segs = parse_device_path("device:0/pad:36/device:1").unwrap();
        assert_eq!(
            segs,
            vec![
                PathSegment::Device(0),
                PathSegment::Pad(36),
                PathSegment::Device(1),
            ]
        );
    }

    #[test]
    fn parse_path_rejects_invalid_pad_note() {
        assert!(parse_device_path("device:0/pad:200").is_err());
    }

    #[test]
    fn parse_path_rejects_malformed() {
        assert!(parse_device_path("bogus").is_err());
        assert!(parse_device_path("device:abc").is_err());
        assert!(parse_device_path("unknown:0").is_err());
    }

    #[test]
    fn tier3_resolves_effect_on_chain() {
        let layout = sample_layout();
        let result =
            DeviceParameters::build(Some(&layout), "droplets-bbbb", "device:1").unwrap();
        assert_eq!(result.device_name, "Delay");
        assert_eq!(result.parameters.len(), 1);
        assert_eq!(result.parameters[0].name, "Amount");
    }

    #[test]
    fn tier3_resolves_instrument_inside_pad() {
        let layout = sample_layout();
        // device:0 = drum machine, pad:36 = kick, device:0 = sampler within pad
        let result =
            DeviceParameters::build(Some(&layout), "droplets-aaaa", "device:0/pad:36/device:0")
                .unwrap();
        assert_eq!(result.device_name, "Sampler");
    }

    #[test]
    fn tier3_errors_on_unknown_path() {
        let layout = sample_layout();
        assert!(DeviceParameters::build(Some(&layout), "droplets-bbbb", "device:99").is_err());
        assert!(DeviceParameters::build(Some(&layout), "droplets-missing", "device:0").is_err());
        assert!(DeviceParameters::build(None, "droplets-aaaa", "device:0").is_err());
    }

    #[test]
    fn round_trip_serde() {
        let layout = sample_layout();
        let json = serde_json::to_string(&layout).unwrap();
        let back: ProjectLayout = serde_json::from_str(&json).unwrap();
        assert_eq!(back.tracks.len(), 3);
    }
}
