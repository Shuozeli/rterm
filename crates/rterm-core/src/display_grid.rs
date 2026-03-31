//! Client-side display grid: maintains visible cells + local scrollback.
//! Shared between WASM renderer and native tests.
//!
//! This is the single source of truth for "what should the terminal show
//! at any given scroll position."

/// A cell in the display grid (matches the protocol's CellData).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DisplayCell {
    pub ch: char,
    pub fg: u32,
    pub bg: u32,
    pub attrs: u8,
}

impl Default for DisplayCell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: 0xFFFFFFFF, // COLOR_DEFAULT
            bg: 0xFFFFFFFF,
            attrs: 0,
        }
    }
}

/// A range of cells on one row (matches protocol CellRange).
#[derive(Debug, Clone)]
pub struct DisplayCellRange {
    pub row: u16,
    pub col_start: u16,
    pub cells: Vec<DisplayCell>,
}

/// Screen data received from the server.
#[derive(Debug, Clone)]
pub struct DisplayScreenData {
    pub changes: Vec<DisplayCellRange>,
    pub cursor_row: u16,
    pub cursor_col: u16,
    pub cursor_visible: bool,
    pub cursor_style: u8,
    pub cols: u16,
    pub rows: u16,
    pub scrollback_len: u32,
}

/// The display grid: 2D cell array + local scrollback buffer.
pub struct DisplayGrid {
    pub cells: Vec<Vec<DisplayCell>>,
    pub cols: usize,
    pub rows: usize,
    pub cursor_row: u16,
    pub cursor_col: u16,
    pub cursor_visible: bool,
    pub cursor_style: u8,
}

impl DisplayGrid {
    pub fn new(cols: usize, rows: usize) -> Self {
        Self {
            cells: vec![vec![DisplayCell::default(); cols]; rows],
            cols,
            rows,
            cursor_row: 0,
            cursor_col: 0,
            cursor_visible: true,
            cursor_style: 0,
        }
    }

    /// Apply a full screen snapshot (replaces all cells).
    pub fn apply_snapshot(&mut self, data: &DisplayScreenData) {
        let cols = data.cols as usize;
        let rows = data.rows as usize;
        self.cols = cols;
        self.rows = rows;
        self.cells = vec![vec![DisplayCell::default(); cols]; rows];

        for cr in &data.changes {
            let row = cr.row as usize;
            for (i, cell) in cr.cells.iter().enumerate() {
                let col = cr.col_start as usize + i;
                if row < rows && col < cols {
                    self.cells[row][col] = *cell;
                }
            }
        }
        self.cursor_row = data.cursor_row;
        self.cursor_col = data.cursor_col;
        self.cursor_visible = data.cursor_visible;
        self.cursor_style = data.cursor_style;
    }

    /// Apply a screen update (diff). Detects scroll and accumulates scrollback.
    pub fn apply_update(&mut self, data: &DisplayScreenData) {
        // Handle resize.
        if data.cols as usize != self.cols || data.rows as usize != self.rows {
            self.cols = data.cols as usize;
            self.rows = data.rows as usize;
            self.cells
                .resize(self.rows, vec![DisplayCell::default(); self.cols]);
            for row in &mut self.cells {
                row.resize(self.cols, DisplayCell::default());
            }
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
    }

    pub fn visible_cell(&self, row: usize, col: usize) -> &DisplayCell {
        static DEFAULT: DisplayCell = DisplayCell {
            ch: ' ',
            fg: 0xFFFFFFFF,
            bg: 0xFFFFFFFF,
            attrs: 0,
        };

        if row < self.cells.len() && col < self.cols {
            return &self.cells[row][col];
        }
        &DEFAULT
    }

    /// Get visible text for a row (for testing).
    pub fn visible_row_text(&self, row: usize) -> String {
        (0..self.cols)
            .map(|col| self.visible_cell(row, col).ch)
            .collect::<String>()
            .trim_end()
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(ch: char) -> DisplayCell {
        DisplayCell {
            ch,
            ..Default::default()
        }
    }

    fn screen_data(lines: &[&str], cols: u16, rows: u16, scrollback_len: u32) -> DisplayScreenData {
        DisplayScreenData {
            changes: lines
                .iter()
                .enumerate()
                .map(|(i, text)| DisplayCellRange {
                    row: i as u16,
                    col_start: 0,
                    cells: text.chars().map(cell).collect(),
                })
                .collect(),
            cursor_row: 0,
            cursor_col: 0,
            cursor_visible: true,
            cursor_style: 0,
            cols,
            rows,
            scrollback_len,
        }
    }

    // =====================================================================
    // Live view (no scroll)
    // =====================================================================

    #[test]
    fn live_view_shows_screen() {
        let mut g = DisplayGrid::new(10, 3);
        g.apply_snapshot(&screen_data(&["Hello", "World", "Test"], 10, 3, 0));
        assert_eq!(g.visible_row_text(0), "Hello");
        assert_eq!(g.visible_row_text(1), "World");
        assert_eq!(g.visible_row_text(2), "Test");
    }

    #[test]
    fn cursor_updates_applied() {
        let mut g = DisplayGrid::new(10, 2);
        g.apply_snapshot(&screen_data(&["hi", ""], 10, 2, 0));
        assert_eq!(g.cursor_row, 0);
        assert_eq!(g.cursor_col, 0);

        let mut data = screen_data(&["hi", "world"], 10, 2, 0);
        data.cursor_row = 1;
        data.cursor_col = 5;
        data.cursor_visible = false;
        g.apply_update(&data);
        assert_eq!(g.cursor_row, 1);
        assert_eq!(g.cursor_col, 5);
        assert!(!g.cursor_visible);
    }
}
