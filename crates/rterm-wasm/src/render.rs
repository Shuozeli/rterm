/// Thin terminal renderer: maintains a cell grid from server ScreenUpdate messages
/// and paints it using egui.
use crate::messages::{CellData, ScreenData, ATTR_BOLD, ATTR_DIM, ATTR_HIDDEN,
    ATTR_REVERSE, ATTR_STRIKETHROUGH, ATTR_UNDERLINE, ATTR_WIDE, COLOR_DEFAULT};
use egui::{Color32, FontFamily, FontId, Pos2, Rect, Sense, Ui, Vec2};

const DEFAULT_FG: Color32 = Color32::from_rgb(229, 229, 229);
const DEFAULT_BG: Color32 = Color32::from_rgb(0, 0, 0);

/// The display buffer — a 2D grid of cells received from the server.
pub struct DisplayGrid {
    cells: Vec<Vec<CellData>>,
    pub cols: usize,
    pub rows: usize,
    pub cursor_row: u16,
    pub cursor_col: u16,
    pub cursor_visible: bool,
    pub cursor_style: u8,
    /// Scrollback lines received from server (most recent first).
    pub scrollback: Vec<Vec<CellData>>,
    /// How many lines scrolled back (0 = live view).
    pub scroll_offset: usize,
    /// Total scrollback lines available on server.
    pub scrollback_total: u32,
    /// Selection state.
    pub selection_start: Option<(usize, usize)>,
    pub selection_end: Option<(usize, usize)>,
}

impl DisplayGrid {
    pub fn new(cols: usize, rows: usize) -> Self {
        let default_cell = CellData { ch: ' ', fg: COLOR_DEFAULT, bg: COLOR_DEFAULT, attrs: 0 };
        Self {
            cells: vec![vec![default_cell; cols]; rows],
            cols, rows,
            cursor_row: 0, cursor_col: 0, cursor_visible: true, cursor_style: 0,
            scrollback: Vec::new(),
            scroll_offset: 0,
            scrollback_total: 0,
            selection_start: None,
            selection_end: None,
        }
    }

    /// Apply a ScreenSnapshot (full screen replace).
    pub fn apply_snapshot(&mut self, data: &ScreenData) {
        let cols = data.cols as usize;
        let rows = data.rows as usize;
        let default_cell = CellData { ch: ' ', fg: COLOR_DEFAULT, bg: COLOR_DEFAULT, attrs: 0 };
        self.cols = cols;
        self.rows = rows;
        self.cells = vec![vec![default_cell; cols]; rows];

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
        self.scrollback_total = data.scrollback_len;
    }

    /// Apply a ScreenUpdate (diff — only changed cells).
    pub fn apply_update(&mut self, data: &ScreenData) {
        // Handle resize.
        if data.cols as usize != self.cols || data.rows as usize != self.rows {
            let default_cell = CellData { ch: ' ', fg: COLOR_DEFAULT, bg: COLOR_DEFAULT, attrs: 0 };
            self.cols = data.cols as usize;
            self.rows = data.rows as usize;
            self.cells.resize(self.rows, vec![default_cell; self.cols]);
            for row in &mut self.cells {
                row.resize(self.cols, default_cell);
            }
        }

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
        // Update scrollback count from server.
        if data.scrollback_len > 0 {
            self.scrollback_total = data.scrollback_len;
        }
    }

    /// Get the cell that should be visible at (row, col) accounting for scroll offset.
    /// This is the single source of truth for what the renderer shows.
    pub fn visible_cell(&self, row: usize, col: usize) -> &CellData {
        static DEFAULT: CellData = CellData {
            ch: ' ',
            fg: COLOR_DEFAULT,
            bg: COLOR_DEFAULT,
            attrs: 0,
        };

        if self.scroll_offset == 0 {
            // Live view.
            if row < self.cells.len() && col < self.cols {
                return &self.cells[row][col];
            }
            return &DEFAULT;
        }

        let sb_count = self.scrollback.len();
        if sb_count == 0 {
            return &DEFAULT;
        }

        // Scrolled back: show a window into scrollback + live screen.
        // scroll_offset = how many lines back from bottom.
        // View row 0 = scrollback[sb_count - scroll_offset]
        let sb_start = sb_count.saturating_sub(self.scroll_offset);
        let sb_idx = sb_start + row;

        if sb_idx < sb_count {
            // This row is in scrollback.
            self.scrollback
                .get(sb_idx)
                .and_then(|line| line.get(col))
                .unwrap_or(&DEFAULT)
        } else {
            // Past scrollback — show live screen.
            let screen_row = sb_idx - sb_count;
            if screen_row < self.cells.len() && col < self.cols {
                &self.cells[screen_row][col]
            } else {
                &DEFAULT
            }
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

    /// Apply scrollback data from server.
    pub fn apply_scrollback(&mut self, lines: &[super::messages::CellRange], offset: u32, total: u32) {
        self.scrollback_total = total;
        self.scrollback.clear();
        for line in lines {
            self.scrollback.push(line.cells.clone());
        }
    }

    /// Check if a cell is selected.
    pub fn is_selected(&self, row: usize, col: usize) -> bool {
        let Some((sr, sc)) = self.selection_start else { return false; };
        let Some((er, ec)) = self.selection_end else { return false; };
        let (sr, sc, er, ec) = if (sr, sc) <= (er, ec) {
            (sr, sc, er, ec)
        } else {
            (er, ec, sr, sc)
        };
        if row < sr || row > er { return false; }
        if row == sr && row == er { return col >= sc && col <= ec; }
        if row == sr { return col >= sc; }
        if row == er { return col <= ec; }
        true
    }

    /// Get selected text.
    pub fn selected_text(&self) -> String {
        let Some((sr, sc)) = self.selection_start else { return String::new(); };
        let Some((er, ec)) = self.selection_end else { return String::new(); };
        let (sr, sc, er, ec) = if (sr, sc) <= (er, ec) {
            (sr, sc, er, ec)
        } else {
            (er, ec, sr, sc)
        };
        let mut text = String::new();
        for row in sr..=er {
            if row >= self.rows { break; }
            let col_start = if row == sr { sc } else { 0 };
            let col_end = if row == er { ec.min(self.cols - 1) } else { self.cols - 1 };
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

/// Paint the display grid using egui. Returns (response, cell_size, fit_cols, fit_rows).
pub fn paint_grid(
    ui: &mut Ui,
    grid: &DisplayGrid,
    font_size: f32,
) -> (egui::Response, Vec2, usize, usize) {
    let font_id = FontId::new(font_size, FontFamily::Monospace);
    let cell_size = ui.fonts(|f| {
        let layout = f.layout_no_wrap("0".repeat(20), font_id.clone(), Color32::WHITE);
        let w = layout.rect.width() / 20.0;
        let h = f.row_height(&font_id);
        Vec2::new(w, h)
    });

    let available = ui.available_size();
    let fit_cols = (available.x / cell_size.x).floor().max(1.0) as usize;
    let fit_rows = (available.y / cell_size.y).floor().max(1.0) as usize;

    let grid_size = Vec2::new(
        cell_size.x * grid.cols as f32,
        cell_size.y * grid.rows as f32,
    );

    let (response, painter) = ui.allocate_painter(grid_size, Sense::click_and_drag());
    let origin = response.rect.min;
    let grid_clip = Rect::from_min_size(origin, grid_size);

    painter.rect_filled(response.rect, 0.0, DEFAULT_BG);

    for row in 0..grid.rows {
        let y = origin.y + row as f32 * cell_size.y;
        for col in 0..grid.cols {
            let cell = grid.visible_cell(row, col);
            let (mut fg, mut bg) = (unpack_color32(cell.fg, DEFAULT_FG), unpack_color32(cell.bg, DEFAULT_BG));

            if cell.attrs & ATTR_REVERSE != 0 { std::mem::swap(&mut fg, &mut bg); }
            if cell.attrs & ATTR_DIM != 0 {
                fg = Color32::from_rgba_premultiplied(
                    (fg.r() as u16 * 60 / 100) as u8,
                    (fg.g() as u16 * 60 / 100) as u8,
                    (fg.b() as u16 * 60 / 100) as u8,
                    fg.a(),
                );
            }
            if cell.attrs & ATTR_HIDDEN != 0 { fg = bg; }

            let cell_rect = Rect::from_min_size(
                Pos2::new(origin.x + col as f32 * cell_size.x, y),
                cell_size,
            );

            if bg != DEFAULT_BG {
                painter.rect_filled(cell_rect, 0.0, bg);
            }

            // Selection highlight.
            if grid.is_selected(row, col) {
                painter.rect_filled(cell_rect, 0.0, Color32::from_rgba_premultiplied(80, 120, 200, 100));
            }

            // Skip wide continuation cells (right half of CJK char).
            if cell.attrs & ATTR_WIDE != 0 {
                // This is a wide char — draw it spanning 2 cells.
                let wide_rect = Rect::from_min_size(
                    cell_rect.min,
                    Vec2::new(cell_size.x * 2.0, cell_size.y),
                );
                if bg != DEFAULT_BG {
                    painter.rect_filled(wide_rect, 0.0, bg);
                }
                let clipped = painter.with_clip_rect(wide_rect.intersect(grid_clip));
                clipped.text(
                    cell_rect.min,
                    egui::Align2::LEFT_TOP,
                    cell.ch.to_string(),
                    font_id.clone(),
                    fg,
                );
                // Bold: draw again with 1px offset for faux bold.
                if cell.attrs & ATTR_BOLD != 0 {
                    clipped.text(
                        Pos2::new(cell_rect.min.x + 0.5, cell_rect.min.y),
                        egui::Align2::LEFT_TOP,
                        cell.ch.to_string(),
                        font_id.clone(),
                        fg,
                    );
                }
            } else if cell.ch == ' ' && cell.attrs & (ATTR_UNDERLINE | ATTR_STRIKETHROUGH) == 0 {
                // Skip plain spaces.
            } else {
                let clipped = painter.with_clip_rect(cell_rect.intersect(grid_clip));
                clipped.text(
                    cell_rect.min,
                    egui::Align2::LEFT_TOP,
                    cell.ch.to_string(),
                    font_id.clone(),
                    fg,
                );
                // Bold: faux bold by drawing twice with slight offset.
                if cell.attrs & ATTR_BOLD != 0 {
                    clipped.text(
                        Pos2::new(cell_rect.min.x + 0.5, cell_rect.min.y),
                        egui::Align2::LEFT_TOP,
                        cell.ch.to_string(),
                        font_id.clone(),
                        fg,
                    );
                }
            }

            if cell.attrs & ATTR_UNDERLINE != 0 {
                let ly = cell_rect.max.y - 2.0;
                painter.line_segment(
                    [Pos2::new(cell_rect.min.x, ly), Pos2::new(cell_rect.max.x, ly)],
                    egui::Stroke::new(1.0, fg),
                );
            }
            if cell.attrs & ATTR_STRIKETHROUGH != 0 {
                let ly = cell_rect.center().y;
                painter.line_segment(
                    [Pos2::new(cell_rect.min.x, ly), Pos2::new(cell_rect.max.x, ly)],
                    egui::Stroke::new(1.0, fg),
                );
            }
        }
    }

    // Cursor — different shapes based on cursor_style.
    if grid.cursor_visible
        && (grid.cursor_row as usize) < grid.rows
        && (grid.cursor_col as usize) < grid.cols
    {
        let cx = origin.x + grid.cursor_col as f32 * cell_size.x;
        let cy = origin.y + grid.cursor_row as f32 * cell_size.y;
        let cursor_color = Color32::from_rgba_premultiplied(200, 200, 200, 180);

        match grid.cursor_style {
            5 | 6 => {
                // Bar cursor (thin vertical line).
                painter.rect_filled(
                    Rect::from_min_size(Pos2::new(cx, cy), Vec2::new(2.0, cell_size.y)),
                    0.0, cursor_color,
                );
            }
            3 | 4 => {
                // Underline cursor.
                painter.rect_filled(
                    Rect::from_min_size(
                        Pos2::new(cx, cy + cell_size.y - 3.0),
                        Vec2::new(cell_size.x, 3.0),
                    ),
                    0.0, cursor_color,
                );
            }
            _ => {
                // Block cursor (default, 0, 1, 2).
                painter.rect_filled(
                    Rect::from_min_size(Pos2::new(cx, cy), cell_size),
                    0.0, cursor_color,
                );
            }
        }
    }

    // Scroll indicator (zellij style: SCROLL: position/total).
    if grid.scroll_offset > 0 {
        let text = format!(" SCROLL: {}/{} ", grid.scroll_offset, grid.scrollback_total);
        let text_width = text.len() as f32 * cell_size.x * 0.6;
        let indicator_x = origin.x + grid_size.x - text_width - 4.0;
        let indicator_y = origin.y + 2.0;
        // Background for readability.
        painter.rect_filled(
            Rect::from_min_size(
                Pos2::new(indicator_x - 2.0, indicator_y),
                Vec2::new(text_width + 4.0, cell_size.y),
            ),
            2.0,
            Color32::from_rgba_premultiplied(40, 40, 40, 220),
        );
        painter.text(
            Pos2::new(indicator_x, indicator_y),
            egui::Align2::LEFT_TOP,
            text,
            font_id.clone(),
            Color32::from_rgb(255, 200, 0),
        );
    }

    (response, cell_size, fit_cols, fit_rows)
}

/// Unpack a packed u32 color to Color32.
fn unpack_color32(packed: u32, default: Color32) -> Color32 {
    if packed == COLOR_DEFAULT {
        default
    } else if packed & 0xFF000000 == 0xFF000000 {
        // Indexed color — use ANSI palette.
        indexed_to_color32((packed & 0xFF) as u8)
    } else {
        // RGB.
        Color32::from_rgb(
            ((packed >> 16) & 0xFF) as u8,
            ((packed >> 8) & 0xFF) as u8,
            (packed & 0xFF) as u8,
        )
    }
}

const ANSI_COLORS: [Color32; 16] = [
    Color32::from_rgb(0, 0, 0),
    Color32::from_rgb(205, 0, 0),
    Color32::from_rgb(0, 205, 0),
    Color32::from_rgb(205, 205, 0),
    Color32::from_rgb(0, 0, 238),
    Color32::from_rgb(205, 0, 205),
    Color32::from_rgb(0, 205, 205),
    Color32::from_rgb(229, 229, 229),
    Color32::from_rgb(127, 127, 127),
    Color32::from_rgb(255, 0, 0),
    Color32::from_rgb(0, 255, 0),
    Color32::from_rgb(255, 255, 0),
    Color32::from_rgb(92, 92, 255),
    Color32::from_rgb(255, 0, 255),
    Color32::from_rgb(0, 255, 255),
    Color32::from_rgb(255, 255, 255),
];

fn indexed_to_color32(idx: u8) -> Color32 {
    match idx {
        0..=15 => ANSI_COLORS[idx as usize],
        16..=231 => {
            let n = idx - 16;
            let b = (n % 6) as u32;
            let g = ((n / 6) % 6) as u32;
            let r = (n / 36) as u32;
            let to_val = |v: u32| -> u8 { if v == 0 { 0 } else { (55 + v * 40) as u8 } };
            Color32::from_rgb(to_val(r), to_val(g), to_val(b))
        }
        232..=255 => {
            let v = (8 + (idx - 232) as u32 * 10) as u8;
            Color32::from_rgb(v, v, v)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::{CellRange, ScreenData};

    fn make_cell(ch: char) -> CellData {
        CellData { ch, fg: COLOR_DEFAULT, bg: COLOR_DEFAULT, attrs: 0 }
    }

    fn make_line(text: &str, cols: usize) -> Vec<CellData> {
        let mut cells: Vec<CellData> = text.chars().map(|c| make_cell(c)).collect();
        cells.resize(cols, make_cell(' '));
        cells
    }

    fn make_screen_data(lines: &[&str], cols: u16, rows: u16) -> ScreenData {
        let changes: Vec<CellRange> = lines.iter().enumerate().map(|(i, text)| {
            CellRange {
                row: i as u16,
                col_start: 0,
                cells: text.chars().map(|c| make_cell(c)).collect(),
            }
        }).collect();
        ScreenData {
            changes,
            cursor_row: 0,
            cursor_col: 0,
            cursor_visible: true,
            cursor_style: 0,
            cols,
            rows,
            scrollback_len: 0,
        }
    }

    #[test]
    fn live_view_shows_screen_cells() {
        let mut grid = DisplayGrid::new(10, 3);
        let data = make_screen_data(&["Hello", "World", "Test"], 10, 3);
        grid.apply_snapshot(&data);

        assert_eq!(grid.scroll_offset, 0);
        assert_eq!(grid.visible_row_text(0), "Hello");
        assert_eq!(grid.visible_row_text(1), "World");
        assert_eq!(grid.visible_row_text(2), "Test");
    }

    #[test]
    fn scrollback_shows_old_lines() {
        let mut grid = DisplayGrid::new(10, 3);
        let data = make_screen_data(&["visible1", "visible2", "visible3"], 10, 3);
        grid.apply_snapshot(&data);

        // Simulate scrollback: 5 old lines received.
        grid.scrollback = vec![
            make_line("old1", 10),
            make_line("old2", 10),
            make_line("old3", 10),
            make_line("old4", 10),
            make_line("old5", 10),
        ];
        grid.scrollback_total = 5;

        // Scroll up 3 lines (show 3 rows of scrollback).
        grid.scroll_offset = 3;
        // sb_start = 5 - 3 = 2
        // row 0 = scrollback[2] = "old3"
        // row 1 = scrollback[3] = "old4"
        // row 2 = scrollback[4] = "old5"
        assert_eq!(grid.visible_row_text(0), "old3");
        assert_eq!(grid.visible_row_text(1), "old4");
        assert_eq!(grid.visible_row_text(2), "old5");
    }

    #[test]
    fn scroll_to_top() {
        let mut grid = DisplayGrid::new(10, 3);
        let data = make_screen_data(&["vis1", "vis2", "vis3"], 10, 3);
        grid.apply_snapshot(&data);

        grid.scrollback = vec![
            make_line("old1", 10),
            make_line("old2", 10),
            make_line("old3", 10),
            make_line("old4", 10),
            make_line("old5", 10),
        ];
        grid.scrollback_total = 5;

        // Scroll all the way up.
        grid.scroll_offset = 5;
        // sb_start = 5 - 5 = 0
        // row 0 = scrollback[0] = "old1"
        // row 1 = scrollback[1] = "old2"
        // row 2 = scrollback[2] = "old3"
        assert_eq!(grid.visible_row_text(0), "old1");
        assert_eq!(grid.visible_row_text(1), "old2");
        assert_eq!(grid.visible_row_text(2), "old3");
    }

    #[test]
    fn scroll_partial_shows_mix() {
        let mut grid = DisplayGrid::new(10, 4);
        let data = make_screen_data(&["scr1", "scr2", "scr3", "scr4"], 10, 4);
        grid.apply_snapshot(&data);

        grid.scrollback = vec![
            make_line("old1", 10),
            make_line("old2", 10),
        ];
        grid.scrollback_total = 2;

        // Scroll up 1 line.
        grid.scroll_offset = 1;
        // sb_start = 2 - 1 = 1
        // row 0 = scrollback[1] = "old2"
        // row 1 = past scrollback -> screen_row 0 = "scr1"
        // row 2 = screen_row 1 = "scr2"
        // row 3 = screen_row 2 = "scr3"
        assert_eq!(grid.visible_row_text(0), "old2");
        assert_eq!(grid.visible_row_text(1), "scr1");
        assert_eq!(grid.visible_row_text(2), "scr2");
        assert_eq!(grid.visible_row_text(3), "scr3");
    }

    #[test]
    fn scroll_offset_zero_shows_live() {
        let mut grid = DisplayGrid::new(10, 2);
        let data = make_screen_data(&["live1", "live2"], 10, 2);
        grid.apply_snapshot(&data);

        grid.scrollback = vec![make_line("old", 10)];
        grid.scroll_offset = 0; // Not scrolled.

        assert_eq!(grid.visible_row_text(0), "live1");
        assert_eq!(grid.visible_row_text(1), "live2");
    }

    #[test]
    fn scroll_with_no_scrollback_data() {
        let mut grid = DisplayGrid::new(10, 2);
        let data = make_screen_data(&["live1", "live2"], 10, 2);
        grid.apply_snapshot(&data);

        grid.scroll_offset = 5; // Scrolled but no data yet.
        // Should show blanks.
        assert_eq!(grid.visible_row_text(0), "");
        assert_eq!(grid.visible_row_text(1), "");
    }

    #[test]
    fn scroll_large_offset_with_many_lines() {
        let mut grid = DisplayGrid::new(20, 3);

        // Simulate 100 scrollback lines.
        let mut scrollback = Vec::new();
        for i in 1..=100 {
            scrollback.push(make_line(&format!("line{}", i), 20));
        }
        grid.scrollback = scrollback;
        grid.scrollback_total = 100;

        // Scroll to the very top.
        grid.scroll_offset = 100;
        assert_eq!(grid.visible_row_text(0), "line1");
        assert_eq!(grid.visible_row_text(1), "line2");
        assert_eq!(grid.visible_row_text(2), "line3");

        // Scroll to middle.
        grid.scroll_offset = 50;
        // sb_start = 100 - 50 = 50
        assert_eq!(grid.visible_row_text(0), "line51");
        assert_eq!(grid.visible_row_text(1), "line52");
        assert_eq!(grid.visible_row_text(2), "line53");

        // Scroll near bottom.
        grid.scroll_offset = 3;
        // sb_start = 100 - 3 = 97
        assert_eq!(grid.visible_row_text(0), "line98");
        assert_eq!(grid.visible_row_text(1), "line99");
        assert_eq!(grid.visible_row_text(2), "line100");
    }
}
