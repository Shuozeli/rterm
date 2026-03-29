use crate::buffer::ScreenBuffer;
use crate::cell::CellAttributes;
use crate::color::Color;

/// Terminal modes that affect behavior.
#[derive(Debug, Clone)]
pub struct TerminalModes {
    /// DECAWM: Auto-wrap mode (cursor wraps at end of line).
    pub autowrap: bool,
    /// DECCKM: Application cursor keys mode.
    pub application_cursor_keys: bool,
    /// Insert mode (IRM): insert chars shift existing chars right.
    pub insert: bool,
    /// Origin mode (DECOM): cursor addressing relative to scroll region.
    pub origin: bool,
}

impl Default for TerminalModes {
    fn default() -> Self {
        Self {
            autowrap: true,
            application_cursor_keys: false,
            insert: false,
            origin: false,
        }
    }
}

/// Terminal emulator: wraps a ScreenBuffer and handles VT escape sequences.
pub struct Terminal {
    /// The primary screen buffer.
    primary: ScreenBuffer,
    /// The alternate screen buffer (used by fullscreen apps like vim).
    alternate: ScreenBuffer,
    /// Whether the alternate screen is active.
    alt_active: bool,
    /// Terminal modes.
    pub modes: TerminalModes,
    /// Saved cursor position (for DECSC/DECRC).
    saved_cursor: (usize, usize),
    /// Response bytes to be read by the PTY (e.g., DSR responses).
    response_buf: Vec<u8>,
    /// Persistent VT parser (retains state between feed() calls).
    parser: vte::Parser,
    /// Synchronized output mode (CSI ?2026 h/l).
    /// When true, screen updates are being batched — the renderer should
    /// wait until this goes false before repainting.
    sync_mode: bool,
}

impl Terminal {
    pub fn new(cols: usize, rows: usize) -> Self {
        Self {
            primary: ScreenBuffer::new(cols, rows),
            alternate: ScreenBuffer::new(cols, rows),
            alt_active: false,
            modes: TerminalModes::default(),
            saved_cursor: (0, 0),
            response_buf: Vec::new(),
            parser: vte::Parser::new(),
            sync_mode: false,
        }
    }

    /// Get a reference to the active screen buffer.
    pub fn screen(&self) -> &ScreenBuffer {
        if self.alt_active {
            &self.alternate
        } else {
            &self.primary
        }
    }

    /// Get a mutable reference to the active screen buffer.
    pub fn screen_mut(&mut self) -> &mut ScreenBuffer {
        if self.alt_active {
            &mut self.alternate
        } else {
            &mut self.primary
        }
    }

    /// Drain any pending response bytes (for DSR, DA, etc.).
    pub fn take_response(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.response_buf)
    }

    /// Whether synchronized output mode is active.
    /// When true, the renderer should NOT repaint — wait for it to go false.
    pub fn is_sync_mode(&self) -> bool {
        self.sync_mode
    }

    /// Resize both primary and alternate screen buffers.
    pub fn resize(&mut self, cols: usize, rows: usize) {
        self.primary.resize(cols, rows);
        self.alternate.resize(cols, rows);
    }

    /// Feed raw bytes through the VT parser.
    /// The parser retains state between calls, so split escape sequences
    /// across multiple feed() calls are handled correctly.
    pub fn feed(&mut self, bytes: &[u8]) {
        // TODO(refactor): vte::Parser::advance takes &mut self for both parser and performer.
        // We need to temporarily take the parser out of self to satisfy the borrow checker.
        let mut parser = std::mem::replace(&mut self.parser, vte::Parser::new());
        parser.advance(self, bytes);
        self.parser = parser;
    }

    /// Switch to alternate screen buffer.
    fn enter_alternate_screen(&mut self) {
        if !self.alt_active {
            self.alt_active = true;
            self.alternate.reset();
        }
    }

    /// Switch back to primary screen buffer.
    fn leave_alternate_screen(&mut self) {
        self.alt_active = false;
    }

    /// Save cursor position (DECSC).
    fn save_cursor(&mut self) {
        let s = self.screen();
        self.saved_cursor = (s.cursor.row, s.cursor.col);
    }

    /// Restore cursor position (DECRC).
    fn restore_cursor(&mut self) {
        let (row, col) = self.saved_cursor;
        let s = self.screen_mut();
        s.cursor.row = row;
        s.cursor.col = col;
    }

    /// Handle SGR (Select Graphic Rendition) parameters.
    fn handle_sgr(&mut self, params: &vte::Params) {
        // Collect params to avoid borrow conflicts with self.
        let param_list: Vec<u16> = params.iter().flat_map(|p| p.iter().copied()).collect();
        let mut i = 0;

        while i < param_list.len() {
            let code = param_list[i];
            i += 1;

            match code {
                0 => {
                    let s = self.screen_mut();
                    s.pen.fg = Color::Default;
                    s.pen.bg = Color::Default;
                    s.pen.attrs = CellAttributes::NORMAL;
                }
                1 => self.screen_mut().pen.attrs.bold = true,
                2 => self.screen_mut().pen.attrs.dim = true,
                3 => self.screen_mut().pen.attrs.italic = true,
                4 => self.screen_mut().pen.attrs.underline = true,
                7 => self.screen_mut().pen.attrs.reverse = true,
                8 => self.screen_mut().pen.attrs.hidden = true,
                9 => self.screen_mut().pen.attrs.strikethrough = true,
                21 => self.screen_mut().pen.attrs.bold = false,
                22 => {
                    let s = self.screen_mut();
                    s.pen.attrs.bold = false;
                    s.pen.attrs.dim = false;
                }
                23 => self.screen_mut().pen.attrs.italic = false,
                24 => self.screen_mut().pen.attrs.underline = false,
                27 => self.screen_mut().pen.attrs.reverse = false,
                28 => self.screen_mut().pen.attrs.hidden = false,
                29 => self.screen_mut().pen.attrs.strikethrough = false,

                30..=37 => self.screen_mut().pen.fg = Color::Indexed((code - 30) as u8),
                38 => {
                    if let Some((color, consumed)) = Self::parse_extended_color(&param_list[i..]) {
                        self.screen_mut().pen.fg = color;
                        i += consumed;
                    }
                }
                39 => self.screen_mut().pen.fg = Color::Default,

                40..=47 => self.screen_mut().pen.bg = Color::Indexed((code - 40) as u8),
                48 => {
                    if let Some((color, consumed)) = Self::parse_extended_color(&param_list[i..]) {
                        self.screen_mut().pen.bg = color;
                        i += consumed;
                    }
                }
                49 => self.screen_mut().pen.bg = Color::Default,

                90..=97 => self.screen_mut().pen.fg = Color::Indexed((code - 90 + 8) as u8),
                100..=107 => self.screen_mut().pen.bg = Color::Indexed((code - 100 + 8) as u8),

                _ => {}
            }
        }
    }

    /// Parse extended color (256-color or RGB) from remaining SGR param slice.
    /// Returns (Color, number of params consumed).
    fn parse_extended_color(remaining: &[u16]) -> Option<(Color, usize)> {
        if remaining.is_empty() {
            return None;
        }
        match remaining[0] {
            5 if remaining.len() >= 2 => Some((Color::Indexed(remaining[1] as u8), 2)),
            2 if remaining.len() >= 4 => Some((
                Color::Rgb(remaining[1] as u8, remaining[2] as u8, remaining[3] as u8),
                4,
            )),
            _ => None,
        }
    }

    /// Handle DEC private mode set (DECSET) / reset (DECRST).
    fn handle_dec_mode(&mut self, params: &vte::Params, set: bool) {
        for param in params.iter() {
            match param[0] {
                1 => self.modes.application_cursor_keys = set, // DECCKM
                4 => self.modes.insert = set,                  // IRM
                6 => self.modes.origin = set,                  // DECOM
                7 => self.modes.autowrap = set,                // DECAWM
                25 => self.screen_mut().cursor.visible = set,  // DECTCEM
                1049 => {
                    // Alternate screen buffer with save/restore cursor.
                    if set {
                        self.save_cursor();
                        self.enter_alternate_screen();
                    } else {
                        self.leave_alternate_screen();
                        self.restore_cursor();
                    }
                }
                2004 => {} // Bracketed paste mode — ignored (handled by PTY layer).
                2026 => {
                    // Synchronized output: buffer screen updates.
                    self.sync_mode = set;
                }
                1047 => {
                    // Alternate screen buffer (without save/restore cursor).
                    if set {
                        self.enter_alternate_screen();
                    } else {
                        self.leave_alternate_screen();
                    }
                }
                _ => {} // Ignore unknown modes.
            }
        }
    }
}

impl vte::Perform for Terminal {
    fn print(&mut self, c: char) {
        self.screen_mut().write_char(c);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            0x08 => self.screen_mut().cursor_back(1), // BS (backspace)
            0x09 => {
                // HT (horizontal tab): advance to next tab stop (every 8 cols).
                let s = self.screen_mut();
                let next_tab = (s.cursor.col / 8 + 1) * 8;
                s.cursor.col = next_tab.min(s.cols() - 1);
            }
            0x0A..=0x0C => self.screen_mut().line_feed(), // LF, VT, FF
            0x0D => self.screen_mut().carriage_return(),  // CR
            _ => {}
        }
    }

    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        let first = params.iter().next().map(|p| p[0]).unwrap_or(0);
        let second = params.iter().nth(1).map(|p| p[0]).unwrap_or(0);

        // Handle DEC private modes (CSI ? ... h/l).
        if intermediates == [b'?'] {
            match action {
                'h' => {
                    self.handle_dec_mode(params, true);
                    return;
                }
                'l' => {
                    self.handle_dec_mode(params, false);
                    return;
                }
                _ => return,
            }
        }

        // Standard CSI sequences.
        let n = if first == 0 { 1 } else { first as usize };

        match action {
            // Cursor movement.
            'A' => self.screen_mut().cursor_up(n),      // CUU
            'B' => self.screen_mut().cursor_down(n),    // CUD
            'C' => self.screen_mut().cursor_forward(n), // CUF
            'D' => self.screen_mut().cursor_back(n),    // CUB
            'H' | 'f' => {
                // CUP / HVP: set cursor position (row;col, 1-indexed).
                let row = if first == 0 { 1 } else { first as usize };
                let col = if second == 0 { 1 } else { second as usize };
                self.screen_mut().set_cursor_pos(row, col);
            }
            'G' => {
                // CHA: cursor character absolute (column only, 1-indexed).
                self.screen_mut().cursor.col = n.saturating_sub(1).min(self.screen().cols() - 1);
            }
            'd' => {
                // VPA: line position absolute (row only, 1-indexed).
                self.screen_mut().cursor.row = n.saturating_sub(1).min(self.screen().rows() - 1);
            }

            // Erase.
            'J' => self.screen_mut().erase_in_display(first), // ED
            'K' => self.screen_mut().erase_in_line(first),    // EL

            // Scroll.
            'S' => self.screen_mut().scroll_up(n),   // SU
            'T' => self.screen_mut().scroll_down(n), // SD
            'r' => {
                // DECSTBM: set scroll region.
                let top = if first == 0 { 1 } else { first as usize };
                let bottom = if second == 0 {
                    self.screen().rows()
                } else {
                    second as usize
                };
                self.screen_mut().set_scroll_region(top, bottom);
            }

            // Insert / Delete.
            'L' => self.screen_mut().insert_lines(n), // IL
            'M' => self.screen_mut().delete_lines(n), // DL
            '@' => self.screen_mut().insert_chars(n), // ICH
            'P' => self.screen_mut().delete_chars(n), // DCH

            // SGR — only handle standard SGR (no intermediates).
            // CSI > m and CSI < m are xterm/kitty private sequences, not SGR.
            'm' if intermediates.is_empty() => self.handle_sgr(params),

            // Device Status Report.
            'n' => {
                if first == 6 {
                    // CPR: cursor position report.
                    let row = self.screen().cursor.row + 1;
                    let col = self.screen().cursor.col + 1;
                    let response = format!("\x1b[{};{}R", row, col);
                    self.response_buf.extend_from_slice(response.as_bytes());
                }
            }

            // Soft terminal reset (DECSTR).
            'p' if intermediates == [b'!'] => {
                self.screen_mut().reset();
                self.modes = TerminalModes::default();
            }

            _ => {} // Ignore unknown CSI sequences.
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], _ignore: bool, byte: u8) {
        match (intermediates, byte) {
            ([], b'7') => self.save_cursor(),            // DECSC
            ([], b'8') => self.restore_cursor(),         // DECRC
            ([], b'D') => self.screen_mut().line_feed(), // IND (index = LF)
            ([], b'M') => {
                // RI (reverse index): move cursor up, scroll down if at top.
                let top = self.screen().cursor.row;
                if top == 0 {
                    self.screen_mut().scroll_down(1);
                } else {
                    self.screen_mut().cursor_up(1);
                }
            }
            ([], b'c') => {
                // RIS: full reset.
                self.screen_mut().reset();
                self.modes = TerminalModes::default();
            }
            _ => {}
        }
    }

    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {
        // TODO: handle OSC 0/2 (window title), OSC 8 (hyperlinks), OSC 52 (clipboard).
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn term() -> Terminal {
        Terminal::new(80, 24)
    }

    fn feed(t: &mut Terminal, s: &str) {
        t.feed(s.as_bytes());
    }

    #[test]
    fn print_text() {
        let mut t = term();
        feed(&mut t, "Hello");
        assert_eq!(t.screen().row_text(0), "Hello");
        assert_eq!(t.screen().cursor.col, 5);
    }

    #[test]
    fn crlf() {
        let mut t = term();
        feed(&mut t, "Hello\r\nWorld");
        assert_eq!(t.screen().row_text(0), "Hello");
        assert_eq!(t.screen().row_text(1), "World");
    }

    #[test]
    fn cursor_movement_csi() {
        let mut t = term();
        feed(&mut t, "\x1b[5;10H"); // CUP to row 5, col 10
        assert_eq!(t.screen().cursor.row, 4);
        assert_eq!(t.screen().cursor.col, 9);

        feed(&mut t, "\x1b[2A"); // CUU 2
        assert_eq!(t.screen().cursor.row, 2);

        feed(&mut t, "\x1b[3B"); // CUD 3
        assert_eq!(t.screen().cursor.row, 5);

        feed(&mut t, "\x1b[5C"); // CUF 5
        assert_eq!(t.screen().cursor.col, 14);

        feed(&mut t, "\x1b[3D"); // CUB 3
        assert_eq!(t.screen().cursor.col, 11);
    }

    #[test]
    fn sgr_bold_and_color() {
        let mut t = term();
        feed(&mut t, "\x1b[1;31mX\x1b[0m"); // bold + red, then reset
        let cell = t.screen().cell(0, 0);
        assert_eq!(cell.ch, 'X');
        assert_eq!(cell.fg, Color::Indexed(1)); // red
        assert!(cell.attrs.bold);

        // After reset, pen should be default.
        feed(&mut t, "Y");
        let cell = t.screen().cell(0, 1);
        assert_eq!(cell.fg, Color::Default);
        assert!(!cell.attrs.bold);
    }

    #[test]
    fn sgr_256_color() {
        let mut t = term();
        feed(&mut t, "\x1b[38;5;200mX"); // fg = indexed 200
        let cell = t.screen().cell(0, 0);
        assert_eq!(cell.fg, Color::Indexed(200));
    }

    #[test]
    fn sgr_rgb_color() {
        let mut t = term();
        feed(&mut t, "\x1b[38;2;255;128;0mX"); // fg = RGB(255, 128, 0)
        let cell = t.screen().cell(0, 0);
        assert_eq!(cell.fg, Color::Rgb(255, 128, 0));
    }

    #[test]
    fn erase_display() {
        let mut t = term();
        feed(&mut t, "Hello\r\n World");
        feed(&mut t, "\x1b[2J"); // erase entire display
        assert_eq!(t.screen().row_text(0), "");
        assert_eq!(t.screen().row_text(1), "");
    }

    #[test]
    fn erase_line_from_cursor() {
        let mut t = term();
        feed(&mut t, "Hello");
        feed(&mut t, "\x1b[3G"); // CHA to col 3
        feed(&mut t, "\x1b[K"); // EL mode 0 (cursor to end)
        assert_eq!(t.screen().row_text(0), "He");
    }

    #[test]
    fn scroll_region() {
        let mut t = Terminal::new(10, 5);
        feed(&mut t, "A\r\nB\r\nC\r\nD\r\nE");
        feed(&mut t, "\x1b[2;4r"); // scroll region rows 2-4
        feed(&mut t, "\x1b[S"); // scroll up 1
        assert_eq!(t.screen().cell(0, 0).ch, 'A'); // outside region
        assert_eq!(t.screen().cell(1, 0).ch, 'C'); // shifted up
        assert_eq!(t.screen().cell(2, 0).ch, 'D');
        assert_eq!(t.screen().cell(3, 0).ch, ' '); // blank
        assert_eq!(t.screen().cell(4, 0).ch, 'E'); // outside region
    }

    #[test]
    fn alternate_screen() {
        let mut t = term();
        feed(&mut t, "Primary");
        assert_eq!(t.screen().row_text(0), "Primary");

        feed(&mut t, "\x1b[?1049h"); // enter alt screen
        assert_eq!(t.screen().row_text(0), ""); // alt is blank
        assert!(t.alt_active);

        feed(&mut t, "Alt");
        assert_eq!(t.screen().row_text(0), "Alt");

        feed(&mut t, "\x1b[?1049l"); // leave alt screen
        assert!(!t.alt_active);
        assert_eq!(t.screen().row_text(0), "Primary"); // primary restored
    }

    #[test]
    fn cursor_visibility() {
        let mut t = term();
        assert!(t.screen().cursor.visible);
        feed(&mut t, "\x1b[?25l"); // hide cursor
        assert!(!t.screen().cursor.visible);
        feed(&mut t, "\x1b[?25h"); // show cursor
        assert!(t.screen().cursor.visible);
    }

    #[test]
    fn device_status_report_cursor_position() {
        let mut t = term();
        feed(&mut t, "\x1b[5;10H"); // cursor at row 5, col 10
        feed(&mut t, "\x1b[6n"); // request CPR
        let response = t.take_response();
        assert_eq!(response, b"\x1b[5;10R");
    }

    #[test]
    fn insert_delete_chars() {
        let mut t = Terminal::new(10, 1);
        feed(&mut t, "ABCDE");
        feed(&mut t, "\x1b[2G"); // col 2 (0-indexed: 1)
        feed(&mut t, "\x1b[2@"); // insert 2 chars
        assert_eq!(t.screen().cell(0, 0).ch, 'A');
        assert_eq!(t.screen().cell(0, 1).ch, ' ');
        assert_eq!(t.screen().cell(0, 2).ch, ' ');
        assert_eq!(t.screen().cell(0, 3).ch, 'B');
    }

    #[test]
    fn insert_delete_lines() {
        let mut t = Terminal::new(5, 4);
        feed(&mut t, "A\r\nB\r\nC\r\nD");
        feed(&mut t, "\x1b[2;1H"); // row 2
        feed(&mut t, "\x1b[1L"); // insert 1 line
        assert_eq!(t.screen().cell(0, 0).ch, 'A');
        assert_eq!(t.screen().cell(1, 0).ch, ' '); // inserted
        assert_eq!(t.screen().cell(2, 0).ch, 'B');
        assert_eq!(t.screen().cell(3, 0).ch, 'C'); // D pushed off
    }

    #[test]
    fn backspace() {
        let mut t = term();
        feed(&mut t, "AB\x08C"); // write A, B, backspace, C (overwrites B)
        assert_eq!(t.screen().cell(0, 0).ch, 'A');
        assert_eq!(t.screen().cell(0, 1).ch, 'C');
    }

    #[test]
    fn tab_stops() {
        let mut t = term();
        feed(&mut t, "A\tB");
        assert_eq!(t.screen().cell(0, 0).ch, 'A');
        assert_eq!(t.screen().cell(0, 8).ch, 'B'); // tab to col 8
    }

    #[test]
    fn save_restore_cursor() {
        let mut t = term();
        feed(&mut t, "\x1b[5;10H"); // row 5, col 10
        feed(&mut t, "\x1b7"); // DECSC
        feed(&mut t, "\x1b[1;1H"); // move to 1,1
        feed(&mut t, "\x1b8"); // DECRC
        assert_eq!(t.screen().cursor.row, 4);
        assert_eq!(t.screen().cursor.col, 9);
    }

    #[test]
    fn full_reset() {
        let mut t = term();
        feed(&mut t, "\x1b[1;31mHello");
        feed(&mut t, "\x1bc"); // RIS
        assert_eq!(t.screen().row_text(0), "");
        assert_eq!(t.screen().cursor.row, 0);
    }

    #[test]
    fn soft_reset() {
        let mut t = term();
        t.modes.autowrap = false;
        feed(&mut t, "\x1b[!p"); // DECSTR
        assert!(t.modes.autowrap); // restored to default
    }

    #[test]
    fn bright_colors() {
        let mut t = term();
        feed(&mut t, "\x1b[91mX"); // bright red fg
        assert_eq!(t.screen().cell(0, 0).fg, Color::Indexed(9));
    }

    #[test]
    fn ls_color_output() {
        let mut t = Terminal::new(40, 5);
        // Simulated `ls --color` output with green for executables.
        feed(
            &mut t,
            "\x1b[0m\x1b[01;32mscript.sh\x1b[0m  \x1b[01;34mdir/\x1b[0m\r\n",
        );
        // script.sh should be bold green.
        let cell = t.screen().cell(0, 0);
        assert_eq!(cell.ch, 's');
        assert_eq!(cell.fg, Color::Indexed(2)); // green
        assert!(cell.attrs.bold);
    }
    #[test]
    fn sync_mode() {
        let mut t = term();
        assert!(!t.is_sync_mode());
        feed(&mut t, "\x1b[?2026h");
        assert!(t.is_sync_mode());
        feed(&mut t, "\x1b[?2026l");
        assert!(!t.is_sync_mode());
    }

    #[test]
    fn terminal_resize() {
        let mut t = Terminal::new(80, 24);
        feed(&mut t, "Hello");
        t.resize(40, 10);
        assert_eq!(t.screen().cols(), 40);
        assert_eq!(t.screen().rows(), 10);
        // Content may shift during resize — verify no panic and dimensions correct.
    }

    #[test]
    fn sgr_all_attributes() {
        let mut t = term();
        feed(
            &mut t,
            "\x1b[2mD\x1b[3mI\x1b[4mU\x1b[7mR\x1b[8mH\x1b[9mS\x1b[0m",
        );
        assert!(t.screen().cell(0, 0).attrs.dim);
        assert!(t.screen().cell(0, 1).attrs.italic);
        assert!(t.screen().cell(0, 2).attrs.underline);
        assert!(t.screen().cell(0, 3).attrs.reverse);
        assert!(t.screen().cell(0, 4).attrs.hidden);
        assert!(t.screen().cell(0, 5).attrs.strikethrough);
    }

    #[test]
    fn sgr_reset_individual_attrs() {
        let mut t = term();
        feed(&mut t, "\x1b[1;2;3;4;7;8;9m");
        feed(
            &mut t,
            "\x1b[21m\x1b[22m\x1b[23m\x1b[24m\x1b[27m\x1b[28m\x1b[29m",
        );
        feed(&mut t, "X");
        let c = t.screen().cell(0, 0);
        assert!(!c.attrs.bold);
        assert!(!c.attrs.dim);
        assert!(!c.attrs.italic);
        assert!(!c.attrs.underline);
        assert!(!c.attrs.reverse);
        assert!(!c.attrs.hidden);
        assert!(!c.attrs.strikethrough);
    }

    #[test]
    fn sgr_default_fg_bg() {
        let mut t = term();
        feed(&mut t, "\x1b[31m\x1b[42m"); // red fg, green bg
        feed(&mut t, "\x1b[39m"); // default fg
        feed(&mut t, "A");
        assert_eq!(t.screen().cell(0, 0).fg, Color::Default);
        assert_eq!(t.screen().cell(0, 0).bg, Color::Indexed(2)); // green still
        feed(&mut t, "\x1b[49m"); // default bg
        feed(&mut t, "B");
        assert_eq!(t.screen().cell(0, 1).bg, Color::Default);
    }

    #[test]
    fn sgr_256_bg_color() {
        let mut t = term();
        feed(&mut t, "\x1b[48;5;100mX\x1b[0m");
        assert_eq!(t.screen().cell(0, 0).bg, Color::Indexed(100));
    }

    #[test]
    fn sgr_bright_bg() {
        let mut t = term();
        feed(&mut t, "\x1b[105mX\x1b[0m");
        assert_eq!(t.screen().cell(0, 0).bg, Color::Indexed(13));
    }

    #[test]
    fn dec_modes() {
        let mut t = term();
        feed(&mut t, "\x1b[?1h"); // DECCKM on
        assert!(t.modes.application_cursor_keys);
        feed(&mut t, "\x1b[?1l"); // DECCKM off
        assert!(!t.modes.application_cursor_keys);

        feed(&mut t, "\x1b[?4h"); // IRM on
        assert!(t.modes.insert);
        feed(&mut t, "\x1b[?4l");
        assert!(!t.modes.insert);

        feed(&mut t, "\x1b[?7l"); // DECAWM off
        assert!(!t.modes.autowrap);
        feed(&mut t, "\x1b[?7h");
        assert!(t.modes.autowrap);
    }

    #[test]
    fn bracketed_paste_mode_ignored() {
        let mut t = term();
        // Should not panic or change state.
        feed(&mut t, "\x1b[?2004h");
        feed(&mut t, "\x1b[?2004l");
        feed(&mut t, "OK");
        assert_eq!(t.screen().cell(0, 0).ch, 'O');
    }

    #[test]
    fn extended_color_invalid() {
        let mut t = term();
        // SGR 38;9;... — invalid sub-command, should be ignored.
        feed(&mut t, "\x1b[38;9;1;2;3mX\x1b[0m");
        // Should not crash, X rendered with default color.
        assert_eq!(t.screen().cell(0, 0).ch, 'X');
    }
}
