pub mod colors;
pub mod grid;
pub mod input;

pub use colors::to_egui_color;
pub use grid::{GridResult, ScrollState, Selection, TerminalGridConfig, terminal_grid};
pub use input::{encode_char, encode_key};
