//! # Asynchronous PS/2 and Serial Keyboard Decoders
//!
//! Exposes keyboard input types, physical I/O port polling drivers,
//! serial COM1 data stream receivers, and scancode translation matrices
//! supporting both US and Turkish Q layouts (mapped to ASCII equivalents).

use x86_64::instructions::port::Port;

/// Represents keyboard events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardInput {
    /// Standard character key press.
    Char(char),
    /// Backspace key press.
    Backspace,
    /// Enter key press.
    Enter,
    /// F1 mode-switch key.
    F1,
    /// F2 mode-switch key.
    F2,
    /// Page Up scrollback key.
    PageUp,
    /// Page Down scrollback key.
    PageDown,
}

/// Keyboard layout type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardLayout {
    /// US standard English layout.
    Us,
    /// Turkish Q layout mapped to ASCII equivalents for monospace display compatibility.
    Trq,
}

/// Static manager tracking shifting states and active keyboard decoding layouts.
pub struct KeyboardState {
    shift_pressed: bool,
    layout: KeyboardLayout,
}

impl KeyboardState {
    /// Creates a new KeyboardState defaulting to US layout.
    pub const fn new() -> Self {
        Self {
            shift_pressed: false,
            layout: KeyboardLayout::Us,
        }
    }

    /// Sets the active decoding layout (e.g. US or Turkish Q).
    pub fn set_layout(&mut self, layout: KeyboardLayout) {
        self.layout = layout;
    }

    /// Intercepts a raw hardware scancode and translates it to a `KeyboardInput` event.
    pub fn handle_scancode(&mut self, scancode: u8) -> Option<KeyboardInput> {
        match scancode {
            // Left & Right Shift Pressed
            0x2A | 0x36 => {
                self.shift_pressed = true;
                None
            }
            // Left & Right Shift Released
            0xAA | 0xB6 => {
                self.shift_pressed = false;
                None
            }
            // Backspace
            0x0E => Some(KeyboardInput::Backspace),
            // Enter
            0x1C => Some(KeyboardInput::Enter),
            // F1 Pressed
            0x3B => Some(KeyboardInput::F1),
            // F2 Pressed
            0x3C => Some(KeyboardInput::F2),
            // Page Up Pressed
            0x49 => Some(KeyboardInput::PageUp),
            // Page Down Pressed
            0x51 => Some(KeyboardInput::PageDown),
            // Standard scan codes
            code => {
                // Ignore key releases (scan code set 1 sets bit 7)
                if code & 0x80 == 0 {
                    if let Some(c) = translate_scancode(code, self.shift_pressed, self.layout) {
                        Some(KeyboardInput::Char(c))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
        }
    }
}

/// Dynamic scancode to char converter.
/// Translates Turkish Q keys to their visually closest ASCII equivalents to support CP437 display graphics.
fn translate_scancode(scancode: u8, shift: bool, layout: KeyboardLayout) -> Option<char> {
    match layout {
        KeyboardLayout::Us => {
            let char_map = match scancode {
                0x02 => if shift { '!' } else { '1' },
                0x03 => if shift { '@' } else { '2' },
                0x04 => if shift { '#' } else { '3' },
                0x05 => if shift { '$' } else { '4' },
                0x06 => if shift { '%' } else { '5' },
                0x07 => if shift { '^' } else { '6' },
                0x08 => if shift { '&' } else { '7' },
                0x09 => if shift { '*' } else { '8' },
                0x0A => if shift { '(' } else { '9' },
                0x0B => if shift { ')' } else { '0' },
                0x0C => if shift { '_' } else { '-' },
                0x0D => if shift { '+' } else { '=' },
                0x10 => if shift { 'Q' } else { 'q' },
                0x11 => if shift { 'W' } else { 'w' },
                0x12 => if shift { 'E' } else { 'e' },
                0x13 => if shift { 'R' } else { 'r' },
                0x14 => if shift { 'T' } else { 't' },
                0x15 => if shift { 'Y' } else { 'y' },
                0x16 => if shift { 'U' } else { 'u' },
                0x17 => if shift { 'I' } else { 'i' },
                0x18 => if shift { 'O' } else { 'o' },
                0x19 => if shift { 'P' } else { 'p' },
                0x1A => if shift { '{' } else { '[' },
                0x1B => if shift { '}' } else { ']' },
                0x1E => if shift { 'A' } else { 'a' },
                0x1F => if shift { 'S' } else { 's' },
                0x20 => if shift { 'D' } else { 'd' },
                0x21 => if shift { 'F' } else { 'f' },
                0x22 => if shift { 'G' } else { 'g' },
                0x23 => if shift { 'H' } else { 'h' },
                0x24 => if shift { 'J' } else { 'j' },
                0x25 => if shift { 'K' } else { 'k' },
                0x26 => if shift { 'L' } else { 'l' },
                0x27 => if shift { ':' } else { ';' },
                0x28 => if shift { '"' } else { '\'' },
                0x2C => if shift { 'Z' } else { 'z' },
                0x2D => if shift { 'X' } else { 'x' },
                0x2E => if shift { 'C' } else { 'c' },
                0x2F => if shift { 'V' } else { 'v' },
                0x30 => if shift { 'B' } else { 'b' },
                0x31 => if shift { 'N' } else { 'n' },
                0x32 => if shift { 'M' } else { 'm' },
                0x33 => if shift { '<' } else { ',' },
                0x34 => if shift { '>' } else { '.' },
                0x35 => if shift { '?' } else { '/' },
                0x39 => ' ', // Space
                _ => return None,
            };
            Some(char_map)
        }
        KeyboardLayout::Trq => {
            let char_map = match scancode {
                0x02 => if shift { '!' } else { '1' },
                0x03 => if shift { '\'' } else { '2' },
                0x04 => if shift { '^' } else { '3' },
                0x05 => if shift { '+' } else { '4' },
                0x06 => if shift { '%' } else { '5' },
                0x07 => if shift { '&' } else { '6' },
                0x08 => if shift { '/' } else { '7' },
                0x09 => if shift { '(' } else { '8' },
                0x0A => if shift { ')' } else { '9' },
                0x0B => if shift { '=' } else { '0' },
                0x0C => if shift { '?' } else { '*' },
                0x0D => if shift { '_' } else { '-' },
                0x10 => if shift { 'Q' } else { 'q' },
                0x11 => if shift { 'W' } else { 'w' },
                0x12 => if shift { 'E' } else { 'e' },
                0x13 => if shift { 'R' } else { 'r' },
                0x14 => if shift { 'T' } else { 't' },
                0x15 => if shift { 'Y' } else { 'y' },
                0x16 => if shift { 'U' } else { 'u' },
                // Map Turkish characters to standard ASCII equivalents to render correctly on GOP font
                0x17 => if shift { 'I' } else { 'i' }, // Turkish I/ı -> ASCII I/i
                0x18 => if shift { 'O' } else { 'o' },
                0x19 => if shift { 'P' } else { 'p' },
                0x1A => if shift { 'G' } else { 'g' }, // Turkish Ğ/ğ -> ASCII G/g
                0x1B => if shift { 'U' } else { 'u' }, // Turkish Ü/ü -> ASCII U/u
                0x1E => if shift { 'A' } else { 'a' },
                0x1F => if shift { 'S' } else { 's' },
                0x20 => if shift { 'D' } else { 'd' },
                0x21 => if shift { 'F' } else { 'f' },
                0x22 => if shift { 'G' } else { 'g' },
                0x23 => if shift { 'H' } else { 'h' },
                0x24 => if shift { 'J' } else { 'j' },
                0x25 => if shift { 'K' } else { 'k' },
                0x26 => if shift { 'L' } else { 'l' },
                0x27 => if shift { 'S' } else { 's' }, // Turkish Ş/ş -> ASCII S/s
                0x28 => if shift { 'I' } else { 'i' }, // Turkish İ/i -> ASCII I/i
                0x2B => if shift { ';' } else { ',' }, // Turkish `,` / `;`
                0x2C => if shift { 'Z' } else { 'z' },
                0x2D => if shift { 'X' } else { 'x' },
                0x2E => if shift { 'C' } else { 'c' },
                0x2F => if shift { 'V' } else { 'v' },
                0x30 => if shift { 'B' } else { 'b' },
                0x31 => if shift { 'N' } else { 'n' },
                0x32 => if shift { 'M' } else { 'm' },
                0x33 => if shift { 'O' } else { 'o' }, // Turkish Ö/ö -> ASCII O/o
                0x34 => if shift { 'C' } else { 'c' }, // Turkish Ç/ç -> ASCII C/c
                0x35 => if shift { ':' } else { '.' }, // Turkish `.` / `:`
                0x39 => ' ', // Space
                _ => return None,
            };
            Some(char_map)
        }
    }
}

pub fn poll_keyboard() -> Option<KeyboardInput> {
    unsafe {
        // First check the asynchronous interrupt buffer in case interrupts are active
        if let Some(input) = crate::interrupts::KEYBOARD_BUFFER.take() {
            return Some(input);
        }

        let mut status_port: Port<u8> = Port::new(0x64);
        if status_port.read() & 1 != 0 {
            let mut data_port: Port<u8> = Port::new(0x60);
            let scancode = data_port.read();
            x86_64::instructions::interrupts::without_interrupts(|| {
                crate::interrupts::KEYBOARD_STATE.handle_scancode(scancode)
            })
        } else {
            None
        }
    }
}

/// Cooperative serial receiver.
pub fn poll_serial() -> Option<KeyboardInput> {
    unsafe {
        let mut lsr: Port<u8> = Port::new(0x3F8 + 5);
        if lsr.read() & 1 != 0 {
            let mut data: Port<u8> = Port::new(0x3F8);
            let byte = data.read();
            match byte {
                b'\r' | b'\n' => Some(KeyboardInput::Enter),
                0x08 | 0x7F => Some(KeyboardInput::Backspace),
                0x20..=0x7E => Some(KeyboardInput::Char(byte as char)),
                _ => None,
            }
        } else {
            None
        }
    }
}
