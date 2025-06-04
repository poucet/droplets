use wry::application::event_loop::{EventLoop, ControlFlow};
use wry::application::window::WindowBuilder;
use wry::webview::WebViewBuilder;

fn main() {
    let event_loop = EventLoop::new();
    
    let window = WindowBuilder::new()
        .with_title("Simply Droplets - Simple UI Test")
        .with_inner_size(wry::application::dpi::LogicalSize::new(800, 600))
        .build(&event_loop)
        .expect("Failed to create window");

    // Test with a simple HTML content first
    let html_content = r#"
        <!DOCTYPE html>
        <html>
        <head>
            <title>Simply Droplets Test</title>
            <style>
                body { 
                    font-family: Arial, sans-serif; 
                    background: #1a1a1a; 
                    color: #fff; 
                    padding: 20px;
                }
                .param { margin: 20px 0; }
                input[type="range"] { width: 300px; }
            </style>
        </head>
        <body>
            <h1>Simply Droplets - UI Test</h1>
            <div class="param">
                <label>Grain Size: <span id="grain-value">1024</span></label><br>
                <input type="range" id="grain-size" min="64" max="8192" value="1024" 
                       onchange="document.getElementById('grain-value').textContent = this.value">
            </div>
            <div class="param">
                <label>Density: <span id="density-value">10.0</span></label><br>
                <input type="range" id="density" min="0.1" max="100" step="0.1" value="10.0"
                       onchange="document.getElementById('density-value').textContent = this.value">
            </div>
            <div class="param">
                <label>Time Warp: <span id="time-warp-value">1.0</span></label><br>
                <input type="range" id="time-warp" min="0.1" max="4.0" step="0.1" value="1.0"
                       onchange="document.getElementById('time-warp-value').textContent = this.value">
            </div>
            <div class="param">
                <label>Spatial Spread: <span id="spatial-value">100%</span></label><br>
                <input type="range" id="spatial" min="0" max="1" step="0.01" value="1.0"
                       onchange="document.getElementById('spatial-value').textContent = Math.round(this.value * 100) + '%'">
            </div>
            <div class="param">
                <label>Dry/Wet: <span id="dry-wet-value">100%</span></label><br>
                <input type="range" id="dry-wet" min="0" max="1" step="0.01" value="1.0"
                       onchange="document.getElementById('dry-wet-value').textContent = Math.round(this.value * 100) + '%'">
            </div>
            <p><em>UI is working! Parameters can be adjusted.</em></p>
        </body>
        </html>
    "#;

    let _webview = WebViewBuilder::new(window)
        .expect("Failed to create webview builder")
        .with_html(html_content)
        .expect("Failed to load HTML")
        .build()
        .expect("Failed to create webview");

    println!("UI Test window opened successfully!");

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            wry::application::event::Event::WindowEvent {
                event: wry::application::event::WindowEvent::CloseRequested,
                ..
            } => {
                println!("Window closed - UI test successful!");
                *control_flow = ControlFlow::Exit;
            }
            _ => {}
        }
    });
}