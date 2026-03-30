/// Terminal application: TerminalApp struct, shared state, and the eframe::App update loop.
use crate::connection;
use crate::input::encode_vt_key;
use crate::messages::encode_key_input;
use crate::protocol::encode_message;
use crate::render::{paint_grid, DisplayGrid};
use crate::scroll;
use crate::selection;
use eframe::egui;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

/// Shared mutable state between the app update loop and the async connection task.
pub struct Shared {
    pub grid: DisplayGrid,
    pub send_queue: VecDeque<Vec<u8>>,
    pub connected: bool,
    pub connection_started: bool,
    pub initial_size: Option<(usize, usize)>,
    pub current_cols: usize,
    pub current_rows: usize,
}

pub struct TerminalApp {
    shared: Rc<RefCell<Shared>>,
    font_size: f32,
}

impl TerminalApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let shared = Rc::new(RefCell::new(Shared {
            grid: DisplayGrid::new(80, 24),
            send_queue: VecDeque::new(),
            connected: false,
            connection_started: false,
            initial_size: None,
            current_cols: 80,
            current_rows: 24,
        }));

        Self {
            shared,
            font_size: 14.0,
        }
    }

    fn send_key(&self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        if let Ok(mut s) = self.shared.try_borrow_mut() {
            if s.connected {
                let ki = encode_key_input(bytes);
                s.send_queue.push_back(encode_message(&ki));
            }
        }
    }
}

impl eframe::App for TerminalApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(egui::Color32::BLACK))
            .show(ctx, |ui| {
                let s = self.shared.borrow();
                let (response, cell_size, fit_cols, fit_rows) =
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
                            &format!("[rterm] initial size: {}x{}", fit_cols, fit_rows)
                                .into(),
                        );

                        let shared_clone = Rc::clone(&self.shared);
                        let ctx2 = ctx.clone();
                        wasm_bindgen_futures::spawn_local(async move {
                            connection::run_connection(shared_clone, ctx2).await;
                        });
                    }
                }

                // Dynamic resize.
                if fit_cols >= 10
                    && fit_rows >= 3
                    && (fit_cols != cols || fit_rows != rows)
                {
                    if let Ok(mut s) = self.shared.try_borrow_mut() {
                        if s.connected {
                            s.current_cols = fit_cols;
                            s.current_rows = fit_rows;
                            s.grid = DisplayGrid::new(fit_cols, fit_rows);
                            let resize = crate::messages::encode_resize(
                                fit_cols as u16,
                                fit_rows as u16,
                            );
                            s.send_queue.push_back(encode_message(&resize));
                        }
                    }
                }

                // Scroll handling.
                scroll::handle_scroll(ui, &response, &self.shared);

                // Mouse selection.
                selection::handle_selection(
                    &response,
                    cell_size,
                    cols,
                    rows,
                    &self.shared,
                );

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
                                let encoded = ch.encode_utf8(&mut buf);
                                self.send_key(encoded.as_bytes());
                            }
                        }
                        egui::Event::Key {
                            key,
                            pressed: true,
                            modifiers,
                            ..
                        } => {
                            // Ctrl+C with selection = copy, not interrupt
                            if *key == egui::Key::C && modifiers.ctrl {
                                if let Ok(s) = self.shared.try_borrow() {
                                    if s.grid.selection_start.is_some() {
                                        let text = s.grid.selected_text();
                                        if !text.is_empty() {
                                            selection::copy_to_clipboard(&text);
                                        }
                                        drop(s);
                                        if let Ok(mut s) =
                                            self.shared.try_borrow_mut()
                                        {
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
