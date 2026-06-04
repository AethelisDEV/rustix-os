//! # x86-64 Bare-Metal User Mode (Ring 3) Transition and Paging
//!
//! This module implements memory mapping and privilege level transitions:
//! 1. Walks active page tables to resolve virtual-to-physical addresses.
//! 2. Creates NEW page table entries mapping arbitrary virtual addresses to physical frames.
//! 3. Loads flat binary user programs at a fixed virtual address (0x400000) matching their
//!    linker script, ensuring all absolute references resolve correctly.
//! 4. Declares `enter_user_mode` in assembly using an `iretq` frame to swap CPU privilege levels.

use x86_64::VirtAddr;
use x86_64::registers::control::Cr3;
use crate::log_info;

/// Fixed virtual address where user-space programs are loaded.
/// Must match the base address in usermode-desktop/linker.ld.
pub const USER_CODE_BASE: u64 = 0x400000;

/// Fixed virtual address for the top of the user stack (grows downward).
/// NOTE: Must be above BSS end (4K wallpaper cache/buffers end at ~0x3400000), so we use 0x8000000 (128 MB).
pub const USER_STACK_TOP: u64 = 0x8000000;

/// Size of user stack in bytes (128 KB).
pub const USER_STACK_SIZE: usize = 128 * 1024;

/// Extra BSS allocation beyond the flat binary (64 MB for BACK_BUFFER and statics).
const USER_BSS_EXTRA: usize = 64 * 1024 * 1024;

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
    if offset == 0 {
        return;
    }

    let (pml4_frame, _) = Cr3::read();
    let pml4_phys = pml4_frame.start_address().as_u64();
    let pml4_virt_ptr = (pml4_phys + offset) as *mut u64;

    let p4_idx = virt_addr.p4_index();
    let p3_idx = virt_addr.p3_index();
    let p2_idx = virt_addr.p2_index();
    let p1_idx = virt_addr.p1_index();

    let mut changed = false;

    // 1. Level 4 Entry
    let pml4_entry_ptr = pml4_virt_ptr.add(usize::from(p4_idx));
    let mut pml4_entry = pml4_entry_ptr.read();
    if (pml4_entry & 0x01) == 0 {
        return;
    }
    if (pml4_entry & 0x04) == 0 || (pml4_entry & (1u64 << 63)) != 0 {
        pml4_entry |= 0x04; // Set USER_ACCESSIBLE
        pml4_entry &= !(1u64 << 63); // Clear NX bit
        pml4_entry_ptr.write(pml4_entry);
        changed = true;
    }

    let p3_phys = pml4_entry & 0x000F_FFFF_FFFF_F000;
    let p3_virt_ptr = (p3_phys + offset) as *mut u64;

    // 2. Level 3 Entry
    let p3_entry_ptr = p3_virt_ptr.add(usize::from(p3_idx));
    let mut p3_entry = p3_entry_ptr.read();
    if (p3_entry & 0x01) == 0 {
        return;
    }
    if (p3_entry & 0x04) == 0 || (p3_entry & (1u64 << 63)) != 0 {
        p3_entry |= 0x04; // Set USER_ACCESSIBLE
        p3_entry &= !(1u64 << 63); // Clear NX bit
        p3_entry_ptr.write(p3_entry);
        changed = true;
    }

    let p2_phys = p3_entry & 0x000F_FFFF_FFFF_F000;
    let p2_virt_ptr = (p2_phys + offset) as *mut u64;

    // 3. Level 2 Entry
    let p2_entry_ptr = p2_virt_ptr.add(usize::from(p2_idx));
    let mut p2_entry = p2_entry_ptr.read();
    if (p2_entry & 0x01) == 0 {
        return;
    }
    if (p2_entry & 0x04) == 0 || (p2_entry & (1u64 << 63)) != 0 {
        p2_entry |= 0x04; // Set USER_ACCESSIBLE
        p2_entry &= !(1u64 << 63); // Clear NX bit
        p2_entry_ptr.write(p2_entry);
        changed = true;
    }

    // If it's a huge 2MB page, no Level 1 page table exists
    if (p2_entry & 0x80) != 0 {
        if changed {
            let (pml4_frame, flags) = Cr3::read();
            Cr3::write(pml4_frame, flags);
        }
        return;
    }

    let p1_phys = p2_entry & 0x000F_FFFF_FFFF_F000;
    let p1_virt_ptr = (p1_phys + offset) as *mut u64;

    // 4. Level 1 Entry (Page Table Entry)
    let p1_entry_ptr = p1_virt_ptr.add(usize::from(p1_idx));
    let mut p1_entry = p1_entry_ptr.read();
    if (p1_entry & 0x01) == 0 {
        return;
    }
    if (p1_entry & 0x04) == 0 || (p1_entry & (1u64 << 63)) != 0 {
        p1_entry |= 0x04; // Set USER_ACCESSIBLE
        p1_entry &= !(1u64 << 63); // Clear NX bit
        p1_entry_ptr.write(p1_entry);
        changed = true;
    }

    if changed {
        // Complete TLB flush by rewriting CR3
        let (pml4_frame, flags) = Cr3::read();
        Cr3::write(pml4_frame, flags);
    }
}

pub unsafe fn map_page_user_readonly(virt_addr: VirtAddr) {
    // 1. Call map_page_user to set the user flag throughout the traversal
    map_page_user(virt_addr);

    let offset = PHYSICAL_MEMORY_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
    if offset == 0 {
        return;
    }

    let (pml4_frame, _) = Cr3::read();
    let pml4_phys = pml4_frame.start_address().as_u64();
    let pml4_virt_ptr = (pml4_phys + offset) as *mut u64;

    let p4_idx = virt_addr.p4_index();
    let p3_idx = virt_addr.p3_index();
    let p2_idx = virt_addr.p2_index();
    let p1_idx = virt_addr.p1_index();

    let pml4_entry = pml4_virt_ptr.add(usize::from(p4_idx)).read();
    let p3_phys = pml4_entry & 0x000F_FFFF_FFFF_F000;
    let p3_virt_ptr = (p3_phys + offset) as *mut u64;

    let p3_entry = p3_virt_ptr.add(usize::from(p3_idx)).read();
    let p2_phys = p3_entry & 0x000F_FFFF_FFFF_F000;
    let p2_virt_ptr = (p2_phys + offset) as *mut u64;

    let p2_entry_ptr = p2_virt_ptr.add(usize::from(p2_idx));
    let mut p2_entry = p2_entry_ptr.read();
    if (p2_entry & 0x80) != 0 {
        // If it's a huge 2MB page, clear write bit in Level 2 entry
        p2_entry &= !0x02; // Clear Read/Write bit
        p2_entry_ptr.write(p2_entry);
        let (pml4_frame, flags) = Cr3::read();
        Cr3::write(pml4_frame, flags);
        return;
    }

    let p1_phys = p2_entry & 0x000F_FFFF_FFFF_F000;
    let p1_virt_ptr = (p1_phys + offset) as *mut u64;

    // 4. Level 1 Entry (Page Table Entry)
    let p1_entry_ptr = p1_virt_ptr.add(usize::from(p1_idx));
    let mut p1_entry = p1_entry_ptr.read();
    p1_entry &= !0x02; // Clear Read/Write bit
    p1_entry_ptr.write(p1_entry);

    // Complete TLB flush by rewriting CR3
    let (pml4_frame, flags) = Cr3::read();
    Cr3::write(pml4_frame, flags);
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

// ============================================================================
// Virtual-to-Physical Address Translation
// ============================================================================

/// Walks the active x86-64 4-level page table hierarchy to resolve a virtual
/// address to its backing physical address. Handles 4KB, 2MB, and 1GB pages.
///
/// # Safety
/// Reads raw page table entries via physical memory offset mapping.
pub unsafe fn virt_to_phys(va: u64) -> Option<u64> {
    let offset = PHYSICAL_MEMORY_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
    if offset == 0 {
        return None;
    }

    let virt_addr = VirtAddr::new(va);
    let (pml4_frame, _) = Cr3::read();
    let pml4_phys = pml4_frame.start_address().as_u64();
    let pml4_virt = (pml4_phys + offset) as *const u64;

    // Level 4 (PML4)
    let pml4_entry = pml4_virt.add(usize::from(virt_addr.p4_index())).read();
    if (pml4_entry & 0x01) == 0 {
        return None; // Not present
    }

    // Level 3 (PDPT)
    let pdpt_phys = pml4_entry & 0x000F_FFFF_FFFF_F000;
    let pdpt_virt = (pdpt_phys + offset) as *const u64;
    let pdpt_entry = pdpt_virt.add(usize::from(virt_addr.p3_index())).read();
    if (pdpt_entry & 0x01) == 0 {
        return None;
    }
    if (pdpt_entry & 0x80) != 0 {
        // 1 GB huge page
        let phys_base = pdpt_entry & 0x000F_FFFF_C000_0000;
        return Some(phys_base | (va & 0x3FFF_FFFF));
    }

    // Level 2 (PD)
    let pd_phys = pdpt_entry & 0x000F_FFFF_FFFF_F000;
    let pd_virt = (pd_phys + offset) as *const u64;
    let pd_entry = pd_virt.add(usize::from(virt_addr.p2_index())).read();
    if (pd_entry & 0x01) == 0 {
        return None;
    }
    if (pd_entry & 0x80) != 0 {
        // 2 MB huge page
        let phys_base = pd_entry & 0x000F_FFFF_FFE0_0000;
        return Some(phys_base | (va & 0x1F_FFFF));
    }

    // Level 1 (PT) - 4 KB page
    let pt_phys = pd_entry & 0x000F_FFFF_FFFF_F000;
    let pt_virt = (pt_phys + offset) as *const u64;
    let pt_entry = pt_virt.add(usize::from(virt_addr.p1_index())).read();
    if (pt_entry & 0x01) == 0 {
        return None;
    }
    let phys_base = pt_entry & 0x000F_FFFF_FFFF_F000;
    Some(phys_base | (va & 0xFFF))
}

// ============================================================================
// Page Table Frame Allocation
// ============================================================================

/// Allocates a zeroed 4KB-aligned page from the kernel heap and returns its
/// physical address. The allocation is intentionally leaked (never freed)
/// because page table frames must persist for the lifetime of the mapping.
unsafe fn alloc_page_table_frame() -> u64 {
    let layout = core::alloc::Layout::from_size_align_unchecked(4096, 4096);
    let ptr = alloc::alloc::alloc_zeroed(layout);
    if ptr.is_null() {
        panic!("alloc_page_table_frame: out of memory");
    }
    let va = ptr as u64;
    match virt_to_phys(va) {
        Some(pa) => pa,
        None => panic!("alloc_page_table_frame: failed to resolve VA {:x} to PA", va),
    }
}

// ============================================================================
// Create User-Space Page Mapping at Arbitrary Virtual Address
// ============================================================================

/// Creates a 4KB page table entry mapping `user_virt` -> `phys_addr` with
/// Present | Writable | User flags. Allocates intermediate page table levels
/// (PDPT, PD, PT) from the kernel heap if they don't yet exist.
///
/// # Safety
/// Manipulates raw page table entries and allocates physical frames.
pub unsafe fn create_user_page_mapping(user_virt: VirtAddr, phys_addr: u64) {
    let offset = PHYSICAL_MEMORY_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
    if offset == 0 {
        return;
    }

    let (pml4_frame, _) = Cr3::read();
    let pml4_phys = pml4_frame.start_address().as_u64();
    let pml4_virt = (pml4_phys + offset) as *mut u64;

    let p4_idx = user_virt.p4_index();
    let p3_idx = user_virt.p3_index();
    let p2_idx = user_virt.p2_index();
    let p1_idx = user_virt.p1_index();

    // ---- Level 4 (PML4) ----
    let pml4_entry_ptr = pml4_virt.add(usize::from(p4_idx));
    let mut pml4_entry = pml4_entry_ptr.read();
    if (pml4_entry & 0x01) == 0 {
        // No entry - allocate a new PDPT page
        let new_frame = alloc_page_table_frame();
        pml4_entry = new_frame | 0x07; // Present | Writable | User
        pml4_entry_ptr.write(pml4_entry);
    } else {
        // Entry exists - ensure User flag is set and NX is clear
        pml4_entry |= 0x04;
        pml4_entry &= !(1u64 << 63);
        pml4_entry_ptr.write(pml4_entry);
    }

    // ---- Level 3 (PDPT) ----
    let pdpt_phys = pml4_entry & 0x000F_FFFF_FFFF_F000;
    let pdpt_virt = (pdpt_phys + offset) as *mut u64;
    let pdpt_entry_ptr = pdpt_virt.add(usize::from(p3_idx));
    let mut pdpt_entry = pdpt_entry_ptr.read();
    if (pdpt_entry & 0x01) == 0 {
        let new_frame = alloc_page_table_frame();
        pdpt_entry = new_frame | 0x07;
        pdpt_entry_ptr.write(pdpt_entry);
    } else if (pdpt_entry & 0x80) != 0 {
        // 1GB huge page - cannot split, allocate fresh PD
        let new_frame = alloc_page_table_frame();
        pdpt_entry = new_frame | 0x07;
        pdpt_entry_ptr.write(pdpt_entry);
    } else {
        pdpt_entry |= 0x04;
        pdpt_entry &= !(1u64 << 63);
        pdpt_entry_ptr.write(pdpt_entry);
    }

    // ---- Level 2 (PD) ----
    let pd_phys = pdpt_entry & 0x000F_FFFF_FFFF_F000;
    let pd_virt = (pd_phys + offset) as *mut u64;
    let pd_entry_ptr = pd_virt.add(usize::from(p2_idx));
    let mut pd_entry = pd_entry_ptr.read();
    if (pd_entry & 0x01) == 0 {
        let new_frame = alloc_page_table_frame();
        pd_entry = new_frame | 0x07;
        pd_entry_ptr.write(pd_entry);
    } else if (pd_entry & 0x80) != 0 {
        // 2MB huge page - allocate a fresh PT to replace it
        let new_frame = alloc_page_table_frame();
        pd_entry = new_frame | 0x07;
        pd_entry_ptr.write(pd_entry);
    } else {
        pd_entry |= 0x04;
        pd_entry &= !(1u64 << 63);
        pd_entry_ptr.write(pd_entry);
    }

    // ---- Level 1 (PT) ---- Set the final 4KB page mapping
    let pt_phys = pd_entry & 0x000F_FFFF_FFFF_F000;
    let pt_virt = (pt_phys + offset) as *mut u64;
    let pt_entry_ptr = pt_virt.add(usize::from(p1_idx));
    // Present (0x01) | Writable (0x02) | User (0x04) = 0x07, NX bit 63 clear
    let pt_entry = (phys_addr & 0x000F_FFFF_FFFF_F000) | 0x07;
    pt_entry_ptr.write(pt_entry);
}

/// Creates a 4KB page table entry mapping `user_virt` -> `phys_addr` with
/// Present | User flags (excluding Writable to make it read-only).
///
/// # Safety
/// Manipulates raw page table entries.
pub unsafe fn create_user_page_mapping_readonly(user_virt: VirtAddr, phys_addr: u64) {
    let offset = PHYSICAL_MEMORY_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
    if offset == 0 {
        return;
    }

    let (pml4_frame, _) = Cr3::read();
    let pml4_phys = pml4_frame.start_address().as_u64();
    let pml4_virt = (pml4_phys + offset) as *mut u64;

    let p4_idx = user_virt.p4_index();
    let p3_idx = user_virt.p3_index();
    let p2_idx = user_virt.p2_index();
    let p1_idx = user_virt.p1_index();

    // ---- Level 4 (PML4) ----
    let pml4_entry_ptr = pml4_virt.add(usize::from(p4_idx));
    let mut pml4_entry = pml4_entry_ptr.read();
    if (pml4_entry & 0x01) == 0 {
        let new_frame = alloc_page_table_frame();
        pml4_entry = new_frame | 0x07; // We can keep PML4 writable/user
        pml4_entry_ptr.write(pml4_entry);
    } else {
        pml4_entry |= 0x04;
        pml4_entry &= !(1u64 << 63);
        pml4_entry_ptr.write(pml4_entry);
    }

    // ---- Level 3 (PDPT) ----
    let pdpt_phys = pml4_entry & 0x000F_FFFF_FFFF_F000;
    let pdpt_virt = (pdpt_phys + offset) as *mut u64;
    let pdpt_entry_ptr = pdpt_virt.add(usize::from(p3_idx));
    let mut pdpt_entry = pdpt_entry_ptr.read();
    if (pdpt_entry & 0x01) == 0 {
        let new_frame = alloc_page_table_frame();
        pdpt_entry = new_frame | 0x07;
        pdpt_entry_ptr.write(pdpt_entry);
    } else {
        pdpt_entry |= 0x04;
        pdpt_entry &= !(1u64 << 63);
        pdpt_entry_ptr.write(pdpt_entry);
    }

    // ---- Level 2 (PD) ----
    let pd_phys = pdpt_entry & 0x000F_FFFF_FFFF_F000;
    let pd_virt = (pd_phys + offset) as *mut u64;
    let pd_entry_ptr = pd_virt.add(usize::from(p2_idx));
    let mut pd_entry = pd_entry_ptr.read();
    if (pd_entry & 0x01) == 0 {
        let new_frame = alloc_page_table_frame();
        pd_entry = new_frame | 0x07;
        pd_entry_ptr.write(pd_entry);
    } else {
        pd_entry |= 0x04;
        pd_entry &= !(1u64 << 63);
        pd_entry_ptr.write(pd_entry);
    }

    // ---- Level 1 (PT) ----
    let pt_phys = pd_entry & 0x000F_FFFF_FFFF_F000;
    let pt_virt = (pt_phys + offset) as *mut u64;
    let pt_entry_ptr = pt_virt.add(usize::from(p1_idx));
    // Present (0x01) | User (0x04) = 0x05, NO Writable (0x02) bit
    let pt_entry = (phys_addr & 0x000F_FFFF_FFFF_F000) | 0x05;
    pt_entry_ptr.write(pt_entry);

    // Flush TLB
    let (pml4_frame, flags) = Cr3::read();
    Cr3::write(pml4_frame, flags);
}

/// Allocates isolated physical pages, maps them at the fixed user virtual address
/// (USER_CODE_BASE = 0x400000), copies the flat binary program, sets up a user stack
/// at USER_STACK_TOP, and transitions execution to Ring 3 User Mode via iretq.
///
/// The binary MUST be compiled with a linker script placing it at USER_CODE_BASE.
/// After objcopy -O binary, byte 0 of the flat binary corresponds to _start.
pub fn execute_user_program(program_bytes: &[u8]) {
    log_info!("🔌 Step 1: Allocating physical memory for Ring 3 program...");

    let prog_len = program_bytes.len();
    let code_total = prog_len + USER_BSS_EXTRA;
    // Round up to page boundary
    let code_pages = (code_total + 4095) / 4096;
    let stack_pages = (USER_STACK_SIZE + 4095) / 4096;

    log_info!("  Binary size: {} bytes, Total (+ BSS): {} bytes = {} pages",
              prog_len, code_total, code_pages);
    log_info!("  Stack: {} bytes = {} pages", USER_STACK_SIZE, stack_pages);

    // Allocate zeroed memory from kernel heap (provides physical backing)
    // We use Layout to ensure 4096-byte page alignment.
    let code_layout = core::alloc::Layout::from_size_align(code_pages * 4096, 4096)
        .unwrap_or_else(|_| panic!("Invalid layout size/align"));
    let code_ptr = unsafe { alloc::alloc::alloc_zeroed(code_layout) };
    if code_ptr.is_null() {
        panic!("execute_user_program: failed to allocate code pages");
    }

    let stack_layout = core::alloc::Layout::from_size_align(stack_pages * 4096, 4096)
        .unwrap_or_else(|_| panic!("Invalid layout size/align"));
    let stack_ptr = unsafe { alloc::alloc::alloc_zeroed(stack_layout) };
    if stack_ptr.is_null() {
        panic!("execute_user_program: failed to allocate stack pages");
    }

    // Copy flat binary into the code buffer (BSS region stays zeroed)
    unsafe {
        core::ptr::copy_nonoverlapping(
            program_bytes.as_ptr(),
            code_ptr,
            prog_len,
        );
    }

    let code_kernel_va = code_ptr as u64;
    let stack_kernel_va = stack_ptr as u64;

    log_info!("  code_kernel_va: 0x{:x} (aligned: {})", code_kernel_va, code_kernel_va % 4096 == 0);
    log_info!("  stack_kernel_va: 0x{:x} (aligned: {})", stack_kernel_va, stack_kernel_va % 4096 == 0);

    // Verify first 16 bytes copied
    unsafe {
        log_info!("  First 8 bytes of binary: {:x} {:x} {:x} {:x} {:x} {:x} {:x} {:x}",
                  *code_ptr.add(0), *code_ptr.add(1), *code_ptr.add(2), *code_ptr.add(3),
                  *code_ptr.add(4), *code_ptr.add(5), *code_ptr.add(6), *code_ptr.add(7));
    }

    log_info!("📂 Step 2: Creating user page table mappings at VA 0x{:x}...", USER_CODE_BASE);

    // Map each code page: kernel_heap_VA -> physical_addr -> user_VA (0x400000+)
    for i in 0..code_pages {
        let kernel_va = code_kernel_va + (i as u64 * 4096);
        let user_va = USER_CODE_BASE + (i as u64 * 4096);
        unsafe {
            let phys = virt_to_phys(kernel_va)
                .unwrap_or_else(|| panic!("Failed to resolve code page {} VA {:x}", i, kernel_va));
            create_user_page_mapping(VirtAddr::new(user_va), phys);
        }
    }

    // Map stack pages at USER_STACK_TOP - USER_STACK_SIZE
    let stack_base_va = USER_STACK_TOP - USER_STACK_SIZE as u64;
    for i in 0..stack_pages {
        let kernel_va = stack_kernel_va + (i as u64 * 4096);
        let user_va = stack_base_va + (i as u64 * 4096);
        unsafe {
            let phys = virt_to_phys(kernel_va)
                .unwrap_or_else(|| panic!("Failed to resolve stack page {} VA {:x}", i, kernel_va));
            create_user_page_mapping(VirtAddr::new(user_va), phys);
        }
    }

    // Full TLB flush
    unsafe {
        let (pml4_frame, flags) = Cr3::read();
        Cr3::write(pml4_frame, flags);
    }

    log_info!("✅ User page tables created successfully!");
    log_info!("  Code: VA 0x{:x} - 0x{:x} ({} pages)",
              USER_CODE_BASE, USER_CODE_BASE + code_pages as u64 * 4096, code_pages);
    log_info!("  Stack: VA 0x{:x} - 0x{:x} ({} pages)",
              stack_base_va, USER_STACK_TOP, stack_pages);

    // Entry point = USER_CODE_BASE (byte 0 of flat binary = _start via linker script)
    let user_entry = USER_CODE_BASE;
    // Stack grows downward, top is USER_STACK_TOP - 8 (16-byte aligned)
    let user_stack_ptr = USER_STACK_TOP - 8;

    log_info!("🚀 Step 3: Jumping to Ring 3 User Mode at 0x{:x}...", user_entry);
    log_info!("------------------------------------------------------------");

    unsafe {
        core::arch::asm!(
            "mov [rip + KERNEL_SHELL_RSP], rsp",
            "mov [rip + KERNEL_SHELL_RBP], rbp",
        );

        enter_user_mode(user_entry, user_stack_ptr);
    }
}

pub fn demonstrate_user_mode() {
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

    execute_user_program(user_program_bytes);
}
