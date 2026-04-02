//! Automation helpers: key resolution and command execution logic.

/// Translate a named key to raw PTY bytes, respecting the session's current
/// VT mode (`application_cursor_keys`).
pub fn resolve_key(name: &str, app_cursor: bool) -> Option<Vec<u8>> {
    // Arrow keys differ between normal and application cursor key modes.
    let up = if app_cursor {
        b"\x1bOA".as_ref()
    } else {
        b"\x1b[A".as_ref()
    };
    let down = if app_cursor {
        b"\x1bOB".as_ref()
    } else {
        b"\x1b[B".as_ref()
    };
    let right = if app_cursor {
        b"\x1bOC".as_ref()
    } else {
        b"\x1b[C".as_ref()
    };
    let left = if app_cursor {
        b"\x1bOD".as_ref()
    } else {
        b"\x1b[D".as_ref()
    };

    let bytes: &[u8] = match name {
        "Enter" => b"\r",
        "Escape" | "Esc" => b"\x1b",
        "Tab" => b"\t",
        "Backspace" => b"\x7f",
        "Delete" => b"\x1b[3~",
        "Up" | "ArrowUp" => up,
        "Down" | "ArrowDown" => down,
        "Right" | "ArrowRight" => right,
        "Left" | "ArrowLeft" => left,
        "Home" => b"\x1b[H",
        "End" => b"\x1b[F",
        "PageUp" => b"\x1b[5~",
        "PageDown" => b"\x1b[6~",
        "Ctrl+C" | "C-c" => b"\x03",
        "Ctrl+D" | "C-d" => b"\x04",
        "Ctrl+Z" | "C-z" => b"\x1a",
        "Ctrl+L" | "C-l" => b"\x0c",
        "Ctrl+A" | "C-a" => b"\x01",
        "Ctrl+E" | "C-e" => b"\x05",
        "Ctrl+U" | "C-u" => b"\x15",
        "Ctrl+W" | "C-w" => b"\x17",
        "F1" => b"\x1bOP",
        "F2" => b"\x1bOQ",
        "F3" => b"\x1bOR",
        "F4" => b"\x1bOS",
        "F5" => b"\x1b[15~",
        "F6" => b"\x1b[17~",
        "F7" => b"\x1b[18~",
        "F8" => b"\x1b[19~",
        "F9" => b"\x1b[20~",
        "F10" => b"\x1b[21~",
        "F11" => b"\x1b[23~",
        "F12" => b"\x1b[24~",
        _ => return None,
    };
    Some(bytes.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_key_normal_cursor() {
        assert_eq!(resolve_key("Up", false).unwrap(), b"\x1b[A");
        assert_eq!(resolve_key("Down", false).unwrap(), b"\x1b[B");
        assert_eq!(resolve_key("Right", false).unwrap(), b"\x1b[C");
        assert_eq!(resolve_key("Left", false).unwrap(), b"\x1b[D");
    }

    #[test]
    fn resolve_key_application_cursor() {
        assert_eq!(resolve_key("Up", true).unwrap(), b"\x1bOA");
        assert_eq!(resolve_key("Down", true).unwrap(), b"\x1bOB");
        assert_eq!(resolve_key("Right", true).unwrap(), b"\x1bOC");
        assert_eq!(resolve_key("Left", true).unwrap(), b"\x1bOD");
    }

    #[test]
    fn resolve_key_aliases() {
        assert_eq!(resolve_key("ArrowUp", false).unwrap(), b"\x1b[A");
        assert_eq!(resolve_key("Escape", false).unwrap(), b"\x1b");
        assert_eq!(resolve_key("Esc", false).unwrap(), b"\x1b");
        assert_eq!(resolve_key("Ctrl+C", false).unwrap(), b"\x03");
        assert_eq!(resolve_key("C-c", false).unwrap(), b"\x03");
    }

    #[test]
    fn resolve_key_unknown_returns_none() {
        assert!(resolve_key("Bogus", false).is_none());
        assert!(resolve_key("ctrl+x", false).is_none()); // case-sensitive
    }

    #[test]
    fn resolve_key_special() {
        assert_eq!(resolve_key("Enter", false).unwrap(), b"\r");
        assert_eq!(resolve_key("Tab", false).unwrap(), b"\t");
        assert_eq!(resolve_key("Backspace", false).unwrap(), b"\x7f");
        assert_eq!(resolve_key("Delete", false).unwrap(), b"\x1b[3~");
        assert_eq!(resolve_key("PageUp", false).unwrap(), b"\x1b[5~");
        assert_eq!(resolve_key("PageDown", false).unwrap(), b"\x1b[6~");
        assert_eq!(resolve_key("F1", false).unwrap(), b"\x1bOP");
        assert_eq!(resolve_key("F12", false).unwrap(), b"\x1b[24~");
    }
}
