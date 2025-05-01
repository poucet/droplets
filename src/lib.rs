use nih_plug::prelude::*;
use rand::{rngs::SmallRng, Rng, SeedableRng};
use std::sync::Arc;

// A simple circular buffer for storing audio samples
struct DropletBuffer {
    buffer: Vec<f32>,
    write_pos: usize,
    size: usize,
}

impl DropletBuffer {
    fn new(size: usize) -> Self {
        Self {
            buffer: vec![0.0; size],
            write_pos: 0,
            size,
        }
    }

    fn write(&mut self, sample: f32) {
        self.buffer[self.write_pos] = sample;
        self.write_pos = (self.write_pos + 1) % self.size;
    }

    fn read(&self, position: f32) -> f32 {
        // position is normalized 0.0 to 1.0
        let pos = (self.write_pos as f32 - (position * self.size as f32)).rem_euclid(self.size as f32) as usize;
        self.buffer[pos]
    }
    
    // Read with time warping using a curve function
    fn read_warped(&self, position: f32, time_warp: f32, warp_curve: f32) -> f32 {
        // Apply warp curve to the position
        // When warp_curve = 0.0, no warping
        // When warp_curve > 0.0, exponential warping
        // When warp_curve < 0.0, logarithmic warping
        let warped_position = if warp_curve.abs() < 0.01 {
            position // Linear (no warping)
        } else if warp_curve > 0.0 {
            // Exponential curve
            (position.powf(1.0 + warp_curve * 3.0) * time_warp).min(1.0)
        } else {
            // Logarithmic curve
            (position.powf(1.0 / (1.0 + warp_curve.abs() * 3.0)) * time_warp).min(1.0)
        };
        
        self.read(warped_position)
    }
}

// A struct to represent a single droplet
struct Droplet {
    position: f32,       // Normalized position in the buffer (0.0 to 1.0)
    age: usize,          // Current age in samples
    duration: usize,     // Total duration in samples
    time_warp: f32,      // Time warp factor (1.0 = normal speed)
    active: bool,
    // 3D audio positioning
    pan: f32,            // -1.0 (left) to 1.0 (right)
    depth: f32,          // 0.0 (close) to 1.0 (far)
    height: f32,         // -1.0 (below) to 1.0 (above)
}

impl Droplet {
    fn new(position: f32, duration: usize, time_warp: f32, pan: f32, depth: f32, height: f32) -> Self {
        Self {
            position,
            age: 0,
            duration,
            time_warp,
            active: true,
            pan,
            depth,
            height,
        }
    }

    fn is_active(&self) -> bool {
        self.active && self.age < self.duration
    }

    fn process(
        &mut self, 
        buffer: &DropletBuffer, 
        position_offset: f32,
        spread: f32,
        warp_curve: f32
    ) -> (f32, f32, f32) {  // Returns (left, right, height) channel samples
        if !self.is_active() {
            return (0.0, 0.0, 0.0);
        }

        // Envelope (ADSR)
        let envelope = if self.age < self.duration / 4 {
            // Attack (25% of duration)
            self.age as f32 / (self.duration as f32 / 4.0)
        } else if self.age > (self.duration * 3) / 4 {
            // Release (25% of duration)
            1.0 - ((self.age - (self.duration * 3) / 4) as f32 / (self.duration as f32 / 4.0))
        } else {
            // Sustain (50% of duration)
            1.0
        };

        // Calculate droplet position with offset and randomized spread
        let mut final_position = self.position + position_offset;
        
        // Add a pseudo-random variation based on the droplet's properties
        let variation = ((self.age as f32 * 0.1).sin() * spread).clamp(-0.1, 0.1);
        final_position += variation;
        
        // Keep position in 0..1 range
        final_position = final_position.clamp(0.0, 0.99);

        // Read and apply envelope, with time warping
        let sample = buffer.read_warped(final_position, self.time_warp, warp_curve) * envelope;
        
        // Apply 3D positioning
        
        // Pan (left-right): -1.0 (full left) to 1.0 (full right)
        let left_gain = if self.pan <= 0.0 { 1.0 } else { 1.0 - self.pan };
        let right_gain = if self.pan >= 0.0 { 1.0 } else { 1.0 + self.pan };
        
        // Depth (front-back): 0.0 (close) to 1.0 (far)
        // Simulate distance with volume reduction and some filtering (simplified here)
        let depth_gain = 1.0 - (self.depth * 0.7); // Maximum reduction of 70% at full depth
        
        // Apply gains to the sample
        let left_sample = sample * left_gain * depth_gain;
        let right_sample = sample * right_gain * depth_gain;
        
        // Height (above-below): -1.0 (below) to 1.0 (above)
        // In a real implementation, this might drive vertical channel outputs
        // For now, we'll just return it as a separate value that could be used
        // for creative effects or real 3D audio if available
        let height_sample = sample * self.height;
        
        self.age += 1;
        if self.age >= self.duration {
            self.active = false;
        }
        
        (left_sample, right_sample, height_sample)
    }
}

// Struct to hold our plugin's parameters
#[derive(Params)]
struct DropletParams {
    // Parameters for the droplet synthesis
    #[id = "droplet_min_size"]
    pub droplet_min_size: FloatParam,
    
    #[id = "droplet_max_size"]
    pub droplet_max_size: FloatParam,

    #[id = "droplet_density"]
    pub droplet_density: FloatParam,

    #[id = "droplet_position"]
    pub droplet_position: FloatParam,

    #[id = "droplet_spread"]
    pub droplet_spread: FloatParam,
    
    #[id = "time_warp"]
    pub time_warp: FloatParam,
    
    #[id = "warp_curve"]
    pub warp_curve: FloatParam,
    
    // 3D Audio parameters
    #[id = "pan_spread"]
    pub pan_spread: FloatParam,
    
    #[id = "depth_spread"]
    pub depth_spread: FloatParam,
    
    #[id = "height_spread"]
    pub height_spread: FloatParam,
    
    #[id = "height_mix"]
    pub height_mix: FloatParam,

    #[id = "dry_wet"]
    pub dry_wet: FloatParam,
}

impl Default for DropletParams {
    fn default() -> Self {
        Self {
            droplet_min_size: FloatParam::new(
                "Min Size",
                20.0,  // Default value in milliseconds
                FloatRange::Skewed {
                    min: 5.0,
                    max: 100.0,
                    factor: FloatRange::skew_factor(-1.0),
                },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(2)),

            droplet_max_size: FloatParam::new(
                "Max Size",
                150.0,  // Default value in milliseconds
                FloatRange::Skewed {
                    min: 20.0,
                    max: 500.0,
                    factor: FloatRange::skew_factor(-1.0),
                },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(2)),

            droplet_density: FloatParam::new(
                "Density",
                50.0, // Default percentage
                FloatRange::Linear {
                    min: 1.0,
                    max: 100.0,
                },
            )
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            droplet_position: FloatParam::new(
                "Position",
                0.5, // Default is center of buffer (0.0-1.0)
                FloatRange::Linear {
                    min: 0.0,
                    max: 1.0,
                },
            )
            .with_value_to_string(formatters::v2s_f32_percentage(2)),

            droplet_spread: FloatParam::new(
                "Spread",
                0.2, // Default spread
                FloatRange::Linear {
                    min: 0.0,
                    max: 1.0,
                },
            )
            .with_value_to_string(formatters::v2s_f32_percentage(2)),
            
            time_warp: FloatParam::new(
                "Time Warp",
                1.0, // Default is normal speed
                FloatRange::Skewed {
                    min: 0.25,
                    max: 4.0,
                    factor: FloatRange::skew_factor(-1.0),
                },
            )
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
            
            warp_curve: FloatParam::new(
                "Warp Curve",
                0.0, // Default is linear
                FloatRange::Linear {
                    min: -1.0, // Logarithmic
                    max: 1.0,  // Exponential
                },
            )
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
            
            // 3D Audio parameters
            pan_spread: FloatParam::new(
                "Pan Spread",
                0.5, // Default spread
                FloatRange::Linear {
                    min: 0.0,
                    max: 1.0,
                },
            )
            .with_value_to_string(formatters::v2s_f32_percentage(2)),
            
            depth_spread: FloatParam::new(
                "Depth",
                0.3, // Default spread
                FloatRange::Linear {
                    min: 0.0,
                    max: 1.0,
                },
            )
            .with_value_to_string(formatters::v2s_f32_percentage(2)),
            
            height_spread: FloatParam::new(
                "Height Spread",
                0.3, // Default spread
                FloatRange::Linear {
                    min: 0.0,
                    max: 1.0,
                },
            )
            .with_value_to_string(formatters::v2s_f32_percentage(2)),
            
            height_mix: FloatParam::new(
                "Height Mix",
                0.5, // Default mix
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

// The main plugin struct
pub struct DropletPlugin {
    params: Arc<DropletParams>,
    
    // Sample rate for time calculations
    sample_rate: f32,
    
    // Buffer for storing audio for droplets
    buffer_left: DropletBuffer,
    buffer_right: DropletBuffer,
    
    // Droplets (up to 32 active droplets)
    droplets: Vec<Droplet>,
    droplet_counter: usize, // To track when to spawn new droplets
    
    // Current droplet spawn interval in samples
    droplet_spawn_interval: usize,
    
    // Thread-safe random number generator
    rng: SmallRng,
}

impl Default for DropletPlugin {
    fn default() -> Self {
        Self {
            params: Arc::new(DropletParams::default()),
            sample_rate: 44100.0,
            buffer_left: DropletBuffer::new(44100 * 2), // 2 seconds buffer
            buffer_right: DropletBuffer::new(44100 * 2),
            droplets: Vec::with_capacity(32),
            droplet_counter: 0,
            droplet_spawn_interval: 1000, // Will be updated in initialize
            rng: SmallRng::from_entropy(),
        }
    }
}

impl Plugin for DropletPlugin {
    const NAME: &'static str = "Simply Droplets";
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
        
        // Initialize our audio buffer (2 seconds at this sample rate)
        let buffer_size = (self.sample_rate * 2.0) as usize;
        self.buffer_left = DropletBuffer::new(buffer_size);
        self.buffer_right = DropletBuffer::new(buffer_size);
        
        // Reset droplets
        self.droplets.clear();

        true
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Get the parameters
        let droplet_min_size_ms = self.params.droplet_min_size.value();
        let droplet_max_size_ms = self.params.droplet_max_size.value();
        let droplet_density = self.params.droplet_density.value() / 100.0; // 0.0-1.0
        let droplet_position = self.params.droplet_position.value();
        let droplet_spread = self.params.droplet_spread.value();
        let time_warp = self.params.time_warp.value();
        let warp_curve = self.params.warp_curve.value();
        let pan_spread = self.params.pan_spread.value();
        let depth_spread = self.params.depth_spread.value();
        let height_spread = self.params.height_spread.value();
        let height_mix = self.params.height_mix.value();
        let dry_wet = self.params.dry_wet.value();
        
        // Update droplet spawn interval based on density
        // More density = more frequent droplets = smaller interval
        self.droplet_spawn_interval = (self.sample_rate as f32 / (10.0 * droplet_density.max(0.001))) as usize;

        // Skip processing if we don't have 2 channels (stereo)
        if buffer.channels() != 2 {
            return ProcessStatus::Normal;
        }

        let num_samples = buffer.samples();
        
        // Process each channel separately using temporary buffers
        let mut channel_left_in = Vec::with_capacity(num_samples);
        let mut channel_right_in = Vec::with_capacity(num_samples);
        let mut channel_left_out = vec![0.0; num_samples];
        let mut channel_right_out = vec![0.0; num_samples];
        
        // Extract input samples from buffer
        for i in 0..num_samples {
            let frame = buffer.get_frame(i);
            channel_left_in.push(frame[0]);
            channel_right_in.push(frame[1]);
        }
        
        // Process each sample
        for i in 0..num_samples {
            let in_left = channel_left_in[i];
            let in_right = channel_right_in[i];
            
            // Write input to circular buffer for droplets to read from
            self.buffer_left.write(in_left);
            self.buffer_right.write(in_right);
            
            // Check if we need to spawn a new droplet
            self.droplet_counter += 1;
            if self.droplet_counter >= self.droplet_spawn_interval {
                self.droplet_counter = 0;
                
                // Remove inactive droplets
                self.droplets.retain(|droplet| droplet.is_active());
                
                // Check if we have space for a new droplet
                if self.droplets.len() < 32 {
                    // Random duration between min and max
                    let rand_duration_ms = self.rng.gen_range(droplet_min_size_ms..=droplet_max_size_ms);
                    let duration_samples = (rand_duration_ms / 1000.0 * self.sample_rate) as usize;
                    
                    // Random 3D positioning
                    let rand_pan = self.rng.gen_range(-1.0..=1.0) * pan_spread;
                    let rand_depth = self.rng.gen_range(0.0..=1.0) * depth_spread; 
                    let rand_height = self.rng.gen_range(-1.0..=1.0) * height_spread;
                    
                    // Random time warp variation 
                    let rand_time_warp = time_warp * self.rng.gen_range(0.8..=1.2);
                    
                    // Create a new droplet
                    self.droplets.push(Droplet::new(
                        droplet_position, 
                        duration_samples,
                        rand_time_warp,
                        rand_pan,
                        rand_depth,
                        rand_height
                    ));
                }
            }
            
            // Process all active droplets for this sample
            let mut droplet_left = 0.0;
            let mut droplet_right = 0.0;
            let mut droplet_height = 0.0;
            
            for droplet in &mut self.droplets {
                let (left, right, height) = droplet.process(
                    &self.buffer_left,
                    0.0,
                    droplet_spread,
                    warp_curve
                );
                
                droplet_left += left;
                droplet_right += right;
                droplet_height += height;
            }
            
            // Normalize based on active droplet count
            let droplet_count = self.droplets.len().max(1) as f32;
            droplet_left /= droplet_count.sqrt();
            droplet_right /= droplet_count.sqrt();
            droplet_height /= droplet_count.sqrt();
            
            // Mix height into stereo (creative 3D effect)
            let left_height_mix = droplet_left * (1.0 - height_mix) + 
                                (droplet_left + droplet_height * 0.5) * height_mix;
            let right_height_mix = droplet_right * (1.0 - height_mix) + 
                                 (droplet_right + droplet_height * 0.5) * height_mix;
            
            // Apply dry/wet mix and store output
            channel_left_out[i] = in_left * (1.0 - dry_wet) + left_height_mix * dry_wet;
            channel_right_out[i] = in_right * (1.0 - dry_wet) + right_height_mix * dry_wet;
        }
        
        // Write processed audio back to buffer
        for i in 0..num_samples {
            let frame = buffer.get_frame_mut(i);
            frame[0] = channel_left_out[i];
            frame[1] = channel_right_out[i];
        }
        
        ProcessStatus::Normal
    }

    // No GUI for now to simplify things
    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        None
    }
}

impl ClapPlugin for DropletPlugin {
    const CLAP_ID: &'static str = "com.simply-chris.simply-droplets";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("A 3D droplet-based granular synthesis audio plugin");
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
