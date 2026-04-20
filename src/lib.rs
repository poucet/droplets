use clack_extensions::{audio_ports::*, gui::*, note_ports::*, params::*, state::*};
use clack_plugin::stream::{InputStream, OutputStream};
use clack_plugin::prelude::*;
use clack_plugin::plugin::features::*;
use crossbeam::channel::{Receiver, Sender};
use rtrb::Consumer;
use std::sync::Arc;
use std::sync::Mutex;

use fugue::{FugueBridge, FugueInfoHandle};
use gui::DropletGui;
use mcp::{CcBridge, MidiMessage};
use midi::DropletMidiProcessor;
use params::DropletParams;

mod midi;
pub mod fugue;
pub mod gui;
pub mod instance_param;
pub mod logger;
pub mod mcp;
pub mod params;

/// Serde helper for serializing u64 as string (for JS BigInt compatibility)
pub mod serde_u64_string {
    use serde::{self, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(value: &u64, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&value.to_string())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<u64, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

pub struct DropletPlugin;

impl Plugin for DropletPlugin {
    type AudioProcessor<'a> = DropletMidiProcessor<'a>;
    type Shared<'a> = DropletShared<'a>;
    type MainThread<'a> = DropletMainThread<'a>;

    fn declare_extensions(builder: &mut PluginExtensions<Self>, _shared: Option<&Self::Shared<'_>>) {
        builder
            .register::<PluginAudioPorts>()
            .register::<PluginNotePorts>()
            .register::<PluginGui>()
            .register::<PluginState>()
            // One read-only informational param exposing the instance ID, so
            // host controller scripts can correlate a device-on-track with an
            // MCP instance. This plugin still doesn't use params for anything
            // musical — it controls other plugins' params via MIDI.
            .register::<PluginParams>();
    }
}

impl DefaultPluginFactory for DropletPlugin {
    fn get_descriptor() -> PluginDescriptor {
        // For VST3 builds, include INSTRUMENT so Ableton routes MIDI correctly.
        // For CLAP builds, use NOTE_EFFECT only (proper categorization for Bitwig etc).
        #[cfg(clap_wrapper_vst3)]
        let features = [NOTE_EFFECT, INSTRUMENT];
        #[cfg(not(clap_wrapper_vst3))]
        let features = [NOTE_EFFECT];

        PluginDescriptor::new("com.simply-chris.simply-droplets", "Simply Droplets")
            .with_vendor("Simply Chris")
            .with_features(features)
    }

    fn new_shared(host: HostSharedHandle) -> Result<Self::Shared<'_>, PluginError> {
        logger::init_logger();
        logger::log_plugin_initialization("Droplets", "Creating shared instance");

        // GUI IPC channel
        let (sender, receiver) = crossbeam::channel::unbounded();
        logger::log_ipc_channel_created();

        // Create shared params (Arc for MCP bridge access)
        let params = Arc::new(DropletParams::new());

        // Generate unique instance ID and register with CcBridge and FugueBridge
        let instance_id = format!("droplets-{:08x}", fastrand::u32(..));
        let midi_consumer = CcBridge::register(&instance_id, Arc::clone(&params));
        let (fugue_consumer, fugue_info_handle) = FugueBridge::register(&instance_id, &instance_id);
        log::info!("Registered MCP instance: {}", instance_id);

        // Start singleton servers (only first instance actually starts them)
        mcp::start_server(mcp::DEFAULT_MCP_PORT);
        gui::server::start_server(gui::server::DEFAULT_GUI_PORT);

        Ok(DropletShared {
            host,
            params,
            ipc_sender: sender,
            ipc_receiver: receiver,
            instance_id,
            midi_consumer: Mutex::new(Some(midi_consumer)),
            fugue_consumer: Mutex::new(Some(fugue_consumer)),
            fugue_info_handle: Mutex::new(Some(fugue_info_handle)),
        })
    }

    fn new_main_thread<'a>(
        _host: HostMainThreadHandle<'a>,
        shared: &'a Self::Shared<'a>,
    ) -> Result<Self::MainThread<'a>, PluginError> {
        logger::log_plugin_initialization("Droplets", "Creating main thread instance");

        Ok(Self::MainThread {
            shared,
            gui: DropletGui::new(),
        })
    }
}

pub struct DropletShared<'a> {
    pub host: HostSharedHandle<'a>,
    pub params: Arc<DropletParams>,
    pub ipc_sender: Sender<serde_json::Value>,
    pub ipc_receiver: Receiver<serde_json::Value>,
    pub instance_id: String,
    /// MIDI consumer - taken by MIDI processor during activation
    pub midi_consumer: Mutex<Option<Consumer<MidiMessage>>>,
    /// Fugue command consumer - taken by MIDI processor during activation
    pub fugue_consumer: Mutex<Option<Consumer<fugue::FugueCommand>>>,
    /// Fugue info handle for lock-free updates from audio thread
    pub fugue_info_handle: Mutex<Option<FugueInfoHandle>>,
}

impl<'a> PluginShared<'a> for DropletShared<'a> {}

impl Drop for DropletShared<'_> {
    fn drop(&mut self) {
        CcBridge::unregister(&self.instance_id);
        FugueBridge::unregister(&self.instance_id);
        log::info!("Unregistered MCP instance: {}", self.instance_id);
    }
}

pub struct DropletMainThread<'a> {
    pub shared: &'a DropletShared<'a>,
    gui: DropletGui,
}

impl<'a> PluginMainThread<'a, DropletShared<'a>> for DropletMainThread<'a> {
    fn on_main_thread(&mut self) {
        // Drain any old IPC messages (no longer used, but prevents queue buildup)
        while self.shared.ipc_receiver.try_recv().is_ok() {}

        // Push realtime updates to webview if GUI is active.
        //
        // Iterate ALL connected instances — each plugin's UI shows data
        // for every instance via the dropdown, not just its own. The push
        // methods are per-instance change-cached so this is cheap even
        // when nothing's moving.
        if self.gui.is_active() {
            for (id, _name) in mcp::CcBridge::list_instances() {
                if let Ok(transport) = FugueBridge::get_transport(&id) {
                    self.gui.push_transport(&id, &transport);
                }
                if let Ok(infos) = FugueBridge::get_fugue_info(&id) {
                    if let Ok(definitions) = FugueBridge::get_definitions(&id) {
                        self.gui.push_fugues(&id, &infos, &definitions);
                    }
                }
            }

            // Push DAW project layout from the host controller extension.
            // `push_project_layout` short-circuits when nothing changed, so
            // this is cheap even though we poll on every main-thread tick.
            if let Some(layout) = mcp::CcBridge::get_project_layout() {
                self.gui.push_project_layout(&layout);
            }
        }
    }
}

/// CLAP State extension - save/load plugin state
impl<'a> PluginStateImpl for DropletMainThread<'a> {
    fn save(&mut self, output: &mut OutputStream) -> Result<(), PluginError> {
        use std::io::Write;

        let id = &self.shared.instance_id;

        let slots: Vec<_> = (0..params::NUM_CC_SLOTS)
            .map(|i| {
                let slot = &self.shared.params.slots[i];
                serde_json::json!({
                    "cc": slot.get_cc(),
                    "channel": slot.get_channel(),
                    "name": slot.get_name(),
                    "value": slot.value.load(),
                })
            })
            .collect();

        let instance_name = mcp::CcBridge::get_name(id);
        let fugues = fugue::FugueBridge::get_definitions(id).unwrap_or_default();

        let state = serde_json::json!({
            "slots": slots,
            "instance_name": instance_name,
            "fugues": fugues,
        });

        let json = serde_json::to_vec(&state).map_err(|_| PluginError::Message("serialize failed"))?;
        output.write_all(&json).map_err(|_| PluginError::Message("write failed"))?;
        Ok(())
    }

    fn load(&mut self, input: &mut InputStream) -> Result<(), PluginError> {
        use std::io::Read;

        let mut data = Vec::new();
        input.read_to_end(&mut data).map_err(|_| PluginError::Message("read failed"))?;

        let state: serde_json::Value = serde_json::from_slice(&data)
            .map_err(|_| PluginError::Message("deserialize failed"))?;

        // Support legacy format (bare array of slots)
        let slots = if state.is_array() {
            state.as_array().unwrap().clone()
        } else {
            state.get("slots").and_then(|v| v.as_array()).cloned().unwrap_or_default()
        };

        for (i, slot_state) in slots.iter().enumerate().take(params::NUM_CC_SLOTS) {
            let slot = &self.shared.params.slots[i];
            if let Some(cc) = slot_state.get("cc").and_then(|v| v.as_u64()) {
                slot.set_cc(cc as u8);
            }
            if let Some(channel) = slot_state.get("channel").and_then(|v| v.as_u64()) {
                slot.set_channel(channel as u8);
            }
            if let Some(name) = slot_state.get("name").and_then(|v| v.as_str()) {
                slot.set_name(name);
            }
            if let Some(value) = slot_state.get("value").and_then(|v| v.as_f64()) {
                slot.value.store(value);
            }
        }

        let id = self.shared.instance_id.clone();

        if let Some(name) = state.get("instance_name").and_then(|v| v.as_str()) {
            let _ = mcp::CcBridge::rename(&id, name);
        }

        if let Some(fugues) = state.get("fugues").and_then(|v| v.as_array()) {
            for fugue_val in fugues {
                if let Ok(def) = serde_json::from_value::<fugue::FugueDefinition>(fugue_val.clone()) {
                    let _ = fugue::FugueBridge::queue(&id, def);
                }
            }
        }

        Ok(())
    }
}

clack_export_entry!(SinglePluginEntry<DropletPlugin>);

// VST3 wrapper export
clap_wrapper::export_vst3!();
