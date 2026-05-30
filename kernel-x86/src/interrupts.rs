//! # Interrupts Management for AE Rustanium
//!
//! This module configures the low-level x86-64 interrupt architecture:
//! 1. Manages the dual 8259 Chained PICs (Programmable Interrupt Controllers) using direct I/O ports.
//! 2. Registers hardware interrupt vectors inside the `InterruptDescriptorTable` (IDT).
//! 3. Maps and handles PIT (Programmable Interval Timer) ticking at 100 Hz.
//! 4. Handles asynchronous PS/2 Keyboard interrupts, translating scancodes on the fly.
//!
//! Written adhering to safe encapsulation boundaries and strict modularity.

use x86_64::instructions::port::Port;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};
use crate::println;

/// The interrupt vector offset for the primary (master) PIC interrupts (IRQ 0-7 mapped to vectors 32-39).
pub const PIC_1_OFFSET: u8 = 32;

/// The interrupt vector offset for the secondary (slave) PIC interrupts (IRQ 8-15 mapped to vectors 40-47).
pub const PIC_2_OFFSET: u8 = 40;

/// Hardware Interrupt Vector indices mapped to PIC IRQ offsets.
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InterruptIndex {
    /// System Timer (PIT IRQ 0) mapped to vector offset 32.
    Timer = PIC_1_OFFSET,
    /// PS/2 Keyboard (IRQ 1) mapped to vector offset 33.
    Keyboard = PIC_1_OFFSET + 1,
}

impl InterruptIndex {
    /// Returns the raw u8 vector value.
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    /// Returns the raw usize vector value for array indexing.
    pub fn as_usize(self) -> usize {
        self as usize
    }
}

/// Helper representing a single 8259 PIC hardware controller bound to command and data ports.
struct Pic {
    command: Port<u8>,
    data: Port<u8>,
}

/// A structure representing the chained master-slave 8259 PIC controllers.
pub struct ChainedPics {
    pics: [Pic; 2],
}

impl ChainedPics {
    /// Creates a new ChainedPics manager mapped to standard PIC I/O ports.
    pub const fn new(offset1: u8, offset2: u8) -> Self {
        let _ = (offset1, offset2);
        Self {
            pics: [
                Pic {
                    command: Port::new(0x20),
                    data: Port::new(0x21),
                },
                Pic {
                    command: Port::new(0xA0),
                    data: Port::new(0xA1),
                },
            ],
        }
    }

    /// Initializes both PIC controllers by writing Initialization Control Words (ICWs) to ports.
    ///
    /// # Safety
    /// This function is unsafe because it performs raw I/O port writes, which can interfere
    /// with host or hardware state if configured incorrectly.
    pub unsafe fn initialize(&mut self) {
        // Save existing interrupt masks
        let mask1 = self.pics[0].data.read();
        let mask2 = self.pics[1].data.read();

        // ICW1: Start initialization in cascade mode
        self.pics[0].command.write(0x11);
        self.pics[1].command.write(0x11);

        // ICW2: Vector offsets
        self.pics[0].data.write(PIC_1_OFFSET);
        self.pics[1].data.write(PIC_2_OFFSET);

        // ICW3: Tell Master PIC that Slave PIC is at IRQ2 (0000_0100b), and Slave PIC its cascade identity (2)
        self.pics[0].data.write(4);
        self.pics[1].data.write(2);

        // ICW4: Use 8086 mode
        self.pics[0].data.write(0x01);
        self.pics[1].data.write(0x01);

        // Restore masks
        self.pics[0].data.write(mask1);
        self.pics[1].data.write(mask2);
    }

    /// Enables only a specific IRQ line (clears the mask bit in OCW1).
    ///
    /// # Safety
    /// This function performs direct port reads/writes to enable hardware lines.
    pub unsafe fn enable_irq(&mut self, irq: u8) {
        if irq < 8 {
            let mask = self.pics[0].data.read();
            self.pics[0].data.write(mask & !(1 << irq));
        } else if irq < 16 {
            let mask = self.pics[1].data.read();
            self.pics[1].data.write(mask & !(1 << (irq - 8)));
        }
    }

    /// Sends an End of Interrupt (EOI) command to PIC controllers.
    /// Must be called at the end of every hardware interrupt handler.
    ///
    /// # Safety
    /// Direct raw port write.
    pub unsafe fn notify_end_of_interrupt(&mut self, interrupt_id: u8) {
        if interrupt_id >= PIC_2_OFFSET && interrupt_id < PIC_2_OFFSET + 8 {
            self.pics[1].command.write(0x20); // EOI to Slave
        }
        self.pics[0].command.write(0x20); // EOI to Master
    }
}

/// Global chained PICs controller instance.
pub static mut PICS: ChainedPics = ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET);

/// Global Interrupt Descriptor Table instance.
pub static mut IDT: InterruptDescriptorTable = InterruptDescriptorTable::new();

/// Initializes the IDT and binds all exception and hardware interrupt handlers.
pub fn init_idt() {
    unsafe {
        // Exception handlers
        IDT.breakpoint.set_handler_fn(breakpoint_handler);
        IDT.double_fault.set_handler_fn(double_fault_handler);
        IDT.page_fault.set_handler_fn(page_fault_handler);
        IDT.general_protection_fault.set_handler_fn(general_protection_fault_handler);
        IDT.invalid_opcode.set_handler_fn(invalid_opcode_handler);
        IDT.stack_segment_fault.set_handler_fn(stack_segment_fault_handler);
        IDT.divide_error.set_handler_fn(divide_by_zero_handler);

        // Hardware Interrupt Handlers mapped to PIC offsets
        IDT[InterruptIndex::Timer.as_usize()].set_handler_fn(timer_interrupt_handler);
        IDT[InterruptIndex::Keyboard.as_usize()].set_handler_fn(keyboard_interrupt_handler);

        IDT.load();
    }
}

extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    println!("\x1B[38;5;220m[CPU EXCEPTION] Breakpoint Interrupt:\x1B[0m");
    println!("{:#?}", stack_frame);
}

extern "x86-interrupt" fn double_fault_handler(stack_frame: InterruptStackFrame, error_code: u64) -> ! {
    panic!("[CPU CATASTROPHIC] Double Fault Exception (error code: {:#X}):\n{:#?}", error_code, stack_frame);
}

extern "x86-interrupt" fn page_fault_handler(stack_frame: InterruptStackFrame, error_code: PageFaultErrorCode) {
    use x86_64::registers::control::Cr2;
    panic!(
        "[CPU EXCEPTION] Page Fault accessing address: {:#X}\nError Code: {:?}\n{:#?}",
        Cr2::read().as_u64(),
        error_code,
        stack_frame
    );
}

extern "x86-interrupt" fn general_protection_fault_handler(stack_frame: InterruptStackFrame, error_code: u64) {
    panic!(
        "[CPU EXCEPTION] General Protection Fault (error code: {:#X}):\n{:#?}",
        error_code, stack_frame
    );
}

extern "x86-interrupt" fn invalid_opcode_handler(stack_frame: InterruptStackFrame) {
    panic!(
        "[CPU EXCEPTION] Invalid Opcode:\n{:#?}",
        stack_frame
    );
}

extern "x86-interrupt" fn stack_segment_fault_handler(stack_frame: InterruptStackFrame, error_code: u64) {
    panic!(
        "[CPU EXCEPTION] Stack Segment Fault (error code: {:#X}):\n{:#?}",
        error_code, stack_frame
    );
}

extern "x86-interrupt" fn divide_by_zero_handler(stack_frame: InterruptStackFrame) {
    panic!(
        "[CPU EXCEPTION] Divide by Zero:\n{:#?}",
        stack_frame
    );
}

/// Global ticks accumulator populated asynchronously by PIT timer interrupts.
pub static TIMER_TICKS: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

extern "x86-interrupt" fn timer_interrupt_handler(_stack_frame: InterruptStackFrame) {
    TIMER_TICKS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    unsafe {
        PICS.notify_end_of_interrupt(InterruptIndex::Timer.as_u8());
    }
}

/// Static buffer holding keyboard inputs received asynchronously.
pub static mut KEYBOARD_BUFFER: Option<crate::keyboard::KeyboardInput> = None;
/// Static manager tracking shifting states for keyboard decoding.
pub static mut KEYBOARD_STATE: crate::keyboard::KeyboardState = crate::keyboard::KeyboardState::new();

extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame) {
    unsafe {
        let mut data_port: Port<u8> = Port::new(0x60);
        let scancode = data_port.read();
        if let Some(input) = KEYBOARD_STATE.handle_scancode(scancode) {
            KEYBOARD_BUFFER = Some(input);
        }
        PICS.notify_end_of_interrupt(InterruptIndex::Keyboard.as_u8());
    }
}

/// Configures the Programmable Interval Timer (PIT) channel 0 to tick at 100 Hz.
///
/// Divides the base PIT crystal frequency (1193182 Hz) by 100 to get a precise tick rate.
pub fn init_pit() {
    unsafe {
        let mut command_port: Port<u8> = Port::new(0x43);
        let mut data_port: Port<u8> = Port::new(0x40);

        // Command: Channel 0, lobyte/hibyte, operating mode 2 (rate generator), binary mode.
        command_port.write(0x34);

        let divisor: u16 = (1193182u32 / 100) as u16;
        data_port.write((divisor & 0xFF) as u8); // Low byte
        data_port.write((divisor >> 8) as u8);   // High byte
    }
}
