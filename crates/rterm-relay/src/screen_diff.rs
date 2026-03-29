/// Screen differ: compares terminal screen state between frames
/// and produces typed ScreenUpdate messages with only changed cells.
use rterm_core::buffer::ScreenBuffer;
use rterm_core::cell::CellAttributes;
use rterm_core::color::Color;
use rterm_proto::*;

/// Convert an rterm Color to packed u32.
fn pack_color(color: &Color) -> u32 {
    match color {
        Color::Default => COLOR_DEFAULT,
        Color::Indexed(idx) => pack_color_indexed(*idx),
        Color::Rgb(r, g, b) => pack_color_rgb(*r, *g, *b),
    }
}

/// Convert cell attributes to packed bitflags.
fn pack_attrs(attrs: &CellAttributes) -> u8 {
    let mut flags = 0u8;
    if attrs.bold {
        flags |= ATTR_BOLD;
    }
    if attrs.italic {
        flags |= ATTR_ITALIC;
    }
    if attrs.underline {
        flags |= ATTR_UNDERLINE;
    }
    if attrs.strikethrough {
        flags |= ATTR_STRIKETHROUGH;
    }
    if attrs.reverse {
        flags |= ATTR_REVERSE;
    }
    if attrs.dim {
        flags |= ATTR_DIM;
    }
    if attrs.hidden {
        flags |= ATTR_HIDDEN;
    }
    flags
}

/// Convert a screen buffer cell to a CellData.
fn cell_to_data(cell: &rterm_core::Cell) -> CellData {
    CellData {
        ch: cell.ch,
        fg: pack_color(&cell.fg),
        bg: pack_color(&cell.bg),
        attrs: pack_attrs(&cell.attrs),
    }
}

/// Create a full ScreenSnapshot from the current buffer state.
pub fn snapshot(buffer: &ScreenBuffer) -> ScreenSnapshotData {
    let cols = buffer.cols();
    let rows = buffer.rows();

    let row_data: Vec<CellRangeData> = (0..rows)
        .map(|row| CellRangeData {
            row: row as u16,
            col_start: 0,
            cells: (0..cols)
                .map(|col| cell_to_data(buffer.cell(row, col)))
                .collect(),
        })
        .collect();

    ScreenSnapshotData {
        rows: row_data,
        cursor: CursorData {
            row: buffer.cursor.row as u16,
            col: buffer.cursor.col as u16,
            visible: buffer.cursor.visible,
        },
        cols: cols as u16,
        num_rows: rows as u16,
        title: None,
        scrollback_len: buffer.scrollback_len() as u32,
    }
}

/// Previous screen state for diffing.
pub struct PrevScreen {
    cells: Vec<Vec<(char, u32, u32, u8)>>, // (ch, fg, bg, attrs)
    cursor_row: u16,
    cursor_col: u16,
    cursor_visible: bool,
    cols: usize,
    rows: usize,
}

impl PrevScreen {
    pub fn new(cols: usize, rows: usize) -> Self {
        Self {
            cells: vec![vec![(' ', COLOR_DEFAULT, COLOR_DEFAULT, 0); cols]; rows],
            cursor_row: 0,
            cursor_col: 0,
            cursor_visible: true,
            cols,
            rows,
        }
    }

    /// Update from a snapshot (after resize or initial connect).
    pub fn update_from_snapshot(&mut self, ss: &ScreenSnapshotData) {
        let cols = ss.cols as usize;
        let rows = ss.num_rows as usize;
        self.cols = cols;
        self.rows = rows;
        self.cells = vec![vec![(' ', COLOR_DEFAULT, COLOR_DEFAULT, 0); cols]; rows];

        for cr in &ss.rows {
            let row = cr.row as usize;
            if row < rows {
                for (i, cell) in cr.cells.iter().enumerate() {
                    let col = cr.col_start as usize + i;
                    if col < cols {
                        self.cells[row][col] = (cell.ch, cell.fg, cell.bg, cell.attrs);
                    }
                }
            }
        }
        self.cursor_row = ss.cursor.row;
        self.cursor_col = ss.cursor.col;
        self.cursor_visible = ss.cursor.visible;
    }

    /// Diff the current buffer against the previous state.
    /// Returns a ScreenUpdate with only changed cells, or None if nothing changed.
    pub fn diff(&mut self, buffer: &ScreenBuffer) -> Option<ScreenUpdateData> {
        let cols = buffer.cols();
        let rows = buffer.rows();

        // If dimensions changed, caller should send a full snapshot instead.
        if cols != self.cols || rows != self.rows {
            return None;
        }

        let mut changes = Vec::new();
        let cursor = CursorData {
            row: buffer.cursor.row as u16,
            col: buffer.cursor.col as u16,
            visible: buffer.cursor.visible,
        };

        for row in 0..rows {
            let mut range_start: Option<usize> = None;
            let mut range_cells: Vec<CellData> = Vec::new();

            for col in 0..cols {
                let cell = buffer.cell(row, col);
                let new = (
                    cell.ch,
                    pack_color(&cell.fg),
                    pack_color(&cell.bg),
                    pack_attrs(&cell.attrs),
                );
                let old = self.cells[row][col];

                if new != old {
                    // Cell changed.
                    if range_start.is_none() {
                        range_start = Some(col);
                    }
                    range_cells.push(cell_to_data(cell));
                    self.cells[row][col] = new;
                } else if range_start.is_some() {
                    // End of changed range — flush.
                    changes.push(CellRangeData {
                        row: row as u16,
                        col_start: range_start.unwrap() as u16,
                        cells: std::mem::take(&mut range_cells),
                    });
                    range_start = None;
                }
            }

            // Flush remaining range.
            if let Some(start) = range_start {
                changes.push(CellRangeData {
                    row: row as u16,
                    col_start: start as u16,
                    cells: range_cells,
                });
            }
        }

        let cursor_changed = cursor.row != self.cursor_row
            || cursor.col != self.cursor_col
            || cursor.visible != self.cursor_visible;

        if changes.is_empty() && !cursor_changed {
            return None; // Nothing changed.
        }

        self.cursor_row = cursor.row;
        self.cursor_col = cursor.col;
        self.cursor_visible = cursor.visible;

        Some(ScreenUpdateData {
            changes,
            cursor,
            cols: cols as u16,
            rows: rows as u16,
            title: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rterm_core::Terminal;

    #[test]
    fn snapshot_basic() {
        let mut t = Terminal::new(10, 3);
        t.feed(b"Hello");
        let ss = snapshot(t.screen());
        assert_eq!(ss.cols, 10);
        assert_eq!(ss.num_rows, 3);
        assert_eq!(ss.rows[0].cells[0].ch, 'H');
        assert_eq!(ss.rows[0].cells[4].ch, 'o');
        assert_eq!(ss.rows[0].cells[5].ch, ' ');
    }

    #[test]
    fn diff_detects_changes() {
        let mut t = Terminal::new(10, 3);
        t.feed(b"Hello");
        let ss = snapshot(t.screen());

        let mut prev = PrevScreen::new(10, 3);
        prev.update_from_snapshot(&ss);

        // No changes yet.
        assert!(prev.diff(t.screen()).is_none());

        // Write more text.
        t.feed(b" World");
        let update = prev.diff(t.screen()).unwrap();
        assert!(!update.changes.is_empty());

        // The changed cells should start where actual content changed.
        let first_change = &update.changes[0];
        assert_eq!(first_change.row, 0);
        // "World" starts at col 6 (col 5 was already a space).
        assert!(
            first_change.cells.iter().any(|c| c.ch == 'W'),
            "should contain 'W'"
        );
    }

    #[test]
    fn diff_cursor_only() {
        let mut t = Terminal::new(10, 3);
        t.feed(b"Hi");
        let ss = snapshot(t.screen());
        let mut prev = PrevScreen::new(10, 3);
        prev.update_from_snapshot(&ss);
        prev.diff(t.screen()); // consume initial diff

        // Move cursor without writing.
        t.feed(b"\x1b[1;1H");
        let update = prev.diff(t.screen()).unwrap();
        assert!(update.changes.is_empty());
        assert_eq!(update.cursor.row, 0);
        assert_eq!(update.cursor.col, 0);
    }

    #[test]
    fn color_packing_in_diff() {
        let mut t = Terminal::new(10, 3);
        t.feed(b"\x1b[31mRed\x1b[0m");
        let ss = snapshot(t.screen());
        assert_eq!(ss.rows[0].cells[0].fg, pack_color_indexed(1)); // red
        assert_eq!(ss.rows[0].cells[0].ch, 'R');
    }
}
