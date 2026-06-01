//! # Font Character and String Rendering Engine
//!
//! Exposes text rendering routines on `UefiGraphics` supporting:
//! 1. Standard 8x8 bitmap monospace font rendering.
//! 2. Safe 8x16 bitmap monospace font rendering (using embedded CP437 symbols).

use crate::framebuffer::font::{Color, FONT_8X8, FONT_8X16_DATA};
use crate::framebuffer::core::UefiGraphics;

impl UefiGraphics {
    /// Renders a single 8x8 character at specified coordinates with custom scale and colors.
    ///
    /// Background rendering is optional:
    /// - If `bg` is `Some(Color)`, empty character bits are filled with the background color.
    /// - If `bg` is `None`, empty bits are bypassed, enabling transparent text overlays.
    pub fn draw_char(&mut self, x: usize, y: usize, c: char, color: Color, bg: Option<Color>, scale: usize) {
        let ascii = (c as usize) & 0x7F;
        let row_data = FONT_8X8[ascii];

        for row in 0..8 {
            let byte = row_data[row];
            for col in 0..8 {
                // Font bitmap bits are mapped MSB to LSB
                let bit = (byte >> (7 - col)) & 1;
                if bit == 1 {
                    if scale == 1 {
                        self.write_pixel(x + col, y + row, color);
                    } else {
                        self.draw_rect(x + col * scale, y + row * scale, scale, scale, color);
                    }
                } else if let Some(bg_color) = bg {
                    if scale == 1 {
                        self.write_pixel(x + col, y + row, bg_color);
                    } else {
                        self.draw_rect(x + col * scale, y + row * scale, scale, scale, bg_color);
                    }
                }
            }
        }
    }

    /// Draws an ASCII string to the screen with character wrapping and screen bounds checking.
    pub fn draw_string(&mut self, mut x: usize, y: usize, text: &str, color: Color, bg: Option<Color>, scale: usize) {
        let char_w = 8 * scale;
        let spacing = 1 * scale;
        
        for c in text.chars() {
            if x + char_w >= self.width {
                break; // Boundary check: do not draw offscreen
            }
            self.draw_char(x, y, c, color, bg, scale);
            x += char_w + spacing;
        }
    }

    /// Renders a single 8x16 character at specified coordinates with custom scale and colors.
    ///
    /// Ideal for high-fidelity terminal consoles where clear legibility is safety-critical.
    pub fn draw_char_8x16(&mut self, x: usize, y: usize, c: char, color: Color, bg: Option<Color>, scale: usize) {
        let ascii = c as usize;
        if ascii >= 256 {
            return;
        }
        let char_offset = ascii * 16;

        for row in 0..16 {
            let byte = FONT_8X16_DATA.0[char_offset + row];
            for col in 0..8 {
                // Font bitmap bits are mapped MSB to LSB
                let bit = (byte >> (7 - col)) & 1;
                if bit == 1 {
                    if scale == 1 {
                        self.write_pixel(x + col, y + row, color);
                    } else {
                        self.draw_rect(x + col * scale, y + row * scale, scale, scale, color);
                    }
                } else if let Some(bg_color) = bg {
                    if scale == 1 {
                        self.write_pixel(x + col, y + row, bg_color);
                    } else {
                        self.draw_rect(x + col * scale, y + row * scale, scale, scale, bg_color);
                    }
                }
            }
        }
    }

    /// Draws an ASCII string using the 8x16 font with character wrapping and bounds checking.
    pub fn draw_string_8x16(&mut self, mut x: usize, y: usize, text: &str, color: Color, bg: Option<Color>, scale: usize) {
        let char_w = 8 * scale;
        let spacing = 1 * scale;
        
        for c in text.chars() {
            if x + char_w >= self.width {
                break; // Screen boundary check
            }
            self.draw_char_8x16(x, y, c, color, bg, scale);
            x += char_w + spacing;
        }
    }
}
