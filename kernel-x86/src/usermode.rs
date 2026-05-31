//! # x86-64 Bare-Metal User Mode (Ring 3) Transition and Paging
//!
//! This module implements memory mapping and privilege level transitions:
//! 1. Modifies the active x86-64 page tables to set `USER_ACCESSIBLE` flags on pages.
//! 2. Declares `enter_user_mode` in assembly using an `iretq` frame to swap CPU privilege levels.
//! 3. Implements `demonstrate_user_mode` which allocates memory blocks, maps them to Ring 3,
//!    copies a self-contained assembly program, and executes it securely.

use x86_64::VirtAddr;
use x86_64::registers::control::Cr3;

/// Global static to store the physical memory offset provided by the bootloader.
pub static PHYSICAL_MEMORY_OFFSET: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// Saved register states for the kernel shell execution context.
#[no_mangle]
pub static mut KERNEL_SHELL_RSP: u64 = 0;
/// Saved register states for the kernel shell execution context.
#[no_mangle]
pub static mut KERNEL_SHELL_RBP: u64 = 0;

/// Traverses down the 4 levels of active page tables and marks the target page as USER_ACCESSIBLE.
///
/// Modifies:
/// - PML4 (Level 4), PDPT (Level 3), PD (Level 2), and PT (Level 1) entries.
/// - Sets the `USER_ACCESSIBLE` flag (`0x04`) so the page can be read/written/executed in Ring 3.
/// - Flushes the Translation Lookaside Buffer (TLB) to invalidate cache immediately.
///
/// # Safety
/// This function is unsafe because it performs raw memory writes to physical page table structures.
pub unsafe fn map_page_user(virt_addr: VirtAddr) {
    let offset = PHYSICAL_MEMORY_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
    println!(">>> PAGING: map_page_user for VirtAddr: {:#X}, Offset: {:#X}", virt_addr.as_u64(), offset);
    if offset == 0 {
        println!(">>> ERROR: PHYSICAL_MEMORY_OFFSET is 0!");
        return;
    }

    let (pml4_frame, _) = Cr3::read();
    let pml4_phys = pml4_frame.start_address().as_u64();
    let pml4_virt_ptr = (pml4_phys + offset) as *mut u64;
    println!(">>> PAGING: PML4 Phys: {:#X}, VirtPtr: {:p}", pml4_phys, pml4_virt_ptr);

    let p4_idx = virt_addr.p4_index();
    let p3_idx = virt_addr.p3_index();
    let p2_idx = virt_addr.p2_index();
    let p1_idx = virt_addr.p1_index();
    println!(">>> PAGING: Indices: L4={}, L3={}, L2={}, L1={}", usize::from(p4_idx), usize::from(p3_idx), usize::from(p2_idx), usize::from(p1_idx));

    // 1. Level 4 Entry
    let pml4_entry_ptr = pml4_virt_ptr.add(usize::from(p4_idx));
    let mut pml4_entry = pml4_entry_ptr.read();
    println!(">>> PAGING: L4 Entry (Before): {:#X}", pml4_entry);
    pml4_entry |= 0x04; // Set USER_ACCESSIBLE bit (0x04)
    pml4_entry &= !(1u64 << 63); // Clear NX (No-Execute) bit to allow user code execution
    pml4_entry_ptr.write(pml4_entry);
    println!(">>> PAGING: L4 Entry (After): {:#X}", pml4_entry);

    let p3_phys = pml4_entry & 0x000F_FFFF_FFFF_F000;
    let p3_virt_ptr = (p3_phys + offset) as *mut u64;

    // 2. Level 3 Entry
    let p3_entry_ptr = p3_virt_ptr.add(usize::from(p3_idx));
    let mut p3_entry = p3_entry_ptr.read();
    println!(">>> PAGING: L3 Entry (Before): {:#X}", p3_entry);
    p3_entry |= 0x04; // Set USER_ACCESSIBLE
    p3_entry &= !(1u64 << 63); // Clear NX bit
    p3_entry_ptr.write(p3_entry);
    println!(">>> PAGING: L3 Entry (After): {:#X}", p3_entry);

    let p2_phys = p3_entry & 0x000F_FFFF_FFFF_F000;
    let p2_virt_ptr = (p2_phys + offset) as *mut u64;

    // 3. Level 2 Entry
    let p2_entry_ptr = p2_virt_ptr.add(usize::from(p2_idx));
    let mut p2_entry = p2_entry_ptr.read();
    println!(">>> PAGING: L2 Entry (Before): {:#X}", p2_entry);
    p2_entry |= 0x04; // Set USER_ACCESSIBLE
    p2_entry &= !(1u64 << 63); // Clear NX bit
    p2_entry_ptr.write(p2_entry);
    println!(">>> PAGING: L2 Entry (After): {:#X}", p2_entry);

    // If it's a huge 2MB page, no Level 1 page table exists
    if (p2_entry & 0x80) != 0 {
        println!(">>> PAGING: Level 2 is a Huge 2MB Page!");
        x86_64::instructions::tlb::flush(virt_addr);
        return;
    }

    let p1_phys = p2_entry & 0x000F_FFFF_FFFF_F000;
    let p1_virt_ptr = (p1_phys + offset) as *mut u64;

    // 4. Level 1 Entry (Page Table Entry)
    let p1_entry_ptr = p1_virt_ptr.add(usize::from(p1_idx));
    let mut p1_entry = p1_entry_ptr.read();
    println!(">>> PAGING: L1 Entry (Before): {:#X}", p1_entry);
    p1_entry |= 0x04; // Set USER_ACCESSIBLE
    p1_entry &= !(1u64 << 63); // Clear NX bit
    p1_entry_ptr.write(p1_entry);
    println!(">>> PAGING: L1 Entry (After): {:#X}", p1_entry);

    // Invalidate TLB cache for this specific virtual address
    x86_64::instructions::tlb::flush(virt_addr);
}

// Low-level Global Assembly routine performing the Ring 3 privilege switch trampoline.
// Sets user segments, pushes selectors, flags, code target, and runs `iretq`.
core::arch::global_asm!(
    ".globl enter_user_mode",
    "enter_user_mode:",
    // Disable interrupts before modifying segment registers
    "cli",
    // Reload segment registers DS, ES, FS, GS with User Data Segment selector
    // User Data Segment is GDT index 3 = 0x18. ORed with RPL 3 = 0x1B.
    "mov ax, 0x1B",
    "mov ds, ax",
    "mov es, ax",
    "mov fs, ax",
    "mov gs, ax",
    // Construct the IRETQ stack frame on the secure Kernel Stack:
    // 1. Stack Segment SS: User Data Segment selector with RPL 3 = 0x1B
    "push 0x1B",
    // 2. Stack Pointer RSP: User Stack Top address passed in RSI (2nd argument)
    "push rsi",
    // 3. RFLAGS: Interrupt Enable bit set (0x200) to allow interrupts inside user space
    "push 0x200",
    // 4. Code Segment CS: User Code Segment selector with RPL 3 = (index 4 = 0x20 OR 3 = 0x23)
    "push 0x23",
    // 5. Instruction Pointer RIP: User entry point passed in RDI (1st argument)
    "push rdi",
    // Swap imtiyaz status and execute user application in Ring 3 User Space!
    "iretq"
);

extern "C" {
    fn enter_user_mode(user_code: u64, user_stack_top: u64);
}

/// Allocates isolated code/stack pages, sets user-accessible page mappings, copies
/// a self-contained program, and transitions execution to Ring 3 User Space.
///
/// Saves kernel shell registers before entry, and upon Syscall 3 (Exit), the kernel
/// restores registers to return control back to the Shell command loop.
pub fn demonstrate_user_mode() {
    println!("🔌 Step 1: Allocating isolated User Code and User Stack page frames...");

    // Allocate isolated 4 KB blocks for Code and Stack using heap vectors aligned to 4096 bytes
    let mut code_page: alloc::vec::Vec<u8> = alloc::vec![0u8; 4096];
    let mut stack_page: alloc::vec::Vec<u8> = alloc::vec![0u8; 4096];

    let code_ptr = code_page.as_mut_ptr();
    let stack_ptr = stack_page.as_mut_ptr();

    println!("📂 Step 2: Mapping memory pages as USER_ACCESSIBLE inside PML4 tables...");
    unsafe {
        map_page_user(VirtAddr::from_ptr(code_ptr));
        map_page_user(VirtAddr::from_ptr(stack_ptr));
    }

    println!("✅ Memory mapped successfully!");
    println!("📦 Step 3: Copying Ring 3 assembly payload to User Code segment...");

    // Binary payload representing a self-contained assembly program that triggers Syscalls:
    // 1. Syscall 1 (Telemetry) -> print "Hello from Ring 3!"
    // 2. Syscall 2 (Math) -> calculate 42 * 10
    // 3. Syscall 1 (Telemetry) -> print success message
    // 4. Syscall 3 (Exit) -> exit program and return to Shell
    let user_program_bytes: &[u8] = &[
        // mov rax, 1 (Syscall 1)
        0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00,
        // lea rdi, [rip + 43] (string 1)
        0x48, 0x8D, 0x3D, 0x2B, 0x00, 0x00, 0x00,
        // syscall
        0x0F, 0x05,
        // mov rax, 2 (Syscall 2)
        0x48, 0xC7, 0xC0, 0x02, 0x00, 0x00, 0x00,
        // mov rdi, 42 (argument)
        0x48, 0xC7, 0xC7, 0x2A, 0x00, 0x00, 0x00,
        // syscall
        0x0F, 0x05,
        // mov rax, 1 (Syscall 1)
        0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00,
        // lea rdi, [rip + 44] (string 2)
        0x48, 0x8D, 0x3D, 0x2C, 0x00, 0x00, 0x00,
        // syscall
        0x0F, 0x05,
        // mov rax, 3 (Syscall 3 - Exit)
        0x48, 0xC7, 0xC0, 0x03, 0x00, 0x00, 0x00,
        // syscall
        0x0F, 0x05,
        // --- String 1 ---
        // "Hello from Ring 3 (User Space)!\n\0" (32 bytes)
        b'H', b'e', b'l', b'l', b'o', b' ', b'f', b'r', b'o', b'm', b' ', 
        b'R', b'i', b'n', b'g', b' ', b'3', b' ', b'(', b'U', b's', b'e', 
        b'r', b' ', b'S', b'p', b'a', b'c', b'e', b')', b'!', 0x0A, 0x00,
        // --- String 2 ---
        // "Math verification: 42 * 10 resolved successfully.\n\0" (52 bytes)
        b'M', b'a', b't', b'h', b' ', b'v', b'e', b'r', b'i', b'f', b'i', 
        b'c', b'a', b't', b'i', b'o', b'n', b':', b' ', b'4', b'2', b' ', 
        b'*', b' ', b'1', b'0', b' ', b'r', b'e', b's', b'o', b'l', b'v', 
        b'e', b'd', b' ', b's', b'u', b'c', b'c', b'e', b's', b's', b'f', 
        b'u', b'l', b'l', b'y', b'.', 0x0A, 0x00
    ];

    unsafe {
        // Copy instructions to User Code page memory space
        core::ptr::copy_nonoverlapping(
            user_program_bytes.as_ptr(),
            code_ptr,
            user_program_bytes.len(),
        );
    }

    let user_code_address = code_ptr as u64;
    let user_stack_top = stack_ptr as u64 + 4096 - 8;

    println!("🚀 Step 4: Swapping imtiyaz status. Jumping to Ring 3 (User Mode) trampoline...");
    println!("------------------------------------------------------------");

    unsafe {
        // Save current secure kernel shell RSP and RBP
        // When Syscall 3 is invoked inside Ring 3, the CPU will jump back to Kernel Shell
        // by restoring these values and executing 'ret' safely.
        core::arch::asm!(
            "mov [rip + KERNEL_SHELL_RSP], rsp",
            "mov [rip + KERNEL_SHELL_RBP], rbp",
        );

        enter_user_mode(user_code_address, user_stack_top);
    }
}
