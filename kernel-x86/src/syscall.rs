//! # x86-64 Bare-Metal System Call (Syscall) Interface
//!
//! This module configures model-specific registers (MSRs) and handles raw system calls
//! initiated by Ring 3 user programs via the `syscall` assembly instruction.
//!
//! Implements:
//! 1. Enabling System Call Extensions (SCE) inside `IA32_EFER`.
//! 2. Configuring privilege boundaries and segment bases inside `IA32_STAR`.
//! 3. Initializing the 64-bit target instruction pointer inside `IA32_LSTAR` pointing to `syscall_entry`.
//! 4. Masking interrupt flags inside `IA32_FMASK` on entry.
//! 5. Stacking, executing, and returning from the system call via `sysretq`.

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
pub unsafe fn init_syscalls() {
    // 1. Populate the secure kernel stack top from our loaded TSS privilege stack
    let stack_top = crate::gdt::TSS.privilege_stack_table[0].as_u64();
    KERNEL_STACK_TOP = stack_top;

    // 2. Enable System Call Extensions inside EFER
    let mut efer_msr = Msr::new(MSR_EFER);
    let efer_val = efer_msr.read();
    efer_msr.write(efer_val | 1); // Set SCE bit (bit 0)

    // 3. Configure Segment Selectors in STAR
    // Bits 32-47: Kernel base selector (loaded into CS on syscall. SS is loaded as CS + 8).
    // Bits 48-63: User base selector (on sysret, CS is loaded as User Base + 16, SS as User Base + 8).
    // Kernel Code = 0x08, Kernel Data = 0x10.
    // User Data = 0x18 (index 3), User Code = 0x20 (index 4).
    // Thus: User Base = GDT index 2 (0x10) ORed with RPL 3 = 0x13.
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
    // User argument is in RDI -> becomes 2nd argument (RSI in Rust)
    "mov rsi, rdi",
    "mov rdi, rax",
    // Call our Rust handler safely
    "call rust_syscall_handler",
    // Now RAX contains the return value of our syscall handler
    "cmp rax, 3",
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
    // 3. Return to the instruction following enter_user_mode inside demonstrate_user_mode
    "ret"
);

// Declare the external assembly function so Rust can reference it
extern "C" {
    fn syscall_entry();
}

/// Safe Rust Syscall Dispatcher handler called directly from the global assembly handler.
///
/// Parses the incoming syscall ID and returns the result back to RAX.
///
/// Supported Syscalls:
/// - **Syscall `1`**: Prints a Ring 3 Telemetry log onto the serial COM1 interface and GOP.
/// - **Syscall `2`**: Multiplies the provided user value by 10 and returns the calculation.
/// - **Syscall `3`**: Exits the user-mode execution and returns safely to the Kernel TTY Shell.
#[no_mangle]
pub extern "C" fn rust_syscall_handler(id: u64, arg: u64) -> u64 {
    match id {
        1 => {
            // Print user telemetry string passed as raw pointer arg
            let ptr = arg as *const u8;
            unsafe {
                // Safety-check: Read a bounded ASCII string safely from Ring 3
                let mut len = 0;
                while len < 100 && *ptr.add(len) != 0 {
                    len += 1;
                }
                let bytes = core::slice::from_raw_parts(ptr, len);
                if let Ok(s) = core::str::from_utf8(bytes) {
                    println!("\x1B[38;5;46m[SYSCALL 1 (TELE)] User Telemetry: {}\x1B[0m", s);
                }
            }
            1 // Status OK
        }
        2 => {
            // Echo calculation syscall
            println!("\x1B[38;5;51m[SYSCALL 2 (MATH)] User Math request. Multiplying {} * 10...\x1B[0m", arg);
            arg * 10 // Return calculated result
        }
        3 => {
            println!("\x1B[38;5;46m[SYSCALL 3 (EXIT)] User program requested exit. Returning to Kernel TTY Shell.\x1B[0m");
            3 // return 3 to trigger the exit trampoline
        }
        _ => {
            println!("\x1B[38;5;196m[SYSCALL ERR] Invalid system call ID received: {}\x1B[0m", id);
            0
        }
    }
}
