use crate::DropletParams;
use nih_plug::prelude::*;
use std::sync::Arc;

pub struct DropletEditor {
    params: Arc<DropletParams>,
    context: Arc<dyn GuiContext>,
}

impl DropletEditor {
    pub fn new(params: Arc<DropletParams>, context: Arc<dyn GuiContext>) -> Self {
        Self { params, context }
    }

    fn get_frontend_path() -> std::path::PathBuf {
        // Look for frontend files in plugin bundle location first
        let exe_path = std::env::current_exe().unwrap_or_default();
        let bundle_paths = [
            exe_path.parent().unwrap_or_else(|| std::path::Path::new(".")).join("Contents").join("Resources").join("index.html"),
            exe_path.parent().unwrap_or_else(|| std::path::Path::new(".")).join("Resources").join("index.html"),
        ];
        
        for p in &bundle_paths {
            if p.exists() {
                return p.clone();
            }
        }
        
        // Development fallback paths
        let dev_paths = [
            std::env::current_dir().unwrap_or_default().join("frontend").join("dist").join("index.html"),
            std::env::current_dir().unwrap_or_default().join("dist").join("index.html"),
        ];
        
        for p in &dev_paths {
            if p.exists() {
                return p.clone();
            }
        }
        
        // Final fallback
        std::env::current_dir()
            .unwrap_or_default()
            .join("frontend")
            .join("dist")
            .join("index.html")
    }
}

impl Editor for DropletEditor {
    fn spawn(
        &self,
        _parent: ParentWindowHandle,
        _context: Arc<dyn GuiContext>,
    ) -> Box<dyn std::any::Any + Send> {
        let frontend_path = Self::get_frontend_path();
        let url = format!("file://{}", frontend_path.display());
        
        // For now, just return a simple placeholder to prevent DAW crashes
        // The complex webview integration can be added later once basic plugin loading works
        println!("Editor would load: {}", url);
        
        Box::new(())
    }

    fn size(&self) -> (u32, u32) {
        (800, 600)
    }

    fn set_scale_factor(&self, _factor: f32) -> bool {
        false
    }

    fn param_value_changed(&self, id: &str, normalized_value: f32) {
        println!("Parameter {} changed to {}", id, normalized_value);
    }

    fn param_modulation_changed(&self, _id: &str, _modulation_offset: f32) {
        // TODO: Handle modulation changes
    }

    fn param_values_changed(&self) {
        // TODO: Handle bulk parameter updates
    }
}