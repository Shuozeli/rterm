/// Headless egui rendering test harness for terminal grids.
///
/// Runs egui's `Context` without a window, calls `terminal_grid()` to paint,
/// then extracts `Shape::Text` entries from the paint output to verify what
/// characters appear at each grid position.
///
/// This catches rendering bugs (wrong scroll direction, wrong row mapping,
/// off-by-one errors) that data-model-only tests miss.
use crate::grid::{Selection, TerminalGridConfig, render_screen_buffer};
use egui::{Pos2, Rect, Shape, Vec2};
use rterm_core::buffer::ScreenBuffer;
use std::collections::HashMap;

/// A rendered grid extracted from egui paint output.
/// Maps (row, col) → char based on what `paint_grid` / `terminal_grid` actually painted.
#[derive(Debug)]
pub struct RenderedGrid {
    /// Characters extracted from Shape::Text output, keyed by (row, col).
    chars: HashMap<(usize, usize), char>,
    pub cols: usize,
    pub rows: usize,
    /// Whether a cursor rectangle was found (non-text rect at expected cursor position).
    pub has_cursor_rect: bool,
}

impl RenderedGrid {
    /// Get the rendered text for a row (assembled from individual cell characters).
    /// Trailing spaces are trimmed, like `ScreenBuffer::row_text()`.
    pub fn row_text(&self, row: usize) -> String {
        let mut text = String::new();
        for col in 0..self.cols {
            let ch = self.chars.get(&(row, col)).copied().unwrap_or(' ');
            text.push(ch);
        }
        text.trim_end().to_string()
    }

    /// Assert a row matches expected text (trimmed).
    /// Panics with a helpful message on mismatch.
    pub fn assert_row(&self, row: usize, expected: &str) {
        let actual = self.row_text(row);
        assert_eq!(
            actual, expected,
            "row {} mismatch: rendered='{}', expected='{}'",
            row, actual, expected
        );
    }

    /// Assert the character at a specific cell.
    pub fn assert_cell(&self, row: usize, col: usize, expected: char) {
        let actual = self.chars.get(&(row, col)).copied().unwrap_or(' ');
        assert_eq!(
            actual, expected,
            "cell ({},{}) mismatch: rendered='{}', expected='{}'",
            row, col, actual, expected
        );
    }

    /// Get the character at a cell (space if nothing was painted).
    pub fn cell(&self, row: usize, col: usize) -> char {
        self.chars.get(&(row, col)).copied().unwrap_or(' ')
    }

    /// Dump all rendered rows as text (for debugging).
    pub fn dump(&self) -> Vec<String> {
        (0..self.rows).map(|r| self.row_text(r)).collect()
    }
}

/// Headless egui rendering harness.
///
/// Creates an egui `Context` without a window, renders a `ScreenBuffer`
/// via `terminal_grid()`, and extracts the painted characters.
pub struct EguiRenderHarness {
    ctx: egui::Context,
    font_size: f32,
    screen_width: f32,
    screen_height: f32,
    /// Cell size (computed on first render).
    cell_size: Option<Vec2>,
}

impl EguiRenderHarness {
    /// Create a new harness sized for the given terminal dimensions.
    ///
    /// Uses a generous screen size to ensure the terminal fits.
    /// `font_size` matches rterm's default (14.0).
    pub fn new(cols: usize, rows: usize, font_size: f32) -> Self {
        // Estimate cell size: monospace at 14pt ≈ 8.4 x 18 px.
        // Use generous padding to ensure the grid fits.
        let est_cell_w = font_size * 0.6 + 1.0;
        let est_cell_h = font_size * 1.3 + 2.0;
        Self {
            ctx: egui::Context::default(),
            font_size,
            screen_width: est_cell_w * cols as f32 + 50.0,
            screen_height: est_cell_h * rows as f32 + 50.0,
            cell_size: None,
        }
    }

    /// Render a ScreenBuffer with the given scroll state, return the extracted grid.
    ///
    /// This runs a full egui frame headlessly:
    /// 1. Sets up RawInput with a screen_rect
    /// 2. Calls `terminal_grid()` inside `CentralPanel`
    /// 3. Collects all `Shape::Text` entries from the output
    /// 4. Maps text positions to (row, col) using cell size
    pub fn render(&mut self, buffer: &ScreenBuffer) -> RenderedGrid {
        let config = TerminalGridConfig {
            font_size: self.font_size,
            ..Default::default()
        };
        let selection = Selection::default();
        let cols = buffer.cols();
        let rows = buffer.rows();

        let raw_input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                Pos2::ZERO,
                Vec2::new(self.screen_width, self.screen_height),
            )),
            ..Default::default()
        };

        // We need to run two frames: first to let egui initialize fonts/layout,
        // second to get stable rendering.
        let _ = self.ctx.run_ui(raw_input.clone(), |ui| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(egui::Color32::BLACK))
                .show_inside(ui, |ui| {
                    render_screen_buffer(ui, buffer, &config, &selection);
                });
        });

        // Second frame captures the actual paint output.
        let mut captured_cell_size = Vec2::ZERO;
        let output = self.ctx.run_ui(raw_input, |ui| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(egui::Color32::BLACK))
                .show_inside(ui, |ui| {
                    let result = render_screen_buffer(ui, buffer, &config, &selection);
                    captured_cell_size = result.cell_size;
                });
        });

        if captured_cell_size.x > 0.0 {
            self.cell_size = Some(captured_cell_size);
        }

        let cell_size = self.cell_size.unwrap_or(Vec2::new(8.4, 18.0));

        // Extract text shapes from the paint output.
        self.extract_grid(&output.shapes, cell_size, cols, rows)
    }

    /// Extract a RenderedGrid from egui paint shapes.
    fn extract_grid(
        &self,
        shapes: &[egui::epaint::ClippedShape],
        cell_size: Vec2,
        cols: usize,
        rows: usize,
    ) -> RenderedGrid {
        let mut chars: HashMap<(usize, usize), char> = HashMap::new();
        let mut has_cursor_rect = false;

        // Walk all shapes recursively.
        let mut shape_stack: Vec<&Shape> = shapes.iter().map(|cs| &cs.shape).collect();

        while let Some(shape) = shape_stack.pop() {
            match shape {
                Shape::Text(text_shape) => {
                    let text = &text_shape.galley.job.text;
                    let pos = text_shape.pos;

                    // Map position to grid (row, col).
                    // The origin of the grid is at (0,0) or wherever CentralPanel starts.
                    // We compute row and col from the text position relative to cell_size.
                    if cell_size.x > 0.0 && cell_size.y > 0.0 {
                        let col_f = pos.x / cell_size.x;
                        let row_f = pos.y / cell_size.y;

                        // Round to nearest integer cell position.
                        let col = col_f.round() as isize;
                        let row = row_f.round() as isize;

                        if row >= 0 && (row as usize) < rows && col >= 0 && (col as usize) < cols {
                            // Extract the first character from the text.
                            if let Some(ch) = text.chars().next()
                                && ch != ' '
                            {
                                chars.insert((row as usize, col as usize), ch);
                            }
                        }
                    }
                }
                Shape::Rect(rect_shape) => {
                    // Detect cursor: a semi-transparent rect at a cell position.
                    let fill = rect_shape.fill;
                    if fill.a() > 0 && fill.a() < 255 && fill.r() > 150 {
                        // This could be a cursor rectangle.
                        has_cursor_rect = true;
                    }
                }
                Shape::Vec(v) => {
                    // Recurse into shape vectors.
                    for s in v {
                        shape_stack.push(s);
                    }
                }
                _ => {}
            }
        }

        RenderedGrid {
            chars,
            cols,
            rows,
            has_cursor_rect,
        }
    }
}

/// Helper: populate a ScreenBuffer with text lines.
/// Each line is written starting at row 0, advancing with CR+LF.
pub fn fill_buffer(buffer: &mut ScreenBuffer, lines: &[&str]) {
    buffer.set_cursor_pos(1, 1);
    for (i, line) in lines.iter().enumerate() {
        buffer.set_cursor_pos(i + 1, 1);
        for ch in line.chars() {
            buffer.write_char(ch);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rendered_grid_row_text_trims() {
        let mut g = RenderedGrid {
            chars: HashMap::new(),
            cols: 10,
            rows: 2,
            has_cursor_rect: false,
        };
        g.chars.insert((0, 0), 'H');
        g.chars.insert((0, 1), 'i');
        assert_eq!(g.row_text(0), "Hi");
        assert_eq!(g.row_text(1), "");
    }

    #[test]
    fn rendered_grid_cell_default_space() {
        let g = RenderedGrid {
            chars: HashMap::new(),
            cols: 5,
            rows: 2,
            has_cursor_rect: false,
        };
        assert_eq!(g.cell(0, 0), ' ');
    }

    #[test]
    fn fill_buffer_helper() {
        let mut buf = ScreenBuffer::new(10, 3);
        fill_buffer(&mut buf, &["Hello", "World"]);
        assert_eq!(buf.row_text(0), "Hello");
        assert_eq!(buf.row_text(1), "World");
    }
}
