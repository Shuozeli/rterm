//! Color unpacking from packed u32 protocol colors to egui Color32.

use egui::Color32;

pub const ANSI_COLORS: [Color32; 16] = [
    Color32::from_rgb(0, 0, 0),       // 0: black
    Color32::from_rgb(205, 0, 0),     // 1: red
    Color32::from_rgb(0, 205, 0),     // 2: green
    Color32::from_rgb(205, 205, 0),   // 3: yellow
    Color32::from_rgb(0, 0, 238),     // 4: blue
    Color32::from_rgb(205, 0, 205),   // 5: magenta
    Color32::from_rgb(0, 205, 205),   // 6: cyan
    Color32::from_rgb(229, 229, 229), // 7: white
    Color32::from_rgb(127, 127, 127), // 8: bright black
    Color32::from_rgb(255, 0, 0),     // 9: bright red
    Color32::from_rgb(0, 255, 0),     // 10: bright green
    Color32::from_rgb(255, 255, 0),   // 11: bright yellow
    Color32::from_rgb(92, 92, 255),   // 12: bright blue
    Color32::from_rgb(255, 0, 255),   // 13: bright magenta
    Color32::from_rgb(0, 255, 255),   // 14: bright cyan
    Color32::from_rgb(255, 255, 255), // 15: bright white
];

pub fn indexed_to_color32(idx: u8) -> Color32 {
    match idx {
        0..=15 => ANSI_COLORS[idx as usize],
        16..=231 => {
            let n = idx - 16;
            let b = (n % 6) as u32;
            let g = ((n / 6) % 6) as u32;
            let r = (n / 36) as u32;
            let to_val = |v: u32| -> u8 { if v == 0 { 0 } else { (55 + v * 40) as u8 } };
            Color32::from_rgb(to_val(r), to_val(g), to_val(b))
        }
        232..=255 => {
            let v = (8 + (idx - 232) as u32 * 10) as u8;
            Color32::from_rgb(v, v, v)
        }
    }
}

/// Unpack a packed u32 color to Color32.
///
/// Format: COLOR_DEFAULT (0xFFFFFFFF), indexed (0xFF0000NN, bits 8-23 zero),
/// or RGB (0xFFRRGGBB, bits 8-23 may be non-zero).
pub fn unpack_color32(packed: u32, default: Color32) -> Color32 {
    if packed == super::COLOR_DEFAULT {
        default
    } else if (packed & 0xFFFF0000) == 0xFF000000 {
        // Indexed color (bits 8-23 are zero) — use ANSI palette.
        indexed_to_color32((packed & 0xFF) as u8)
    } else {
        // RGB.
        Color32::from_rgb(
            ((packed >> 16) & 0xFF) as u8,
            ((packed >> 8) & 0xFF) as u8,
            (packed & 0xFF) as u8,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexed_standard_colors() {
        assert_eq!(indexed_to_color32(0), Color32::from_rgb(0, 0, 0));
        assert_eq!(indexed_to_color32(1), Color32::from_rgb(205, 0, 0));
        assert_eq!(indexed_to_color32(15), Color32::from_rgb(255, 255, 255));
    }

    #[test]
    fn indexed_216_cube() {
        assert_eq!(indexed_to_color32(16), Color32::from_rgb(0, 0, 0));
        assert_eq!(indexed_to_color32(196), Color32::from_rgb(255, 0, 0));
    }

    #[test]
    fn indexed_grayscale() {
        assert_eq!(indexed_to_color32(232), Color32::from_rgb(8, 8, 8));
        assert_eq!(indexed_to_color32(255), Color32::from_rgb(238, 238, 238));
    }

    #[test]
    fn unpack_rgb() {
        // 0xFFRRGGBB packed
        let c = unpack_color32(0xFFFF0000, Color32::BLACK);
        assert_eq!(c, Color32::from_rgb(255, 0, 0));
    }

    #[test]
    fn unpack_default() {
        let c = unpack_color32(crate::COLOR_DEFAULT, Color32::from_rgb(100, 150, 200));
        assert_eq!(c, Color32::from_rgb(100, 150, 200));
    }
}
