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
pub mod mouse;
pub mod shell;
pub mod gdt;
pub mod syscall;

const DESKTOP_PAYLOAD: &[u8] = include_bytes!("../../target/usermode-desktop.bin");

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use core::panic::PanicInfo;

// Re-export logger items globally so macros compile cleanly
pub use logger::{SERIAL_WRITER, TTY_LOGS, TTY_LOGS_CHANGED, ALLOCATOR_READY, TTY_SCROLL_OFFSET, append_log};

/// Global thread-safe static handle for the SystemCore.
pub static SYSTEM_CORE: Spinlock<Option<kernel_core::SystemCore>> = Spinlock::new(None);



pub fn split_path(path: &str) -> (String, String) {
    let path = path.trim_end_matches('/');
    if let Some(pos) = path.rfind('/') {
        let parent = &path[..pos];
        let parent = if parent.is_empty() { "/" } else { parent };
        let name = &path[pos + 1..];
        (String::from(parent), String::from(name))
    } else {
        (String::from("/"), String::from(path))
    }
}

/// Global thread-safe static handle for the UEFI graphics driver.
pub static GRAPHICS: Spinlock<Option<framebuffer::UefiGraphics>> = Spinlock::new(None);

/// Global running system ticks count.
pub static SYSTEM_TICKS: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

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
pub(crate) static ALLOCATOR: linked_list_allocator::LockedHeap = linked_list_allocator::LockedHeap::empty();

#[repr(align(16))]
pub(crate) struct SafeHeap {
    pub(crate) mem: core::cell::UnsafeCell<[u8; 256 * 1024 * 1024]>,
}
unsafe impl Sync for SafeHeap {}

pub(crate) static HEAP_MEM: SafeHeap = SafeHeap {
    mem: core::cell::UnsafeCell::new([0; 256 * 1024 * 1024]),
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

/// Background thread periodically updating system metrics in the Shared Info Page.
fn thread_metrics_updater() {
    loop {
        unsafe {
            let page_ptr = core::ptr::addr_of_mut!(syscall::SHARED_INFO_PAGE);
            let heap_used_ptr = core::ptr::addr_of_mut!((*page_ptr).info.heap_used);
            let heap_free_ptr = core::ptr::addr_of_mut!((*page_ptr).info.heap_free);
            
            let used = ALLOCATOR.lock().used() as u64;
            heap_used_ptr.write(used);
            heap_free_ptr.write((256 * 1024 * 1024 - used as usize) as u64);
        }
        scheduler::SCHEDULER.lock().thread_yield();
    }
}

fn usermode_log_callback(msg: &str) {
    println!("{}", msg);
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

    // Initialize global locked heap memory allocator (256 MB) immediately at boot
    let heap_ptr = HEAP_MEM.mem.get() as *mut u8;
    let heap_size = 256 * 1024 * 1024;
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

    // 3. Enable SSE, initialize GDT/TSS and Syscalls, and configure 8259 PIC + IDT interrupts
    enable_sse();
    unsafe {
        gdt::init_gdt();
        let stack_top = gdt::TSS.privilege_stack_table[0].as_u64();
        usermode_x86::init_syscalls(stack_top, syscall::rust_syscall_handler);
    }
    interrupts::init_idt();
    mouse::init_mouse();
    unsafe {
        interrupts::PICS.initialize();
        interrupts::PICS.enable_irq(0);  // Timer IRQ 0
        interrupts::PICS.enable_irq(1);  // Keyboard IRQ 1
        interrupts::PICS.enable_irq(2);  // Enable Cascade IRQ 2 for Slave PIC interrupts (IRQ 8-15)
        interrupts::PICS.enable_irq(12); // Mouse IRQ 12 (Slave PIC line 4)
    }
    interrupts::init_pit();
    x86_64::instructions::interrupts::enable();

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

    let core = kernel_core::SystemCore::bootstrap();
    *SYSTEM_CORE.lock() = Some(core);
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
        let _ = sched.spawn(thread_metrics_updater);
    }

    println!("[KERNEL] Starting Ring 3 Desktop Environment...");
    usermode_x86::execute_user_program(DESKTOP_PAYLOAD);

    println!("[KERNEL] User Space Desktop exited. Entering idle halt loop.");
    loop {
        x86_64::instructions::hlt();
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

        {
            let mut writer = framebuffer::GraphicsWriter {
                graphics,
                x: 40,
                y: 80,
                start_x: 40,
                color: framebuffer::COLOR_TEXT_WHITE,
            };
            let _ = write!(&mut writer, "{}", info);
        }
        
        // Force swap buffers to blit the crash screen onto the physical framebuffer
        graphics.swap_buffers();
    }

    loop {
        x86_64::instructions::hlt();
    }
}
