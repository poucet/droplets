use clack_extensions::{audio_ports::*, gui::*, note_ports::*};
use clack_plugin::prelude::*;
use clack_plugin::plugin::features::*;
use crossbeam::channel::{Receiver, Sender};
use rtrb::Consumer;
use std::sync::Arc;
use std::sync::Mutex;

use audio::DropletAudioProcessor;
use gui::DropletGui;
use mcp::{CcBridge, CcMessage};
use params::DropletParams;

mod audio;
mod gui;
pub mod logger;
pub mod mcp;
mod params;

pub struct DropletPlugin;

impl Plugin for DropletPlugin {
    type AudioProcessor<'a> = DropletAudioProcessor<'a>;
    type Shared<'a> = DropletShared<'a>;
    type MainThread<'a> = DropletMainThread<'a>;

    fn declare_extensions(builder: &mut PluginExtensions<Self>, _shared: Option<&Self::Shared<'_>>) {
        builder
            .register::<PluginAudioPorts>()
            .register::<PluginNotePorts>()
            .register::<PluginGui>();
    }
}

impl DefaultPluginFactory for DropletPlugin {
    fn get_descriptor() -> PluginDescriptor {
        PluginDescriptor::new("com.simply-chris.simply-droplets", "Simply Droplets")
            .with_vendor("Simply Chris")
            .with_features([UTILITY])
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
        let cc_consumer = CcBridge::register(&instance_id, Arc::clone(&params));
        log::info!("Registered MCP instance: {}", instance_id);

        // Start singleton MCP server (only first instance actually starts it)
        mcp::start_server(mcp::DEFAULT_MCP_PORT);

        Ok(DropletShared {
            host,
            params,
            ipc_sender: sender,
            ipc_receiver: receiver,
            instance_id,
            cc_consumer: Mutex::new(Some(cc_consumer)),
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
    /// CC consumer - taken by audio processor during activation
    pub cc_consumer: Mutex<Option<Consumer<CcMessage>>>,
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

clack_export_entry!(SinglePluginEntry<DropletPlugin>);

// VST3 wrapper export
clap_wrapper::export_vst3!();
