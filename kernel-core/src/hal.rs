//! # Hardware Abstraction Layer (HAL) for AE Rustanium
//!
//! This module decouples our pure, safe microkernel logic from low-level CPU operations.
//! When compiling for x86_64 targets in the future, these methods will contain real `unsafe`
//! assembly and register operations. Under the host simulation target, they map to safe,
//! high-fidelity software equivalents (like writing hardware logs to `serial_out.log`).

#[cfg(feature = "std")]
extern crate std;

#[cfg(not(feature = "std"))]
core::arch::global_asm!(include_str!("hal.s"));

#[cfg(not(feature = "std"))]
extern "C" {
    /// Assembly helper defined in hal.s to write a byte to COM1 serial port (0x3F8).
    fn hal_write_byte(b: u8);
    /// Assembly helper to read a 32-bit value from PCI config space.
    fn pci_read_config_dword(address: u32) -> u32;
    /// Assembly helper to write a 32-bit value to PCI config space.
    fn pci_write_config_dword(address: u32, value: u32);
}

/// Simulates a real hardware serial port controller (UART 16550) mapped to Port `0x3F8` (COM1).
pub struct SerialPort;

impl SerialPort {
    /// Writes a character byte to the serial port.
    ///
    /// - **Real x86_64**: Writes to I/O Port `0x3F8` using external assembly `hal_write_byte`.
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
                hal_write_byte(b);
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

/// Represents a discovered PCI device.
#[derive(Debug, Clone, Copy)]
pub struct PciDevice {
    pub bus: u8,
    pub slot: u8,
    pub func: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub class_code: u8,
    pub subclass: u8,
    pub bar0: u32,
}

/// Interface for PCI configuration space access and bus scanning.
pub struct PciBus;

impl PciBus {
    /// Reads a 32-bit dword from PCI config space.
    pub fn read(bus: u8, slot: u8, func: u8, offset: u8) -> u32 {
        #[cfg(feature = "std")]
        {
            let _ = (bus, slot, func, offset);
            0xFFFF_FFFF
        }
        #[cfg(not(feature = "std"))]
        {
            let address = (1u32 << 31)
                | ((bus as u32) << 16)
                | ((slot as u32) << 11)
                | ((func as u32) << 8)
                | ((offset as u32) & 0xFC);
            unsafe {
                pci_read_config_dword(address)
            }
        }
    }

    /// Writes a 32-bit dword to PCI config space.
    pub fn write(bus: u8, slot: u8, func: u8, offset: u8, value: u32) {
        #[cfg(feature = "std")]
        {
            let _ = (bus, slot, func, offset, value);
        }
        #[cfg(not(feature = "std"))]
        {
            let address = (1u32 << 31)
                | ((bus as u32) << 16)
                | ((slot as u32) << 11)
                | ((func as u32) << 8)
                | ((offset as u32) & 0xFC);
            unsafe {
                pci_write_config_dword(address, value);
            }
        }
    }

    /// Scans the entire PCI/PCIe topology for active devices.
    pub fn scan_devices() -> alloc::vec::Vec<PciDevice> {
        let mut devices = alloc::vec::Vec::new();

        #[cfg(feature = "std")]
        {
            // Under simulation, mock an Nvidia GPU
            devices.push(PciDevice {
                bus: 0,
                slot: 2,
                func: 0,
                vendor_id: 0x10DE, // Nvidia
                device_id: 0x2484, // RTX 3070 Ti (GA104)
                class_code: 0x03,  // Display Controller
                subclass: 0x00,    // VGA compatible
                bar0: 0xFD00_0000,
            });
            return devices;
        }

        #[cfg(not(feature = "std"))]
        {
            for bus in 0..=255 {
                for slot in 0..32 {
                    let val = Self::read(bus, slot, 0, 0);
                    let vendor_id = (val & 0xFFFF) as u16;
                    if vendor_id == 0xFFFF || vendor_id == 0x0000 {
                        continue;
                    }

                    let header_type_val = Self::read(bus, slot, 0, 0x0C);
                    let is_multi_function = (header_type_val & 0x0080_0000) != 0;
                    let func_count = if is_multi_function { 8 } else { 1 };

                    for func in 0..func_count {
                        let f_val = Self::read(bus, slot, func, 0);
                        let f_vendor = (f_val & 0xFFFF) as u16;
                        if f_vendor == 0xFFFF || f_vendor == 0x0000 {
                            continue;
                        }
                        let device_id = (f_val >> 16) as u16;

                        let class_val = Self::read(bus, slot, func, 0x08);
                        let class_code = ((class_val >> 24) & 0xFF) as u8;
                        let subclass = ((class_val >> 16) & 0xFF) as u8;

                        let bar0 = Self::read(bus, slot, func, 0x10);

                        devices.push(PciDevice {
                            bus,
                            slot,
                            func,
                            vendor_id: f_vendor,
                            device_id,
                            class_code,
                            subclass,
                            bar0,
                        });
                    }
                }
            }
            devices
        }
    }
}
