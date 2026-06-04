use crate::utils::Align4096;

// ------------------------------------------------------------
// Double Buffer Software Renderer (Stored in BSS Segment)
// Support up to 4K resolution (3840 * 2160 * 3 BGR format = 24,883,200 bytes)
// ------------------------------------------------------------

pub static mut BACK_BUFFER: Align4096<[u8; 24_883_200]> = Align4096([0; 24_883_200]);
pub static mut WALLPAPER_CACHE: Align4096<[u8; 24_883_200]> = Align4096([0; 24_883_200]);

pub static mut SCREEN_WIDTH: i32 = 1280;
pub static mut SCREEN_HEIGHT: i32 = 720;
pub static mut SCREEN_FORMAT: u32 = 0; // 0 = Bgr, 1 = Rgb
pub static mut CPU_HISTORY: [u8; 40] = [0; 40];

pub static mut START_MENU_OPEN: bool = false;
pub static mut START_MENU_ANIMATING: bool = false;
pub static mut START_MENU_ANIM_PROGRESS: f32 = 0.0;
