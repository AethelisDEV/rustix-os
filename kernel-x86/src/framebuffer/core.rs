//! # Core GOP Framebuffer Drawing Context
//!
//! Implements the main display context `UefiGraphics` mapping pixel layouts,
//! clearing screens, drawing rectangles, and providing safe direct writers for crash contexts.

use bootloader_api::info::{FrameBuffer, PixelFormat};
use crate::framebuffer::font::{Color, COLOR_BG};
use alloc::vec::Vec;

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
    /// Saved background color values for the mouse cursor area.
    pub mouse_backbuffer: [Color; 12 * 12],
    /// Whether a mouse cursor background is currently saved.
    pub mouse_saved: bool,
    /// The X coordinate where the mouse was drawn.
    pub mouse_saved_x: usize,
    /// The Y coordinate where the mouse was drawn.
    pub mouse_saved_y: usize,
    /// Off-screen compositor backbuffer memory canvas for flicker-free double buffering.
    pub backbuffer: Vec<Color>,
    /// Minimum dirty X coordinate for damage tracking.
    pub dirty_x1: usize,
    /// Minimum dirty Y coordinate for damage tracking.
    pub dirty_y1: usize,
    /// Maximum dirty X coordinate for damage tracking.
    pub dirty_x2: usize,
    /// Maximum dirty Y coordinate for damage tracking.
    pub dirty_y2: usize,
}

impl UefiGraphics {
    /// Resets the dirty bounding box tracking coordinates to an empty state.
    pub fn reset_dirty(&mut self) {
        self.dirty_x1 = usize::MAX;
        self.dirty_y1 = usize::MAX;
        self.dirty_x2 = 0;
        self.dirty_y2 = 0;
    }

    /// Creates a new graphics driver instance from the raw bootloader FrameBuffer.
    pub fn new(fb: &'static mut FrameBuffer) -> Self {
        let info = fb.info();
        let size = info.width * info.height;
        let mut backbuffer = Vec::with_capacity(size);
        backbuffer.resize(size, COLOR_BG);
        let mut graphics = Self {
            buffer: fb.buffer_mut(),
            width: info.width,
            height: info.height,
            stride: info.stride,
            bytes_per_pixel: info.bytes_per_pixel,
            format: info.pixel_format,
            mouse_backbuffer: [COLOR_BG; 12 * 12],
            mouse_saved: false,
            mouse_saved_x: 0,
            mouse_saved_y: 0,
            backbuffer,
            dirty_x1: usize::MAX,
            dirty_y1: usize::MAX,
            dirty_x2: 0,
            dirty_y2: 0,
        };
        graphics.reset_dirty();
        graphics
    }

    /// Returns the start virtual address of the physical framebuffer.
    pub fn framebuffer_addr(&self) -> u64 {
        self.buffer.as_ptr() as u64
    }

    /// Returns the size in bytes of the physical framebuffer.
    pub fn framebuffer_len(&self) -> usize {
        self.buffer.len()
    }

    /// Reads a single pixel Color from the specified (x, y) coordinates with layout conversion.
    #[inline]
    pub fn read_pixel(&self, x: usize, y: usize) -> Color {
        if x >= self.width || y >= self.height {
            return COLOR_BG;
        }
        self.backbuffer[y * self.width + x]
    }

    /// Saves the 12x12 background pixels at the specified (mx, my) coordinates
    /// into the internal mouse backbuffer before drawing the cursor.
    pub fn save_mouse_backbuffer(&mut self, mx: usize, my: usize) {
        self.mouse_saved_x = mx;
        self.mouse_saved_y = my;
        self.mouse_saved = true;

        for dy in 0..12 {
            for dx in 0..12 {
                let px = mx + dx;
                let py = my + dy;
                self.mouse_backbuffer[dy * 12 + dx] = self.read_pixel(px, py);
            }
        }
    }

    /// Restores the saved 12x12 background pixels to the screen from the internal backbuffer,
    /// completely removing any visual traces of the previously drawn mouse cursor.
    pub fn restore_mouse_backbuffer(&mut self) {
        if !self.mouse_saved {
            return;
        }

        let mx = self.mouse_saved_x;
        let my = self.mouse_saved_y;

        for dy in 0..12 {
            for dx in 0..12 {
                let px = mx + dx;
                let py = my + dy;
                let color = self.mouse_backbuffer[dy * 12 + dx];
                self.write_pixel(px, py, color);
            }
        }

        self.mouse_saved = false;
    }

    /// Invalidates the mouse saved backbuffer flag.
    /// This should be called whenever the screen layout is completely redrawn (e.g. F1/F2 screen swaps),
    /// as the old background cache is no longer valid.
    pub fn invalidate_mouse(&mut self) {
        self.mouse_saved = false;
    }

    /// Draws a highly stylized neon premium mouse cursor at (mx, my).
    /// Uses a custom 12x12 triangle shape with neon cyan borders and hot pink body.
    pub fn draw_cursor(&mut self, mx: usize, my: usize) {
        // Neon Cyan Border
        let cyan = Color::new(0, 245, 255);
        // Neon Hot Pink Body
        let pink = Color::new(255, 0, 160);

        #[rustfmt::skip]
        const MOUSE_MASK: [[u8; 12]; 12] = [
            [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            [1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            [1, 2, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            [1, 2, 2, 1, 0, 0, 0, 0, 0, 0, 0, 0],
            [1, 2, 2, 2, 1, 0, 0, 0, 0, 0, 0, 0],
            [1, 2, 2, 2, 2, 1, 0, 0, 0, 0, 0, 0],
            [1, 2, 2, 2, 2, 2, 1, 0, 0, 0, 0, 0],
            [1, 2, 2, 2, 2, 2, 2, 1, 0, 0, 0, 0],
            [1, 2, 2, 2, 1, 1, 1, 1, 1, 0, 0, 0],
            [1, 2, 2, 1, 0, 0, 0, 0, 0, 0, 0, 0],
            [1, 2, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            [1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        ];

        for dy in 0..12 {
            for dx in 0..12 {
                let pixel_type = MOUSE_MASK[dy][dx];
                if pixel_type != 0 {
                    let px = mx + dx;
                    let py = my + dy;
                    let color = if pixel_type == 1 { cyan } else { pink };
                    self.write_pixel(px, py, color);
                }
            }
        }
    }

    /// Clears the entire screen viewport to a solid Color.
    pub fn clear(&mut self, color: Color) {
        self.invalidate_mouse();
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
        self.backbuffer[y * self.width + x] = color;

        // Expand the dirty bounding box to track screen changes (Damage Tracking)
        if x < self.dirty_x1 { self.dirty_x1 = x; }
        if x > self.dirty_x2 { self.dirty_x2 = x; }
        if y < self.dirty_y1 { self.dirty_y1 = y; }
        if y > self.dirty_y2 { self.dirty_y2 = y; }
    }

    /// Copies the dirty off-screen backbuffer area onto the physical GOP framebuffer (damage blitting).
    pub fn swap_buffers(&mut self) {
        // If nothing is dirty, skip physical MMIO copy completely!
        if self.dirty_x1 > self.dirty_x2 || self.dirty_y1 > self.dirty_y2 {
            return;
        }

        // Clamp dirty boundaries to screen size safely
        let x1 = self.dirty_x1.min(self.width - 1);
        let y1 = self.dirty_y1.min(self.height - 1);
        let x2 = self.dirty_x2.min(self.width - 1);
        let y2 = self.dirty_y2.min(self.height - 1);

        for y in y1..=y2 {
            let stride_offset = y * self.stride;
            let width_offset = y * self.width;
            
            for x in x1..=x2 {
                let color = self.backbuffer[width_offset + x];
                let pixel_offset = (stride_offset + x) * self.bytes_per_pixel;
                if pixel_offset + 3 > self.buffer.len() {
                    continue;
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
        }

        // Reset tracking coordinates for the next frame
        self.reset_dirty();
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
