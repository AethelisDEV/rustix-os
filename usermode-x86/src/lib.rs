#![no_std]

extern crate alloc;

pub mod usermode;
pub mod syscall;

// Re-export items for easier access
pub use usermode::{
    PHYSICAL_MEMORY_OFFSET, KERNEL_SHELL_RSP, KERNEL_SHELL_RBP,
    map_page_user, execute_user_program, demonstrate_user_mode,
};
pub use syscall::{init_syscalls, SYSCALL_HANDLER};

/// Global logger callback to allow clean decoupled logging back to the host kernel console.
pub static mut LOG_CALLBACK: Option<fn(&str)> = None;

/// Sets the active logger callback function.
pub fn init_logger(callback: fn(&str)) {
    unsafe {
        LOG_CALLBACK = Some(callback);
    }
}

/// Dispatches a log message back to the registered console callback.
pub fn log(msg: &str) {
    unsafe {
        if let Some(cb) = LOG_CALLBACK {
            cb(msg);
        }
    }
}

/// Internal macro to simplify formatting and logging across usermode and syscall submodules.
#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        $crate::log(&alloc::format!($($arg)*));
    };
}
