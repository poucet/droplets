//! Standalone debugging binary for Simply Droplets
//!
//! This runs the MCP bridge, GUI server, and outputs MIDI messages to a virtual MIDI port
//! for quick testing without needing to load the plugin in a DAW.

use midir::{MidiOutput, MidiOutputConnection};
#[cfg(unix)]
use midir::os::unix::VirtualOutput;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use wry::dpi::LogicalSize;
use wry::http::{header::CONTENT_TYPE, Response};
use wry::WebViewBuilder;

fn main() {
    // Initialize logging
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    println!("=== Simply Droplets Standalone ===");
    println!("Starting MCP server, GUI server, and MIDI output...\n");

    // Create params and register with CcBridge
    let params_inst = Arc::new(simply_droplets::params::DropletParams::new());
    let instance_id = "standalone";
    let mut midi_consumer =
        simply_droplets::mcp::CcBridge::register(instance_id, Arc::clone(&params_inst));

    // Register with FugueBridge for fugue sequencing
    let (_fugue_consumer, _fugue_info_handle) =
        simply_droplets::fugue::FugueBridge::register(instance_id, instance_id);

    // Start MCP server (shared singleton)
    simply_droplets::mcp::start_server(simply_droplets::mcp::DEFAULT_MCP_PORT);
    println!(
        "MCP server running on port {}",
        simply_droplets::mcp::DEFAULT_MCP_PORT
    );

    // Start GUI server (shared singleton) - enables browser-based UI access
    simply_droplets::gui::server::start_server(simply_droplets::gui::server::DEFAULT_GUI_PORT);
    println!(
        "GUI server running on http://127.0.0.1:{}",
        simply_droplets::gui::server::DEFAULT_GUI_PORT
    );

    // Setup MIDI output
    let midi_out = match MidiOutput::new("Simply Droplets Standalone") {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Failed to create MIDI output: {}", e);
            return;
        }
    };
    let out_ports = midi_out.ports();

    // List available MIDI ports
    println!("\nAvailable MIDI output ports:");
    for (i, p) in out_ports.iter().enumerate() {
        if let Ok(name) = midi_out.port_name(p) {
            println!("  {}: {}", i, name);
        }
    }

    // Create a virtual MIDI port or use first available
    let mut conn_out: Option<MidiOutputConnection> = None;

    if out_ports.is_empty() {
        println!("\nCreating virtual MIDI port: 'Droplets Out'");
        match midi_out.create_virtual("Droplets Out") {
            Ok(c) => {
                println!("Virtual port created successfully!");
                conn_out = Some(c);
            }
            Err(e) => {
                println!("Warning: Could not create virtual port: {}", e);
                println!("MIDI messages will be logged only (no audio output)");
            }
        }
    } else {
        if let Ok(name) = midi_out.port_name(&out_ports[0]) {
            println!("\nUsing first available MIDI port: {}", name);
        }
        conn_out = midi_out.connect(&out_ports[0], "droplets").ok();
    }

    println!("\nStandalone ready!");
    println!("- GUI window will open");
    println!(
        "- Browser UI available at http://127.0.0.1:{}",
        simply_droplets::gui::server::DEFAULT_GUI_PORT
    );
    println!("- Click notes on the piano keyboard");
    println!("- Or use MCP tools to send MIDI");
    println!("\nClose the window to quit\n");

    // Wrap MIDI connection in Arc<Mutex> to share with MIDI thread
    let conn_out = Arc::new(std::sync::Mutex::new(conn_out));
    let conn_out_clone = Arc::clone(&conn_out);

    // Spawn MIDI output thread
    thread::spawn(move || loop {
        while let Ok(msg) = midi_consumer.pop() {
            match msg {
                simply_droplets::mcp::MidiMessage::Note(note) => {
                    let note_type = if note.is_note_on { "NoteOn" } else { "NoteOff" };
                    println!(
                        "🎵 {} note={} vel={} ch={}",
                        note_type, note.note, note.velocity, note.channel
                    );

                    // Send MIDI 1.0 message
                    if let Ok(mut conn) = conn_out_clone.lock() {
                        if let Some(c) = conn.as_mut() {
                            let status = if note.is_note_on {
                                0x90 | (note.channel & 0x0F)
                            } else {
                                0x80 | (note.channel & 0x0F)
                            };
                            let midi_data = [status, note.note, note.velocity];
                            if let Err(e) = c.send(&midi_data) {
                                eprintln!("Error sending MIDI: {}", e);
                            }
                        }
                    }
                }
                simply_droplets::mcp::MidiMessage::Cc(cc) => {
                    println!("🎛️  CC{} = {} ch={}", cc.cc, cc.value, cc.channel);

                    if let Ok(mut conn) = conn_out_clone.lock() {
                        if let Some(c) = conn.as_mut() {
                            let status = 0xB0 | (cc.channel & 0x0F);
                            let midi_data = [status, cc.cc, cc.value];
                            if let Err(e) = c.send(&midi_data) {
                                eprintln!("Error sending MIDI: {}", e);
                            }
                        }
                    }
                }
                simply_droplets::mcp::MidiMessage::PerNoteExpression(expr) => {
                    println!(
                        "🎚️  Per-note expression on note {} ch={}",
                        expr.note, expr.channel
                    );
                    // MIDI 2.0 only - skip for now in standalone
                }
            }
        }

        thread::sleep(Duration::from_millis(10));
    });

    // Create GUI window with wry/tao
    use tao::{
        event::{Event, WindowEvent},
        event_loop::{ControlFlow, EventLoop},
        window::WindowBuilder,
    };

    let event_loop = EventLoop::new();
    let window = match WindowBuilder::new()
        .with_title("Simply Droplets - Standalone")
        .with_inner_size(LogicalSize::new(800.0, 600.0))
        .build(&event_loop)
    {
        Ok(w) => w,
        Err(e) => {
            eprintln!("Failed to create window: {}", e);
            return;
        }
    };

    let params_for_protocol = Arc::clone(&params_inst);

    // Build webview with same configuration as plugin GUI
    let _webview = match WebViewBuilder::new()
        .with_html(include_str!("../../frontend/dist/index.html"))
        .with_devtools(cfg!(debug_assertions) || cfg!(feature = "dev-gui"))
        .with_initialization_script(include_str!("../gui/script.js"))
        .with_asynchronous_custom_protocol(
            "droplets".to_string(),
            move |_webview_id, request, responder| {
                let params = Arc::clone(&params_for_protocol);
                let uri = request.uri();
                let path = uri.path();
                let method = request.method().as_str();
                let body = request.body();

                let response_body =
                    simply_droplets::gui::routes::handle_request(path, method, body, &params);

                let response = Response::builder()
                    .header(CONTENT_TYPE, "application/json")
                    .header("Access-Control-Allow-Origin", "*")
                    .body(response_body.into_bytes())
                    .unwrap();
                responder.respond(response);
            },
        )
        .with_navigation_handler(|url| {
            if url.starts_with("http") {
                if let Err(e) = open::that(url) {
                    eprintln!("Failed to open URL: {}", e);
                }
                false
            } else {
                true
            }
        })
        .build(&window)
    {
        Ok(w) => w,
        Err(e) => {
            eprintln!("Failed to create webview: {}", e);
            return;
        }
    };

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                println!("Window closed, exiting...");
                *control_flow = ControlFlow::Exit;
            }
            _ => (),
        }
    });
}
