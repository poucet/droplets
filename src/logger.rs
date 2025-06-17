use log::{debug, info, warn, error, LevelFilter};
use simplelog::*;
use std::fs::File;

pub fn init_logger() {
    let home_dir = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let log_path = format!("{}/droplets_plugin.log", home_dir);
    
    let _ = WriteLogger::init(
        LevelFilter::Debug,
        Config::default(),
        File::create(&log_path).unwrap_or_else(|_| File::create("/tmp/droplets_plugin.log").unwrap()),
    );
    
    info!("Droplets plugin initialized, logging to: {}", log_path);
    info!("=== Plugin session started ===");
}

pub fn log_info(message: &str) {
    info!("{}", message);
}

pub fn log_warn(message: &str) {
    warn!("{}", message);
}

pub fn log_error(message: &str) {
    error!("{}", message);
}

pub fn log_debug(message: &str) {
    debug!("{}", message);
}

pub fn log_plugin_initialization(plugin_name: &str, step: &str) {
    info!("{} plugin: {}", plugin_name, step);
}

pub fn log_main_thread_tick() {
    debug!("Main thread tick - processing IPC messages");
}

pub fn log_ipc_message_received(message: &str) {
    debug!("Received IPC message from web view: {}", message);
}

pub fn log_ipc_message_parsed(message: &serde_json::Value) {
    debug!("Parsed IPC message, sending to channel: {:?}", message);
}

pub fn log_ipc_send_error(error: &str) {
    warn!("Failed to send IPC message to channel: {}", error);
}

pub fn log_ipc_parse_error(message: &str) {
    warn!("Failed to parse IPC message as JSON: {}", message);
}

pub fn log_ipc_messages_processed(count: usize) {
    if count > 0 {
        info!("Processed {} IPC messages", count);
    }
}

pub fn log_ipc_message_processing(count: usize, message: &serde_json::Value) {
    debug!("Processing IPC message #{}: {}", count, message);
}

pub fn log_ipc_channel_created() {
    info!("Created IPC channel for GUI communication");
}

pub fn log_parameter_change(param_name: &str, value: f64) {
    info!("Parameter changed: {} = {}", param_name, value);
}

pub fn log_audio_processor_activation(sample_rate: f32) {
    info!("Audio processor activated - sample_rate: {}", sample_rate);
}

pub fn log_droplet_creation(count: usize, radius: f32, azimuth: f32, elevation: f32) {
    debug!("Created droplet #{} - radius: {:.3}, azimuth: {:.3}, elevation: {:.3}", count, radius, azimuth, elevation);
}

pub fn log_active_droplets_count(count: usize) {
    if count > 0 {
        debug!("Active droplets: {}", count);
    }
}

pub fn log_gui_event(event: &str, details: &str) {
    debug!("GUI {}: {}", event, details);
}