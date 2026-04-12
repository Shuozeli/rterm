use egui::{Pos2, Vec2};
use rterm_core::buffer::ScreenBuffer;
use rterm_core::cell::Flags;
use rterm_core::color::Color;
use rterm_render::paint_grid;
use rterm_render::{COLOR_DEFAULT, DisplayCell, DisplayCellRange, DisplayGrid, ScreenData};

const DEFAULT_FG: egui::Color32 = egui::Color32::from_rgb(229, 229, 229);
const DEFAULT_BG: egui::Color32 = egui::Color32::from_rgb(0, 0, 0);

/// Convert rterm-core Color to packed u32 (matches protocol format).
fn pack_color(color: &Color) -> u32 {
    match color {
        Color::Default => COLOR_DEFAULT,
        Color::Indexed(idx) => 0xFF000000u32 | (*idx as u32),
        Color::Rgb(r, g, b) => {
            0xFF000000u32 | ((*r as u32) << 16) | ((*g as u32) << 8) | (*b as u32)
        }
    }
}

/// Convert rterm-core Flags to u16 bits.
fn pack_flags(flags: Flags) -> u16 {
    flags.bits()
}

/// Render a ScreenBuffer by first building a DisplayGrid snapshot, then painting it.
pub fn render_screen_buffer(
    ui: &mut egui::Ui,
    buffer: &ScreenBuffer,
    config: &TerminalGridConfig,
    selection: &Selection,
) -> GridResult {
    let cols = buffer.cols();
    let rows = buffer.rows();

    let changes: Vec<DisplayCellRange> = (0..rows)
        .map(|row| {
            let cells: Vec<DisplayCell> = (0..cols)
                .map(|col| {
                    let cell = buffer.cell(row, col);
                    DisplayCell {
                        ch: cell.ch,
                        fg: pack_color(&cell.fg),
                        bg: pack_color(&cell.bg),
                        flags: pack_flags(cell.flags),
                    }
                })
                .collect();
            DisplayCellRange {
                row: row as u16,
                col_start: 0,
                cells,
            }
        })
        .collect();

    let data = ScreenData {
        changes,
        cursor_row: buffer.cursor.row as u16,
        cursor_col: buffer.cursor.col as u16,
        cursor_visible: buffer.cursor.visible,
        cursor_style: 0,
        cols: cols as u16,
        rows: rows as u16,
        mouse_tracking_mode: 0,
        alt_screen_active: false,
        application_cursor_keys: false,
        viewport_offset: 0,
    };

    let mut grid = DisplayGrid::new(cols, rows);
    grid.selection_start = selection.anchor;
    grid.selection_end = selection.end;
    grid.apply_snapshot(&data);

    let (response, cell_size, fit_cols, fit_rows) = paint_grid(ui, &grid, config.font_size);

    GridResult {
        response,
        cell_size,
        fit_cols,
        fit_rows,
    }
}

/// Text selection state (start and end in row,col coordinates).
#[derive(Debug, Clone, Default)]
pub struct Selection {
    pub anchor: Option<(usize, usize)>,
    pub end: Option<(usize, usize)>,
    pub active: bool,
}

impl Selection {
    pub fn range(&self) -> Option<(usize, usize, usize, usize)> {
        let (r1, c1) = self.anchor?;
        let (r2, c2) = self.end?;
        if (r1, c1) <= (r2, c2) {
            Some((r1, c1, r2, c2))
        } else {
            Some((r2, c2, r1, c1))
        }
    }

    pub fn contains(&self, row: usize, col: usize) -> bool {
        let Some((sr, sc, er, ec)) = self.range() else {
            return false;
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

    pub fn selected_text(&self, buffer: &ScreenBuffer) -> String {
        let Some((sr, sc, er, ec)) = self.range() else {
            return String::new();
        };
        let mut text = String::new();
        for row in sr..=er {
            if row >= buffer.rows() {
                break;
            }
            let col_start = if row == sr { sc } else { 0 };
            let col_end = if row == er {
                ec.min(buffer.cols() - 1)
            } else {
                buffer.cols() - 1
            };
            for col in col_start..=col_end {
                text.push(buffer.cell(row, col).ch);
            }
            if row < er {
                let trimmed = text.trim_end();
                text = trimmed.to_string();
                text.push('\n');
            }
        }
        text.trim_end().to_string()
    }
}

pub struct TerminalGridConfig {
    pub font_size: f32,
    pub default_fg: egui::Color32,
    pub default_bg: egui::Color32,
}

impl Default for TerminalGridConfig {
    fn default() -> Self {
        Self {
            font_size: 14.0,
            default_fg: DEFAULT_FG,
            default_bg: DEFAULT_BG,
        }
    }
}

/// Result returned from terminal_grid.
pub struct GridResult {
    pub response: egui::Response,
    pub cell_size: Vec2,
    pub fit_cols: usize,
    pub fit_rows: usize,
}

/// Convert a pixel position within the grid to a (row, col) cell coordinate.
pub fn pixel_to_cell(
    pos: Pos2,
    origin: Pos2,
    cell_size: Vec2,
    cols: usize,
    rows: usize,
) -> Option<(usize, usize)> {
    let x = pos.x - origin.x;
    let y = pos.y - origin.y;
    if x < 0.0 || y < 0.0 {
        return None;
    }
    let col = (x / cell_size.x) as usize;
    let row = (y / cell_size.y) as usize;
    if col < cols && row < rows {
        Some((row, col))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rterm_core::buffer::ScreenBuffer;

    #[test]
    fn pixel_to_cell_basic() {
        let cell_size = Vec2::new(8.0, 16.0);
        let origin = Pos2::new(10.0, 20.0);
        assert_eq!(
            pixel_to_cell(Pos2::new(10.0, 20.0), origin, cell_size, 80, 24),
            Some((0, 0))
        );
        assert_eq!(
            pixel_to_cell(Pos2::new(18.0, 20.0), origin, cell_size, 80, 24),
            Some((0, 1))
        );
    }

    #[test]
    fn pixel_to_cell_out_of_bounds() {
        let cell_size = Vec2::new(8.0, 16.0);
        let origin = Pos2::new(0.0, 0.0);
        assert_eq!(
            pixel_to_cell(Pos2::new(-1.0, 0.0), origin, cell_size, 80, 24),
            None
        );
    }

    #[test]
    fn selection_contains() {
        let sel = Selection {
            anchor: Some((1, 5)),
            end: Some((3, 10)),
            active: false,
        };
        assert!(!sel.contains(0, 5)); // before selection
        assert!(sel.contains(1, 5)); // start
        assert!(sel.contains(1, 79)); // rest of first line
        assert!(sel.contains(2, 0)); // middle line
        assert!(sel.contains(3, 0)); // last line start
        assert!(sel.contains(3, 10)); // last line end
        assert!(!sel.contains(3, 11)); // after selection
        assert!(!sel.contains(4, 0)); // below
    }

    #[test]
    fn selection_reversed() {
        let sel = Selection {
            anchor: Some((3, 10)),
            end: Some((1, 5)),
            active: false,
        };
        assert!(sel.contains(2, 0));
        assert!(sel.contains(1, 5));
    }

    #[test]
    fn selection_same_line() {
        let sel = Selection {
            anchor: Some((1, 3)),
            end: Some((1, 7)),
            active: false,
        };
        assert!(!sel.contains(1, 2));
        assert!(sel.contains(1, 3));
        assert!(sel.contains(1, 5));
        assert!(sel.contains(1, 7));
        assert!(!sel.contains(1, 8));
    }

    #[test]
    fn selected_text_basic() {
        let mut buf = ScreenBuffer::new(10, 3);
        for ch in "Hello".chars() {
            buf.write_char(ch);
        }
        buf.set_cursor_pos(2, 1);
        for ch in "World".chars() {
            buf.write_char(ch);
        }

        let sel = Selection {
            anchor: Some((0, 0)),
            end: Some((1, 4)),
            active: false,
        };
        let text = sel.selected_text(&buf);
        assert_eq!(text, "Hello\nWorld");
    }

    #[test]
    fn config_defaults() {
        let config = TerminalGridConfig::default();
        assert_eq!(config.font_size, 14.0);
    }
}
