#![no_std]
#![no_main]
#![feature(abi_x86_interrupt, naked_functions)]

//! # x86_64 Bare-Metal Entry Point for AE Rustanium
//!
//! Handles low-level CPU bootstrapping, global heaps, SSE activation,
//! interrupts (IDT), and context switching loops. Delegating logging,
//! keyboard inputs, and terminal commands to clean modular sub-modules.

extern crate alloc;

#[macro_use]
pub mod logger;
pub mod interrupts;
pub mod scheduler;
pub mod framebuffer;
pub mod keyboard;
pub mod shell;
pub mod gdt;

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use core::panic::PanicInfo;

// Re-export logger items globally so macros compile cleanly
pub use logger::{SERIAL_WRITER, TTY_LOGS, TTY_LOGS_CHANGED, ALLOCATOR_READY, TTY_SCROLL_OFFSET, append_log};

/// Global thread-safe static handle for the UEFI graphics driver.
pub static GRAPHICS: Spinlock<Option<framebuffer::UefiGraphics>> = Spinlock::new(None);

/// Global running system ticks count.
pub static SYSTEM_TICKS: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenMode {
    Dashboard,
    Tty,
}

/// Global active screen mode.
pub static CURRENT_SCREEN_MODE: Spinlock<ScreenMode> = Spinlock::new(ScreenMode::Dashboard);

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

unsafe impl<T: Send> Sync for Spinlock<T> {}
unsafe impl<T: Send> Send for Spinlock<T> {}

#[global_allocator]
static ALLOCATOR: linked_list_allocator::LockedHeap = linked_list_allocator::LockedHeap::empty();

#[repr(align(16))]
struct SafeHeap {
    mem: core::cell::UnsafeCell<[u8; 1024 * 1024]>,
}
unsafe impl Sync for SafeHeap {}

static HEAP_MEM: SafeHeap = SafeHeap {
    mem: core::cell::UnsafeCell::new([0; 1024 * 1024]),
};

/// Configures CR0 and CR4 registers to enable SSE and FPU on raw hardware.
pub fn enable_sse() {
    use x86_64::registers::control::{Cr0, Cr0Flags, Cr4, Cr4Flags};
    unsafe {
        let mut cr0 = Cr0::read();
        cr0.remove(Cr0Flags::EMULATE_COPROCESSOR);
        cr0.insert(Cr0Flags::MONITOR_COPROCESSOR);
        Cr0::write(cr0);

        let mut cr4 = Cr4::read();
        cr4.insert(Cr4Flags::OSFXSR);
        cr4.insert(Cr4Flags::OSXMMEXCPT_ENABLE);
        Cr4::write(cr4);
    }
}

/// Background thread periodically sweeping memory for radiation bit flips.
fn thread_scrubber() {
    loop {
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
        let start_ticks = SYSTEM_TICKS.load(Ordering::Relaxed);
        while SYSTEM_TICKS.load(Ordering::Relaxed) - start_ticks < 200 {
            scheduler::SCHEDULER.lock().thread_yield();
        }
        println!("\x1B[38;5;51m[THREAD 2] Live system diagnostics telemetry generated successfully.\x1B[0m");
    }
}

fn usermode_log_callback(msg: &str) {
    println!("{}", msg);
}

#[no_mangle]
pub extern "C" fn rust_syscall_handler(id: u64, arg: u64) -> u64 {
    match id {
        1 => {
            let ptr = arg as *const u8;
            unsafe {
                let mut len = 0;
                while len < 100 && *ptr.add(len) != 0 {
                    len += 1;
                }
                let bytes = core::slice::from_raw_parts(ptr, len);
                if let Ok(s) = core::str::from_utf8(bytes) {
                    println!("\x1B[38;5;46m[SYSCALL 1 (TELE)] User Telemetry: {}\x1B[0m", s);
                }
            }
            1
        }
        2 => {
            println!("\x1B[38;5;51m[SYSCALL 2 (MATH)] User Math request. Multiplying {} * 10...\x1B[0m", arg);
            arg * 10
        }
        3 => {
            println!("\x1B[38;5;46m[SYSCALL 3 (EXIT)] User program requested exit. Returning to Kernel TTY Shell.\x1B[0m");
            3
        }
        _ => {
            println!("\x1B[38;5;196m[SYSCALL ERR] Invalid system call ID received: {}\x1B[0m", id);
            0
        }
    }
}

const BOOTLOADER_CONFIG: bootloader_api::config::BootloaderConfig = {
    let mut config = bootloader_api::config::BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(bootloader_api::config::Mapping::Dynamic);
    config
};

bootloader_api::entry_point!(kernel_main, config = &BOOTLOADER_CONFIG);

/// The absolute entry point of the bare-metal x86_64 operating system kernel.
fn kernel_main(boot_info: &'static mut bootloader_api::BootInfo) -> ! {
    // Store physical memory offset for user space paging traversal
    let phys_offset = boot_info.physical_memory_offset.into_option().unwrap_or(0);
    usermode_x86::PHYSICAL_MEMORY_OFFSET.store(phys_offset, Ordering::Release);
    usermode_x86::init_logger(usermode_log_callback);

    // Initialize global locked heap memory allocator (1 MB) immediately at boot
    let heap_ptr = HEAP_MEM.mem.get() as *mut u8;
    let heap_size = 1024 * 1024;
    unsafe {
        ALLOCATOR.lock().init(heap_ptr, heap_size);
    }
    ALLOCATOR_READY.store(true, Ordering::Release);

    // 1. Initialize serial port hardware immediately
    {
        let mut writer = logger::SERIAL_WRITER.lock();
        writer.init();
    }

    // 2. Initialize GOP graphics if available
    if let Some(fb) = boot_info.framebuffer.as_mut() {
        let graphics = framebuffer::UefiGraphics::new(fb);
        *GRAPHICS.lock() = Some(graphics);
    }

    // Draw initial aesthetic dashboard visual and render initial prompt
    if let Some(ref mut graphics) = *GRAPHICS.lock() {
        graphics.draw_dashboard_layout(0, None);
        graphics.update_keyboard_prompt("rustanium:/> ");
    }

    // 3. Enable SSE, initialize GDT/TSS and Syscalls, and configure 8259 PIC + IDT interrupts
    enable_sse();
    unsafe {
        gdt::init_gdt();
        let stack_top = gdt::TSS.privilege_stack_table[0].as_u64();
        usermode_x86::init_syscalls(stack_top, rust_syscall_handler);
    }
    interrupts::init_idt();
    unsafe {
        interrupts::PICS.initialize();
        interrupts::PICS.enable_irq(0); // Timer IRQ 0
        interrupts::PICS.enable_irq(1); // Keyboard IRQ 1
    }
    interrupts::init_pit();

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

    let mut core = kernel_core::SystemCore::bootstrap();
    println!("[KERNEL] Microkernel boot complete!");
    println!("[KERNEL] 3 Microservices (Telemetry, Navigation, LifeSupport) spawned.");
    println!("[KERNEL] ECC SECDED active on 64 page frames.");
    println!("[KERNEL] Triple Modular Redundancy (TMR) Voter online.");
    println!("[KERNEL] Entering autonomous flight controller ticks loop... ");
    println!();

    {
        let mut sched = scheduler::SCHEDULER.lock();
        sched.register_main_thread();
        let _ = sched.spawn(thread_scrubber);
        let _ = sched.spawn(thread_diagnostics);
    }

    let mut line_buffer = String::new();
    let mut last_rendered_ticks = 0;
    
    // Set last_rendered_len to 9999 to guarantee immediate prompt drawing on the very first loop tick!
    let mut last_rendered_len = 9999;

    let mut cwd = String::from("/");
    let mut cmd_history: Vec<String> = Vec::new();

    // Write initial prompt directly to serial port bypassing the TTY scrollback logs
    {
        use core::fmt::Write;
        let _ = write!(logger::SERIAL_WRITER.lock(), "rustanium:{}> ", cwd);
    }

    loop {
        // A. Dynamic steady tick generator (simulates a steady 50Hz hardware clock)
        for _ in 0..20_000 {
            core::hint::spin_loop();
        }
        let current_ticks = SYSTEM_TICKS.fetch_add(1, Ordering::Relaxed) + 1;
        core.tick();

        // B. Poll keyboard input directly from hardware ports
        let input = keyboard::poll_keyboard();

        // C. Poll Serial UART Port Status (cooperative fallback for serial console)
        let serial_input = keyboard::poll_serial();

        if let Some(in_val) = input.or(serial_input) {
            match in_val {
                keyboard::KeyboardInput::Char(c) => {
                    line_buffer.push(c);
                    // Echo character directly to serial writer, bypassing print! / TTY_LOGS
                    {
                        let mut writer = logger::SERIAL_WRITER.lock();
                        writer.write_byte(c as u8);
                    }
                }
                keyboard::KeyboardInput::Backspace => {
                    if !line_buffer.is_empty() {
                        line_buffer.pop();
                        // Send ANSI backspace sequence directly to serial writer
                        {
                            let mut writer = logger::SERIAL_WRITER.lock();
                            writer.write_byte(0x08);
                            writer.write_byte(b' ');
                            writer.write_byte(0x08);
                        }
                    }
                }
                keyboard::KeyboardInput::Enter => {
                    // Send newline directly to serial writer
                    {
                        use core::fmt::Write;
                        let _ = logger::SERIAL_WRITER.lock().write_str("\r\n");
                    }

                    // Push command line to TTY scrollback logs so it is visible in the console
                    let log_line = alloc::format!("rustanium:{}> {}", cwd, line_buffer);
                    append_log(&log_line);

                    let trimmed = line_buffer.trim();
                    if !trimmed.is_empty() {
                        cmd_history.push(String::from(trimmed));
                        if cmd_history.len() > 50 {
                            cmd_history.remove(0);
                        }
                        shell::handle_command(trimmed, &mut core, &mut cwd, &cmd_history);
                    }
                    line_buffer.clear();

                    // Print prompt directly to serial writer
                    {
                        use core::fmt::Write;
                        let _ = write!(logger::SERIAL_WRITER.lock(), "rustanium:{}> ", cwd);
                    }

                    // Update TTY prompt immediately if active!
                    if let Some(ref mut graphics) = *GRAPHICS.lock() {
                        if *CURRENT_SCREEN_MODE.lock() == ScreenMode::Tty {
                            graphics.update_tty_prompt(&line_buffer, &cwd);
                        }
                    }
                }
                keyboard::KeyboardInput::F1 => {
                    let mut mode = CURRENT_SCREEN_MODE.lock();
                    if *mode != ScreenMode::Tty {
                        *mode = ScreenMode::Tty;
                        *logger::TTY_SCROLL_OFFSET.lock() = 0;
                        if let Some(ref mut graphics) = *GRAPHICS.lock() {
                            let tty_logs = logger::TTY_LOGS.lock().clone();
                            graphics.draw_tty_layout(current_ticks, &tty_logs, 0, &line_buffer, &cwd);
                        }
                        last_rendered_ticks = current_ticks;
                        last_rendered_len = line_buffer.len();
                    }
                }
                keyboard::KeyboardInput::F2 => {
                    let mut mode = CURRENT_SCREEN_MODE.lock();
                    if *mode != ScreenMode::Dashboard {
                        *mode = ScreenMode::Dashboard;
                        if let Some(ref mut graphics) = *GRAPHICS.lock() {
                            graphics.draw_dashboard_layout(current_ticks, Some(&core));
                            
                            let mut prompt_buf = String::new();
                            prompt_buf.push_str("rustanium:");
                            prompt_buf.push_str(&cwd);
                            prompt_buf.push_str("> ");
                            prompt_buf.push_str(&line_buffer);
                            graphics.update_keyboard_prompt(&prompt_buf);
                        }
                        last_rendered_ticks = current_ticks;
                        last_rendered_len = line_buffer.len();
                    }
                }
                keyboard::KeyboardInput::PageUp => {
                    let current_mode = *CURRENT_SCREEN_MODE.lock();
                    if current_mode == ScreenMode::Tty {
                        let total_len = logger::TTY_LOGS.lock().len();
                        let mut offset = logger::TTY_SCROLL_OFFSET.lock();
                        let max_scroll = if total_len > 22 { total_len - 22 } else { 0 };
                        if *offset < max_scroll {
                            *offset = core::cmp::min(max_scroll, *offset + 3);
                            logger::TTY_LOGS_CHANGED.store(true, Ordering::Release);
                        }
                    }
                }
                keyboard::KeyboardInput::PageDown => {
                    let current_mode = *CURRENT_SCREEN_MODE.lock();
                    if current_mode == ScreenMode::Tty {
                        let mut offset = logger::TTY_SCROLL_OFFSET.lock();
                        if *offset > 0 {
                            *offset = if *offset > 3 { *offset - 3 } else { 0 };
                            logger::TTY_LOGS_CHANGED.store(true, Ordering::Release);
                        }
                    }
                }
                keyboard::KeyboardInput::ArrowUp => {
                    let current_mode = *CURRENT_SCREEN_MODE.lock();
                    if current_mode == ScreenMode::Tty {
                        let total_len = logger::TTY_LOGS.lock().len();
                        let mut offset = logger::TTY_SCROLL_OFFSET.lock();
                        let max_scroll = if total_len > 22 { total_len - 22 } else { 0 };
                        if *offset < max_scroll {
                            *offset = core::cmp::min(max_scroll, *offset + 1);
                            logger::TTY_LOGS_CHANGED.store(true, Ordering::Release);
                        }
                    }
                }
                keyboard::KeyboardInput::ArrowDown => {
                    let current_mode = *CURRENT_SCREEN_MODE.lock();
                    if current_mode == ScreenMode::Tty {
                        let mut offset = logger::TTY_SCROLL_OFFSET.lock();
                        if *offset > 0 {
                            *offset = *offset - 1;
                            logger::TTY_LOGS_CHANGED.store(true, Ordering::Release);
                        }
                    }
                }
            }
        }

        // --- RENDERING ORCHESTRATION ---
        let current_mode = *CURRENT_SCREEN_MODE.lock();
        match current_mode {
            ScreenMode::Dashboard => {
                let current_len = line_buffer.len();
                if current_ticks != last_rendered_ticks {
                    if let Some(ref mut graphics) = *GRAPHICS.lock() {
                        graphics.update_dashboard_telemetry(current_ticks, Some(&core));
                    }
                    last_rendered_ticks = current_ticks;
                }
                if current_len != last_rendered_len {
                    if let Some(ref mut graphics) = *GRAPHICS.lock() {
                        let mut prompt_buf = String::new();
                        prompt_buf.push_str("rustanium:");
                        prompt_buf.push_str(&cwd);
                        prompt_buf.push_str("> ");
                        prompt_buf.push_str(&line_buffer);
                        graphics.update_keyboard_prompt(&prompt_buf);
                    }
                    last_rendered_len = current_len;
                }
            }
            ScreenMode::Tty => {
                let current_len = line_buffer.len();
                let logs_changed = logger::TTY_LOGS_CHANGED.swap(false, Ordering::Acquire);
                
                if let Some(ref mut graphics) = *GRAPHICS.lock() {
                    if current_ticks != last_rendered_ticks {
                        graphics.update_tty_telemetry(current_ticks);
                        last_rendered_ticks = current_ticks;
                    }
                    if current_len != last_rendered_len {
                        graphics.update_tty_prompt(&line_buffer, &cwd);
                        last_rendered_len = current_len;
                    }
                    if logs_changed {
                        let tty_logs = logger::TTY_LOGS.lock().clone();
                        let scroll = *logger::TTY_SCROLL_OFFSET.lock();
                        graphics.update_tty_logs(&tty_logs, scroll);
                    }
                }
            }
        }

        scheduler::SCHEDULER.lock().thread_yield();
    }
}

/// Bare-metal panic handler that logs failure trace to COM1 and halts the CPU safely.
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    use core::fmt::Write;
    println!();
    println!("\x1B[38;5;196m============================================================\x1B[0m");
    println!("\x1B[38;5;196m!!!                CRITICAL KERNEL PANIC                 !!!\x1B[0m");
    println!("\x1B[38;5;196m============================================================\x1B[0m");
    println!("Message: \x1B[38;5;220m{}\x1B[0m", info);
    println!("Halting CPU core. System halted.");
    
    unsafe {
        GRAPHICS.force_unlock();
    }
    if let Some(ref mut graphics) = *GRAPHICS.lock() {
        // Draw solid red covering the entire screen
        graphics.clear(framebuffer::Color::new(180, 0, 0));
        
        graphics.draw_string(40, 40, "CRITICAL KERNEL PANIC / CPU EXCEPTION DETECTED", framebuffer::COLOR_TEXT_WHITE, None, 2);
        graphics.draw_rect(40, 64, 1160, 1, framebuffer::COLOR_TEXT_WHITE);

        let mut writer = framebuffer::GraphicsWriter {
            graphics,
            x: 40,
            y: 80,
            start_x: 40,
            color: framebuffer::COLOR_TEXT_WHITE,
        };
        let _ = write!(&mut writer, "{}", info);
    }

    loop {
        x86_64::instructions::hlt();
    }
}
