//! # Global Descriptor Table (GDT) and Task State Segment (TSS) for Ring 3 Isolation
//!
//! This module defines and initializes a custom GDT and TSS on the bare-metal x86-64 CPU:
//! 1. Allocates a secure privilege stack (`RSP0`) inside the Task State Segment.
//! 2. Registers descriptor selectors for Kernel Code, Kernel Data, User Data, and User Code.
//! 3. Dynamically reloads segment registers (`CS`, `DS`, `SS`) and loads the Task Register (`TR`).
//!
//! Required to establish hardware boundaries between executive supervisor levels (Ring 0) and
//! restricted user application spaces (Ring 3).

use x86_64::VirtAddr;
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::instructions::segmentation::{CS, DS, SS, Segment};
use x86_64::instructions::tables::load_tss;

/// Secure privilege stack size (8 KB) used during CPU context transitions from Ring 3 to Ring 0.
pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;

/// Static Task State Segment allocating hardware stacks for Ring transitions and double faults.
pub static mut TSS: TaskStateSegment = TaskStateSegment::new();

/// Static Global Descriptor Table defining privilege levels and segment capabilities.
pub static mut GDT: GlobalDescriptorTable = GlobalDescriptorTable::new();

/// Segment selectors holding offsets inside our GDT.
pub static mut SELECTORS: Option<Selectors> = None;

/// Secure static memory buffer allocated for the privilege stack RSP0.
static mut STACK_BUFFER: [u8; 8192] = [0; 8192];

/// Holds the exact segment selectors mapped to our custom Global Descriptor Table.
#[derive(Debug, Clone, Copy)]
pub struct Selectors {
    /// Ring 0 Kernel Code Segment Selector.
    pub kernel_code: SegmentSelector,
    /// Ring 0 Kernel Data Segment Selector.
    pub kernel_data: SegmentSelector,
    /// Ring 3 User Data Segment Selector.
    pub user_data: SegmentSelector,
    /// Ring 3 User Code Segment Selector.
    pub user_code: SegmentSelector,
    /// Task State Segment Selector.
    pub tss: SegmentSelector,
}

/// Initializes and loads the custom Global Descriptor Table and Task State Segment on the CPU.
///
/// Sets up:
/// - `RSP0` pointer inside `TSS` pointing to the secure kernel stack buffer.
/// - GDT containing Kernel Code/Data, User Code/Data, and TSS.
/// - Segment registers reloaded with new offsets.
///
/// # Safety
/// This function is unsafe because it manipulates CPU segmentation registers,
/// loads a low-level Task Register, and configures raw hardware privilege selectors.
pub unsafe fn init_gdt() {
    // 1. Initialize TSS and set the Ring 0 privilege stack (RSP0)
    // When an interrupt or exception triggers while executing in Ring 3,
    // the CPU reads RSP0 from the TSS to securely transition to the Kernel Stack.
    let stack_top = VirtAddr::from_ptr(&STACK_BUFFER as *const u8).as_u64() + 8192;
    TSS.privilege_stack_table[0] = VirtAddr::new(stack_top);

    // 2. Initialize the Global Descriptor Table
    // The selector offsets are structured sequentially. User space SYSRET instruction
    // requires User Code to follow User Data exactly.
    let mut gdt = GlobalDescriptorTable::new();
    let kernel_code = gdt.add_entry(Descriptor::kernel_code_segment());
    let kernel_data = gdt.add_entry(Descriptor::kernel_data_segment());
    let user_data = gdt.add_entry(Descriptor::user_data_segment());
    let user_code = gdt.add_entry(Descriptor::user_code_segment());
    let tss = gdt.add_entry(Descriptor::tss_segment(&TSS));

    // Save GDT and Selectors into static storage
    GDT = gdt;
    SELECTORS = Some(Selectors {
        kernel_code,
        kernel_data,
        user_data,
        user_code,
        tss,
    });

    // Load GDT onto the active CPU core
    GDT.load();

    // 3. Reload segment registers to use the newly loaded selectors
    CS::set_reg(kernel_code);
    DS::set_reg(kernel_data);
    SS::set_reg(kernel_data);

    // Load Task Register to activate our Task State Segment (TSS)
    load_tss(tss);
}
