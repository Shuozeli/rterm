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

    pub fn is_default(&self) -> bool {
        *self == Self::NORMAL
    }
}

/// A single terminal cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub fg: Color,
    pub bg: Color,
    pub attrs: CellAttributes,
    /// If true, this cell is the right half of a wide (CJK) character.
    /// The actual character is in the cell to the left.
    pub wide_continuation: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: Color::Default,
            bg: Color::Default,
            attrs: CellAttributes::NORMAL,
            wide_continuation: false,
        }
    }
}

impl Cell {
    pub fn with_char(ch: char) -> Self {
        Self {
            ch,
            ..Self::default()
        }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Check if a character is wide (takes 2 columns).
pub fn is_wide_char(ch: char) -> bool {
    unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1) > 1
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
        assert!(!cell.wide_continuation);
    }

    #[test]
    fn cell_with_char() {
        let cell = Cell::with_char('A');
        assert_eq!(cell.ch, 'A');
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
            wide_continuation: true,
        };
        cell.reset();
        assert_eq!(cell, Cell::default());
        assert!(!cell.wide_continuation);
    }

    #[test]
    fn attributes_normal_is_default() {
        let attrs = CellAttributes::default();
        assert!(attrs.is_default());
    }

    #[test]
    fn cell_is_copy() {
        let a = Cell::with_char('Z');
        let b = a;
        assert_eq!(a, b);
    }

    #[test]
    fn wide_char_detection() {
        assert!(is_wide_char('世')); // CJK
        assert!(is_wide_char('界'));
        assert!(is_wide_char('中'));
        assert!(!is_wide_char('A'));
        assert!(!is_wide_char(' '));
        assert!(!is_wide_char('─')); // box drawing is not wide
    }
}
