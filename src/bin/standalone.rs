//! Standalone debugging binary for Droplets
//!
//! This runs the MCP bridge, GUI server, and outputs MIDI messages to a virtual MIDI port
//! for quick testing without needing to load the plugin in a DAW.

use midir::{MidiOutput, MidiOutputConnection};
#[cfg(unix)]
use midir::os::unix::VirtualOutput;
use droplets::gui::{configure_webview, WebViewConfig, DEFAULT_GUI_SIZE};
use droplets::mcp::MidiMessage;
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

/// Clean shutdown path for the standalone MIDI output. Called both when
/// the window closes normally and when we receive Ctrl+C / SIGINT.
///
/// Two-step:
/// 1. `clear_all` on the fugue sequencer emits proper note-offs for every
///    note currently held by an active fugue. 80ms gives the 5ms sim loop
///    plenty of buffers to drain.
/// 2. A MIDI panic (CC 123 = All Notes Off, CC 120 = All Sound Off) on
///    every channel — covers notes not tracked by fugues, e.g. piano-
///    keyboard presses from the UI.
fn shutdown_midi(conn: &Arc<Mutex<Option<MidiOutputConnection>>>, instance_id: &str) {
    let _ = droplets::fugue::FugueBridge::clear_all(instance_id);
    thread::sleep(Duration::from_millis(80));

    if let Ok(mut guard) = conn.lock() {
        if let Some(c) = guard.as_mut() {
            for ch in 0u8..16 {
                let status = 0xB0 | ch;
                let _ = c.send(&[status, 123, 0]);
                let _ = c.send(&[status, 120, 0]);
            }
        }
    }
}

fn main() {
    // Initialize logging
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    println!("=== Droplets Standalone ===");
    println!("Starting MCP server, GUI server, and MIDI output...\n");

    // Create params and register with CcBridge
    let params_inst = Arc::new(droplets::params::DropletParams::new());
    let instance_id = "standalone";
    let mut midi_consumer =
        droplets::mcp::CcBridge::register(instance_id, Arc::clone(&params_inst));

    // Register with FugueBridge for fugue sequencing. Unlike the plugin path
    // (where the DAW's audio thread drives FugueSequencer::process), standalone
    // has no audio thread — so we spawn one below that simulates a fixed-tempo
    // always-playing transport and drains the fugue command buffer. Without
    // this, queued fugues sit in the ring buffer forever and never appear in
    // the fugue info cache, which is the "queue fugue disappears" UI bug.
    let (fugue_consumer, fugue_info_handle) =
        droplets::fugue::FugueBridge::register(instance_id, instance_id);

    // Start MCP server (shared singleton) - use standalone port to avoid conflict with plugin
    droplets::mcp::start_server(droplets::mcp::STANDALONE_MCP_PORT);
    println!(
        "MCP server running on port {}",
        droplets::mcp::STANDALONE_MCP_PORT
    );

    // Start GUI server (shared singleton) - use standalone port to avoid conflict with plugin
    droplets::gui::server::start_server(droplets::gui::server::STANDALONE_GUI_PORT);
    println!(
        "GUI server running on http://127.0.0.1:{}",
        droplets::gui::server::STANDALONE_GUI_PORT
    );

    // Setup MIDI output
    let midi_out = match MidiOutput::new("Droplets Standalone") {
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
        droplets::gui::server::STANDALONE_GUI_PORT
    );
    println!("- Click notes on the piano keyboard");
    println!(
        "- Or use MCP tools on port {}",
        droplets::mcp::STANDALONE_MCP_PORT
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
                droplets::mcp::MidiMessage::Note(note) => {
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
                droplets::mcp::MidiMessage::Cc(cc) => {
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
                droplets::mcp::MidiMessage::PerNoteExpression(expr) => {
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
        use droplets::fugue::{FugueSequencer, ProcessedEvent, TransportState};
        use droplets::mcp::{CcMessage, MidiMessage};

        let sample_rate: f64 = 48_000.0;
        let bpm: f64 = 120.0;
        let step_ms: u64 = 5;
        let time_sig_num: u32 = 4;
        let beats_per_step: f64 = bpm / 60.0 * step_ms as f64 / 1000.0;
        let frames_per_step: u32 = (sample_rate * step_ms as f64 / 1000.0) as u32;

        let mut sequencer = FugueSequencer::new(fugue_consumer, sample_rate);
        let mut current_beat: f64 = 0.0;

        loop {
            sequencer.process(true, current_beat, bpm, frames_per_step, time_sig_num);
            for event in sequencer.events() {
                match *event {
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
            if sequencer.take_state_dirty() {
                fugue_info_handle.update(sequencer.list_fugues(current_beat, time_sig_num));
                fugue_info_handle.update_definitions(sequencer.get_definitions());
            }

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
        event_loop::{ControlFlow, EventLoop, EventLoopBuilder},
        window::WindowBuilder,
    };

    // UserEvent::Poll wakes the event loop every ~100ms so we can push cached
    // fugue/transport state to the webview via evaluate_script. The webview
    // can't be moved to another thread (not Send), so any evaluate_script
    // call has to happen on the event loop thread.
    #[derive(Debug, Clone)]
    enum UserEvent {
        Poll,
    }

    let event_loop: EventLoop<UserEvent> = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let window = match WindowBuilder::new()
        .with_title("Droplets - Standalone")
        .with_inner_size(DEFAULT_GUI_SIZE)
        .build(&event_loop)
    {
        Ok(w) => w,
        Err(e) => {
            eprintln!("Failed to create window: {}", e);
            return;
        }
    };

    // Build webview with shared configuration (same as plugin GUI).
    // Drag-out is disabled in standalone for now — populating the state
    // with `None` means drag IPC messages hit the `NoWindow` fallback
    // (reveal-in-file-manager) rather than attempting a native drag
    // against a window handle we don't control cleanly here.
    let drag_state: droplets::gui::drag::DragState =
        Arc::new(Mutex::new(None));
    let config = WebViewConfig::plugin(ipc_sender, instance_id, drag_state);
    let builder = configure_webview(WebViewBuilder::new(), Arc::clone(&params_inst), config);

    let webview = match builder.build(&window) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("Failed to create webview: {}", e);
            return;
        }
    };

    // Polling thread: every 100ms, poke the event loop to push cached fugue
    // and transport state into the webview. Without this, LLM-queued fugues
    // update the FugueBridge info cache but never reach the UI, because the
    // webview's IPC-WebSocket shim only fires when Rust calls evaluate_script
    // from the event loop thread (where the WebView lives — not Send).
    let proxy = event_loop.create_proxy();
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_millis(100));
            if proxy.send_event(UserEvent::Poll).is_err() {
                // Event loop is gone (process shutting down).
                break;
            }
        }
    });

    // Change-detection state for the push handler. Matches DropletGui's
    // plugin-side logic: only push when IDs or waiting flags change so we
    // don't flood the webview with identical messages every tick.
    let mut last_transport: Option<droplets::fugue::TransportState> = None;
    let mut last_fugue_ids: Vec<u64> = Vec::new();
    let mut last_waiting_states: Vec<bool> = Vec::new();

    // Keep handles for the two shutdown paths (window close, signal).
    let conn_out_shutdown = Arc::clone(&conn_out);
    let conn_out_signal = Arc::clone(&conn_out);

    // Signal-handler thread: Ctrl+C / SIGINT runs the same cleanup as the
    // window close, then exits the process. Uses a dedicated tokio runtime
    // so tokio::signal::ctrl_c() works without hijacking the main thread
    // (which belongs to the GUI event loop).
    thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("signal-handler runtime");
        runtime.block_on(async {
            if tokio::signal::ctrl_c().await.is_ok() {
                eprintln!("\nCtrl+C received, cleaning up MIDI state...");
                shutdown_midi(&conn_out_signal, instance_id);
                std::process::exit(0);
            }
        });
    });

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                println!("Window closed, cleaning up MIDI state...");
                shutdown_midi(&conn_out_shutdown, instance_id);
                *control_flow = ControlFlow::Exit;
            }
            Event::UserEvent(UserEvent::Poll) => {
                push_state_to_webview(
                    &webview,
                    instance_id,
                    &mut last_transport,
                    &mut last_fugue_ids,
                    &mut last_waiting_states,
                );
            }
            _ => (),
        }
    });
}

/// Push cached fugue + transport state into the webview via evaluate_script.
///
/// Mirrors `DropletGui::push_fugues` / `push_transport` in the plugin path,
/// inlined here because the standalone doesn't construct a `DropletGui`.
/// Only emits messages when state actually changed — the frontend's render
/// doesn't need the noise, and evaluate_script is not free.
fn push_state_to_webview(
    webview: &wry::WebView,
    instance_id: &str,
    last_transport: &mut Option<droplets::fugue::TransportState>,
    last_fugue_ids: &mut Vec<u64>,
    last_waiting_states: &mut Vec<bool>,
) {
    // Transport: push when beat advances enough to matter, or playing/tempo change.
    if let Ok(transport) = droplets::fugue::FugueBridge::get_transport(instance_id) {
        let should_send = last_transport
            .map(|last| {
                (transport.beat - last.beat).abs() > 0.001
                    || transport.playing != last.playing
                    || transport.tempo != last.tempo
            })
            .unwrap_or(true);
        if should_send {
            let js = format!(
                "window.simplyvst._pushTransport({{beat:{},tempo:{},playing:{},time_sig_numerator:{}}})",
                transport.beat, transport.tempo, transport.playing, transport.time_sig_numerator
            );
            let _ = webview.evaluate_script(&js);
            *last_transport = Some(transport);
        }
    }

    // Fugues: push when the set of IDs or waiting-flags changes.
    if let Ok(infos) = droplets::fugue::FugueBridge::get_fugue_info(instance_id) {
        let current_ids: Vec<u64> = infos.iter().map(|f| f.id).collect();
        let current_waiting: Vec<bool> = infos.iter().map(|f| f.is_waiting).collect();
        if current_ids != *last_fugue_ids || current_waiting != *last_waiting_states {
            let defs = droplets::fugue::FugueBridge::get_definitions(instance_id)
                .unwrap_or_default();
            let infos_json = serde_json::to_string(&infos).unwrap_or_else(|_| "[]".into());
            let defs_json = serde_json::to_string(&defs).unwrap_or_else(|_| "[]".into());
            let js = format!(
                "window.simplyvst._pushFugues({},{})",
                infos_json, defs_json
            );
            let _ = webview.evaluate_script(&js);
            *last_fugue_ids = current_ids;
            *last_waiting_states = current_waiting;
        }
    }
}
