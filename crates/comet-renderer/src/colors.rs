//! Color handling for the renderer.
//!
//! Supports ANSI 16 colors, 256 colors, and true color (24-bit).

/// RGBA color with 8 bits per channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub fn from_rgb(val: u32) -> Self {
        Self {
            r: ((val >> 16) & 0xFF) as u8,
            g: ((val >> 8) & 0xFF) as u8,
            b: (val & 0xFF) as u8,
            a: 255,
        }
    }

    pub fn from_rgba(val: u32) -> Self {
        Self {
            r: ((val >> 24) & 0xFF) as u8,
            g: ((val >> 16) & 0xFF) as u8,
            b: ((val >> 8) & 0xFF) as u8,
            a: (val & 0xFF) as u8,
        }
    }

    pub fn to_f32_array(&self) -> [f32; 4] {
        [
            self.r as f32 / 255.0,
            self.g as f32 / 255.0,
            self.b as f32 / 255.0,
            self.a as f32 / 255.0,
        ]
    }

    pub fn lerp(&self, other: &Rgba, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self {
            r: (self.r as f32 + (other.r as f32 - self.r as f32) * t) as u8,
            g: (self.g as f32 + (other.g as f32 - self.g as f32) * t) as u8,
            b: (self.b as f32 + (other.b as f32 - self.b as f32) * t) as u8,
            a: (self.a as f32 + (other.a as f32 - self.a as f32) * t) as u8,
        }
    }
}

/// Standard ANSI 16 colors.
pub const ANSI_COLORS: [Rgba; 16] = [
    Rgba::rgb(0x00, 0x00, 0x00), // 0: Black
    Rgba::rgb(0xCD, 0x00, 0x00), // 1: Red
    Rgba::rgb(0x00, 0xCD, 0x00), // 2: Green
    Rgba::rgb(0xCD, 0xCD, 0x00), // 3: Yellow
    Rgba::rgb(0x00, 0x00, 0xEE), // 4: Blue
    Rgba::rgb(0xCD, 0x00, 0xCD), // 5: Magenta
    Rgba::rgb(0x00, 0xCD, 0xCD), // 6: Cyan
    Rgba::rgb(0xE5, 0xE5, 0xE5), // 7: White
    Rgba::rgb(0x7F, 0x7F, 0x7F), // 8: Bright Black
    Rgba::rgb(0xFF, 0x00, 0x00), // 9: Bright Red
    Rgba::rgb(0x00, 0xFF, 0x00), // 10: Bright Green
    Rgba::rgb(0xFF, 0xFF, 0x00), // 11: Bright Yellow
    Rgba::rgb(0x5C, 0x5C, 0xFF), // 12: Bright Blue
    Rgba::rgb(0xFF, 0x00, 0xFF), // 13: Bright Magenta
    Rgba::rgb(0x00, 0xFF, 0xFF), // 14: Bright Cyan
    Rgba::rgb(0xFF, 0xFF, 0xFF), // 15: Bright White
];

/// Gets 256-color palette entry.
pub fn ansi_256_color(index: u8) -> Rgba {
    match index {
        0..=15 => ANSI_COLORS[index as usize],
        16..=231 => {
            // 6x6x6 color cube
            let idx = index - 16;
            let r = (idx / 36) * 51;
            let g = ((idx % 36) / 6) * 51;
            let b = (idx % 6) * 51;
            Rgba::rgb(r, g, b)
        }
        232..=255 => {
            // Grayscale ramp
            let val = (index - 232) * 10 + 8;
            Rgba::rgb(val, val, val)
        }
    }
}

/// Color palette for the renderer.
#[derive(Debug, Clone)]
pub struct ColorPalette {
    pub ansi: [Rgba; 16],
    pub default_fg: Rgba,
    pub default_bg: Rgba,
    pub cursor: Rgba,
    pub selection_bg: Rgba,
    pub selection_fg: Rgba,
}

impl Default for ColorPalette {
    fn default() -> Self {
        Self {
            ansi: ANSI_COLORS,
            default_fg: Rgba::rgb(0xE5, 0xE5, 0xE5),
            default_bg: Rgba::rgb(0x1E, 0x1E, 0x1E),
            cursor: Rgba::rgb(0xFF, 0xFF, 0xFF),
            selection_bg: Rgba::rgb(0x4A, 0x4A, 0x4A),
            selection_fg: Rgba::rgb(0xFF, 0xFF, 0xFF),
        }
    }
}

impl ColorPalette {
    pub fn get(&self, index: u8) -> Rgba {
        if index < 16 {
            self.ansi[index as usize]
        } else {
            ansi_256_color(index)
        }
    }

    pub fn set_ansi(&mut self, index: u8, color: Rgba) {
        if index < 16 {
            self.ansi[index as usize] = color;
        }
    }

    pub fn from_theme(name: &str) -> Self {
        match name.to_lowercase().as_str() {
            "light" => Self {
                ansi: ANSI_COLORS,
                default_fg: Rgba::rgb(0x1E, 0x1E, 0x1E),
                default_bg: Rgba::rgb(0xFF, 0xFF, 0xFF),
                cursor: Rgba::rgb(0x00, 0x00, 0x00),
                selection_bg: Rgba::rgb(0xBB, 0xBB, 0xBB),
                selection_fg: Rgba::rgb(0x00, 0x00, 0x00),
            },
            "solarized-dark" => Self {
                ansi: [
                    Rgba::rgb(0x00, 0x2B, 0x36),
                    Rgba::rgb(0xDC, 0x32, 0x2F),
                    Rgba::rgb(0x85, 0x99, 0x00),
                    Rgba::rgb(0xB5, 0x89, 0x00),
                    Rgba::rgb(0x26, 0x8B, 0xD2),
                    Rgba::rgb(0xD3, 0x36, 0x82),
                    Rgba::rgb(0x2A, 0xA1, 0x98),
                    Rgba::rgb(0xEE, 0xE8, 0xD5),
                    Rgba::rgb(0x07, 0x36, 0x42),
                    Rgba::rgb(0xCB, 0x4B, 0x16),
                    Rgba::rgb(0x58, 0x6E, 0x75),
                    Rgba::rgb(0x65, 0x7B, 0x83),
                    Rgba::rgb(0x83, 0x94, 0x96),
                    Rgba::rgb(0x6C, 0x71, 0xC4),
                    Rgba::rgb(0x93, 0xA1, 0xA1),
                    Rgba::rgb(0xFD, 0xF6, 0xE3),
                ],
                default_fg: Rgba::rgb(0x83, 0x94, 0x96),
                default_bg: Rgba::rgb(0x00, 0x2B, 0x36),
                cursor: Rgba::rgb(0x83, 0x94, 0x96),
                selection_bg: Rgba::rgb(0x07, 0x36, 0x42),
                selection_fg: Rgba::rgb(0x83, 0x94, 0x96),
            },
            _ => Self::default(),
        }
    }

    /// Create a ColorPalette from individual hex color strings.
    /// Falls back to defaults if parsing fails.
    pub fn from_hex(background: &str, foreground: &str, cursor: &str, selection: &str) -> Self {
        let default_bg = parse_hex_color(background).unwrap_or(Rgba::rgb(0x1E, 0x1E, 0x1E));
        let default_fg = parse_hex_color(foreground).unwrap_or(Rgba::rgb(0xE5, 0xE5, 0xE5));
        let cursor_color = parse_hex_color(cursor).unwrap_or(Rgba::rgb(0xFF, 0xFF, 0xFF));
        let selection_bg = parse_hex_color(selection).unwrap_or(Rgba::rgb(0x4A, 0x4A, 0x4A));

        ColorPalette {
            ansi: ANSI_COLORS,
            default_fg,
            default_bg,
            cursor: cursor_color,
            selection_bg,
            selection_fg: default_fg,
        }
    }
}

/// Parse a hex color string (e.g., "#rrggbb") into Rgba.
pub fn parse_hex_color(hex: &str) -> Option<Rgba> {
    if !hex.starts_with('#') || hex.len() != 7 {
        return None;
    }
    let r = u8::from_str_radix(&hex[1..3], 16).ok()?;
    let g = u8::from_str_radix(&hex[3..5], 16).ok()?;
    let b = u8::from_str_radix(&hex[5..7], 16).ok()?;
    Some(Rgba::rgb(r, g, b))
}

/// Create a ColorPalette from individual hex color strings.
pub fn palette_from_hex(
    background: &str,
    foreground: &str,
    cursor: &str,
    selection: &str,
) -> ColorPalette {
    let default_bg = parse_hex_color(background).unwrap_or(Rgba::rgb(0x1E, 0x1E, 0x1E));
    let default_fg = parse_hex_color(foreground).unwrap_or(Rgba::rgb(0xE5, 0xE5, 0xE5));
    let cursor_color = parse_hex_color(cursor).unwrap_or(Rgba::rgb(0xFF, 0xFF, 0xFF));
    let selection_bg = parse_hex_color(selection).unwrap_or(Rgba::rgb(0x4A, 0x4A, 0x4A));

    ColorPalette {
        ansi: ANSI_COLORS,
        default_fg,
        default_bg,
        cursor: cursor_color,
        selection_bg,
        selection_fg: default_fg,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rgba_creation() {
        let c = Rgba::rgb(255, 128, 64);
        assert_eq!(c.r, 255);
        assert_eq!(c.g, 128);
        assert_eq!(c.b, 64);
        assert_eq!(c.a, 255);
    }

    #[test]
    fn test_rgba_from_rgb() {
        let c = Rgba::from_rgb(0xFF8040);
        assert_eq!(c.r, 0xFF);
        assert_eq!(c.g, 0x80);
        assert_eq!(c.b, 0x40);
    }

    #[test]
    fn test_ansi_256() {
        assert_eq!(ansi_256_color(0), ANSI_COLORS[0]);
        assert_eq!(ansi_256_color(15), ANSI_COLORS[15]);

        // Color cube
        let c = ansi_256_color(16);
        assert_eq!(c.r, 0);
        assert_eq!(c.g, 0);
        assert_eq!(c.b, 0);

        let c = ansi_256_color(231);
        assert_eq!(c.r, 255);
        assert_eq!(c.g, 255);
        assert_eq!(c.b, 255);

        // Grayscale
        let c = ansi_256_color(232);
        assert_eq!(c.r, 8);
        let c = ansi_256_color(255);
        assert_eq!(c.r, 238);
    }

    #[test]
    fn test_palette_theme() {
        let dark = ColorPalette::from_theme("dark");
        assert_eq!(dark.default_bg, Rgba::rgb(0x1E, 0x1E, 0x1E));

        let light = ColorPalette::from_theme("light");
        assert_eq!(light.default_bg, Rgba::rgb(0xFF, 0xFF, 0xFF));
    }
}
