//! rterm-render: Shared terminal rendering crate for egui.
//!
//! Provides `DisplayGrid`, `paint_grid()`, and color utilities — the canonical
//! rendering implementation used by both the WASM browser client and the native
//! GUI demo.

pub mod cell;
pub mod colors;
pub mod grid;
pub mod paint;

pub use cell::{
    ATTR_ALL_UNDERLINES, ATTR_BOLD, ATTR_DASHED_UNDERLINE, ATTR_DIM, ATTR_DOTTED_UNDERLINE,
    ATTR_DOUBLE_UNDERLINE, ATTR_HIDDEN, ATTR_INVERSE, ATTR_ITALIC, ATTR_STRIKEOUT, ATTR_UNDERCURL,
    ATTR_UNDERLINE, ATTR_WIDE, ATTR_WIDE_SPACER, COLOR_DEFAULT, DisplayCell,
};
pub use colors::{ANSI_COLORS, indexed_to_color32, unpack_color32};
pub use grid::{DisplayCellRange, DisplayGrid, ScreenData, ScrollbackData};
pub use paint::paint_grid;
