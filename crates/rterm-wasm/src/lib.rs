#[allow(unused_imports, dead_code, clippy::all, non_snake_case)]
mod generated;
mod messages;
mod protocol;
mod render;
mod transport;

use eframe::egui;
use messages::{decode_server_msg, encode_key_input, encode_resize, ServerMsg};
use protocol::{encode_message, RecvBuffer};
use render::{paint_grid, DisplayGrid};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn main() {
    console_error_panic_hook::set_once();

    wasm_bindgen_futures::spawn_local(async {
        let document = web_sys::window().expect("no window").document().expect("no document");
        let canvas: web_sys::HtmlCanvasElement = document
            .get_element_by_id("rterm-canvas")
            .expect("no canvas")
            .unchecked_into();

        eframe::WebRunner::new()
            .start(canvas, eframe::WebOptions::default(),
                Box::new(|cc| Ok(Box::new(TerminalApp::new(cc)))))
            .await
            .expect("failed to start eframe");
    });
}

struct Shared {
    grid: DisplayGrid,
    send_queue: VecDeque<Vec<u8>>,
    connected: bool,
    connection_started: bool,
    initial_size: Option<(usize, usize)>,
    current_cols: usize,
    current_rows: usize,
}

struct TerminalApp {
    shared: Rc<RefCell<Shared>>,
    font_size: f32,
}

impl TerminalApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let shared = Rc::new(RefCell::new(Shared {
            grid: DisplayGrid::new(80, 24),
            send_queue: VecDeque::new(),
            connected: false,
            connection_started: false,
            initial_size: None,
            current_cols: 80,
            current_rows: 24,
        }));

        Self { shared, font_size: 14.0 }
    }
}

impl eframe::App for TerminalApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(egui::Color32::BLACK))
            .show(ctx, |ui| {
                let s = self.shared.borrow();
                let (response, _cell_size, fit_cols, fit_rows) =
                    paint_grid(ui, &s.grid, self.font_size);
                let cols = s.current_cols;
                let rows = s.current_rows;
                drop(s);

                // Start connection on first frame.
                {
                    let mut s = self.shared.borrow_mut();
                    if !s.connection_started && fit_cols >= 10 && fit_rows >= 3 {
                        s.current_cols = fit_cols;
                        s.current_rows = fit_rows;
                        s.grid = DisplayGrid::new(fit_cols, fit_rows);
                        s.initial_size = Some((fit_cols, fit_rows));
                        s.connection_started = true;
                        drop(s);

                        web_sys::console::log_1(
                            &format!("[rterm] initial size: {}x{}", fit_cols, fit_rows).into(),
                        );

                        let shared_clone = Rc::clone(&self.shared);
                        let ctx2 = ctx.clone();
                        wasm_bindgen_futures::spawn_local(async move {
                            run_connection(shared_clone, ctx2).await;
                        });
                    }
                }

                // Dynamic resize.
                if fit_cols >= 10 && fit_rows >= 3 && (fit_cols != cols || fit_rows != rows) {
                    if let Ok(mut s) = self.shared.try_borrow_mut() {
                        if s.connected {
                            s.current_cols = fit_cols;
                            s.current_rows = fit_rows;
                            s.grid = DisplayGrid::new(fit_cols, fit_rows);
                            let resize = encode_resize(fit_cols as u16, fit_rows as u16);
                            s.send_queue.push_back(encode_message(&resize));
                        }
                    }
                }

                // Mouse wheel scrolling.
                let scroll_delta = ui.input(|i| {
                    i.events.iter().filter_map(|e| {
                        if let egui::Event::MouseWheel { delta, .. } = e {
                            Some(delta.y)
                        } else { None }
                    }).sum::<f32>()
                });
                if scroll_delta != 0.0 && response.hovered() {
                    if let Ok(mut s) = self.shared.try_borrow_mut() {
                        let lines = (scroll_delta / 3.0).round() as isize;
                        let new_offset = (s.grid.scroll_offset as isize - lines)
                            .max(0).min(s.grid.scrollback_total as isize) as usize;
                        if new_offset != s.grid.scroll_offset {
                            s.grid.scroll_offset = new_offset;
                            // Request scrollback from server.
                            if new_offset > 0 && s.connected {
                                let req = messages::encode_scrollback_request(
                                    new_offset as u32, rows as u32);
                                s.send_queue.push_back(encode_message(&req));
                            }
                        }
                    }
                }

                // Mouse selection.
                let origin = response.rect.min;
                if response.drag_started() {
                    if let Some(pos) = response.interact_pointer_pos() {
                        let col = ((pos.x - origin.x) / _cell_size.x) as usize;
                        let row = ((pos.y - origin.y) / _cell_size.y) as usize;
                        if let Ok(mut s) = self.shared.try_borrow_mut() {
                            s.grid.selection_start = Some((row.min(rows - 1), col.min(cols - 1)));
                            s.grid.selection_end = Some((row.min(rows - 1), col.min(cols - 1)));
                        }
                    }
                }
                if response.dragged() {
                    if let Some(pos) = response.interact_pointer_pos() {
                        let col = ((pos.x - origin.x) / _cell_size.x) as usize;
                        let row = ((pos.y - origin.y) / _cell_size.y) as usize;
                        if let Ok(mut s) = self.shared.try_borrow_mut() {
                            s.grid.selection_end = Some((row.min(rows - 1), col.min(cols - 1)));
                        }
                    }
                }
                if response.drag_stopped() {
                    // Copy to clipboard.
                    if let Ok(s) = self.shared.try_borrow() {
                        let text = s.grid.selected_text();
                        if !text.is_empty() {
                            if let Some(window) = web_sys::window() {
                                if let Ok(clipboard) = js_sys::Reflect::get(
                                    &window.navigator(), &"clipboard".into()) {
                                    let cb: web_sys::Clipboard = clipboard.unchecked_into();
                                    let _ = cb.write_text(&text);
                                }
                            }
                        }
                    }
                }
                if response.clicked() {
                    if let Ok(mut s) = self.shared.try_borrow_mut() {
                        s.grid.selection_start = None;
                        s.grid.selection_end = None;
                        s.grid.scroll_offset = 0; // click snaps to bottom
                    }
                }

                // Keyboard input.
                let events = ui.input(|i| i.events.clone());
                for event in &events {
                    match event {
                        egui::Event::Text(text) => {
                            if let Ok(mut s) = self.shared.try_borrow_mut() {
                                s.grid.selection_start = None;
                                s.grid.selection_end = None;
                                s.grid.scroll_offset = 0;
                            }
                            for ch in text.chars() {
                                let mut buf = [0u8; 4];
                                let s = ch.encode_utf8(&mut buf);
                                self.send_key(s.as_bytes());
                            }
                        }
                        egui::Event::Key { key, pressed: true, modifiers, .. } => {
                            // Ctrl+C with selection = copy, not interrupt
                            if *key == egui::Key::C && modifiers.ctrl {
                                if let Ok(s) = self.shared.try_borrow() {
                                    if s.grid.selection_start.is_some() {
                                        let text = s.grid.selected_text();
                                        if !text.is_empty() {
                                            if let Some(window) = web_sys::window() {
                                                if let Ok(cb) = js_sys::Reflect::get(
                                                    &window.navigator(), &"clipboard".into()) {
                                                    let cb: web_sys::Clipboard = cb.unchecked_into();
                                                    let _ = cb.write_text(&text);
                                                }
                                            }
                                        }
                                        drop(s);
                                        if let Ok(mut s) = self.shared.try_borrow_mut() {
                                            s.grid.selection_start = None;
                                            s.grid.selection_end = None;
                                        }
                                        continue;
                                    }
                                }
                            }
                            if let Some(bytes) = encode_vt_key(*key, modifiers) {
                                self.send_key(&bytes);
                            }
                        }
                        _ => {}
                    }
                }
            });
    }
}

impl TerminalApp {
    fn send_key(&self, bytes: &[u8]) {
        if bytes.is_empty() { return; }
        if let Ok(mut s) = self.shared.try_borrow_mut() {
            if s.connected {
                let ki = encode_key_input(bytes);
                s.send_queue.push_back(encode_message(&ki));
            }
        }
    }
}

/// Encode egui key to VT sequence.
fn encode_vt_key(key: egui::Key, modifiers: &egui::Modifiers) -> Option<Vec<u8>> {
    if modifiers.ctrl {
        let ctrl_byte = match key {
            egui::Key::A => 1, egui::Key::B => 2, egui::Key::C => 3,
            egui::Key::D => 4, egui::Key::E => 5, egui::Key::F => 6,
            egui::Key::G => 7, egui::Key::H => 8, egui::Key::I => 9,
            egui::Key::J => 10, egui::Key::K => 11, egui::Key::L => 12,
            egui::Key::M => 13, egui::Key::N => 14, egui::Key::O => 15,
            egui::Key::P => 16, egui::Key::Q => 17, egui::Key::R => 18,
            egui::Key::S => 19, egui::Key::T => 20, egui::Key::U => 21,
            egui::Key::V => 22, egui::Key::W => 23, egui::Key::X => 24,
            egui::Key::Y => 25, egui::Key::Z => 26,
            _ => return None,
        };
        return Some(vec![ctrl_byte]);
    }
    match key {
        egui::Key::Enter => Some(b"\r".to_vec()),
        egui::Key::Backspace => Some(vec![0x7f]),
        egui::Key::Tab => Some(b"\t".to_vec()),
        egui::Key::Escape => Some(vec![0x1b]),
        egui::Key::Delete => Some(b"\x1b[3~".to_vec()),
        egui::Key::ArrowUp => Some(b"\x1b[A".to_vec()),
        egui::Key::ArrowDown => Some(b"\x1b[B".to_vec()),
        egui::Key::ArrowRight => Some(b"\x1b[C".to_vec()),
        egui::Key::ArrowLeft => Some(b"\x1b[D".to_vec()),
        egui::Key::Home => Some(b"\x1b[H".to_vec()),
        egui::Key::End => Some(b"\x1b[F".to_vec()),
        egui::Key::PageUp => Some(b"\x1b[5~".to_vec()),
        egui::Key::PageDown => Some(b"\x1b[6~".to_vec()),
        _ => None,
    }
}

async fn run_connection(shared: Rc<RefCell<Shared>>, ctx: egui::Context) {
    let location = web_sys::window().and_then(|w| Some(w.location()));
    let hostname = location.as_ref().and_then(|l| l.hostname().ok())
        .unwrap_or_else(|| "localhost".to_string());
    let port = location.as_ref().and_then(|l| l.port().ok())
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
                web_sys::console::error_1(&format!("[rterm] connection FAILED: {}", e).into());
                return;
            }
        };

    let (init_cols, init_rows) = shared.borrow().initial_size.unwrap_or((80, 24));
    web_sys::console::log_1(&format!("[rterm] sending Resize({}, {})", init_cols, init_rows).into());
    let resize = encode_resize(init_cols as u16, init_rows as u16);
    if let Err(e) = sender.send(&encode_message(&resize)).await {
        web_sys::console::error_1(&format!("[rterm] send failed: {}", e).into());
        return;
    }

    shared.borrow_mut().connected = true;
    web_sys::console::log_1(&"[rterm] session started".into());
    ctx.request_repaint();

    // Send loop.
    let shared_send = Rc::clone(&shared);
    let sender = Rc::new(sender);
    let sender_clone = Rc::clone(&sender);
    wasm_bindgen_futures::spawn_local(async move {
        loop {
            let msg = shared_send.try_borrow_mut().ok().and_then(|mut s| s.send_queue.pop_front());
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
                let mut s = loop {
                    match shared.try_borrow_mut() {
                        Ok(s) => break s,
                        Err(_) => sleep_ms(1).await,
                    }
                };
                while let Some(msg_bytes) = recv_buf.try_read_message() {
                    match decode_server_msg(&msg_bytes) {
                        Ok(ServerMsg::ScreenSnapshot(sd)) => { s.grid.apply_snapshot(&sd); }
                        Ok(ServerMsg::ScreenUpdate(sd)) => { s.grid.apply_update(&sd); }
                        Ok(ServerMsg::ScrollbackData(sd)) => {
                            s.grid.apply_scrollback(&sd.lines, sd.offset, sd.total);
                        }
                        Ok(ServerMsg::Exit(_)) => {
                            s.connected = false;
                            ctx.request_repaint();
                            return;
                        }
                        Ok(ServerMsg::Error(msg)) => {
                            web_sys::console::error_1(&format!("[rterm] error: {}", msg).into());
                        }
                        Ok(ServerMsg::Bell) => {} // TODO: visual/audio bell
                        Err(e) => {
                            web_sys::console::log_1(&format!("[rterm] decode error: {}", e).into());
                        }
                    }
                }
                ctx.request_repaint();
            }
            Ok(None) => {
                shared.borrow_mut().connected = false;
                ctx.request_repaint();
                return;
            }
            Err(e) => {
                web_sys::console::error_1(&format!("[rterm] recv error: {}", e).into());
                shared.borrow_mut().connected = false;
                ctx.request_repaint();
                return;
            }
        }
    }
}

async fn sleep_ms(ms: i32) {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        web_sys::window().unwrap()
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms).unwrap();
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}

fn get_cert_hash_from_global() -> Option<Vec<u8>> {
    let window = web_sys::window()?;
    let hash_js = js_sys::Reflect::get(&window, &"__RTERM_CERT_HASH__".into()).ok()?;
    let hash_str = hash_js.as_string()?;
    if hash_str.is_empty() { return None; }
    web_sys::console::log_1(&format!("[rterm] cert hash from server: {}", hash_str).into());
    let atob_fn: js_sys::Function = js_sys::Reflect::get(&window, &"atob".into()).ok()?.unchecked_into();
    let decoded_js = atob_fn.call1(&JsValue::NULL, &hash_str.into()).ok()?;
    let decoded_str: js_sys::JsString = decoded_js.into();
    Some((0..decoded_str.length()).map(|i| decoded_str.char_code_at(i) as u8).collect())
}

fn get_cert_hash_from_url() -> Option<Vec<u8>> {
    let window = web_sys::window()?;
    let search = window.location().search().ok()?;
    if search.is_empty() { return None; }
    for param in search.trim_start_matches('?').split('&') {
        if let Some(value) = param.strip_prefix("cert=") {
            let decoded = js_sys::decode_uri_component(value).ok()?;
            let hash_str: String = decoded.into();
            let atob_fn: js_sys::Function = js_sys::Reflect::get(&window, &"atob".into()).ok()?.unchecked_into();
            let decoded_js = atob_fn.call1(&JsValue::NULL, &hash_str.into()).ok()?;
            let decoded_str: js_sys::JsString = decoded_js.into();
            return Some((0..decoded_str.length()).map(|i| decoded_str.char_code_at(i) as u8).collect());
        }
    }
    None
}
