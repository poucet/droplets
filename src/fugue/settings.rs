//! Settings management for Simply Droplets
//!
//! Handles persistent settings like export path, loaded from/saved to a JSON file.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::RwLock;
use ts_rs::TS;

/// Application settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Path where exported MIDI files are saved
    pub export_path: PathBuf,

    /// Free-form text appended to the MCP server's system instructions.
    /// Lets the user hand the LLM per-setup context like "Instance 'lead'
    /// drives a Vital synth, CC20 is wavetable position" or "stay in C major
    /// for this session". Read at MCP session start; edit via the Settings UI.
    #[serde(default)]
    pub custom_instructions: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            export_path: default_export_path(),
            custom_instructions: String::new(),
        }
    }
}

/// Get the default export path based on platform
pub fn default_export_path() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("Music")
            .join("Simply Droplets")
            .join("Exports")
    }

    #[cfg(target_os = "windows")]
    {
        dirs::document_dir()
            .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from("C:\\")))
            .join("Simply Droplets")
            .join("Exports")
    }

    #[cfg(target_os = "linux")]
    {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("Music")
            .join("Simply Droplets")
            .join("Exports")
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        PathBuf::from("/tmp/simply-droplets/exports")
    }
}

/// Get the settings file path
fn settings_file_path() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join(".simply-droplets")
            .join("settings.json")
    }

    #[cfg(target_os = "windows")]
    {
        dirs::config_dir()
            .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from("C:\\")))
            .join("Simply Droplets")
            .join("settings.json")
    }

    #[cfg(target_os = "linux")]
    {
        dirs::config_dir()
            .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp")))
            .join("simply-droplets")
            .join("settings.json")
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        PathBuf::from("/tmp/simply-droplets/settings.json")
    }
}

/// Global settings singleton
static SETTINGS: RwLock<Option<Settings>> = RwLock::new(None);

/// Load settings from disk, or return defaults
pub fn load_settings() -> Settings {
    let path = settings_file_path();

    if path.exists() {
        match std::fs::read_to_string(&path) {
            Ok(contents) => {
                match serde_json::from_str::<Settings>(&contents) {
                    Ok(settings) => {
                        log::info!("Loaded settings from {:?}", path);
                        return settings;
                    }
                    Err(e) => {
                        log::warn!("Failed to parse settings file: {}", e);
                    }
                }
            }
            Err(e) => {
                log::warn!("Failed to read settings file: {}", e);
            }
        }
    }

    log::info!("Using default settings");
    Settings::default()
}

/// Save settings to disk
pub fn save_settings(settings: &Settings) -> Result<(), String> {
    let path = settings_file_path();

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create settings directory: {}", e))?;
    }

    let contents = serde_json::to_string_pretty(settings)
        .map_err(|e| format!("Failed to serialize settings: {}", e))?;

    std::fs::write(&path, contents)
        .map_err(|e| format!("Failed to write settings file: {}", e))?;

    log::info!("Saved settings to {:?}", path);
    Ok(())
}

/// Get the current settings (loads from disk on first call)
pub fn get_settings() -> Settings {
    {
        let guard = SETTINGS.read().unwrap();
        if let Some(ref settings) = *guard {
            return settings.clone();
        }
    }

    // Load settings
    let settings = load_settings();
    {
        let mut guard = SETTINGS.write().unwrap();
        *guard = Some(settings.clone());
    }
    settings
}

/// Update settings and save to disk
pub fn update_settings(new_settings: Settings) -> Result<(), String> {
    save_settings(&new_settings)?;
    {
        let mut guard = SETTINGS.write().unwrap();
        *guard = Some(new_settings);
    }
    Ok(())
}

/// Update just the export path
pub fn set_export_path(path: PathBuf) -> Result<(), String> {
    let mut settings = get_settings();
    settings.export_path = path;
    update_settings(settings)
}

/// Ensure the export directory exists, creating it if necessary
pub fn ensure_export_dir() -> Result<PathBuf, String> {
    let settings = get_settings();
    let path = &settings.export_path;

    if !path.exists() {
        std::fs::create_dir_all(path)
            .map_err(|e| format!("Failed to create export directory: {}", e))?;
        log::info!("Created export directory: {:?}", path);
    }

    Ok(path.clone())
}

/// Open the export directory in the system file manager
pub fn reveal_export_dir() -> Result<(), String> {
    let path = ensure_export_dir()?;
    reveal_in_file_manager(&path, None)
}

/// Reveal a specific file in the system file manager
pub fn reveal_file(file_path: &PathBuf) -> Result<(), String> {
    if let Some(parent) = file_path.parent() {
        reveal_in_file_manager(&parent.to_path_buf(), Some(file_path))
    } else {
        Err("Invalid file path".to_string())
    }
}

/// Platform-specific file reveal implementation
fn reveal_in_file_manager(dir: &PathBuf, file: Option<&PathBuf>) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let path_to_reveal = file.unwrap_or(dir);
        std::process::Command::new("open")
            .arg("-R")
            .arg(path_to_reveal)
            .spawn()
            .map_err(|e| format!("Failed to open Finder: {}", e))?;
        Ok(())
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(file_path) = file {
            std::process::Command::new("explorer")
                .arg("/select,")
                .arg(file_path)
                .spawn()
                .map_err(|e| format!("Failed to open Explorer: {}", e))?;
        } else {
            std::process::Command::new("explorer")
                .arg(dir)
                .spawn()
                .map_err(|e| format!("Failed to open Explorer: {}", e))?;
        }
        Ok(())
    }

    #[cfg(target_os = "linux")]
    {
        // Linux can't select files, just open the directory
        std::process::Command::new("xdg-open")
            .arg(dir)
            .spawn()
            .map_err(|e| format!("Failed to open file manager: {}", e))?;
        Ok(())
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        Err("File reveal not supported on this platform".to_string())
    }
}

/// API response type for settings
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct SettingsResponse {
    pub export_path: String,
    pub mcp_port: u16,
    pub mcp_url: String,
    pub custom_instructions: String,
}

impl SettingsResponse {
    pub fn new(settings: &Settings, mcp_port: u16) -> Self {
        Self {
            export_path: settings.export_path.to_string_lossy().to_string(),
            mcp_port,
            mcp_url: format!("http://localhost:{}/mcp", mcp_port),
            custom_instructions: settings.custom_instructions.clone(),
        }
    }
}

/// Request type for updating settings. Any field `None` is left unchanged;
/// distinguishes "don't touch" from "set to empty string."
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateSettingsRequest {
    pub export_path: Option<String>,
    pub custom_instructions: Option<String>,
}
