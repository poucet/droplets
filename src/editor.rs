use crate::DropletParams;
use nih_plug::prelude::*;
use std::sync::Arc;
use std::thread;

pub struct DropletEditor {
    params: Arc<DropletParams>,
    context: Arc<dyn GuiContext>,
}

impl DropletEditor {
    pub fn new(params: Arc<DropletParams>, context: Arc<dyn GuiContext>) -> Self {
        Self { params, context }
    }

    fn get_frontend_path() -> std::path::PathBuf {
        // Try to find the frontend dist directory
        let possible_paths = [
            std::env::current_dir().unwrap_or_default().join("frontend").join("dist").join("index.html"),
            std::env::current_dir().unwrap_or_default().join("dist").join("index.html"),
        ];
        
        for p in &possible_paths {
            if p.exists() {
                return p.clone();
            }
        }
        
        // Fallback to development path
        std::env::current_dir()
            .unwrap_or_default()
            .join("frontend")
            .join("dist")
            .join("index.html")
    }

    fn create_standalone_window(&self) {
        let frontend_path = Self::get_frontend_path();
        let params = self.params.clone();
        
        thread::spawn(move || {
            use wry::application::event_loop::{EventLoop, ControlFlow};
            use wry::application::window::WindowBuilder;
            use wry::webview::WebViewBuilder;
            
            let event_loop = EventLoop::new();
            
            let window = WindowBuilder::new()
                .with_title("Simply Droplets")
                .with_inner_size(wry::application::dpi::LogicalSize::new(800, 600))
                .build(&event_loop)
                .expect("Failed to create window");

            let _webview = WebViewBuilder::new(window)
                .expect("Failed to create webview builder")
                .with_url(&format!("file://{}", frontend_path.display()))
                .expect("Failed to load URL")
                .with_ipc_handler(move |_window, request| {
                    // Simple logging for now - parameter setting will be added later
                    println!("Received IPC message: {}", request);
                    
                    // Parse and handle basic messages
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&request) {
                        if let Some(msg_type) = json["type"].as_str() {
                            match msg_type {
                                "GetAllParameters" => {
                                    println!("Frontend requested all parameters");
                                    // TODO: Send parameters back to frontend
                                }
                                "SetParameter" => {
                                    if let (Some(id), Some(value)) = (
                                        json["id"].as_str(),
                                        json["value"].as_f64()
                                    ) {
                                        println!("Frontend wants to set {} to {}", id, value);
                                        // TODO: Update parameter through proper API
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                })
                .build()
                .expect("Failed to create webview");

            event_loop.run(move |event, _, control_flow| {
                *control_flow = ControlFlow::Wait;

                match event {
                    wry::application::event::Event::WindowEvent {
                        event: wry::application::event::WindowEvent::CloseRequested,
                        ..
                    } => {
                        *control_flow = ControlFlow::Exit;
                    }
                    _ => {}
                }
            });
        });
    }
}

impl Editor for DropletEditor {
    fn spawn(
        &self,
        _parent: ParentWindowHandle,
        _context: Arc<dyn GuiContext>,
    ) -> Box<dyn std::any::Any + Send> {
        // For MVP, create a standalone window
        // This will work for testing the React UI integration
        self.create_standalone_window();
        
        Box::new(())
    }

    fn size(&self) -> (u32, u32) {
        (800, 600)
    }

    fn set_scale_factor(&self, _factor: f32) -> bool {
        false
    }

    fn param_value_changed(&self, id: &str, normalized_value: f32) {
        // Convert normalized value back to actual value for logging
        let actual_value = match id {
            "grain_size" => {
                let min = 64.0;
                let max = 8192.0;
                min + normalized_value * (max - min)
            }
            "density" => {
                let min = 0.1;
                let max = 100.0;
                min + normalized_value * (max - min)
            }
            "time_warp" => {
                let min = 0.1;
                let max = 4.0;
                min + normalized_value * (max - min)
            }
            "spatial_spread" | "dry_wet" => normalized_value,
            _ => normalized_value,
        };
        
        println!("Parameter {} changed to {} (normalized: {})", id, actual_value, normalized_value);
    }

    fn param_modulation_changed(&self, _id: &str, _modulation_offset: f32) {
        // TODO: Handle modulation changes
    }

    fn param_values_changed(&self) {
        // TODO: Handle bulk parameter updates
    }
}