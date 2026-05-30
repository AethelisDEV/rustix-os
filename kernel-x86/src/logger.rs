//! # Bare-Metal System Logging and Telemetry Output Module
//!
//! Provides global thread-safe buffers and custom formatting writers to manage
//! scrollback console history (`TTY_LOGS`) and serial COM1 communication lines.
//! Implements active diagnostics print suppression.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::{self, Write};
use core::sync::atomic::{AtomicBool, Ordering};
use crate::Spinlock;

/// Global scrollback offset for full-screen TTY view.
pub static TTY_SCROLL_OFFSET: Spinlock<usize> = Spinlock::new(0);

/// Global full-screen TTY logs buffer (up to 250 lines).
pub static TTY_LOGS: Spinlock<Vec<String>> = Spinlock::new(Vec::new());

/// Global flag signifying that the TTY console needs a re-render.
pub static TTY_LOGS_CHANGED: AtomicBool = AtomicBool::new(false);

/// Global atomic flag to prevent boot-stage allocator panics.
pub static ALLOCATOR_READY: AtomicBool = AtomicBool::new(false);

/// Retained for potential future use; dashboard now uses static panel.
pub static LOGS_CHANGED: AtomicBool = AtomicBool::new(false);

/// Driver for a raw physical 16550 UART Serial Port mapped to a specific port address.
pub struct SerialPort {
    port_num: u16,
}

impl SerialPort {
    /// Creates a new SerialPort.
    pub const fn new(port_num: u16) -> Self {
        Self { port_num }
    }

    /// Initializes the serial controller hardware for 38400 baud, 8 bits, no parity, 1 stop bit.
    pub fn init(&mut self) {
        use x86_64::instructions::port::Port;
        unsafe {
            // Disable all interrupts
            Port::new(self.port_num + 1).write(0x00u8);
            // Enable DLAB (set baud rate divisor)
            Port::new(self.port_num + 3).write(0x80u8);
            // Set divisor to 3 (lo byte) 38400 baud
            Port::new(self.port_num).write(0x03u8);
            // Divisor (hi byte)
            Port::new(self.port_num + 1).write(0x00u8);
            // 8 bits, no parity, one stop bit
            Port::new(self.port_num + 3).write(0x03u8);
            // Enable FIFO, clear them, with 14-byte threshold
            Port::new(self.port_num + 2).write(0xC7u8);
            // IRQs enabled, RTS/DSR set
            Port::new(self.port_num + 4).write(0x0Bu8);
        }
    }

    /// Writes a character byte directly to the serial transmission line.
    pub fn write_byte(&mut self, b: u8) {
        use x86_64::instructions::port::Port;
        unsafe {
            let mut data_port: Port<u8> = Port::new(self.port_num);
            data_port.write(b);
        }
    }
}

impl Write for SerialPort {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for b in s.bytes() {
            if b == b'\n' {
                self.write_byte(b'\r');
            }
            self.write_byte(b);
        }
        Ok(())
    }
}

/// Global thread-safe static writer bound to COM1 serial port (0x3F8).
pub static SERIAL_WRITER: Spinlock<SerialPort> = Spinlock::new(SerialPort::new(0x3F8));

/// Custom print! macro utilizing the COM1 serial writer.
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::logger::_print(format_args!($($arg)*)));
}

/// Custom println! macro utilizing the COM1 serial writer.
#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

/// Appends a new message line to the TTY scrollback log buffer.
pub fn append_log(msg: &str) {
    let mut tty_logs = TTY_LOGS.lock();
    let mut appended = false;
    for line in msg.lines() {
        let cleaned = line.replace("\r", "");
        // Ignore empty lines
        if cleaned.trim().is_empty() {
            continue;
        }
        tty_logs.push(cleaned);
        appended = true;
    }
    // Limit TTY scrollback logs to 250 lines
    while tty_logs.len() > 250 {
        tty_logs.remove(0);
    }
    // Only signal a TTY redraw when a line was actually added
    if appended {
        TTY_LOGS_CHANGED.store(true, Ordering::Release);
    }
}

/// Dynamic writer routing print! arguments to COM1 serial and TTY buffers.
/// Filters background telemetry thread logs from serial transmission for terminal clarity.
#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    if ALLOCATOR_READY.load(Ordering::Acquire) {
        let mut msg = String::new();
        let _ = core::fmt::write(&mut msg, args);

        // Filter out thread scrubbing sweeps and telemetries from serial terminal output
        if msg.contains("[THREAD 1]") || msg.contains("[THREAD 2]") {
            return;
        }

        let mut writer = SERIAL_WRITER.lock();
        let _ = writer.write_str(&msg);
        append_log(&msg);
    } else {
        let mut writer = SERIAL_WRITER.lock();
        let _ = writer.write_fmt(args);
    }
}
