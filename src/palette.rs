/// Default ANSI palette (colors 0-15, 3 bytes each = 48 bytes)
pub const DEFAULT_PALETTE: [u8; 48] = [
    // Standard colors (0-7)
    0x00, 0x00, 0x00, // 0: Black
    0xCC, 0x00, 0x00, // 1: Red
    0x00, 0xCC, 0x00, // 2: Green
    0xCC, 0xCC, 0x00, // 3: Yellow
    0x00, 0x00, 0xCC, // 4: Blue
    0xCC, 0x00, 0xCC, // 5: Magenta
    0x00, 0xCC, 0xCC, // 6: Cyan
    0xCC, 0xCC, 0xCC, // 7: White
    // Bright colors (8-15)
    0x66, 0x66, 0x66, // 8: Bright Black (Gray)
    0xFF, 0x00, 0x00, // 9: Bright Red
    0x00, 0xFF, 0x00, // 10: Bright Green
    0xFF, 0xFF, 0x00, // 11: Bright Yellow
    0x00, 0x00, 0xFF, // 12: Bright Blue
    0xFF, 0x00, 0xFF, // 13: Bright Magenta
    0x00, 0xFF, 0xFF, // 14: Bright Cyan
    0xFF, 0xFF, 0xFF, // 15: Bright White
];

/// Convert indexed color (16-231) from 6x6x6 cube to RGB
pub fn idx_to_rgb(idx: u8) -> (u8, u8, u8) {
    if idx < 16 {
        // Should use palette lookup, not this function
        panic!("idx_to_rgb called with standard color index {idx}");
    } else if idx < 232 {
        // 6x6x6 color cube
        let idx = idx - 16;
        let r = idx / 36;
        let g = (idx % 36) / 6;
        let b = idx % 6;
        let to_val = |c: u8| if c == 0 { 0 } else { 55 + 40 * c };
        (to_val(r), to_val(g), to_val(b))
    } else {
        // Grayscale ramp (232-255)
        let val = 8 + 10 * (idx - 232);
        (val, val, val)
    }
}

/// Convert vt100::Color to Kakoune face color string
pub fn color_to_kak(color: &vt100::Color, palette: &[u8; 48]) -> Option<String> {
    match color {
        vt100::Color::Default => None,
        vt100::Color::Rgb(r, g, b) => Some(format!("rgb:{r:02X}{g:02X}{b:02X}")),
        vt100::Color::Idx(idx) => {
            let (r, g, b) = if *idx < 16 {
                let base = *idx as usize * 3;
                (palette[base], palette[base + 1], palette[base + 2])
            } else {
                idx_to_rgb(*idx)
            };
            Some(format!("rgb:{r:02X}{g:02X}{b:02X}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_color_returns_none() {
        assert_eq!(color_to_kak(&vt100::Color::Default, &DEFAULT_PALETTE), None);
    }

    #[test]
    fn rgb_passthrough() {
        let result = color_to_kak(&vt100::Color::Rgb(0xFF, 0x00, 0xAB), &DEFAULT_PALETTE);
        assert_eq!(result, Some("rgb:FF00AB".to_string()));
    }

    #[test]
    fn indexed_standard_colors() {
        // Color 0 = black
        let result = color_to_kak(&vt100::Color::Idx(0), &DEFAULT_PALETTE);
        assert_eq!(result, Some("rgb:000000".to_string()));

        // Color 9 = bright red
        let result = color_to_kak(&vt100::Color::Idx(9), &DEFAULT_PALETTE);
        assert_eq!(result, Some("rgb:FF0000".to_string()));
    }

    #[test]
    fn indexed_color_cube() {
        // Index 196 = 6x6x6 cube: r=5,g=0,b=0 => (255,0,0)
        let (r, g, b) = idx_to_rgb(196);
        assert_eq!((r, g, b), (255, 0, 0));

        // Index 16 = first cube color: r=0,g=0,b=0 => (0,0,0)
        let (r, g, b) = idx_to_rgb(16);
        assert_eq!((r, g, b), (0, 0, 0));

        // Index 231 = last cube color: r=5,g=5,b=5 => (255,255,255)
        let (r, g, b) = idx_to_rgb(231);
        assert_eq!((r, g, b), (255, 255, 255));

        // Index 21 = r=0,g=0,b=5 => (0,0,255)
        let (r, g, b) = idx_to_rgb(21);
        assert_eq!((r, g, b), (0, 0, 255));

        // Index 46 = r=0,g=5,b=0 => (0,255,0)
        let (r, g, b) = idx_to_rgb(46);
        assert_eq!((r, g, b), (0, 255, 0));
    }

    #[test]
    fn indexed_grayscale() {
        // Index 232 = first grayscale: 8 + 10*0 = 8
        let (r, g, b) = idx_to_rgb(232);
        assert_eq!((r, g, b), (8, 8, 8));

        // Index 255 = last grayscale: 8 + 10*23 = 238
        let (r, g, b) = idx_to_rgb(255);
        assert_eq!((r, g, b), (238, 238, 238));
    }

    #[test]
    fn indexed_color_to_kak() {
        let result = color_to_kak(&vt100::Color::Idx(196), &DEFAULT_PALETTE);
        assert_eq!(result, Some("rgb:FF0000".to_string()));
    }

    // --- Phase 3: LOW priority ---

    #[test]
    fn idx_to_rgb_panics_on_standard_color() {
        let result0 = std::panic::catch_unwind(|| idx_to_rgb(0));
        assert!(result0.is_err(), "idx_to_rgb(0) should panic");
        let result15 = std::panic::catch_unwind(|| idx_to_rgb(15));
        assert!(result15.is_err(), "idx_to_rgb(15) should panic");
    }

    #[test]
    fn standard_palette_representative_colors() {
        assert_eq!(
            color_to_kak(&vt100::Color::Idx(1), &DEFAULT_PALETTE),
            Some("rgb:CC0000".to_string())
        );
        assert_eq!(
            color_to_kak(&vt100::Color::Idx(4), &DEFAULT_PALETTE),
            Some("rgb:0000CC".to_string())
        );
        assert_eq!(
            color_to_kak(&vt100::Color::Idx(7), &DEFAULT_PALETTE),
            Some("rgb:CCCCCC".to_string())
        );
        assert_eq!(
            color_to_kak(&vt100::Color::Idx(15), &DEFAULT_PALETTE),
            Some("rgb:FFFFFF".to_string())
        );
    }

    #[test]
    fn color_to_kak_with_grayscale_index() {
        assert_eq!(
            color_to_kak(&vt100::Color::Idx(232), &DEFAULT_PALETTE),
            Some("rgb:080808".to_string())
        );
        assert_eq!(
            color_to_kak(&vt100::Color::Idx(255), &DEFAULT_PALETTE),
            Some("rgb:EEEEEE".to_string())
        );
    }

    #[test]
    fn idx_to_rgb_mid_cube_color() {
        let (r, g, b) = idx_to_rgb(67);
        assert_eq!((r, g, b), (95, 135, 175));
    }
}
