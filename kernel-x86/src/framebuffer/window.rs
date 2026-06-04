//! # Wayland-Style Composited Window Manager
//!
//! Implements a modern window management and composition system using private
//! off-screen buffers for each window. Pencereler draw on their own private canvas
//! which the Compositor then copies directly to the UEFI Graphics plane in Z-order.
//! This completely prevents X11-style flickering and achieves Wayland-like performance.

use alloc::vec::Vec;
use crate::framebuffer::{Color, UefiGraphics, COLOR_PANEL_BG, COLOR_BG, COLOR_ACCENT_BLUE, COLOR_ACCENT_PURPLE, COLOR_TEXT_WHITE, COLOR_TEXT_MUTED};

/// Cyber-glow pink active focus accent color.
pub const COLOR_GLOW_PINK: Color = Color::new(255, 0, 160);

/// Cyber-glow cyan active window borders.
pub const COLOR_GLOW_CYAN: Color = Color::new(0, 245, 255);

/// Cyber-glow green active radar signals.
pub const COLOR_GLOW_GREEN: Color = Color::new(32, 223, 127);

/// Represents a Wayland-style composited Window holding its coordinates, title,
/// active status, and private off-screen pixel backbuffer.
pub struct Window {
    /// Unique identifier for the window.
    pub id: usize,
    /// Header title string.
    pub title: &'static str,
    /// Absolute horizontal screen coordinate (x=0 left).
    pub x: i32,
    /// Absolute vertical screen coordinate (y=0 top).
    pub y: i32,
    /// Total width of the client canvas area in pixels.
    pub width: i32,
    /// Total height of the client canvas area in pixels.
    pub height: i32,
    /// Odakta (en üstte) olup olmadığını belirtir.
    pub is_active: bool,
    /// Private off-screen pixel array backbuffer.
    pub buffer: Vec<Color>,
    /// Bir önceki çizilen X koordinatı.
    pub prev_x: i32,
    /// Bir önceki çizilen Y koordinatı.
    pub prev_y: i32,
    /// Bir önceki çizilen Genişlik.
    pub prev_width: i32,
    /// Bir önceki çizilen Yükseklik.
    pub prev_height: i32,
    /// Bir önceki konum verisinin geçerli olup olmadığı.
    pub has_prev: bool,
}

impl Window {
    /// Creates a new Window allocating its private off-screen memory canvas.
    pub fn new(id: usize, title: &'static str, x: i32, y: i32, width: i32, height: i32) -> Self {
        let size = (width * height) as usize;
        let mut buffer = Vec::with_capacity(size);
        buffer.resize(size, COLOR_PANEL_BG);
        Self {
            id,
            title,
            x,
            y,
            width,
            height,
            is_active: false,
            buffer,
            prev_x: x,
            prev_y: y,
            prev_width: width,
            prev_height: height,
            has_prev: false,
        }
    }

    /// Safely writes a single pixel to the private window canvas.
    #[inline]
    pub fn write_pixel(&mut self, wx: i32, wy: i32, color: Color) {
        if wx < 0 || wx >= self.width || wy < 0 || wy >= self.height {
            return;
        }
        self.buffer[(wy * self.width + wx) as usize] = color;
    }

    /// Fills the entire window canvas with a solid Color.
    pub fn clear(&mut self, color: Color) {
        for pixel in self.buffer.iter_mut() {
            *pixel = color;
        }
    }

    /// Draws a solid filled rectangle inside the window canvas.
    pub fn draw_rect(&mut self, rx: i32, ry: i32, rw: i32, rh: i32, color: Color) {
        for y in 0..rh {
            for x in 0..rw {
                self.write_pixel(rx + x, ry + y, color);
            }
        }
    }

    /// Draws a thin frame border inside the window canvas.
    pub fn draw_border_rect(&mut self, rx: i32, ry: i32, rw: i32, rh: i32, color: Color) {
        for x in 0..rw {
            self.write_pixel(rx + x, ry, color);
            self.write_pixel(rx + x, ry + rh - 1, color);
        }
        for y in 0..rh {
            self.write_pixel(rx, ry + y, color);
            self.write_pixel(rx + rw - 1, ry + y, color);
        }
    }

    /// Renders a single CP437 ASCII character onto the off-screen window canvas.
    pub fn draw_char(&mut self, cx: i32, cy: i32, c: char, color: Color) {
        let codepoint = c as usize;
        if codepoint >= 128 {
            return;
        }
        let bitmap = crate::framebuffer::font::FONT_8X8[codepoint];
        for row in 0..8 {
            let row_val = bitmap[row];
            for col in 0..8 {
                if (row_val & (0x80 >> col)) != 0 {
                    self.write_pixel(cx + col, cy + row as i32, color);
                }
            }
        }
    }

    /// Renders a full text string onto the off-screen window canvas.
    pub fn draw_string(&mut self, mut sx: i32, sy: i32, text: &str, color: Color) {
        for c in text.chars() {
            self.draw_char(sx, sy, c, color);
            sx += 8;
        }
    }

    /// Generates dynamic cyber metrics and updates the window's off-screen buffer.
    pub fn render_window_contents(&mut self, ticks: usize, _core: Option<&kernel_core::SystemCore>, logs: &[alloc::string::String]) {
        self.clear(COLOR_PANEL_BG);

        // Thin interior border decoration
        self.draw_border_rect(0, 0, self.width, self.height, Color::new(40, 45, 55));

        match self.id {
            0 => {
                // Window 1: SYSTEM TELEMETRY
                self.draw_string(16, 16, "★ SYSTEM TELEMETRY (LIVE)", COLOR_GLOW_CYAN);
                self.draw_rect(16, 28, self.width - 32, 1, Color::new(60, 65, 75));

                let mut ticks_buf = [0u8; 16];
                let ticks_str = crate::framebuffer::dashboard::format_ticks(ticks, &mut ticks_buf);
                let mut line1 = alloc::string::String::new();
                line1.push_str("System Ticks : ");
                line1.push_str(ticks_str);
                self.draw_string(24, 42, &line1, COLOR_TEXT_WHITE);

                let heap_free = 1024 * 1024; // Simulated static free heap for canvas
                let mut heap_buf = [0u8; 16];
                let heap_str = crate::framebuffer::dashboard::format_ticks(heap_free / 1024, &mut heap_buf);
                let mut line2 = alloc::string::String::new();
                line2.push_str("Heap Free    : ");
                line2.push_str(heap_str);
                line2.push_str(" KB");
                self.draw_string(24, 62, &line2, COLOR_TEXT_WHITE);

                self.draw_string(24, 82, "Scheduler    : Round-Robin", COLOR_TEXT_MUTED);
                self.draw_string(24, 102, "TMR Health   : 100.00%", COLOR_GLOW_GREEN);

                // Progress indicator grid
                let bar_width = ((ticks / 4) % (self.width as usize - 48)) as i32;
                self.draw_rect(24, 126, self.width - 48, 6, COLOR_BG);
                self.draw_rect(24, 126, bar_width, 6, COLOR_GLOW_GREEN);
            }
            1 => {
                // Window 2: TTY LOG VIEWER
                self.draw_string(16, 16, "★ TTY SYSTEM LOGS (F1)", COLOR_GLOW_PINK);
                self.draw_rect(16, 28, self.width - 32, 1, Color::new(60, 65, 75));

                let total_lines = logs.len();
                let visible_lines = 7;
                let start_idx = if total_lines > visible_lines { total_lines - visible_lines } else { 0 };
                let display_slice = &logs[start_idx..total_lines];

                for (idx, line) in display_slice.iter().enumerate() {
                    let y = 42 + idx as i32 * 18;
                    // Truncate line if it exceeds window width
                    let mut display_line = line.clone();
                    if display_line.len() > 40 {
                        display_line.truncate(38);
                        display_line.push_str("..");
                    }
                    let color = if line.starts_with(">>>") || line.contains("[SYSTEM]") {
                        COLOR_TEXT_MUTED
                    } else if line.contains("Unknown") {
                        Color::new(255, 60, 60)
                    } else {
                        COLOR_TEXT_WHITE
                    };
                    self.draw_string(24, y, &display_line, color);
                }
            }
            2 => {
                // Window 3: FLIGHT RADAR & SATELLITE
                self.draw_string(16, 16, "★ ORBITAL RADAR SENSOR", COLOR_GLOW_GREEN);
                self.draw_rect(16, 28, self.width - 32, 1, Color::new(60, 65, 75));

                self.draw_string(24, 42, "SAT ID : AE-RUST-COMP", COLOR_TEXT_WHITE);
                self.draw_string(24, 62, "ALT    : 340.21 KM", COLOR_TEXT_WHITE);
                self.draw_string(24, 82, "VEL    : 7.68 KM/S", COLOR_TEXT_WHITE);

                // Pulsing sweeping coordinate mock radar line
                let sweep_val = (ticks * 2) % 360;
                let mut sweep_buf = [0u8; 16];
                let sweep_str = crate::framebuffer::dashboard::format_ticks(sweep_val, &mut sweep_buf);
                let mut coord = alloc::string::String::new();
                coord.push_str("Sweep  : ");
                coord.push_str(sweep_str);
                coord.push_str(" DEG");
                self.draw_string(24, 102, &coord, COLOR_GLOW_GREEN);
            }
            _ => {}
        }
    }
}

/// Z-order manager that maps, compositionally composites, and coordinates pencereler.
pub struct WindowManager {
    /// Active windows list arranged by depth Z-order (window at index len-1 is on top).
    pub windows: Vec<Window>,
    /// Index of the window currently being dragged.
    pub active_window_idx: Option<usize>,
    /// Whether the mouse cursor is currently dragging a title bar.
    pub is_dragging: bool,
    /// Grab coordinate relative offset X.
    pub drag_offset_x: i32,
    /// Grab coordinate relative offset Y.
    pub drag_offset_y: i32,
}

impl WindowManager {
    /// Creates and positions the 3 default premium cyberpunk windows.
    pub fn new() -> Self {
        let w1 = Window::new(0, "SYSTEM TELEMETRY", 80, 100, 320, 160);
        let w2 = Window::new(1, "VIRTUAL CONSOLE LOGS", 450, 150, 380, 200);
        let w3 = Window::new(2, "FLIGHT RADAR", 180, 360, 280, 140);
        
        let mut windows = Vec::new();
        windows.push(w3);
        windows.push(w1);
        
        // Make TTY Logs the active window at boot
        let mut w2_mut = w2;
        w2_mut.is_active = true;
        windows.push(w2_mut);

        Self {
            windows,
            active_window_idx: None,
            is_dragging: false,
            drag_offset_x: 0,
            drag_offset_y: 0,
        }
    }

    /// Evaluates mouse inputs (coordinates, click states) and calculates dragging/Z-ordering logic.
    pub fn handle_mouse(&mut self, mx: i32, my: i32, clicked: bool) {
        if clicked {
            if self.is_dragging {
                if let Some(idx) = self.active_window_idx {
                    // Update window coordinates
                    let mut win = self.windows.remove(idx);
                    
                    let mut new_x = mx - self.drag_offset_x;
                    let mut new_y = my - self.drag_offset_y;
                    
                    // Clamp to screen bounds to keep title bar accessible and prevent panel overlapping
                    let top_limit = 48 + 25; // Header bar height (48) + window title bar (24) + border (1)
                    let bottom_limit = 610 - win.height; // Navigation tab bar top (610) - window height
                    
                    if new_y < top_limit {
                        new_y = top_limit;
                    }
                    if new_y > bottom_limit {
                        new_y = bottom_limit;
                    }
                    
                    let left_limit = -win.width + 50;
                    let right_limit = 1280 - 50;
                    if new_x < left_limit {
                        new_x = left_limit;
                    }
                    if new_x > right_limit {
                        new_x = right_limit;
                    }
                    
                    win.x = new_x;
                    win.y = new_y;
                    
                    // Keep window at the top of Z-order
                    self.windows.push(win);
                    self.active_window_idx = Some(self.windows.len() - 1);
                }
            } else {
                // Find if clicked on the title bar of any window (iterating from top to bottom)
                let mut clicked_idx = None;
                for i in (0..self.windows.len()).rev() {
                    let win = &self.windows[i];
                    // Title bar is 24px high directly above the window y coordinate
                    if mx >= win.x && mx < win.x + win.width && my >= win.y - 24 && my < win.y {
                        clicked_idx = Some(i);
                        break;
                    }
                }

                if let Some(idx) = clicked_idx {
                    // Odak durumlarını güncelle
                    let mut win = self.windows.remove(idx);
                    for w in self.windows.iter_mut() {
                        w.is_active = false;
                    }
                    win.is_active = true;

                    // Initialize drag state variables
                    self.drag_offset_x = mx - win.x;
                    self.drag_offset_y = my - win.y;
                    self.is_dragging = true;

                    // Push to the top of Z-order
                    self.windows.push(win);
                    self.active_window_idx = Some(self.windows.len() - 1);
                } else {
                    // Check if clicked inside the client body of any window just to focus it
                    let mut focus_idx = None;
                    for i in (0..self.windows.len()).rev() {
                        let win = &self.windows[i];
                        if mx >= win.x && mx < win.x + win.width && my >= win.y && my < win.y + win.height {
                            focus_idx = Some(i);
                            break;
                        }
                    }

                    if let Some(idx) = focus_idx {
                        let mut win = self.windows.remove(idx);
                        for w in self.windows.iter_mut() {
                            w.is_active = false;
                        }
                        win.is_active = true;
                        self.windows.push(win);
                    }
                }
            }
        } else {
            self.is_dragging = false;
            self.active_window_idx = None;
        }
    }
}

/// Helper to clear a specific rectangle area with the cyber background color and grid lines.
pub fn clear_rect_with_grid(graphics: &mut UefiGraphics, rx: i32, ry: i32, rw: i32, rh: i32) {
    for y in 0..rh {
        let py = ry + y;
        if py < 0 || py >= graphics.height as i32 {
            continue;
        }
        for x in 0..rw {
            let px = rx + x;
            if px < 0 || px >= graphics.width as i32 {
                continue;
            }
            
            // Do not corrupt the top header bar area (y < 48)
            if py < 48 {
                continue;
            }

            // Draw grid or background color
            let is_grid = (px % 40 == 0) || (py % 40 == 0);
            let color = if is_grid {
                Color::new(22, 26, 32)
            } else {
                COLOR_BG
            };
            graphics.write_pixel(px as usize, py as usize, color);
        }
    }
}

/// Draws a solid rectangle with proper coordinate boundary checks (clipping).
pub fn draw_rect_safe(graphics: &mut UefiGraphics, x: i32, y: i32, width: i32, height: i32, color: Color) {
    for row in 0..height {
        let py = y + row;
        if py < 0 || py >= graphics.height as i32 {
            continue;
        }
        for col in 0..width {
            let px = x + col;
            if px < 0 || px >= graphics.width as i32 {
                continue;
            }
            graphics.write_pixel(px as usize, py as usize, color);
        }
    }
}

/// Draws a horizontal gradient rectangle with proper coordinate boundary checks (clipping).
pub fn draw_gradient_rect_safe(graphics: &mut UefiGraphics, x: i32, y: i32, width: i32, height: i32, start: Color, end: Color) {
    for col in 0..width {
        let px = x + col;
        if px < 0 || px >= graphics.width as i32 {
            continue;
        }
        let ratio = col as f32 / width as f32;
        let r = (start.r as f32 + (end.r as f32 - start.r as f32) * ratio) as u8;
        let g = (start.g as f32 + (end.g as f32 - start.g as f32) * ratio) as u8;
        let b = (start.b as f32 + (end.b as f32 - start.b as f32) * ratio) as u8;
        let color = Color::new(r, g, b);

        for row in 0..height {
            let py = y + row;
            if py < 0 || py >= graphics.height as i32 {
                continue;
            }
            graphics.write_pixel(px as usize, py as usize, color);
        }
    }
}

/// Draws a CP437 ASCII string with proper coordinate boundary checks (clipping).
pub fn draw_string_safe(graphics: &mut UefiGraphics, x: i32, y: i32, text: &str, color: Color) {
    let mut sx = x;
    for c in text.chars() {
        let codepoint = c as usize;
        if codepoint < 128 {
            let bitmap = crate::framebuffer::font::FONT_8X8[codepoint];
            for row in 0..8 {
                let py = y + row as i32;
                if py < 0 || py >= graphics.height as i32 {
                    continue;
                }
                let row_val = bitmap[row];
                for col in 0..8 {
                    let px = sx + col;
                    if px < 0 || px >= graphics.width as i32 {
                        continue;
                    }
                    if (row_val & (0x80 >> col)) != 0 {
                        graphics.write_pixel(px as usize, py as usize, color);
                    }
                }
            }
        }
        sx += 8;
    }
}

impl WindowManager {
    /// Composites and draws all windows' off-screen buffers onto the main GOP framebuffer.
    pub fn draw(&mut self, graphics: &mut UefiGraphics, ticks: usize, core: Option<&kernel_core::SystemCore>, logs: &[alloc::string::String], force_clear: bool, render_contents: bool) {
        // 1. Render all window contents off-screen ONLY when requested (reduces character redraw overhead during drag!)
        if render_contents {
            for window in self.windows.iter_mut() {
                window.render_window_contents(ticks, core, logs);
            }
        }

        // 2. Clear old positions of windows that have moved (Incremental Smart Clearing)
        // This completely replaces full screen clearing during drag, eliminating all flicker!
        for window in self.windows.iter() {
            if window.has_prev && (window.x != window.prev_x || window.y != window.prev_y) {
                // Clear the window's old bounds including title bar (-25px) and cyber glow border padding (+-2px)
                let rx = window.prev_x - 2;
                let ry = window.prev_y - 26;
                let rw = window.prev_width + 4;
                let rh = window.prev_height + 28;
                clear_rect_with_grid(graphics, rx, ry, rw, rh);
            }
        }

        // If force_clear is set (e.g. boot/F3 activation, or TTY log changes), perform a full redraw of the environment
        if force_clear {
            // Clear screen to solid charcoal background
            graphics.clear(COLOR_BG);

            // Draw premium cyber desktop background grid lines
            for y in (0..graphics.height).step_by(40) {
                graphics.draw_rect(0, y, graphics.width, 1, Color::new(22, 26, 32));
            }
            for x in (0..graphics.width).step_by(40) {
                graphics.draw_rect(x, 0, 1, graphics.height, Color::new(22, 26, 32));
            }

            // Draw modern visual header bar gradient
            graphics.draw_horizontal_gradient_rect(0, 0, graphics.width, 48, COLOR_ACCENT_BLUE, COLOR_ACCENT_PURPLE);
            graphics.draw_string_8x16(24, 16, "AE RUSTANIUM DESKTOP - WAYLAND COMPOSITED WINDOW MANAGER (F3)", COLOR_TEXT_WHITE, None, 1);
            
            // Draw desktop visual navigation tab buttons
            graphics.draw_navigation_tabs_desktop();
        }
        
        // 3. Composite pencereler in Z-order (bottom to top)
        for window in self.windows.iter() {
            // Copy (blit) private off-screen window canvas onto GOP framebuffer
            for dy in 0..window.height {
                let py = window.y + dy;
                if py < 0 || py >= graphics.height as i32 {
                    continue;
                }
                for dx in 0..window.width {
                    let px = window.x + dx;
                    if px < 0 || px >= graphics.width as i32 {
                        continue;
                    }
                    let pixel_color = window.buffer[(dy * window.width + dx) as usize];
                    graphics.write_pixel(px as usize, py as usize, pixel_color);
                }
            }

            // Draw horizontal gradient Title Bar directly above the window body with safe clipping
            draw_gradient_rect_safe(
                graphics,
                window.x,
                window.y - 24,
                window.width,
                24,
                COLOR_ACCENT_BLUE,
                COLOR_ACCENT_PURPLE,
            );

            // Draw Title text with safe clipping
            draw_string_safe(
                graphics,
                window.x + 8,
                window.y - 16,
                window.title,
                COLOR_TEXT_WHITE,
            );

            // Draw premium Z-order Cyber-glow borders (active pink vs inactive gray)
            let border_color = if window.is_active { COLOR_GLOW_PINK } else { Color::new(60, 65, 75) };
            
            // Draw window decoration outer frames with safe clipping
            draw_rect_safe(graphics, window.x, window.y - 25, window.width, 1, border_color); // Top edge
            draw_rect_safe(graphics, window.x, window.y + window.height, window.width, 1, border_color); // Bottom edge
            draw_rect_safe(graphics, window.x, window.y - 25, 1, window.height + 26, border_color); // Left edge
            draw_rect_safe(graphics, window.x + window.width, window.y - 25, 1, window.height + 26, border_color); // Right edge
        }

        // 4. Update the previous coordinates history trackers for the next tick
        for window in self.windows.iter_mut() {
            window.prev_x = window.x;
            window.prev_y = window.y;
            window.prev_width = window.width;
            window.prev_height = window.height;
            window.has_prev = true;
        }
    }
}

/// Helper method added to UefiGraphics to render F3 tab button.
impl UefiGraphics {
    /// Renders three interactive tabs F1, F2 and F3 at the bottom.
    pub fn draw_navigation_tabs_desktop(&mut self) {
        let tty_tab_x = 60;
        let tty_tab_y = 610;
        let tty_tab_w = 200;
        let tty_tab_h = 32;

        let db_tab_x = 280;
        let db_tab_y = 610;
        let db_tab_w = 240;
        let db_tab_h = 32;

        let ds_tab_x = 540;
        let ds_tab_y = 610;
        let ds_tab_w = 240;
        let ds_tab_h = 32;

        let active_color = COLOR_GLOW_PINK;
        let inactive_color = COLOR_PANEL_BG;
        let border_color = Color::new(80, 85, 95);

        // F1 Tab (Inactive)
        self.draw_rect(tty_tab_x, tty_tab_y, tty_tab_w, tty_tab_h, inactive_color);
        self.draw_border_lines(tty_tab_x, tty_tab_y, tty_tab_w, tty_tab_h, border_color);
        self.draw_string(tty_tab_x + 24, tty_tab_y + 9, "[F1] TTY CONSOLE", COLOR_TEXT_MUTED, None, 1);

        // F2 Tab (Inactive)
        self.draw_rect(db_tab_x, db_tab_y, db_tab_w, db_tab_h, inactive_color);
        self.draw_border_lines(db_tab_x, db_tab_y, db_tab_w, db_tab_h, border_color);
        self.draw_string(db_tab_x + 10, db_tab_y + 9, "[F2] TELEMETRY DASHBOARD", COLOR_TEXT_MUTED, None, 1);

        // F3 Tab (Active!)
        self.draw_rect(ds_tab_x, ds_tab_y, ds_tab_w, ds_tab_h, active_color);
        self.draw_string(ds_tab_x + 8, ds_tab_y + 9, "★ [F3] WAYLAND WM DESKTOP", COLOR_TEXT_WHITE, None, 1);
    }

    /// Internal line border helper.
    fn draw_border_lines(&mut self, bx: usize, by: usize, bw: usize, bh: usize, color: Color) {
        for i in 0..bw {
            self.write_pixel(bx + i, by, color);
            self.write_pixel(bx + i, by + bh - 1, color);
        }
        for i in 0..bh {
            self.write_pixel(bx, by + i, color);
            self.write_pixel(bx + bw - 1, by + i, color);
        }
    }
}
