#[allow(unused_imports, dead_code, clippy::all, non_snake_case)]
mod generated;
mod messages;
mod protocol;
mod transport;

use eframe::egui;
use messages::{decode_server_msg, encode_data_in, encode_resize, ServerMsg};
use protocol::{encode_message, RecvBuffer};
use rterm_core::Terminal;
use rterm_gui::{encode_char, encode_key, terminal_grid, GridResult, ScrollState, Selection, TerminalGridConfig};
use rterm_gui::grid::pixel_to_cell;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn main() {
    console_error_panic_hook::set_once();

    wasm_bindgen_futures::spawn_local(async {
        let document = web_sys::window()
            .expect("no window")
            .document()
            .expect("no document");
        let canvas: web_sys::HtmlCanvasElement = document
            .get_element_by_id("rterm-canvas")
            .expect("no canvas")
            .unchecked_into();

        eframe::WebRunner::new()
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(|cc| Ok(Box::new(TerminalApp::new(cc)))),
            )
            .await
            .expect("failed to start eframe");
    });
}

struct Shared {
    terminal: Terminal,
    send_queue: VecDeque<Vec<u8>>,
    connected: bool,
    current_cols: usize,
    current_rows: usize,
    auto_scroll: bool,
    /// Initial size determined from first frame, used for connection.
    initial_size: Option<(usize, usize)>,
    /// Whether the connection has been started.
    connection_started: bool,
}

struct TerminalApp {
    shared: Rc<RefCell<Shared>>,
    config: TerminalGridConfig,
    selection: Selection,
    scroll: ScrollState,
}

impl TerminalApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut terminal = Terminal::new(80, 24);
        terminal.feed(b"\x1b[1;34mrterm\x1b[0m - terminal in the browser\r\n\r\n");
        terminal.feed(b"Connecting to relay server...\r\n");

        let shared = Rc::new(RefCell::new(Shared {
            terminal,
            send_queue: VecDeque::new(),
            connected: false,
            current_cols: 80,
            current_rows: 24,
            auto_scroll: false,
            initial_size: None,
            connection_started: false,
        }));

        // Don't connect yet — wait for first frame to determine correct size.
        Self {
            shared,
            config: TerminalGridConfig::default(),
            selection: Selection::default(),
            scroll: ScrollState::default(),
        }
    }

    fn queue_input(&self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let mut s = self.shared.borrow_mut();
        if s.connected {
            let data_in = encode_data_in(bytes);
            s.send_queue.push_back(encode_message(&data_in));
        }
    }

    fn queue_resize(&self, cols: usize, rows: usize) {
        let Ok(mut s) = self.shared.try_borrow_mut() else {
            return;
        };
        if !s.connected {
            return;
        }
        if cols != s.current_cols || rows != s.current_rows {
            web_sys::console::log_1(
                &format!("[rterm] resize: {}x{} -> {}x{}", s.current_cols, s.current_rows, cols, rows).into(),
            );
            s.current_cols = cols;
            s.current_rows = rows;
            s.terminal.resize(cols, rows);
            let resize = encode_resize(cols as u16, rows as u16);
            s.send_queue.push_back(encode_message(&resize));
        }
    }

    fn copy_selection_to_clipboard(&self) {
        let s = self.shared.borrow();
        let text = self.selection.selected_text(s.terminal.screen());
        if text.is_empty() {
            return;
        }
        drop(s);

        // Use the clipboard API.
        if let Some(window) = web_sys::window() {
            if let Ok(clipboard) = js_sys::Reflect::get(&window.navigator(), &"clipboard".into()) {
                let clipboard: web_sys::Clipboard = clipboard.unchecked_into();
                let _ = clipboard.write_text(&text);
                web_sys::console::log_1(
                    &format!("[rterm] copied {} chars to clipboard", text.len()).into(),
                );
            }
        }
    }
}

impl eframe::App for TerminalApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(self.config.default_bg))
            .show(ctx, |ui| {
                // Brief borrow_mut to check auto_scroll flag.
                {
                    let mut s = self.shared.borrow_mut();
                    if s.auto_scroll && self.scroll.offset > 0 {
                        self.scroll.offset = 0;
                    }
                    s.auto_scroll = false;
                }
                // Immutable borrow for rendering — safe to hold during the frame.
                let s = self.shared.borrow();
                let grid = terminal_grid(ui, s.terminal.screen(), &self.config, &self.selection, &mut self.scroll);
                let app_cursor = s.terminal.modes.application_cursor_keys;
                let cols = s.terminal.screen().cols();
                let rows = s.terminal.screen().rows();
                drop(s);

                // On first frame: determine the correct terminal size and start connection.
                {
                    let mut s = self.shared.borrow_mut();
                    if !s.connection_started && grid.fit_cols >= 10 && grid.fit_rows >= 3 {
                        let init_cols = grid.fit_cols;
                        let init_rows = grid.fit_rows;
                        s.current_cols = init_cols;
                        s.current_rows = init_rows;
                        s.terminal.resize(init_cols, init_rows);
                        s.initial_size = Some((init_cols, init_rows));
                        s.connection_started = true;
                        drop(s);

                        web_sys::console::log_1(
                            &format!("[rterm] initial size: {}x{}, starting connection", init_cols, init_rows).into(),
                        );

                        let shared_clone = Rc::clone(&self.shared);
                        let ctx = ctx.clone();
                        wasm_bindgen_futures::spawn_local(async move {
                            run_connection(shared_clone, ctx).await;
                        });
                    }
                }

                // Dynamic resize: only when connected and size actually changed.
                if grid.fit_cols >= 10 && grid.fit_rows >= 3
                    && (grid.fit_cols != cols || grid.fit_rows != rows)
                {
                    self.queue_resize(grid.fit_cols, grid.fit_rows);
                }

                // Handle mouse for selection.
                let origin = grid.response.rect.min;
                if grid.response.drag_started() {
                    if let Some(pos) = grid.response.interact_pointer_pos() {
                        if let Some((row, col)) = pixel_to_cell(pos, origin, grid.cell_size, cols, rows) {
                            self.selection.anchor = Some((row, col));
                            self.selection.end = Some((row, col));
                            self.selection.active = true;
                        }
                    }
                }
                if grid.response.dragged() && self.selection.active {
                    if let Some(pos) = grid.response.interact_pointer_pos() {
                        if let Some((row, col)) = pixel_to_cell(pos, origin, grid.cell_size, cols, rows) {
                            self.selection.end = Some((row, col));
                        }
                    }
                }
                if grid.response.drag_stopped() && self.selection.active {
                    self.selection.active = false;
                    self.copy_selection_to_clipboard();
                }

                // Clear selection on click (non-drag).
                if grid.response.clicked() {
                    self.selection = Selection::default();
                }

                // Handle keyboard input.
                let events = ui.input(|i| i.events.clone());
                for event in &events {
                    match event {
                        egui::Event::Text(text) => {
                            self.selection = Selection::default();
                            self.scroll.offset = 0; // snap to bottom on typing
                            for ch in text.chars() {
                                self.queue_input(&encode_char(ch));
                            }
                        }
                        egui::Event::Key {
                            key,
                            pressed: true,
                            modifiers,
                            ..
                        } => {
                            // Ctrl+C with selection = copy, not send interrupt
                            if *key == egui::Key::C && modifiers.ctrl && self.selection.range().is_some() {
                                self.copy_selection_to_clipboard();
                                self.selection = Selection::default();
                                continue;
                            }
                            if let Some(bytes) = encode_key(*key, modifiers, app_cursor) {
                                self.selection = Selection::default();
                                self.scroll.offset = 0;
                                self.queue_input(&bytes);
                            }
                        }
                        _ => {}
                    }
                }
            });
    }
}

async fn run_connection(shared: Rc<RefCell<Shared>>, ctx: egui::Context) {
    let location = web_sys::window().and_then(|w| Some(w.location()));
    let hostname = location
        .as_ref()
        .and_then(|l| l.hostname().ok())
        .unwrap_or_else(|| "localhost".to_string());
    let port = location
        .as_ref()
        .and_then(|l| l.port().ok())
        .and_then(|p| if p.is_empty() { None } else { Some(p) })
        .unwrap_or_else(|| "4433".to_string());
    let url = format!("https://{}:{}/wt", hostname, port);

    let cert_hash = get_cert_hash_from_global().or_else(get_cert_hash_from_url);

    web_sys::console::log_1(&format!("[rterm] connecting to {}", url).into());

    let (sender, receiver, _transport) =
        match transport::connect(&url, cert_hash.as_deref()).await {
            Ok(parts) => {
                web_sys::console::log_1(&"[rterm] WebTransport connected!".into());
                parts
            }
            Err(e) => {
                web_sys::console::error_1(
                    &format!("[rterm] connection FAILED: {}", e).into(),
                );
                web_sys::console::error_1(
                    &"[rterm] troubleshooting:".into(),
                );
                web_sys::console::error_1(
                    &"[rterm]   1. Is rterm-relay running?".into(),
                );
                web_sys::console::error_1(
                    &"[rterm]   2. Chrome needs --webtransport-developer-mode".into(),
                );

                let mut s = shared.borrow_mut();
                s.terminal.feed(
                    format!("\x1b[1;31mConnection failed\x1b[0m: {}\r\n\r\n", e).as_bytes(),
                );
                s.terminal.feed(b"\x1b[33mTroubleshooting:\x1b[0m\r\n");
                s.terminal.feed(b"  1. Is rterm-relay running on port 4433?\r\n");
                s.terminal.feed(b"  2. Chrome needs \x1b[1m--webtransport-developer-mode\x1b[0m\r\n");
                s.terminal.feed(b"\r\nRunning in \x1b[1;32mstatic demo\x1b[0m mode.\r\n");
                ctx.request_repaint();
                return;
            }
        };

    let (init_cols, init_rows) = shared.borrow().initial_size.unwrap_or((80, 24));
    web_sys::console::log_1(
        &format!("[rterm] sending initial Resize({}, {})", init_cols, init_rows).into(),
    );
    let resize = encode_resize(init_cols as u16, init_rows as u16);
    if let Err(e) = sender.send(&encode_message(&resize)).await {
        web_sys::console::error_1(&format!("[rterm] send resize failed: {}", e).into());
        return;
    }

    web_sys::console::log_1(&"[rterm] PTY session started, terminal ready".into());
    {
        let mut s = shared.borrow_mut();
        s.terminal.screen_mut().reset();
        s.connected = true;
    }
    ctx.request_repaint();

    // Spawn send loop.
    let shared_send = Rc::clone(&shared);
    let sender = Rc::new(sender);
    let sender_clone = Rc::clone(&sender);
    wasm_bindgen_futures::spawn_local(async move {
        loop {
            let msg = shared_send
                .try_borrow_mut()
                .ok()
                .and_then(|mut s| s.send_queue.pop_front());

            if let Some(msg) = msg {
                if let Err(e) = sender_clone.send(&msg).await {
                    web_sys::console::log_1(&format!("[rterm] send error: {}", e).into());
                    return;
                }
            } else {
                sleep_ms(10).await;
            }
        }
    });

    // Receive loop.
    let mut recv_buf = RecvBuffer::new();
    loop {
        match receiver.recv().await {
            Ok(Some(data)) => {
                recv_buf.push(&data);
                // Wait until we can borrow — the UI thread might be reading.
                let mut s = loop {
                    match shared.try_borrow_mut() {
                        Ok(s) => break s,
                        Err(_) => sleep_ms(1).await,
                    }
                };
                while let Some(msg_bytes) = recv_buf.try_read_message() {
                    match decode_server_msg(&msg_bytes) {
                        Ok(ServerMsg::DataOut(payload)) => {
                            s.terminal.feed(&payload);
                            s.auto_scroll = true;
                            // Only request repaint when NOT in synchronized output mode.
                            // Ink wraps each frame in CSI ?2026 h/l — we should wait
                            // for the end marker before repainting to avoid showing
                            // half-rendered frames.
                            if !s.terminal.is_sync_mode() {
                                ctx.request_repaint();
                            }
                        }
                        Ok(ServerMsg::Exit(code)) => {
                            s.terminal.feed(
                                format!("\r\n\x1b[33mShell exited ({})\x1b[0m\r\n", code)
                                    .as_bytes(),
                            );
                            s.connected = false;
                            ctx.request_repaint();
                            return;
                        }
                        Ok(ServerMsg::Error(msg)) => {
                            s.terminal.feed(
                                format!("\r\n\x1b[31mError: {}\x1b[0m\r\n", msg).as_bytes(),
                            );
                        }
                        Err(e) => {
                            web_sys::console::log_1(&format!("[rterm] decode error: {}", e).into());
                        }
                    }
                }
                // Always repaint after processing messages.
                // The borrow is already dropped by here.
                ctx.request_repaint();
            }
            Ok(None) => {
                shared.borrow_mut().terminal.feed(b"\r\n\x1b[33mConnection closed\x1b[0m\r\n");
                shared.borrow_mut().connected = false;
                ctx.request_repaint();
                return;
            }
            Err(e) => {
                shared.borrow_mut().terminal.feed(
                    format!("\r\n\x1b[31mRecv error: {}\x1b[0m\r\n", e).as_bytes(),
                );
                shared.borrow_mut().connected = false;
                ctx.request_repaint();
                return;
            }
        }
    }
}

async fn sleep_ms(ms: i32) {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        web_sys::window()
            .unwrap()
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms)
            .unwrap();
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}

fn get_cert_hash_from_global() -> Option<Vec<u8>> {
    let window = web_sys::window()?;
    let hash_js = js_sys::Reflect::get(&window, &"__RTERM_CERT_HASH__".into()).ok()?;
    let hash_str = hash_js.as_string()?;
    if hash_str.is_empty() {
        return None;
    }
    web_sys::console::log_1(&format!("[rterm] cert hash from server: {}", hash_str).into());
    let atob_fn: js_sys::Function =
        js_sys::Reflect::get(&window, &"atob".into()).ok()?.unchecked_into();
    let decoded_js = atob_fn.call1(&JsValue::NULL, &hash_str.into()).ok()?;
    let decoded_str: js_sys::JsString = decoded_js.into();
    Some((0..decoded_str.length()).map(|i| decoded_str.char_code_at(i) as u8).collect())
}

fn get_cert_hash_from_url() -> Option<Vec<u8>> {
    let window = web_sys::window()?;
    let search = window.location().search().ok()?;
    if search.is_empty() {
        return None;
    }
    for param in search.trim_start_matches('?').split('&') {
        if let Some(value) = param.strip_prefix("cert=") {
            let decoded = js_sys::decode_uri_component(value).ok()?;
            let hash_str: String = decoded.into();
            web_sys::console::log_1(&format!("[rterm] cert hash from URL: {}", hash_str).into());
            let atob_fn: js_sys::Function =
                js_sys::Reflect::get(&window, &"atob".into()).ok()?.unchecked_into();
            let decoded_js = atob_fn.call1(&JsValue::NULL, &hash_str.into()).ok()?;
            let decoded_str: js_sys::JsString = decoded_js.into();
            return Some((0..decoded_str.length()).map(|i| decoded_str.char_code_at(i) as u8).collect());
        }
    }
    None
}
