//! # Core GOP Framebuffer Drawing Context
//!
//! Implements the main display context `UefiGraphics` mapping pixel layouts,
//! clearing screens, drawing rectangles, and providing safe direct writers for crash contexts.

use bootloader_api::info::{FrameBuffer, PixelFormat};
use crate::framebuffer::font::{Color, COLOR_BG};

/// High-level UEFI Graphics driver interface mapping raw physical GOP framebuffers.
pub struct UefiGraphics {
    /// Static slice to raw framebuffer memory mapped in Ring 0.
    buffer: &'static mut [u8],
    /// Screen width in pixels.
    pub width: usize,
    /// Screen height in pixels.
    pub height: usize,
    /// Line stride (number of pixels per horizontal scanline).
    pub stride: usize,
    /// Number of bytes per pixel (typically 3 or 4).
    pub bytes_per_pixel: usize,
    /// Format of the pixels (e.g. Bgr or Rgb).
    pub format: PixelFormat,
}

impl UefiGraphics {
    /// Creates a new graphics driver instance from the raw bootloader FrameBuffer.
    pub fn new(fb: &'static mut FrameBuffer) -> Self {
        let info = fb.info();
        Self {
            buffer: fb.buffer_mut(),
            width: info.width,
            height: info.height,
            stride: info.stride,
            bytes_per_pixel: info.bytes_per_pixel,
            format: info.pixel_format,
        }
    }

    /// Clears the entire screen viewport to a solid Color.
    pub fn clear(&mut self, color: Color) {
        for y in 0..self.height {
            for x in 0..self.width {
                self.write_pixel(x, y, color);
            }
        }
    }

    /// Writes a single pixel value to specified (x, y) coordinates with layout conversion.
    ///
    /// Supported Pixel Formats:
    /// - `PixelFormat::Bgr`: Writes Blue, Green, Red, [Alpha] sequence.
    /// - `PixelFormat::Rgb`: Writes Red, Green, Blue, [Alpha] sequence.
    /// - `PixelFormat::U8`: Direct single-byte grayscale scale.
    #[inline]
    pub fn write_pixel(&mut self, x: usize, y: usize, color: Color) {
        if x >= self.width || y >= self.height {
            return;
        }

        let pixel_offset = (y * self.stride + x) * self.bytes_per_pixel;
        if pixel_offset + 3 > self.buffer.len() {
            return;
        }

        match self.format {
            PixelFormat::Bgr => {
                self.buffer[pixel_offset] = color.b;
                self.buffer[pixel_offset + 1] = color.g;
                self.buffer[pixel_offset + 2] = color.r;
            }
            PixelFormat::Rgb => {
                self.buffer[pixel_offset] = color.r;
                self.buffer[pixel_offset + 1] = color.g;
                self.buffer[pixel_offset + 2] = color.b;
            }
            PixelFormat::U8 => {
                self.buffer[pixel_offset] = color.r;
            }
            _ => {}
        }
    }

    /// Draws a solid filled rectangle with specified dimensions and Color.
    pub fn draw_rect(&mut self, x: usize, y: usize, width: usize, height: usize, color: Color) {
        for row in 0..height {
            for col in 0..width {
                self.write_pixel(x + col, y + row, color);
            }
        }
    }

    /// Draws an empty border-only rectangle with solid borders and cleared background.
    pub fn draw_border_rect(&mut self, x: usize, y: usize, width: usize, height: usize, border_color: Color, bg_color: Color) {
        // Clear background area inside card boundaries
        self.draw_rect(x + 1, y + 1, width - 2, height - 2, bg_color);

        // Draw horizontal borders
        for col in 0..width {
            self.write_pixel(x + col, y, border_color);
            self.write_pixel(x + col, y + height - 1, border_color);
        }

        // Draw vertical borders
        for row in 0..height {
            self.write_pixel(x, y + row, border_color);
            self.write_pixel(x + width - 1, y + row, border_color);
        }
    }

    /// Draws a horizontal linear color gradient box shifting from start to end Color.
    pub fn draw_horizontal_gradient_rect(&mut self, x: usize, y: usize, width: usize, height: usize, start: Color, end: Color) {
        for col in 0..width {
            let ratio = col as f32 / width as f32;
            let r = (start.r as f32 + (end.r as f32 - start.r as f32) * ratio) as u8;
            let g = (start.g as f32 + (end.g as f32 - start.g as f32) * ratio) as u8;
            let b = (start.b as f32 + (end.b as f32 - start.b as f32) * ratio) as u8;
            let gradient_color = Color::new(r, g, b);

            for row in 0..height {
                self.write_pixel(x + col, y + row, gradient_color);
            }
        }
    }
}

/// A direct formatting writer that prints text directly onto the UEFI GOP framebuffer.
/// Used in panic/crash handlers where dynamic memory allocation is unsafe or offline.
pub struct GraphicsWriter<'a> {
    /// Mutable reference to the parent UefiGraphics context.
    pub graphics: &'a mut UefiGraphics,
    /// Current cursor column pixel coordinate.
    pub x: usize,
    /// Current cursor row pixel coordinate.
    pub y: usize,
    /// Base starting column pixel coordinate for auto-wrapping.
    pub start_x: usize,
    /// Font color channel mapping.
    pub color: Color,
}

impl<'a> core::fmt::Write for GraphicsWriter<'a> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for c in s.chars() {
            if c == '\n' {
                self.x = self.start_x;
                self.y += 12; // 8px font + 4px vertical line-spacing
            } else {
                if self.x + 8 >= self.graphics.width {
                    self.x = self.start_x;
                    self.y += 12;
                }
                self.graphics.draw_char(self.x, self.y, c, self.color, None, 1);
                self.x += 9; // 8px font + 1px horizontal character-spacing
            }
        }
        Ok(())
    }
}
