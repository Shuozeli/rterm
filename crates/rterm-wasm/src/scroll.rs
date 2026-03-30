/// Scroll event handling: translates mouse wheel / trackpad deltas into scrollback requests.
use crate::app::Shared;
use crate::messages;
use crate::protocol::encode_message;
use eframe::egui;
use std::cell::RefCell;
use std::rc::Rc;

/// Process scroll input on the terminal grid. Returns true if a scroll occurred.
pub fn handle_scroll(
    ui: &egui::Ui,
    response: &egui::Response,
    shared: &Rc<RefCell<Shared>>,
) {
    let scroll_delta = ui.input(|i| {
        // Method 1: MouseWheel events.
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
        // Method 2: smooth_scroll_delta (trackpad, touch).
        i.smooth_scroll_delta.y
    });
    if scroll_delta != 0.0 && response.hovered() {
        if let Ok(mut s) = shared.try_borrow_mut() {
            let lines = (scroll_delta / 3.0).round() as isize;
            // Positive delta = scroll up (show older content = increase offset).
            let new_offset = (s.grid.scroll_offset as isize + lines)
                .max(0)
                .min(s.grid.scrollback_total as isize) as usize;
            if new_offset != s.grid.scroll_offset {
                web_sys::console::log_1(
                    &format!(
                        "[scroll] delta={:.1} lines={} offset={}->{} sb_total={}",
                        scroll_delta,
                        lines,
                        s.grid.scroll_offset,
                        new_offset,
                        s.grid.scrollback_total
                    )
                    .into(),
                );
                s.grid.scroll_offset = new_offset;
                if new_offset > 0 && s.connected {
                    // Request exactly `new_offset` lines of scrollback.
                    let req = messages::encode_scrollback_request(0, new_offset as u32);
                    s.send_queue.push_back(encode_message(&req));
                } else {
                    // Back to live view -- clear scrollback display.
                    s.grid.scrollback.clear();
                }
            }
        }
    }
}
