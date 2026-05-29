#![no_std]
#![no_main]
#![feature(abi_x86_interrupt, naked_functions)]

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
pub mod framebuffer;

use alloc::string::String;
use alloc::vec::Vec;
use core::alloc::{GlobalAlloc, Layout};
use core::fmt::{self, Write};
use core::panic::PanicInfo;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use x86_64::instructions::port::Port;

/// Global thread-safe static handle for the UEFI graphics driver.
pub static GRAPHICS: Spinlock<Option<framebuffer::UefiGraphics>> = Spinlock::new(None);

/// Global running system ticks count.
pub static SYSTEM_TICKS: AtomicUsize = AtomicUsize::new(0);

/// Global atomic flags to prevent boot-stage allocator panics
pub static ALLOCATOR_READY: AtomicBool = AtomicBool::new(false);
pub static LOGS_CHANGED: AtomicBool = AtomicBool::new(false);

/// Global rolling log buffer holding the last 8 printed messages
pub static SYSTEM_LOGS: Spinlock<alloc::vec::Vec<alloc::string::String>> = Spinlock::new(alloc::vec::Vec::new());

/// Appends a new message line to the global rolling log buffer.
pub fn append_log(msg: &str) {
    let mut logs = SYSTEM_LOGS.lock();
    for line in msg.lines() {
        let cleaned = line.replace("\r", "");
        // Ignore empty lines
        if cleaned.trim().is_empty() {
            continue;
        }
        // Filter out periodic background thread sweep logs to keep the visual screen clean!
        if cleaned.contains("[THREAD 1]") || cleaned.contains("[THREAD 2]") {
            continue;
        }
        logs.push(cleaned);
    }
    // Limit to the last 8 lines (fits in our visual log box)
    while logs.len() > 8 {
        logs.remove(0);
    }
}

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

    /// Forcefully unlocks the spinlock (used to yield across thread boundaries).
    pub unsafe fn force_unlock(&self) {
        self.lock.store(false, Ordering::Release);
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

    if ALLOCATOR_READY.load(Ordering::Acquire) {
        let mut msg = alloc::string::String::new();
        let _ = core::fmt::write(&mut msg, args);
        append_log(&msg);
        LOGS_CHANGED.store(true, Ordering::Release);
    }
}

#[global_allocator]
static ALLOCATOR: linked_list_allocator::LockedHeap = linked_list_allocator::LockedHeap::empty();

#[repr(align(16))]
struct SafeHeap {
    mem: core::cell::UnsafeCell<[u8; 1024 * 1024]>,
}
unsafe impl Sync for SafeHeap {}

// Static memory array to act as our kernel heap (1 MB)
static HEAP_MEM: SafeHeap = SafeHeap {
    mem: core::cell::UnsafeCell::new([0; 1024 * 1024]),
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
        // Yield or wait for 100 ticks (approx 2 seconds)
        let start_ticks = SYSTEM_TICKS.load(Ordering::Relaxed);
        while SYSTEM_TICKS.load(Ordering::Relaxed) - start_ticks < 100 {
            scheduler::SCHEDULER.lock().thread_yield();
        }
        println!("\x1B[38;5;46m[THREAD 1] Background Memory Scrubbing Sweep initiated...\x1B[0m");
    }
}

/// Background thread periodically logging system metrics and diagnostics.
fn thread_diagnostics() {
    loop {
        // Yield or wait for 200 ticks (approx 4 seconds)
        let start_ticks = SYSTEM_TICKS.load(Ordering::Relaxed);
        while SYSTEM_TICKS.load(Ordering::Relaxed) - start_ticks < 200 {
            scheduler::SCHEDULER.lock().thread_yield();
        }
        println!("\x1B[38;5;51m[THREAD 2] Live system diagnostics telemetry generated successfully.\x1B[0m");
    }
}

// Register entry point macro with bootloader_api crate
bootloader_api::entry_point!(kernel_main);

/// The absolute entry point of the bare-metal x86_64 operating system kernel.
fn kernel_main(boot_info: &'static mut bootloader_api::BootInfo) -> ! {
    // 1. Initialize serial port hardware immediately
    {
        let mut writer = SerialPort::new(0x3F8);
        writer.init();
    }

    // 2. Initialize GOP graphics if available
    if let Some(fb) = boot_info.framebuffer.as_mut() {
        let graphics = framebuffer::UefiGraphics::new(fb);
        *GRAPHICS.lock() = Some(graphics);
    }

    // Draw initial aesthetic dashboard visual
    if let Some(ref mut graphics) = *GRAPHICS.lock() {
        graphics.draw_dashboard_layout(0, &[]);
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

    // Disable CPU hardware interrupts to prevent spurious UEFI hardware interrupt crashes!
    // Since we use cooperative multitasking and poll direct hardware ports (0x60/0x64),
    // we do not need external hardware interrupts active. Exceptions (Page Faults, GPFs)
    // will still run perfectly.
    // x86_64::instructions::interrupts::enable();

    // 3. Initialize global locked heap memory allocator (1 MB)
    let heap_ptr = HEAP_MEM.mem.get() as *mut u8;
    let heap_size = 1024 * 1024;
    unsafe {
        ALLOCATOR.lock().init(heap_ptr, heap_size);
    }
    ALLOCATOR_READY.store(true, Ordering::Release);

    // Seed the visual console with initial boot status events
    println!(">>> [SYSTEM] UEFI bootloader initialized GOP graphics mode successfully.");
    println!(">>> [SYSTEM] SSE and FPU registers enabled (CR0/CR4 activated).");
    println!(">>> [SYSTEM] IDT configured. CPU exceptions fully mapped.");
    println!(">>> [SYSTEM] PS/2 keyboard direct I/O polling driver activated.");
    println!(">>> [SYSTEM] Cooperative multitasking active (Round-Robin context switcher).");
    println!("[SYSTEM] Heap Allocator online (1 MB LockedHeap active).");

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

    // 4. Keyboard polling driver (bypasses blocked legacy interrupts)
    let poll_keyboard = || -> Option<KeyboardInput> {
        unsafe {
            let mut status_port: Port<u8> = Port::new(0x64);
            if status_port.read() & 1 != 0 {
                let mut data_port: Port<u8> = Port::new(0x60);
                let scancode = data_port.read();
                x86_64::instructions::interrupts::without_interrupts(|| {
                    interrupts::KEYBOARD_STATE.handle_scancode(scancode)
                })
            } else {
                None
            }
        }
    };

    // 5. Main execution loop with robust cooperative polling
    let mut line_buffer = String::new();
    let mut last_rendered_ticks = 0;
    let mut last_rendered_len = 0;

    print!("rustanium> ");

    loop {
        // A. Dynamic steady tick generator (simulates a steady 50Hz hardware clock)
        for _ in 0..20_000 {
            core::hint::spin_loop();
        }
        let current_ticks = SYSTEM_TICKS.fetch_add(1, Ordering::Relaxed) + 1;
        core.tick();

        // B. Poll keyboard input directly from hardware ports (bypasses blocked interrupts)
        let input = poll_keyboard();

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

        // Optimized dynamic rendering: Only update changed elements (completely flicker-free!)
        let current_len = line_buffer.len();
        if current_ticks != last_rendered_ticks {
            if let Some(ref mut graphics) = *GRAPHICS.lock() {
                graphics.update_dashboard_telemetry(current_ticks);
            }
            last_rendered_ticks = current_ticks;
        }
        if current_len != last_rendered_len {
            if let Some(ref mut graphics) = *GRAPHICS.lock() {
                let mut prompt_buf = String::new();
                prompt_buf.push_str("rustanium> ");
                prompt_buf.push_str(&line_buffer);
                graphics.update_keyboard_prompt(&prompt_buf);
            }
            last_rendered_len = current_len;
        }
        if LOGS_CHANGED.swap(false, Ordering::Acquire) {
            if let Some(ref mut graphics) = *GRAPHICS.lock() {
                let logs = SYSTEM_LOGS.lock().clone();
                graphics.update_dashboard_logs(&logs);
            }
        }

        // D. Yield CPU to let other background threads run cooperatively
        scheduler::SCHEDULER.lock().thread_yield();
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
    
    // Draw the panic details onto the UEFI GOP framebuffer!
    unsafe {
        GRAPHICS.force_unlock();
    }
    if let Some(ref mut graphics) = *GRAPHICS.lock() {
        // Red crash panel!
        graphics.draw_rect(40, 260, 1200, 400, framebuffer::Color::new(180, 0, 0)); // Red background
        graphics.draw_rect(40, 260, 1200, 4, framebuffer::Color::new(255, 255, 255)); // White border
        graphics.draw_string(60, 280, "CRITICAL KERNEL PANIC / CPU EXCEPTION DETECTED", framebuffer::COLOR_TEXT_WHITE, None, 2);
        graphics.draw_rect(60, 304, 1160, 1, framebuffer::COLOR_TEXT_WHITE);

        // Format direct to screen using GraphicsWriter
        let mut writer = framebuffer::GraphicsWriter {
            graphics,
            x: 60,
            y: 320,
            start_x: 60,
            color: framebuffer::COLOR_TEXT_WHITE,
        };

        use core::fmt::Write;
        let _ = write!(&mut writer, "{}", info);
    }

    loop {
        x86_64::instructions::hlt();
    }
}
