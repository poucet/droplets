use wry::application::event_loop::{EventLoop, ControlFlow};
use wry::application::window::WindowBuilder;
use wry::webview::WebViewBuilder;
use std::path::Path;

fn main() {
    // Find the frontend dist directory
    let frontend_path = find_frontend_path();
    
    println!("Loading frontend from: {}", frontend_path.display());
    
    let event_loop = EventLoop::new();
    
    let window = WindowBuilder::new()
        .with_title("Simply Droplets - UI Test")
        .with_inner_size(wry::application::dpi::LogicalSize::new(800, 600))
        .build(&event_loop)
        .expect("Failed to create window");

    let _webview = WebViewBuilder::new(window)
        .expect("Failed to create webview builder")
        .with_url(&format!("file://{}", frontend_path.display()))
        .expect("Failed to load URL")
        .with_ipc_handler(move |_window, request| {
            println!("Received IPC message: {}", request);
            
            // Parse and handle basic messages
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&request) {
                if let Some(msg_type) = json["type"].as_str() {
                    match msg_type {
                        "GetAllParameters" => {
                            println!("Frontend requested all parameters");
                        }
                        "SetParameter" => {
                            if let (Some(id), Some(value)) = (
                                json["id"].as_str(),
                                json["value"].as_f64()
                            ) {
                                println!("Frontend wants to set {} to {}", id, value);
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
}

fn find_frontend_path() -> std::path::PathBuf {
    let current_dir = std::env::current_dir().unwrap_or_default();
    let possible_paths = [
        current_dir.join("frontend").join("dist").join("index.html"),
        current_dir.join("dist").join("index.html"),
    ];
    
    for p in &possible_paths {
        println!("Checking path: {}", p.display());
        if p.exists() {
            println!("Found frontend at: {}", p.display());
            return p.clone();
        }
    }
    
    // Fallback to the most likely path
    current_dir.join("frontend").join("dist").join("index.html")
}