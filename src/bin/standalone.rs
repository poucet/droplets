//! Standalone debugging binary for Simply Droplets
//!
//! This runs the MCP bridge, GUI server, and outputs MIDI messages to a virtual MIDI port
//! for quick testing without needing to load the plugin in a DAW.

use midir::{MidiOutput, MidiOutputConnection};
#[cfg(unix)]
use midir::os::unix::VirtualOutput;
use simply_droplets::gui::{configure_webview, WebViewConfig, DEFAULT_GUI_SIZE};
use simply_droplets::mcp::MidiMessage;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use wry::WebViewBuilder;

/// Send a MidiMessage over a shared virtual-MIDI connection. MIDI 2.0 per-note
/// expressions don't map to 3-byte MIDI 1.0 and are skipped here; test per-note
/// features via the plugin path in a DAW instead.
fn send_standalone_midi(conn: &Arc<Mutex<Option<MidiOutputConnection>>>, msg: &MidiMessage) {
    let Ok(mut guard) = conn.lock() else { return };
    let Some(c) = guard.as_mut() else { return };
    match msg {
        MidiMessage::Note(note) => {
            let status = (if note.is_note_on { 0x90 } else { 0x80 }) | (note.channel & 0x0F);
            let _ = c.send(&[status, note.note, note.velocity]);
        }
        MidiMessage::Cc(cc) => {
            let status = 0xB0 | (cc.channel & 0x0F);
            let _ = c.send(&[status, cc.cc, cc.value]);
        }
        MidiMessage::PerNoteExpression(_) => {
            // Not representable as MIDI 1.0 three-byte messages.
        }
    }
}

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

    // Register with FugueBridge for fugue sequencing. Unlike the plugin path
    // (where the DAW's audio thread drives FugueSequencer::process), standalone
    // has no audio thread — so we spawn one below that simulates a fixed-tempo
    // always-playing transport and drains the fugue command buffer. Without
    // this, queued fugues sit in the ring buffer forever and never appear in
    // the fugue info cache, which is the "queue fugue disappears" UI bug.
    let (fugue_consumer, fugue_info_handle) =
        simply_droplets::fugue::FugueBridge::register(instance_id, instance_id);

    // Start MCP server (shared singleton) - use standalone port to avoid conflict with plugin
    simply_droplets::mcp::start_server(simply_droplets::mcp::STANDALONE_MCP_PORT);
    println!(
        "MCP server running on port {}",
        simply_droplets::mcp::STANDALONE_MCP_PORT
    );

    // Start GUI server (shared singleton) - use standalone port to avoid conflict with plugin
    simply_droplets::gui::server::start_server(simply_droplets::gui::server::STANDALONE_GUI_PORT);
    println!(
        "GUI server running on http://127.0.0.1:{}",
        simply_droplets::gui::server::STANDALONE_GUI_PORT
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
        simply_droplets::gui::server::STANDALONE_GUI_PORT
    );
    println!("- Click notes on the piano keyboard");
    println!(
        "- Or use MCP tools on port {}",
        simply_droplets::mcp::STANDALONE_MCP_PORT
    );
    println!("\nClose the window to quit\n");

    // Wrap MIDI connection in Arc<Mutex> to share with MIDI thread and fugue thread
    let conn_out = Arc::new(std::sync::Mutex::new(conn_out));
    let conn_out_clone = Arc::clone(&conn_out);
    let conn_out_fugue = Arc::clone(&conn_out);

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

    // Spawn fugue simulation thread. In plugin mode this work is done by the DAW's
    // audio thread calling FugueSequencer::process each buffer. Standalone has no
    // audio thread, so we fake one: a fixed-tempo always-playing transport that
    // drains the fugue command ring buffer and dispatches events to MIDI. Without
    // this, queued fugues never appear in the info cache and seem to "disappear."
    thread::spawn(move || {
        use simply_droplets::fugue::{FugueSequencer, ProcessedEvent, TransportState};
        use simply_droplets::mcp::{CcMessage, MidiMessage};

        let sample_rate: f64 = 48_000.0;
        let bpm: f64 = 120.0;
        let step_ms: u64 = 5;
        let time_sig_num: u32 = 4;
        let beats_per_step: f64 = bpm / 60.0 * step_ms as f64 / 1000.0;
        let frames_per_step: u32 = (sample_rate * step_ms as f64 / 1000.0) as u32;

        let mut sequencer = FugueSequencer::new(fugue_consumer, sample_rate);
        let mut current_beat: f64 = 0.0;

        loop {
            let events: Vec<ProcessedEvent> = sequencer
                .process(true, current_beat, bpm, frames_per_step, time_sig_num)
                .collect();

            for event in events {
                match event {
                    ProcessedEvent::Instant { message, .. } => {
                        send_standalone_midi(&conn_out_fugue, &message);
                    }
                    ProcessedEvent::CcRamp { channel, cc, end_value, .. } => {
                        // Each 5ms step emits the interpolated value at the end
                        // of this buffer — ~200 Hz update rate, plenty smooth.
                        let msg = MidiMessage::Cc(CcMessage::new(channel, cc, end_value));
                        send_standalone_midi(&conn_out_fugue, &msg);
                    }
                }
            }

            // Update UI caches so /api/transport and /api/fugues reflect live state.
            fugue_info_handle.update_transport(TransportState {
                beat: current_beat,
                tempo: bpm,
                playing: true,
                time_sig_numerator: time_sig_num,
                is_looping: false,
                loop_start_beat: 0.0,
                loop_end_beat: 0.0,
            });
            fugue_info_handle.update(sequencer.list_fugues(current_beat, time_sig_num));
            fugue_info_handle.update_definitions(sequencer.get_definitions());

            current_beat += beats_per_step;
            thread::sleep(Duration::from_millis(step_ms));
        }
    });

    // Create IPC channel for receiving messages from the webview
    let (ipc_sender, ipc_receiver) = crossbeam::channel::unbounded::<serde_json::Value>();

    // Spawn IPC message handler thread
    thread::spawn(move || loop {
        while let Ok(msg) = ipc_receiver.try_recv() {
            // Handle IPC messages from the frontend (e.g., WebSocket shim messages)
            if let Some(msg_type) = msg.get("type").and_then(|t| t.as_str()) {
                match msg_type {
                    "ws_message" => {
                        // Handle WebSocket-like messages from frontend
                        if let Some(data) = msg.get("data") {
                            println!("IPC ws_message: {:?}", data);
                        }
                    }
                    _ => {
                        println!("IPC message: {:?}", msg);
                    }
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
        .with_inner_size(DEFAULT_GUI_SIZE)
        .build(&event_loop)
    {
        Ok(w) => w,
        Err(e) => {
            eprintln!("Failed to create window: {}", e);
            return;
        }
    };

    // Build webview with shared configuration (same as plugin GUI)
    let config = WebViewConfig::standalone().with_ipc_sender(ipc_sender);
    let builder = configure_webview(WebViewBuilder::new(), Arc::clone(&params_inst), config);

    let _webview = match builder.build(&window) {
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
