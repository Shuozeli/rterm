use crate::color::Color;

bitflags::bitflags! {
    /// Cell attribute bitflags (alacritty-compatible layout).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct Flags: u16 {
        const INVERSE                  = 0x0001;
        const BOLD                     = 0x0002;
        const ITALIC                   = 0x0004;
        const BOLD_ITALIC              = 0x0006;
        const UNDERLINE                = 0x0008;
        const WRAPLINE                 = 0x0010;
        const WIDE_CHAR                = 0x0020;
        const WIDE_CHAR_SPACER         = 0x0040;
        const DIM                      = 0x0080;
        const HIDDEN                   = 0x0100;
        const STRIKEOUT                = 0x0200;
        const LEADING_WIDE_CHAR_SPACER = 0x0400;
        const DOUBLE_UNDERLINE         = 0x0800;
        const UNDERCURL                = 0x1000;
        const DOTTED_UNDERLINE         = 0x2000;
        const DASHED_UNDERLINE         = 0x4000;
        const ALL_UNDERLINES           = Self::UNDERLINE.bits()
                                       | Self::DOUBLE_UNDERLINE.bits()
                                       | Self::UNDERCURL.bits()
                                       | Self::DOTTED_UNDERLINE.bits()
                                       | Self::DASHED_UNDERLINE.bits();
    }
}

/// A single terminal cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub fg: Color,
    pub bg: Color,
    pub flags: Flags,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: Color::Default,
            bg: Color::Default,
            flags: Flags::empty(),
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
        assert!(cell.flags.is_empty());
        assert!(!cell.flags.contains(Flags::WIDE_CHAR_SPACER));
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
            flags: Flags::BOLD,
        };
        cell.reset();
        assert_eq!(cell, Cell::default());
        assert!(!cell.flags.contains(Flags::WIDE_CHAR_SPACER));
    }

    #[test]
    fn flags_default_is_empty() {
        let flags = Flags::default();
        assert!(flags.is_empty());
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

    #[test]
    fn all_underlines_covers_all_variants() {
        assert!(Flags::ALL_UNDERLINES.contains(Flags::UNDERLINE));
        assert!(Flags::ALL_UNDERLINES.contains(Flags::DOUBLE_UNDERLINE));
        assert!(Flags::ALL_UNDERLINES.contains(Flags::UNDERCURL));
        assert!(Flags::ALL_UNDERLINES.contains(Flags::DOTTED_UNDERLINE));
        assert!(Flags::ALL_UNDERLINES.contains(Flags::DASHED_UNDERLINE));
    }
}
