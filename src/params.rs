use clack_plugin::events::{UnknownEvent};
use clack_plugin::events::spaces::CoreEventSpace;
use clack_plugin::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::atomic::Ordering;

use crate::atomic::AtomicF32;

// Parameter IDs
pub const PARAM_GAIN_ID: ClapId = ClapId::new(1);
pub const PARAM_GRAIN_SIZE_ID: ClapId = ClapId::new(2);
pub const PARAM_DENSITY_ID: ClapId = ClapId::new(3);
pub const PARAM_TIME_WARP_ID: ClapId = ClapId::new(4);
pub const PARAM_SPATIAL_SPREAD_ID: ClapId = ClapId::new(5);
pub const PARAM_DRY_WET_ID: ClapId = ClapId::new(6);

// Default values
const DEFAULT_GAIN: f32 = 1.0; // 0 dB
const DEFAULT_GRAIN_SIZE: i32 = 1024;
const DEFAULT_DENSITY: f32 = 10.0;
const DEFAULT_TIME_WARP: f32 = 1.0;
const DEFAULT_SPATIAL_SPREAD: f32 = 1.0;
const DEFAULT_DRY_WET: f32 = 1.0;

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum IpcMessage {
    #[serde(rename = "parameter_change")]
    ParameterChange {
        parameter_id: u64,
        value: f64,
    },
    #[serde(rename = "get_parameter")]
    GetParameter {
        parameter_id: u64,
    },
}

pub struct DropletParams {
    gain: AtomicF32,
    grain_size: std::sync::atomic::AtomicI32,
    density: AtomicF32,
    time_warp: AtomicF32,
    spatial_spread: AtomicF32,
    dry_wet: AtomicF32,
}

impl DropletParams {
    pub fn new() -> Self {  
        Self { 
            gain: AtomicF32::new(DEFAULT_GAIN),
            grain_size: std::sync::atomic::AtomicI32::new(DEFAULT_GRAIN_SIZE),
            density: AtomicF32::new(DEFAULT_DENSITY),
            time_warp: AtomicF32::new(DEFAULT_TIME_WARP),
            spatial_spread: AtomicF32::new(DEFAULT_SPATIAL_SPREAD),
            dry_wet: AtomicF32::new(DEFAULT_DRY_WET),
        }
    }

    pub fn get_gain(&self) -> f32 {
        self.gain.load(Ordering::SeqCst)
    }

    pub fn set_gain(&self, gain: f32) {
        let gain = gain.clamp(0.0, 4.0); // 0 to +12dB range
        crate::logger::log_debug(&format!("Setting gain parameter: {}", gain));
        self.gain.store(gain, Ordering::SeqCst);
    }
    
    pub fn get_grain_size(&self) -> i32 {
        self.grain_size.load(Ordering::SeqCst)
    }

    pub fn set_grain_size(&self, size: i32) {
        let size = size.clamp(64, 8192);
        crate::logger::log_debug(&format!("Setting grain_size parameter: {}", size));
        self.grain_size.store(size, Ordering::SeqCst);
    }
    
    pub fn get_density(&self) -> f32 {
        self.density.load(Ordering::SeqCst)
    }

    pub fn set_density(&self, density: f32) {
        let density = density.clamp(0.1, 100.0);
        crate::logger::log_debug(&format!("Setting density parameter: {}", density));
        self.density.store(density, Ordering::SeqCst);
    }
    
    pub fn get_time_warp(&self) -> f32 {
        self.time_warp.load(Ordering::SeqCst)
    }

    pub fn set_time_warp(&self, warp: f32) {
        let warp = warp.clamp(0.1, 4.0);
        crate::logger::log_debug(&format!("Setting time_warp parameter: {}", warp));
        self.time_warp.store(warp, Ordering::SeqCst);
    }
    
    pub fn get_spatial_spread(&self) -> f32 {
        self.spatial_spread.load(Ordering::SeqCst)
    }

    pub fn set_spatial_spread(&self, spread: f32) {
        let spread = spread.clamp(0.0, 1.0);
        crate::logger::log_debug(&format!("Setting spatial_spread parameter: {}", spread));
        self.spatial_spread.store(spread, Ordering::SeqCst);
    }
    
    pub fn get_dry_wet(&self) -> f32 {
        self.dry_wet.load(Ordering::SeqCst)
    }

    pub fn set_dry_wet(&self, dry_wet: f32) {
        let dry_wet = dry_wet.clamp(0.0, 1.0);
        crate::logger::log_debug(&format!("Setting dry_wet parameter: {}", dry_wet));
        self.dry_wet.store(dry_wet, Ordering::SeqCst);
    }
    
    /// Handles incoming parameter events from the host
    pub fn handle_event(&self, event: &UnknownEvent) {
        if let Some(CoreEventSpace::ParamValue(event)) = event.as_core_event() {
            let param_id = event.param_id();
            match param_id {
                Some(PARAM_GAIN_ID) => self.set_gain(event.value() as f32),
                Some(PARAM_GRAIN_SIZE_ID) => self.set_grain_size(event.value() as i32),
                Some(PARAM_DENSITY_ID) => self.set_density(event.value() as f32),
                Some(PARAM_TIME_WARP_ID) => self.set_time_warp(event.value() as f32),
                Some(PARAM_SPATIAL_SPREAD_ID) => self.set_spatial_spread(event.value() as f32),
                Some(PARAM_DRY_WET_ID) => self.set_dry_wet(event.value() as f32),
                _ => {}
            }
        }
    }

    pub fn handle_ipc_message(&self, message: &serde_json::Value) {
        match serde_json::from_value::<IpcMessage>(message.clone()) {
            Ok(IpcMessage::ParameterChange { parameter_id, value }) => {
                let param_id = ClapId::new(parameter_id as u32);
                match param_id {
                    PARAM_GAIN_ID => {
                        crate::logger::log_debug(&format!("IPC gain change: {}", value));
                        self.set_gain(value as f32);
                    }
                    PARAM_GRAIN_SIZE_ID => {
                        crate::logger::log_debug(&format!("IPC grain_size change: {}", value));
                        self.set_grain_size(value as i32);
                    }
                    PARAM_DENSITY_ID => {
                        crate::logger::log_debug(&format!("IPC density change: {}", value));
                        self.set_density(value as f32);
                    }
                    PARAM_TIME_WARP_ID => {
                        crate::logger::log_debug(&format!("IPC time_warp change: {}", value));
                        self.set_time_warp(value as f32);
                    }
                    PARAM_SPATIAL_SPREAD_ID => {
                        crate::logger::log_debug(&format!("IPC spatial_spread change: {}", value));
                        self.set_spatial_spread(value as f32);
                    }
                    PARAM_DRY_WET_ID => {
                        crate::logger::log_debug(&format!("IPC dry_wet change: {}", value));
                        self.set_dry_wet(value as f32);
                    }
                    _ => {
                        crate::logger::log_warn(&format!("Unknown parameter ID from IPC: {}", parameter_id));
                    }
                }
            }
            Ok(IpcMessage::GetParameter { parameter_id }) => {
                let param_id = ClapId::new(parameter_id as u32);
                match param_id {
                    PARAM_GAIN_ID => {
                        crate::logger::log_debug(&format!("IPC get gain: {}", self.get_gain()));
                    }
                    PARAM_GRAIN_SIZE_ID => {
                        crate::logger::log_debug(&format!("IPC get grain_size: {}", self.get_grain_size()));
                    }
                    PARAM_DENSITY_ID => {
                        crate::logger::log_debug(&format!("IPC get density: {}", self.get_density()));
                    }
                    PARAM_TIME_WARP_ID => {
                        crate::logger::log_debug(&format!("IPC get time_warp: {}", self.get_time_warp()));
                    }
                    PARAM_SPATIAL_SPREAD_ID => {
                        crate::logger::log_debug(&format!("IPC get spatial_spread: {}", self.get_spatial_spread()));
                    }
                    PARAM_DRY_WET_ID => {
                        crate::logger::log_debug(&format!("IPC get dry_wet: {}", self.get_dry_wet()));
                    }
                    _ => {
                        crate::logger::log_warn(&format!("Unknown parameter ID from IPC get: {}", parameter_id));
                    }
                }
            }
            Err(e) => {
                crate::logger::log_error(&format!("Failed to parse IPC message: {}", e));
            }
        }
    }
}