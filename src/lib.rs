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
        //
        // Note: attempts to classify the VST3 build as a MIDI effect
        // (stackable before a synth on the same Ableton track) have not
        // succeeded through the clap-wrapper. Zero audio buses made Ableton
        // reject the plugin at instantiation; category-only changes without
        // INSTRUMENT left the wrapper without a main VST3 category and
        // Ableton still treated it inconsistently. Tracking as a known
        // limitation — workaround is External Instrument routing, fix
        // likely requires wrapper patches.
        #[cfg(clap_wrapper_vst3)]
        let features = [NOTE_EFFECT, INSTRUMENT];
        #[cfg(not(clap_wrapper_vst3))]
        let features = [NOTE_EFFECT];

        // Bundle identity comes from `[package.metadata.bundle]` in
        // Cargo.toml — see `build.rs`, which surfaces these values as
        // compile-time env vars so the plugin descriptor and bundle-folder
        // names stay in sync without maintaining them in two places.
        PluginDescriptor::new(
            env!("DROPLETS_CLAP_BUNDLE_ID"),
            env!("DROPLETS_DISPLAY_NAME"),
        )
            .with_vendor(env!("DROPLETS_VENDOR"))
            .with_features(features)
    }

    fn new_shared(host: HostSharedHandle) -> Result<Self::Shared<'_>, PluginError> {
        logger::init_logger();

        // GUI IPC channel
        let (sender, receiver) = crossbeam::channel::unbounded();

        // Create shared params (Arc for MCP bridge access)
        let params = Arc::new(DropletParams::new());

        // Generate unique instance ID and register with CcBridge and FugueBridge
        let instance_id = format!("droplets-{:08x}", fastrand::u32(..));
        let midi_consumer = CcBridge::register(&instance_id, Arc::clone(&params));
        let (fugue_consumer, fugue_info_handle) = FugueBridge::register(&instance_id, &instance_id);

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
            drag_state: Arc::new(Mutex::new(None)),
        })
    }

    fn new_main_thread<'a>(
        _host: HostMainThreadHandle<'a>,
        shared: &'a Self::Shared<'a>,
    ) -> Result<Self::MainThread<'a>, PluginError> {
        
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
    /// Parent-window handle for native drag-out (Feature 15). Populated
    /// by the CLAP GUI extension's `set_parent` hook; read by the IPC
    /// handler when a drag gesture starts.
    pub drag_state: gui::drag::DragState,
}

impl<'a> PluginShared<'a> for DropletShared<'a> {}

impl Drop for DropletShared<'_> {
    fn drop(&mut self) {
        CcBridge::unregister(&self.instance_id);
        FugueBridge::unregister(&self.instance_id);
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

/// CLAP State extension — persist only *configuration* across saves:
/// instance name + per-slot CC-mapping identity (cc/channel/name).
///
/// Explicitly NOT persisted:
/// - **Fugues.** Fugues are live musical state, not a saved project. Reloading
///   them on project open would re-fire long-running loops that the user may
///   no longer want; the AI should re-queue what's needed for the current
///   session.
/// - **Slot values.** Transient — driven by MCP calls and live CC input.
impl<'a> PluginStateImpl for DropletMainThread<'a> {
    fn save(&mut self, output: &mut OutputStream) -> Result<(), PluginError> {
        use std::io::Write;

        let id = &self.shared.instance_id;

        let slots: Vec<_> = self
            .shared
            .params
            .get_all_slots()
            .into_iter()
            .map(|s| {
                serde_json::json!({
                    "cc": s.cc,
                    "channel": s.channel,
                    "name": s.name,
                })
            })
            .collect();

        let instance_name = mcp::CcBridge::get_name(id);
        let custom_instructions = fugue::settings::get_settings().custom_instructions;

        let state = serde_json::json!({
            "slots": slots,
            "instance_name": instance_name,
            "custom_instructions": custom_instructions,
        });

        let json = serde_json::to_vec(&state).map_err(|_| PluginError::Message("serialize failed"))?;
        output.write_all(&json).map_err(|_| PluginError::Message("write failed"))?;
        Ok(())
    }

    fn load(&mut self, input: &mut InputStream) -> Result<(), PluginError> {
        use std::io::Read;
        use std::sync::Arc;
        use params::CcSlot;

        let mut data = Vec::new();
        input.read_to_end(&mut data).map_err(|_| PluginError::Message("read failed"))?;

        let state: serde_json::Value = serde_json::from_slice(&data)
            .map_err(|_| PluginError::Message("deserialize failed"))?;

        // Support legacy format (bare array of slots).
        let slot_array = if state.is_array() {
            state.as_array().unwrap().clone()
        } else {
            state.get("slots").and_then(|v| v.as_array()).cloned().unwrap_or_default()
        };

        // Rebuild slot list from the persisted array. Slots are dynamic now,
        // so we replace the whole list atomically rather than writing into
        // fixed indices. Any persisted `value` field is intentionally
        // ignored — transient state.
        let mut new_slots: Vec<Arc<CcSlot>> = Vec::with_capacity(slot_array.len());
        for slot_state in slot_array {
            let cc = slot_state.get("cc").and_then(|v| v.as_u64()).unwrap_or(255) as u8;
            let name = slot_state
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("Slot")
                .to_string();
            let slot = Arc::new(CcSlot::new(cc, &name));
            if let Some(channel) = slot_state.get("channel").and_then(|v| v.as_u64()) {
                slot.set_channel(channel as u8);
            }
            new_slots.push(slot);
        }
        if !new_slots.is_empty() {
            self.shared.params.replace_slots(new_slots);
        }

        let id = self.shared.instance_id.clone();

        if let Some(name) = state.get("instance_name").and_then(|v| v.as_str()) {
            let _ = mcp::CcBridge::rename(&id, name);
        }

        // Restore custom instructions (per-project LLM context). These live
        // in the global settings singleton — the MCP server reads them at
        // session start and merges them into the system prompt. If multiple
        // Droplets instances exist in the project, last-loaded wins; that's
        // fine since instructions are session-scoped, not instance-scoped.
        if let Some(custom) = state.get("custom_instructions").and_then(|v| v.as_str()) {
            let mut settings = fugue::settings::get_settings();
            if settings.custom_instructions != custom {
                settings.custom_instructions = custom.to_string();
                let _ = fugue::settings::update_settings(settings);
            }
        }

        // Intentionally ignore any persisted `fugues` array. Fugues are live
        // session state, not saved configuration.

        Ok(())
    }
}

clack_export_entry!(SinglePluginEntry<DropletPlugin>);

// VST3 wrapper export
clap_wrapper::export_vst3!();
