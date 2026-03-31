//! Demo: egui terminal connected to rterm-relay.
//!
//! Usage:
//!   1. Start rterm-relay: `cargo run -p rterm-relay`
//!   2. Run this demo:     `cargo run -p rterm-gui --example demo`
//!
//! If no relay is running, renders a static demo with colored text.

use eframe::egui;
use grpc_client::{Grpc, H3Channel};
use grpc_codec_flatbuffers::FlatBuffersCodec;
use grpc_core::Request;
use rterm_core::Terminal;
use rterm_gui::{TerminalGridConfig, encode_char, encode_key, terminal_grid};
use rterm_proto::{ClientMsg, KeyInput, Resize, ServerMsg};
use std::sync::{Arc, Mutex};
use tokio::runtime::Runtime;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([800.0, 500.0])
            .with_title("rterm demo"),
        ..Default::default()
    };

    eframe::run_native(
        "rterm demo",
        options,
        Box::new(|cc| Ok(Box::new(TerminalApp::new(cc)))),
    )
}

struct TerminalApp {
    terminal: Arc<Mutex<Terminal>>,
    input_tx: Option<mpsc::Sender<ClientMsg>>,
    config: TerminalGridConfig,
    _runtime: Runtime,
}

impl TerminalApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let terminal = Arc::new(Mutex::new(Terminal::new(80, 24)));
        let config = TerminalGridConfig::default();
        let rt = Runtime::new().unwrap();

        // Populate with a static demo if we can't connect.
        {
            let mut t = terminal.lock().unwrap();
            t.feed(b"\x1b[1;34mrterm\x1b[0m - terminal emulator\r\n");
            t.feed(b"Connecting to relay server...\r\n");
        }

        // Try to connect to rterm-relay in the background.
        let term_clone = Arc::clone(&terminal);
        let ctx = cc.egui_ctx.clone();
        let (input_tx, input_rx) = mpsc::channel::<ClientMsg>(64);

        let input_tx_clone = input_tx.clone();
        rt.spawn(async move {
            match try_connect(term_clone, ctx, input_tx_clone, input_rx).await {
                Ok(()) => {}
                Err(e) => {
                    eprintln!("connection error: {}", e);
                }
            }
        });

        Self {
            terminal,
            input_tx: Some(input_tx),
            config,
            _runtime: rt,
        }
    }
}

impl eframe::App for TerminalApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(self.config.default_bg))
            .show(ctx, |ui| {
                let terminal = self.terminal.lock().unwrap();
                let sel = rterm_gui::Selection::default();
                let _grid = terminal_grid(ui, terminal.screen(), &self.config, &sel);

                // Handle keyboard input.
                {
                    let events = ui.input(|i| i.events.clone());
                    for event in &events {
                        match event {
                            egui::Event::Text(text) => {
                                if let Some(tx) = &self.input_tx {
                                    for ch in text.chars() {
                                        let bytes = encode_char(ch);
                                        let _ = tx.try_send(ClientMsg::KeyInput(KeyInput {
                                            data: bytes,
                                        }));
                                    }
                                }
                            }
                            egui::Event::Key {
                                key,
                                pressed: true,
                                modifiers,
                                ..
                            } => {
                                let app_cursor = {
                                    let t = self.terminal.lock().unwrap();
                                    t.modes.application_cursor_keys
                                };
                                if let Some(bytes) = encode_key(*key, modifiers, app_cursor) {
                                    if let Some(tx) = &self.input_tx {
                                        let _ = tx.try_send(ClientMsg::KeyInput(KeyInput {
                                            data: bytes,
                                        }));
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            });

        // Repaint continuously for terminal updates.
        ctx.request_repaint();
    }
}

async fn try_connect(
    terminal: Arc<Mutex<Terminal>>,
    ctx: egui::Context,
    input_tx: mpsc::Sender<ClientMsg>,
    input_rx: mpsc::Receiver<ClientMsg>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Try to connect to rterm-relay on localhost:4433.
    let uri: http::Uri = "https://127.0.0.1:4433".parse()?;

    // Generate a self-signed cert for the client to trust.
    // In production, this would come from config or the server's cert.
    // For now, we trust any cert (skip verification).
    // TODO: proper cert handling.
    let cert_pem = None; // Will fail if server uses self-signed cert without CA.

    let channel = match H3Channel::connect(uri.clone(), cert_pem).await {
        Ok(ch) => ch,
        Err(e) => {
            let mut t = terminal.lock().unwrap();
            t.feed(format!("\x1b[31mFailed to connect: {}\x1b[0m\r\n", e).as_bytes());
            t.feed(b"\r\nRunning in static demo mode.\r\n");
            t.feed(b"\x1b[1;32mThis is green bold text.\x1b[0m\r\n");
            t.feed(b"\x1b[38;5;208mThis is 256-color orange.\x1b[0m\r\n");
            t.feed(b"\x1b[38;2;100;150;255mThis is RGB blue.\x1b[0m\r\n");
            t.feed(
                b"\x1b[4mUnderlined\x1b[0m \x1b[9mStrikethrough\x1b[0m \x1b[7mReversed\x1b[0m\r\n",
            );
            ctx.request_repaint();
            return Err(e.into());
        }
    };

    let mut grpc = Grpc::with_origin(channel, uri);
    let request_stream = tokio_stream::wrappers::ReceiverStream::new(input_rx);
    let codec = FlatBuffersCodec::<ClientMsg, ServerMsg>::default();
    let path: http::uri::PathAndQuery = "/rterm.protocol.TerminalService/Session".parse().unwrap();

    // Send initial resize.
    input_tx
        .send(ClientMsg::Resize(Resize { cols: 80, rows: 24 }))
        .await?;

    let response = grpc
        .streaming(Request::new(request_stream), path, codec)
        .await?;
    let mut stream = response.into_inner();

    {
        let mut t = terminal.lock().unwrap();
        t.screen_mut().reset();
        t.feed(b"\x1b[1;32mConnected to rterm-relay!\x1b[0m\r\n");
    }
    ctx.request_repaint();

    // Read PTY output and feed to terminal.
    while let Some(msg) = stream.next().await {
        match msg {
            Ok(ServerMsg::ScreenUpdate(_d)) => {
                let mut t = terminal.lock().unwrap();
                t.feed(&format!("screen update").into_bytes());
                ctx.request_repaint();
            }
            Ok(ServerMsg::Exit(e)) => {
                let mut t = terminal.lock().unwrap();
                t.feed(
                    format!("\r\n\x1b[33mShell exited with code {}\x1b[0m\r\n", e.code).as_bytes(),
                );
                ctx.request_repaint();
                break;
            }
            Ok(ServerMsg::Error(e)) => {
                let mut t = terminal.lock().unwrap();
                t.feed(format!("\r\n\x1b[31mError: {}\x1b[0m\r\n", e.message).as_bytes());
                ctx.request_repaint();
                break;
            }
            Ok(_) => {} // ScreenSnapshot, ScrollbackData, Bell — ignore in demo.
            Err(status) => {
                let mut t = terminal.lock().unwrap();
                t.feed(format!("\r\n\x1b[31mgRPC error: {}\x1b[0m\r\n", status).as_bytes());
                ctx.request_repaint();
                break;
            }
        }
    }

    Ok(())
}
