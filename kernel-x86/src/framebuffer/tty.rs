//! # F1 Virtual TTY Console Visual Panels
//!
//! Renders the live graphical terminal (TTY) console view:
//! 1. `draw_tty_layout` - The static background structures, uptime monitors, and divider lines.
//! 2. `update_tty_telemetry` - targeted updates for uptime counters and heartbeat pulse lines.
//! 3. `update_tty_prompt` - updates terminal user prompts and current working directories.
//! 4. `update_tty_logs` - line-by-line flicker-free scrollback buffer rendering.
//! 5. `draw_navigation_tabs` - F1 Console vs F2 Dashboard tab bar buttons.

use crate::framebuffer::core::UefiGraphics;
use crate::framebuffer::font::*;

impl UefiGraphics {
    /// Renders the full-screen virtual terminal (TTY) console layout using the 8x16 font.
    pub fn draw_tty_layout(&mut self, ticks: usize, logs: &[alloc::string::String], scroll_offset: usize, line_buffer: &str, cwd: &str) {
        // 1. Sleek Charcoal Background
        self.clear(COLOR_BG);

        // 2. Vibrant Color Gradient Header bar (cyan-blue to deep purple) using 8x16 font
        self.draw_horizontal_gradient_rect(0, 0, self.width, 48, COLOR_ACCENT_BLUE, COLOR_ACCENT_PURPLE);
        self.draw_string_8x16(24, 16, "AE RUSTANIUM TTY - VIRTUAL CONSOLE TTY1 (8x16 Font)", COLOR_TEXT_WHITE, None, 1);
        
        let uptime_secs = ticks / 50;
        let mut uptime_buf = [0u8; 32];
        let uptime_str = format_uptime(uptime_secs, &mut uptime_buf);
        self.draw_string_8x16(self.width - 320, 16, uptime_str, COLOR_ACCENT_GREEN, None, 1);

        // 3. Render Scrollback Logs (y: 80 to 520, height 440)
        // Each character line is 16px high + 4px vertical spacing = 20px.
        // That gives 440 / 20 = 22 visible lines.
        let visible_lines = 22;
        let total_lines = logs.len();
        
        // Calculate slice indices based on scrollback offset from bottom
        let end_idx = if total_lines > scroll_offset {
            total_lines - scroll_offset
        } else {
            0
        };
        let start_idx = if end_idx > visible_lines {
            end_idx - visible_lines
        } else {
            0
        };

        let active_slice = &logs[start_idx..end_idx];

        for (idx, line) in active_slice.iter().enumerate() {
            let y = 80 + idx * 20;
            let color = if line.starts_with(">>>") || line.starts_with("[SYSTEM]") || line.starts_with("[BOOT]") || line.contains("[KERNEL]") {
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
            self.draw_string_8x16(60, y, line, color, None, 1);
        }

        // 4. Render Scrollbar Track
        let scroll_track_x = 1240;
        let scroll_track_y = 80;
        let scroll_track_w = 8;
        let scroll_track_h = 440;
        let scroll_track_color = Color::new(24, 28, 34);
        self.draw_rect(scroll_track_x, scroll_track_y, scroll_track_w, scroll_track_h, scroll_track_color);

        if total_lines > visible_lines {
            // Draw Scrollbar Thumb
            let thumb_h = core::cmp::max(30, (scroll_track_h * visible_lines) / total_lines);
            let max_scroll = total_lines - visible_lines;
            let scroll_ratio = scroll_offset as f32 / max_scroll as f32;
            let thumb_y = scroll_track_y + scroll_track_h - thumb_h - ((scroll_track_h - thumb_h) as f32 * scroll_ratio) as usize;
            self.draw_rect(scroll_track_x, thumb_y, scroll_track_w, thumb_h, COLOR_ACCENT_BLUE);
        }

        // Divider
        self.draw_rect(40, 535, 1200, 1, Color::new(80, 85, 95));

        // 5. Active Command Prompt (y: 550) using 8x16 font
        let mut prompt_buf = alloc::string::String::new();
        prompt_buf.push_str("rustanium:");
        prompt_buf.push_str(cwd);
        prompt_buf.push_str("> ");
        prompt_buf.push_str(line_buffer);
        self.draw_string_8x16(60, 550, &prompt_buf, COLOR_ACCENT_GREEN, None, 1);

        // 6. Visual navigation buttons
        self.draw_navigation_tabs(true);

        // 7. Heartbeat progress bar at bottom
        let bar_width = ((ticks * 8) % 1160) as usize;
        self.draw_rect(60, 660, 1160, 12, COLOR_BG);
        self.draw_rect(60, 660, bar_width, 12, COLOR_ACCENT_GREEN);
        self.draw_string_8x16(60, 680, "TTY Console heartbeat tick pulse - [ESC / F2] Back to Telemetry", COLOR_TEXT_MUTED, None, 1);
    }

    /// Dynamically updates ONLY the active telemetry values on the TTY layout
    /// (uptime header, bottom progress bar) to completely eliminate TTY flickering!
    pub fn update_tty_telemetry(&mut self, ticks: usize) {
        // 1. Update Uptime Header (x: self.width - 320, y: 16)
        self.draw_rect(self.width - 320, 16, 280, 16, COLOR_ACCENT_PURPLE);
        
        let uptime_secs = ticks / 50;
        let mut uptime_buf = [0u8; 32];
        let uptime_str = format_uptime(uptime_secs, &mut uptime_buf);
        self.draw_string_8x16(self.width - 320, 16, uptime_str, COLOR_ACCENT_GREEN, None, 1);

        // 2. Update Heartbeat Progress Bar (x: 60, y: 660)
        let bar_width = ((ticks * 8) % 1160) as usize;
        self.draw_rect(60, 660, 1160, 12, COLOR_BG); // Clear progress bar background
        self.draw_rect(60, 660, bar_width, 12, COLOR_ACCENT_GREEN); // Draw new filled progress
    }

    /// Dynamically updates the TTY active prompt line without touching other areas.
    pub fn update_tty_prompt(&mut self, line_buffer: &str, cwd: &str) {
        // Clear prompt area (x: 60, y: 550, w: 1160, h: 16) using main Background Color
        self.draw_rect(60, 550, 1160, 16, COLOR_BG);
        
        let mut prompt_buf = alloc::string::String::new();
        prompt_buf.push_str("rustanium:");
        prompt_buf.push_str(cwd);
        prompt_buf.push_str("> ");
        prompt_buf.push_str(line_buffer);
        self.draw_string_8x16(60, 550, &prompt_buf, COLOR_ACCENT_GREEN, None, 1);
    }

    /// Dynamically redraws the logs panel and the scrollbar inside TTY view.
    pub fn update_tty_logs(&mut self, logs: &[alloc::string::String], scroll_offset: usize) {
        let visible_lines = 22;
        let total_lines = logs.len();
        
        let end_idx = if total_lines > scroll_offset {
            total_lines - scroll_offset
        } else {
            0
        };
        let start_idx = if end_idx > visible_lines {
            end_idx - visible_lines
        } else {
            0
        };

        let active_slice = &logs[start_idx..end_idx];

        for (idx, line) in active_slice.iter().enumerate() {
            let y = 80 + idx * 20;
            // Clear only this single line's bounding box using Background Color (instantaneous, flicker-free!)
            self.draw_rect(60, y, 1170, 20, COLOR_BG);

            let color = if line.starts_with(">>>") || line.starts_with("[SYSTEM]") || line.starts_with("[BOOT]") || line.contains("[KERNEL]") {
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
            self.draw_string_8x16(60, y, line, color, Some(COLOR_BG), 1);
        }

        // Clear any remaining lines at the bottom of the logs panel (to maintain background integrity)
        for idx in active_slice.len()..visible_lines {
            let y = 80 + idx * 20;
            self.draw_rect(60, y, 1170, 20, COLOR_BG);
        }

        // Clear and update the Scrollbar Track & Thumb
        let scroll_track_x = 1240;
        let scroll_track_y = 80;
        let scroll_track_w = 8;
        let scroll_track_h = 440;
        let scroll_track_color = Color::new(24, 28, 34);
        self.draw_rect(scroll_track_x, scroll_track_y, scroll_track_w, scroll_track_h, scroll_track_color);

        if total_lines > visible_lines {
            // Draw Scrollbar Thumb
            let thumb_h = core::cmp::max(30, (scroll_track_h * visible_lines) / total_lines);
            let max_scroll = total_lines - visible_lines;
            let scroll_ratio = scroll_offset as f32 / max_scroll as f32;
            let thumb_y = scroll_track_y + scroll_track_h - thumb_h - ((scroll_track_h - thumb_h) as f32 * scroll_ratio) as usize;
            self.draw_rect(scroll_track_x, thumb_y, scroll_track_w, thumb_h, COLOR_ACCENT_BLUE);
        }
    }

    /// Draws the modern space-grade interactive mode buttons/tabs at the bottom.
    pub fn draw_navigation_tabs(&mut self, is_tty: bool) {
        let tty_tab_x = 60;
        let tty_tab_y = 550;
        let tty_tab_w = 200;
        let tty_tab_h = 32;

        let db_tab_x = 280;
        let db_tab_y = 550;
        let db_tab_w = 240;
        let db_tab_h = 32;

        // Colors
        let active_color = COLOR_ACCENT_BLUE;
        let inactive_color = COLOR_PANEL_BG;
        let border_color = Color::new(80, 85, 95);

        // 1. TTY Console Tab
        if is_tty {
            // Active Tab (Glowing Cyan-Blue)
            self.draw_rect(tty_tab_x, tty_tab_y, tty_tab_w, tty_tab_h, active_color);
            self.draw_string(tty_tab_x + 14, tty_tab_y + 9, "★ [F1] TTY CONSOLE", COLOR_TEXT_WHITE, None, 1);
        } else {
            // Inactive Tab
            self.draw_rect(tty_tab_x, tty_tab_y, tty_tab_w, tty_tab_h, inactive_color);
            // Draw border
            for i in 0..tty_tab_w {
                self.write_pixel(tty_tab_x + i, tty_tab_y, border_color);
                self.write_pixel(tty_tab_x + i, tty_tab_y + tty_tab_h - 1, border_color);
            }
            for i in 0..tty_tab_h {
                self.write_pixel(tty_tab_x, tty_tab_y + i, border_color);
                self.write_pixel(tty_tab_x + tty_tab_w - 1, tty_tab_y + i, border_color);
            }
            self.draw_string(tty_tab_x + 24, tty_tab_y + 9, "[F1] TTY CONSOLE", COLOR_TEXT_MUTED, None, 1);
        }

        // 2. Dashboard Tab
        if !is_tty {
            // Active Tab (Glowing Deep Purple)
            self.draw_rect(db_tab_x, db_tab_y, db_tab_w, db_tab_h, COLOR_ACCENT_PURPLE);
            self.draw_string(db_tab_x + 0, db_tab_y + 9, "★ [F2] TELEMETRY DASHBOARD", COLOR_TEXT_WHITE, None, 1);
        } else {
            // Inactive Tab
            self.draw_rect(db_tab_x, db_tab_y, db_tab_w, db_tab_h, inactive_color);
            // Draw border
            for i in 0..db_tab_w {
                self.write_pixel(db_tab_x + i, db_tab_y, border_color);
                self.write_pixel(db_tab_x + i, db_tab_y + db_tab_h - 1, border_color);
            }
            for i in 0..db_tab_h {
                self.write_pixel(db_tab_x, db_tab_y + i, border_color);
                self.write_pixel(db_tab_x + db_tab_w - 1, db_tab_y + i, border_color);
            }
            self.draw_string(db_tab_x + 10, db_tab_y + 9, "[F2] TELEMETRY DASHBOARD", COLOR_TEXT_MUTED, None, 1);
        }
    }
}

/// Dynamic seconds to uptime string formatter.
fn format_uptime(mut secs: usize, buf: &mut [u8; 32]) -> &str {
    if secs == 0 {
        return "UPTIME: 0s";
    }
    let mut i = 31;
    buf[i] = b's';
    i -= 1;
    while secs > 0 && i > 8 {
        buf[i] = b'0' + (secs % 10) as u8;
        secs /= 10;
        i -= 1;
    }
    buf[i] = b' ';
    buf[i-1] = b':';
    buf[i-2] = b'E';
    buf[i-3] = b'M';
    buf[i-4] = b'I';
    buf[i-5] = b'T';
    buf[i-6] = b'P';
    buf[i-7] = b'U';
    core::str::from_utf8(&buf[i-7..32]).unwrap_or("UPTIME: Unknown")
}
