pub mod colors;
pub mod egui_harness;
pub mod grid;
pub mod input;

pub use colors::to_egui_color;
pub use egui_harness::{EguiRenderHarness, RenderedGrid};
pub use grid::{GridResult, Selection, TerminalGridConfig, terminal_grid};
pub use input::{encode_char, encode_key};
