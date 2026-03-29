pub mod colors;
pub mod grid;
pub mod input;

pub use colors::to_egui_color;
pub use grid::{terminal_grid, GridResult, ScrollState, Selection, TerminalGridConfig};
pub use input::{encode_char, encode_key};
