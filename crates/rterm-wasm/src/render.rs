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

    pub selection_start: Option<(usize, usize)>,
    pub selection_end: Option<(usize, usize)>,
    pub mouse_tracking_mode: u8,
    pub alt_screen_active: bool,
    pub application_cursor_keys: bool,
}

impl DisplayGrid {
    pub fn new(cols: usize, rows: usize) -> Self {
        let default_cell = CellData { ch: ' ', fg: COLOR_DEFAULT, bg: COLOR_DEFAULT, attrs: 0 };
        Self {
            cells: vec![vec![default_cell; cols]; rows],
            cols, rows,
            cursor_row: 0, cursor_col: 0, cursor_visible: true, cursor_style: 0,

            selection_start: None,
            selection_end: None,
            mouse_tracking_mode: 0,
            alt_screen_active: false,
            application_cursor_keys: false,
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

        self.mouse_tracking_mode = data.mouse_tracking_mode;
        self.alt_screen_active = data.alt_screen_active;
        self.application_cursor_keys = data.application_cursor_keys;
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

    /// Get the cell that should be visible at (row, col) accounting for scroll offset.
    /// This is the single source of truth for what the renderer shows.
    pub fn visible_cell(&self, row: usize, col: usize) -> &CellData {
        static DEFAULT: CellData = CellData {
            ch: ' ',
            fg: COLOR_DEFAULT,
            bg: COLOR_DEFAULT,
            attrs: 0,
        };

        if row < self.cells.len() && col < self.cols {
            &self.cells[row][col]
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
            mouse_tracking_mode: 0,
            alt_screen_active: false,
            application_cursor_keys: false,
        }
    }

    #[test]
    fn live_view_shows_screen_cells() {
        let mut grid = DisplayGrid::new(10, 3);
        let data = make_screen_data(&["Hello", "World", "Test"], 10, 3);
        grid.apply_snapshot(&data);
        assert_eq!(grid.visible_row_text(0), "Hello");
        assert_eq!(grid.visible_row_text(1), "World");
        assert_eq!(grid.visible_row_text(2), "Test");
    }


}
