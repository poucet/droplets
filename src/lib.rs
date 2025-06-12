use nih_plug::prelude::*;
use std::sync::Arc;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};

mod droplet;
mod editor;
mod logger;

use droplet::{Droplet, RainCatcher, WarpCurve};
use logger::{init_logger, log_info};

// Dummy GuiContext for the webview editor
struct DummyGuiContext;

impl GuiContext for DummyGuiContext {
    fn request_resize(&self) -> bool {
        false
    }

    fn plugin_api(&self) -> PluginApi {
        PluginApi::Standalone
    }

    unsafe fn raw_begin_set_parameter(&self, _param: ParamPtr) {}

    unsafe fn raw_set_parameter_normalized(&self, _param: ParamPtr, _normalized: f32) {}

    unsafe fn raw_end_set_parameter(&self, _param: ParamPtr) {}

    fn get_state(&self) -> PluginState {
        PluginState {
            version: env!("CARGO_PKG_VERSION").to_string(),
            params: std::collections::BTreeMap::new(),
            fields: std::collections::BTreeMap::new(),
        }
    }

    fn set_state(&self, _state: PluginState) {}
}

#[derive(Params)]
pub struct DropletParams {
    #[id = "gain"]
    pub gain: FloatParam,
    
    #[id = "grain_size"]
    pub grain_size: IntParam,
    
    #[id = "density"]
    pub density: FloatParam,
    
    #[id = "time_warp"]
    pub time_warp: FloatParam,
    
    #[id = "spatial_spread"]
    pub spatial_spread: FloatParam,
    
    #[id = "dry_wet"]
    pub dry_wet: FloatParam,
    
    // Parameter change tracking
    pub gain_value_changed: Arc<AtomicBool>,
    pub dry_wet_value_changed: Arc<AtomicBool>,
}

impl Default for DropletParams {
    fn default() -> Self {
        let gain_value_changed = Arc::new(AtomicBool::new(false));
        let dry_wet_value_changed = Arc::new(AtomicBool::new(false));
        
        let gain_callback = {
            let v = gain_value_changed.clone();
            Arc::new(move |_: f32| {
                v.store(true, Ordering::Relaxed);
            })
        };
        
        let dry_wet_callback = {
            let v = dry_wet_value_changed.clone();
            Arc::new(move |_: f32| {
                v.store(true, Ordering::Relaxed);
            })
        };
        
        Self {
            gain: FloatParam::new(
                "Gain",
                util::db_to_gain(0.0),
                FloatRange::Skewed {
                    min: util::db_to_gain(-30.0),
                    max: util::db_to_gain(30.0),
                    factor: FloatRange::gain_skew_factor(-30.0, 30.0),
                },
            )
            .with_smoother(SmoothingStyle::Logarithmic(50.0))
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(2))
            .with_string_to_value(formatters::s2v_f32_gain_to_db())
            .with_callback(gain_callback),
            
            grain_size: IntParam::new(
                "Grain Size",
                1024,
                IntRange::Linear {
                    min: 64,
                    max: 8192,
                },
            ),
            
            density: FloatParam::new(
                "Density",
                10.0,
                FloatRange::Linear {
                    min: 0.1,
                    max: 100.0,
                },
            )
            .with_unit(" /s"),
            
            time_warp: FloatParam::new(
                "Time Warp",
                1.0,
                FloatRange::Linear {
                    min: 0.1,
                    max: 4.0,
                },
            ),
            
            spatial_spread: FloatParam::new(
                "Spatial Spread",
                1.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 1.0,
                },
            )
            .with_value_to_string(formatters::v2s_f32_percentage(2)),
            
            dry_wet: FloatParam::new(
                "Dry/Wet",
                1.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 1.0,
                },
            )
            .with_value_to_string(formatters::v2s_f32_percentage(2))
            .with_callback(dry_wet_callback),
            
            gain_value_changed,
            dry_wet_value_changed,
        }
    }
}

// The main plugin struct
pub struct DropletPlugin {
    params: Arc<DropletParams>,
    sample_rate: f32,
    
    // Droplet processing components
    rain_catcher_left: RainCatcher,
    rain_catcher_right: RainCatcher,
    active_droplets: Vec<Droplet>,
    
    // Input delay buffer for dry signal
    dry_buffer_left: VecDeque<f32>,
    dry_buffer_right: VecDeque<f32>,
    dry_delay_samples: usize,
}

impl Default for DropletPlugin {
    fn default() -> Self {
        init_logger();
        log_info("DropletPlugin::default() called");
        
        let grain_size = 1024;
        Self {
            params: Arc::new(DropletParams::default()),
            sample_rate: 44100.0,
            rain_catcher_left: RainCatcher::new(grain_size),
            rain_catcher_right: RainCatcher::new(grain_size),
            active_droplets: Vec::new(),
            dry_buffer_left: VecDeque::new(),
            dry_buffer_right: VecDeque::new(),
            dry_delay_samples: grain_size / 2,
        }
    }
}

impl Plugin for DropletPlugin {
    const NAME: &'static str = "Simply Droplets";
    const VENDOR: &'static str = "Simply Chris";
    const URL: &'static str = "https://simply-music.ai";
    const EMAIL: &'static str = "chris@simply-music.ai";

    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
    ];

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.sample_rate = buffer_config.sample_rate;
        
        let grain_size = self.params.grain_size.value() as usize;
        self.rain_catcher_left = RainCatcher::new(grain_size);
        self.rain_catcher_right = RainCatcher::new(grain_size);
        self.dry_delay_samples = grain_size / 2;
        
        self.active_droplets.clear();
        self.dry_buffer_left.clear();
        self.dry_buffer_right.clear();
        
        true
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let gain = self.params.gain.smoothed.next();
        let density = self.params.density.value();
        let time_warp = self.params.time_warp.value();
        let spatial_spread = self.params.spatial_spread.value();
        let dry_wet = self.params.dry_wet.value();
        
        self.rain_catcher_left.density = density;
        self.rain_catcher_right.density = density;

        for mut channel_samples in buffer.iter_samples() {
            let input_left = *channel_samples.get_mut(0).unwrap_or(&mut 0.0);
            let input_right = if channel_samples.len() > 1 {
                *channel_samples.get_mut(1).unwrap_or(&mut 0.0)
            } else {
                input_left
            };
            
            // Store dry samples with delay compensation
            self.dry_buffer_left.push_back(input_left);
            self.dry_buffer_right.push_back(input_right);
            
            while self.dry_buffer_left.len() > self.dry_delay_samples + 1 {
                self.dry_buffer_left.pop_front();
                self.dry_buffer_right.pop_front();
            }
            
            let dry_left = self.dry_buffer_left.front().copied().unwrap_or(0.0);
            let dry_right = self.dry_buffer_right.front().copied().unwrap_or(0.0);
            
            // Create new droplets
            if let Some(mut droplet) = self.rain_catcher_left.process_input(input_left, self.sample_rate) {
                droplet.radius *= spatial_spread;
                droplet.warp_curve = WarpCurve::Exponential(time_warp);
                self.active_droplets.push(droplet);
            }
            
            if let Some(mut droplet) = self.rain_catcher_right.process_input(input_right, self.sample_rate) {
                droplet.radius *= spatial_spread;
                droplet.warp_curve = WarpCurve::Exponential(time_warp);
                droplet.azimuth += std::f32::consts::PI / 4.0;
                self.active_droplets.push(droplet);
            }
            
            // Process active droplets
            let mut wet_left = 0.0;
            let mut wet_right = 0.0;
            
            self.active_droplets.retain_mut(|droplet| {
                if droplet.is_active {
                    let (left, right) = droplet.process_sample();
                    wet_left += left;
                    wet_right += right;
                    droplet.is_active
                } else {
                    false
                }
            });
            
            // Mix dry and wet signals
            let mixed_left = dry_left * (1.0 - dry_wet) + wet_left * dry_wet;
            let mixed_right = dry_right * (1.0 - dry_wet) + wet_right * dry_wet;
            
            // Apply gain
            let output_left = mixed_left * gain;
            let output_right = mixed_right * gain;
            
            *channel_samples.get_mut(0).unwrap() = output_left;
            if channel_samples.len() > 1 {
                *channel_samples.get_mut(1).unwrap() = output_right;
            }
        }
        
        ProcessStatus::Normal
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        log_info("Creating editor instance");
        Some(editor::create_droplet_editor(
            self.params.clone()
        ))
    }
}

impl ClapPlugin for DropletPlugin {
    const CLAP_ID: &'static str = "com.simply-chris.simply-droplets";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("3D droplet-based granular synthesis");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for DropletPlugin {
    const VST3_CLASS_ID: [u8; 16] = *b"SimplyDroplets01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[
        Vst3SubCategory::Fx,
        Vst3SubCategory::Modulation,
    ];
}

nih_export_vst3!(DropletPlugin);
nih_export_clap!(DropletPlugin);