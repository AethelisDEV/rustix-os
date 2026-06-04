use crate::graphics::draw_rounded_rect_alpha;
use crate::atlas_font::{AtlasSize, AtlasWeight};

#[derive(Copy, Clone)]
pub struct Window {
    pub id: u8,
    pub title: &'static str,
    pub x: i32,
    pub y: i32,
    pub width: usize,
    pub height: usize,
    pub is_dragging: bool,
    pub is_focused: bool,
    
    // Window state extensions
    pub is_open: bool,
    pub is_maximized: bool,
    pub prev_x: i32,
    pub prev_y: i32,
    pub prev_w: usize,
    pub prev_h: usize,
    
    // Slide animation support
    pub is_animating: bool,
    pub anim_progress: i32, // 0 to 100
    pub anim_direction: bool, // true = opening, false = closing
}

impl Window {
    pub fn get_animated_pos(&self) -> (i32, i32) {
        if self.is_animating {
            let progress = self.anim_progress as f32 / 100.0;
            let offset_y = (30.0 * (1.0 - progress)) as i32;
            (self.x, self.y + offset_y)
        } else {
            (self.x, self.y)
        }
    }
}

pub static mut WINDOWS: [Option<Window>; 4] = [None, None, None, None];
pub static mut IO_BUFFER: [u8; 4096] = [0; 4096];

pub fn hit_test_title(win: &Window, mx: i32, my: i32) -> bool {
    let (ax, ay) = win.get_animated_pos();
    mx >= ax && mx < ax + win.width as i32 &&
    my >= ay && my < ay + 34
}

pub fn hit_test_body(win: &Window, mx: i32, my: i32) -> bool {
    let (ax, ay) = win.get_animated_pos();
    mx >= ax && mx < ax + win.width as i32 &&
    my >= ay && my < ay + win.height as i32
}

pub fn focus_window_by_id(id: u8) {
    unsafe {
        let mut count = 0;
        for i in 0..4 {
            if WINDOWS[i].is_some() {
                count += 1;
            }
        }
        
        let mut found_idx = None;
        for i in 0..count {
            if let Some(ref win) = WINDOWS[i] {
                if win.id == id {
                    found_idx = Some(i);
                    break;
                }
            }
        }
        
        if let Some(idx) = found_idx {
            let mut win = WINDOWS[idx].take().unwrap();
            win.is_focused = true;
            win.is_dragging = false;
            
            // If the window was closed, start the opening animation
            if !win.is_open {
                win.is_open = true;
                win.is_animating = true;
                win.anim_direction = true;
                win.anim_progress = 0;
            }
            
            // Shift other windows left
            for j in idx..(count - 1) {
                WINDOWS[j] = WINDOWS[j + 1].take();
            }
            
            // Set others focused = false
            for j in 0..(count - 1) {
                if let Some(ref mut w) = WINDOWS[j] {
                    w.is_focused = false;
                    w.is_dragging = false;
                }
            }
            
            WINDOWS[count - 1] = Some(win);
        }
    }
}

pub fn draw_window(win: &Window) {
    let (ax, ay) = win.get_animated_pos();
    let w   = win.width as i32;
    let h   = win.height as i32;
    let r   = 14;   // corner radius — macOS-like roundness
    let tb  = 34;   // title-bar height

    // ── Window body — unified dark glass ──────────────────────────
    draw_rounded_rect_alpha(ax, ay, w, h, r, 24, 24, 28, 235);

    // ── Title text — Inter Regular, spaced out and centered ────────
    {
        use crate::atlas_font::{draw_text_atlas_spaced, measure_text_spaced};
        let spacing = 3;
        let tw = measure_text_spaced(win.title, AtlasSize::Small, AtlasWeight::Regular, spacing);
        let tx = ax + (w - tw) / 2;
        let ty = ay + (tb - 14) / 2;
        draw_text_atlas_spaced(tx, ty, win.title, 220, 228, 245, AtlasSize::Small, AtlasWeight::Regular, spacing);
    }

    // ── Traffic-light buttons — LEFT side, modern pastel colors ─
    let button_y = ay + 11;
    // Close (red pastel)
    draw_rounded_rect_alpha(ax + 16, button_y, 12, 12, 6, 255, 95, 87, 255);
    // Minimize (yellow pastel)
    draw_rounded_rect_alpha(ax + 34, button_y, 12, 12, 6, 255, 189, 46, 255);
    // Maximize (green pastel)
    draw_rounded_rect_alpha(ax + 52, button_y, 12, 12, 6, 39, 201, 63, 255);
}



