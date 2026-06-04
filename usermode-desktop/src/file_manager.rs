use crate::utils::StrbufWriter;
use crate::syscalls::{sys_open, sys_read, sys_close};
use crate::atlas_font::{draw_text_atlas, AtlasSize, AtlasWeight};
use crate::graphics::{draw_rect, draw_tiny_folder_icon, draw_tiny_file_icon};

pub fn draw_file_manager(ax: i32, ay: i32, win_width: usize, win_height: usize) {
    draw_text_atlas(ax + 24, ay + 46, "/ (root)", 165, 182, 210, AtlasSize::Small, AtlasWeight::SemiBold);
    draw_rect(ax + 24, ay + 63, win_width as i32 - 48, 1, 48, 52, 70);
    
    // Read directory `/` dynamically
    let fd = sys_open("/".as_ptr(), 1, 0);
    if fd != u64::MAX && fd < 16 {
        let mut dir_buf = [0u8; 1024];
        let bytes_read = sys_read(fd, dir_buf.as_mut_ptr(), 1024);
        sys_close(fd);
        if bytes_read != u64::MAX && bytes_read > 0 {
            let mut line_y = ay + 70;
            let slice = &dir_buf[..bytes_read as usize];
            if let Ok(s) = core::str::from_utf8(slice) {
                for entry in s.lines() {
                    if line_y + 28 > ay + win_height as i32 - 14 {
                        break;
                    }
                    let mut entry_buf = [0u8; 64];
                    let mut writer = StrbufWriter::new(&mut entry_buf);
                    if entry.ends_with('/') {
                        draw_tiny_folder_icon(ax + 24, line_y + 2);
                        let _ = core::fmt::write(&mut writer, format_args!("{}", &entry[..entry.len()-1]));
                        draw_text_atlas(ax + 46, line_y, writer.as_str(), 200, 210, 228, AtlasSize::Small, AtlasWeight::Regular);
                    } else {
                        draw_tiny_file_icon(ax + 24, line_y + 1);
                        let _ = core::fmt::write(&mut writer, format_args!("{}", entry));
                        draw_text_atlas(ax + 46, line_y, writer.as_str(), 215, 220, 235, AtlasSize::Small, AtlasWeight::Regular);
                    }
                    line_y += 28;
                }
            }
        }
    }
}
