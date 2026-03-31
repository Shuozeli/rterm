/// Keyboard input encoding: egui keys to VT terminal sequences.
use eframe::egui;

/// Encode an egui key press into a VT escape sequence.
/// When `app_cursor_keys` is true (DECCKM mode), arrow/Home/End use SS3 (`\x1bO`)
/// instead of CSI (`\x1b[`).
pub fn encode_vt_key(key: egui::Key, modifiers: &egui::Modifiers, app_cursor_keys: bool) -> Option<Vec<u8>> {
    if modifiers.ctrl {
        let ctrl_byte = match key {
            egui::Key::A => 1,
            egui::Key::B => 2,
            egui::Key::C => 3,
            egui::Key::D => 4,
            egui::Key::E => 5,
            egui::Key::F => 6,
            egui::Key::G => 7,
            egui::Key::H => 8,
            egui::Key::I => 9,
            egui::Key::J => 10,
            egui::Key::K => 11,
            egui::Key::L => 12,
            egui::Key::M => 13,
            egui::Key::N => 14,
            egui::Key::O => 15,
            egui::Key::P => 16,
            egui::Key::Q => 17,
            egui::Key::R => 18,
            egui::Key::S => 19,
            egui::Key::T => 20,
            egui::Key::U => 21,
            egui::Key::V => 22,
            egui::Key::W => 23,
            egui::Key::X => 24,
            egui::Key::Y => 25,
            egui::Key::Z => 26,
            _ => return None,
        };
        return Some(vec![ctrl_byte]);
    }

    // Application cursor keys: arrow keys use SS3 prefix.
    let arrow_prefix: &[u8] = if app_cursor_keys { b"\x1bO" } else { b"\x1b[" };
    match key {
        egui::Key::Enter => Some(b"\r".to_vec()),
        egui::Key::Backspace => Some(vec![0x7f]),
        egui::Key::Tab => Some(b"\t".to_vec()),
        egui::Key::Escape => Some(vec![0x1b]),
        egui::Key::Delete => Some(b"\x1b[3~".to_vec()),
        egui::Key::ArrowUp => Some([arrow_prefix, b"A"].concat()),
        egui::Key::ArrowDown => Some([arrow_prefix, b"B"].concat()),
        egui::Key::ArrowRight => Some([arrow_prefix, b"C"].concat()),
        egui::Key::ArrowLeft => Some([arrow_prefix, b"D"].concat()),
        egui::Key::Home => Some([arrow_prefix, b"H"].concat()),
        egui::Key::End => Some([arrow_prefix, b"F"].concat()),
        egui::Key::PageUp => Some(b"\x1b[5~".to_vec()),
        egui::Key::PageDown => Some(b"\x1b[6~".to_vec()),
        _ => None,
    }
}
