use crate::cell::{Cell, Flags};
use crate::color::Color;
use crate::grid::{CellRange, Column, Grid, Line, Point};

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
    pub flags: Flags,
}

/// Terminal screen buffer: a 2D grid of cells with cursor, scroll region,
/// scrollback, and the current pen (style for new characters).
pub struct ScreenBuffer {
    /// The grid stores cells indexed by Line (i32) and Column (usize).
    /// Viewport lines are Line(0) through Line(rows-1).
    grid: Grid<Cell>,

    /// Scrollback buffer: rows that have scrolled off the top of the viewport.
    /// Each entry is (line_index, cells) where line_index is the original
    /// line number when it was visible.
    scrollback: Vec<Vec<Cell>>,

    /// Cursor position and visibility.
    pub cursor: Cursor,
    /// The current pen style for new characters.
    pub pen: Pen,
    /// Scroll region: top row (inclusive), bottom row (inclusive).
    /// Both 0-indexed. Defaults to (0, rows-1).
    scroll_top: usize,
    scroll_bottom: usize,
    /// Tab stops: one entry per column. `true` = tab stop set.
    /// Initialized with stops every 8 columns.
    tab_stops: Vec<bool>,
}

/// Build initial tab stops for `cols` columns (every 8 columns).
fn default_tab_stops(cols: usize) -> Vec<bool> {
    (0..cols).map(|c| c > 0 && c % 8 == 0).collect()
}

impl ScreenBuffer {
    /// Create a new screen buffer with the given dimensions.
    pub fn new(cols: usize, rows: usize) -> Self {
        assert!(cols > 0 && rows > 0, "dimensions must be > 0");
        let template = Cell::default();
        let grid = Grid::new(&template, rows, cols);
        Self {
            grid,
            scrollback: Vec::new(),
            cursor: Cursor::default(),
            pen: Pen::default(),
            scroll_top: 0,
            scroll_bottom: rows - 1,
            tab_stops: default_tab_stops(cols),
        }
    }

    /// Resize the buffer to new dimensions.
    /// Preserves content where possible. New cells are blank.
    /// Clears scrollback since buffer content is invalidated.
    pub fn resize(&mut self, new_cols: usize, new_rows: usize) {
        assert!(new_cols > 0 && new_rows > 0, "dimensions must be > 0");
        let template = Cell::default();
        self.grid.resize(&template, new_rows, new_cols);

        // Clamp cursor.
        self.cursor.row = self.cursor.row.min(new_rows.saturating_sub(1));
        self.cursor.col = self.cursor.col.min(new_cols.saturating_sub(1));

        // Reset scroll region to full screen.
        self.scroll_top = 0;
        self.scroll_bottom = new_rows - 1;

        // Rebuild tab stops.
        self.tab_stops = default_tab_stops(new_cols);

        // Clear scrollback since buffer dimensions changed.
        self.scrollback.clear();
    }

    pub fn cols(&self) -> usize {
        self.grid.columns()
    }

    pub fn rows(&self) -> usize {
        self.grid.lines()
    }

    /// Get a reference to a cell at (row, col).
    pub fn cell(&self, row: usize, col: usize) -> &Cell {
        let point = Point::new(Line(row as i32), Column(col));
        // Grid's cell method returns Option<&T>, we expect valid coordinates
        self.grid.cell(point).expect("cell out of bounds")
    }

    /// Get a mutable reference to a cell at (row, col).
    pub fn cell_mut(&mut self, row: usize, col: usize) -> &mut Cell {
        let point = Point::new(Line(row as i32), Column(col));
        // Grid's cell_mut method returns Option<&mut T>
        self.grid.cell_mut(point).expect("cell out of bounds")
    }

    /// Get the grid's display offset for screen_diff.
    pub fn display_offset(&self) -> usize {
        self.grid.display_offset()
    }

    /// Get the scrollback buffer (rows that have scrolled off the viewport).
    pub fn scrollback(&self) -> &[Vec<Cell>] {
        &self.scrollback
    }

    /// Get the number of lines in scrollback.
    pub fn scrollback_len(&self) -> usize {
        self.scrollback.len()
    }

    /// Write a character at the current cursor position using the current pen,
    /// then advance the cursor. Wide (CJK) characters occupy 2 columns.
    pub fn write_char(&mut self, ch: char) {
        let wide = crate::cell::is_wide_char(ch);

        if self.cursor.col >= self.cols() {
            self.cursor.col = 0;
            self.cursor_down_with_scroll();
        }

        // Wide chars need 2 columns. If at the last column, wrap first.
        if wide && self.cursor.col + 1 >= self.cols() {
            self.cell_mut(self.cursor.row, self.cursor.col).reset();
            self.cursor.col = 0;
            self.cursor_down_with_scroll();
        }

        let row = self.cursor.row;
        let col = self.cursor.col;

        // Copy pen values to avoid borrow conflict.
        let fg = self.pen.fg;
        let bg = self.pen.bg;
        let pen_flags = self.pen.flags;

        // If we're overwriting the left half of a wide char, clear the continuation (right half).
        if col + 1 < self.cols() {
            let next_cell = self.cell(row, col + 1);
            if next_cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                self.cell_mut(row, col + 1).reset();
            }
        }

        // Write the cell at cursor position.
        let cell = self.cell_mut(row, col);
        cell.ch = ch;
        cell.fg = fg;
        cell.bg = bg;
        cell.flags = if wide {
            pen_flags | Flags::WIDE_CHAR
        } else {
            pen_flags
        };

        if wide && col + 1 < self.cols() {
            // Mark the next cell as a wide continuation (spacer).
            let next = self.cell_mut(row, col + 1);
            next.ch = ' ';
            next.fg = fg;
            next.bg = bg;
            next.flags = pen_flags | Flags::WIDE_CHAR_SPACER;
            self.cursor.col += 2;
        } else {
            self.cursor.col += 1;
        }
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
            self.rows() - 1
        };
        self.cursor.row = (self.cursor.row + n).min(max_row);
    }

    /// Move cursor forward (right) by `n` columns, clamped at last column.
    pub fn cursor_forward(&mut self, n: usize) {
        self.cursor.col = (self.cursor.col + n).min(self.cols() - 1);
    }

    /// Move cursor backward (left) by `n` columns, clamped at column 0.
    pub fn cursor_back(&mut self, n: usize) {
        self.cursor.col = self.cursor.col.saturating_sub(n);
    }

    /// Set cursor position (1-indexed row, col as received from VT sequences).
    /// Clamps to valid range.
    pub fn set_cursor_pos(&mut self, row_1: usize, col_1: usize) {
        self.cursor.row = row_1.saturating_sub(1).min(self.rows() - 1);
        self.cursor.col = col_1.saturating_sub(1).min(self.cols() - 1);
    }

    /// Move cursor down, scrolling the scroll region if at the bottom.
    fn cursor_down_with_scroll(&mut self) {
        if self.cursor.row == self.scroll_bottom {
            self.scroll_up(1);
        } else if self.cursor.row < self.rows() - 1 {
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

    // --- Tab Stops ---

    /// Set a tab stop at the current cursor column (HTS / ESC H).
    pub fn set_tab_stop(&mut self) {
        let col = self.cursor.col;
        if col < self.cols() {
            self.tab_stops[col] = true;
        }
    }

    /// Clear tab stops (TBC / CSI g).
    /// mode 0: clear at cursor column only.
    /// mode 3: clear all tab stops.
    pub fn clear_tab_stop(&mut self, mode: u16) {
        match mode {
            0 => {
                let col = self.cursor.col;
                if col < self.cols() {
                    self.tab_stops[col] = false;
                }
            }
            3 => {
                for stop in &mut self.tab_stops {
                    *stop = false;
                }
            }
            _ => {}
        }
    }

    /// Advance cursor forward by `n` tab stops (CHT / CSI I).
    pub fn cursor_forward_tab(&mut self, n: usize) {
        for _ in 0..n {
            let start = self.cursor.col + 1;
            let mut found = false;
            for c in start..self.cols() {
                if self.tab_stops[c] {
                    self.cursor.col = c;
                    found = true;
                    break;
                }
            }
            if !found {
                self.cursor.col = self.cols() - 1;
                break;
            }
        }
    }

    /// Move cursor backward by `n` tab stops (CBT / CSI Z).
    pub fn cursor_backward_tab(&mut self, n: usize) {
        for _ in 0..n {
            if self.cursor.col == 0 {
                break;
            }
            let start = self.cursor.col - 1;
            let mut found = false;
            for c in (0..=start).rev() {
                if self.tab_stops[c] {
                    self.cursor.col = c;
                    found = true;
                    break;
                }
            }
            if !found {
                self.cursor.col = 0;
                break;
            }
        }
    }

    // --- Scroll ---

    /// Set the scroll region (1-indexed top and bottom, inclusive).
    /// Resets cursor to top-left of the scroll region.
    pub fn set_scroll_region(&mut self, top_1: usize, bottom_1: usize) {
        let top = top_1.saturating_sub(1).min(self.rows() - 1);
        let bottom = bottom_1.saturating_sub(1).min(self.rows() - 1);
        if top < bottom {
            self.scroll_top = top;
            self.scroll_bottom = bottom;
            self.cursor.row = top;
            self.cursor.col = 0;
        }
    }

    /// Scroll the scroll region up by `n` lines.
    pub fn scroll_up(&mut self, n: usize) {
        let top = self.scroll_top;
        let bottom = self.scroll_bottom;
        let n = n.min(bottom - top + 1);
        if n == 0 {
            return;
        }

        let cols = self.cols();
        let region = CellRange::from_line_cols(top as i32, 0, bottom as i32, cols - 1);
        let template = Cell::default();

        // Capture top n rows before they're overwritten.
        // We need to read the grid before calling scroll_up.
        let mut rows_to_save: Vec<Vec<Cell>> = Vec::with_capacity(n);
        for i in 0..n {
            let line = Line((top + i) as i32);
            if let Some(row) = self.grid.get(line) {
                let cells: Vec<Cell> = (0..cols).map(|c| row[Column(c)]).collect();
                rows_to_save.push(cells);
            }
        }

        // Do the scroll (this overwrites top n lines)
        self.grid.scroll_up(&region, n, &template);

        // Save the captured rows to scrollback
        self.scrollback.extend(rows_to_save);
    }

    /// Scroll the scroll region down by `n` lines.
    /// Bottom lines are discarded, top lines become blank.
    pub fn scroll_down(&mut self, n: usize) {
        let top = self.scroll_top;
        let bottom = self.scroll_bottom;
        let n = n.min(bottom - top + 1);
        if n == 0 {
            return;
        }

        let cols = self.cols();
        let region = CellRange::from_line_cols(top as i32, 0, bottom as i32, cols - 1);
        let template = Cell::default();

        // Capture bottom n rows before they're overwritten.
        // For scroll down, these are the rows that get "pushed off" (top) conceptually
        // but physically the bottom rows are discarded.
        let mut rows_to_save: Vec<Vec<Cell>> = Vec::with_capacity(n);
        let start_save_idx = bottom.saturating_sub(n - 1);
        for i in 0..n {
            let line = Line((start_save_idx + i) as i32);
            if let Some(row) = self.grid.get(line) {
                let cells: Vec<Cell> = (0..cols).map(|c| row[Column(c)]).collect();
                rows_to_save.push(cells);
            }
        }

        // Do the scroll
        self.grid.scroll_down(&region, n, &template);

        // Save captured rows to scrollback (prepend since scroll down exposes older content)
        for row in rows_to_save.into_iter().rev() {
            self.scrollback.insert(0, row);
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
                for row in (self.cursor.row + 1)..self.rows() {
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
                self.clear_screen();
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
                for col in self.cursor.col..self.cols() {
                    self.cell_mut(row, col).reset();
                }
            }
            1 => {
                for col in 0..=self.cursor.col.min(self.cols() - 1) {
                    self.cell_mut(row, col).reset();
                }
            }
            2 => {
                self.clear_row(row);
            }
            _ => {}
        }
    }

    /// Erase `n` characters at the cursor position without moving the cursor (ECH / CSI X).
    pub fn erase_chars(&mut self, n: usize) {
        let row = self.cursor.row;
        let col = self.cursor.col;
        let end = (col + n).min(self.cols());
        for c in col..end {
            self.cell_mut(row, c).reset();
        }
    }

    fn clear_row(&mut self, row: usize) {
        for col in 0..self.cols() {
            self.cell_mut(row, col).reset();
        }
    }

    fn clear_screen(&mut self) {
        let template = Cell::default();
        self.grid.clear_viewport(&template);
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
        let template = Cell::default();

        // Use Grid's scroll_down within the region for efficiency.
        // But we need to handle this specially since insert_lines is at cursor row.
        for _ in 0..n {
            let region = CellRange::from_line_cols(row as i32, 0, bottom as i32, self.cols() - 1);
            self.grid.scroll_down(&region, 1, &template);
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
        let template = Cell::default();

        for _ in 0..n {
            let region = CellRange::from_line_cols(row as i32, 0, bottom as i32, self.cols() - 1);
            self.grid.scroll_up(&region, 1, &template);
        }
    }

    /// Insert `n` blank characters at the cursor, shifting existing chars right.
    pub fn insert_chars(&mut self, n: usize) {
        let row = self.cursor.row;
        let col = self.cursor.col;
        let n = n.min(self.cols() - col);

        if n == 0 {
            return;
        }

        // Shift right by n within [col, cols).
        // Iterate in reverse to avoid overwriting source cells before they're copied.
        for c in (col..self.cols()).rev() {
            if c >= col + n {
                *self.cell_mut(row, c) = *self.cell(row, c - n);
            } else {
                self.cell_mut(row, c).reset();
            }
        }
    }

    /// Delete `n` characters at the cursor, shifting remaining chars left.
    pub fn delete_chars(&mut self, n: usize) {
        let row = self.cursor.row;
        let col = self.cursor.col;
        let cols = self.cols();
        let n = n.min(cols - col);

        if n == 0 {
            return;
        }

        // Copy cells that will remain first to avoid borrow conflict.
        // Cell is Copy, so simple dereference works.
        let cells: Vec<Cell> = (0..cols).map(|c| *self.cell(row, c)).collect();

        // Shift left by n.
        for c in col..cols {
            let src_col = c + n;
            if src_col < cols {
                *self.cell_mut(row, c) = cells[src_col];
            } else {
                self.cell_mut(row, c).reset();
            }
        }
    }

    // --- Reset ---

    /// Reset the buffer to initial state.
    pub fn reset(&mut self) {
        self.clear_screen();
        self.cursor = Cursor::default();
        self.pen = Pen::default();
        self.scroll_top = 0;
        self.scroll_bottom = self.rows() - 1;
        self.tab_stops = default_tab_stops(self.cols());
        self.scrollback.clear();
    }

    /// Extract the text content of a row as a string (trimming trailing spaces).
    pub fn row_text(&self, row: usize) -> String {
        let mut s = String::new();
        if let Some(r) = self.grid.get(Line(row as i32)) {
            for c in 0..self.cols() {
                s.push(r[Column(c)].ch);
            }
        }
        s.trim_end().to_string()
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
        buf.pen.flags.insert(Flags::BOLD);
        buf.write_char('X');
        let cell = buf.cell(0, 0);
        assert_eq!(cell.fg, Color::RED);
        assert!(cell.flags.contains(Flags::BOLD));
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

    #[test]
    fn cell_mut_modify() {
        let mut buf = ScreenBuffer::new(5, 2);
        buf.cell_mut(0, 0).ch = 'Z';
        assert_eq!(buf.cell(0, 0).ch, 'Z');
    }

    #[test]
    fn erase_in_display_mode_0() {
        let mut buf = ScreenBuffer::new(10, 3);
        for ch in "AAAAAAAAAA".chars() {
            buf.write_char(ch);
        }
        buf.set_cursor_pos(1, 1); // row 0, col 0
        buf.cursor_forward(5); // col 5
        buf.erase_in_display(0); // from cursor to end
        assert_eq!(buf.cell(0, 4).ch, 'A'); // before cursor
        assert_eq!(buf.cell(0, 5).ch, ' '); // erased
        assert_eq!(buf.row_text(1), ""); // below erased
    }

    #[test]
    fn erase_in_display_mode_1() {
        let mut buf = ScreenBuffer::new(10, 3);
        for row in 0..3 {
            buf.set_cursor_pos(row + 1, 1);
            for ch in "ABCDE".chars() {
                buf.write_char(ch);
            }
        }
        buf.set_cursor_pos(2, 3); // row 1, col 2
        buf.erase_in_display(1); // from start to cursor
        assert_eq!(buf.row_text(0), ""); // above erased
        assert_eq!(buf.cell(1, 0).ch, ' '); // erased
        assert_eq!(buf.cell(1, 2).ch, ' '); // erased (cursor pos)
        assert_eq!(buf.cell(1, 3).ch, 'D'); // after cursor preserved
    }

    #[test]
    fn erase_in_line_mode_2() {
        let mut buf = ScreenBuffer::new(10, 1);
        for ch in "Hello".chars() {
            buf.write_char(ch);
        }
        buf.erase_in_line(2);
        assert_eq!(buf.row_text(0), "");
    }

    #[test]
    fn resize_clamps_cursor() {
        let mut buf = ScreenBuffer::new(80, 24);
        buf.set_cursor_pos(20, 70); // row 19, col 69
        buf.resize(40, 10);
        assert!(buf.cursor.row < 10);
        assert!(buf.cursor.col < 40);
    }

    #[test]
    fn insert_lines_outside_region() {
        let mut buf = ScreenBuffer::new(5, 5);
        buf.set_scroll_region(2, 4); // rows 1-3
        buf.set_cursor_pos(1, 1); // row 0 -- outside region
        buf.insert_lines(1);
        // Should have no effect since cursor is outside scroll region
    }

    #[test]
    fn delete_lines_outside_region() {
        let mut buf = ScreenBuffer::new(5, 5);
        buf.set_scroll_region(2, 4);
        buf.set_cursor_pos(1, 1); // row 0 -- outside region
        buf.delete_lines(1);
        // Should have no effect
    }

    #[test]
    fn set_scroll_region_invalid() {
        let mut buf = ScreenBuffer::new(80, 24);
        buf.set_scroll_region(10, 5); // top > bottom -- should be ignored
        // Scroll region should remain at defaults (0, 23)
    }

    #[test]
    fn unicode_wide_char_test() {
        let mut buf = ScreenBuffer::new(20, 3);
        buf.write_char('\u{4e16}');
        assert_eq!(buf.cell(0, 0).ch, '\u{4e16}');
        assert!(buf.cell(0, 0).flags.contains(Flags::WIDE_CHAR));
        assert!(buf.cell(0, 1).flags.contains(Flags::WIDE_CHAR_SPACER));
        assert_eq!(buf.cursor.col, 2);
    }

    #[test]
    fn wide_char_sets_wide_flag_on_left_cell() {
        let mut buf = ScreenBuffer::new(20, 3);
        buf.write_char('\u{4e16}');
        // Left cell has WIDE_CHAR flag
        let left = buf.cell(0, 0);
        assert_eq!(left.ch, '\u{4e16}');
        assert!(left.flags.contains(Flags::WIDE_CHAR));
        assert!(!left.flags.contains(Flags::WIDE_CHAR_SPACER));
        // Right cell has WIDE_CHAR_SPACER flag
        let right = buf.cell(0, 1);
        assert!(right.flags.contains(Flags::WIDE_CHAR_SPACER));
        assert!(!right.flags.contains(Flags::WIDE_CHAR));
    }

    // --- Tab stop tests ---

    #[test]
    fn default_tab_stops_every_8() {
        let buf = ScreenBuffer::new(80, 1);
        // col 0 is NOT a tab stop, cols 8, 16, 24, ... are.
        assert!(!buf.tab_stops[0]);
        assert!(buf.tab_stops[8]);
        assert!(buf.tab_stops[16]);
        assert!(buf.tab_stops[24]);
        assert!(!buf.tab_stops[7]);
    }

    #[test]
    fn cursor_forward_tab_basic() {
        let mut buf = ScreenBuffer::new(80, 1);
        buf.cursor.col = 0;
        buf.cursor_forward_tab(1);
        assert_eq!(buf.cursor.col, 8);
        buf.cursor_forward_tab(1);
        assert_eq!(buf.cursor.col, 16);
    }

    #[test]
    fn cursor_forward_tab_multiple() {
        let mut buf = ScreenBuffer::new(80, 1);
        buf.cursor.col = 0;
        buf.cursor_forward_tab(3);
        assert_eq!(buf.cursor.col, 24);
    }

    #[test]
    fn cursor_backward_tab_basic() {
        let mut buf = ScreenBuffer::new(80, 1);
        buf.cursor.col = 20;
        buf.cursor_backward_tab(1);
        assert_eq!(buf.cursor.col, 16);
        buf.cursor_backward_tab(1);
        assert_eq!(buf.cursor.col, 8);
    }

    #[test]
    fn cursor_backward_tab_at_zero() {
        let mut buf = ScreenBuffer::new(80, 1);
        buf.cursor.col = 3;
        buf.cursor_backward_tab(1);
        assert_eq!(buf.cursor.col, 0); // no tab stop found, go to 0
    }

    #[test]
    fn set_and_clear_tab_stop() {
        let mut buf = ScreenBuffer::new(80, 1);
        // Set a custom tab stop at col 5.
        buf.cursor.col = 5;
        buf.set_tab_stop();
        assert!(buf.tab_stops[5]);

        // Forward tab from col 0 should now go to 5 (before 8).
        buf.cursor.col = 0;
        buf.cursor_forward_tab(1);
        assert_eq!(buf.cursor.col, 5);

        // Clear at cursor (col 5).
        buf.clear_tab_stop(0);
        assert!(!buf.tab_stops[5]);

        // Now forward tab from 0 should go to 8 again.
        buf.cursor.col = 0;
        buf.cursor_forward_tab(1);
        assert_eq!(buf.cursor.col, 8);
    }

    #[test]
    fn clear_all_tab_stops() {
        let mut buf = ScreenBuffer::new(80, 1);
        buf.clear_tab_stop(3); // clear all
        // No tab stops -- forward tab should go to last column.
        buf.cursor.col = 0;
        buf.cursor_forward_tab(1);
        assert_eq!(buf.cursor.col, 79);
    }

    #[test]
    fn erase_chars_basic() {
        let mut buf = ScreenBuffer::new(10, 1);
        for ch in "ABCDEFGH".chars() {
            buf.write_char(ch);
        }
        buf.cursor.col = 2;
        buf.erase_chars(3);
        assert_eq!(buf.cell(0, 0).ch, 'A');
        assert_eq!(buf.cell(0, 1).ch, 'B');
        assert_eq!(buf.cell(0, 2).ch, ' '); // erased
        assert_eq!(buf.cell(0, 3).ch, ' '); // erased
        assert_eq!(buf.cell(0, 4).ch, ' '); // erased
        assert_eq!(buf.cell(0, 5).ch, 'F'); // not erased
        // Cursor should NOT have moved.
        assert_eq!(buf.cursor.col, 2);
    }

    #[test]
    fn reset_restores_tab_stops() {
        let mut buf = ScreenBuffer::new(80, 1);
        buf.clear_tab_stop(3); // clear all
        buf.reset();
        // Tab stops should be restored to defaults.
        assert!(buf.tab_stops[8]);
        assert!(buf.tab_stops[16]);
    }
}
