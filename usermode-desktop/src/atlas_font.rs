/// Atlas font renderer — Inter font glyphs pre-rasterized at build time.
///
/// Sizes available (suffix):
///   S = 14px body  |  M = 18px medium  |  L = 24px large / titles
/// Weights available:
///   REGULAR  |  SEMIBOLD
///
/// Usage:
///   draw_text_atlas(x, y, text, r, g, b, AtlasSize::M, AtlasWeight::Regular);

// Include the build-time generated atlas data
include!(concat!(env!("OUT_DIR"), "/atlas_data.rs"));

#[derive(Copy, Clone)]
pub enum AtlasSize {
    /// 14px — body text, labels
    Small,
    /// 18px — medium UI text, window content
    Medium,
    /// 24px — titles, headings
    Large,
}

#[derive(Copy, Clone)]
pub enum AtlasWeight {
    Regular,
    SemiBold,
}

/// Draw a single glyph from the atlas.
/// `x`, `y` = top-left of the cell.
/// `alpha_mul` = multiplier applied to the glyph coverage (0..=255).
#[inline]
pub fn draw_glyph_atlas(
    x: i32,
    y: i32,
    ch: char,
    r: u8,
    g: u8,
    b: u8,
    size: AtlasSize,
    weight: AtlasWeight,
) -> i32 // returns advance_x (pixels to move pen right)
{
    let code = ch as usize;
    if code < 32 || code >= 127 {
        return match size {
            AtlasSize::Small  => ATLAS_REGULAR_S_CELL_W as i32 / 2,
            AtlasSize::Medium => ATLAS_REGULAR_M_CELL_W as i32 / 2,
            AtlasSize::Large  => ATLAS_REGULAR_L_CELL_W as i32 / 2,
        };
    }
    let idx = code - 32;

    match (size, weight) {
        (AtlasSize::Small, AtlasWeight::Regular) => {
            blit_glyph(x, y, idx, r, g, b,
                &ATLAS_REGULAR_S_PIXELS,
                &ATLAS_REGULAR_S_ADV,
                ATLAS_REGULAR_S_CELL_W,
                ATLAS_REGULAR_S_CELL_H)
        }
        (AtlasSize::Small, AtlasWeight::SemiBold) => {
            blit_glyph(x, y, idx, r, g, b,
                &ATLAS_SEMIBOLD_S_PIXELS,
                &ATLAS_SEMIBOLD_S_ADV,
                ATLAS_SEMIBOLD_S_CELL_W,
                ATLAS_SEMIBOLD_S_CELL_H)
        }
        (AtlasSize::Medium, AtlasWeight::Regular) => {
            blit_glyph(x, y, idx, r, g, b,
                &ATLAS_REGULAR_M_PIXELS,
                &ATLAS_REGULAR_M_ADV,
                ATLAS_REGULAR_M_CELL_W,
                ATLAS_REGULAR_M_CELL_H)
        }
        (AtlasSize::Medium, AtlasWeight::SemiBold) => {
            blit_glyph(x, y, idx, r, g, b,
                &ATLAS_SEMIBOLD_M_PIXELS,
                &ATLAS_SEMIBOLD_M_ADV,
                ATLAS_SEMIBOLD_M_CELL_W,
                ATLAS_SEMIBOLD_M_CELL_H)
        }
        (AtlasSize::Large, AtlasWeight::Regular) => {
            blit_glyph(x, y, idx, r, g, b,
                &ATLAS_REGULAR_L_PIXELS,
                &ATLAS_REGULAR_L_ADV,
                ATLAS_REGULAR_L_CELL_W,
                ATLAS_REGULAR_L_CELL_H)
        }
        (AtlasSize::Large, AtlasWeight::SemiBold) => {
            blit_glyph(x, y, idx, r, g, b,
                &ATLAS_SEMIBOLD_L_PIXELS,
                &ATLAS_SEMIBOLD_L_ADV,
                ATLAS_SEMIBOLD_L_CELL_W,
                ATLAS_SEMIBOLD_L_CELL_H)
        }
    }
}

/// Blit one glyph from a specific atlas array into BACK_BUFFER.
/// Returns the advance width in pixels.
fn blit_glyph<const N: usize, const CW: usize, const CH: usize>(
    pen_x: i32,
    pen_y: i32,
    idx: usize,
    r: u8,
    g: u8,
    b: u8,
    pixels: &[[[u8; CW]; CH]; N],
    advances: &[u8; N],
    _cell_w: usize,
    _cell_h: usize,
) -> i32 {
    use crate::state::{BACK_BUFFER, SCREEN_WIDTH, SCREEN_HEIGHT};
    let sw = unsafe { SCREEN_WIDTH };
    let sh = unsafe { SCREEN_HEIGHT };

    for row in 0..CH {
        for col in 0..CW {
            let alpha = pixels[idx][row][col];
            if alpha == 0 { continue; }
            let px = pen_x + col as i32;
            let py = pen_y + row as i32;
            if px < 0 || py < 0 || px >= sw || py >= sh { continue; }
            let off = (py * sw + px) as usize * 3;
            unsafe {
                let buf = &mut BACK_BUFFER.0;
                if off + 2 < buf.len() {
                    let a = alpha as u32;
                    let ia = 255 - a;
                    let br = buf[off + 2] as u32;
                    let bg = buf[off + 1] as u32;
                    let bb = buf[off    ] as u32;
                    buf[off + 2] = ((r as u32 * a + br * ia) / 255) as u8;
                    buf[off + 1] = ((g as u32 * a + bg * ia) / 255) as u8;
                    buf[off    ] = ((b as u32 * a + bb * ia) / 255) as u8;
                }
            }
        }
    }
    advances[idx] as i32
}

/// Draw a full string using the Inter atlas.
/// Returns the total width rendered in pixels.
pub fn draw_text_atlas(
    mut x: i32,
    y: i32,
    text: &str,
    r: u8,
    g: u8,
    b: u8,
    size: AtlasSize,
    weight: AtlasWeight,
) -> i32 {
    let start_x = x;
    for ch in text.chars() {
        let adv = draw_glyph_atlas(x, y, ch, r, g, b, size, weight);
        x += adv;
    }
    x - start_x
}

/// Measure the pixel width of a string without rendering.
pub fn measure_text(text: &str, size: AtlasSize, weight: AtlasWeight) -> i32 {
    let mut w = 0i32;
    for ch in text.chars() {
        let code = ch as usize;
        if code < 32 || code >= 127 { continue; }
        let idx = code - 32;
        let adv = match (size, weight) {
            (AtlasSize::Small,  AtlasWeight::Regular)  => ATLAS_REGULAR_S_ADV[idx]  as i32,
            (AtlasSize::Small,  AtlasWeight::SemiBold) => ATLAS_SEMIBOLD_S_ADV[idx] as i32,
            (AtlasSize::Medium, AtlasWeight::Regular)  => ATLAS_REGULAR_M_ADV[idx]  as i32,
            (AtlasSize::Medium, AtlasWeight::SemiBold) => ATLAS_SEMIBOLD_M_ADV[idx] as i32,
            (AtlasSize::Large,  AtlasWeight::Regular)  => ATLAS_REGULAR_L_ADV[idx]  as i32,
            (AtlasSize::Large,  AtlasWeight::SemiBold) => ATLAS_SEMIBOLD_L_ADV[idx] as i32,
        };
        w += adv;
    }
    w
}

/// Draw a full string using the Inter atlas with character spacing padding.
/// Returns the total width rendered in pixels.
pub fn draw_text_atlas_spaced(
    mut x: i32,
    y: i32,
    text: &str,
    r: u8,
    g: u8,
    b: u8,
    size: AtlasSize,
    weight: AtlasWeight,
    spacing: i32,
) -> i32 {
    let start_x = x;
    for ch in text.chars() {
        let adv = draw_glyph_atlas(x, y, ch, r, g, b, size, weight);
        x += adv + spacing;
    }
    if x > start_x {
        x - start_x - spacing
    } else {
        0
    }
}

/// Measure the pixel width of a string with spacing padding without rendering.
pub fn measure_text_spaced(text: &str, size: AtlasSize, weight: AtlasWeight, spacing: i32) -> i32 {
    let mut w = 0i32;
    let mut count = 0;
    for ch in text.chars() {
        let code = ch as usize;
        if code < 32 || code >= 127 { continue; }
        let idx = code - 32;
        let adv = match (size, weight) {
            (AtlasSize::Small,  AtlasWeight::Regular)  => ATLAS_REGULAR_S_ADV[idx]  as i32,
            (AtlasSize::Small,  AtlasWeight::SemiBold) => ATLAS_SEMIBOLD_S_ADV[idx] as i32,
            (AtlasSize::Medium, AtlasWeight::Regular)  => ATLAS_REGULAR_M_ADV[idx]  as i32,
            (AtlasSize::Medium, AtlasWeight::SemiBold) => ATLAS_SEMIBOLD_M_ADV[idx] as i32,
            (AtlasSize::Large,  AtlasWeight::Regular)  => ATLAS_REGULAR_L_ADV[idx]  as i32,
            (AtlasSize::Large,  AtlasWeight::SemiBold) => ATLAS_SEMIBOLD_L_ADV[idx] as i32,
        };
        w += adv + spacing;
        count += 1;
    }
    if count > 0 {
        w - spacing
    } else {
        0
    }
}

