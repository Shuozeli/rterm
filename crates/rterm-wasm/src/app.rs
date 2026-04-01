/// Terminal application: TerminalApp struct, shared state, and the eframe::App update loop.
use crate::connection;
use crate::input::encode_vt_key;
use crate::messages::{encode_key_input, encode_mouse_event, encode_paste_input};
use crate::protocol::encode_message;
use crate::render::{paint_grid, DisplayGrid};
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
                        s.grid.resize(fit_cols, fit_rows);
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
                        s.current_cols = fit_cols;
                        s.current_rows = fit_rows;
                        s.grid.resize(fit_cols, fit_rows);
                        if s.connected {
                            let resize = crate::messages::encode_resize(
                                fit_cols as u16,
                                fit_rows as u16,
                            );
                            s.send_queue.push_back(encode_message(&resize));
                        }
                    }
                    ctx.request_repaint();
                }

                // Read terminal modes for input handling.
                let mouse_tracking_mode = self.shared.borrow().grid.mouse_tracking_mode;
                let app_cursor_keys = self.shared.borrow().grid.application_cursor_keys;

                // Mouse: forward to PTY when tracking is on, otherwise use for selection.
                if mouse_tracking_mode > 0 {
                    // Forward mouse events to the PTY.
                    self.handle_mouse_events(&response, cell_size, cols, rows, mouse_tracking_mode);
                } else {
                    // Local text selection.
                    selection::handle_selection(
                        &response,
                        cell_size,
                        cols,
                        rows,
                        &self.shared,
                    );
                }

                // Keyboard input.
                let events = ui.input(|i| i.events.clone());
                for event in &events {
                    match event {
                        egui::Event::Paste(text) => {
                            if !text.is_empty() {
                                self.send_key(text.as_bytes());
                            }
                        }
                        egui::Event::Text(text) => {
                            if let Ok(mut s) = self.shared.try_borrow_mut() {
                                s.grid.selection_start = None;
                                s.grid.selection_end = None;
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
                            if let Ok(mut s) = self.shared.try_borrow_mut() {
                                s.grid.selection_start = None;
                                s.grid.selection_end = None;
                            }

                            if let Some(bytes) = encode_vt_key(*key, modifiers, app_cursor_keys) {
                                self.send_key(&bytes);
                            }
                        }
                        _ => {}
                    }
                }
            });

        ctx.request_repaint();
    }
}

impl TerminalApp {
    /// Forward mouse events to the PTY when mouse tracking is active.
    fn handle_mouse_events(
        &self,
        response: &egui::Response,
        cell_size: egui::Vec2,
        cols: usize,
        rows: usize,
        _tracking_mode: u8,
    ) {
        let origin = response.rect.min;
        let pos_to_cell = |pos: egui::Pos2| -> (u16, u16) {
            let col = ((pos.x - origin.x) / cell_size.x).floor().max(0.0) as u16;
            let row = ((pos.y - origin.y) / cell_size.y).floor().max(0.0) as u16;
            (row.min(rows.saturating_sub(1) as u16), col.min(cols.saturating_sub(1) as u16))
        };

        // Press (button down).
        if response.drag_started() {
            if let Some(pos) = response.interact_pointer_pos() {
                let (row, col) = pos_to_cell(pos);
                let button = 0u8; // left button
                self.send_mouse(row, col, button, 0, 0); // kind=Press=0
            }
        }

        // Drag (button held + move).
        if response.dragged() {
            if let Some(pos) = response.interact_pointer_pos() {
                let (row, col) = pos_to_cell(pos);
                let button = 32u8; // drag modifier
                self.send_mouse(row, col, button, 0, 2); // kind=Drag=2
            }
        }

        // Release.
        if response.drag_stopped() {
            if let Some(pos) = response.interact_pointer_pos() {
                let (row, col) = pos_to_cell(pos);
                let button = 3u8; // release
                self.send_mouse(row, col, button, 0, 1); // kind=Release=1
            }
        }

        // Scroll.
        let scroll_delta = response.ctx.input(|i| i.smooth_scroll_delta.y);
        if scroll_delta != 0.0 {
            if let Some(pos) = response.hover_pos() {
                let (row, col) = pos_to_cell(pos);
                if scroll_delta > 0.0 {
                    self.send_mouse(row, col, 64, 0, 3); // ScrollUp=3
                } else {
                    self.send_mouse(row, col, 65, 0, 4); // ScrollDown=4
                }
            }
        }
    }

    fn send_mouse(&self, row: u16, col: u16, button: u8, modifiers: u8, kind: u8) {
        if let Ok(mut s) = self.shared.try_borrow_mut() {
            if s.connected {
                let msg = encode_mouse_event(row, col, button, modifiers, kind);
                s.send_queue.push_back(encode_message(&msg));
            }
        }
    }
}
