//! # Unified GOP Framebuffer Graphic Drivers Module
//!
//! Separates low-level graphics context rendering, monospace bitmap fonts,
//! character formatting, and dedicated screen layouts into clean Unix-like sub-modules:
//! - `font`: Holds color representations and static character bitmaps.
//! - `core`: Exposes low-level `UefiGraphics` context and shape drawing.
//! - `text`: Renders text strings on the display planes.
//! - `dashboard`: Displays F2 orbital navigation and telemetry metrics.
//! - `tty`: Displays F1 virtual log terminal and prompt interfaces.

pub mod font;
pub mod core;
pub mod text;
pub mod dashboard;
pub mod tty;

// Re-export public interface items to simplify external imports across the kernel
pub use font::{
    Color, COLOR_BG, COLOR_ACCENT_BLUE, COLOR_ACCENT_GREEN, COLOR_ACCENT_PURPLE,
    COLOR_TEXT_WHITE, COLOR_TEXT_MUTED, COLOR_PANEL_BG,
};
pub use core::{UefiGraphics, GraphicsWriter};
pub use dashboard::format_ticks;
