use crate::utils::{StrbufWriter, serial_print};
use crate::state::{WALLPAPER_CACHE, BACK_BUFFER, SCREEN_WIDTH, SCREEN_HEIGHT, SCREEN_FORMAT};

pub fn decode_bmp(bmp_data: &[u8]) -> Result<(), &'static str> {
    {
        let mut buf = [0u8; 128];
        let mut w = StrbufWriter::new(&mut buf);
        let _ = core::fmt::write(&mut w, format_args!("[DE] decode_bmp: ptr={:x}, len={}\n", bmp_data.as_ptr() as u64, bmp_data.len()));
        serial_print(w.as_str());
    }
    if bmp_data.len() < 54 {
        return Err("BMP data too short");
    }
    if bmp_data[0] != b'B' || bmp_data[1] != b'M' {
        return Err("Not a BMP file");
    }
    let data_offset = u32::from_le_bytes([bmp_data[10], bmp_data[11], bmp_data[12], bmp_data[13]]) as usize;
    let width = i32::from_le_bytes([bmp_data[18], bmp_data[19], bmp_data[20], bmp_data[21]]) as usize;
    let height = i32::from_le_bytes([bmp_data[22], bmp_data[23], bmp_data[24], bmp_data[25]]) as usize;
    let bpp = u16::from_le_bytes([bmp_data[28], bmp_data[29]]) as usize;
    
    if width != 640 || height != 360 {
        return Err("Unsupported resolution. Must be 640x360");
    }
    if bpp != 24 {
        return Err("Unsupported bits per pixel. Must be 24-bit BGR");
    }
    
    unsafe {
        let dest_ptr = core::ptr::addr_of_mut!(WALLPAPER_CACHE.0) as *mut u8;
        let src_ptr = bmp_data.as_ptr();
        let sw = SCREEN_WIDTH as usize;
        let sh = SCREEN_HEIGHT as usize;
        let is_bgr = SCREEN_FORMAT == 0;
        
        for dy in 0..sh {
            // Map destination Y directly to source BGR BMP (bottom-up flip and vertical scale)
            let src_y = 360 - 1 - (dy * 360 / sh);
            let src_row_start = data_offset + src_y * 640 * 3;
            let dest_row_start = dy * sw * 3;
            
            for dx in 0..sw {
                let src_x = dx * 640 / sw;
                let src_pixel_offset = src_row_start + src_x * 3;
                let dest_pixel_offset = dest_row_start + dx * 3;
                
                let b = *src_ptr.add(src_pixel_offset);
                let g = *src_ptr.add(src_pixel_offset + 1);
                let r = *src_ptr.add(src_pixel_offset + 2);
                
                if is_bgr {
                    *dest_ptr.add(dest_pixel_offset) = b;
                    *dest_ptr.add(dest_pixel_offset + 1) = g;
                    *dest_ptr.add(dest_pixel_offset + 2) = r;
                } else {
                    *dest_ptr.add(dest_pixel_offset) = r;
                    *dest_ptr.add(dest_pixel_offset + 1) = g;
                    *dest_ptr.add(dest_pixel_offset + 2) = b;
                }
            }
        }
    }
    Ok(())
}

pub fn draw_wallpaper() {
    unsafe {
        let sw = SCREEN_WIDTH as usize;
        let sh = SCREEN_HEIGHT as usize;
        let size = sw * sh * 3;
        let src_ptr = core::ptr::addr_of!(WALLPAPER_CACHE.0) as *const u8;
        let dest_ptr = core::ptr::addr_of_mut!(BACK_BUFFER.0) as *mut u8;
        core::ptr::copy_nonoverlapping(src_ptr, dest_ptr, size);
    }
}
