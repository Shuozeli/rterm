/// Terminal color representation.
///
/// Supports the default terminal color, 256-color indexed palette,
/// and 24-bit RGB true color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    /// The terminal's default foreground or background color.
    Default,
    /// Standard 8 colors (0-7) and bright variants (8-15),
    /// plus the 216-color cube (16-231) and grayscale ramp (232-255).
    Indexed(u8),
    /// 24-bit true color.
    Rgb(u8, u8, u8),
}

impl Default for Color {
    fn default() -> Self {
        Self::Default
    }
}

/// Named standard ANSI color indices for readability.
impl Color {
    pub const BLACK: Self = Self::Indexed(0);
    pub const RED: Self = Self::Indexed(1);
    pub const GREEN: Self = Self::Indexed(2);
    pub const YELLOW: Self = Self::Indexed(3);
    pub const BLUE: Self = Self::Indexed(4);
    pub const MAGENTA: Self = Self::Indexed(5);
    pub const CYAN: Self = Self::Indexed(6);
    pub const WHITE: Self = Self::Indexed(7);

    pub const BRIGHT_BLACK: Self = Self::Indexed(8);
    pub const BRIGHT_RED: Self = Self::Indexed(9);
    pub const BRIGHT_GREEN: Self = Self::Indexed(10);
    pub const BRIGHT_YELLOW: Self = Self::Indexed(11);
    pub const BRIGHT_BLUE: Self = Self::Indexed(12);
    pub const BRIGHT_MAGENTA: Self = Self::Indexed(13);
    pub const BRIGHT_CYAN: Self = Self::Indexed(14);
    pub const BRIGHT_WHITE: Self = Self::Indexed(15);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_color_is_default() {
        assert_eq!(Color::default(), Color::Default);
    }

    #[test]
    fn named_colors_have_correct_indices() {
        assert_eq!(Color::BLACK, Color::Indexed(0));
        assert_eq!(Color::WHITE, Color::Indexed(7));
        assert_eq!(Color::BRIGHT_RED, Color::Indexed(9));
    }

    #[test]
    fn rgb_color() {
        let c = Color::Rgb(255, 128, 0);
        assert_eq!(c, Color::Rgb(255, 128, 0));
    }

    #[test]
    fn color_is_copy() {
        let a = Color::RED;
        let b = a;
        assert_eq!(a, b);
    }
}
