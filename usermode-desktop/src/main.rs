#![no_std]
#![no_main]

//! # Ring 3 User Space Desktop Environment for AE Rustanium
//!
//! Features:
//! 1. Modularity with Unix-style split submodules.
//! 2. 4K screen resolution support (up to 3840x2160, 24.88 MB buffers).
//! 3. Dynamic startup wallpaper scaling.
//! 4. Event-driven software rendering (low CPU overhead, zero mouse lag).

pub mod syscalls;
pub mod font;
pub mod graphics;
pub mod window;
pub mod console;
pub mod atlas_font;
pub mod utils;
pub mod state;
pub mod wallpaper;
pub mod monitor;
pub mod taskbar;
pub mod file_manager;

use syscalls::*;
use graphics::*;
use window::*;
use console::*;
use atlas_font::*;
use utils::*;
use state::*;
use wallpaper::*;
use monitor::*;
use taskbar::*;
use file_manager::*;

// ------------------------------------------------------------
// Entry Point and Stack Alignment
// ------------------------------------------------------------

#[link_section = ".text.start"]
#[no_mangle]
pub extern "C" fn _start() -> ! {
    unsafe {
        // Enforce 16-byte stack alignment conforming to System V AMD64 ABI before entering main loop
        core::arch::asm!(
            "and rsp, -16",
            "call main_rust",
            options(noreturn)
        );
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    struct StderrWriter;
    impl core::fmt::Write for StderrWriter {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            let _ = sys_write(2, s.as_ptr(), s.len());
            Ok(())
        }
    }

    let mut writer = StderrWriter;
    let _ = core::fmt::write(&mut writer, format_args!("\n!!! USERMODE DESKTOP PANIC !!!\n{}\n", info));
    loop {}
}

// ------------------------------------------------------------
// Main User-Space Desktop Loop
// ------------------------------------------------------------

#[no_mangle]
extern "C" fn main_rust() -> ! {
    serial_print("[DE] Entered main_rust\n");
    let shared_info = sys_get_shared_info();
    serial_print("[DE] Shared info fetched\n");

    let mut screen_info = ScreenInfo {
        framebuffer_addr: 0,
        width: 0,
        height: 0,
        stride: 0,
        bytes_per_pixel: 0,
        format: 0,
    };
    let map_status = sys_map_fb(&mut screen_info);
    serial_print("[DE] Framebuffer mapping requested\n");
    if map_status != 0 {
        serial_print("[DE] Framebuffer mapping failed! Exiting...\n");
        syscall0(3);
    }

    unsafe {
        SCREEN_WIDTH = screen_info.width as i32;
        SCREEN_HEIGHT = screen_info.height as i32;
        SCREEN_FORMAT = screen_info.format;
    }
    {
        let mut buf = [0u8; 128];
        let mut w = StrbufWriter::new(&mut buf);
        unsafe {
            let sw = SCREEN_WIDTH;
            let sh = SCREEN_HEIGHT;
            let fmt = SCREEN_FORMAT;
            let _ = core::fmt::write(&mut w, format_args!("[DE] Screen: {}x{} format={}\n", sw, sh, fmt));
        }
        serial_print(w.as_str());
    }

    // Generate the nebula wallpaper once — cached in WALLPAPER_CACHE for per-frame blit
    serial_print("[DE] Generating nebula wallpaper...\n");
    init_nebula_wallpaper();
    serial_print("[DE] Wallpaper ready.\n");

    term_init();
    serial_print("[DE] Term initialized\n");

    unsafe {
        WINDOWS[0] = Some(Window {
            id: 0,
            title: "System Monitor",
            x: 100,
            y: 60,
            width: 520,
            height: 420,
            is_dragging: false,
            is_focused: false,
            is_open: true,
            is_maximized: false,
            prev_x: 100,
            prev_y: 60,
            prev_w: 520,
            prev_h: 420,
            is_animating: false,
            anim_progress: 100,
            anim_direction: true,
        });

        WINDOWS[1] = Some(Window {
            id: 2,
            title: "File Manager",
            x: 680,
            y: 80,
            width: 480,
            height: 360,
            is_dragging: false,
            is_focused: true,
            is_open: true,
            is_maximized: false,
            prev_x: 680,
            prev_y: 80,
            prev_w: 480,
            prev_h: 360,
            is_animating: false,
            anim_progress: 100,
            anim_direction: true,
        });

        WINDOWS[2] = Some(Window {
            id: 1,
            title: "Console",
            x: 200,
            y: 200,
            width: 580,
            height: 380,
            is_dragging: false,
            is_focused: false,
            is_open: false,  // start closed — opens via taskbar/launcher
            is_maximized: false,
            prev_x: 200,
            prev_y: 200,
            prev_w: 580,
            prev_h: 380,
            is_animating: false,
            anim_progress: 100,
            anim_direction: true,
        });
    }

    let mut cursor_x: i32 = 640;
    let mut cursor_y: i32 = 360;
    let mut prev_mouse_x: i32 = 640;
    let mut prev_mouse_y: i32 = 360;
    let mut prev_render_x: i32 = 640;
    let mut prev_render_y: i32 = 360;
    let mut prev_left_clicked: u32 = 0;

    let mut serial_buf = [0u8; 16];

    let mut event = InputEvent {
        event_type: 0,
        keyboard_key: 0,
        mouse_x: 0,
        mouse_y: 0,
        mouse_left_clicked: 0,
        mouse_right_clicked: 0,
    };

    let mut needs_redraw = true;
    let mut last_tick_update = 0;
    let mut last_anim_tick = 0;

    loop {
        let got_event = sys_wait_event(&mut event, 2);
        let mut event_processed = false;

        if got_event == 1 {
            event_processed = true;
            loop {
                if event.event_type == 1 {
                    needs_redraw = true;
                    let key = event.keyboard_key;
                    
                    let mut terminal_focused = false;
                    unsafe {
                        for i in 0..4 {
                            if let Some(ref win) = WINDOWS[i] {
                                if win.id == 1 && win.is_focused {
                                    terminal_focused = true;
                                    break;
                                }
                            }
                        }
                    }

                    if terminal_focused {
                        if key == 0x1001 { // Enter
                            term_process_command();
                        } else if key == 0x1000 { // Backspace
                            term_print_char('\x08');
                        } else if key < 0x1000 {
                            term_print_char((key as u8) as char);
                        }
                    }
                } else if event.event_type == 2 {
                    cursor_x = event.mouse_x;
                    cursor_y = event.mouse_y;
                    let left_clicked = event.mouse_left_clicked;

                    unsafe {
                        let sw = SCREEN_WIDTH;
                        let sh = SCREEN_HEIGHT;
                        let in_dock_zone = cursor_y >= (sh - 120);
                        let prev_in_dock_zone = prev_render_y >= (sh - 120);
                        let in_start_menu_zone = if START_MENU_OPEN {
                            let (_dock_start_x, _dock_w, dock_sizes, dock_xs) = get_dock_layout(sw, sh, cursor_x, cursor_y);
                            let launchpad_cx = dock_xs[0] + dock_sizes[0] / 2.0;
                            let menu_w = 220i32;
                            let menu_h = 185i32;
                            let menu_x = (launchpad_cx - menu_w as f32 / 2.0) as i32;
                            let menu_y = (sh - 82) - menu_h - 12;
                            cursor_x >= menu_x && cursor_x < menu_x + menu_w &&
                            cursor_y >= menu_y && cursor_y < menu_y + menu_h
                        } else {
                            false
                        };
                        let prev_in_start_menu_zone = if START_MENU_OPEN {
                            let (_dock_start_x, _dock_w, dock_sizes, dock_xs) = get_dock_layout(sw, sh, prev_render_x, prev_render_y);
                            let launchpad_cx = dock_xs[0] + dock_sizes[0] / 2.0;
                            let menu_w = 220i32;
                            let menu_h = 185i32;
                            let menu_x = (launchpad_cx - menu_w as f32 / 2.0) as i32;
                            let menu_y = (sh - 82) - menu_h - 12;
                            prev_render_x >= menu_x && prev_render_x < menu_x + menu_w &&
                            prev_render_y >= menu_y && prev_render_y < menu_y + menu_h
                        } else {
                            false
                        };

                        if left_clicked == 1 || prev_left_clicked == 1 || in_dock_zone || prev_in_dock_zone || in_start_menu_zone || prev_in_start_menu_zone {
                            needs_redraw = true;
                        }
                    }

                    let dx = cursor_x - prev_mouse_x;
                    let dy = cursor_y - prev_mouse_y;

                    if left_clicked == 1 {
                        if prev_left_clicked == 0 {
                            // Mouse Down!
                            let mut event_consumed = false;
                            
                            // Check Start Menu (Launchpad) first
                            unsafe {
                                let (_dock_start_x, _dock_w, dock_sizes, dock_xs) = get_dock_layout(SCREEN_WIDTH, SCREEN_HEIGHT, cursor_x, cursor_y);
                                let dock_y = SCREEN_HEIGHT - 82;
                                
                                if START_MENU_OPEN && !START_MENU_ANIMATING {
                                    let launchpad_cx = dock_xs[0] + dock_sizes[0] / 2.0;
                                    let menu_w = 220i32;
                                    let menu_h = 185i32;
                                    let menu_x = (launchpad_cx - menu_w as f32 / 2.0) as i32;
                                    let menu_y = dock_y - menu_h - 12;
                                    
                                    if cursor_x >= menu_x && cursor_x < menu_x + menu_w &&
                                       cursor_y >= menu_y && cursor_y < menu_y + menu_h {
                                        event_consumed = true;
                                        for i in 0..4 {
                                            let iy = menu_y + 44 + (i as i32) * 33;
                                            if cursor_x >= menu_x + 8 && cursor_x < menu_x + menu_w - 8 &&
                                               cursor_y >= iy && cursor_y < iy + 27 {
                                                if i == 0 {
                                                    focus_window_by_id(0); // Metrics
                                                } else if i == 1 {
                                                    focus_window_by_id(2); // Files
                                                } else if i == 2 {
                                                    focus_window_by_id(1); // Console
                                                } else if i == 3 {
                                                    sys_write(2, "Shutting down system...\n".as_ptr(), 24);
                                                    syscall0(3);
                                                }
                                                START_MENU_ANIMATING = true;
                                                START_MENU_OPEN = false;
                                                break;
                                            }
                                        }
                                    } else {
                                        let on_launchpad = cursor_x >= dock_xs[0] as i32 && cursor_x < (dock_xs[0] + dock_sizes[0]) as i32 &&
                                                           cursor_y >= dock_y && cursor_y < dock_y + 72;
                                        if !on_launchpad {
                                            START_MENU_ANIMATING = true;
                                            START_MENU_OPEN = false;
                                            event_consumed = true;
                                        }
                                    }
                                }
                            }
                            
                            if !event_consumed {
                                // Check Dock click
                                unsafe {
                                    let (dock_start_x, dock_w, dock_sizes, dock_xs) = get_dock_layout(SCREEN_WIDTH, SCREEN_HEIGHT, cursor_x, cursor_y);
                                    let dock_y = SCREEN_HEIGHT - 82;
                                    
                                    if cursor_y >= dock_y && cursor_y < dock_y + 72 &&
                                       cursor_x >= dock_start_x as i32 && cursor_x < (dock_start_x + dock_w) as i32 {
                                        
                                        event_consumed = true;
                                        
                                        for i in 0..4 {
                                            let item_x = dock_xs[i];
                                            let item_size = dock_sizes[i];
                                            if cursor_x >= item_x as i32 && cursor_x < (item_x + item_size) as i32 {
                                                if i == 0 {
                                                    START_MENU_ANIMATING = true;
                                                    START_MENU_OPEN = !START_MENU_OPEN;
                                                } else {
                                                    let win_id = match i {
                                                        1 => 0, // Metrics
                                                        2 => 2, // Files
                                                        3 => 1, // Console
                                                        _ => 0,
                                                    };
                                                    
                                                    let mut found_win_idx = None;
                                                    for idx in 0..4 {
                                                        if let Some(ref win) = WINDOWS[idx] {
                                                            if win.id == win_id {
                                                                found_win_idx = Some(idx);
                                                                break;
                                                            }
                                                        }
                                                    }
                                                    
                                                    if let Some(idx) = found_win_idx {
                                                        let is_open = WINDOWS[idx].as_ref().unwrap().is_open;
                                                        let is_focused = WINDOWS[idx].as_ref().unwrap().is_focused;
                                                        
                                                        if is_open && is_focused {
                                                            let mut win_mut = WINDOWS[idx].take().unwrap();
                                                            win_mut.is_animating = true;
                                                            win_mut.anim_direction = false;
                                                            win_mut.anim_progress = 100;
                                                            WINDOWS[idx] = Some(win_mut);
                                                        } else {
                                                            focus_window_by_id(win_id);
                                                        }
                                                    }
                                                }
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                            
                            if !event_consumed {
                                // Mouse Down! Hit test from top to bottom
                                let mut clicked_idx = None;
                                let mut drag_started = false;
                                unsafe {
                                    let mut count = 0;
                                    for i in 0..4 {
                                        if WINDOWS[i].is_some() {
                                            count += 1;
                                        }
                                    }
                                    
                                    if count > 0 {
                                        for i in (0..count).rev() {
                                            if let Some(ref win) = WINDOWS[i] {
                                                if !win.is_open {
                                                    continue;
                                                }
                                                if hit_test_title(win, cursor_x, cursor_y) {
                                                    let (ax, ay) = win.get_animated_pos();
                                                    // Traffic-light buttons: LEFT side (ax+16, ax+34, ax+52), 12x12, vertically centered at ay+11
                                                    let is_close_click = cursor_x >= ax + 16 && cursor_x < ax + 28 &&
                                                                         cursor_y >= ay + 11 && cursor_y < ay + 23;
                                                    let is_min_click   = cursor_x >= ax + 34 && cursor_x < ax + 46 &&
                                                                         cursor_y >= ay + 11 && cursor_y < ay + 23;
                                                    let is_max_click   = cursor_x >= ax + 52 && cursor_x < ax + 64 &&
                                                                         cursor_y >= ay + 11 && cursor_y < ay + 23;
                                                    
                                                    if is_close_click {
                                                        let mut win_mut = WINDOWS[i].take().unwrap();
                                                        win_mut.is_animating = true;
                                                        win_mut.anim_direction = false;
                                                        win_mut.anim_progress = 100;
                                                        WINDOWS[i] = Some(win_mut);
                                                        clicked_idx = Some(i);
                                                        drag_started = false;
                                                    } else if is_max_click {
                                                        let mut win_mut = WINDOWS[i].take().unwrap();
                                                        if !win_mut.is_maximized {
                                                            win_mut.prev_x = win_mut.x;
                                                            win_mut.prev_y = win_mut.y;
                                                            win_mut.prev_w = win_mut.width;
                                                            win_mut.prev_h = win_mut.height;
                                                            win_mut.x = 0;
                                                            win_mut.y = 0;
                                                            win_mut.width = SCREEN_WIDTH as usize;
                                                            win_mut.height = (SCREEN_HEIGHT - 52) as usize;
                                                            win_mut.is_maximized = true;
                                                        } else {
                                                            win_mut.x = win_mut.prev_x;
                                                            win_mut.y = win_mut.prev_y;
                                                            win_mut.width = win_mut.prev_w;
                                                            win_mut.height = win_mut.prev_h;
                                                            win_mut.is_maximized = false;
                                                        }
                                                        WINDOWS[i] = Some(win_mut);
                                                        clicked_idx = Some(i);
                                                        drag_started = false;
                                                    } else if is_min_click {
                                                        let mut win_mut = WINDOWS[i].take().unwrap();
                                                        win_mut.is_animating = true;
                                                        win_mut.anim_direction = false;
                                                        win_mut.anim_progress = 100;
                                                        WINDOWS[i] = Some(win_mut);
                                                        clicked_idx = Some(i);
                                                        drag_started = false;
                                                    } else {
                                                        if !win.is_maximized {
                                                            clicked_idx = Some(i);
                                                            drag_started = true;
                                                        } else {
                                                            clicked_idx = Some(i);
                                                            drag_started = false;
                                                        }
                                                    }
                                                    break;
                                                } else if hit_test_body(win, cursor_x, cursor_y) {
                                                    clicked_idx = Some(i);
                                                    drag_started = false;
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                    
                                    if let Some(idx) = clicked_idx {
                                        let mut win = WINDOWS[idx].take().unwrap();
                                        win.is_focused = true;
                                        if drag_started {
                                            win.is_dragging = true;
                                        }
                                        
                                        // Shift other windows left
                                        for j in idx..(count - 1) {
                                            WINDOWS[j] = WINDOWS[j + 1].take();
                                        }
                                        
                                        // Set others focused/dragging = false
                                        for j in 0..(count - 1) {
                                            if let Some(ref mut w) = WINDOWS[j] {
                                                w.is_focused = false;
                                                w.is_dragging = false;
                                            }
                                        }
                                        
                                        WINDOWS[count - 1] = Some(win);
                                    } else {
                                        // Clicked background, unfocus all
                                        for j in 0..count {
                                            if let Some(ref mut w) = WINDOWS[j] {
                                                w.is_focused = false;
                                                w.is_dragging = false;
                                            }
                                        }
                                        
                                        // Check if we clicked on desktop icons!
                                        if cursor_x >= 40 && cursor_x < 72 {
                                            if cursor_y >= 40 && cursor_y < 72 {
                                                focus_window_by_id(2);
                                            } else if cursor_y >= 120 && cursor_y < 152 {
                                                focus_window_by_id(1);
                                            } else if cursor_y >= 200 && cursor_y < 232 {
                                                focus_window_by_id(0);
                                            }
                                        }
                                    }
                                }
                            }
                        } else {
                            // Mouse Drag
                            unsafe {
                                for i in 0..4 {
                                    if let Some(ref mut win) = WINDOWS[i] {
                                        if win.is_dragging {
                                            win.x += dx;
                                            win.y += dy;
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        // Mouse Up
                        unsafe {
                            for i in 0..4 {
                                if let Some(ref mut win) = WINDOWS[i] {
                                    win.is_dragging = false;
                                }
                            }
                        }
                    }

                    prev_mouse_x = cursor_x;
                    prev_mouse_y = cursor_y;
                    prev_left_clicked = left_clicked;
                }

                if sys_wait_event(&mut event, 0) == 1 {
                    continue;
                } else {
                    break;
                }
            }
        }

        let read_bytes = sys_read(0, serial_buf.as_mut_ptr(), 16);
        if read_bytes > 0 && read_bytes != u64::MAX {
            event_processed = true;
            needs_redraw = true;
            let mut terminal_focused = false;
            unsafe {
                for i in 0..4 {
                    if let Some(ref win) = WINDOWS[i] {
                        if win.id == 1 && win.is_focused {
                            terminal_focused = true;
                            break;
                        }
                    }
                }
            }

            if terminal_focused {
                for i in 0..read_bytes as usize {
                    let byte = serial_buf[i];
                    if byte == b'\r' || byte == b'\n' {
                        term_process_command();
                    } else if byte == 0x08 || byte == 0x7F {
                        term_print_char('\x08');
                    } else if byte >= 32 && byte <= 126 {
                        term_print_char(byte as char);
                    }
                }
            }
        }

        let ticks = unsafe { (*shared_info).system_ticks };
        if ticks - last_tick_update >= 10 { // Update metrics every 100ms
            last_tick_update = ticks;
            needs_redraw = true;
            unsafe {
                let cpu_load = ((*shared_info).cpu_usage / 100) as u8;
                for i in 0..39 {
                    CPU_HISTORY[i] = CPU_HISTORY[i+1];
                }
                CPU_HISTORY[39] = cpu_load;
            }
        }

        // Tick animations based on system time rather than raw loop iterations.
        // This makes animation speeds consistent regardless of frame/event rate.
        let mut anim_running = false;
        unsafe {
            if START_MENU_ANIMATING { anim_running = true; }
            for i in 0..4 {
                if let Some(ref win) = WINDOWS[i] {
                    if win.is_animating { anim_running = true; }
                }
            }
        }

        let ticks = unsafe { (*shared_info).system_ticks };
        let tick_diff = (ticks - last_anim_tick) as i32;

        if tick_diff > 0 && anim_running {
            last_anim_tick = ticks;
            unsafe {
                if START_MENU_ANIMATING {
                    if START_MENU_OPEN {
                        START_MENU_ANIM_PROGRESS += 0.08 * tick_diff as f32;
                        if START_MENU_ANIM_PROGRESS >= 1.0 {
                            START_MENU_ANIM_PROGRESS = 1.0;
                            START_MENU_ANIMATING = false;
                        }
                    } else {
                        START_MENU_ANIM_PROGRESS -= 0.08 * tick_diff as f32;
                        if START_MENU_ANIM_PROGRESS <= 0.0 {
                            START_MENU_ANIM_PROGRESS = 0.0;
                            START_MENU_ANIMATING = false;
                        }
                    }
                }

                for i in 0..4 {
                    if let Some(ref mut win) = WINDOWS[i] {
                        if win.is_animating {
                            let step = 8 * tick_diff;
                            if win.anim_direction {
                                win.anim_progress += step;
                                if win.anim_progress >= 100 {
                                    win.anim_progress = 100;
                                    win.is_animating = false;
                                }
                            } else {
                                win.anim_progress -= step;
                                if win.anim_progress <= 0 {
                                    win.anim_progress = 0;
                                    win.is_animating = false;
                                    win.is_open = false;
                                    win.is_focused = false;
                                    
                                    // Focus next window
                                    let mut next_to_focus = None;
                                    for j in (0..4).rev() {
                                        if let Some(ref other_win) = WINDOWS[j] {
                                            if other_win.id != win.id && other_win.is_open {
                                                next_to_focus = Some(other_win.id);
                                                break;
                                            }
                                        }
                                    }
                                    if let Some(nid) = next_to_focus {
                                        focus_window_by_id(nid);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        } else if !anim_running {
            last_anim_tick = ticks;
        }

        if anim_running {
            needs_redraw = true;
        }

        if needs_redraw {
            // Blit the cached nebula wallpaper as the first layer
            draw_wallpaper();

            // Desktop sidebar icons — 48px size, more vertical spacing
            draw_icon(0, 20, 30);
            draw_text_atlas(4, 84, "Files", 210, 222, 240, AtlasSize::Small, AtlasWeight::Regular);
            draw_icon(1, 20, 130);
            draw_text_atlas(4, 184, "Terminal", 210, 222, 240, AtlasSize::Small, AtlasWeight::Regular);
            draw_icon(2, 20, 230);
            draw_text_atlas(4, 284, "Monitor", 210, 222, 240, AtlasSize::Small, AtlasWeight::Regular);

            unsafe {
                let sw = SCREEN_WIDTH;
                let sh = SCREEN_HEIGHT;

                for i in 0..4 {
                    if let Some(ref win) = WINDOWS[i] {
                        if !win.is_open && !win.is_animating {
                            continue;
                        }
                        let (ax, ay) = win.get_animated_pos();
                        draw_window_shadow(ax, ay, win.width as i32, win.height as i32);
                        draw_window(win);
                        
                        if win.id == 0 {
                            draw_monitor_window(ax, ay, shared_info);
                        }

                        if win.id == 1 {
                            draw_console_window(ax, ay);
                        }

                        if win.id == 2 {
                            draw_file_manager(ax, ay, win.width, win.height);
                        }
                    }
                }

                // Render modular taskbar and start menu
                draw_taskbar(sw, sh, cursor_x, cursor_y, ticks, shared_info);
            }

            let fb_ptr = screen_info.framebuffer_addr as *mut u8;
            unsafe {
                let back_buffer_ptr = core::ptr::addr_of!(BACK_BUFFER.0) as *const u8;
                let sw = SCREEN_WIDTH;
                let sh = SCREEN_HEIGHT;
                core::ptr::copy_nonoverlapping(
                    back_buffer_ptr,
                    fb_ptr,
                    (sw * sh * 3) as usize,
                );
                
                // Draw cursor directly on framebuffer
                draw_cursor_to_buf(fb_ptr, cursor_x, cursor_y, sw, sh);
                prev_render_x = cursor_x;
                prev_render_y = cursor_y;
            }

            needs_redraw = false;
        } else if cursor_x != prev_render_x || cursor_y != prev_render_y {
            let fb_ptr = screen_info.framebuffer_addr as *mut u8;
            unsafe {
                let sw = SCREEN_WIDTH;
                let sh = SCREEN_HEIGHT;
                // Restore background under old cursor from BACK_BUFFER
                copy_rect_back_to_fb(fb_ptr, prev_render_x, prev_render_y, 8, 12, sw, sh);
                // Draw cursor at new position directly to framebuffer
                draw_cursor_to_buf(fb_ptr, cursor_x, cursor_y, sw, sh);
                prev_render_x = cursor_x;
                prev_render_y = cursor_y;
            }
        }
    }
}


