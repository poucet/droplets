use clack_plugin::{host::HostAudioProcessorHandle, plugin::{PluginAudioProcessor, PluginError}, process::{Audio, Events, PluginAudioConfiguration, Process, ProcessStatus}};
use clack_plugin::process::audio::ChannelPair;
use std::collections::VecDeque;

use crate::{DropletMainThread, DropletShared, droplet::{Droplet, RainCatcher, WarpCurve}};

pub struct DropletAudioProcessor<'a> {
    shared: &'a DropletShared<'a>,
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

impl<'a> PluginAudioProcessor<'a, DropletShared<'a>, DropletMainThread<'a>>
    for DropletAudioProcessor<'a>
{
    fn activate(
        _host: HostAudioProcessorHandle<'a>,
        _main_thread: &mut DropletMainThread<'a>,
        shared: &'a DropletShared,
        audio_config: PluginAudioConfiguration,
    ) -> Result<Self, PluginError> {
        crate::logger::log_info("Audio processor activated");
        
        let sample_rate = audio_config.sample_rate as f32;
        let grain_size = shared.params.get_grain_size() as usize;
        
        Ok(Self { 
            shared,
            sample_rate,
            rain_catcher_left: RainCatcher::new(grain_size),
            rain_catcher_right: RainCatcher::new(grain_size),
            active_droplets: Vec::new(),
            dry_buffer_left: VecDeque::new(),
            dry_buffer_right: VecDeque::new(),
            dry_delay_samples: grain_size / 2,
        })
    }

    fn process(
        &mut self,
        _process: Process,
        mut audio: Audio,
        events: Events,
    ) -> Result<ProcessStatus, PluginError> {
        self.shared.host.request_callback();

        // Get the first port pair for stereo I/O
        let mut port_pair = audio
            .port_pair(0)
            .ok_or(PluginError::Message("No input/output ports found"))?;

        let mut output_channels = port_pair
            .channels()?
            .into_f32()
            .ok_or(PluginError::Message("Expected f32 input/output"))?;

        let mut channel_buffers = [None, None];

        // Extract buffer slices
        for (pair, buf) in output_channels.iter_mut().zip(&mut channel_buffers) {
            *buf = match pair {
                ChannelPair::InputOnly(_) => None,
                ChannelPair::OutputOnly(_) => None,
                ChannelPair::InPlace(b) => Some(b),
                ChannelPair::InputOutput(i, o) => {
                    o.copy_from_slice(i);
                    Some(o)
                }
            }
        }

        // Process parameter events
        for event_batch in events.input.batch() {
            for event in event_batch.events() {
                self.shared.params.handle_event(event)
            }

            // Get parameter values after processing events
            let gain = self.shared.params.get_gain();
            let density = self.shared.params.get_density();
            let time_warp = self.shared.params.get_time_warp();
            let spatial_spread = self.shared.params.get_spatial_spread();
            let dry_wet = self.shared.params.get_dry_wet();
            
            // Update rain catcher parameters
            self.rain_catcher_left.density = density;
            self.rain_catcher_right.density = density;

            // Process audio samples
            let channel_0 = channel_buffers[0].take();
            let channel_1 = channel_buffers[1].take();
            match (channel_0, channel_1) {
                (Some(left_buf), Some(right_buf)) => {
                    // Stereo processing
                    for (left_sample, right_sample) in left_buf.iter_mut().zip(right_buf.iter_mut()) {
                        let input_left = *left_sample;
                        let input_right = *right_sample;
                        
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
                        *left_sample = mixed_left * gain;
                        *right_sample = mixed_right * gain;
                    }
                }
                (Some(mono_buf), None) => {
                    // Mono processing
                    for sample in mono_buf.iter_mut() {
                        let input = *sample;
                        
                        // Store dry sample with delay compensation
                        self.dry_buffer_left.push_back(input);
                        
                        while self.dry_buffer_left.len() > self.dry_delay_samples + 1 {
                            self.dry_buffer_left.pop_front();
                        }
                        
                        let dry_sample = self.dry_buffer_left.front().copied().unwrap_or(0.0);
                        
                        // Create new droplets
                        if let Some(mut droplet) = self.rain_catcher_left.process_input(input, self.sample_rate) {
                            droplet.radius *= spatial_spread;
                            droplet.warp_curve = WarpCurve::Exponential(time_warp);
                            self.active_droplets.push(droplet);
                        }
                        
                        // Process active droplets
                        let mut wet_sample = 0.0;
                        
                        self.active_droplets.retain_mut(|droplet| {
                            if droplet.is_active {
                                let (left, right) = droplet.process_sample();
                                wet_sample += (left + right) * 0.5; // Mix to mono
                                droplet.is_active
                            } else {
                                false
                            }
                        });
                        
                        // Mix dry and wet signals
                        let mixed = dry_sample * (1.0 - dry_wet) + wet_sample * dry_wet;
                        
                        // Apply gain
                        *sample = mixed * gain;
                    }
                }
                _ => {
                    // No valid channels
                }
            }
        }

        Ok(ProcessStatus::ContinueIfNotQuiet)   
    }
}