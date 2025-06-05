use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static LOGGER: Mutex<Option<std::fs::File>> = Mutex::new(None);

pub fn init_logger() {
    let home_dir = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let log_path = format!("{}/droplets_plugin.log", home_dir);
    
    match OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    {
        Ok(file) => {
            *LOGGER.lock().unwrap() = Some(file);
            log_info(&format!("Logger initialized, writing to: {}", log_path));
            log_info("=== Plugin session started ===");
        }
        Err(e) => {
            eprintln!("Failed to initialize logger: {}", e);
        }
    }
}

fn get_timestamp() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let millis = now.subsec_millis();
    format!("{}.{:03}", secs, millis)
}

pub fn log_info(message: &str) {
    log_message("INFO", message);
}

pub fn log_warn(message: &str) {
    log_message("WARN", message);
}

pub fn log_error(message: &str) {
    log_message("ERROR", message);
}

pub fn log_debug(message: &str) {
    log_message("DEBUG", message);
}

fn log_message(level: &str, message: &str) {
    let timestamp = get_timestamp();
    let log_line = format!("[{}] {}: {}\n", timestamp, level, message);
    
    // Try to write to file
    if let Ok(mut logger) = LOGGER.lock() {
        if let Some(ref mut file) = *logger {
            if let Err(e) = file.write_all(log_line.as_bytes()) {
                eprintln!("Failed to write to log file: {}", e);
            } else {
                let _ = file.flush();
            }
        }
    }
    
    // Also print to console for development
    print!("{}", log_line);
}

pub fn log_ipc_message(direction: &str, message: &str) {
    log_debug(&format!("IPC {}: {}", direction, message));
}

pub fn log_webview_event(event: &str, details: &str) {
    log_debug(&format!("WebView {}: {}", event, details));
}

pub fn log_parameter_change(param: &str, value: f32, normalized: f32) {
    log_debug(&format!("Parameter {}: value={}, normalized={}", param, value, normalized));
}