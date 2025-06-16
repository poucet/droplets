use clack_extensions::{audio_ports::*, gui::*};
use clack_plugin::prelude::*;
use clack_plugin::plugin::features::*;
use crossbeam::channel::{Receiver, Sender};

mod atomic;
mod audio;
mod droplet;
mod gui;
pub mod logger;
mod params;

use audio::DropletAudioProcessor;
use gui::DropletGui;
use params::DropletParams;

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
            .with_description("3D droplet-based granular synthesis")
            .with_version(env!("CARGO_PKG_VERSION"))
            .with_features([AUDIO_EFFECT, STEREO])
    }

    fn new_shared(host: HostSharedHandle) -> Result<Self::Shared<'_>, PluginError> {
        logger::init_logger();
        logger::log_info("DropletPlugin shared instance created");
        
        let (sender, receiver) = crossbeam::channel::unbounded();
        
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
        // Process IPC messages from the GUI
        let mut message_count = 0;
        while let Ok(message) = self.shared.ipc_receiver.try_recv() {
            message_count += 1;
            logger::log_debug(&format!("Processing IPC message #{}", message_count));
            self.shared.params.handle_ipc_message(&message);
        }
        
        if message_count > 0 {
            logger::log_debug(&format!("Processed {} IPC messages", message_count));
        }
    }
}

clack_export_entry!(SinglePluginEntry<DropletPlugin>);

// VST3 wrapper export
clap_wrapper::export_vst3!();