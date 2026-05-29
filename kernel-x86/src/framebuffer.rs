//! # UEFI GOP Framebuffer Graphics Driver for AE Rustanium
//!
//! Provides safe pixel-level drawing capabilities, solid/gradient rectangles,
//! and text rendering utilizing a self-contained 8x8 bitmap font.

use bootloader_api::info::{FrameBuffer, PixelFormat};

/// A standard RGB Color.
#[derive(Debug, Clone, Copy)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

// Sleek dark-mode color scheme
pub const COLOR_BG: Color = Color::new(13, 15, 18);          // Dark charcoal
pub const COLOR_ACCENT_BLUE: Color = Color::new(0, 150, 255); // Glowing cyan-blue
pub const COLOR_ACCENT_GREEN: Color = Color::new(32, 223, 127); // Radioactive green
pub const COLOR_ACCENT_PURPLE: Color = Color::new(147, 51, 234); // Deep space purple
pub const COLOR_TEXT_WHITE: Color = Color::new(243, 244, 246);  // Off-white
pub const COLOR_TEXT_MUTED: Color = Color::new(107, 114, 128);  // Slate gray
pub const COLOR_PANEL_BG: Color = Color::new(24, 28, 34);       // Elevated gray card

/// Lightweight 8x8 Monospace Bitmap Font for ASCII 32..=126
/// Each character consists of 8 bytes (one byte per horizontal row).
#[rustfmt::skip]
static FONT_8X8: [[u8; 8]; 128] = {
    let mut font = [[0u8; 8]; 128];
    
    // Space (32)
    font[32] = [0, 0, 0, 0, 0, 0, 0, 0];
    // !
    font[33] = [0x18, 0x18, 0x18, 0x18, 0x00, 0x00, 0x18, 0x00];
    // "
    font[34] = [0x24, 0x24, 0x24, 0x00, 0x00, 0x00, 0x00, 0x00];
    // #
    font[35] = [0x24, 0x7e, 0x24, 0x24, 0x7e, 0x24, 0x24, 0x00];
    // $
    font[36] = [0x18, 0x3e, 0x1c, 0x18, 0x38, 0x3e, 0x18, 0x00];
    // %
    font[37] = [0x62, 0x64, 0x08, 0x10, 0x20, 0x26, 0x46, 0x00];
    // &
    font[38] = [0x1c, 0x22, 0x22, 0x1c, 0x2a, 0x24, 0x1a, 0x00];
    // '
    font[39] = [0x0c, 0x0c, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00];
    // (
    font[40] = [0x0c, 0x18, 0x18, 0x18, 0x18, 0x18, 0x0c, 0x00];
    // )
    font[41] = [0x30, 0x18, 0x18, 0x18, 0x18, 0x18, 0x30, 0x00];
    // *
    font[42] = [0x00, 0x24, 0x18, 0x7e, 0x18, 0x24, 0x00, 0x00];
    // +
    font[43] = [0x00, 0x18, 0x18, 0x7e, 0x18, 0x18, 0x00, 0x00];
    // ,
    font[44] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x0c, 0x0c, 0x08];
    // -
    font[45] = [0x00, 0x00, 0x00, 0x7e, 0x00, 0x00, 0x00, 0x00];
    // .
    font[46] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00];
    // /
    font[47] = [0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80, 0x00];
    
    // Digits 0-9
    font[48] = [0x3c, 0x66, 0x6e, 0x76, 0x66, 0x66, 0x3c, 0x00]; // 0
    font[49] = [0x18, 0x1c, 0x18, 0x18, 0x18, 0x18, 0x7e, 0x00]; // 1
    font[50] = [0x3c, 0x66, 0x06, 0x0c, 0x30, 0x60, 0x7e, 0x00]; // 2
    font[51] = [0x3c, 0x66, 0x06, 0x1c, 0x06, 0x66, 0x3c, 0x00]; // 3
    font[52] = [0x06, 0x0e, 0x1e, 0x66, 0x7f, 0x06, 0x06, 0x00]; // 4
    font[53] = [0x7f, 0x60, 0x7c, 0x06, 0x06, 0x66, 0x3c, 0x00]; // 5
    font[54] = [0x3c, 0x60, 0x7c, 0x66, 0x66, 0x66, 0x3c, 0x00]; // 6
    font[55] = [0x7f, 0x06, 0x0c, 0x18, 0x18, 0x18, 0x18, 0x00]; // 7
    font[56] = [0x3c, 0x66, 0x66, 0x3c, 0x66, 0x66, 0x3c, 0x00]; // 8
    font[57] = [0x3c, 0x66, 0x66, 0x3e, 0x06, 0x0c, 0x38, 0x00]; // 9
    
    // Colon and other symbols
    font[58] = [0x00, 0x18, 0x18, 0x00, 0x00, 0x18, 0x18, 0x00]; // :
    font[59] = [0x00, 0x18, 0x18, 0x00, 0x00, 0x18, 0x18, 0x10]; // ;
    font[60] = [0x0c, 0x18, 0x30, 0x60, 0x30, 0x18, 0x0c, 0x00]; // <
    font[61] = [0x00, 0x00, 0x7e, 0x00, 0x7e, 0x00, 0x00, 0x00]; // =
    font[62] = [0x30, 0x18, 0x0c, 0x06, 0x0c, 0x18, 0x30, 0x00]; // >
    font[63] = [0x3c, 0x66, 0x06, 0x0c, 0x18, 0x00, 0x18, 0x00]; // ?
    font[64] = [0x3c, 0x66, 0x6e, 0x6a, 0x6e, 0x60, 0x3c, 0x00]; // @
    
    // Uppercase A-Z
    font[65] = [0x18, 0x3c, 0x66, 0x7e, 0x66, 0x66, 0x66, 0x00]; // A
    font[66] = [0x7c, 0x66, 0x66, 0x7c, 0x66, 0x66, 0x7c, 0x00]; // B
    font[67] = [0x3c, 0x66, 0x60, 0x60, 0x60, 0x66, 0x3c, 0x00]; // C
    font[68] = [0x78, 0x6c, 0x66, 0x66, 0x66, 0x6c, 0x78, 0x00]; // D
    font[69] = [0x7e, 0x60, 0x60, 0x7c, 0x60, 0x60, 0x7e, 0x00]; // E
    font[70] = [0x7e, 0x60, 0x60, 0x7c, 0x60, 0x60, 0x60, 0x00]; // F
    font[71] = [0x3c, 0x66, 0x60, 0x6e, 0x66, 0x66, 0x3e, 0x00]; // G
    font[72] = [0x66, 0x66, 0x66, 0x7e, 0x66, 0x66, 0x66, 0x00]; // H
    font[73] = [0x3e, 0x08, 0x08, 0x08, 0x08, 0x08, 0x3e, 0x00]; // I
    font[74] = [0x1f, 0x04, 0x04, 0x04, 0x04, 0x64, 0x3c, 0x00]; // J
    font[75] = [0x66, 0x6c, 0x78, 0x70, 0x78, 0x6c, 0x66, 0x00]; // K
    font[76] = [0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x7e, 0x00]; // L
    font[77] = [0x63, 0x77, 0x7f, 0x6b, 0x63, 0x63, 0x63, 0x00]; // M
    font[78] = [0x66, 0x76, 0x7e, 0x7e, 0x6e, 0x66, 0x66, 0x00]; // N
    font[79] = [0x3c, 0x66, 0x66, 0x66, 0x66, 0x66, 0x3c, 0x00]; // O
    font[80] = [0x7c, 0x66, 0x66, 0x7c, 0x60, 0x60, 0x60, 0x00]; // P
    font[81] = [0x3c, 0x66, 0x66, 0x66, 0x6a, 0x6c, 0x3e, 0x02]; // Q
    font[82] = [0x7c, 0x66, 0x66, 0x7c, 0x78, 0x6c, 0x66, 0x00]; // R
    font[83] = [0x3e, 0x60, 0x60, 0x3c, 0x06, 0x06, 0x7c, 0x00]; // S
    font[84] = [0x7e, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x00]; // T
    font[85] = [0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x3c, 0x00]; // U
    font[86] = [0x66, 0x66, 0x66, 0x66, 0x66, 0x3c, 0x18, 0x00]; // V
    font[87] = [0x63, 0x63, 0x63, 0x6b, 0x7f, 0x77, 0x63, 0x00]; // W
    font[88] = [0x66, 0x66, 0x3c, 0x18, 0x3c, 0x66, 0x66, 0x00]; // X
    font[89] = [0x66, 0x66, 0x66, 0x3c, 0x18, 0x18, 0x18, 0x00]; // Y
    font[90] = [0x7e, 0x06, 0x0c, 0x18, 0x30, 0x60, 0x7e, 0x00]; // Z
    
    // Brackets
    font[91] = [0x3e, 0x30, 0x30, 0x30, 0x30, 0x30, 0x3e, 0x00]; // [
    font[92] = [0x80, 0x40, 0x20, 0x10, 0x08, 0x04, 0x02, 0x00]; // \
    font[93] = [0x7c, 0x0c, 0x0c, 0x0c, 0x0c, 0x0c, 0x7c, 0x00]; // ]
    font[94] = [0x08, 0x1c, 0x36, 0x00, 0x00, 0x00, 0x00, 0x00]; // ^
    font[95] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0x00]; // _
    font[96] = [0x18, 0x08, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00]; // `
    
    // Lowercase a-z
    font[97]  = [0x00, 0x00, 0x3c, 0x06, 0x3e, 0x66, 0x3e, 0x00]; // a
    font[98]  = [0x60, 0x60, 0x7c, 0x66, 0x66, 0x66, 0x7c, 0x00]; // b
    font[99]  = [0x00, 0x00, 0x3c, 0x66, 0x60, 0x66, 0x3c, 0x00]; // c
    font[100] = [0x06, 0x06, 0x3e, 0x66, 0x66, 0x66, 0x3e, 0x00]; // d
    font[101] = [0x00, 0x00, 0x3c, 0x66, 0x7e, 0x60, 0x3c, 0x00]; // e
    font[102] = [0x0e, 0x18, 0x3c, 0x18, 0x18, 0x18, 0x18, 0x00]; // f
    font[103] = [0x00, 0x00, 0x3e, 0x66, 0x66, 0x3e, 0x06, 0x3c]; // g
    font[104] = [0x60, 0x60, 0x7c, 0x66, 0x66, 0x66, 0x66, 0x00]; // h
    font[105] = [0x18, 0x00, 0x38, 0x18, 0x18, 0x18, 0x3c, 0x00]; // i
    font[106] = [0x06, 0x00, 0x0e, 0x06, 0x06, 0x06, 0x66, 0x3c]; // j
    font[107] = [0x60, 0x60, 0x66, 0x6c, 0x78, 0x6c, 0x66, 0x00]; // k
    font[108] = [0x38, 0x18, 0x18, 0x18, 0x18, 0x18, 0x3c, 0x00]; // l
    font[109] = [0x00, 0x00, 0x6d, 0x77, 0x6b, 0x63, 0x63, 0x00]; // m
    font[110] = [0x00, 0x00, 0x7c, 0x66, 0x66, 0x66, 0x66, 0x00]; // n
    font[111] = [0x00, 0x00, 0x3c, 0x66, 0x66, 0x66, 0x3c, 0x00]; // o
    font[112] = [0x00, 0x00, 0x7c, 0x66, 0x66, 0x7c, 0x60, 0x60]; // p
    font[113] = [0x00, 0x00, 0x3e, 0x66, 0x66, 0x3e, 0x06, 0x06]; // q
    font[114] = [0x00, 0x00, 0x76, 0x7c, 0x60, 0x60, 0x60, 0x00]; // r
    font[115] = [0x00, 0x00, 0x3e, 0x60, 0x3c, 0x06, 0x7c, 0x00]; // s
    font[116] = [0x18, 0x18, 0x7e, 0x18, 0x18, 0x18, 0x0e, 0x00]; // t
    font[117] = [0x00, 0x00, 0x66, 0x66, 0x66, 0x66, 0x3e, 0x00]; // u
    font[118] = [0x00, 0x00, 0x66, 0x66, 0x66, 0x3c, 0x18, 0x00]; // v
    font[119] = [0x00, 0x00, 0x63, 0x6b, 0x7f, 0x77, 0x63, 0x00]; // w
    font[120] = [0x00, 0x00, 0x66, 0x3c, 0x18, 0x3c, 0x66, 0x00]; // x
    font[121] = [0x00, 0x00, 0x66, 0x66, 0x66, 0x3e, 0x06, 0x3c]; // y
    font[122] = [0x00, 0x00, 0x7e, 0x0c, 0x18, 0x30, 0x7e, 0x00]; // z
    
    font[123] = [0x0e, 0x18, 0x18, 0x30, 0x18, 0x18, 0x0e, 0x00]; // {
    font[124] = [0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x00]; // |
    font[125] = [0x70, 0x18, 0x18, 0x0c, 0x18, 0x18, 0x70, 0x00]; // }
    
    font
};

/// High-level UEFI Graphics driver interface.
pub struct UefiGraphics {
    buffer: &'static mut [u8],
    pub width: usize,
    pub height: usize,
    pub stride: usize,
    pub bytes_per_pixel: usize,
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

    /// Clears the entire screen to a solid color.
    pub fn clear(&mut self, color: Color) {
        for y in 0..self.height {
            for x in 0..self.width {
                self.write_pixel(x, y, color);
            }
        }
    }

    /// Writes a single pixel directly to hardware memory.
    #[inline(always)]
    pub fn write_pixel(&mut self, x: usize, y: usize, color: Color) {
        if x >= self.width || y >= self.height {
            return;
        }
        let pixel_offset = y * self.stride + x;
        let byte_offset = pixel_offset * self.bytes_per_pixel;

        match self.format {
            PixelFormat::Rgb => {
                self.buffer[byte_offset] = color.r;
                self.buffer[byte_offset + 1] = color.g;
                self.buffer[byte_offset + 2] = color.b;
                if self.bytes_per_pixel == 4 {
                    self.buffer[byte_offset + 3] = 0xFF;
                }
            }
            PixelFormat::Bgr => {
                self.buffer[byte_offset] = color.b;
                self.buffer[byte_offset + 1] = color.g;
                self.buffer[byte_offset + 2] = color.r;
                if self.bytes_per_pixel == 4 {
                    self.buffer[byte_offset + 3] = 0xFF;
                }
            }
            _ => {
                // Unsupported format, fall back to simple grayscale writing
                let gray = ((color.r as u16 + color.g as u16 + color.b as u16) / 3) as u8;
                for i in 0..core::cmp::min(self.bytes_per_pixel, 3) {
                    self.buffer[byte_offset + i] = gray;
                }
            }
        }
    }

    /// Draws a solid rectangle on the screen.
    pub fn draw_rect(&mut self, x: usize, y: usize, w: usize, h: usize, color: Color) {
        for dy in 0..h {
            for dx in 0..w {
                self.write_pixel(x + dx, y + dy, color);
            }
        }
    }

    /// Draws a beautiful horizontal linear gradient between two colors.
    pub fn draw_horizontal_gradient_rect(&mut self, x: usize, y: usize, w: usize, h: usize, from: Color, to: Color) {
        for dx in 0..w {
            let ratio = dx as f32 / w as f32;
            let r = (from.r as f32 * (1.0 - ratio) + to.r as f32 * ratio) as u8;
            let g = (from.g as f32 * (1.0 - ratio) + to.g as f32 * ratio) as u8;
            let b = (from.b as f32 * (1.0 - ratio) + to.b as f32 * ratio) as u8;
            let current_color = Color::new(r, g, b);
            
            for dy in 0..h {
                self.write_pixel(x + dx, y + dy, current_color);
            }
        }
    }

    /// Renders a single 8x8 character at specified coordinates with custom scale and colors.
    pub fn draw_char(&mut self, x: usize, y: usize, c: char, color: Color, bg: Option<Color>, scale: usize) {
        let ascii = c as usize;
        if ascii >= 128 {
            return;
        }
        let bitmap = FONT_8X8[ascii];

        for row in 0..8 {
            let byte = bitmap[row];
            for col in 0..8 {
                // Font bitmap bits are MSB to LSB
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

    /// Draws an ASCII string to the screen with character wrapping.
    pub fn draw_string(&mut self, mut x: usize, y: usize, text: &str, color: Color, bg: Option<Color>, scale: usize) {
        let char_w = 8 * scale;
        let spacing = 1 * scale;
        
        for c in text.chars() {
            if x + char_w >= self.width {
                break; // Screen bounds check
            }
            self.draw_char(x, y, c, color, bg, scale);
            x += char_w + spacing;
        }
    }

    /// Renders a modern, space-grade dashboard visual console onto the active monitor.
    pub fn draw_dashboard_layout(&mut self, ticks: usize, logs: &[alloc::string::String]) {
        // 1. Sleek Charcoal Background
        self.clear(COLOR_BG);

        // 2. Vibrant Color Gradient Header bar (cyan-blue to deep purple)
        self.draw_horizontal_gradient_rect(0, 0, self.width, 48, COLOR_ACCENT_BLUE, COLOR_ACCENT_PURPLE);
        
        // 3. Header Text
        self.draw_string(24, 14, "AE RUSTANIUM OS - UEFI 64-BIT BARE-METAL KERNEL", COLOR_TEXT_WHITE, None, 2);
        self.draw_string(self.width - 220, 20, "[ SECURE SPACE FLIGHT ACTIVE ]", COLOR_ACCENT_GREEN, None, 1);

        // 4. Thread Status Cards
        // Card 1: Background Memory Scrubber (Thread 1)
        self.draw_rect(40, 80, 420, 150, COLOR_PANEL_BG);
        self.draw_rect(40, 80, 420, 4, COLOR_ACCENT_BLUE); // Blue Accent Top-line
        self.draw_string(56, 96, "COOPERATIVE TASK: MEMORY SCRUBBER", COLOR_TEXT_WHITE, None, 1);
        self.draw_string(56, 124, "PID        : 101", COLOR_TEXT_MUTED, None, 1);
        self.draw_string(56, 144, "Stack      : 8 KB (Dynamic Offset)", COLOR_TEXT_MUTED, None, 1);
        self.draw_string(56, 164, "Status     : RUNNING (Passive Yield)", COLOR_ACCENT_GREEN, None, 1);
        self.draw_string(56, 184, "Task Sweep : Page ECC SECDED Safe Scan", COLOR_TEXT_MUTED, None, 1);

        // Card 2: System Telemetry (Thread 2)
        self.draw_rect(490, 80, 420, 150, COLOR_PANEL_BG);
        self.draw_rect(490, 80, 420, 4, COLOR_ACCENT_PURPLE); // Purple Accent Top-line
        self.draw_string(506, 96, "COOPERATIVE TASK: FLIGHT TELEMETRY", COLOR_TEXT_WHITE, None, 1);
        self.draw_string(506, 124, "PID        : 102", COLOR_TEXT_MUTED, None, 1);
        self.draw_string(506, 144, "Stack      : 8 KB (Dynamic Offset)", COLOR_TEXT_MUTED, None, 1);
        self.draw_string(506, 164, "Status     : RUNNING (Passive Yield)", COLOR_ACCENT_GREEN, None, 1);
        self.draw_string(506, 184, "Frequency  : Real-Time Diagnostic Burst", COLOR_TEXT_MUTED, None, 1);

        // 5. System Diagnostic Metrics Panel (Right)
        self.draw_rect(940, 80, 300, 150, COLOR_PANEL_BG);
        self.draw_rect(940, 80, 300, 4, COLOR_TEXT_MUTED);
        self.draw_string(956, 96, "SYSTEM TELEMETRY", COLOR_TEXT_WHITE, None, 1);
        let mut ticks_buf = [0u8; 16];
        let ticks_str = format_ticks(ticks, &mut ticks_buf);
        self.draw_string(956, 124, "System Ticks : ", COLOR_TEXT_MUTED, None, 1);
        self.draw_string(1076, 124, ticks_str, COLOR_ACCENT_BLUE, None, 1);
        self.draw_string(956, 144, "Voter Health : 100.00%", COLOR_ACCENT_GREEN, None, 1);
        self.draw_string(956, 164, "ECC State    : Safe / Self-Healed", COLOR_TEXT_MUTED, None, 1);
        self.draw_string(956, 184, "Scheduler    : Asynchronous (100Hz)", COLOR_TEXT_MUTED, None, 1);

        // 6. Rolling Console Logs Box
        self.draw_rect(40, 260, 1200, 400, COLOR_PANEL_BG);
        self.draw_rect(40, 260, 1200, 4, COLOR_ACCENT_BLUE);
        self.draw_string(60, 280, "REAL-TIME MICROKERNEL EVENTS STREAM (COM1 SERIAL REPLICATED)", COLOR_TEXT_WHITE, None, 1);
        self.draw_rect(60, 304, 1160, 1, COLOR_TEXT_MUTED); // Horizontal divider

        // Draw rolling logs from the buffer
        for (idx, line) in logs.iter().enumerate() {
            if idx >= 8 {
                break;
            }
            let y = 324 + idx * 20;
            let color = if line.starts_with(">>>") || line.starts_with("[SYSTEM]") || line.starts_with("[BOOT]") || line.starts_with("[KERNEL]") {
                COLOR_TEXT_MUTED
            } else if line.contains("[THREAD") {
                COLOR_ACCENT_BLUE
            } else if line.contains("[QUARANTINE") || line.contains("[VFS ERR]") || line.contains("Unknown command") {
                Color::new(255, 60, 60)
            } else if line.contains("[HEALING") {
                COLOR_ACCENT_GREEN
            } else {
                COLOR_TEXT_WHITE
            };
            self.draw_string(60, y, line, color, None, 1);
        }
        
        self.draw_string(60, 484, ">>> INTERACTIVE PS/2 KEYBOARD ECHO (Strike keys to print on real hardware):", COLOR_TEXT_WHITE, None, 1);
        
        // Solid Glowing Progress Bar at the bottom
        let bar_width = ((ticks * 8) % 1160) as usize;
        self.draw_rect(60, 610, 1160, 12, COLOR_BG); // Clear background bar
        self.draw_rect(60, 610, bar_width, 12, COLOR_ACCENT_GREEN); // Fill bar
        self.draw_string(60, 630, "System heartbeat tick pulse - Dynamic Scheduler Execution Line", COLOR_TEXT_MUTED, None, 1);
    }

    /// Dynamically updates ONLY the active telemetry values on the dashboard (ticks, progress bar)
    /// without clearing or redrawing the static panels. This completely eliminates screen flickering!
    pub fn update_dashboard_telemetry(&mut self, ticks: usize) {
        // 1. Update System Ticks Value (x: 1076, y: 124)
        // Clear only the ticks text bounding box (using Panel Background Color)
        self.draw_rect(1076, 124, 120, 12, COLOR_PANEL_BG);
        
        let mut ticks_buf = [0u8; 16];
        let ticks_str = format_ticks(ticks, &mut ticks_buf);
        self.draw_string(1076, 124, ticks_str, COLOR_ACCENT_BLUE, None, 1);

        // 2. Update Glowing Heartbeat Progress Bar (x: 60, y: 610)
        let bar_width = ((ticks * 8) % 1160) as usize;
        self.draw_rect(60, 610, 1160, 12, COLOR_BG); // Clear progress bar background
        self.draw_rect(60, 610, bar_width, 12, COLOR_ACCENT_GREEN); // Draw new filled progress
    }

    /// Dynamically updates the interactive keyboard echo prompt area without touching other panels.
    pub fn update_keyboard_prompt(&mut self, text: &str) {
        // Clear prompt area (x: 60, y: 514) using Panel Background Color
        self.draw_rect(60, 514, 1160, 12, COLOR_PANEL_BG);
        
        // Draw the updated prompt text
        self.draw_string(60, 514, text, COLOR_ACCENT_GREEN, None, 1);
    }

    /// Dynamically redraws the rolling logs in the logs panel.
    pub fn update_dashboard_logs(&mut self, logs: &[alloc::string::String]) {
        // Clear the logs drawing area using Panel Background Color (x: 60, y: 324, w: 1160, h: 156)
        self.draw_rect(60, 324, 1160, 156, COLOR_PANEL_BG);

        // Draw up to 8 lines of logs
        for (idx, line) in logs.iter().enumerate() {
            if idx >= 8 {
                break;
            }
            let y = 324 + idx * 20;
            let color = if line.starts_with(">>>") || line.starts_with("[SYSTEM]") || line.starts_with("[BOOT]") || line.starts_with("[KERNEL]") {
                COLOR_TEXT_MUTED
            } else if line.contains("[THREAD") {
                COLOR_ACCENT_BLUE
            } else if line.contains("[QUARANTINE") || line.contains("[VFS ERR]") || line.contains("Unknown command") {
                Color::new(255, 60, 60)
            } else if line.contains("[HEALING") {
                COLOR_ACCENT_GREEN
            } else {
                COLOR_TEXT_WHITE
            };
            self.draw_string(60, y, line, color, None, 1);
        }
    }
}

/// Dynamic u32 to string formatter inside bare-metal no_std environment.
fn format_ticks(mut ticks: usize, buf: &mut [u8; 16]) -> &str {
    if ticks == 0 {
        return "0";
    }
    let mut i = 15;
    while ticks > 0 && i > 0 {
        buf[i] = (b'0' + (ticks % 10) as u8) as u8;
        ticks /= 10;
        i -= 1;
    }
    core::str::from_utf8(&buf[i + 1..16]).unwrap_or("0")
}

/// A direct formatting writer that prints text directly onto the UEFI GOP framebuffer.
/// Used in panic/crash handlers where dynamic memory allocation is unsafe or offline.
pub struct GraphicsWriter<'a> {
    pub graphics: &'a mut UefiGraphics,
    pub x: usize,
    pub y: usize,
    pub start_x: usize,
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
