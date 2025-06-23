use std::collections::VecDeque;

// Core droplet structure for granular synthesis
#[derive(Clone)]
pub struct Droplet {
    // Audio data - streaming buffer
    pub samples: Vec<f32>,
    pub write_position: usize,    // Where new samples are written
    pub read_position: f32,       // Where playback reads from (can be fractional)
    pub grain_size: usize,        // Target size when fully filled
    pub is_fully_filled: bool,    // Whether writing is complete
    
    // 3D positioning (spherical coordinates)
    pub radius: f32,    // Distance from center (0.0 to 1.0)
    pub azimuth: f32,   // Horizontal angle in radians (-π to π)
    pub elevation: f32, // Vertical angle in radians (-π/2 to π/2)
    
    // Time warping
    pub warp_curve: WarpCurve,
    pub playback_rate: f32,
    
    // Envelope and lifecycle
    pub amplitude: f32,
    pub age: usize,
    pub lifetime: usize,
    pub is_active: bool,
    pub playback_started: bool,   // Whether playback has begun
}

// Time warp curve types
#[derive(Clone)]
pub enum WarpCurve {
    Linear,
    Exponential(f32), // exponent parameter
    Sine,
    Custom(Vec<(f32, f32)>), // list of (time, value) control points
}

impl Droplet {
    pub fn new(grain_size: usize, radius: f32, azimuth: f32, elevation: f32) -> Self {
        Self {
            samples: Vec::with_capacity(grain_size),
            write_position: 0,
            read_position: 0.0,
            grain_size,
            is_fully_filled: false,
            radius,
            azimuth, 
            elevation,
            warp_curve: WarpCurve::Linear,
            playback_rate: 1.0,
            amplitude: 1.0,
            age: 0,
            lifetime: grain_size,
            is_active: true,
            playback_started: false,
        }
    }
    
    // Add a sample to the droplet's buffer
    pub fn write_sample(&mut self, sample: f32) -> bool {
        if self.is_fully_filled {
            return false;
        }
        
        if self.write_position < self.grain_size {
            if self.samples.len() <= self.write_position {
                self.samples.push(sample);
            } else {
                self.samples[self.write_position] = sample;
            }
            self.write_position += 1;
            
            if self.write_position >= self.grain_size {
                self.is_fully_filled = true;
            }
            true
        } else {
            self.is_fully_filled = true;
            false
        }
    }
    
    // Check if droplet has enough samples to start playback
    pub fn can_start_playback(&self) -> bool {
        self.samples.len() >= 64 // Minimum buffer for smooth playback
    }
    
    // Process one sample from this droplet with 3D positioning
    pub fn process_sample(&mut self) -> (f32, f32) { // returns (left, right)
        if !self.is_active || self.age >= self.lifetime {
            self.is_active = false;
            return (0.0, 0.0);
        }
        
        // Check if we can start/continue playback
        if !self.playback_started {
            if !self.can_start_playback() {
                return (0.0, 0.0); // Not enough samples yet
            }
            self.playback_started = true;
        }
        
        // Apply time warping to position
        let warped_position = self.apply_time_warp();
        
        // Get interpolated sample, respecting write boundary
        let sample = self.get_interpolated_sample_bounded(warped_position);
        
        // Apply envelope (simple linear fade in/out)
        let envelope = self.calculate_envelope();
        let processed_sample = sample * envelope * self.amplitude;
        
        // Convert 3D position to stereo
        let (left, right) = self.position_to_stereo(processed_sample);
        
        self.age += 1;
        
        // Advance read position, respecting playback rate
        self.read_position += self.playback_rate;
        
        (left, right)
    }
    
    fn apply_time_warp(&self) -> f32 {
        let progress = self.age as f32 / self.lifetime as f32;
        let available_samples = self.samples.len() as f32;
        
        match &self.warp_curve {
            WarpCurve::Linear => self.read_position,
            WarpCurve::Exponential(exp) => {
                let warped_progress = progress.powf(*exp);
                warped_progress * available_samples
            },
            WarpCurve::Sine => {
                let warped_progress = (progress * std::f32::consts::PI / 2.0).sin();
                warped_progress * available_samples
            },
            WarpCurve::Custom(control_points) => {
                let interpolated_value = self.interpolate_curve(progress, control_points);
                interpolated_value * available_samples
            }
        }
    }
    
    fn interpolate_curve(&self, t: f32, control_points: &[(f32, f32)]) -> f32 {
        if control_points.is_empty() {
            return t; // fallback to linear
        }
        
        if control_points.len() == 1 {
            return control_points[0].1;
        }
        
        // Find the two control points to interpolate between
        let mut left_point = control_points[0];
        let mut right_point = control_points[control_points.len() - 1];
        
        for i in 0..control_points.len() - 1 {
            if t >= control_points[i].0 && t <= control_points[i + 1].0 {
                left_point = control_points[i];
                right_point = control_points[i + 1];
                break;
            }
        }
        
        // Linear interpolation between control points
        if (right_point.0 - left_point.0).abs() < f32::EPSILON {
            return left_point.1;
        }
        
        let local_t = (t - left_point.0) / (right_point.0 - left_point.0);
        left_point.1 * (1.0 - local_t) + right_point.1 * local_t
    }
    
    fn get_interpolated_sample_bounded(&self, position: f32) -> f32 {
        if self.samples.is_empty() {
            return 0.0;
        }
        
        // Clamp position to written samples only
        let max_pos = (self.write_position as f32 - 1.0).max(0.0);
        let pos = position.min(max_pos);
        
        if pos < 0.0 {
            return 0.0;
        }
        
        let index = pos as usize;
        let frac = pos - index as f32;
        
        if index >= self.samples.len() {
            return 0.0;
        }
        
        let sample1 = self.samples[index];
        let sample2 = if index + 1 < self.samples.len() {
            self.samples[index + 1]
        } else {
            sample1 // No next sample available yet
        };
        
        // Linear interpolation
        sample1 * (1.0 - frac) + sample2 * frac
    }
    
    fn calculate_envelope(&self) -> f32 {
        let progress = self.age as f32 / self.lifetime as f32;
        
        // Simple linear fade in/out envelope
        if progress < 0.1 {
            progress / 0.1 // fade in
        } else if progress > 0.9 {
            (1.0 - progress) / 0.1 // fade out
        } else {
            1.0 // sustain
        }
    }
    
    // Convert 3D spherical coordinates to stereo positioning
    fn position_to_stereo(&self, sample: f32) -> (f32, f32) {
        // Convert spherical to cartesian for easier stereo calculation
        let x = self.radius * self.azimuth.cos() * self.elevation.cos();
        let _y = self.radius * self.azimuth.sin() * self.elevation.cos();
        let _z = self.radius * self.elevation.sin();
        
        // Simple stereo panning based on x position (-1 = left, +1 = right)
        let pan = x.clamp(-1.0, 1.0);
        let left_gain = ((1.0 - pan) * 0.5).sqrt();
        let right_gain = ((1.0 + pan) * 0.5).sqrt();
        
        // Apply distance attenuation based on radius
        let distance_attenuation = 1.0 / (1.0 + self.radius * 2.0);
        
        let final_sample = sample * distance_attenuation;
        (final_sample * left_gain, final_sample * right_gain)
    }
}

// Rain catcher system for generating droplets
pub struct RainCatcher {
    pub input_buffer: VecDeque<f32>,
    pub grain_size: usize,
    pub overlap: f32,
    pub density: f32, // droplets per second
    pub samples_since_last: usize,
    pub active_droplets: Vec<usize>, // Indices of droplets being filled
}

impl RainCatcher {
    pub fn new() -> Self {
        Self {
            input_buffer: VecDeque::new(),
            grain_size: 1024, // Default value, will be updated dynamically
            overlap: 0.5,
            density: 10.0,
            samples_since_last: 0,
            active_droplets: Vec::new(),
        }
    }
    
    // Process input and manage droplet creation - now returns new droplet if one should be created
    pub fn process_input(&mut self, input: f32, sample_rate: f32, current_grain_size: usize) -> Option<Droplet> {
        self.input_buffer.push_back(input);
        
        // Update grain size if it has changed
        self.grain_size = current_grain_size;
        
        // Keep buffer size manageable
        if self.input_buffer.len() > self.grain_size * 4 {
            self.input_buffer.pop_front();
        }
        
        self.samples_since_last += 1;
        
        // Check if we should generate a new droplet
        let samples_per_droplet = sample_rate / self.density;
        if self.samples_since_last as f32 >= samples_per_droplet {
            self.samples_since_last = 0;
            Some(self.create_droplet())
        } else {
            None
        }
    }
    
    // Feed samples to active droplets that are still filling
    pub fn feed_droplets(&mut self, droplets: &mut [Droplet], input: f32) {
        // Remove completed droplets from active list
        self.active_droplets.retain(|&index| {
            if index < droplets.len() {
                !droplets[index].is_fully_filled
            } else {
                false
            }
        });
        
        // Feed input to all active droplets
        for &index in &self.active_droplets {
            if index < droplets.len() {
                droplets[index].write_sample(input);
            }
        }
    }
    
    // Register a new droplet for feeding
    pub fn register_droplet(&mut self, droplet_index: usize) {
        if !self.active_droplets.contains(&droplet_index) {
            self.active_droplets.push(droplet_index);
        }
    }
    
    fn create_droplet(&self) -> Droplet {
        // Generate random 3D position
        let radius = fastrand::f32() * 0.8 + 0.2; // 0.2 to 1.0
        let azimuth = (fastrand::f32() - 0.5) * 2.0 * std::f32::consts::PI; // -π to π
        let elevation = (fastrand::f32() - 0.5) * std::f32::consts::PI; // -π/2 to π/2
        
        // Create empty droplet that will be filled gradually
        Droplet::new(self.grain_size, radius, azimuth, elevation)
    }
}