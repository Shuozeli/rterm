//! Client-side display grid: maintains visible cells + local scrollback.
//!
//! This is the single source of truth for "what should the terminal show
//! at any given scroll position." Used by both the WASM browser client and
//! the native GUI demo.

use crate::cell::{COLOR_DEFAULT, DisplayCell};

/// A range of cells on one row (matches protocol CellRange).
#[derive(Debug, Clone)]
pub struct DisplayCellRange {
    pub row: u16,
    pub col_start: u16,
    pub cells: Vec<DisplayCell>,
}

/// Screen data received from the server.
#[derive(Debug, Clone)]
pub struct ScreenData {
    pub changes: Vec<DisplayCellRange>,
    pub cursor_row: u16,
    pub cursor_col: u16,
    pub cursor_visible: bool,
    pub cursor_style: u8,
    pub cols: u16,
    pub rows: u16,
    pub mouse_tracking_mode: u8,
    pub alt_screen_active: bool,
    pub application_cursor_keys: bool,
    /// Viewport offset: non-zero when this snapshot represents a scrolled viewport.
    pub viewport_offset: u32,
}

/// Scrollback data returned by the relay.
#[derive(Debug, Clone)]
pub struct ScrollbackData {
    pub lines: Vec<DisplayCellRange>,
    pub offset: u32,
    pub total: u32,
}

/// The display grid: 2D cell array + local scrollback buffer.
#[derive(Clone)]
pub struct DisplayGrid {
    pub cells: Vec<Vec<DisplayCell>>,
    pub cols: usize,
    pub rows: usize,
    pub cursor_row: u16,
    pub cursor_col: u16,
    pub cursor_visible: bool,
    pub cursor_style: u8,

    pub selection_start: Option<(usize, usize)>,
    pub selection_end: Option<(usize, usize)>,
    pub mouse_tracking_mode: u8,
    pub alt_screen_active: bool,
    pub application_cursor_keys: bool,

    // Scrollback state.
    pub scroll_offset: u32,
    pub scrollback_total: u32,
    scrollback_lines: Vec<Vec<DisplayCell>>,
    /// Viewport offset from the last ScreenSnapshot (non-zero = in scrolled viewport mode).
    viewport_offset: u32,
}

impl DisplayGrid {
    pub fn new(cols: usize, rows: usize) -> Self {
        let default_cell = DisplayCell {
            ch: ' ',
            fg: COLOR_DEFAULT,
            bg: COLOR_DEFAULT,
            flags: 0,
        };
        Self {
            cells: vec![vec![default_cell; cols]; rows],
            cols,
            rows,
            cursor_row: 0,
            cursor_col: 0,
            cursor_visible: true,
            cursor_style: 0,

            selection_start: None,
            selection_end: None,
            mouse_tracking_mode: 0,
            alt_screen_active: false,
            application_cursor_keys: false,

            scroll_offset: 0,
            scrollback_total: 0,
            scrollback_lines: Vec::new(),
            viewport_offset: 0,
        }
    }

    /// Apply a full screen snapshot (replaces all cells).
    /// When viewport_offset > 0, this is a viewport snapshot (from scroll) containing
    /// viewport_offset scrollback rows followed by current screen rows.
    pub fn apply_snapshot(&mut self, data: &ScreenData) {
        let cols = data.cols as usize;
        let rows = data.rows as usize;
        let default_cell = DisplayCell {
            ch: ' ',
            fg: COLOR_DEFAULT,
            bg: COLOR_DEFAULT,
            flags: 0,
        };
        self.cols = cols;
        self.rows = rows;

        self.viewport_offset = data.viewport_offset;

        if data.viewport_offset > 0 {
            // Viewport snapshot: first viewport_offset rows are scrollback, rest are current screen.
            let total_rows = data.changes.len();
            let scrollback_count = data.viewport_offset as usize;
            let screen_count = total_rows.saturating_sub(scrollback_count);

            self.scrollback_lines.clear();
            self.scrollback_lines.reserve(scrollback_count);
            for i in 0..scrollback_count {
                if i < data.changes.len() {
                    let cr = &data.changes[i];
                    let row_cells: Vec<DisplayCell> = cr.cells.to_vec();
                    self.scrollback_lines.push(row_cells);
                }
            }

            self.cells = vec![vec![default_cell; cols]; rows];
            for i in 0..screen_count {
                let idx = scrollback_count + i;
                if idx < data.changes.len() {
                    let cr = &data.changes[idx];
                    let row = cr.row as usize;
                    if row < rows {
                        for (j, cell) in cr.cells.iter().enumerate() {
                            let col = cr.col_start as usize + j;
                            if col < cols {
                                self.cells[row][col] = *cell;
                            }
                        }
                    }
                }
            }

            self.scroll_offset = 0;
            self.scrollback_total = data.viewport_offset;
        } else {
            // Normal snapshot (live view): all rows are current screen.
            self.cells = vec![vec![default_cell; cols]; rows];
            self.scrollback_lines.clear();
            self.scroll_offset = 0;
            self.scrollback_total = 0;

            for cr in &data.changes {
                let row = cr.row as usize;
                for (i, cell) in cr.cells.iter().enumerate() {
                    let col = cr.col_start as usize + i;
                    if row < rows && col < cols {
                        self.cells[row][col] = *cell;
                    }
                }
            }
        }

        self.cursor_row = data.cursor_row;
        self.cursor_col = data.cursor_col;
        self.cursor_visible = data.cursor_visible;
        self.cursor_style = data.cursor_style;

        self.mouse_tracking_mode = data.mouse_tracking_mode;
        self.alt_screen_active = data.alt_screen_active;
        self.application_cursor_keys = data.application_cursor_keys;
    }

    /// Apply a screen update (diff — only changed cells).
    pub fn apply_update(&mut self, data: &ScreenData) {
        // Handle resize.
        if data.cols as usize != self.cols || data.rows as usize != self.rows {
            self.resize(data.cols as usize, data.rows as usize);
        }

        // Apply cell changes.
        for cr in &data.changes {
            let row = cr.row as usize;
            for (i, cell) in cr.cells.iter().enumerate() {
                let col = cr.col_start as usize + i;
                if row < self.rows && col < self.cols {
                    self.cells[row][col] = *cell;
                }
            }
        }
        self.cursor_row = data.cursor_row;
        self.cursor_col = data.cursor_col;
        self.cursor_visible = data.cursor_visible;
        self.cursor_style = data.cursor_style;
        self.mouse_tracking_mode = data.mouse_tracking_mode;
        self.alt_screen_active = data.alt_screen_active;
        self.application_cursor_keys = data.application_cursor_keys;
    }

    /// Apply scrollback data from the relay.
    pub fn apply_scrollback(&mut self, data: &ScrollbackData) {
        self.scrollback_total = data.total;
        self.scroll_offset = data.offset;

        self.scrollback_lines.clear();
        self.scrollback_lines.reserve(data.lines.len());
        for line in &data.lines {
            let row_cells: Vec<DisplayCell> = line.cells.to_vec();
            self.scrollback_lines.push(row_cells);
        }
    }

    /// Scroll the view by `delta` lines (positive = scroll up/back, negative = scroll down/forward).
    /// Returns true if the scroll changed.
    pub fn scroll_by(&mut self, delta: i32) -> bool {
        if self.scrollback_total == 0 {
            return false;
        }
        let new_offset = if delta > 0 {
            (self.scroll_offset + delta as u32)
                .min(self.scrollback_total.saturating_sub(self.rows as u32))
        } else {
            self.scroll_offset.saturating_sub((-delta) as u32)
        };
        if new_offset == self.scroll_offset {
            return false;
        }
        self.scroll_offset = new_offset;
        true
    }

    /// Resize the local grid immediately so viewport changes repaint without
    /// waiting for a round-trip snapshot from the server.
    pub fn resize(&mut self, cols: usize, rows: usize) {
        let default_cell = DisplayCell {
            ch: ' ',
            fg: COLOR_DEFAULT,
            bg: COLOR_DEFAULT,
            flags: 0,
        };
        self.cols = cols;
        self.rows = rows;
        self.cells.resize(rows, vec![default_cell; cols]);
        for row in &mut self.cells {
            row.resize(cols, default_cell);
        }

        if rows > 0 {
            self.cursor_row = self.cursor_row.min((rows - 1) as u16);
        } else {
            self.cursor_row = 0;
        }
        if cols > 0 {
            self.cursor_col = self.cursor_col.min((cols - 1) as u16);
        } else {
            self.cursor_col = 0;
        }

        self.selection_start = None;
        self.selection_end = None;
    }

    /// Get the cell that should be visible at (row, col) accounting for scroll offset.
    /// When viewport_offset > 0, we're in viewport mode (from a scroll) and render
    /// directly from the stored snapshot rows.
    /// When scroll_offset > 0 in normal mode, rows show scrollback content.
    /// Below the scrollback region, the current terminal screen is shown.
    pub fn visible_cell(&self, row: usize, col: usize) -> &DisplayCell {
        static DEFAULT: DisplayCell = DisplayCell {
            ch: ' ',
            fg: COLOR_DEFAULT,
            bg: COLOR_DEFAULT,
            flags: 0,
        };

        if col >= self.cols {
            return &DEFAULT;
        }

        // Viewport mode: render from the stored snapshot rows.
        if self.viewport_offset > 0 {
            if row < self.scrollback_lines.len() {
                let line = &self.scrollback_lines[row];
                if col < line.len() {
                    return &line[col];
                }
            }
            return &DEFAULT;
        }

        // Normal mode: scrollback + current screen.
        if self.scroll_offset > 0 && row < self.scrollback_total as usize {
            // Show scrollback content at the top.
            let scrollback_row = self.scroll_offset as usize + row;
            if scrollback_row < self.scrollback_lines.len() {
                let line = &self.scrollback_lines[scrollback_row];
                if col < line.len() {
                    return &line[col];
                }
            }
            return &DEFAULT;
        }

        // Show current terminal screen.
        let screen_row = if self.scroll_offset > 0 {
            row.saturating_sub(self.scrollback_total as usize)
        } else {
            row
        };

        if screen_row < self.cells.len() && screen_row < self.rows {
            &self.cells[screen_row][col]
        } else {
            &DEFAULT
        }
    }

    /// Get visible text for a row (accounting for scroll offset).
    pub fn visible_row_text(&self, row: usize) -> String {
        (0..self.cols)
            .map(|col| self.visible_cell(row, col).ch)
            .collect::<String>()
            .trim_end()
            .to_string()
    }

    /// Check if a cell is selected.
    pub fn is_selected(&self, row: usize, col: usize) -> bool {
        let Some((sr, sc)) = self.selection_start else {
            return false;
        };
        let Some((er, ec)) = self.selection_end else {
            return false;
        };
        let (sr, sc, er, ec) = if (sr, sc) <= (er, ec) {
            (sr, sc, er, ec)
        } else {
            (er, ec, sr, sc)
        };
        if row < sr || row > er {
            return false;
        }
        if row == sr && row == er {
            return col >= sc && col <= ec;
        }
        if row == sr {
            return col >= sc;
        }
        if row == er {
            return col <= ec;
        }
        true
    }

    /// Get selected text.
    pub fn selected_text(&self) -> String {
        let Some((sr, sc)) = self.selection_start else {
            return String::new();
        };
        let Some((er, ec)) = self.selection_end else {
            return String::new();
        };
        let (sr, sc, er, ec) = if (sr, sc) <= (er, ec) {
            (sr, sc, er, ec)
        } else {
            (er, ec, sr, sc)
        };
        let mut text = String::new();
        for row in sr..=er {
            if row >= self.rows {
                break;
            }
            let col_start = if row == sr { sc } else { 0 };
            let col_end = if row == er {
                ec.min(self.cols - 1)
            } else {
                self.cols - 1
            };
            for col in col_start..=col_end {
                text.push(self.cells[row][col].ch);
            }
            if row < er {
                let trimmed = text.trim_end().to_string();
                text = trimmed;
                text.push('\n');
            }
        }
        text.trim_end().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(ch: char) -> DisplayCell {
        DisplayCell {
            ch,
            fg: COLOR_DEFAULT,
            bg: COLOR_DEFAULT,
            flags: 0,
        }
    }

    fn screen_data(lines: &[&str], cols: u16, rows: u16) -> ScreenData {
        let changes: Vec<DisplayCellRange> = lines
            .iter()
            .enumerate()
            .map(|(i, text)| DisplayCellRange {
                row: i as u16,
                col_start: 0,
                cells: text.chars().map(cell).collect(),
            })
            .collect();
        ScreenData {
            changes,
            cursor_row: 0,
            cursor_col: 0,
            cursor_visible: true,
            cursor_style: 0,
            cols,
            rows,
            mouse_tracking_mode: 0,
            alt_screen_active: false,
            application_cursor_keys: false,
            viewport_offset: 0,
        }
    }

    #[test]
    fn live_view_shows_screen() {
        let mut g = DisplayGrid::new(10, 3);
        g.apply_snapshot(&screen_data(&["Hello", "World", "Test"], 10, 3));
        assert_eq!(g.visible_row_text(0), "Hello");
        assert_eq!(g.visible_row_text(1), "World");
        assert_eq!(g.visible_row_text(2), "Test");
    }

    #[test]
    fn cursor_updates_applied() {
        let mut g = DisplayGrid::new(10, 2);
        g.apply_snapshot(&screen_data(&["hi", ""], 10, 2));
        assert_eq!(g.cursor_row, 0);
        assert_eq!(g.cursor_col, 0);

        let mut data = screen_data(&["hi", "world"], 10, 2);
        data.cursor_row = 1;
        data.cursor_col = 5;
        data.cursor_visible = false;
        g.apply_update(&data);
        assert_eq!(g.cursor_row, 1);
        assert_eq!(g.cursor_col, 5);
        assert!(!g.cursor_visible);
    }
}
