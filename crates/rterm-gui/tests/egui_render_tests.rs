/// Integration tests: headless egui rendering of terminal grids.
///
/// These tests run egui headlessly, call `terminal_grid()` to paint a
/// ScreenBuffer, then extract the rendered text from Shape::Text entries.
/// This tests the actual rendering path — the same code that paints the
/// terminal in both native and WASM builds.
use rterm_core::buffer::ScreenBuffer;
use rterm_gui::egui_harness::{EguiRenderHarness, fill_buffer};

// ============================================================================
// Tests: basic rendering
// ============================================================================

#[test]
fn live_view_renders_screen_content() {
    let mut buf = ScreenBuffer::new(20, 5);
    fill_buffer(&mut buf, &["Hello", "World", "Test"]);

    let mut h = EguiRenderHarness::new(20, 5, 14.0);
    let grid = h.render(&buf);

    grid.assert_row(0, "Hello");
    grid.assert_row(1, "World");
    grid.assert_row(2, "Test");
    grid.assert_row(3, "");
    grid.assert_row(4, "");
}

#[test]
fn multiline_content_renders() {
    let mut buf = ScreenBuffer::new(20, 5);
    fill_buffer(&mut buf, &["AAAA", "BBBB", "CCCC", "DDDD", "EEEE"]);

    let mut h = EguiRenderHarness::new(20, 5, 14.0);
    let grid = h.render(&buf);

    grid.assert_row(0, "AAAA");
    grid.assert_row(1, "BBBB");
    grid.assert_row(2, "CCCC");
    grid.assert_row(3, "DDDD");
    grid.assert_row(4, "EEEE");
}

#[test]
fn individual_cells_render_correctly() {
    let mut buf = ScreenBuffer::new(10, 3);
    fill_buffer(&mut buf, &["ABC"]);

    let mut h = EguiRenderHarness::new(10, 3, 14.0);
    let grid = h.render(&buf);

    grid.assert_cell(0, 0, 'A');
    grid.assert_cell(0, 1, 'B');
    grid.assert_cell(0, 2, 'C');
    grid.assert_cell(0, 3, ' ');
}

#[test]
fn harness_empty_screen() {
    let buf = ScreenBuffer::new(10, 3);
    let mut h = EguiRenderHarness::new(10, 3, 14.0);
    let grid = h.render(&buf);

    // Empty buffer should render all empty rows.
    grid.assert_row(0, "");
    grid.assert_row(1, "");
    grid.assert_row(2, "");
}
