//! Canonical terminal grid renderer using egui.
//!
//! Paints a DisplayGrid into an egui Ui. Handles cursor shapes, wide characters,
//! attribute decorations (underline variants, strikethrough), and selection highlighting.

use crate::cell::{
    ATTR_ALL_UNDERLINES, ATTR_BOLD, ATTR_DASHED_UNDERLINE, ATTR_DIM, ATTR_DOTTED_UNDERLINE,
    ATTR_DOUBLE_UNDERLINE, ATTR_HIDDEN, ATTR_INVERSE, ATTR_STRIKEOUT, ATTR_UNDERCURL,
    ATTR_UNDERLINE, ATTR_WIDE, ATTR_WIDE_SPACER,
};
use crate::colors::unpack_color32;
use crate::grid::DisplayGrid;
use egui::{Color32, FontFamily, FontId, Pos2, Rect, Sense, Ui, Vec2};

const DEFAULT_FG: Color32 = Color32::from_rgb(229, 229, 229);
const DEFAULT_BG: Color32 = Color32::from_rgb(0, 0, 0);

/// Paint the display grid using egui. Returns (response, cell_size, fit_cols, fit_rows).
pub fn paint_grid(
    ui: &mut Ui,
    grid: &DisplayGrid,
    font_size: f32,
) -> (egui::Response, Vec2, usize, usize) {
    let font_id = FontId::new(font_size, FontFamily::Monospace);
    let cell_size = ui.fonts_mut(|f| {
        let layout = f.layout_no_wrap("0".repeat(20), font_id.clone(), Color32::WHITE);
        let w = layout.rect.width() / 20.0;
        let h = f.row_height(&font_id);
        Vec2::new(w, h)
    });

    let available = ui.available_size();
    let fit_cols = (available.x / cell_size.x).floor().max(1.0) as usize;
    let fit_rows = (available.y / cell_size.y).floor().max(1.0) as usize;

    let grid_size = Vec2::new(cell_size.x * fit_cols as f32, cell_size.y * fit_rows as f32);

    let (response, painter) = ui.allocate_painter(grid_size, Sense::click_and_drag());
    let origin = response.rect.min;
    let grid_clip = Rect::from_min_size(origin, grid_size);

    painter.rect_filled(response.rect, 0.0, DEFAULT_BG);

    for row in 0..fit_rows {
        let y = origin.y + row as f32 * cell_size.y;
        for col in 0..fit_cols {
            let cell = grid.visible_cell(row, col);

            // Skip wide-char spacer cells (right half of CJK char).
            if cell.flags & ATTR_WIDE_SPACER != 0 {
                continue;
            }

            let (mut fg, mut bg) = (
                unpack_color32(cell.fg, DEFAULT_FG),
                unpack_color32(cell.bg, DEFAULT_BG),
            );

            if cell.flags & ATTR_INVERSE != 0 {
                std::mem::swap(&mut fg, &mut bg);
            }
            if cell.flags & ATTR_DIM != 0 {
                fg = Color32::from_rgba_premultiplied(
                    (fg.r() as u16 * 60 / 100) as u8,
                    (fg.g() as u16 * 60 / 100) as u8,
                    (fg.b() as u16 * 60 / 100) as u8,
                    fg.a(),
                );
            }
            if cell.flags & ATTR_HIDDEN != 0 {
                fg = bg;
            }

            let cell_rect =
                Rect::from_min_size(Pos2::new(origin.x + col as f32 * cell_size.x, y), cell_size);

            if bg != DEFAULT_BG {
                painter.rect_filled(cell_rect, 0.0, bg);
            }

            // Selection highlight.
            if grid.is_selected(row, col) {
                painter.rect_filled(
                    cell_rect,
                    0.0,
                    Color32::from_rgba_premultiplied(80, 120, 200, 100),
                );
            }

            if cell.flags & ATTR_WIDE != 0 {
                // This is a wide char — draw it spanning 2 cells.
                let wide_rect =
                    Rect::from_min_size(cell_rect.min, Vec2::new(cell_size.x * 2.0, cell_size.y));
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
                if cell.flags & ATTR_BOLD != 0 {
                    clipped.text(
                        Pos2::new(cell_rect.min.x + 0.5, cell_rect.min.y),
                        egui::Align2::LEFT_TOP,
                        cell.ch.to_string(),
                        font_id.clone(),
                        fg,
                    );
                }
            } else if cell.ch == ' ' && cell.flags & ATTR_ALL_UNDERLINES == 0 {
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
                // Bold: draw again with 0.5px offset for faux bold.
                if cell.flags & ATTR_BOLD != 0 {
                    clipped.text(
                        Pos2::new(cell_rect.min.x + 0.5, cell_rect.min.y),
                        egui::Align2::LEFT_TOP,
                        cell.ch.to_string(),
                        font_id.clone(),
                        fg,
                    );
                }
            }

            // Underline variants — all rendered as colored lines at different positions/styles.
            if cell.flags & ATTR_UNDERLINE != 0 {
                let ly = cell_rect.max.y - 2.0;
                painter.line_segment(
                    [
                        Pos2::new(cell_rect.min.x, ly),
                        Pos2::new(cell_rect.max.x, ly),
                    ],
                    egui::Stroke::new(1.0, fg),
                );
            } else if cell.flags & ATTR_DOUBLE_UNDERLINE != 0 {
                let ly1 = cell_rect.max.y - 3.0;
                let ly2 = cell_rect.max.y - 1.0;
                painter.line_segment(
                    [
                        Pos2::new(cell_rect.min.x, ly1),
                        Pos2::new(cell_rect.max.x, ly1),
                    ],
                    egui::Stroke::new(1.0, fg),
                );
                painter.line_segment(
                    [
                        Pos2::new(cell_rect.min.x, ly2),
                        Pos2::new(cell_rect.max.x, ly2),
                    ],
                    egui::Stroke::new(1.0, fg),
                );
            } else if cell.flags & (ATTR_UNDERCURL | ATTR_DOTTED_UNDERLINE | ATTR_DASHED_UNDERLINE)
                != 0
            {
                // Approximate curly/dotted/dashed underlines as a simple underline for now.
                let ly = cell_rect.max.y - 2.0;
                painter.line_segment(
                    [
                        Pos2::new(cell_rect.min.x, ly),
                        Pos2::new(cell_rect.max.x, ly),
                    ],
                    egui::Stroke::new(1.0, fg),
                );
            }

            if cell.flags & ATTR_STRIKEOUT != 0 {
                let ly = cell_rect.center().y;
                painter.line_segment(
                    [
                        Pos2::new(cell_rect.min.x, ly),
                        Pos2::new(cell_rect.max.x, ly),
                    ],
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
                    0.0,
                    cursor_color,
                );
            }
            3 | 4 => {
                // Underline cursor.
                painter.rect_filled(
                    Rect::from_min_size(
                        Pos2::new(cx, cy + cell_size.y - 3.0),
                        Vec2::new(cell_size.x, 3.0),
                    ),
                    0.0,
                    cursor_color,
                );
            }
            _ => {
                // Block cursor (default, 0, 1, 2).
                painter.rect_filled(
                    Rect::from_min_size(Pos2::new(cx, cy), cell_size),
                    0.0,
                    cursor_color,
                );
            }
        }
    }

    (response, cell_size, fit_cols, fit_rows)
}
