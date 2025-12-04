use clack_extensions::{audio_ports::*, gui::*, note_ports::*, params::*, state::*};
use clack_plugin::stream::{InputStream, OutputStream};
use clack_plugin::utils::Cookie;
use clack_plugin::prelude::*;
use clack_plugin::plugin::features::*;
use crossbeam::channel::{Receiver, Sender};
use rtrb::Consumer;
use std::sync::Arc;
use std::sync::Mutex;

use gui::DropletGui;
use mcp::{CcBridge, MidiMessage};
use midi::DropletMidiProcessor;
use params::DropletParams;

mod midi;
pub mod gui;
pub mod logger;
pub mod mcp;
pub mod params;

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
            .register::<PluginParams>()
            .register::<PluginState>();
    }
}

impl DefaultPluginFactory for DropletPlugin {
    fn get_descriptor() -> PluginDescriptor {
        PluginDescriptor::new("com.simply-chris.simply-droplets", "Simply Droplets")
            .with_vendor("Simply Chris")
            .with_features([NOTE_EFFECT, UTILITY])
    }

    fn new_shared(host: HostSharedHandle) -> Result<Self::Shared<'_>, PluginError> {
        logger::init_logger();
        logger::log_plugin_initialization("Droplets", "Creating shared instance");

        // GUI IPC channel
        let (sender, receiver) = crossbeam::channel::unbounded();
        logger::log_ipc_channel_created();

        // Create shared params (Arc for MCP bridge access)
        let params = Arc::new(DropletParams::new());

        // Generate unique instance ID and register with CcBridge
        let instance_id = format!("droplets-{:08x}", fastrand::u32(..));
        let midi_consumer = CcBridge::register(&instance_id, Arc::clone(&params));
        log::info!("Registered MCP instance: {}", instance_id);

        // Start singleton MCP server (only first instance actually starts it)
        mcp::start_server(mcp::DEFAULT_MCP_PORT);

        Ok(DropletShared {
            host,
            params,
            ipc_sender: sender,
            ipc_receiver: receiver,
            instance_id,
            midi_consumer: Mutex::new(Some(midi_consumer)),
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
}

impl<'a> PluginShared<'a> for DropletShared<'a> {}

impl Drop for DropletShared<'_> {
    fn drop(&mut self) {
        CcBridge::unregister(&self.instance_id);
        log::info!("Unregistered MCP instance: {}", self.instance_id);
    }
}

pub struct DropletMainThread<'a> {
    pub shared: &'a DropletShared<'a>,
    gui: DropletGui,
}

impl<'a> PluginMainThread<'a, DropletShared<'a>> for DropletMainThread<'a> {
    fn on_main_thread(&mut self) {
        // IPC is now handled via custom protocol in gui/mod.rs
        // This callback can be used for any future main-thread-only operations

        // Drain any old IPC messages (no longer used, but prevents queue buildup)
        while self.shared.ipc_receiver.try_recv().is_ok() {}
    }
}

/// Base ID for slot parameters (CLAP IDs can't be 0, so we start at 1)
const SLOT_PARAM_ID_BASE: u32 = 1;

/// Convert slot index to CLAP param ID
fn slot_to_param_id(slot_index: usize) -> Option<ClapId> {
    ClapId::from_raw(slot_index as u32 + SLOT_PARAM_ID_BASE)
}

/// Convert CLAP param ID to slot index
fn param_id_to_slot(param_id: ClapId) -> Option<usize> {
    let raw = param_id.get();
    if raw >= SLOT_PARAM_ID_BASE {
        let index = (raw - SLOT_PARAM_ID_BASE) as usize;
        if index < params::NUM_CC_SLOTS {
            return Some(index);
        }
    }
    None
}

/// CLAP Params extension - exposes slot values as automatable parameters
impl<'a> PluginMainThreadParams for DropletMainThread<'a> {
    fn count(&mut self) -> u32 {
        params::NUM_CC_SLOTS as u32
    }

    fn get_info(&mut self, param_index: u32, info: &mut ParamInfoWriter) {
        let index = param_index as usize;
        if index < params::NUM_CC_SLOTS {
            let name = self.shared.params.slots[index].get_name();
            if let Some(id) = slot_to_param_id(index) {
                info.set(&ParamInfo {
                    id,
                    flags: ParamInfoFlags::IS_AUTOMATABLE | ParamInfoFlags::IS_MODULATABLE,
                    cookie: Cookie::empty(),
                    name: name.as_bytes(),
                    module: b"Slots",
                    min_value: 0.0,
                    max_value: 1.0,
                    default_value: 0.0,
                });
            }
        }
    }

    fn get_value(&mut self, param_id: ClapId) -> Option<f64> {
        param_id_to_slot(param_id).map(|index| self.shared.params.get_slot(index))
    }

    fn value_to_text(
        &mut self,
        param_id: ClapId,
        value: f64,
        writer: &mut ParamDisplayWriter,
    ) -> std::fmt::Result {
        use std::fmt::Write;
        if param_id_to_slot(param_id).is_some() {
            write!(writer, "{:.1}%", value * 100.0)
        } else {
            Err(std::fmt::Error)
        }
    }

    fn text_to_value(&mut self, param_id: ClapId, text: &core::ffi::CStr) -> Option<f64> {
        param_id_to_slot(param_id)?;
        let input = text.to_str().ok()?;
        // Handle percentage values (strip % and divide by 100)
        let trimmed = input.trim().trim_end_matches('%').trim();
        trimmed.parse::<f64>().ok().map(|v| (v / 100.0).clamp(0.0, 1.0))
    }

    fn flush(
        &mut self,
        input_parameter_changes: &InputEvents,
        _output_parameter_changes: &mut OutputEvents,
    ) {
        for event in input_parameter_changes {
            if let Some(clack_plugin::events::spaces::CoreEventSpace::ParamValue(pv)) =
                event.as_core_event()
            {
                if let Some(param_id) = pv.param_id() {
                    if let Some(index) = param_id_to_slot(param_id) {
                        self.shared.params.slots[index].value.store(pv.value());
                    }
                }
            }
        }
    }
}

/// CLAP State extension - save/load plugin state
impl<'a> PluginStateImpl for DropletMainThread<'a> {
    fn save(&mut self, output: &mut OutputStream) -> Result<(), PluginError> {
        use std::io::Write;

        // Serialize slot state as JSON
        let state: Vec<_> = (0..params::NUM_CC_SLOTS)
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

        let json = serde_json::to_vec(&state).map_err(|_| PluginError::Message("serialize failed"))?;
        output.write_all(&json).map_err(|_| PluginError::Message("write failed"))?;
        Ok(())
    }

    fn load(&mut self, input: &mut InputStream) -> Result<(), PluginError> {
        use std::io::Read;

        let mut data = Vec::new();
        input.read_to_end(&mut data).map_err(|_| PluginError::Message("read failed"))?;

        let state: Vec<serde_json::Value> = serde_json::from_slice(&data)
            .map_err(|_| PluginError::Message("deserialize failed"))?;

        for (i, slot_state) in state.iter().enumerate().take(params::NUM_CC_SLOTS) {
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

        Ok(())
    }
}

clack_export_entry!(SinglePluginEntry<DropletPlugin>);

// VST3 wrapper export
clap_wrapper::export_vst3!();
