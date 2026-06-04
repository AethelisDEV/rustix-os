//! # x86-64 Bare-Metal System Call (Syscall) Interface
//!
//! This module configures model-specific registers (MSRs) and handles raw system calls
//! initiated by Ring 3 user programs via the `syscall` assembly instruction.

use x86_64::registers::model_specific::Msr;

/// Model-Specific Register for Extended Feature Enable Register (EFER).
pub const MSR_EFER: u32 = 0xC0000080;
/// Model-Specific Register for System Call Target Segment Selectors (STAR).
pub const MSR_STAR: u32 = 0xC0000081;
/// Model-Specific Register for System Call Target Instruction Pointer (LSTAR).
pub const MSR_LSTAR: u32 = 0xC0000082;
/// Model-Specific Register for System Call Flag Mask (FMASK).
pub const MSR_FMASK: u32 = 0xC0000084;

/// Temporary static buffer to store the User stack pointer (RSP) during active Syscall executions.
#[no_mangle]
pub static mut USER_RSP: u64 = 0;

/// Static pointer to the secure kernel stack top used to switch execution contexts on Syscall entry.
#[no_mangle]
pub static mut KERNEL_STACK_TOP: u64 = 0;

/// Global dynamic system call handler registered from the main kernel at boot.
#[no_mangle]
pub static mut SYSCALL_HANDLER: usize = 0;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ScreenInfo {
    pub framebuffer_addr: u64,
    pub width: u64,
    pub height: u64,
    pub stride: u64,
    pub bytes_per_pixel: u64,
    pub format: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct InputEvent {
    pub event_type: u32,       // 0 = None, 1 = Keyboard, 2 = Mouse
    pub keyboard_key: u32,     // Decoded character or special key code
    pub mouse_x: i32,
    pub mouse_y: i32,
    pub mouse_left_clicked: u32,
    pub mouse_right_clicked: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SharedSystemInfo {
    pub system_ticks: u64,
    pub heap_free: u64,
    pub heap_used: u64,
    pub cpu_usage: u64,
}

/// Initializes MSR registers to enable and route system calls on x86-64 hardware.
///
/// Sets up:
/// - `EFER.SCE` flag to enable syscall instructions.
/// - `STAR` with Kernel code segment `0x08` and User base segment `0x13` (GDT index 2 | RPL 3).
/// - `LSTAR` pointing to the assembly routine `syscall_entry`.
/// - `FMASK` cleared flags (masks Interrupt Flag IF `0x200` to prevent interrupts during transition).
///
/// # Safety
/// This function is unsafe because it writes directly to Model-Specific Registers,
/// which can trigger CPU exceptions if descriptors or handlers are invalid.
pub unsafe fn init_syscalls(kernel_stack_top: u64, handler: extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64) {
    // 1. Populate the secure kernel stack top and the dynamic syscall handler callback
    KERNEL_STACK_TOP = kernel_stack_top;
    SYSCALL_HANDLER = handler as usize;

    // 2. Enable System Call Extensions inside EFER
    let mut efer_msr = Msr::new(MSR_EFER);
    let efer_val = efer_msr.read();
    efer_msr.write(efer_val | 1); // Set SCE bit (bit 0)

    // 3. Configure Segment Selectors in STAR
    let mut star_msr = Msr::new(MSR_STAR);
    let star_high: u64 = ((0x08u64) << 32) | ((0x13u64) << 48);
    star_msr.write(star_high);

    // 4. Initialize LSTAR pointing to the absolute assembly entry point
    let mut lstar_msr = Msr::new(MSR_LSTAR);
    lstar_msr.write(syscall_entry as *const () as u64);

    // 5. Configure FMASK to clear CPU flags (mask IF flag 0x200 to clear interrupt status)
    let mut fmask_msr = Msr::new(MSR_FMASK);
    fmask_msr.write(0x200);
}

// Low-level Global Assembly routine capturing syscall entries, swapping stacks,
// saving registers, routing to Rust parser, restoring registers, and returning via sysretq.
core::arch::global_asm!(
    ".globl syscall_entry",
    "syscall_entry:",
    // Save User Stack Pointer (RSP) into a static memory location immediately
    "mov [rip + USER_RSP], rsp",
    // Load secure Kernel Stack Pointer (KERNEL_STACK_TOP)
    "mov rsp, [rip + KERNEL_STACK_TOP]",
    // Push User Instruction Pointer (RCX) and User RFLAGS (R11) onto secure Kernel stack
    "push r11",
    "push rcx",
    // Push remaining caller-saved registers to comply with System V ABI rules
    "push rdi",
    "push rsi",
    "push rdx",
    "push rbp",
    "push r8",
    "push r9",
    "push r10",
    // Re-route Syscall parameters to conform to Rust ABI:
    // User syscall ID is in RAX -> becomes 1st argument (RDI in Rust)
    // User arguments (RDI, RSI, RDX, R10, R8) -> become 2nd to 6th arguments (RSI, RDX, RCX, R8, R9)
    "mov r9, r8",
    "mov r8, r10",
    "mov rcx, rdx",
    "mov rdx, rsi",
    "mov rsi, rdi",
    "mov rdi, rax",
    // Call our registered dynamic Rust handler function pointer safely
    "push rax", // Save the syscall ID on the stack
    "mov rax, [rip + SYSCALL_HANDLER]",
    "call rax",
    // Now RAX contains the return value of our syscall handler
    "pop rdi", // Retrieve the syscall ID into RDI
    "cmp rdi, 3", // Check if the SYSCALL ID was 3
    "je syscall_exit_handler",
    // Restore saved registers
    "pop r10",
    "pop r9",
    "pop r8",
    "pop rbp",
    "pop rdx",
    "pop rsi",
    "pop rdi",
    // Restore User Instruction Pointer (RCX) and Flags (R11)
    "pop rcx",
    "pop r11",
    // Restore User Stack Pointer (RSP)
    "mov rsp, [rip + USER_RSP]",
    // Return back to Ring 3 User Space!
    "sysretq",
    "",
    "syscall_exit_handler:",
    // 1. Reload kernel data segment selectors to ensure stability in Ring 0
    "mov ax, 0x10",
    "mov ds, ax",
    "mov es, ax",
    // 2. Restore stack and base pointers to the state expected after calling enter_user_mode
    "mov rsp, [rip + KERNEL_SHELL_RSP]",
    "sub rsp, 8",
    "mov rbp, [rip + KERNEL_SHELL_RBP]",
    // 3. Return to the instruction following enter_user_mode inside execute_user_program
    "ret"
);

// Declare the external assembly function so Rust can reference it
extern "C" {
    fn syscall_entry();
}
