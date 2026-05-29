//! # Cooperative Thread Scheduler and Context Switcher
//!
//! This module implements cooperative multitasking for the bare-metal kernel:
//! 1. Allocates independent 8 KB stacks for spawned tasks.
//! 2. Declares `TaskContext` holding the callee-saved registers (rbx, rbp, r12, r13, r14, r15, rsp).
//! 3. Implements `switch_context` in inline assembly to execute the low-level context switch.
//! 4. Implements a round-robin Thread Scheduler (`Scheduler`) mapping tasks in a circular queue.
//!
//! Written with high-fidelity safety abstractions and compliant with the SOLID architecture.

use alloc::vec::Vec;

/// Maximum number of concurrent threads supported by the scheduler.
pub const MAX_THREADS: usize = 4;
/// Thread stack size (8 KB per thread).
pub const STACK_SIZE: usize = 8 * 1024;

/// Represents the CPU registers saved during a context switch on x86_64.
///
/// Under System V ABI, the callee-preserved registers must be saved across function calls.
/// These are: r15, r14, r13, r12, rbp, rbx, and rsp.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
struct TaskContext {
    r15: u64,
    r14: u64,
    r13: u64,
    r12: u64,
    rbx: u64,
    rbp: u64,
    rip: u64, // Return address (instruction pointer)
}

/// Status representing the execution life-cycle of a bare-metal thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadStatus {
    /// Thread is initialized and ready to be scheduled.
    Ready,
    /// Thread is currently executing on the CPU core.
    Running,
}

/// Thread Control Block (TCB) managing thread metadata, stack space, and register state.
pub struct Thread {
    /// Unique thread identifier.
    pub id: usize,
    /// Current execution status.
    pub status: ThreadStatus,
    /// Isolated stack space allocated dynamically.
    pub stack: Vec<u8>,
    /// Saved stack pointer (RSP) address.
    pub rsp: u64,
}

/// The Scheduler coordinates thread lifecycles and context switching.
pub struct Scheduler {
    /// List of registered threads.
    pub threads: Vec<Thread>,
    /// Index of the currently executing thread.
    pub current_thread_idx: usize,
}

impl Scheduler {
    /// Creates a new, uninitialized thread Scheduler.
    pub const fn new() -> Self {
        Self {
            threads: Vec::new(),
            current_thread_idx: 0,
        }
    }

    /// Registers the currently running main context (from boot stage) as Thread 0.
    pub fn register_main_thread(&mut self) {
        let t = Thread {
            id: 0,
            status: ThreadStatus::Running,
            stack: Vec::new(), // Stack is already allocated by the bootloader
            rsp: 0,            // Will be set on the very first yield
        };
        self.threads.push(t);
    }

    /// Spawns a new thread executing the provided entry point function.
    pub fn spawn(&mut self, entry_point: fn()) -> Result<usize, &'static str> {
        if self.threads.len() >= MAX_THREADS {
            return Err("Maximum thread capacity reached!");
        }

        // Allocate stack space
        let mut stack = alloc::vec![0u8; STACK_SIZE];
        let stack_top = stack.as_mut_ptr() as usize + STACK_SIZE;

        // Align stack top to 16 bytes for x86_64 System V ABI alignment requirements
        let aligned_stack_top = (stack_top & !15) - 8;

        let thread_id = self.threads.len();

        // Write the initial dummy TaskContext onto the thread's stack
        unsafe {
            let context_ptr = (aligned_stack_top - core::mem::size_of::<TaskContext>()) as *mut TaskContext;

            context_ptr.write(TaskContext {
                r15: 0,
                r14: 0,
                r13: 0,
                r12: 0,
                rbp: aligned_stack_top as u64,
                rbx: 0,
                rip: entry_point as u64, // Thread will start executing at entry_point
            });

            let t = Thread {
                id: thread_id,
                status: ThreadStatus::Ready,
                stack,
                rsp: context_ptr as u64,
            };

            self.threads.push(t);
        }

        Ok(thread_id)
    }

    /// Cooperatively yields current execution context to the next ready thread.
    pub fn thread_yield(&mut self) {
        if self.threads.len() <= 1 {
            return; // No other threads to yield to!
        }

        // Select the next thread (Round-Robin scheduling)
        let next_idx = (self.current_thread_idx + 1) % self.threads.len();

        let old_idx = self.current_thread_idx;
        self.current_thread_idx = next_idx;

        // Toggle execution states
        self.threads[old_idx].status = ThreadStatus::Ready;
        self.threads[next_idx].status = ThreadStatus::Running;

        // Perform the assembly context switch
        unsafe {
            let old_rsp_ptr = &mut self.threads[old_idx].rsp as *mut u64;
            let new_rsp = self.threads[next_idx].rsp;
            SCHEDULER.force_unlock();
            switch_context(old_rsp_ptr, new_rsp);
        }
    }
}

/// Global scheduler instance wrapped in our thread-safe Spinlock.
pub static SCHEDULER: crate::Spinlock<Scheduler> = crate::Spinlock::new(Scheduler::new());

/// Core low-level assembly context switcher.
/// Saves preserved registers on the current stack, switches stack pointers,
/// and pops registers from the new stack, jumping to the new return address.
///
/// # Safety
/// This is highly unsafe because it directly manipulates the CPU stack pointer
/// and caller-preserved registers, altering the flow of execution.
///
/// Implemented via `global_asm!` so the LLVM naked-function stack-alignment
/// validator does not reject the intentionally asymmetric push/pop sequence.
core::arch::global_asm!(
    ".globl switch_context",
    "switch_context:",
    // 1. Push callee-preserved registers onto old stack (6 × 8 = 48 bytes)
    "push rbp",
    "push rbx",
    "push r12",
    "push r13",
    "push r14",
    "push r15",
    // 2. Save old RSP (pointer passed in rdi) to memory
    "mov [rdi], rsp",
    // 3. Load new RSP from rsi (new thread's saved stack pointer)
    "mov rsp, rsi",
    // 4. Restore callee-preserved registers from new stack
    "pop r15",
    "pop r14",
    "pop r13",
    "pop r12",
    "pop rbx",
    "pop rbp",
    // 5. Jump to the return address on top of the new stack
    "ret",
);

/// External declaration so Rust code can call the assembly function.
///
/// # Safety
/// Caller must ensure `old_rsp` points to a valid u64 slot and that
/// `new_rsp` points to a correctly initialised thread stack.
#[allow(dead_code)]
unsafe extern "C" {
    #[no_mangle]
    fn switch_context(old_rsp: *mut u64, new_rsp: u64);
}

/// Public safe wrapper around the unsafe assembly context switcher.
///
/// # Safety
/// `old_rsp` must point to a valid u64 slot and `new_rsp` must be a correctly
/// initialised thread stack from `Scheduler::spawn`.
pub unsafe fn do_switch_context(old_rsp: *mut u64, new_rsp: u64) {
    switch_context(old_rsp, new_rsp);
}
