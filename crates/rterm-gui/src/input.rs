use egui::Key;

/// Encode an egui key event into VT escape sequence bytes.
///
/// Returns None for keys that don't produce terminal input.
pub fn encode_key(key: Key, modifiers: &egui::Modifiers, application_cursor: bool) -> Option<Vec<u8>> {
    // Ctrl+key combinations.
    if modifiers.ctrl {
        if let Some(ch) = key_to_char(key) {
            let ctrl_byte = match ch {
                'a'..='z' => (ch as u8) - b'a' + 1,
                '@' => 0,
                '[' => 27,
                '\\' => 28,
                ']' => 29,
                '^' => 30,
                '_' => 31,
                _ => return None,
            };
            return Some(vec![ctrl_byte]);
        }
    }

    match key {
        Key::Enter => Some(b"\r".to_vec()),
        Key::Backspace => Some(vec![0x7f]),
        Key::Tab => Some(b"\t".to_vec()),
        Key::Escape => Some(vec![0x1b]),
        Key::Delete => Some(b"\x1b[3~".to_vec()),

        // Arrow keys.
        Key::ArrowUp => Some(arrow_key(b'A', application_cursor)),
        Key::ArrowDown => Some(arrow_key(b'B', application_cursor)),
        Key::ArrowRight => Some(arrow_key(b'C', application_cursor)),
        Key::ArrowLeft => Some(arrow_key(b'D', application_cursor)),

        Key::Home => Some(b"\x1b[H".to_vec()),
        Key::End => Some(b"\x1b[F".to_vec()),
        Key::PageUp => Some(b"\x1b[5~".to_vec()),
        Key::PageDown => Some(b"\x1b[6~".to_vec()),
        Key::Insert => Some(b"\x1b[2~".to_vec()),

        // Function keys.
        Key::F1 => Some(b"\x1bOP".to_vec()),
        Key::F2 => Some(b"\x1bOQ".to_vec()),
        Key::F3 => Some(b"\x1bOR".to_vec()),
        Key::F4 => Some(b"\x1bOS".to_vec()),
        Key::F5 => Some(b"\x1b[15~".to_vec()),
        Key::F6 => Some(b"\x1b[17~".to_vec()),
        Key::F7 => Some(b"\x1b[18~".to_vec()),
        Key::F8 => Some(b"\x1b[19~".to_vec()),
        Key::F9 => Some(b"\x1b[20~".to_vec()),
        Key::F10 => Some(b"\x1b[21~".to_vec()),
        Key::F11 => Some(b"\x1b[23~".to_vec()),
        Key::F12 => Some(b"\x1b[24~".to_vec()),

        _ => None,
    }
}

/// Encode a text character for terminal input.
pub fn encode_char(ch: char) -> Vec<u8> {
    let mut buf = [0u8; 4];
    let s = ch.encode_utf8(&mut buf);
    s.as_bytes().to_vec()
}

fn arrow_key(ch: u8, application_cursor: bool) -> Vec<u8> {
    if application_cursor {
        vec![0x1b, b'O', ch] // SS3 sequence
    } else {
        vec![0x1b, b'[', ch] // CSI sequence
    }
}

fn key_to_char(key: Key) -> Option<char> {
    match key {
        Key::A => Some('a'),
        Key::B => Some('b'),
        Key::C => Some('c'),
        Key::D => Some('d'),
        Key::E => Some('e'),
        Key::F => Some('f'),
        Key::G => Some('g'),
        Key::H => Some('h'),
        Key::I => Some('i'),
        Key::J => Some('j'),
        Key::K => Some('k'),
        Key::L => Some('l'),
        Key::M => Some('m'),
        Key::N => Some('n'),
        Key::O => Some('o'),
        Key::P => Some('p'),
        Key::Q => Some('q'),
        Key::R => Some('r'),
        Key::S => Some('s'),
        Key::T => Some('t'),
        Key::U => Some('u'),
        Key::V => Some('v'),
        Key::W => Some('w'),
        Key::X => Some('x'),
        Key::Y => Some('y'),
        Key::Z => Some('z'),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enter_key() {
        let mods = egui::Modifiers::NONE;
        assert_eq!(encode_key(Key::Enter, &mods, false), Some(b"\r".to_vec()));
    }

    #[test]
    fn arrow_keys_normal_mode() {
        let mods = egui::Modifiers::NONE;
        assert_eq!(encode_key(Key::ArrowUp, &mods, false), Some(b"\x1b[A".to_vec()));
        assert_eq!(encode_key(Key::ArrowDown, &mods, false), Some(b"\x1b[B".to_vec()));
    }

    #[test]
    fn arrow_keys_application_mode() {
        let mods = egui::Modifiers::NONE;
        assert_eq!(encode_key(Key::ArrowUp, &mods, true), Some(b"\x1bOA".to_vec()));
    }

    #[test]
    fn ctrl_c() {
        let mods = egui::Modifiers { ctrl: true, ..Default::default() };
        assert_eq!(encode_key(Key::C, &mods, false), Some(vec![3])); // ETX
    }

    #[test]
    fn ctrl_d() {
        let mods = egui::Modifiers { ctrl: true, ..Default::default() };
        assert_eq!(encode_key(Key::D, &mods, false), Some(vec![4])); // EOT
    }

    #[test]
    fn function_keys() {
        let mods = egui::Modifiers::NONE;
        assert_eq!(encode_key(Key::F1, &mods, false), Some(b"\x1bOP".to_vec()));
        assert_eq!(encode_key(Key::F12, &mods, false), Some(b"\x1b[24~".to_vec()));
    }

    #[test]
    fn encode_ascii_char() {
        assert_eq!(encode_char('A'), b"A".to_vec());
    }

    #[test]
    fn encode_unicode_char() {
        let bytes = encode_char('\u{4e16}'); // CJK character
        assert_eq!(bytes, "\u{4e16}".as_bytes().to_vec());
    }

    #[test]
    fn delete_key() {
        let mods = egui::Modifiers::NONE;
        assert_eq!(encode_key(Key::Delete, &mods, false), Some(b"\x1b[3~".to_vec()));
    }

    #[test]
    fn backspace_is_del() {
        let mods = egui::Modifiers::NONE;
        assert_eq!(encode_key(Key::Backspace, &mods, false), Some(vec![0x7f]));
    }
}
