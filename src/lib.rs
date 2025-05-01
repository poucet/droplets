use nih_plug::prelude::*;
use std::sync::Arc;

// Simple parameter struct for our gain plugin
#[derive(Params)]
struct GainParams {
    #[id = "gain"]
    pub gain: FloatParam,
    
    #[id = "dry_wet"]
    pub dry_wet: FloatParam,
}

impl Default for GainParams {
    fn default() -> Self {
        Self {
            gain: FloatParam::new(
                "Gain",
                0.0, // 0 dB (unity gain)
                FloatRange::Linear {
                    min: -30.0, // -30 dB
                    max: 30.0,  // +30 dB
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
            
            dry_wet: FloatParam::new(
                "Dry/Wet",
                1.0, // Default is 100% wet
                FloatRange::Linear {
                    min: 0.0,
                    max: 1.0,
                },
            )
            .with_value_to_string(formatters::v2s_f32_percentage(2)),
        }
    }
}

// The main plugin struct
pub struct DropletPlugin {
    params: Arc<GainParams>,
    sample_rate: f32,
}

impl Default for DropletPlugin {
    fn default() -> Self {
        Self {
            params: Arc::new(GainParams::default()),
            sample_rate: 44100.0, // Default sample rate
        }
    }
}

impl Plugin for DropletPlugin {
    const NAME: &'static str = "Simply Droplets";
    const VENDOR: &'static str = "Simply Chris";
    const URL: &'static str = "https://simply-music.ai";
    const EMAIL: &'static str = "chris@simply-music.ai";

    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    // Define the supported audio I/O layouts
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
        // Store the sample rate for time calculations
        self.sample_rate = buffer_config.sample_rate;
        true
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Get the parameters
        let gain_db = self.params.gain.value();
        let gain_factor = 10.0_f32.powf(gain_db / 20.0); // Convert dB to amplitude multiplier
        let dry_wet = self.params.dry_wet.value();

        // Process all channels
        for channel_samples in buffer.iter_samples() {
            for sample in channel_samples {
                // Store the original (dry) sample
                let dry_sample = *sample;
                
                // Create the processed (wet) sample with gain applied
                let wet_sample = dry_sample * gain_factor;
                
                // Mix dry and wet signals according to the dry/wet parameter
                *sample = dry_sample * (1.0 - dry_wet) + wet_sample * dry_wet;
            }
        }
        
        ProcessStatus::Normal
    }

    // No GUI
    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        None
    }
}

impl ClapPlugin for DropletPlugin {
    const CLAP_ID: &'static str = "com.simply-chris.simply-droplets";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("A simple gain plugin");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Utility,
    ];
}

impl Vst3Plugin for DropletPlugin {
    const VST3_CLASS_ID: [u8; 16] = *b"SimplyDroplets01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[
        Vst3SubCategory::Fx,
        Vst3SubCategory::Tools,
    ];
}

// Export the VST3 plugin
nih_export_vst3!(DropletPlugin);

// Export the CLAP plugin
nih_export_clap!(DropletPlugin);
