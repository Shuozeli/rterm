// Re-export the canonical rendering types from rterm-render.
pub use rterm_render::{
    DisplayCellRange, DisplayGrid, ScreenData, ScrollbackData, paint_grid, COLOR_DEFAULT,
    ATTR_ALL_UNDERLINES, ATTR_BOLD, ATTR_DASHED_UNDERLINE,
    ATTR_DOTTED_UNDERLINE, ATTR_DOUBLE_UNDERLINE, ATTR_HIDDEN,
    ATTR_INVERSE, ATTR_STRIKEOUT, ATTR_UNDERCURL, ATTR_UNDERLINE, ATTR_WIDE,
    ATTR_WIDE_SPACER, ATTR_DIM,
};

pub type CellData = rterm_render::DisplayCell;
