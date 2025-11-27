use clack_extensions::{audio_ports::*, gui::*, note_ports::*};
use clack_plugin::prelude::*;
use clack_plugin::plugin::features::*;
use crossbeam::channel::{Receiver, Sender};
use rtrb::Consumer;
use std::sync::Arc;

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
            cc_consumer: std::sync::Mutex::new(Some(cc_consumer)),
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
    pub cc_consumer: std::sync::Mutex<Option<Consumer<CcMessage>>>,
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
        crate::logger::log_main_thread_tick();

        // Process IPC messages from the GUI
        let mut message_count = 0;
        while let Ok(message) = self.shared.ipc_receiver.try_recv() {
            message_count += 1;
            crate::logger::log_ipc_message_processing(message_count, &message);

            // Handle GUI messages
            if let Some(msg_type) = message.get("type").and_then(|v| v.as_str()) {
                match msg_type {
                    "get_activity" => {
                        let activity = CcBridge::recent_activity();
                        let response = serde_json::json!({
                            "type": "activity",
                            "data": activity.iter().map(|e| {
                                serde_json::json!({
                                    "timestamp": e.timestamp_ms,
                                    "instance": e.instance,
                                    "channel": e.channel + 1,
                                    "cc": e.cc,
                                    "value": e.value
                                })
                            }).collect::<Vec<_>>()
                        });
                        if let Err(e) = self.gui.send_json(response) {
                            crate::logger::log_error(&format!("Failed to send activity: {}", e));
                        }
                    }
                    "get_slots" => {
                        let slots = self.shared.params.get_all_slots();
                        let response = serde_json::json!({
                            "type": "slots",
                            "data": slots
                        });
                        if let Err(e) = self.gui.send_json(response) {
                            crate::logger::log_error(&format!("Failed to send slots: {}", e));
                        }
                    }
                    _ => {}
                }
            }
        }

        crate::logger::log_ipc_messages_processed(message_count);
    }
}

clack_export_entry!(SinglePluginEntry<DropletPlugin>);

// VST3 wrapper export
clap_wrapper::export_vst3!();
