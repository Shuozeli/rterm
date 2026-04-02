use crate::buffer::{Pen, ScreenBuffer};
use crate::cell::Flags;
use crate::color::Color;

/// Character set designation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Charset {
    /// Standard ASCII.
    #[default]
    Ascii,
    /// DEC Special Graphics (line drawing characters).
    DecSpecialGraphics,
}

/// Map a byte in 0x60..=0x7E to DEC Special Graphics Unicode character.
fn dec_special_graphics_char(ch: char) -> Option<char> {
    let mapped = match ch {
        '_' => ' ',
        '`' => '\u{25c6}', // diamond
        'a' => '\u{2592}', // medium shade
        'b' => '\u{2409}', // HT symbol
        'c' => '\u{240c}', // FF symbol
        'd' => '\u{240d}', // CR symbol
        'e' => '\u{240a}', // LF symbol
        'f' => '\u{00b0}', // degree
        'g' => '\u{00b1}', // plus-minus
        'h' => '\u{2424}', // NL symbol
        'i' => '\u{240b}', // VT symbol
        'j' => '\u{2518}', // box: bottom-right
        'k' => '\u{2510}', // box: top-right
        'l' => '\u{250c}', // box: top-left
        'm' => '\u{2514}', // box: bottom-left
        'n' => '\u{253c}', // box: cross
        'o' => '\u{23ba}', // scan line 1
        'p' => '\u{23bb}', // scan line 3
        'q' => '\u{2500}', // horizontal line
        'r' => '\u{23bc}', // scan line 7
        's' => '\u{23bd}', // scan line 9
        't' => '\u{251c}', // box: left tee
        'u' => '\u{2524}', // box: right tee
        'v' => '\u{2534}', // box: bottom tee
        'w' => '\u{252c}', // box: top tee
        'x' => '\u{2502}', // vertical line
        'y' => '\u{2264}', // less-than-or-equal
        'z' => '\u{2265}', // greater-than-or-equal
        '{' => '\u{03c0}', // pi
        '|' => '\u{2260}', // not-equal
        '}' => '\u{00a3}', // pound sterling
        '~' => '\u{00b7}', // middle dot
        _ => return None,
    };
    Some(mapped)
}

/// Saved cursor state for DECSC/DECRC.
#[derive(Debug, Clone)]
pub struct SavedCursor {
    pub row: usize,
    pub col: usize,
    pub pen: Pen,
    pub origin_mode: bool,
    pub charset_g0: Charset,
    pub charset_g1: Charset,
    pub active_charset: usize,
}

impl Default for SavedCursor {
    fn default() -> Self {
        Self {
            row: 0,
            col: 0,
            pen: Pen::default(),
            origin_mode: false,
            charset_g0: Charset::Ascii,
            charset_g1: Charset::Ascii,
            active_charset: 0,
        }
    }
}

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
    /// Mouse tracking mode: 0=Off, 1=Normal (1000), 2=ButtonEvent (1002), 3=AnyEvent (1003)
    pub mouse_tracking_mode: u8,
    /// SGR Mouse mode (1006): use `<button;x;yM` format instead of legacy 223 max.
    pub mouse_sgr_mode: bool,
    /// Application keypad mode (DECKPAM=true, DECKPNM=false).
    pub application_keypad: bool,
    /// Focus event tracking (DECSET 1004).
    pub focus_events: bool,
}

impl Default for TerminalModes {
    fn default() -> Self {
        Self {
            autowrap: true,
            application_cursor_keys: false,
            insert: false,
            origin: false,
            mouse_tracking_mode: 0,
            mouse_sgr_mode: false,
            application_keypad: false,
            focus_events: false,
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
    /// Saved cursor state (for DECSC/DECRC).
    saved_cursor: SavedCursor,
    /// Response bytes to be read by the PTY (e.g., DSR responses).
    response_buf: Vec<u8>,
    /// Persistent VT parser (retains state between feed() calls).
    parser: vte::Parser,
    /// Synchronized output mode (CSI ?2026 h/l).
    sync_mode: bool,
    /// Bracketed paste mode (CSI ?2004 h/l).
    pub bracketed_paste: bool,
    /// Cursor style (DECSCUSR): 0=default, 1=blinking block, 2=steady block,
    /// 3=blinking underline, 4=steady underline, 5=blinking bar, 6=steady bar.
    pub cursor_style: u8,
    /// Window/icon title set by OSC 0/1/2.
    pub title: Option<String>,
    /// Icon name set by OSC 1.
    pub icon_name: Option<String>,
    /// Bell pending flag -- set when BEL (0x07) is received.
    pub bell_pending: bool,
    /// G0 charset designation.
    charset_g0: Charset,
    /// G1 charset designation.
    charset_g1: Charset,
    /// Active charset: 0 = G0, 1 = G1.
    active_charset: usize,
}

impl Terminal {
    pub fn new(cols: usize, rows: usize) -> Self {
        Self {
            primary: ScreenBuffer::new(cols, rows),
            alternate: ScreenBuffer::new(cols, rows),
            alt_active: false,
            modes: TerminalModes::default(),
            saved_cursor: SavedCursor::default(),
            response_buf: Vec::new(),
            parser: vte::Parser::new(),
            sync_mode: false,
            bracketed_paste: false,
            cursor_style: 0,
            title: None,
            icon_name: None,
            bell_pending: false,
            charset_g0: Charset::Ascii,
            charset_g1: Charset::Ascii,
            active_charset: 0,
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

    /// Whether the terminal is currently showing the alternate screen.
    pub fn is_alt_screen_active(&self) -> bool {
        self.alt_active
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

    /// Push response bytes (for DA1, DA2, DSR, etc.).
    fn write_response(&mut self, bytes: &[u8]) {
        self.response_buf.extend_from_slice(bytes);
    }

    /// Whether synchronized output mode is active.
    /// When true, the renderer should NOT repaint -- wait for it to go false.
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

    /// Get the active charset for printing.
    fn active_charset(&self) -> Charset {
        if self.active_charset == 1 {
            self.charset_g1
        } else {
            self.charset_g0
        }
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

    /// Save cursor position and pen state (DECSC).
    fn save_cursor(&mut self) {
        let s = self.screen();
        self.saved_cursor = SavedCursor {
            row: s.cursor.row,
            col: s.cursor.col,
            pen: s.pen.clone(),
            origin_mode: self.modes.origin,
            charset_g0: self.charset_g0,
            charset_g1: self.charset_g1,
            active_charset: self.active_charset,
        };
    }

    /// Restore cursor position and pen state (DECRC).
    fn restore_cursor(&mut self) {
        let saved = self.saved_cursor.clone();
        let s = self.screen_mut();
        s.cursor.row = saved.row;
        s.cursor.col = saved.col;
        s.pen = saved.pen;
        self.modes.origin = saved.origin_mode;
        self.charset_g0 = saved.charset_g0;
        self.charset_g1 = saved.charset_g1;
        self.active_charset = saved.active_charset;
    }

    /// Handle SGR (Select Graphic Rendition) parameters.
    /// Iterates over param groups to support sub-parameters (e.g., 4:2 for double underline).
    fn handle_sgr(&mut self, params: &vte::Params) {
        // Collect param groups to avoid borrow conflicts with self.
        let groups: Vec<Vec<u16>> = params.iter().map(|p| p.to_vec()).collect();

        for group in &groups {
            if group.is_empty() {
                continue;
            }
            let code = group[0];

            match code {
                0 => {
                    let s = self.screen_mut();
                    s.pen.fg = Color::Default;
                    s.pen.bg = Color::Default;
                    s.pen.flags = Flags::empty();
                }
                1 => self.screen_mut().pen.flags.insert(Flags::BOLD),
                2 => self.screen_mut().pen.flags.insert(Flags::DIM),
                3 => self.screen_mut().pen.flags.insert(Flags::ITALIC),
                4 => {
                    let style = group.get(1).copied().unwrap_or(1);
                    self.screen_mut().pen.flags.remove(Flags::ALL_UNDERLINES);
                    match style {
                        0 => {} // 4:0 = underline off
                        2 => self.screen_mut().pen.flags.insert(Flags::DOUBLE_UNDERLINE),
                        3 => self.screen_mut().pen.flags.insert(Flags::UNDERCURL),
                        4 => self.screen_mut().pen.flags.insert(Flags::DOTTED_UNDERLINE),
                        5 => self.screen_mut().pen.flags.insert(Flags::DASHED_UNDERLINE),
                        _ => self.screen_mut().pen.flags.insert(Flags::UNDERLINE), // 1 or unrecognized
                    }
                }
                7 => self.screen_mut().pen.flags.insert(Flags::INVERSE),
                8 => self.screen_mut().pen.flags.insert(Flags::HIDDEN),
                9 => self.screen_mut().pen.flags.insert(Flags::STRIKEOUT),
                // SGR 21: DOUBLE_UNDERLINE (standard behavior; was non-standard bold-off)
                21 => self.screen_mut().pen.flags.insert(Flags::DOUBLE_UNDERLINE),
                22 => self.screen_mut().pen.flags.remove(Flags::BOLD | Flags::DIM),
                23 => self.screen_mut().pen.flags.remove(Flags::ITALIC),
                24 => self.screen_mut().pen.flags.remove(Flags::ALL_UNDERLINES),
                27 => self.screen_mut().pen.flags.remove(Flags::INVERSE),
                28 => self.screen_mut().pen.flags.remove(Flags::HIDDEN),
                29 => self.screen_mut().pen.flags.remove(Flags::STRIKEOUT),

                30..=37 => self.screen_mut().pen.fg = Color::Indexed((code - 30) as u8),
                38 => {
                    // Extended color: sub-params in same group (38:5:n or 38:2:r:g:b),
                    // or legacy semicolon-separated params (handled via next groups).
                    if group.len() >= 3 && group[1] == 5 {
                        self.screen_mut().pen.fg = Color::Indexed(group[2] as u8);
                    } else if group.len() >= 5 && group[1] == 2 {
                        self.screen_mut().pen.fg =
                            Color::Rgb(group[2] as u8, group[3] as u8, group[4] as u8);
                    }
                    // Legacy semicolon form is handled below via remaining_groups
                }
                39 => self.screen_mut().pen.fg = Color::Default,

                40..=47 => self.screen_mut().pen.bg = Color::Indexed((code - 40) as u8),
                48 => {
                    if group.len() >= 3 && group[1] == 5 {
                        self.screen_mut().pen.bg = Color::Indexed(group[2] as u8);
                    } else if group.len() >= 5 && group[1] == 2 {
                        self.screen_mut().pen.bg =
                            Color::Rgb(group[2] as u8, group[3] as u8, group[4] as u8);
                    }
                }
                49 => self.screen_mut().pen.bg = Color::Default,

                90..=97 => self.screen_mut().pen.fg = Color::Indexed((code - 90 + 8) as u8),
                100..=107 => self.screen_mut().pen.bg = Color::Indexed((code - 100 + 8) as u8),

                _ => {}
            }
        }

        // Handle legacy semicolon-separated 38/48 color forms:
        // e.g., ESC[38;5;200m sends groups [[38],[5],[200]].
        // We need a second pass over the flat list for this.
        self.handle_sgr_legacy_extended_colors(params);
    }

    /// Second-pass handler for legacy semicolon-extended colors (38;5;n, 38;2;r;g;b).
    /// The main handle_sgr loop handles colon sub-params (38:5:n).
    /// This handles the semicolon form where each number is a separate group.
    fn handle_sgr_legacy_extended_colors(&mut self, params: &vte::Params) {
        let flat: Vec<u16> = params.iter().flat_map(|p| p.iter().copied()).collect();
        let mut i = 0;
        while i < flat.len() {
            let code = flat[i];
            match code {
                38 => {
                    if let Some((color, consumed)) = Self::parse_extended_color(&flat[i + 1..]) {
                        self.screen_mut().pen.fg = color;
                        i += 1 + consumed;
                        continue;
                    }
                }
                48 => {
                    if let Some((color, consumed)) = Self::parse_extended_color(&flat[i + 1..]) {
                        self.screen_mut().pen.bg = color;
                        i += 1 + consumed;
                        continue;
                    }
                }
                _ => {}
            }
            i += 1;
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
                1000 => {
                    if set {
                        self.modes.mouse_tracking_mode = 1;
                    } else if self.modes.mouse_tracking_mode == 1 {
                        self.modes.mouse_tracking_mode = 0;
                    }
                }
                1002 => {
                    if set {
                        self.modes.mouse_tracking_mode = 2;
                    } else if self.modes.mouse_tracking_mode == 2 {
                        self.modes.mouse_tracking_mode = 0;
                    }
                }
                1003 => {
                    if set {
                        self.modes.mouse_tracking_mode = 3;
                    } else if self.modes.mouse_tracking_mode == 3 {
                        self.modes.mouse_tracking_mode = 0;
                    }
                }
                1004 => self.modes.focus_events = set, // Focus events
                1006 => self.modes.mouse_sgr_mode = set,
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
                2004 => self.bracketed_paste = set,
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
        // Apply charset mapping if active charset is DEC Special Graphics.
        let ch = if self.active_charset() == Charset::DecSpecialGraphics {
            dec_special_graphics_char(c).unwrap_or(c)
        } else {
            c
        };
        self.screen_mut().write_char(ch);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            0x07 => self.bell_pending = true,                // BEL
            0x08 => self.screen_mut().cursor_back(1),        // BS (backspace)
            0x09 => self.screen_mut().cursor_forward_tab(1), // HT (horizontal tab)
            0x0A..=0x0C => self.screen_mut().line_feed(),    // LF, VT, FF
            0x0D => self.screen_mut().carriage_return(),     // CR
            0x0E => self.active_charset = 1,                 // SO (Shift Out) -> G1
            0x0F => self.active_charset = 0,                 // SI (Shift In) -> G0
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

        // Handle CSI > sequences (secondary DA, xterm private).
        if intermediates == [b'>'] {
            if action == 'c' {
                // DA2 (Secondary Device Attributes): respond with VT100-compat.
                self.write_response(b"\x1b[>0;0;0c");
            }
            return;
        }

        // Reject other intermediates for standard CSI sequences.
        if !intermediates.is_empty() {
            // Handle specific known intermediates.
            match (intermediates, action) {
                // Soft terminal reset (DECSTR).
                ([b'!'], 'p') => {
                    self.screen_mut().reset();
                    self.modes = TerminalModes::default();
                    self.charset_g0 = Charset::Ascii;
                    self.charset_g1 = Charset::Ascii;
                    self.active_charset = 0;
                }
                // DECSCUSR: set cursor style.
                ([b' '], 'q') => {
                    self.cursor_style = first as u8;
                }
                _ => {} // Ignore unknown intermediates.
            }
            return;
        }

        // Standard CSI sequences (no intermediates).
        let n = if first == 0 { 1 } else { first as usize };

        match action {
            // Cursor movement.
            'A' => self.screen_mut().cursor_up(n),      // CUU
            'B' => self.screen_mut().cursor_down(n),    // CUD
            'C' => self.screen_mut().cursor_forward(n), // CUF
            'D' => self.screen_mut().cursor_back(n),    // CUB
            'E' => {
                // CNL: Cursor Next Line -- move down n lines, then CR.
                self.screen_mut().cursor_down(n);
                self.screen_mut().carriage_return();
            }
            'F' => {
                // CPL: Cursor Previous Line -- move up n lines, then CR.
                self.screen_mut().cursor_up(n);
                self.screen_mut().carriage_return();
            }
            'G' => {
                // CHA: cursor character absolute (column only, 1-indexed).
                self.screen_mut().cursor.col = n.saturating_sub(1).min(self.screen().cols() - 1);
            }
            'H' | 'f' => {
                // CUP / HVP: set cursor position (row;col, 1-indexed).
                let row = if first == 0 { 1 } else { first as usize };
                let col = if second == 0 { 1 } else { second as usize };
                self.screen_mut().set_cursor_pos(row, col);
            }
            'I' => {
                // CHT: Cursor Forward Tab -- advance by n tab stops.
                self.screen_mut().cursor_forward_tab(n);
            }
            'Z' => {
                // CBT: Cursor Backward Tab -- move back by n tab stops.
                self.screen_mut().cursor_backward_tab(n);
            }
            '`' => {
                // HPA: Horizontal Position Absolute (same as CHA).
                self.screen_mut().cursor.col = n.saturating_sub(1).min(self.screen().cols() - 1);
            }
            'd' => {
                // VPA: line position absolute (row only, 1-indexed).
                self.screen_mut().cursor.row = n.saturating_sub(1).min(self.screen().rows() - 1);
            }

            // Erase.
            'J' => self.screen_mut().erase_in_display(first), // ED
            'K' => self.screen_mut().erase_in_line(first),    // EL
            'X' => {
                // ECH: Erase Characters -- erase n chars at cursor, no cursor move.
                self.screen_mut().erase_chars(n);
            }

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

            // Tab clear.
            'g' => {
                // TBC: Tab Clear.
                self.screen_mut().clear_tab_stop(first);
            }

            // SGR -- only handle standard SGR (no intermediates).
            // CSI > m and CSI < m are xterm/kitty private sequences, not SGR.
            'm' => self.handle_sgr(params),

            // Device Status Report.
            'n' => {
                if first == 6 {
                    // CPR: cursor position report.
                    let row = self.screen().cursor.row + 1;
                    let col = self.screen().cursor.col + 1;
                    let response = format!("\x1b[{};{}R", row, col);
                    self.write_response(response.as_bytes());
                } else if first == 5 {
                    // DSR 5n: Device Status Report -- terminal OK.
                    self.write_response(b"\x1b[0n");
                }
            }

            // DA1: Primary Device Attributes.
            'c' => {
                // CSI c or CSI 0 c -- respond with VT220 identity.
                if first == 0 {
                    self.write_response(b"\x1b[?62;22c");
                }
            }

            _ => {} // Ignore unknown CSI sequences.
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], _ignore: bool, byte: u8) {
        match (intermediates, byte) {
            ([], b'7') => self.save_cursor(),            // DECSC
            ([], b'8') => self.restore_cursor(),         // DECRC
            ([], b'D') => self.screen_mut().line_feed(), // IND (index = LF)
            ([], b'E') => {
                // NEL: Next Line = LF + CR combined.
                self.screen_mut().line_feed();
                self.screen_mut().carriage_return();
            }
            ([], b'H') => {
                // HTS: Horizontal Tab Set -- set tab stop at current column.
                self.screen_mut().set_tab_stop();
            }
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
                self.charset_g0 = Charset::Ascii;
                self.charset_g1 = Charset::Ascii;
                self.active_charset = 0;
                self.title = None;
                self.icon_name = None;
                self.bell_pending = false;
            }
            ([], b'=') => {
                // DECKPAM: Application Keypad Mode.
                self.modes.application_keypad = true;
            }
            ([], b'>') => {
                // DECKPNM: Normal Keypad Mode.
                self.modes.application_keypad = false;
            }
            // SCS: Designate G0 character set.
            ([b'('], b'0') => self.charset_g0 = Charset::DecSpecialGraphics,
            ([b'('], b'B') => self.charset_g0 = Charset::Ascii,
            // SCS: Designate G1 character set.
            ([b')'], b'0') => self.charset_g1 = Charset::DecSpecialGraphics,
            ([b')'], b'B') => self.charset_g1 = Charset::Ascii,
            _ => {}
        }
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        if params.is_empty() {
            return;
        }
        // First param is the OSC command number (as bytes, e.g., b"0", b"1", b"2").
        let cmd = std::str::from_utf8(params[0]).unwrap_or("");
        let text = if params.len() > 1 {
            std::str::from_utf8(params[1]).ok().map(|s| s.to_string())
        } else {
            None
        };

        match cmd {
            "0" => {
                // OSC 0: set icon name and window title.
                self.icon_name = text.clone();
                self.title = text;
            }
            "1" => {
                // OSC 1: set icon name only.
                self.icon_name = text;
            }
            "2" => {
                // OSC 2: set window title only.
                self.title = text;
            }
            _ => {} // Ignore unknown OSC commands.
        }
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
        assert!(cell.flags.contains(Flags::BOLD));

        // After reset, pen should be default.
        feed(&mut t, "Y");
        let cell = t.screen().cell(0, 1);
        assert_eq!(cell.fg, Color::Default);
        assert!(!cell.flags.contains(Flags::BOLD));
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
        assert!(cell.flags.contains(Flags::BOLD));
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
        // Content may shift during resize -- verify no panic and dimensions correct.
    }

    #[test]
    fn sgr_all_attributes() {
        let mut t = term();
        feed(
            &mut t,
            "\x1b[2mD\x1b[3mI\x1b[4mU\x1b[7mR\x1b[8mH\x1b[9mS\x1b[0m",
        );
        assert!(t.screen().cell(0, 0).flags.contains(Flags::DIM));
        assert!(t.screen().cell(0, 1).flags.contains(Flags::ITALIC));
        assert!(t.screen().cell(0, 2).flags.contains(Flags::UNDERLINE));
        assert!(t.screen().cell(0, 3).flags.contains(Flags::INVERSE));
        assert!(t.screen().cell(0, 4).flags.contains(Flags::HIDDEN));
        assert!(t.screen().cell(0, 5).flags.contains(Flags::STRIKEOUT));
    }

    #[test]
    fn sgr_reset_individual_attrs() {
        let mut t = term();
        feed(&mut t, "\x1b[1;2;3;4;7;8;9m");
        // SGR 21 now sets DOUBLE_UNDERLINE (standard behavior), not bold-off.
        // SGR 22 resets bold+dim, 23 italic, 24 all underlines, 27 inverse, 28 hidden, 29 strikeout.
        feed(&mut t, "\x1b[22m\x1b[23m\x1b[24m\x1b[27m\x1b[28m\x1b[29m");
        feed(&mut t, "X");
        let c = t.screen().cell(0, 0);
        assert!(!c.flags.contains(Flags::BOLD));
        assert!(!c.flags.contains(Flags::DIM));
        assert!(!c.flags.contains(Flags::ITALIC));
        assert!(!c.flags.contains(Flags::UNDERLINE));
        assert!(!c.flags.contains(Flags::INVERSE));
        assert!(!c.flags.contains(Flags::HIDDEN));
        assert!(!c.flags.contains(Flags::STRIKEOUT));
    }

    /// SGR 21 sets DOUBLE_UNDERLINE (standard behavior -- BEHAVIOR CHANGE from old bold-off).
    #[test]
    fn sgr_21_sets_double_underline() {
        let mut t = term();
        feed(&mut t, "\x1b[1m"); // bold on
        feed(&mut t, "\x1b[21m"); // double underline (NOT bold off)
        feed(&mut t, "X");
        let c = t.screen().cell(0, 0);
        assert!(c.flags.contains(Flags::DOUBLE_UNDERLINE));
        // Bold is NOT reset by SGR 21 (only by SGR 22).
        assert!(c.flags.contains(Flags::BOLD));
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
        feed(&mut t, "\x1b[38;9;1;2;3mX\x1b[0m");
        assert_eq!(t.screen().cell(0, 0).ch, 'X');
    }

    #[test]
    fn vpa_line_position_absolute() {
        let mut t = term();
        feed(&mut t, "\x1b[5d"); // VPA: move to row 5 (1-indexed)
        assert_eq!(t.screen().cursor.row, 4);
    }

    #[test]
    fn cha_cursor_character_absolute() {
        let mut t = term();
        feed(&mut t, "\x1b[10G"); // CHA: move to col 10 (1-indexed)
        assert_eq!(t.screen().cursor.col, 9);
    }

    #[test]
    fn reverse_index_at_top() {
        let mut t = Terminal::new(10, 3);
        feed(&mut t, "Line0\r\nLine1\r\nLine2");
        feed(&mut t, "\x1b[H"); // home
        feed(&mut t, "\x1bM"); // RI: reverse index at top -- should scroll down
        assert_eq!(t.screen().cell(0, 0).ch, ' '); // blank line inserted
        assert_eq!(t.screen().cell(1, 0).ch, 'L'); // old line 0
    }

    #[test]
    fn reverse_index_not_at_top() {
        let mut t = Terminal::new(10, 3);
        feed(&mut t, "\x1b[2;1H"); // row 2
        feed(&mut t, "\x1bM"); // RI: just moves up
        assert_eq!(t.screen().cursor.row, 0);
    }

    #[test]
    fn scroll_up_command() {
        let mut t = Terminal::new(10, 3);
        feed(&mut t, "A\r\nB\r\nC");
        feed(&mut t, "\x1b[1S"); // SU: scroll up 1
        assert_eq!(t.screen().cell(0, 0).ch, 'B');
        assert_eq!(t.screen().cell(1, 0).ch, 'C');
        assert_eq!(t.screen().cell(2, 0).ch, ' ');
    }

    #[test]
    fn scroll_down_command() {
        let mut t = Terminal::new(10, 3);
        feed(&mut t, "A\r\nB\r\nC");
        feed(&mut t, "\x1b[1T"); // SD: scroll down 1
        assert_eq!(t.screen().cell(0, 0).ch, ' ');
        assert_eq!(t.screen().cell(1, 0).ch, 'A');
        assert_eq!(t.screen().cell(2, 0).ch, 'B');
    }

    #[test]
    fn esc_d_index() {
        let mut t = Terminal::new(10, 3);
        feed(&mut t, "\x1b[3;1H"); // last row
        feed(&mut t, "\x1bD"); // IND: index (same as LF)
        // At bottom, should scroll up.
        assert_eq!(t.screen().cursor.row, 2);
    }

    #[test]
    fn origin_mode() {
        let mut t = Terminal::new(80, 24);
        feed(&mut t, "\x1b[?6h"); // DECOM on
        assert!(t.modes.origin);
        feed(&mut t, "\x1b[?6l"); // DECOM off
        assert!(!t.modes.origin);
    }

    #[test]
    fn sgr_underline_subparams() {
        // SGR 4:2 -> DOUBLE_UNDERLINE
        let mut t = term();
        feed(&mut t, "\x1b[4:2mA");
        let cell = t.screen().cell(0, 0);
        assert!(cell.flags.contains(Flags::DOUBLE_UNDERLINE));
        assert_eq!(cell.flags & Flags::ALL_UNDERLINES, Flags::DOUBLE_UNDERLINE);

        // SGR 4:3 -> UNDERCURL
        let mut t = term();
        feed(&mut t, "\x1b[4:3mB");
        let cell = t.screen().cell(0, 0);
        assert!(cell.flags.contains(Flags::UNDERCURL));
        assert_eq!(cell.flags & Flags::ALL_UNDERLINES, Flags::UNDERCURL);

        // SGR 4:4 -> DOTTED_UNDERLINE
        let mut t = term();
        feed(&mut t, "\x1b[4:4mC");
        let cell = t.screen().cell(0, 0);
        assert!(cell.flags.contains(Flags::DOTTED_UNDERLINE));
        assert_eq!(cell.flags & Flags::ALL_UNDERLINES, Flags::DOTTED_UNDERLINE);

        // SGR 4:5 -> DASHED_UNDERLINE
        let mut t = term();
        feed(&mut t, "\x1b[4:5mD");
        let cell = t.screen().cell(0, 0);
        assert!(cell.flags.contains(Flags::DASHED_UNDERLINE));
        assert_eq!(cell.flags & Flags::ALL_UNDERLINES, Flags::DASHED_UNDERLINE);

        // SGR 4:0 -> remove underline
        let mut t = term();
        feed(&mut t, "\x1b[4m"); // set underline first
        feed(&mut t, "\x1b[4:0mE");
        let cell = t.screen().cell(0, 0);
        assert!(!cell.flags.intersects(Flags::ALL_UNDERLINES));

        // SGR 4:1 -> UNDERLINE (same as plain SGR 4)
        let mut t = term();
        feed(&mut t, "\x1b[4:1mF");
        let cell = t.screen().cell(0, 0);
        assert!(cell.flags.contains(Flags::UNDERLINE));
        assert_eq!(cell.flags & Flags::ALL_UNDERLINES, Flags::UNDERLINE);
    }

    #[test]
    fn sgr_24_clears_all_underlines() {
        let mut t = term();
        feed(&mut t, "\x1b[4:3m"); // set UNDERCURL
        feed(&mut t, "\x1b[24mA"); // SGR 24 clears all underlines
        let cell = t.screen().cell(0, 0);
        assert!(!cell.flags.intersects(Flags::ALL_UNDERLINES));
    }

    #[test]
    fn sgr_4_colon_0_clears_underline() {
        let mut t = term();
        feed(&mut t, "\x1b[4m"); // set UNDERLINE
        feed(&mut t, "\x1b[4:0mA"); // SGR 4:0 clears underline
        let cell = t.screen().cell(0, 0);
        assert!(!cell.flags.intersects(Flags::ALL_UNDERLINES));
    }

    #[test]
    fn sgr_underline_style_switches() {
        let mut t = term();
        feed(&mut t, "\x1b[4m"); // set UNDERLINE
        feed(&mut t, "\x1b[4:3mA"); // switch to UNDERCURL
        let cell = t.screen().cell(0, 0);
        assert!(cell.flags.contains(Flags::UNDERCURL));
        assert!(!cell.flags.contains(Flags::UNDERLINE));
        assert_eq!(cell.flags & Flags::ALL_UNDERLINES, Flags::UNDERCURL);
    }

    #[test]
    fn bold_italic_combination() {
        let mut t = term();
        feed(&mut t, "\x1b[1;3mA"); // SGR 1;3 = BOLD + ITALIC
        let cell = t.screen().cell(0, 0);
        assert!(cell.flags.contains(Flags::BOLD));
        assert!(cell.flags.contains(Flags::ITALIC));
        assert!(cell.flags.contains(Flags::BOLD_ITALIC));
    }

    // ==================== P0 Tests ====================

    #[test]
    fn da1_primary_device_attributes() {
        let mut t = term();
        feed(&mut t, "\x1b[c"); // DA1: CSI c
        let response = t.take_response();
        assert_eq!(response, b"\x1b[?62;22c");
    }

    #[test]
    fn da1_with_zero_param() {
        let mut t = term();
        feed(&mut t, "\x1b[0c"); // DA1: CSI 0 c
        let response = t.take_response();
        assert_eq!(response, b"\x1b[?62;22c");
    }

    #[test]
    fn osc_0_sets_title_and_icon() {
        let mut t = term();
        feed(&mut t, "\x1b]0;My Title\x07");
        assert_eq!(t.title.as_deref(), Some("My Title"));
        assert_eq!(t.icon_name.as_deref(), Some("My Title"));
    }

    #[test]
    fn osc_1_sets_icon_name() {
        let mut t = term();
        feed(&mut t, "\x1b]1;MyIcon\x07");
        assert_eq!(t.icon_name.as_deref(), Some("MyIcon"));
        assert!(t.title.is_none());
    }

    #[test]
    fn osc_2_sets_title() {
        let mut t = term();
        feed(&mut t, "\x1b]2;Window Title\x07");
        assert_eq!(t.title.as_deref(), Some("Window Title"));
        assert!(t.icon_name.is_none());
    }

    #[test]
    fn osc_st_terminated() {
        let mut t = term();
        // OSC terminated with ST (ESC \) instead of BEL.
        feed(&mut t, "\x1b]0;ST Title\x1b\\");
        assert_eq!(t.title.as_deref(), Some("ST Title"));
    }

    // ==================== P1 Tests ====================

    #[test]
    fn cnl_cursor_next_line() {
        let mut t = term();
        feed(&mut t, "\x1b[5;10H"); // row 5, col 10
        feed(&mut t, "\x1b[2E"); // CNL: move down 2 lines + CR
        assert_eq!(t.screen().cursor.row, 6); // 4 + 2
        assert_eq!(t.screen().cursor.col, 0); // CR
    }

    #[test]
    fn cpl_cursor_previous_line() {
        let mut t = term();
        feed(&mut t, "\x1b[5;10H"); // row 5, col 10
        feed(&mut t, "\x1b[2F"); // CPL: move up 2 lines + CR
        assert_eq!(t.screen().cursor.row, 2); // 4 - 2
        assert_eq!(t.screen().cursor.col, 0); // CR
    }

    #[test]
    fn ech_erase_characters() {
        let mut t = Terminal::new(10, 1);
        feed(&mut t, "ABCDEFGH");
        feed(&mut t, "\x1b[4G"); // col 4 (0-indexed: 3)
        feed(&mut t, "\x1b[3X"); // ECH: erase 3 chars at cursor
        assert_eq!(t.screen().cell(0, 2).ch, 'C'); // not erased
        assert_eq!(t.screen().cell(0, 3).ch, ' '); // erased
        assert_eq!(t.screen().cell(0, 4).ch, ' '); // erased
        assert_eq!(t.screen().cell(0, 5).ch, ' '); // erased
        assert_eq!(t.screen().cell(0, 6).ch, 'G'); // not erased
        // Cursor should NOT move.
        assert_eq!(t.screen().cursor.col, 3);
    }

    #[test]
    fn hpa_horizontal_position_absolute() {
        let mut t = term();
        feed(&mut t, "\x1b[15`"); // HPA: go to column 15 (1-indexed)
        assert_eq!(t.screen().cursor.col, 14);
    }

    #[test]
    fn cht_cursor_forward_tab() {
        let mut t = term();
        feed(&mut t, "\x1b[2I"); // CHT: forward 2 tab stops
        assert_eq!(t.screen().cursor.col, 16); // 0 -> 8 -> 16
    }

    #[test]
    fn cbt_cursor_backward_tab() {
        let mut t = term();
        feed(&mut t, "\x1b[20G"); // col 20 (0-indexed: 19)
        feed(&mut t, "\x1b[2Z"); // CBT: backward 2 tab stops
        assert_eq!(t.screen().cursor.col, 8); // 19 -> 16 -> 8
    }

    #[test]
    fn hts_horizontal_tab_set() {
        let mut t = term();
        feed(&mut t, "\x1b[6G"); // col 6 (0-indexed: 5)
        feed(&mut t, "\x1bH"); // HTS: set tab stop at col 5
        feed(&mut t, "\x1b[1G"); // col 1 (0-indexed: 0)
        feed(&mut t, "\t"); // tab -- should go to col 5 (custom stop)
        assert_eq!(t.screen().cursor.col, 5);
    }

    #[test]
    fn tbc_clear_tab_at_cursor() {
        let mut t = term();
        // Default tab stop at col 8.
        feed(&mut t, "\x1b[9G"); // col 9 (0-indexed: 8)
        feed(&mut t, "\x1b[0g"); // TBC mode 0: clear at cursor
        feed(&mut t, "\x1b[1G"); // go to col 0
        feed(&mut t, "\t"); // tab -- should skip col 8, go to 16
        assert_eq!(t.screen().cursor.col, 16);
    }

    #[test]
    fn tbc_clear_all_tabs() {
        let mut t = term();
        feed(&mut t, "\x1b[3g"); // TBC mode 3: clear all tab stops
        feed(&mut t, "\x1b[1G"); // go to col 0
        feed(&mut t, "\t"); // tab -- no stops, go to last col
        assert_eq!(t.screen().cursor.col, 79);
    }

    #[test]
    fn dsr_5n_device_status() {
        let mut t = term();
        feed(&mut t, "\x1b[5n"); // DSR: status report
        let response = t.take_response();
        assert_eq!(response, b"\x1b[0n"); // terminal OK
    }

    #[test]
    fn da2_secondary_device_attributes() {
        let mut t = term();
        feed(&mut t, "\x1b[>c"); // DA2: CSI > c
        let response = t.take_response();
        assert_eq!(response, b"\x1b[>0;0;0c");
    }

    #[test]
    fn nel_next_line() {
        let mut t = term();
        feed(&mut t, "\x1b[5;10H"); // row 5, col 10
        feed(&mut t, "\x1bE"); // NEL: next line (LF + CR)
        assert_eq!(t.screen().cursor.row, 5); // was 4, now 5
        assert_eq!(t.screen().cursor.col, 0); // CR
    }

    #[test]
    fn bel_sets_bell_pending() {
        let mut t = term();
        assert!(!t.bell_pending);
        feed(&mut t, "\x07"); // BEL
        assert!(t.bell_pending);
        // Clear it.
        t.bell_pending = false;
        assert!(!t.bell_pending);
    }

    #[test]
    fn focus_events_mode() {
        let mut t = term();
        assert!(!t.modes.focus_events);
        feed(&mut t, "\x1b[?1004h"); // enable focus events
        assert!(t.modes.focus_events);
        feed(&mut t, "\x1b[?1004l"); // disable focus events
        assert!(!t.modes.focus_events);
    }

    #[test]
    fn scs_g0_dec_special_graphics() {
        let mut t = term();
        feed(&mut t, "\x1b(0"); // designate G0 as DEC Special Graphics
        assert_eq!(t.charset_g0, Charset::DecSpecialGraphics);
        // 'q' in DEC SG = horizontal line U+2500
        feed(&mut t, "q");
        assert_eq!(t.screen().cell(0, 0).ch, '\u{2500}');
        // 'l' = top-left corner U+250C
        feed(&mut t, "l");
        assert_eq!(t.screen().cell(0, 1).ch, '\u{250c}');
        // Switch back to ASCII.
        feed(&mut t, "\x1b(B");
        assert_eq!(t.charset_g0, Charset::Ascii);
        feed(&mut t, "q");
        assert_eq!(t.screen().cell(0, 2).ch, 'q'); // plain ASCII
    }

    #[test]
    fn scs_g1_and_shift_out_in() {
        let mut t = term();
        feed(&mut t, "\x1b)0"); // designate G1 as DEC Special Graphics
        assert_eq!(t.charset_g1, Charset::DecSpecialGraphics);
        // SO (0x0E) = shift to G1.
        feed(&mut t, "\x0e");
        assert_eq!(t.active_charset, 1);
        feed(&mut t, "x"); // vertical line in DEC SG
        assert_eq!(t.screen().cell(0, 0).ch, '\u{2502}');
        // SI (0x0F) = shift to G0 (ASCII).
        feed(&mut t, "\x0f");
        assert_eq!(t.active_charset, 0);
        feed(&mut t, "x");
        assert_eq!(t.screen().cell(0, 1).ch, 'x'); // plain ASCII
    }

    #[test]
    fn dec_special_graphics_full_mapping() {
        // Test all characters in 0x60..=0x7E range.
        let expected = [
            ('`', '\u{25c6}'),
            ('a', '\u{2592}'),
            ('b', '\u{2409}'),
            ('c', '\u{240c}'),
            ('d', '\u{240d}'),
            ('e', '\u{240a}'),
            ('f', '\u{00b0}'),
            ('g', '\u{00b1}'),
            ('h', '\u{2424}'),
            ('i', '\u{240b}'),
            ('j', '\u{2518}'),
            ('k', '\u{2510}'),
            ('l', '\u{250c}'),
            ('m', '\u{2514}'),
            ('n', '\u{253c}'),
            ('o', '\u{23ba}'),
            ('p', '\u{23bb}'),
            ('q', '\u{2500}'),
            ('r', '\u{23bc}'),
            ('s', '\u{23bd}'),
            ('t', '\u{251c}'),
            ('u', '\u{2524}'),
            ('v', '\u{2534}'),
            ('w', '\u{252c}'),
            ('x', '\u{2502}'),
            ('y', '\u{2264}'),
            ('z', '\u{2265}'),
            ('{', '\u{03c0}'),
            ('|', '\u{2260}'),
            ('}', '\u{00a3}'),
            ('~', '\u{00b7}'),
        ];
        for (input, expected_ch) in expected {
            assert_eq!(
                dec_special_graphics_char(input),
                Some(expected_ch),
                "DEC SG mapping for {:?}",
                input
            );
        }
        // underscore maps to space.
        assert_eq!(dec_special_graphics_char('_'), Some(' '));
        // Characters outside range return None.
        assert_eq!(dec_special_graphics_char('A'), None);
    }

    #[test]
    fn deckpam_deckpnm() {
        let mut t = term();
        assert!(!t.modes.application_keypad);
        feed(&mut t, "\x1b="); // DECKPAM
        assert!(t.modes.application_keypad);
        feed(&mut t, "\x1b>"); // DECKPNM
        assert!(!t.modes.application_keypad);
    }

    #[test]
    fn decsc_decrc_saves_pen_state() {
        let mut t = term();
        // Set pen state.
        feed(&mut t, "\x1b[1;31m"); // bold + red fg
        feed(&mut t, "\x1b[5;10H"); // row 5, col 10
        feed(&mut t, "\x1b(0"); // G0 = DEC Special Graphics
        feed(&mut t, "\x1b7"); // DECSC: save all
        // Change everything.
        feed(&mut t, "\x1b[0m"); // reset pen
        feed(&mut t, "\x1b[1;1H"); // move to 1,1
        feed(&mut t, "\x1b(B"); // G0 = ASCII
        // Restore.
        feed(&mut t, "\x1b8"); // DECRC: restore all
        assert_eq!(t.screen().cursor.row, 4);
        assert_eq!(t.screen().cursor.col, 9);
        assert!(t.screen().pen.flags.contains(Flags::BOLD));
        assert_eq!(t.screen().pen.fg, Color::Indexed(1));
        assert_eq!(t.charset_g0, Charset::DecSpecialGraphics);
    }

    #[test]
    fn decsc_decrc_saves_origin_mode() {
        let mut t = term();
        feed(&mut t, "\x1b[?6h"); // origin mode on
        feed(&mut t, "\x1b7"); // DECSC
        feed(&mut t, "\x1b[?6l"); // origin mode off
        assert!(!t.modes.origin);
        feed(&mut t, "\x1b8"); // DECRC
        assert!(t.modes.origin); // restored
    }

    #[test]
    fn soft_reset_resets_charsets() {
        let mut t = term();
        feed(&mut t, "\x1b(0"); // G0 = DEC SG
        feed(&mut t, "\x0e"); // SO -> G1
        feed(&mut t, "\x1b[!p"); // DECSTR: soft reset
        assert_eq!(t.charset_g0, Charset::Ascii);
        assert_eq!(t.charset_g1, Charset::Ascii);
        assert_eq!(t.active_charset, 0);
    }

    #[test]
    fn full_reset_clears_title() {
        let mut t = term();
        feed(&mut t, "\x1b]0;Hello\x07");
        assert!(t.title.is_some());
        feed(&mut t, "\x1bc"); // RIS
        assert!(t.title.is_none());
        assert!(t.icon_name.is_none());
    }

    #[test]
    fn full_reset_clears_bell() {
        let mut t = term();
        feed(&mut t, "\x07"); // BEL
        assert!(t.bell_pending);
        feed(&mut t, "\x1bc"); // RIS
        assert!(!t.bell_pending);
    }

    #[test]
    fn da1_does_not_respond_to_nonzero_param() {
        let mut t = term();
        feed(&mut t, "\x1b[1c"); // CSI 1 c -- not DA1, should be ignored
        let response = t.take_response();
        assert!(response.is_empty());
    }
}
