/// Scroll event handling: translates mouse wheel / trackpad deltas into scrollback requests.
use crate::app::Shared;
use crate::messages;
use crate::protocol::encode_message;
use eframe::egui;
use std::cell::RefCell;
use std::rc::Rc;

/// Process scroll input on the terminal grid.
pub fn handle_scroll(ui: &egui::Ui, response: &egui::Response, shared: &Rc<RefCell<Shared>>) {
    let scroll_delta = ui.input(|i| {
        let wheel: f32 = i
            .events
            .iter()
            .filter_map(|e| {
                if let egui::Event::MouseWheel { delta, .. } = e {
                    Some(delta.y)
                } else {
                    None
                }
            })
            .sum();
        if wheel != 0.0 {
            return wheel;
        }
        i.smooth_scroll_delta.y
    });

    if scroll_delta == 0.0 || !response.hovered() {
        return;
    }

    if let Ok(mut s) = shared.try_borrow_mut() {
        // Scale scroll delta to lines.
        // egui MouseWheel delta.y: typically 1.0-3.0 per notch.
        // egui smooth_scroll_delta.y: pixels, typically 30-120 per scroll gesture.
        // We want ~3 lines per mouse wheel notch.
        let lines = if scroll_delta.abs() > 10.0 {
            // Pixel-based (smooth_scroll_delta from trackpad): 1 line per 20px.
            (scroll_delta / 20.0).round() as isize
        } else {
            // Notch-based (MouseWheel): 3 lines per notch.
            (scroll_delta * 3.0).round() as isize
        };
        if lines == 0 {
            return;
        }

        // Don't clamp to scrollback_total — let the server be authoritative.
        // The server returns whatever scrollback it actually has.
        // Use a generous max (100k) to avoid overflow, server will clamp.
        let max_scroll = if s.grid.scrollback_total > 0 {
            s.grid.scrollback_total as isize
        } else {
            100_000 // Before we know the actual count, allow scrolling.
        };
        let new_offset = (s.grid.scroll_offset as isize + lines)
            .max(0)
            .min(max_scroll) as usize;

        if new_offset != s.grid.scroll_offset {
            s.grid.scroll_offset = new_offset;
            if new_offset > 0 && s.connected {
                let req = messages::encode_scrollback_request(0, new_offset as u32);
                s.send_queue.push_back(encode_message(&req));
            } else {
                s.grid.scrollback.clear();
            }
        }
    }
}
