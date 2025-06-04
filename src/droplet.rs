use std::collections::VecDeque;

// Core droplet structure for granular synthesis
#[derive(Clone)]
pub struct Droplet {
    // Audio data
    pub samples: Vec<f32>,
    pub position: usize,
    pub grain_size: usize,
    
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
    pub fn new(samples: Vec<f32>, grain_size: usize, radius: f32, azimuth: f32, elevation: f32) -> Self {
        Self {
            samples,
            position: 0,
            grain_size,
            radius,
            azimuth, 
            elevation,
            warp_curve: WarpCurve::Linear,
            playback_rate: 1.0,
            amplitude: 1.0,
            age: 0,
            lifetime: grain_size,
            is_active: true,
        }
    }
    
    // Process one sample from this droplet with 3D positioning
    pub fn process_sample(&mut self) -> (f32, f32) { // returns (left, right)
        if !self.is_active || self.age >= self.lifetime {
            self.is_active = false;
            return (0.0, 0.0);
        }
        
        // Apply time warping to position
        let warped_position = self.apply_time_warp();
        
        // Get interpolated sample
        let sample = self.get_interpolated_sample(warped_position);
        
        // Apply envelope (simple linear fade in/out)
        let envelope = self.calculate_envelope();
        let processed_sample = sample * envelope * self.amplitude;
        
        // Convert 3D position to stereo
        let (left, right) = self.position_to_stereo(processed_sample);
        
        self.age += 1;
        self.position = (self.position + 1) % self.samples.len();
        
        (left, right)
    }
    
    fn apply_time_warp(&self) -> f32 {
        let progress = self.age as f32 / self.lifetime as f32;
        match &self.warp_curve {
            WarpCurve::Linear => progress * self.samples.len() as f32,
            WarpCurve::Exponential(exp) => progress.powf(*exp) * self.samples.len() as f32,
            WarpCurve::Sine => ((progress * std::f32::consts::PI / 2.0).sin()) * self.samples.len() as f32,
            WarpCurve::Custom(control_points) => {
                let interpolated_value = self.interpolate_curve(progress, control_points);
                interpolated_value * self.samples.len() as f32
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
    
    fn get_interpolated_sample(&self, position: f32) -> f32 {
        if self.samples.is_empty() {
            return 0.0;
        }
        
        let len = self.samples.len() as f32;
        let pos = position % len;
        let index = pos as usize;
        let frac = pos - index as f32;
        
        let sample1 = self.samples[index];
        let sample2 = self.samples[(index + 1) % self.samples.len()];
        
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
}

impl RainCatcher {
    pub fn new(grain_size: usize) -> Self {
        Self {
            input_buffer: VecDeque::new(),
            grain_size,
            overlap: 0.5,
            density: 10.0,
            samples_since_last: 0,
        }
    }
    
    pub fn process_input(&mut self, input: f32, sample_rate: f32) -> Option<Droplet> {
        self.input_buffer.push_back(input);
        
        // Keep buffer size manageable
        if self.input_buffer.len() > self.grain_size * 4 {
            self.input_buffer.pop_front();
        }
        
        self.samples_since_last += 1;
        
        // Check if we should generate a new droplet
        let samples_per_droplet = sample_rate / self.density;
        if self.samples_since_last as f32 >= samples_per_droplet && self.input_buffer.len() >= self.grain_size {
            self.samples_since_last = 0;
            Some(self.create_droplet())
        } else {
            None
        }
    }
    
    fn create_droplet(&self) -> Droplet {
        // Extract grain from buffer
        let start_pos = self.input_buffer.len().saturating_sub(self.grain_size);
        let samples: Vec<f32> = self.input_buffer.range(start_pos..).cloned().collect();
        
        // Generate random 3D position
        let radius = fastrand::f32() * 0.8 + 0.2; // 0.2 to 1.0
        let azimuth = (fastrand::f32() - 0.5) * 2.0 * std::f32::consts::PI; // -π to π
        let elevation = (fastrand::f32() - 0.5) * std::f32::consts::PI; // -π/2 to π/2
        
        Droplet::new(samples, self.grain_size, radius, azimuth, elevation)
    }
}