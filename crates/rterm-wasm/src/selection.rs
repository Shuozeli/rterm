/// Text selection state management and clipboard copy.
use crate::app::Shared;
use eframe::egui;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;

/// Handle mouse-based text selection (drag start, drag, drag stop, click).
pub fn handle_selection(
    response: &egui::Response,
    cell_size: egui::Vec2,
    cols: usize,
    rows: usize,
    shared: &Rc<RefCell<Shared>>,
) {
    let origin = response.rect.min;

    if response.drag_started() {
        if let Some(pos) = response.interact_pointer_pos() {
            let col = ((pos.x - origin.x) / cell_size.x) as usize;
            let row = ((pos.y - origin.y) / cell_size.y) as usize;
            if let Ok(mut s) = shared.try_borrow_mut() {
                s.grid.selection_start = Some((row.min(rows - 1), col.min(cols - 1)));
                s.grid.selection_end = Some((row.min(rows - 1), col.min(cols - 1)));
            }
        }
    }
    if response.dragged() {
        if let Some(pos) = response.interact_pointer_pos() {
            let col = ((pos.x - origin.x) / cell_size.x) as usize;
            let row = ((pos.y - origin.y) / cell_size.y) as usize;
            if let Ok(mut s) = shared.try_borrow_mut() {
                s.grid.selection_end = Some((row.min(rows - 1), col.min(cols - 1)));
            }
        }
    }
    if response.drag_stopped() {
        // Copy to clipboard.
        if let Ok(s) = shared.try_borrow() {
            let text = s.grid.selected_text();
            if !text.is_empty() {
                copy_to_clipboard(&text);
            }
        }
    }
    if response.clicked() {
        if let Ok(mut s) = shared.try_borrow_mut() {
            s.grid.selection_start = None;
            s.grid.selection_end = None;
            s.grid.scroll_offset = 0; // click snaps to bottom
        }
    }
}

/// Copy text to the system clipboard via the Web Clipboard API.
pub fn copy_to_clipboard(text: &str) {
    if let Some(window) = web_sys::window() {
        if let Ok(clipboard) =
            js_sys::Reflect::get(&window.navigator(), &"clipboard".into())
        {
            let cb: web_sys::Clipboard = clipboard.unchecked_into();
            let _ = cb.write_text(text);
        }
    }
}
