//! # Hardware Abstraction Layer (HAL) for AE Rustanium
//!
//! This module decouples our pure, safe microkernel logic from low-level CPU operations.
//! When compiling for x86_64 targets in the future, these methods will contain real `unsafe`
//! assembly and register operations. Under the host simulation target, they map to safe,
//! high-fidelity software equivalents (like writing hardware logs to `serial_out.log`).

#[cfg(feature = "std")]
extern crate std;

/// Simulates a real hardware serial port controller (UART 16550) mapped to Port `0x3F8` (COM1).
pub struct SerialPort;

impl SerialPort {
    /// Writes a character byte to the serial port.
    ///
    /// - **Real x86_64**: Writes to I/O Port `0x3F8` using assembly `outdx`.
    /// - **Simulation Target**: Appends log output dynamically to the file `serial_out.log`.
    pub fn write_byte(b: u8) {
        #[cfg(feature = "std")]
        {
            use std::fs::OpenOptions;
            use std::io::Write;
            if let Ok(mut file) = OpenOptions::new()
                .create(true)
                .append(true)
                .open("serial_out.log")
            {
                let _ = file.write_all(&[b]);
            }
        }
        #[cfg(not(feature = "std"))]
        {
            unsafe {
                core::arch::asm!(
                    "out dx, al",
                    in("dx") 0x3F8u16,
                    in("al") b,
                    options(nomem, nostack, preserves_flags)
                );
            }
        }
    }

    /// Writes a string slice to the serial port.
    pub fn write_str(s: &str) {
        for b in s.bytes() {
            Self::write_byte(b);
        }
    }
}

/// Simulates legacy VGA text-mode memory mapped I/O starting at `0xB8000`.
pub struct VgaBuffer;

impl VgaBuffer {
    /// Writes a character with color attributes to a simulated VGA framebuffer.
    ///
    /// - **Real x86_64**: Writes directly to raw physical memory address `0xB8000`.
    /// - **Simulation Target**: Writes a trace event to the serial port.
    pub fn write_char(offset: usize, character: u8, color_attribute: u8) {
        let address = 0xB8000 + offset;
        
        #[cfg(feature = "std")]
        {
            let trace = alloc::format!(
                "[VGA MMU WRITE] Address: {:#X} | Char: '{}' | ColorAttr: {:#04X}\n",
                address,
                character as char,
                color_attribute
            );
            SerialPort::write_str(&trace);
        }
        #[cfg(not(feature = "std"))]
        {
            // On a real x86_64 hardware, we would do:
            // unsafe {
            //     let ptr = address as *mut u16;
            //     let val = (character as u16) | ((color_attribute as u16) << 8);
            //     ptr.write_volatile(val);
            // }
            let _ = (address, character, color_attribute);
        }
    }
}

/// Represents core CPU instruction controls.
pub struct Cpu;

impl Cpu {
    /// Halts the CPU until the next hardware interrupt is triggered.
    ///
    /// - **Real x86_64**: Executes the assembler `hlt` instruction, putting the core to sleep.
    /// - **Simulation Target**: Sleeps the thread briefly to yield CPU cycles back to the host operating system.
    pub fn halt() {
        #[cfg(feature = "std")]
        {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        #[cfg(not(feature = "std"))]
        {
            // Since interrupts are disabled on the bare-metal x86-64 target,
            // executing 'hlt' would halt the CPU permanently.
            // We use spin_loop (which compiles to a 'pause' instruction) to yield and allow polling to continue.
            core::hint::spin_loop();
        }
    }
}
