use crate::color::Color;

/// Visual attributes for a terminal cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CellAttributes {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub reverse: bool,
    pub dim: bool,
    pub hidden: bool,
}

impl CellAttributes {
    pub const NORMAL: Self = Self {
        bold: false,
        italic: false,
        underline: false,
        strikethrough: false,
        reverse: false,
        dim: false,
        hidden: false,
    };

    /// Returns true if all attributes are at their default (off) state.
    pub fn is_default(&self) -> bool {
        *self == Self::NORMAL
    }
}

/// A single terminal cell: one character with foreground color,
/// background color, and text attributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    /// The character displayed in this cell.
    /// A space (' ') for empty cells.
    pub ch: char,
    /// Foreground color.
    pub fg: Color,
    /// Background color.
    pub bg: Color,
    /// Text attributes (bold, italic, etc.).
    pub attrs: CellAttributes,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: Color::Default,
            bg: Color::Default,
            attrs: CellAttributes::NORMAL,
        }
    }
}

impl Cell {
    /// Create a cell with just a character and default style.
    pub fn with_char(ch: char) -> Self {
        Self {
            ch,
            ..Self::default()
        }
    }

    /// Reset the cell to a blank space with default colors and no attributes.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_cell_is_blank() {
        let cell = Cell::default();
        assert_eq!(cell.ch, ' ');
        assert_eq!(cell.fg, Color::Default);
        assert_eq!(cell.bg, Color::Default);
        assert!(cell.attrs.is_default());
    }

    #[test]
    fn cell_with_char() {
        let cell = Cell::with_char('A');
        assert_eq!(cell.ch, 'A');
        assert_eq!(cell.fg, Color::Default);
    }

    #[test]
    fn cell_reset() {
        let mut cell = Cell {
            ch: 'X',
            fg: Color::RED,
            bg: Color::BLUE,
            attrs: CellAttributes {
                bold: true,
                ..CellAttributes::NORMAL
            },
        };
        cell.reset();
        assert_eq!(cell, Cell::default());
    }

    #[test]
    fn attributes_normal_is_default() {
        let attrs = CellAttributes::default();
        assert!(attrs.is_default());
        assert!(!attrs.bold);
        assert!(!attrs.italic);
    }

    #[test]
    fn cell_is_copy() {
        let a = Cell::with_char('Z');
        let b = a;
        assert_eq!(a, b);
    }
}
