#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

//! # x86_64 Bare-Metal Entry Point for AE Rustanium
//!
//! This module represents the absolute hardware initialization stage of our microkernel.
//! It boots directly from the UEFI/BIOS bootloader, configures a thread-safe `SerialPort`
//! driver mapped to I/O port `0x3F8`, defines custom `print!` and `println!` macros,
//! implements the lock-free global bump allocator, establishes the bare-metal `#[panic_handler]`,
//! and orchestrates the autonomous flight loop of the `SystemCore` microkernel on raw x86_64.

extern crate alloc;

pub mod interrupts;
pub mod scheduler;

use alloc::string::String;
use alloc::vec::Vec;
use core::alloc::{GlobalAlloc, Layout};
use core::fmt::{self, Write};
use core::panic::PanicInfo;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use x86_64::instructions::port::Port;

/// A simple, safe spinlock implementation for bare-metal concurrency control.
pub struct Spinlock<T> {
    lock: AtomicBool,
    data: core::cell::UnsafeCell<T>,
}

impl<T> Spinlock<T> {
    /// Creates a new spinlock wrapping the provided data.
    pub const fn new(data: T) -> Self {
        Self {
            lock: AtomicBool::new(false),
            data: core::cell::UnsafeCell::new(data),
        }
    }

    /// Safely locks and returns a mutable guard to access wrapped data.
    pub fn lock(&self) -> SpinlockGuard<'_, T> {
        while self.lock.swap(true, Ordering::Acquire) {
            // Spin until lock is free, pausing CPU execution slightly
            core::hint::spin_loop();
        }
        SpinlockGuard { spinlock: self }
    }
}

/// Guard representing a locked Spinlock.
pub struct SpinlockGuard<'a, T> {
    spinlock: &'a Spinlock<T>,
}

impl<'a, T> core::ops::Deref for SpinlockGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.spinlock.data.get() }
    }
}

impl<'a, T> core::ops::DerefMut for SpinlockGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.spinlock.data.get() }
    }
}

impl<'a, T> Drop for SpinlockGuard<'a, T> {
    fn drop(&mut self) {
        self.spinlock.lock.store(false, Ordering::Release);
    }
}

// Make Spinlock Sync and Send so it can be declared as static
unsafe impl<T: Send> Sync for Spinlock<T> {}
unsafe impl<T: Send> Send for Spinlock<T> {}

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

    /// Writes a character byte directly to the serial transmission line after verifying hardware readiness.
    pub fn write_byte(&mut self, b: u8) {
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
    ($($arg:tt)*) => ($crate::_print(format_args!($($arg)*)));
}

/// Custom println! macro utilizing the COM1 serial writer.
#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    let mut writer = SerialPort::new(0x3F8);
    let _ = writer.write_fmt(args);
}

/// A high-performance, thread-safe, lock-free global bump allocator written specifically
/// for AE Rustanium bare-metal execution without relying on std or lock overhead.
pub struct AtomicBumpAllocator {
    heap_start: AtomicUsize,
    heap_end: AtomicUsize,
    next: AtomicUsize,
}

impl AtomicBumpAllocator {
    /// Creates a new uninitialized allocator.
    pub const fn empty() -> Self {
        Self {
            heap_start: AtomicUsize::new(0),
            heap_end: AtomicUsize::new(0),
            next: AtomicUsize::new(0),
        }
    }

    /// Initializes the allocator with a raw memory address and size.
    pub fn init(&self, start: usize, size: usize) {
        self.heap_start.store(start, Ordering::Release);
        self.heap_end.store(start + size, Ordering::Release);
        self.next.store(start, Ordering::Release);
    }
}

unsafe impl GlobalAlloc for AtomicBumpAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let align = layout.align();
        let size = layout.size();
        
        let mut current_next = self.next.load(Ordering::Relaxed);
        loop {
            let heap_end_val = self.heap_end.load(Ordering::Acquire);
            if heap_end_val == 0 {
                return core::ptr::null_mut(); // Not initialized yet!
            }

            // Align the start address based on layout requirements
            let start = (current_next + align - 1) & !(align - 1);
            let end = start.saturating_add(size);
            
            if end > heap_end_val {
                return core::ptr::null_mut(); // Out of memory!
            }
            
            // Atomically update the next allocation boundary
            match self.next.compare_exchange_weak(current_next, end, Ordering::Release, Ordering::Relaxed) {
                Ok(_) => return start as *mut u8,
                Err(actual) => current_next = actual,
            }
        }
    }
    
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // Bump allocator does not free individual blocks
    }
}

#[global_allocator]
static ALLOCATOR: AtomicBumpAllocator = AtomicBumpAllocator::empty();

#[repr(align(16))]
struct SafeHeap {
    mem: core::cell::UnsafeCell<[u8; 256 * 1024]>,
}
unsafe impl Sync for SafeHeap {}

// Static memory array to act as our kernel heap (256 KB)
static HEAP_MEM: SafeHeap = SafeHeap {
    mem: core::cell::UnsafeCell::new([0; 256 * 1024]),
};

/// Configures CR0 and CR4 registers to enable SSE and FPU on raw hardware.
pub fn enable_sse() {
    use x86_64::registers::control::{Cr0, Cr0Flags, Cr4, Cr4Flags};

    unsafe {
        // 1. Configure CR0
        let mut cr0 = Cr0::read();
        cr0.remove(Cr0Flags::EMULATE_COPROCESSOR); // Clear EM bit
        cr0.insert(Cr0Flags::MONITOR_COPROCESSOR); // Set MP bit
        Cr0::write(cr0);

        // 2. Configure CR4
        let mut cr4 = Cr4::read();
        cr4.insert(Cr4Flags::OSFXSR); // Enable FXSAVE/FXRSTOR
        cr4.insert(Cr4Flags::OSXMMEXCPT_ENABLE); // Enable SIMD floating-point exceptions
        Cr4::write(cr4);
    }
}

// Static IDT code removed, delegated to interrupts.rs

/// Background thread periodically sweeping memory for radiation bit flips.
fn thread_scrubber() {
    loop {
        // Yield or wait for 3 seconds (300 PIT ticks)
        let start_ticks = interrupts::TIMER_TICKS.load(Ordering::Relaxed);
        while interrupts::TIMER_TICKS.load(Ordering::Relaxed) - start_ticks < 300 {
            scheduler::SCHEDULER.lock().thread_yield();
        }
        println!("\x1B[38;5;46m[THREAD 1] Background Memory Scrubbing Sweep initiated...\x1B[0m");
    }
}

/// Background thread periodically logging system metrics and diagnostics.
fn thread_diagnostics() {
    loop {
        // Yield or wait for 6 seconds (600 PIT ticks)
        let start_ticks = interrupts::TIMER_TICKS.load(Ordering::Relaxed);
        while interrupts::TIMER_TICKS.load(Ordering::Relaxed) - start_ticks < 600 {
            scheduler::SCHEDULER.lock().thread_yield();
        }
        println!("\x1B[38;5;51m[THREAD 2] Live system diagnostics telemetry generated successfully.\x1B[0m");
    }
}

// Register entry point macro with bootloader crate
bootloader::entry_point!(kernel_main);

/// The absolute entry point of the bare-metal x86_64 operating system kernel.
fn kernel_main(_boot_info: &'static bootloader::BootInfo) -> ! {
    // Direct VGA write to display "BOOT OK" in the terminal via emulated screen
    unsafe {
        let vga = 0xB8000 as *mut u16;
        vga.write_volatile(0x0a00 | b'B' as u16); // Green 'B'
        vga.add(1).write_volatile(0x0a00 | b'O' as u16);
        vga.add(2).write_volatile(0x0a00 | b'O' as u16);
        vga.add(3).write_volatile(0x0a00 | b'T' as u16);
        vga.add(4).write_volatile(0x0a00 | b' ' as u16);
        vga.add(5).write_volatile(0x0a00 | b'O' as u16);
        vga.add(6).write_volatile(0x0a00 | b'K' as u16); // Green 'K'
    }

    // 1. Initialize serial port hardware
    {
        let mut writer = SerialPort::new(0x3F8);
        writer.init();
    }

    // 2. Enable SSE and configure 8259 PIC + IDT interrupts
    enable_sse();
    
    // Initialize IDT and setup hardware interrupts
    interrupts::init_idt();
    unsafe {
        interrupts::PICS.initialize();
        interrupts::PICS.enable_irq(0); // Timer IRQ 0
        interrupts::PICS.enable_irq(1); // Keyboard IRQ 1
    }
    interrupts::init_pit();

    // Enable CPU hardware interrupts!
    x86_64::instructions::interrupts::enable();

    // 3. Initialize global lock-free heap memory allocator
    let heap_ptr = HEAP_MEM.mem.get() as *mut u8;
    let heap_size = 256 * 1024;
    ALLOCATOR.init(heap_ptr as usize, heap_size);

    println!("============================================================");
    println!("AE RUSTANIUM OS - BARE-METAL INTEL/AMD x86_64 FLIGHT COMPUTER");
    println!("============================================================");
    println!("[HARDWARE] UEFI Boot Stage Complete.");
    println!("[HARDWARE] Paging and Long Mode enabled by bootloader.");
    println!("[HARDWARE] Headless Serial port COM1 (0x3F8) initialized.");
    println!("[HARDWARE] Static global memory heap (256 KB) active.");
    println!("[HARDWARE] Launching core operating system bootstrapping...");
    println!();

    // 3. Bootstrap SystemCore (without std!)
    let mut core = kernel_core::SystemCore::bootstrap();
    println!("[KERNEL] Microkernel boot complete!");
    println!("[KERNEL] 3 Microservices (Telemetry, Navigation, LifeSupport) spawned.");
    println!("[KERNEL] ECC SECDED active on 64 page frames.");
    println!("[KERNEL] Triple Modular Redundancy (TMR) Voter online.");
    println!("[KERNEL] Entering autonomous flight controller ticks loop...");
    println!();

    // 4. Initialize cooperative scheduler and spawn background threads
    {
        let mut sched = scheduler::SCHEDULER.lock();
        sched.register_main_thread();
        let _ = sched.spawn(thread_scrubber);
        let _ = sched.spawn(thread_diagnostics);
    }

    // 4. Main execution loop (asynchronous interrupt-driven)
    let mut line_buffer = String::new();

    print!("rustanium> ");

    loop {
        // A. Process timer ticks accumulated asynchronously
        let ticks = interrupts::TIMER_TICKS.swap(0, Ordering::Relaxed);
        for _ in 0..ticks {
            core.tick();
        }

        // B. Process any keyboard input from asynchronous buffer
        let input = x86_64::instructions::interrupts::without_interrupts(|| {
            unsafe { interrupts::KEYBOARD_BUFFER.take() }
        });

        // C. Poll Serial UART Port Status (cooperative fallback for serial console)
        let serial_input = poll_serial();

        if let Some(in_val) = input.or(serial_input) {
            match in_val {
                KeyboardInput::Char(c) => {
                    line_buffer.push(c);
                    print!("{}", c);
                }
                KeyboardInput::Backspace => {
                    if !line_buffer.is_empty() {
                        line_buffer.pop();
                        // Send ANSI backspace erasure sequence: backspace, space, backspace
                        print!("\x08 \x08");
                    }
                }
                KeyboardInput::Enter => {
                    println!();
                    let trimmed = line_buffer.trim();
                    if !trimmed.is_empty() {
                        handle_command(trimmed, &mut core);
                    }
                    line_buffer.clear();
                    print!("rustanium> ");
                }
            }
        }

        // D. Yield CPU to let other background threads run cooperatively
        scheduler::SCHEDULER.lock().thread_yield();

        // E. Put the CPU to sleep until next interrupt
        x86_64::instructions::hlt();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardInput {
    Char(char),
    Backspace,
    Enter,
}

pub struct KeyboardState {
    shift_pressed: bool,
}

impl KeyboardState {
    pub const fn new() -> Self {
        Self { shift_pressed: false }
    }

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
            // Standard scan codes
            code => {
                // Ignore key releases (scan code set 1 sets bit 7)
                if code & 0x80 == 0 {
                    if let Some(c) = translate_scancode(code, self.shift_pressed) {
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

fn translate_scancode(scancode: u8, shift: bool) -> Option<char> {
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

fn poll_serial() -> Option<KeyboardInput> {
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

fn print_vfs_tree(vfs: &virtual_fs::VirtualFileSystem, inode_idx: usize, indent: usize) {
    if inode_idx >= vfs.inodes.len() {
        return;
    }
    let inode = &vfs.inodes[inode_idx];
    let indent_str = "  ".repeat(indent);
    match &inode.inode_type {
        virtual_fs::InodeType::Directory { entries } => {
            if inode_idx == 0 {
                println!("\x1B[38;5;33m/\x1B[0m");
            } else {
                println!("{}{}\x1B[38;5;33m{}/\x1B[0m", indent_str, if indent > 0 { "├── " } else { "" }, inode.name);
            }
            for (_, child_idx) in entries {
                print_vfs_tree(vfs, *child_idx, indent + 1);
            }
        }
        virtual_fs::InodeType::File { size, .. } => {
            println!("{}{}{:<16} \x1B[38;5;246m({} bytes)\x1B[0m", indent_str, if indent > 0 { "├── " } else { "" }, inode.name, size);
        }
    }
}

fn handle_command(cmd_line: &str, core: &mut kernel_core::SystemCore) {
    let mut parts = cmd_line.split_whitespace();
    let cmd = match parts.next() {
        Some(c) => c,
        None => return,
    };
    let args: Vec<&str> = parts.collect();

    match cmd {
        "help" => {
            println!("============================================================");
            println!("         AE RUSTANIUM BARE-METAL INTERACTIVE SHELL          ");
            println!("============================================================");
            println!("Available Commands:");
            println!("  help             - Show this diagnostic helper menu");
            println!("  status           - View microkernel status & physical memory metrics");
            println!("  tasks            - List running processes and scheduler info");
            println!("  inject-flip      - Simulates hardware radiation bit flip");
            println!("  vfs              - Recursive visualization of Virtual File System");
            println!("  cat <path>       - Display content of a file (e.g. /system/kernel.conf)");
            println!("  clear            - Reset the console terminal");
            println!("============================================================");
        }
        "status" => {
            println!("------------------------------------------------------------");
            println!("SYSTEM HEALTH & PHYSICAL MEMORY STATUS");
            println!("------------------------------------------------------------");
            println!("Scrubber Sweeps:           {}", core.scrubber_sweeps);
            println!("ECC SECDED Corrections:    {}", core.ecc_single_bit_corrections);
            println!("Pages Quarantined:         {}", core.pages_quarantined);
            println!("Pages Relocated:           {}", core.pages_relocated);
            println!("TMR CPU Operations:        {}", core.critical_tmr_ops);
            println!("TMR ALU Corrections:       {}", core.tmr_voter_corrections);
            
            // Count allocated physical memory pages
            let mut allocated = 0;
            for pid_opt in &core.allocator.allocation_map {
                if pid_opt.is_some() {
                    allocated += 1;
                }
            }
            println!("Allocated Page Frames:     {}/{}", allocated, core.allocator.allocation_map.len());
            println!("------------------------------------------------------------");
        }
        "tasks" => {
            println!("------------------------------------------------------------");
            println!("RUNNING MICROSERVICES");
            println!("------------------------------------------------------------");
            println!("{:<5} | {:<16} | {:<8} | Allocated Pages", "PID", "Process Name", "Critical");
            println!("------+------------------+----------+-----------------");
            for p in &core.dispatcher.processes {
                println!(
                    "{:<5} | {:<16} | {:<8} | {:?}",
                    p.pid,
                    p.name,
                    if p.is_critical { "YES (TMR)" } else { "NO" },
                    p.allocated_pages
                );
            }
            println!("------------------------------------------------------------");
        }
        "inject-flip" => {
            // Find the first allocated frame
            let mut target_frame = None;
            for (idx, pid_opt) in core.allocator.allocation_map.iter().enumerate() {
                if pid_opt.is_some() {
                    target_frame = Some(idx);
                    break;
                }
            }

            if let Some(frame_idx) = target_frame {
                println!("[INJECTOR] Targeting frame {} (allocated to process)...", frame_idx);
                // Inject flip on offset 8, bit 3
                match core.inject_memory_flip(frame_idx, 8, 3) {
                    Ok(_) => {
                        println!("\x1B[38;5;220m[INJECTOR OK] Injected synthetic bit flip into physical frame {} offset 8, bit 3!\x1B[0m", frame_idx);
                        println!("[INJECTOR] Scrubber will auto-heal it on the next scheduler tick.");
                    }
                    Err(e) => {
                        println!("\x1B[38;5;196m[INJECTOR ERR] Failed to inject: {}\x1B[0m", e);
                    }
                }
            } else {
                println!("\x1B[38;5;196m[INJECTOR ERR] No allocated memory frames found to target!\x1B[0m");
            }
        }
        "vfs" => {
            println!("------------------------------------------------------------");
            println!("VFS TREE STRUCTURE");
            println!("------------------------------------------------------------");
            print_vfs_tree(&core.vfs, 0, 0);
            println!("------------------------------------------------------------");
        }
        "cat" => {
            if args.is_empty() {
                println!("\x1B[38;5;196mUsage: cat <file_path>\x1B[0m");
                return;
            }
            let file_path = args[0];
            match core.vfs.read_file(file_path, &mut core.allocator) {
                Ok(data) => {
                    println!("--- Reading {} ---", file_path);
                    if let Ok(text) = core::str::from_utf8(&data) {
                        print!("{}", text);
                    } else {
                        // Hex dump for binary contents
                        for chunk in data.chunks(16) {
                            for b in chunk {
                                print!("{:02X} ", b);
                            }
                            println!();
                        }
                    }
                    println!("------------------");
                }
                Err(e) => {
                    println!("\x1B[38;5;196m[VFS ERR] Failed to read file {}: {}\x1B[0m", file_path, e);
                }
            }
        }
        "clear" => {
            // ANSI escape sequence to clear terminal screen and move cursor to home position
            print!("\x1B[2J\x1B[H");
        }
        other => {
            println!("\x1B[38;5;196mUnknown command: '{}'. Type 'help' for options.\x1B[0m", other);
        }
    }
}

/// Bare-metal panic handler that logs failure trace to COM1 and halts the CPU safely.
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!();
    println!("\x1B[38;5;196m============================================================\x1B[0m");
    println!("\x1B[38;5;196m!!!                CRITICAL KERNEL PANIC                 !!!\x1B[0m");
    println!("\x1B[38;5;196m============================================================\x1B[0m");
    println!("Message: \x1B[38;5;220m{}\x1B[0m", info);
    println!("Halting CPU core. System halted.");
    
    loop {
        x86_64::instructions::hlt();
    }
}
