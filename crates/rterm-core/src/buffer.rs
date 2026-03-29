use crate::cell::{Cell, CellAttributes};
use crate::color::Color;
use std::collections::VecDeque;

/// Cursor position and state.
#[derive(Debug, Clone)]
pub struct Cursor {
    /// Column (0-indexed).
    pub col: usize,
    /// Row (0-indexed, relative to the viewport).
    pub row: usize,
    /// Whether the cursor is visible.
    pub visible: bool,
}

impl Default for Cursor {
    fn default() -> Self {
        Self {
            col: 0,
            row: 0,
            visible: true,
        }
    }
}

/// The current pen style applied to new characters.
#[derive(Debug, Clone, Default)]
pub struct Pen {
    pub fg: Color,
    pub bg: Color,
    pub attrs: CellAttributes,
}

/// Terminal screen buffer: a 2D grid of cells with cursor, scroll region,
/// scrollback, and the current pen (style for new characters).
pub struct ScreenBuffer {
    /// Number of columns.
    cols: usize,
    /// Number of visible rows.
    rows: usize,
    /// The active viewport grid: rows x cols.
    grid: Vec<Vec<Cell>>,
    /// Scrollback buffer (lines that scrolled off the top).
    scrollback: VecDeque<Vec<Cell>>,
    /// Maximum scrollback lines.
    max_scrollback: usize,
    /// Cursor position and visibility.
    pub cursor: Cursor,
    /// The current pen style for new characters.
    pub pen: Pen,
    /// Scroll region: top row (inclusive), bottom row (inclusive).
    /// Both 0-indexed. Defaults to (0, rows-1).
    scroll_top: usize,
    scroll_bottom: usize,
}

impl ScreenBuffer {
    /// Create a new screen buffer with the given dimensions.
    pub fn new(cols: usize, rows: usize) -> Self {
        assert!(cols > 0 && rows > 0, "dimensions must be > 0");
        let grid = vec![vec![Cell::default(); cols]; rows];
        Self {
            cols,
            rows,
            grid,
            scrollback: VecDeque::new(),
            max_scrollback: 10_000,
            cursor: Cursor::default(),
            pen: Pen::default(),
            scroll_top: 0,
            scroll_bottom: rows - 1,
        }
    }

    /// Resize the buffer to new dimensions.
    /// Preserves content where possible. New cells are blank.
    pub fn resize(&mut self, new_cols: usize, new_rows: usize) {
        assert!(new_cols > 0 && new_rows > 0, "dimensions must be > 0");

        // Resize each existing row to new_cols.
        for row in &mut self.grid {
            row.resize(new_cols, Cell::default());
        }

        // Add or remove rows.
        if new_rows > self.rows {
            for _ in self.rows..new_rows {
                self.grid.push(vec![Cell::default(); new_cols]);
            }
        } else if new_rows < self.rows {
            // Push excess rows to scrollback before removing.
            let excess = self.rows - new_rows;
            for i in 0..excess {
                if i < self.grid.len() {
                    // Only push non-empty rows to scrollback.
                    let is_empty = self.grid[0].iter().all(|c| c.ch == ' ');
                    if !is_empty {
                        self.scrollback.push_back(self.grid.remove(0));
                    } else {
                        self.grid.remove(0);
                    }
                }
            }
            self.grid.truncate(new_rows);
        }

        self.cols = new_cols;
        self.rows = new_rows;

        // Clamp cursor.
        self.cursor.row = self.cursor.row.min(new_rows - 1);
        self.cursor.col = self.cursor.col.min(new_cols - 1);

        // Reset scroll region to full screen.
        self.scroll_top = 0;
        self.scroll_bottom = new_rows - 1;

        self.trim_scrollback();
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    /// Set the maximum scrollback line count.
    pub fn set_max_scrollback(&mut self, max: usize) {
        self.max_scrollback = max;
        self.trim_scrollback();
    }

    pub fn scrollback_len(&self) -> usize {
        self.scrollback.len()
    }

    /// Get a cell from the scrollback buffer.
    /// `line` is 0-indexed from the top of scrollback (oldest line).
    pub fn scrollback_cell(&self, line: usize, col: usize) -> &Cell {
        if line < self.scrollback.len() && col < self.scrollback[line].len() {
            &self.scrollback[line][col]
        } else {
            // Return a default cell for out-of-bounds.
            // Using a static default since we return a reference.
            static DEFAULT: Cell = Cell {
                ch: ' ',
                fg: Color::Default,
                bg: Color::Default,
                attrs: CellAttributes::NORMAL,
            };
            &DEFAULT
        }
    }

    /// Get the width of a scrollback line.
    pub fn scrollback_cols(&self, line: usize) -> usize {
        if line < self.scrollback.len() {
            self.scrollback[line].len()
        } else {
            0
        }
    }

    /// Get text content of a scrollback line.
    pub fn scrollback_text(&self, line: usize) -> String {
        if line < self.scrollback.len() {
            let text: String = self.scrollback[line].iter().map(|c| c.ch).collect();
            text.trim_end().to_string()
        } else {
            String::new()
        }
    }

    /// Get a reference to a cell at (row, col).
    pub fn cell(&self, row: usize, col: usize) -> &Cell {
        &self.grid[row][col]
    }

    /// Get a mutable reference to a cell at (row, col).
    pub fn cell_mut(&mut self, row: usize, col: usize) -> &mut Cell {
        &mut self.grid[row][col]
    }

    /// Write a character at the current cursor position using the current pen,
    /// then advance the cursor.
    pub fn write_char(&mut self, ch: char) {
        if self.cursor.col >= self.cols {
            // Autowrap: move to next line.
            // TODO: autowrap mode flag. For now always wrap.
            self.cursor.col = 0;
            self.cursor_down_with_scroll();
        }

        let row = self.cursor.row;
        let col = self.cursor.col;
        self.grid[row][col] = Cell {
            ch,
            fg: self.pen.fg,
            bg: self.pen.bg,
            attrs: self.pen.attrs,
        };
        self.cursor.col += 1;
    }

    // --- Cursor Movement ---

    /// Move cursor up by `n` rows, clamped at scroll top (or row 0).
    pub fn cursor_up(&mut self, n: usize) {
        let min_row = if self.cursor.row >= self.scroll_top {
            self.scroll_top
        } else {
            0
        };
        self.cursor.row = self.cursor.row.saturating_sub(n).max(min_row);
    }

    /// Move cursor down by `n` rows, clamped at scroll bottom (or last row).
    pub fn cursor_down(&mut self, n: usize) {
        let max_row = if self.cursor.row <= self.scroll_bottom {
            self.scroll_bottom
        } else {
            self.rows - 1
        };
        self.cursor.row = (self.cursor.row + n).min(max_row);
    }

    /// Move cursor forward (right) by `n` columns, clamped at last column.
    pub fn cursor_forward(&mut self, n: usize) {
        self.cursor.col = (self.cursor.col + n).min(self.cols - 1);
    }

    /// Move cursor backward (left) by `n` columns, clamped at column 0.
    pub fn cursor_back(&mut self, n: usize) {
        self.cursor.col = self.cursor.col.saturating_sub(n);
    }

    /// Set cursor position (1-indexed row, col as received from VT sequences).
    /// Clamps to valid range.
    pub fn set_cursor_pos(&mut self, row_1: usize, col_1: usize) {
        self.cursor.row = row_1.saturating_sub(1).min(self.rows - 1);
        self.cursor.col = col_1.saturating_sub(1).min(self.cols - 1);
    }

    /// Move cursor down, scrolling the scroll region if at the bottom.
    fn cursor_down_with_scroll(&mut self) {
        if self.cursor.row == self.scroll_bottom {
            self.scroll_up(1);
        } else if self.cursor.row < self.rows - 1 {
            self.cursor.row += 1;
        }
    }

    /// Carriage return: move cursor to column 0.
    pub fn carriage_return(&mut self) {
        self.cursor.col = 0;
    }

    /// Line feed: move cursor down, scroll if needed.
    pub fn line_feed(&mut self) {
        self.cursor_down_with_scroll();
    }

    // --- Scroll ---

    /// Set the scroll region (1-indexed top and bottom, inclusive).
    /// Resets cursor to top-left of the scroll region.
    pub fn set_scroll_region(&mut self, top_1: usize, bottom_1: usize) {
        let top = top_1.saturating_sub(1).min(self.rows - 1);
        let bottom = bottom_1.saturating_sub(1).min(self.rows - 1);
        if top < bottom {
            self.scroll_top = top;
            self.scroll_bottom = bottom;
            self.cursor.row = top;
            self.cursor.col = 0;
        }
    }

    /// Scroll the scroll region up by `n` lines.
    /// Top lines are pushed to scrollback, bottom lines become blank.
    pub fn scroll_up(&mut self, n: usize) {
        let top = self.scroll_top;
        let bottom = self.scroll_bottom;
        let n = n.min(bottom - top + 1);

        for i in 0..n {
            // Push the line going off the top to scrollback (only if scroll region is full screen).
            if top == 0 {
                let line = self.grid[top + i].clone();
                self.scrollback.push_back(line);
            }
        }
        self.trim_scrollback();

        // Shift lines up within the scroll region.
        for row in top..=bottom {
            if row + n <= bottom {
                self.grid[row] = self.grid[row + n].clone();
            } else {
                self.grid[row] = vec![Cell::default(); self.cols];
            }
        }
    }

    /// Scroll the scroll region down by `n` lines.
    /// Bottom lines are discarded, top lines become blank.
    pub fn scroll_down(&mut self, n: usize) {
        let top = self.scroll_top;
        let bottom = self.scroll_bottom;
        let n = n.min(bottom - top + 1);

        // Shift lines down within the scroll region.
        for row in (top..=bottom).rev() {
            if row >= top + n {
                self.grid[row] = self.grid[row - n].clone();
            } else {
                self.grid[row] = vec![Cell::default(); self.cols];
            }
        }
    }

    fn trim_scrollback(&mut self) {
        while self.scrollback.len() > self.max_scrollback {
            self.scrollback.pop_front();
        }
    }

    // --- Erase ---

    /// Erase in display (ED).
    /// mode 0: from cursor to end of screen.
    /// mode 1: from start of screen to cursor.
    /// mode 2: entire screen.
    pub fn erase_in_display(&mut self, mode: u16) {
        match mode {
            0 => {
                // Cursor to end: clear rest of current line, then all lines below.
                self.erase_in_line(0);
                for row in (self.cursor.row + 1)..self.rows {
                    self.clear_row(row);
                }
            }
            1 => {
                // Start to cursor: clear lines above, then start of current line.
                for row in 0..self.cursor.row {
                    self.clear_row(row);
                }
                self.erase_in_line(1);
            }
            2 => {
                // Entire screen.
                for row in 0..self.rows {
                    self.clear_row(row);
                }
            }
            _ => {}
        }
    }

    /// Erase in line (EL).
    /// mode 0: from cursor to end of line.
    /// mode 1: from start of line to cursor.
    /// mode 2: entire line.
    pub fn erase_in_line(&mut self, mode: u16) {
        let row = self.cursor.row;
        match mode {
            0 => {
                for col in self.cursor.col..self.cols {
                    self.grid[row][col].reset();
                }
            }
            1 => {
                for col in 0..=self.cursor.col.min(self.cols - 1) {
                    self.grid[row][col].reset();
                }
            }
            2 => {
                self.clear_row(row);
            }
            _ => {}
        }
    }

    fn clear_row(&mut self, row: usize) {
        for col in 0..self.cols {
            self.grid[row][col].reset();
        }
    }

    // --- Insert / Delete ---

    /// Insert `n` blank lines at the cursor row, shifting lines down.
    /// Lines that fall off the scroll bottom are discarded.
    pub fn insert_lines(&mut self, n: usize) {
        let row = self.cursor.row;
        if row < self.scroll_top || row > self.scroll_bottom {
            return;
        }
        let bottom = self.scroll_bottom;
        let n = n.min(bottom - row + 1);

        for r in (row..=bottom).rev() {
            if r >= row + n {
                self.grid[r] = self.grid[r - n].clone();
            } else {
                self.grid[r] = vec![Cell::default(); self.cols];
            }
        }
    }

    /// Delete `n` lines at the cursor row, shifting lines up.
    /// Blank lines appear at the scroll bottom.
    pub fn delete_lines(&mut self, n: usize) {
        let row = self.cursor.row;
        if row < self.scroll_top || row > self.scroll_bottom {
            return;
        }
        let bottom = self.scroll_bottom;
        let n = n.min(bottom - row + 1);

        for r in row..=bottom {
            if r + n <= bottom {
                self.grid[r] = self.grid[r + n].clone();
            } else {
                self.grid[r] = vec![Cell::default(); self.cols];
            }
        }
    }

    /// Insert `n` blank characters at the cursor, shifting existing chars right.
    pub fn insert_chars(&mut self, n: usize) {
        let row = self.cursor.row;
        let col = self.cursor.col;
        let n = n.min(self.cols - col);

        // Shift right.
        for c in (col..self.cols).rev() {
            if c >= col + n {
                self.grid[row][c] = self.grid[row][c - n];
            } else {
                self.grid[row][c] = Cell::default();
            }
        }
    }

    /// Delete `n` characters at the cursor, shifting remaining chars left.
    pub fn delete_chars(&mut self, n: usize) {
        let row = self.cursor.row;
        let col = self.cursor.col;
        let n = n.min(self.cols - col);

        for c in col..self.cols {
            if c + n < self.cols {
                self.grid[row][c] = self.grid[row][c + n];
            } else {
                self.grid[row][c] = Cell::default();
            }
        }
    }

    // --- Reset ---

    /// Reset the buffer to initial state.
    pub fn reset(&mut self) {
        for row in 0..self.rows {
            self.clear_row(row);
        }
        self.cursor = Cursor::default();
        self.pen = Pen::default();
        self.scroll_top = 0;
        self.scroll_bottom = self.rows - 1;
    }

    /// Extract the text content of a row as a string (trimming trailing spaces).
    pub fn row_text(&self, row: usize) -> String {
        let text: String = self.grid[row].iter().map(|c| c.ch).collect();
        text.trim_end().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_buffer_dimensions() {
        let buf = ScreenBuffer::new(80, 24);
        assert_eq!(buf.cols(), 80);
        assert_eq!(buf.rows(), 24);
    }

    #[test]
    fn default_cell_is_blank() {
        let buf = ScreenBuffer::new(80, 24);
        assert_eq!(buf.cell(0, 0).ch, ' ');
    }

    #[test]
    fn write_char_at_cursor() {
        let mut buf = ScreenBuffer::new(80, 24);
        buf.write_char('A');
        assert_eq!(buf.cell(0, 0).ch, 'A');
        assert_eq!(buf.cursor.col, 1);
        assert_eq!(buf.cursor.row, 0);
    }

    #[test]
    fn write_string() {
        let mut buf = ScreenBuffer::new(80, 24);
        for ch in "Hello".chars() {
            buf.write_char(ch);
        }
        assert_eq!(buf.row_text(0), "Hello");
        assert_eq!(buf.cursor.col, 5);
    }

    #[test]
    fn write_char_with_pen() {
        let mut buf = ScreenBuffer::new(80, 24);
        buf.pen.fg = Color::RED;
        buf.pen.attrs.bold = true;
        buf.write_char('X');
        let cell = buf.cell(0, 0);
        assert_eq!(cell.fg, Color::RED);
        assert!(cell.attrs.bold);
    }

    #[test]
    fn autowrap_at_end_of_line() {
        let mut buf = ScreenBuffer::new(5, 3);
        for ch in "ABCDE".chars() {
            buf.write_char(ch);
        }
        assert_eq!(buf.row_text(0), "ABCDE");
        assert_eq!(buf.cursor.col, 5); // one past end
        // Next char wraps.
        buf.write_char('F');
        assert_eq!(buf.cursor.row, 1);
        assert_eq!(buf.cursor.col, 1);
        assert_eq!(buf.cell(1, 0).ch, 'F');
    }

    #[test]
    fn cursor_movement() {
        let mut buf = ScreenBuffer::new(80, 24);
        buf.set_cursor_pos(5, 10); // row 4 (0-indexed), col 9
        assert_eq!(buf.cursor.row, 4);
        assert_eq!(buf.cursor.col, 9);

        buf.cursor_up(2);
        assert_eq!(buf.cursor.row, 2);

        buf.cursor_down(10);
        assert_eq!(buf.cursor.row, 12);

        buf.cursor_forward(5);
        assert_eq!(buf.cursor.col, 14);

        buf.cursor_back(3);
        assert_eq!(buf.cursor.col, 11);
    }

    #[test]
    fn cursor_clamps_to_bounds() {
        let mut buf = ScreenBuffer::new(80, 24);
        buf.cursor_up(100);
        assert_eq!(buf.cursor.row, 0);

        buf.cursor_down(100);
        assert_eq!(buf.cursor.row, 23);

        buf.cursor_back(100);
        assert_eq!(buf.cursor.col, 0);

        buf.cursor_forward(200);
        assert_eq!(buf.cursor.col, 79);
    }

    #[test]
    fn carriage_return() {
        let mut buf = ScreenBuffer::new(80, 24);
        buf.cursor.col = 40;
        buf.carriage_return();
        assert_eq!(buf.cursor.col, 0);
    }

    #[test]
    fn line_feed_scrolls_at_bottom() {
        let mut buf = ScreenBuffer::new(5, 3);
        // Write on each row.
        for ch in "A".chars() {
            buf.write_char(ch);
        }
        buf.set_cursor_pos(2, 1);
        for ch in "B".chars() {
            buf.write_char(ch);
        }
        buf.set_cursor_pos(3, 1);
        for ch in "C".chars() {
            buf.write_char(ch);
        }

        // Now at row 2 (last row). Line feed should scroll.
        buf.line_feed();
        // Row 0 should now have "B" (was row 1), row 1 should have "C" (was row 2).
        assert_eq!(buf.row_text(0), "B");
        assert_eq!(buf.row_text(1), "C");
        assert_eq!(buf.row_text(2), ""); // blank
        assert_eq!(buf.scrollback_len(), 1);
    }

    #[test]
    fn erase_in_display_mode_2() {
        let mut buf = ScreenBuffer::new(10, 3);
        for ch in "Hello".chars() {
            buf.write_char(ch);
        }
        buf.erase_in_display(2);
        assert_eq!(buf.row_text(0), "");
    }

    #[test]
    fn erase_in_line_mode_0() {
        let mut buf = ScreenBuffer::new(10, 3);
        for ch in "Hello".chars() {
            buf.write_char(ch);
        }
        buf.cursor.col = 2;
        buf.erase_in_line(0); // erase from cursor to end
        assert_eq!(buf.row_text(0), "He");
    }

    #[test]
    fn erase_in_line_mode_1() {
        let mut buf = ScreenBuffer::new(10, 3);
        for ch in "Hello".chars() {
            buf.write_char(ch);
        }
        buf.cursor.col = 2;
        buf.erase_in_line(1); // erase from start to cursor
        assert_eq!(buf.cell(0, 0).ch, ' ');
        assert_eq!(buf.cell(0, 1).ch, ' ');
        assert_eq!(buf.cell(0, 2).ch, ' ');
        assert_eq!(buf.cell(0, 3).ch, 'l');
    }

    #[test]
    fn scroll_region() {
        let mut buf = ScreenBuffer::new(10, 5);
        // Fill rows.
        for row in 0..5 {
            buf.set_cursor_pos(row + 1, 1);
            buf.write_char((b'A' + row as u8) as char);
        }
        // Set scroll region to rows 2-4 (1-indexed).
        buf.set_scroll_region(2, 4);
        // Scroll up within region.
        buf.scroll_up(1);
        // Row 1 (index 1) should now have what was row 2 (index 2).
        assert_eq!(buf.cell(1, 0).ch, 'C');
        assert_eq!(buf.cell(2, 0).ch, 'D');
        assert_eq!(buf.cell(3, 0).ch, ' '); // blank
        // Rows outside region unchanged.
        assert_eq!(buf.cell(0, 0).ch, 'A');
        assert_eq!(buf.cell(4, 0).ch, 'E');
    }

    #[test]
    fn insert_lines() {
        let mut buf = ScreenBuffer::new(5, 4);
        for row in 0..4 {
            buf.set_cursor_pos(row + 1, 1);
            buf.write_char((b'A' + row as u8) as char);
        }
        buf.set_cursor_pos(2, 1); // row 1
        buf.insert_lines(1);
        assert_eq!(buf.cell(0, 0).ch, 'A');
        assert_eq!(buf.cell(1, 0).ch, ' '); // inserted blank
        assert_eq!(buf.cell(2, 0).ch, 'B'); // shifted down
        assert_eq!(buf.cell(3, 0).ch, 'C'); // D fell off
    }

    #[test]
    fn delete_lines() {
        let mut buf = ScreenBuffer::new(5, 4);
        for row in 0..4 {
            buf.set_cursor_pos(row + 1, 1);
            buf.write_char((b'A' + row as u8) as char);
        }
        buf.set_cursor_pos(2, 1); // row 1
        buf.delete_lines(1);
        assert_eq!(buf.cell(0, 0).ch, 'A');
        assert_eq!(buf.cell(1, 0).ch, 'C'); // shifted up
        assert_eq!(buf.cell(2, 0).ch, 'D');
        assert_eq!(buf.cell(3, 0).ch, ' '); // blank at bottom
    }

    #[test]
    fn insert_chars() {
        let mut buf = ScreenBuffer::new(5, 1);
        for ch in "ABCDE".chars() {
            buf.write_char(ch);
        }
        buf.cursor.col = 1;
        buf.insert_chars(2);
        assert_eq!(buf.cell(0, 0).ch, 'A');
        assert_eq!(buf.cell(0, 1).ch, ' ');
        assert_eq!(buf.cell(0, 2).ch, ' ');
        assert_eq!(buf.cell(0, 3).ch, 'B');
        assert_eq!(buf.cell(0, 4).ch, 'C');
    }

    #[test]
    fn delete_chars() {
        let mut buf = ScreenBuffer::new(5, 1);
        for ch in "ABCDE".chars() {
            buf.write_char(ch);
        }
        buf.cursor.col = 1;
        buf.delete_chars(2);
        assert_eq!(buf.cell(0, 0).ch, 'A');
        assert_eq!(buf.cell(0, 1).ch, 'D');
        assert_eq!(buf.cell(0, 2).ch, 'E');
        assert_eq!(buf.cell(0, 3).ch, ' ');
        assert_eq!(buf.cell(0, 4).ch, ' ');
    }

    #[test]
    fn scrollback_accumulates() {
        let mut buf = ScreenBuffer::new(5, 2);
        buf.set_max_scrollback(3);
        // Write lines and scroll them off. Cursor starts at row 0.
        for i in 0..5 {
            buf.write_char((b'A' + i) as char);
            buf.carriage_return();
            buf.line_feed(); // moves down, scrolls when at bottom
        }
        // 2-row buffer, 5 line feeds: first LF goes to row 1.
        // LFs 2-5 scroll up (4 scrolls), but only 3 kept due to max_scrollback.
        assert_eq!(buf.scrollback_len(), 3);
    }

    #[test]
    fn reset_clears_everything() {
        let mut buf = ScreenBuffer::new(10, 5);
        buf.write_char('X');
        buf.pen.fg = Color::RED;
        buf.set_scroll_region(2, 4);
        buf.reset();
        assert_eq!(buf.row_text(0), "");
        assert_eq!(buf.cursor.row, 0);
        assert_eq!(buf.cursor.col, 0);
        assert_eq!(buf.pen.fg, Color::Default);
    }

    #[test]
    fn set_cursor_pos_1_indexed() {
        let mut buf = ScreenBuffer::new(80, 24);
        buf.set_cursor_pos(1, 1); // top-left
        assert_eq!(buf.cursor.row, 0);
        assert_eq!(buf.cursor.col, 0);

        buf.set_cursor_pos(24, 80); // bottom-right
        assert_eq!(buf.cursor.row, 23);
        assert_eq!(buf.cursor.col, 79);
    }

    #[test]
    fn scroll_down_in_region() {
        let mut buf = ScreenBuffer::new(5, 4);
        for row in 0..4 {
            buf.set_cursor_pos(row + 1, 1);
            buf.write_char((b'A' + row as u8) as char);
        }
        buf.set_scroll_region(2, 4); // rows 1-3 (0-indexed)
        buf.scroll_down(1);
        assert_eq!(buf.cell(0, 0).ch, 'A'); // outside region, unchanged
        assert_eq!(buf.cell(1, 0).ch, ' '); // blank (scrolled in)
        assert_eq!(buf.cell(2, 0).ch, 'B'); // was row 1
        assert_eq!(buf.cell(3, 0).ch, 'C'); // was row 2, D fell off
    }
}
