use clack_extensions::{audio_ports::*, gui::*};
use clack_plugin::prelude::*;
use clack_plugin::plugin::features::*;
use crossbeam::channel::{Receiver, Sender};

use audio::DropletAudioProcessor;
use gui::DropletGui;
use params::DropletParams;

mod atomic;
mod audio;
mod droplet;
mod gui;
pub mod logger;
mod params;


pub struct DropletPlugin;


impl Plugin for DropletPlugin {
    type AudioProcessor<'a> = DropletAudioProcessor<'a>;
    type Shared<'a> = DropletShared<'a>;
    type MainThread<'a> = DropletMainThread<'a>;

    fn declare_extensions(builder: &mut PluginExtensions<Self>, _shared: Option<&Self::Shared<'_>>) {
        builder
            .register::<PluginAudioPorts>()
            .register::<PluginGui>();
    }
}

impl DefaultPluginFactory for DropletPlugin {
    fn get_descriptor() -> PluginDescriptor {
        PluginDescriptor::new("com.simply-chris.simply-droplets", "Simply Droplets")
            .with_vendor("Simply Chris")
            .with_features([AUDIO_EFFECT, STEREO])
    }

    fn new_shared(host: HostSharedHandle) -> Result<Self::Shared<'_>, PluginError> {
        logger::init_logger();
        logger::log_plugin_initialization("Droplets", "Creating shared instance");
        
        let (sender, receiver) = crossbeam::channel::unbounded();
        logger::log_ipc_channel_created();
        
        Ok(DropletShared {
            params: DropletParams::new(),
            host,
            ipc_sender: sender,
            ipc_receiver: receiver,
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
    pub params: DropletParams,
    pub host: HostSharedHandle<'a>,
    pub ipc_sender: Sender<serde_json::Value>,
    pub ipc_receiver: Receiver<serde_json::Value>,
}

impl<'a> PluginShared<'a> for DropletShared<'a> {}

pub struct DropletMainThread<'a> {
    shared: &'a DropletShared<'a>,
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
            
            if let Some(response) = self.shared.params.handle_ipc_message(&message) {
                if let Err(e) = self.gui.send_json(response) {
                    crate::logger::log_error(&format!("Failed to send GUI response: {}", e));
                }
            }
        }
        
        crate::logger::log_ipc_messages_processed(message_count);
    }
}

clack_export_entry!(SinglePluginEntry<DropletPlugin>);

// VST3 wrapper export
clap_wrapper::export_vst3!();
