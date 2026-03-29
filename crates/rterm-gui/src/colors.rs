use egui::Color32;
use rterm_core::Color;

/// Default 16-color ANSI palette (matches xterm defaults).
const ANSI_COLORS: [Color32; 16] = [
    Color32::from_rgb(0, 0, 0),       // 0: black
    Color32::from_rgb(205, 0, 0),     // 1: red
    Color32::from_rgb(0, 205, 0),     // 2: green
    Color32::from_rgb(205, 205, 0),   // 3: yellow
    Color32::from_rgb(0, 0, 238),     // 4: blue
    Color32::from_rgb(205, 0, 205),   // 5: magenta
    Color32::from_rgb(0, 205, 205),   // 6: cyan
    Color32::from_rgb(229, 229, 229), // 7: white
    Color32::from_rgb(127, 127, 127), // 8: bright black (gray)
    Color32::from_rgb(255, 0, 0),     // 9: bright red
    Color32::from_rgb(0, 255, 0),     // 10: bright green
    Color32::from_rgb(255, 255, 0),   // 11: bright yellow
    Color32::from_rgb(92, 92, 255),   // 12: bright blue
    Color32::from_rgb(255, 0, 255),   // 13: bright magenta
    Color32::from_rgb(0, 255, 255),   // 14: bright cyan
    Color32::from_rgb(255, 255, 255), // 15: bright white
];

/// Convert an rterm Color to an egui Color32.
///
/// `default_color` is used when the terminal color is `Color::Default`.
pub fn to_egui_color(color: &Color, default_color: Color32) -> Color32 {
    match color {
        Color::Default => default_color,
        Color::Indexed(idx) => indexed_to_color32(*idx),
        Color::Rgb(r, g, b) => Color32::from_rgb(*r, *g, *b),
    }
}

/// Convert a 256-color index to Color32.
fn indexed_to_color32(idx: u8) -> Color32 {
    match idx {
        // Standard 16 ANSI colors.
        0..=15 => ANSI_COLORS[idx as usize],
        // 216-color cube: 16 + 36*r + 6*g + b (r,g,b in 0..6).
        16..=231 => {
            let n = idx - 16;
            let b = (n % 6) as u32;
            let g = ((n / 6) % 6) as u32;
            let r = (n / 36) as u32;
            // Map 0..5 to 0, 95, 135, 175, 215, 255.
            let to_val = |v: u32| -> u8 { if v == 0 { 0 } else { (55 + v * 40) as u8 } };
            Color32::from_rgb(to_val(r), to_val(g), to_val(b))
        }
        // Grayscale ramp: 232..=255 -> 8, 18, 28, ..., 238.
        232..=255 => {
            let v = 8 + (idx - 232) as u32 * 10;
            let v = v as u8;
            Color32::from_rgb(v, v, v)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_returns_default_color() {
        let c = to_egui_color(&Color::Default, Color32::WHITE);
        assert_eq!(c, Color32::WHITE);
    }

    #[test]
    fn indexed_standard_colors() {
        assert_eq!(indexed_to_color32(0), Color32::from_rgb(0, 0, 0));
        assert_eq!(indexed_to_color32(1), Color32::from_rgb(205, 0, 0));
        assert_eq!(indexed_to_color32(15), Color32::from_rgb(255, 255, 255));
    }

    #[test]
    fn indexed_216_cube() {
        // Index 16 = rgb(0,0,0) in the cube.
        assert_eq!(indexed_to_color32(16), Color32::from_rgb(0, 0, 0));
        // Index 196 = 16 + 36*5 + 6*0 + 0 = pure red.
        assert_eq!(indexed_to_color32(196), Color32::from_rgb(255, 0, 0));
    }

    #[test]
    fn indexed_grayscale() {
        assert_eq!(indexed_to_color32(232), Color32::from_rgb(8, 8, 8));
        assert_eq!(indexed_to_color32(255), Color32::from_rgb(238, 238, 238));
    }

    #[test]
    fn rgb_color() {
        let c = to_egui_color(&Color::Rgb(100, 200, 50), Color32::BLACK);
        assert_eq!(c, Color32::from_rgb(100, 200, 50));
    }
}
