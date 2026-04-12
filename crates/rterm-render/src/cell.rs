//! Terminal cell display representation (protocol-aligned).
//!
//! DisplayCell uses packed u32 colors and u16 flags to match the FlatBuffers
//! protocol's CellData layout. This is the canonical display cell type.

/// A cell in the display grid (matches the protocol's CellData).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DisplayCell {
    pub ch: char,
    /// Packed color: COLOR_DEFAULT, indexed (0xFF000000 | idx), or RGB (0xFFRRGGBB).
    pub fg: u32,
    pub bg: u32,
    /// Cell attribute flags (bit layout matches rterm_core::cell::Flags).
    pub flags: u16,
}

impl Default for DisplayCell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: COLOR_DEFAULT,
            bg: COLOR_DEFAULT,
            flags: 0,
        }
    }
}

// Attribute bitflags (must match rterm-core::cell::Flags bit layout).
pub const ATTR_INVERSE: u16 = 0x0001;
pub const ATTR_BOLD: u16 = 0x0002;
pub const ATTR_ITALIC: u16 = 0x0004;
pub const ATTR_UNDERLINE: u16 = 0x0008;
pub const ATTR_WIDE: u16 = 0x0020;
pub const ATTR_WIDE_SPACER: u16 = 0x0040;
pub const ATTR_DIM: u16 = 0x0080;
pub const ATTR_HIDDEN: u16 = 0x0100;
pub const ATTR_STRIKEOUT: u16 = 0x0200;
pub const ATTR_DOUBLE_UNDERLINE: u16 = 0x0800;
pub const ATTR_UNDERCURL: u16 = 0x1000;
pub const ATTR_DOTTED_UNDERLINE: u16 = 0x2000;
pub const ATTR_DASHED_UNDERLINE: u16 = 0x4000;
pub const ATTR_ALL_UNDERLINES: u16 = ATTR_UNDERLINE
    | ATTR_DOUBLE_UNDERLINE
    | ATTR_UNDERCURL
    | ATTR_DOTTED_UNDERLINE
    | ATTR_DASHED_UNDERLINE;

/// Sentinel value meaning "use the terminal's default color".
pub const COLOR_DEFAULT: u32 = 0xFFFFFFFF;
