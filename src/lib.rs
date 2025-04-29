use nih_plug::prelude::*;
use std::sync::Arc;

// We're keeping this import for future implementation of grain parameters
#[allow(unused_imports)]
use atomic_float::AtomicF32;

// Struct to hold our plugin's parameters
#[derive(Params)]
struct GranularParams {
    // Parameters for the granular synthesis
    #[id = "grain_size"]
    pub grain_size: FloatParam,

    #[id = "grain_density"]
    pub grain_density: FloatParam,

    #[id = "grain_position"]
    pub grain_position: FloatParam,

    #[id = "grain_spread"]
    pub grain_spread: FloatParam,

    #[id = "dry_wet"]
    pub dry_wet: FloatParam,
}

impl Default for GranularParams {
    fn default() -> Self {
        Self {
            grain_size: FloatParam::new(
                "Grain Size",
                100.0,  // Default value in milliseconds
                FloatRange::Skewed {
                    min: 10.0,
                    max: 500.0,
                    factor: FloatRange::skew_factor(-1.0),
                },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(2)),

            grain_density: FloatParam::new(
                "Density",
                50.0, // Default percentage
                FloatRange::Linear {
                    min: 1.0,
                    max: 100.0,
                },
            )
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            grain_position: FloatParam::new(
                "Position",
                0.5, // Default is center of buffer (0.0-1.0)
                FloatRange::Linear {
                    min: 0.0,
                    max: 1.0,
                },
            )
            .with_value_to_string(formatters::v2s_f32_percentage(2)),

            grain_spread: FloatParam::new(
                "Spread",
                0.2, // Default spread
                FloatRange::Linear {
                    min: 0.0,
                    max: 1.0,
                },
            )
            .with_value_to_string(formatters::v2s_f32_percentage(2)),

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

// The main plugin struct - make it public to expose it for standalone builds
pub struct GranularPlugin {
    params: Arc<GranularParams>,
    
    // We'll keep a buffer here for storing audio for granular processing
    // Buffer size will be determined at initialization
    sample_rate: f32,
    
    // For now, this is a placeholder - we'll develop the actual grain buffer later
    buffer_left: Vec<f32>,
    buffer_right: Vec<f32>,
}

impl Default for GranularPlugin {
    fn default() -> Self {
        Self {
            params: Arc::new(GranularParams::default()),
            sample_rate: 44100.0,
            buffer_left: Vec::new(),
            buffer_right: Vec::new(),
        }
    }
}

impl Plugin for GranularPlugin {
    const NAME: &'static str = "Granular";
    const VENDOR: &'static str = "Simply Chris";
    const URL: &'static str = "https://your-website-here.com";
    const EMAIL: &'static str = "your.email@example.com";

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
        
        // Initialize our audio buffer - for now we'll use a fixed 2 second buffer
        // We'll improve this later
        let buffer_size = (self.sample_rate * 2.0) as usize;
        self.buffer_left = vec![0.0; buffer_size];
        self.buffer_right = vec![0.0; buffer_size];

        true
    }

    fn process(
        &mut self,
        _buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // For now, we'll just do a simple pass-through.
        // We'll implement actual granular processing later.
        ProcessStatus::Normal
    }

    // For now, let's return None for the editor. We'll implement the GUI once
    // we have a basic working plugin and figure out the correct nih_plug_iced APIs.
    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        None
    }
}

impl ClapPlugin for GranularPlugin {
    const CLAP_ID: &'static str = "com.simply-chris.granular";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("A granular synthesis audio plugin");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Utility,
    ];
}

impl Vst3Plugin for GranularPlugin {
    const VST3_CLASS_ID: [u8; 16] = *b"GranularSCPlugin";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[
        Vst3SubCategory::Fx,
        Vst3SubCategory::Tools,
    ];
}

// Export the VST3 plugin
nih_export_vst3!(GranularPlugin);

// Export the CLAP plugin
nih_export_clap!(GranularPlugin);

// Note: Standalone export implementation will be added in a future update
