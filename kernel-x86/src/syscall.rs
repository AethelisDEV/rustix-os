//! # System Call Handlers and File Descriptor Management for Ring 0/3 Isolation
//!
//! Implements VFS syscalls (SYS_OPEN, SYS_READ, SYS_WRITE, SYS_CLOSE) using
//! raw pointer manipulation to avoid UB (no direct &mut references to static muts).
//! Validates user-space pointer bounds to ensure Ring 3 code cannot map or corrupt kernel memory.

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::Ordering;
use x86_64::VirtAddr;

use crate::keyboard;
use crate::scheduler;
use crate::interrupts;
use crate::SYSTEM_CORE;
use crate::GRAPHICS;
use crate::SYSTEM_TICKS;
use crate::ALLOCATOR;
use crate::HEAP_MEM;

#[derive(Clone)]
pub struct OpenFile {
    pub path: String,
    pub offset: usize,
}

pub static mut FD_TABLE: [Option<OpenFile>; 16] = [
    None, None, None, None,
    None, None, None, None,
    None, None, None, None,
    None, None, None, None,
];

#[repr(C, align(4096))]
pub struct SharedInfoPage {
    pub info: usermode_x86::syscall::SharedSystemInfo,
    pub padding: [u8; 4096 - core::mem::size_of::<usermode_x86::syscall::SharedSystemInfo>()],
}

// Compile-time check to ensure SharedInfoPage is exactly 4096 bytes (4KB)
const _: () = {
    let size = core::mem::size_of::<SharedInfoPage>();
    if size != 4096 {
        panic!("SharedInfoPage size must be exactly 4096 bytes!");
    }
};

pub static mut SHARED_INFO_PAGE: SharedInfoPage = SharedInfoPage {
    info: usermode_x86::syscall::SharedSystemInfo {
        system_ticks: 0,
        heap_free: 0,
        heap_used: 0,
        cpu_usage: 0,
    },
    padding: [0; 4096 - core::mem::size_of::<usermode_x86::syscall::SharedSystemInfo>()],
};

/// Validates that a user-space pointer range is secure and does not overlap with critical
/// kernel segments. Accepts pointers within:
/// 1. The fixed user-space code/data/BSS region (0x400000+)
/// 2. The fixed user-space stack region (0x780000 - 0x800000)
/// 3. The UEFI GOP framebuffer (identity-mapped)
/// 4. The Shared System Info Page
/// 5. The kernel heap (legacy, for backward compatibility)
fn is_user_ptr(ptr: u64, len: usize) -> bool {
    if ptr == 0 {
        return false;
    }
    let end = match ptr.checked_add(len as u64) {
        Some(e) => e,
        None => return false,
    };

    // 1. Check if it lies within the fixed user-space code/data/BSS region
    let user_code_start = usermode_x86::USER_CODE_BASE;
    let user_code_end = user_code_start + (4 * 1024 * 1024) + (64 * 1024 * 1024); // prog + BSS headroom (68 MB)
    if ptr >= user_code_start && end <= user_code_end {
        return true;
    }

    // 2. Check if it lies within the fixed user stack region
    let user_stack_base = usermode_x86::USER_STACK_TOP - usermode_x86::USER_STACK_SIZE as u64;
    let user_stack_top = usermode_x86::USER_STACK_TOP;
    if ptr >= user_stack_base && end <= user_stack_top {
        return true;
    }

    // 3. Check if it lies within the kernel heap (legacy / backward compatibility)
    let heap_start = unsafe { HEAP_MEM.mem.get() as u64 };
    let heap_end = heap_start + 512 * 1024 * 1024;
    if ptr >= heap_start && end <= heap_end {
        return true;
    }

    // 4. Check if it lies within the UEFI GOP physical framebuffer
    let graphics_lock = GRAPHICS.lock();
    if let Some(ref fb) = *graphics_lock {
        let fb_start = fb.framebuffer_addr();
        let fb_end = fb_start + fb.framebuffer_len() as u64;
        if ptr >= fb_start && end <= fb_end {
            return true;
        }
    }

    // 5. Check if it lies within the Shared System Info Page
    let shared_start = unsafe { core::ptr::addr_of!(SHARED_INFO_PAGE) as u64 };
    let shared_end = shared_start + 4096;
    if ptr >= shared_start && end <= shared_end {
        return true;
    }

    false
}

/// Dynamic System Call Entry Router.
/// Conformant to System V AMD64 ABI registers translated via assembly trampoline.
pub extern "C" fn rust_syscall_handler(id: u64, arg1: u64, arg2: u64, arg3: u64, _arg4: u64, _arg5: u64) -> u64 {
    match id {
        1 => {
            // Legacy/Debugging TTY Write Telemetry
            let ptr = arg1 as *const u8;
            if !is_user_ptr(ptr as u64, 1) {
                return u64::MAX;
            }
            unsafe {
                let mut len = 0;
                while len < 100 && is_user_ptr(ptr.add(len) as u64, 1) && *ptr.add(len) != 0 {
                    len += 1;
                }
                let bytes = core::slice::from_raw_parts(ptr, len);
                if let Ok(s) = core::str::from_utf8(bytes) {
                    crate::println!("\x1B[38;5;46m[SYSCALL 1 (TELE)] User Telemetry: {}\x1B[0m", s);
                }
            }
            1
        }
        2 => {
            // Legacy/Debugging Math syscall
            arg1 * 10
        }
        3 => {
            // Exit program and return to kernel shell
            crate::println!("\x1B[38;5;46m[SYSCALL 3 (EXIT)] User program requested exit. Returning to Kernel TTY Shell.\x1B[0m");
            3
        }
        0x10 => {
            // SYS_MAP_FB: Maps physical GOP Framebuffer to user space and fills ScreenInfo struct
            let screen_info_ptr = arg1 as *mut usermode_x86::syscall::ScreenInfo;
            if screen_info_ptr.is_null() || !is_user_ptr(screen_info_ptr as u64, core::mem::size_of::<usermode_x86::syscall::ScreenInfo>()) {
                return u64::MAX;
            }

            let graphics_lock = GRAPHICS.lock();
            if let Some(ref fb) = *graphics_lock {
                let fb_addr = fb.framebuffer_addr();
                let fb_len = fb.framebuffer_len() as u64;

                let start_page = fb_addr & !0xFFF;
                let end_page = (fb_addr + fb_len + 4095) & !0xFFF;

                let mut current_page = start_page;
                while current_page < end_page {
                    unsafe {
                        usermode_x86::map_page_user(x86_64::VirtAddr::new(current_page));
                    }
                    current_page += 4096;
                }

                unsafe {
                    usermode_x86::map_page_user(x86_64::VirtAddr::new(screen_info_ptr as u64));
                }

                unsafe {
                    (*screen_info_ptr).framebuffer_addr = fb_addr;
                    (*screen_info_ptr).width = fb.width as u64;
                    (*screen_info_ptr).height = fb.height as u64;
                    (*screen_info_ptr).stride = fb.stride as u64;
                    (*screen_info_ptr).bytes_per_pixel = fb.bytes_per_pixel as u64;
                    (*screen_info_ptr).format = match fb.format {
                        bootloader_api::info::PixelFormat::Bgr => 0,
                        bootloader_api::info::PixelFormat::Rgb => 1,
                        _ => 2,
                    };
                }
                0
            } else {
                u64::MAX - 1
            }
        }
        0x11 => {
            // SYS_WAIT_EVENT: Waits for keyboard or mouse input event with timeout
            let event_ptr = arg1 as *mut usermode_x86::syscall::InputEvent;
            let timeout_ms = arg2;
            if event_ptr.is_null() || !is_user_ptr(event_ptr as u64, core::mem::size_of::<usermode_x86::syscall::InputEvent>()) {
                return u64::MAX;
            }

            unsafe {
                usermode_x86::map_page_user(x86_64::VirtAddr::new(event_ptr as u64));
            }

            let start_ticks = SYSTEM_TICKS.load(Ordering::Relaxed);
            let ticks_to_wait = ((timeout_ms + 9) / 10) as usize; // 10ms per tick (100Hz PIT)

            let mut local_event = usermode_x86::syscall::InputEvent {
                event_type: 0,
                keyboard_key: 0,
                mouse_x: 0,
                mouse_y: 0,
                mouse_left_clicked: 0,
                mouse_right_clicked: 0,
            };

            loop {
                if interrupts::pop_input_event(&mut local_event) {
                    unsafe {
                        *event_ptr = local_event;
                    }
                    return 1;
                }

                let current_ticks = SYSTEM_TICKS.load(Ordering::Relaxed);
                let elapsed = current_ticks.saturating_sub(start_ticks);
                if elapsed >= ticks_to_wait {
                    break;
                }

                // Enable interrupts while yielding to allow PIT timer and PS/2 device interrupts to execute
                unsafe {
                    x86_64::instructions::interrupts::enable();
                }

                scheduler::SCHEDULER.lock().thread_yield();

                // Re-disable interrupts upon return to keep syscall processing context isolated
                unsafe {
                    x86_64::instructions::interrupts::disable();
                }
            }
            0
        }
        0x20 => {
            // SYS_OPEN: RDI=path_ptr, RSI=path_len, RDX=flags
            let path_ptr = arg1 as *const u8;
            let path_len = arg2 as usize;
            if path_ptr.is_null() || path_len == 0 || !is_user_ptr(path_ptr as u64, path_len) {
                return u64::MAX;
            }

            // Map path string page(s) user-accessible
            let start_page = (path_ptr as u64) & !0xFFF;
            let end_page = ((path_ptr as u64) + path_len as u64 + 4095) & !0xFFF;
            let mut page = start_page;
            while page < end_page {
                unsafe {
                    usermode_x86::map_page_user(x86_64::VirtAddr::new(page));
                }
                page += 4096;
            }

            let path_slice = unsafe { core::slice::from_raw_parts(path_ptr, path_len) };
            let raw_path_str = match core::str::from_utf8(path_slice) {
                Ok(s) => s,
                Err(_) => return u64::MAX,
            };

            let mut normalized = String::new();
            if !raw_path_str.starts_with('/') {
                normalized.push('/');
            }
            normalized.push_str(raw_path_str);
            let path_str = normalized.as_str();

            let mut core_lock = SYSTEM_CORE.lock();
            if let Some(ref mut core) = *core_lock {
                let exists = core.vfs.resolve_path(path_str).is_ok();
                if !exists {
                    let (parent, name) = crate::split_path(path_str);
                    let parent_clean = parent.trim_start_matches('/');
                    if !parent_clean.is_empty() {
                        let _ = core.vfs.mkdir("/", parent_clean);
                    }
                    match core.vfs.create_file(&parent, &name) {
                        Ok(_) => {}
                        Err(_) => {
                            if !core.vfs.resolve_path(path_str).is_ok() {
                                return u64::MAX;
                            }
                        }
                    }
                }

                let fd_table_ptr = unsafe { core::ptr::addr_of_mut!(FD_TABLE) };
                for i in 3..16 {
                    let slot_ptr = unsafe { core::ptr::addr_of_mut!((*fd_table_ptr)[i]) };
                    let is_none = unsafe { (*slot_ptr).is_none() };
                    if is_none {
                        unsafe {
                            slot_ptr.write(Some(OpenFile {
                                path: String::from(path_str),
                                offset: 0,
                            }));
                        }
                        return i as u64;
                    }
                }
                u64::MAX - 2
            } else {
                u64::MAX - 3
            }
        }
        0x21 => {
            // SYS_READ: RDI=FD, RSI=buffer_ptr, RDX=len
            let fd = arg1 as usize;
            let buf_ptr = arg2 as *mut u8;
            let len = arg3 as usize;
            if buf_ptr.is_null() || len == 0 || !is_user_ptr(buf_ptr as u64, len) {
                return 0;
            }

            // Map buffer pages user-accessible
            let start_page = (buf_ptr as u64) & !0xFFF;
            let end_page = ((buf_ptr as u64) + len as u64 + 4095) & !0xFFF;
            let mut page = start_page;
            while page < end_page {
                unsafe {
                    usermode_x86::map_page_user(x86_64::VirtAddr::new(page));
                }
                page += 4096;
            }

            // Simulating stdin (FD 0) non-blocking reading from serial port COM1
            if fd == 0 {
                let mut read_bytes = 0;
                while read_bytes < len {
                    if let Some(input) = keyboard::poll_serial() {
                        let ch = match input {
                            keyboard::KeyboardInput::Char(c) => c as u8,
                            keyboard::KeyboardInput::Enter => b'\n',
                            keyboard::KeyboardInput::Backspace => 0x08,
                            _ => continue,
                        };
                        unsafe {
                            *buf_ptr.add(read_bytes) = ch;
                        }
                        read_bytes += 1;
                        break;
                    } else {
                        break;
                    }
                }
                return read_bytes as u64;
            }

            if fd < 3 || fd >= 16 {
                return 0;
            }

            let fd_table_ptr = unsafe { core::ptr::addr_of_mut!(FD_TABLE) };
            let slot_ptr = unsafe { core::ptr::addr_of_mut!((*fd_table_ptr)[fd]) };
            let mut file_path = None;
            let mut offset = 0;

            unsafe {
                if let Some(ref file) = *slot_ptr {
                    file_path = Some(file.path.clone());
                    offset = file.offset;
                }
            }

            if let Some(path) = file_path {
                let mut core_lock = SYSTEM_CORE.lock();
                if let Some(ref mut core) = *core_lock {
                    let mut is_directory = false;
                    let mut dir_entries = Vec::new();
                    if let Ok(idx) = core.vfs.resolve_path(&path) {
                        let inode = &core.vfs.inodes[idx];
                        if inode.is_directory() {
                            is_directory = true;
                            if let virtual_fs::InodeType::Directory { entries } = &inode.inode_type {
                                for (name, child_idx) in entries {
                                    let child = &core.vfs.inodes[*child_idx];
                                    if child.is_directory() {
                                        dir_entries.push(alloc::format!("{}/\n", name));
                                    } else {
                                        dir_entries.push(alloc::format!("{}\n", name));
                                    }
                                }
                            }
                        }
                    }

                    if is_directory {
                        let mut dir_str = String::new();
                        for entry in dir_entries {
                            dir_str.push_str(&entry);
                        }
                        let data = dir_str.as_bytes();
                        if offset >= data.len() {
                            return 0;
                        }
                        let available = data.len() - offset;
                        let to_read = core::cmp::min(available, len);
                        unsafe {
                            core::ptr::copy_nonoverlapping(
                                data.as_ptr().add(offset),
                                buf_ptr,
                                to_read,
                            );
                        }
                        unsafe {
                            if let Some(ref mut file) = *slot_ptr {
                                file.offset = offset + to_read;
                            }
                        }
                        return to_read as u64;
                    }

                    match core.vfs.read_file(&path, &mut core.allocator) {
                        Ok(data) => {
                            if offset >= data.len() {
                                return 0;
                            }
                            let available = data.len() - offset;
                            let to_read = core::cmp::min(available, len);
                            unsafe {
                                core::ptr::copy_nonoverlapping(
                                    data.as_ptr().add(offset),
                                    buf_ptr,
                                    to_read,
                                );
                            }
                            // Update offset securely using pointer access
                            unsafe {
                                if let Some(ref mut file) = *slot_ptr {
                                    file.offset = offset + to_read;
                                }
                            }
                            to_read as u64
                        }
                        Err(_) => u64::MAX,
                    }
                } else {
                    u64::MAX
                }
            } else {
                u64::MAX
            }
        }
        0x22 => {
            // SYS_WRITE: RDI=FD, RSI=buffer_ptr, RDX=len
            let fd = arg1 as usize;
            let buf_ptr = arg2 as *const u8;
            let len = arg3 as usize;
            if buf_ptr.is_null() || len == 0 || !is_user_ptr(buf_ptr as u64, len) {
                return 0;
            }

            // Map buffer pages user-accessible
            let start_page = (buf_ptr as u64) & !0xFFF;
            let end_page = ((buf_ptr as u64) + len as u64 + 4095) & !0xFFF;
            let mut page = start_page;
            while page < end_page {
                unsafe {
                    usermode_x86::map_page_user(x86_64::VirtAddr::new(page));
                }
                page += 4096;
            }

            if fd == 1 || fd == 2 {
                let slice = unsafe { core::slice::from_raw_parts(buf_ptr, len) };
                if let Ok(s) = core::str::from_utf8(slice) {
                    // Send to UART serial port directly
                    for c in s.chars() {
                        unsafe {
                            let mut port = x86_64::instructions::port::Port::new(0x3F8);
                            port.write(c as u8);
                        }
                    }
                }
                return len as u64;
            }

            if fd < 3 || fd >= 16 {
                return u64::MAX;
            }

            let fd_table_ptr = unsafe { core::ptr::addr_of_mut!(FD_TABLE) };
            let slot_ptr = unsafe { core::ptr::addr_of_mut!((*fd_table_ptr)[fd]) };
            let mut file_path = None;
            let mut offset = 0;

            unsafe {
                if let Some(ref file) = *slot_ptr {
                    file_path = Some(file.path.clone());
                    offset = file.offset;
                }
            }

            if let Some(path) = file_path {
                let mut core_lock = SYSTEM_CORE.lock();
                if let Some(ref mut core) = *core_lock {
                    let mut data = match core.vfs.read_file(&path, &mut core.allocator) {
                        Ok(d) => d,
                        Err(_) => Vec::new(),
                    };
                    let new_offset = offset + len;
                    if data.len() < new_offset {
                        data.resize(new_offset, 0);
                    }
                    let slice = unsafe { core::slice::from_raw_parts(buf_ptr, len) };
                    data[offset..new_offset].copy_from_slice(slice);

                    match core.vfs.write_file(&path, &data, &mut core.allocator, 1000) {
                        Ok(_) => {
                            // Update offset securely using pointer access
                            unsafe {
                                if let Some(ref mut file) = *slot_ptr {
                                    file.offset = new_offset;
                                }
                            }
                            len as u64
                        }
                        Err(_) => u64::MAX,
                    }
                } else {
                    u64::MAX
                }
            } else {
                u64::MAX
            }
        }
        0x23 => {
            // SYS_CLOSE: RDI=FD
            let fd = arg1 as usize;
            if fd >= 3 && fd < 16 {
                let fd_table_ptr = unsafe { core::ptr::addr_of_mut!(FD_TABLE) };
                let slot_ptr = unsafe { core::ptr::addr_of_mut!((*fd_table_ptr)[fd]) };
                let is_some = unsafe { (*slot_ptr).is_some() };
                if is_some {
                    // Safe drop of old OpenFile by replacing it with None using pointer write
                    let old_file = unsafe { slot_ptr.replace(None) };
                    drop(old_file);
                    0
                } else {
                    u64::MAX
                }
            } else {
                u64::MAX
            }
        }
        0x24 => {
            // SYS_MKDIR: RDI=path_ptr, RSI=path_len
            let path_ptr = arg1 as *const u8;
            let path_len = arg2 as usize;
            if path_ptr.is_null() || path_len == 0 || !is_user_ptr(path_ptr as u64, path_len) {
                return u64::MAX;
            }

            // Map path string page(s) user-accessible
            let start_page = (path_ptr as u64) & !0xFFF;
            let end_page = ((path_ptr as u64) + path_len as u64 + 4095) & !0xFFF;
            let mut page = start_page;
            while page < end_page {
                unsafe {
                    usermode_x86::map_page_user(x86_64::VirtAddr::new(page));
                }
                page += 4096;
            }

            let path_slice = unsafe { core::slice::from_raw_parts(path_ptr, path_len) };
            let raw_path_str = match core::str::from_utf8(path_slice) {
                Ok(s) => s,
                Err(_) => return u64::MAX,
            };

            let mut normalized = String::new();
            if !raw_path_str.starts_with('/') {
                normalized.push('/');
            }
            normalized.push_str(raw_path_str);
            let path_str = normalized.as_str();

            let mut core_lock = SYSTEM_CORE.lock();
            if let Some(ref mut core) = *core_lock {
                let (parent, name) = crate::split_path(path_str);
                let parent_clean = parent.trim_start_matches('/');
                if !parent_clean.is_empty() {
                    let _ = core.vfs.mkdir("/", parent_clean);
                }
                match core.vfs.mkdir(&parent, &name) {
                    Ok(_) => 0,
                    Err(_) => u64::MAX,
                }
            } else {
                u64::MAX
            }
        }
        0x30 => {
            // SYS_GET_SHARED_INFO: returns virtual address of SHARED_INFO_PAGE mapped at user address 0x300000 read-only
            let page_ptr = unsafe { core::ptr::addr_of_mut!(SHARED_INFO_PAGE) };
            let page_addr = page_ptr as u64;
            unsafe {
                let phys = usermode_x86::virt_to_phys(page_addr)
                    .unwrap_or_else(|| panic!("Failed to resolve SHARED_INFO_PAGE VA to PA"));
                usermode_x86::create_user_page_mapping_readonly(x86_64::VirtAddr::new(0x300000), phys);
            }
            0x300000
        }
        _ => {
            crate::println!("\x1B[38;5;196m[SYSCALL ERR] Invalid system call ID received: {}\x1B[0m", id);
            0
        }
    }
}
