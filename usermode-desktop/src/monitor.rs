use crate::utils::StrbufWriter;
use crate::atlas_font::{draw_text_atlas, AtlasSize, AtlasWeight};
use crate::graphics::{draw_rounded_rect, draw_rect_alpha, draw_line_thick};
use crate::state::CPU_HISTORY;
use crate::syscalls::SharedSystemInfo;

pub fn draw_monitor_window(ax: i32, ay: i32, shared_info: *const SharedSystemInfo) {
    unsafe {
        let ticks = (*shared_info).system_ticks;
        let heap_used = (*shared_info).heap_used;
        let heap_free = (*shared_info).heap_free;
        let cpu_usage = (*shared_info).cpu_usage;

        let mut buf = [0u8; 64];
        
        let uptime_s = ticks / 100;
        let uptime_ms = (ticks % 100) * 10;
        let mut w = StrbufWriter::new(&mut buf);
        let _ = core::fmt::write(&mut w, format_args!("Uptime  {}.{:02} s", uptime_s, uptime_ms));
        draw_text_atlas(ax + 24, ay + 46, w.as_str(), 200, 210, 228, AtlasSize::Small, AtlasWeight::Regular);

        w = StrbufWriter::new(&mut buf);
        let _ = core::fmt::write(&mut w, format_args!("Heap    {} KB used", heap_used / 1024));
        draw_text_atlas(ax + 24, ay + 68, w.as_str(), 200, 210, 228, AtlasSize::Small, AtlasWeight::Regular);

        w = StrbufWriter::new(&mut buf);
        let _ = core::fmt::write(&mut w, format_args!("Free    {} KB avail", heap_free / 1024));
        draw_text_atlas(ax + 24, ay + 90, w.as_str(), 200, 210, 228, AtlasSize::Small, AtlasWeight::Regular);

        let bar_x = ax + 24;
        let bar_y = ay + 138;
        let bar_w = 340;
        let bar_h = 12;
        draw_rounded_rect(bar_x, bar_y, bar_w, bar_h, 4, 40, 44, 62);
        let total_heap = 256 * 1024 * 1024;
        let filled_w = (bar_w as u64 * heap_used / total_heap) as i32;
        draw_rounded_rect(bar_x, bar_y, filled_w.max(4), bar_h, 4, 61, 174, 233);

        w = StrbufWriter::new(&mut buf);
        let _ = core::fmt::write(&mut w, format_args!("CPU     {}.{:02} %", cpu_usage / 100, cpu_usage % 100));
        draw_text_atlas(ax + 24, ay + 124, w.as_str(), 200, 210, 228, AtlasSize::Small, AtlasWeight::Regular);

        draw_text_atlas(ax + 24, ay + 158, "CPU History", 165, 182, 210, AtlasSize::Small, AtlasWeight::SemiBold);
        // Chart background
        draw_rounded_rect(ax + 24, ay + 178, 450, 148, 8, 18, 20, 32);
        draw_rect_alpha(ax + 24, ay + 228, 450, 1, 255, 255, 255, 10);
        draw_rect_alpha(ax + 24, ay + 278, 450, 1, 255, 255, 255, 10);

        let cx_base = ax + 30i32;
        let cy_base = ay + 324i32;
        let seg_w   = 11i32;
        let ch      = 140i32;
        for idx in 0..39usize {
            let v0 = CPU_HISTORY[idx]     as i32;
            let v1 = CPU_HISTORY[idx + 1] as i32;
            let x0 = cx_base + (idx as i32) * seg_w;
            let x1 = cx_base + ((idx + 1) as i32) * seg_w;
            let y0 = cy_base - (v0 * ch) / 100;
            let y1 = cy_base - (v1 * ch) / 100;
            for fill_x in x0..x1 {
                let t = (fill_x - x0) as f32 / seg_w as f32;
                let fy = (y0 as f32 + (y1 as f32 - y0 as f32) * t) as i32;
                let fh = cy_base - fy;
                if fh > 0 { draw_rect_alpha(fill_x, fy, 1, fh, 61, 174, 233, 50); }
            }
            draw_line_thick(x0, y0, x1, y1, 61, 174, 233);
        }
    }
}
