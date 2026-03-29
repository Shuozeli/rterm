use crate::colors::to_egui_color;
use egui::{Color32, FontFamily, FontId, Pos2, Rect, Sense, Ui, Vec2};
use rterm_core::buffer::ScreenBuffer;
use rterm_core::cell::CellAttributes;
use rterm_core::color::Color;

const DEFAULT_FG: Color32 = Color32::from_rgb(229, 229, 229);
const DEFAULT_BG: Color32 = Color32::from_rgb(0, 0, 0);
const SELECTION_BG: Color32 = Color32::from_rgba_premultiplied(80, 120, 200, 100);

pub struct TerminalGridConfig {
    pub font_size: f32,
    pub default_fg: Color32,
    pub default_bg: Color32,
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

/// Text selection state (start and end in row,col coordinates).
#[derive(Debug, Clone, Default)]
pub struct Selection {
    /// Anchor point (where mouse was pressed).
    pub anchor: Option<(usize, usize)>,
    /// Current end point (where mouse is now).
    pub end: Option<(usize, usize)>,
    /// Whether a selection is active (mouse is being dragged).
    pub active: bool,
}

impl Selection {
    /// Get the normalized selection range: (start_row, start_col, end_row, end_col).
    /// Start is always before end in reading order.
    pub fn range(&self) -> Option<(usize, usize, usize, usize)> {
        let (r1, c1) = self.anchor?;
        let (r2, c2) = self.end?;
        if (r1, c1) <= (r2, c2) {
            Some((r1, c1, r2, c2))
        } else {
            Some((r2, c2, r1, c1))
        }
    }

    /// Check if a cell is within the selection.
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
        true // middle row — fully selected
    }

    /// Extract selected text from the buffer.
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
            // Trim trailing spaces from each line, add newline between lines.
            if row < er {
                let trimmed = text.trim_end();
                text = trimmed.to_string();
                text.push('\n');
            }
        }
        text.trim_end().to_string()
    }
}

/// Result returned from terminal_grid with layout info.
pub struct GridResult {
    pub response: egui::Response,
    pub cell_size: Vec2,
    /// Terminal dimensions that fit the available space (cols, rows).
    pub fit_cols: usize,
    pub fit_rows: usize,
}

/// Scroll state for the terminal grid.
#[derive(Debug, Clone, Default)]
pub struct ScrollState {
    /// How many lines scrolled back from the bottom.
    /// 0 = at bottom (showing live terminal), >0 = viewing scrollback.
    pub offset: usize,
}

/// Paint a terminal ScreenBuffer into an egui Ui.
/// Fills the available space and returns the fitted dimensions.
pub fn terminal_grid(
    ui: &mut Ui,
    buffer: &ScreenBuffer,
    config: &TerminalGridConfig,
    selection: &Selection,
    scroll: &mut ScrollState,
) -> GridResult {
    let font_id = FontId::new(config.font_size, FontFamily::Monospace);
    let cell_size = measure_cell_size(ui, &font_id);

    // Calculate how many cols/rows fit in the available space.
    let available = ui.available_size();
    let fit_cols = (available.x / cell_size.x).floor().max(1.0) as usize;
    let fit_rows = (available.y / cell_size.y).floor().max(1.0) as usize;

    let cols = buffer.cols();
    let rows = buffer.rows();
    let grid_size = Vec2::new(cell_size.x * cols as f32, cell_size.y * rows as f32);

    let (response, painter) = ui.allocate_painter(grid_size, Sense::click_and_drag());
    let origin = response.rect.min;

    // Paint full background.
    painter.rect_filled(response.rect, 0.0, config.default_bg);

    // Clip to grid bounds — prevents character overflow.
    let grid_clip = Rect::from_min_size(origin, grid_size);

    // Handle mouse wheel scrolling.
    let scroll_delta = ui.input(|i| {
        i.events.iter().filter_map(|e| {
            if let egui::Event::MouseWheel { delta, .. } = e {
                Some(delta.y)
            } else {
                None
            }
        }).sum::<f32>()
    });
    if scroll_delta != 0.0 && response.hovered() {
        let scroll_lines = (scroll_delta / 3.0).round() as isize;
        let max_scroll = buffer.scrollback_len();
        let new_offset = (scroll.offset as isize - scroll_lines)
            .max(0)
            .min(max_scroll as isize) as usize;
        scroll.offset = new_offset;
    }

    // When new output arrives (scrollback grows), auto-scroll to bottom.
    // Only if we're already at the bottom.
    // (The caller should reset scroll.offset = 0 when new data arrives and offset == 0.)

    let scrollback_len = buffer.scrollback_len();
    let scroll_offset = scroll.offset.min(scrollback_len);

    // Paint row by row. When scroll_offset > 0, show scrollback lines at top.
    let sb_visible = scroll_offset.min(rows);

    for view_row in 0..rows {
        let y = origin.y + view_row as f32 * cell_size.y;

        for col in 0..cols {
            let cell = if view_row < sb_visible {
                // Scrollback line.
                let sb_line = scrollback_len - scroll_offset + view_row;
                buffer.scrollback_cell(sb_line, col)
            } else {
                // Screen line.
                let screen_row = view_row - sb_visible;
                buffer.cell(screen_row, col)
            };
            let (fg, bg) = resolve_colors(cell.fg, cell.bg, cell.attrs.reverse, config);
            let fg = apply_dim_hidden(fg, bg, &cell.attrs);

            let cell_rect = Rect::from_min_size(
                Pos2::new(origin.x + col as f32 * cell_size.x, y),
                cell_size,
            );

            // Background.
            if bg != config.default_bg {
                painter.rect_filled(cell_rect, 0.0, bg);
            }

            // Selection overlay.
            if selection.contains(view_row, col) {
                painter.rect_filled(cell_rect, 0.0, SELECTION_BG);
            }

            // Character — skip spaces unless decorated.
            if cell.ch != ' ' || cell.attrs.underline || cell.attrs.strikethrough {
                // Clip to cell bounds to prevent wide chars from bleeding.
                let clipped = painter.with_clip_rect(cell_rect.intersect(grid_clip));
                clipped.text(
                    cell_rect.min,
                    egui::Align2::LEFT_TOP,
                    cell.ch.to_string(),
                    font_id.clone(),
                    fg,
                );
            }

            // Underline.
            if cell.attrs.underline {
                let line_y = cell_rect.max.y - 2.0;
                painter.line_segment(
                    [Pos2::new(cell_rect.min.x, line_y), Pos2::new(cell_rect.max.x, line_y)],
                    egui::Stroke::new(1.0, fg),
                );
            }

            // Strikethrough.
            if cell.attrs.strikethrough {
                let line_y = cell_rect.center().y;
                painter.line_segment(
                    [Pos2::new(cell_rect.min.x, line_y), Pos2::new(cell_rect.max.x, line_y)],
                    egui::Stroke::new(1.0, fg),
                );
            }
        }
    }

    // Paint cursor — only when not scrolled up (viewing live terminal).
    if scroll_offset == 0 && buffer.cursor.visible && buffer.cursor.row < rows && buffer.cursor.col < cols {
        let cursor_rect = Rect::from_min_size(
            Pos2::new(
                origin.x + buffer.cursor.col as f32 * cell_size.x,
                origin.y + buffer.cursor.row as f32 * cell_size.y,
            ),
            cell_size,
        );
        painter.rect_filled(
            cursor_rect,
            0.0,
            Color32::from_rgba_premultiplied(200, 200, 200, 160),
        );
    }

    // Show scroll indicator when scrolled up.
    if scroll_offset > 0 {
        let indicator = format!("↑ {} lines", scroll_offset);
        painter.text(
            Pos2::new(origin.x + grid_size.x - 120.0, origin.y + 2.0),
            egui::Align2::LEFT_TOP,
            indicator,
            font_id.clone(),
            Color32::from_rgb(255, 200, 0),
        );
    }

    GridResult {
        response,
        cell_size,
        fit_cols,
        fit_rows,
    }
}

fn resolve_colors(
    fg: Color,
    bg: Color,
    reverse: bool,
    config: &TerminalGridConfig,
) -> (Color32, Color32) {
    let fg32 = to_egui_color(&fg, config.default_fg);
    let bg32 = to_egui_color(&bg, config.default_bg);
    if reverse {
        (bg32, fg32)
    } else {
        (fg32, bg32)
    }
}

fn apply_dim_hidden(fg: Color32, bg: Color32, attrs: &CellAttributes) -> Color32 {
    if attrs.hidden {
        bg
    } else if attrs.dim {
        // Dim: reduce brightness by ~40% (not 50% which is too dark).
        let dim = |v: u8| -> u8 { (v as u16 * 60 / 100) as u8 };
        Color32::from_rgba_premultiplied(dim(fg.r()), dim(fg.g()), dim(fg.b()), fg.a())
    } else {
        fg
    }
}

/// Measure cell size using multiple characters for accuracy.
fn measure_cell_size(ui: &Ui, font_id: &FontId) -> Vec2 {
    ui.fonts(|f| {
        // Measure a long string of identical chars to get accurate per-char width.
        // Dividing avoids rounding errors from single-char measurement.
        let n = 20;
        let test_str = "0".repeat(n);
        let layout = f.layout_no_wrap(test_str, font_id.clone(), Color32::WHITE);
        let w = layout.rect.width() / n as f32;

        // Height from row_height is more reliable than layout rect.
        let h = f.row_height(font_id);

        Vec2::new(w, h)
    })
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
    use rterm_core::ScreenBuffer;

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
        // Same as above — range() normalizes.
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
