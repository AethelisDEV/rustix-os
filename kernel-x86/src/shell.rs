//! # Bare-Metal System Command Shell Parser
//!
//! Provides the parser, dispatcher, and underlying logic for the microkernel's
//! interactive command-line interface (CLI). Manages directory structures, VFS tree
//! traversal, memory queries, and diagnostics controls.

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::Ordering;
use crate::SYSTEM_TICKS;
use crate::keyboard::KeyboardLayout;

/// Executes a single text CLI command parsed from input.
/// Supports standard shell commands like help, ls, mkdir, touch, cat, loadkeys, free, status, and tasks.
pub fn handle_command(
    cmd_line: &str,
    core: &mut kernel_core::SystemCore,
    cwd: &mut String,
    history: &[String],
) {
    let mut parts = cmd_line.split_whitespace();
    let cmd = match parts.next() {
        Some(c) => c,
        None => return,
    };
    let args: Vec<&str> = parts.collect();

    match cmd {
        "help" => {
            println!("============================================================");
            println!("         AE RUSTANIUM BARE-METAL INTERACTIVE SHELL          ");
            println!("============================================================");
            println!("File System:");
            println!("  ls [path]        - List directory contents");
            println!("  pwd              - Print current working directory");
            println!("  cd <path>        - Change working directory (.. supported)");
            println!("  mkdir <path>     - Create a new directory");
            println!("  touch <path>     - Create an empty file");
            println!("  write <p> <txt>  - Write text into a file");
            println!("  cat <path>       - Display file content");
            println!("  head [-n] <path> - Show first N lines of a file (default 10)");
            println!("  tail [-n] <path> - Show last N lines of a file (default 10)");
            println!("  wc <path>        - Count lines, words, bytes in file");
            println!("  cp <src> <dst>   - Copy a file to a new location");
            println!("  mv <src> <dst>   - Move / rename a file or directory");
            println!("  rm [-rf] <path>  - Remove file or directory recursively");
            println!("  find <name>      - Search VFS tree for a name");
            println!("  vfs              - Print the full VFS tree");
            println!("System:");
            println!("  echo <text>      - Print text to the console");
            println!("  uname            - Print kernel and hardware identity");
            println!("  uptime           - Show system uptime in ticks and seconds");
            println!("  free             - Show heap and page allocator memory usage");
            println!("  whoami           - Print current user identity");
            println!("  hostname         - Print the system hostname");
            println!("  history          - List previously executed commands");
            println!("  loadkeys <lay>   - Switch keyboard layout (us, trq)");
            println!("  status           - Microkernel health & memory metrics");
            println!("  tasks            - List running microservices");
            println!("  usermode         - Launch Ring 3 User Space & Syscall demonstration");
            println!("  run <path>       - Dynamically load and run a Ring 3 VFS binary");
            println!("  inject-flip      - Inject synthetic radiation bit flip");
            println!("  clear            - Clear the console screen");
            println!("  help             - Show this help menu");
            println!("============================================================");
        }
        "loadkeys" => {
            if args.is_empty() {
                println!("Usage: loadkeys <layout> (e.g. us, trq)");
                return;
            }
            match args[0] {
                "us" => {
                    unsafe {
                        crate::interrupts::KEYBOARD_STATE.set_layout(KeyboardLayout::Us);
                    }
                    println!("Keyboard layout switched to US.");
                }
                "trq" => {
                    unsafe {
                        crate::interrupts::KEYBOARD_STATE.set_layout(KeyboardLayout::Trq);
                    }
                    println!("Keyboard layout switched to Turkish Q (TRQ).");
                }
                other => {
                    println!("Unknown keyboard layout: '{}'. Supported: us, trq", other);
                }
            }
        }
        "status" => {
            println!("------------------------------------------------------------");
            println!("SYSTEM HEALTH & PHYSICAL MEMORY STATUS");
            println!("------------------------------------------------------------");
            println!("Scrubber Sweeps:           {}", core.scrubber_sweeps);
            println!("ECC SECDED Corrections:    {}", core.ecc_single_bit_corrections);
            println!("Pages Quarantined:         {}", core.pages_quarantined);
            println!("Pages Relocated:           {}", core.pages_relocated);
            println!("TMR CPU Operations:        {}", core.critical_tmr_ops);
            println!("TMR ALU Corrections:       {}", core.tmr_voter_corrections);
            
            // Count allocated physical memory pages
            let mut allocated = 0;
            for pid_opt in &core.allocator.allocation_map {
                if pid_opt.is_some() {
                    allocated += 1;
                }
            }
            println!("Allocated Page Frames:     {}/{}", allocated, core.allocator.allocation_map.len());
            println!("------------------------------------------------------------");
        }
        "tasks" => {
            println!("------------------------------------------------------------");
            println!("RUNNING MICROSERVICES");
            println!("------------------------------------------------------------");
            println!("{:<5} | {:<16} | {:<8} | Allocated Pages", "PID", "Process Name", "Critical");
            println!("------+------------------+----------+-----------------");
            for p in &core.dispatcher.processes {
                println!(
                    "{:<5} | {:<16} | {:<8} | {:?}",
                    p.pid,
                    p.name,
                    if p.is_critical { "YES (TMR)" } else { "NO" },
                    p.allocated_pages
                );
            }
            println!("------------------------------------------------------------");
        }
        "usermode" => {
            println!("------------------------------------------------------------");
            println!("LAUNCHING RING 3 USER SPACE & SYSCALL DEMONSTRATION");
            println!("------------------------------------------------------------");
            usermode_x86::demonstrate_user_mode();
            println!("------------------------------------------------------------");
        }
        "run" => {
            if args.is_empty() {
                println!("\x1B[38;5;196mUsage: run <path_to_binary.bin>\x1B[0m");
                return;
            }
            let file_path = resolve_relative_path(cwd, args[0]);
            match core.vfs.read_file(&file_path, &mut core.allocator) {
                Ok(data) => {
                    println!("------------------------------------------------------------");
                    println!("LAUNCHING RING 3 PROCESS FROM VFS: {}", file_path);
                    println!("------------------------------------------------------------");
                    usermode_x86::execute_user_program(&data);
                    println!("------------------------------------------------------------");
                }
                Err(e) => {
                    println!("\x1B[38;5;196m[RUN ERR] Failed to load program '{}': {}\x1B[0m", file_path, e);
                }
            }
        }
        "inject-flip" => {
            // Find the first allocated frame
            let mut target_frame = None;
            for (idx, pid_opt) in core.allocator.allocation_map.iter().enumerate() {
                if pid_opt.is_some() {
                    target_frame = Some(idx);
                    break;
                }
            }

            if let Some(frame_idx) = target_frame {
                println!("[INJECTOR] Targeting frame {} (allocated to process)...", frame_idx);
                // Inject flip on offset 8, bit 3
                match core.inject_memory_flip(frame_idx, 8, 3) {
                    Ok(_) => {
                        println!("\x1B[38;5;220m[INJECTOR OK] Injected synthetic bit flip into physical frame {} offset 8, bit 3!\x1B[0m", frame_idx);
                        println!("[INJECTOR] Scrubber will auto-heal it on the next scheduler tick.");
                    }
                    Err(e) => {
                        println!("\x1B[38;5;196m[INJECTOR ERR] Failed to inject: {}\x1B[0m", e);
                    }
                }
            } else {
                println!("\x1B[38;5;196m[INJECTOR ERR] No allocated memory frames found to target!\x1B[0m");
            }
        }
        "vfs" => {
            println!("------------------------------------------------------------");
            println!("VFS TREE STRUCTURE");
            println!("------------------------------------------------------------");
            print_vfs_tree(&core.vfs, 0, 0);
            println!("------------------------------------------------------------");
        }
        "ls" => {
            // Default to cwd when no argument given
            let path = if args.is_empty() {
                cwd.as_str()
            } else {
                args[0]
            };
            let resolved = resolve_relative_path(cwd, path);
            match core.vfs.resolve_path(&resolved) {
                Ok(idx) => {
                    let inode = &core.vfs.inodes[idx];
                    match &inode.inode_type {
                        virtual_fs::InodeType::Directory { entries } => {
                            println!("{}:", resolved);
                            if entries.is_empty() {
                                    println!("  (directory is empty)");
                            }
                            for (name, child_idx) in entries {
                                let child = &core.vfs.inodes[*child_idx];
                                if child.is_directory() {
                                    println!("  \x1B[38;5;33m{}/\x1B[0m", name);
                                } else {
                                    println!("  {}", name);
                                }
                            }
                        }
                        virtual_fs::InodeType::File { size, .. } => {
                            println!("{} (file, {} bytes)", inode.name, size);
                        }
                    }
                }
                Err(e) => {
                    println!("\x1B[38;5;196m[VFS ERR] Path resolve failed: {}\x1B[0m", e);
                }
            }
        }
        "mkdir" => {
            if args.is_empty() {
                println!("\x1B[38;5;196mUsage: mkdir <path>\x1B[0m");
                return;
            }
            let full_path = resolve_relative_path(cwd, args[0]);
            let (parent_path, name) = split_parent_child(&full_path);
            match core.vfs.mkdir(&parent_path, &name) {
                Ok(_) => {
                    println!("[VFS] Created directory: {}", full_path);
                }
                Err(e) => {
                    println!("\x1B[38;5;196m[VFS ERR] mkdir failed: {}\x1B[0m", e);
                }
            }
        }
        "touch" => {
            if args.is_empty() {
                println!("\x1B[38;5;196mUsage: touch <path>\x1B[0m");
                return;
            }
            let full_path = resolve_relative_path(cwd, args[0]);
            let (parent_path, name) = split_parent_child(&full_path);
            match core.vfs.create_file(&parent_path, &name) {
                Ok(_) => {
                    println!("[VFS] Created file: {}", full_path);
                }
                Err(e) => {
                    println!("\x1B[38;5;196m[VFS ERR] touch failed: {}\x1B[0m", e);
                }
            }
        }
        "write" => {
            if args.len() < 2 {
                println!("\x1B[38;5;196mUsage: write <path> <text_content...>\x1B[0m");
                return;
            }
            let file_path = resolve_relative_path(cwd, args[0]);
            let text_content = args[1..].join(" ");
            match core.vfs.write_file(&file_path, text_content.as_bytes(), &mut core.allocator, 1000) {
                Ok(_) => {
                    println!("[VFS] Wrote {} bytes to file: {}", text_content.len(), file_path);
                }
                Err(e) => {
                    println!("\x1B[38;5;196m[VFS ERR] write failed: {}\x1B[0m", e);
                }
            }
        }
        "rm" => {
            if args.is_empty() {
                println!("\x1B[38;5;196mUsage: rm [-rf] <path>\x1B[0m");
                return;
            }
            let mut recursive = false;
            let raw_path = if args[0] == "-rf" {
                if args.len() < 2 {
                    println!("\x1B[38;5;196mUsage: rm -rf <path>\x1B[0m");
                    return;
                }
                recursive = true;
                args[1]
            } else {
                args[0]
            };
            let path = resolve_relative_path(cwd, raw_path);
            match core.vfs.remove_node(&path, recursive, &mut core.allocator) {
                Ok(_) => {
                    println!("[VFS] Removed node: {}", path);
                }
                Err(e) => {
                    println!("\x1B[38;5;196m[VFS ERR] remove failed: {}\x1B[0m", e);
                }
            }
        }
        "cat" => {
            if args.is_empty() {
                println!("\x1B[38;5;196mUsage: cat <file_path>\x1B[0m");
                return;
            }
            let file_path = resolve_relative_path(cwd, args[0]);
            match core.vfs.read_file(&file_path, &mut core.allocator) {
                Ok(data) => {
                    println!("--- {} ---", file_path);
                    if let Ok(text) = core::str::from_utf8(&data) {
                        print!("{}", text);
                        if !text.ends_with('\n') { println!(); }
                    } else {
                        // Hex dump for binary contents
                        for chunk in data.chunks(16) {
                            for b in chunk {
                                print!("{:02X} ", b);
                            }
                            println!();
                        }
                    }
                    println!("------------------");
                }
                Err(e) => {
                    println!("\x1B[38;5;196m[VFS ERR] Failed to read file {}: {}\x1B[0m", file_path, e);
                }
            }
        }
        "pwd" => {
            println!("{}", cwd);
        }
        "cd" => {
            let target = if args.is_empty() { "/" } else { args[0] };
            let new_path = resolve_relative_path(cwd, target);
            // Validate that the path exists and is a directory
            match core.vfs.resolve_path(&new_path) {
                Ok(idx) => {
                    if core.vfs.inodes[idx].is_directory() {
                        *cwd = new_path;
                    } else {
                        println!("\x1B[38;5;196mcd: '{}' is not a directory\x1B[0m", target);
                    }
                }
                Err(_) => {
                    println!("\x1B[38;5;196mcd: '{}': No such file or directory\x1B[0m", target);
                }
            }
        }
        "echo" => {
            println!("{}", args.join(" "));
        }
        "cp" => {
            if args.len() < 2 {
                println!("\x1B[38;5;196mUsage: cp <src> <dst>\x1B[0m");
                return;
            }
            let src = resolve_relative_path(cwd, args[0]);
            let (dst_parent, dst_name) = split_parent_child(args[1]);
            match core.vfs.copy_file(&src, &dst_parent, &dst_name, &mut core.allocator, 1000) {
                Ok(_) => println!("[VFS] Copied '{}' -> '{}'", src, args[1]),
                Err(e) => println!("\x1B[38;5;196m[VFS ERR] cp failed: {}\x1B[0m", e),
            }
        }
        "mv" => {
            if args.len() < 2 {
                println!("\x1B[38;5;196mUsage: mv <src> <dst>\x1B[0m");
                return;
            }
            let src = resolve_relative_path(cwd, args[0]);
            let (dst_parent, dst_name) = split_parent_child(args[1]);
            match core.vfs.rename_node(&src, &dst_parent, &dst_name) {
                Ok(_) => println!("[VFS] Moved '{}' -> '{}'", src, args[1]),
                Err(e) => println!("\x1B[38;5;196m[VFS ERR] mv failed: {}\x1B[0m", e),
            }
        }
        "uname" => {
            println!("AE-RUSTANIUM 0.1.0 bare-metal x86_64 UEFI/BIOS");
            println!("Kernel: no_std Rust microkernel (nightly)");
            println!("Arch:   x86_64  |  CPU: AMD/Intel 64-bit");
            println!("Boot:   UEFI GOP + Legacy BIOS MBR");
        }
        "uptime" => {
            let ticks = SYSTEM_TICKS.load(Ordering::Relaxed);
            let secs_approx = ticks / 50;
            println!("Uptime: {} ticks  (~{} seconds)", ticks, secs_approx);
            println!("Load:   cooperative round-robin scheduler — 3 threads active");
        }
        "free" => {
            let total_pages = core.allocator.allocation_map.len();
            let used_pages = core.allocator.allocation_map.iter().filter(|p| p.is_some()).count();
            let free_pages = total_pages - used_pages;
            println!("------------------------------------------------------------");
            println!("MEMORY USAGE (Page Allocator)");
            println!("------------------------------------------------------------");
            println!("Total  pages : {}", total_pages);
            println!("Used   pages : {}", used_pages);
            println!("Free   pages : {}", free_pages);
            println!("Page size    : 64 bytes (SECDED Hamming blocks)");
            println!("Heap         : 1 MB LockedHeap (linked-list allocator)");
            println!("------------------------------------------------------------");
        }
        "whoami" => {
            println!("root");
        }
        "hostname" => {
            println!("rustanium");
        }
        "history" => {
            if history.is_empty() {
                println!("(no command history yet)");
            } else {
                for (i, entry) in history.iter().enumerate() {
                    println!("{:>4}  {}", i + 1, entry);
                }
            }
        }
        "head" => {
            let (n, path) = parse_n_flag(&args, 10);
            let path = match path {
                Some(p) => resolve_relative_path(cwd, p),
                None => { println!("\x1B[38;5;196mUsage: head [-n <count>] <path>\x1B[0m"); return; }
            };
            match core.vfs.read_file(&path, &mut core.allocator) {
                Ok(data) => {
                    if let Ok(text) = core::str::from_utf8(&data) {
                        for (i, line) in text.lines().enumerate() {
                            if i >= n { break; }
                            println!("{}", line);
                        }
                    } else {
                        println!("(binary file — {} bytes)", data.len());
                    }
                }
                Err(e) => println!("\x1B[38;5;196m[VFS ERR] head: {}\x1B[0m", e),
            }
        }
        "tail" => {
            let (n, path) = parse_n_flag(&args, 10);
            let path = match path {
                Some(p) => resolve_relative_path(cwd, p),
                None => { println!("\x1B[38;5;196mUsage: tail [-n <count>] <path>\x1B[0m"); return; }
            };
            match core.vfs.read_file(&path, &mut core.allocator) {
                Ok(data) => {
                    if let Ok(text) = core::str::from_utf8(&data) {
                        let all_lines: Vec<&str> = text.lines().collect();
                        let start = if all_lines.len() > n { all_lines.len() - n } else { 0 };
                        for line in &all_lines[start..] {
                            println!("{}", line);
                        }
                    } else {
                        println!("(binary file — {} bytes)", data.len());
                    }
                }
                Err(e) => println!("\x1B[38;5;196m[VFS ERR] tail: {}\x1B[0m", e),
            }
        }
        "wc" => {
            if args.is_empty() {
                println!("\x1B[38;5;196mUsage: wc <path>\x1B[0m");
                return;
            }
            let path = resolve_relative_path(cwd, args[0]);
            match core.vfs.read_file(&path, &mut core.allocator) {
                Ok(data) => {
                    let bytes = data.len();
                    if let Ok(text) = core::str::from_utf8(&data) {
                        let lines = text.lines().count();
                        let words = text.split_whitespace().count();
                        println!("  lines: {}  words: {}  bytes: {}  {}", lines, words, bytes, path);
                    } else {
                        println!("  (binary)  bytes: {}  {}", bytes, path);
                    }
                }
                Err(e) => println!("\x1B[38;5;196m[VFS ERR] wc: {}\x1B[0m", e),
            }
        }
        "find" => {
            if args.is_empty() {
                println!("\x1B[38;5;196mUsage: find <name>\x1B[0m");
                return;
            }
            let query = args[0];
            let mut results: Vec<String> = Vec::new();
            find_in_vfs(&core.vfs, 0, "/", query, &mut results);
            if results.is_empty() {
                println!("(no match found for '{}')", query);
            } else {
                for r in &results {
                    println!("{}", r);
                }
            }
        }
        "clear" => {
            print!("\x1B[2J\x1B[H");
        }
        other => {
            println!("\x1B[38;5;196mUnknown command: '{}'. Type 'help' for options.\x1B[0m", other);
        }
    }
}

/// Resolves a path relative to the given current working directory.
pub fn resolve_relative_path(cwd: &str, path: &str) -> String {
    if path.starts_with('/') {
        return normalize_path(path);
    }
    let base = if cwd.ends_with('/') {
        alloc::format!("{}{}", cwd, path)
    } else {
        alloc::format!("{}/{}", cwd, path)
    };
    normalize_path(&base)
}

/// Collapses '.' and '..' segments in an absolute path string.
pub fn normalize_path(path: &str) -> String {
    let mut stack: Vec<&str> = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => { stack.pop(); }
            s => stack.push(s),
        }
    }
    if stack.is_empty() {
        String::from("/")
    } else {
        let mut out = String::new();
        for s in &stack {
            out.push('/');
            out.push_str(s);
        }
        out
    }
}

/// Parses an optional `-n <count>` flag from the beginning of an args slice.
fn parse_n_flag<'a>(args: &'a [&str], default: usize) -> (usize, Option<&'a str>) {
    if args.len() >= 2 && args[0] == "-n" {
        let n = args[1].parse::<usize>().unwrap_or(default);
        (n, args.get(2).copied())
    } else {
        (default, args.first().copied())
    }
}

/// Recursively walks the VFS tree from `inode_idx` and collects full paths.
fn find_in_vfs(
    vfs: &virtual_fs::VirtualFileSystem,
    inode_idx: usize,
    current_path: &str,
    query: &str,
    results: &mut Vec<String>,
) {
    if inode_idx >= vfs.inodes.len() {
        return;
    }
    let inode = &vfs.inodes[inode_idx];
    if !inode.name.is_empty() && inode.name.contains(query) {
        results.push(String::from(current_path));
    }
    if let virtual_fs::InodeType::Directory { entries } = &inode.inode_type {
        for (name, child_idx) in entries {
            let child_path = if current_path == "/" {
                alloc::format!("/{}", name)
            } else {
                alloc::format!("{}/{}", current_path, name)
            };
            find_in_vfs(vfs, *child_idx, &child_path, query, results);
        }
    }
}

/// Splits a path into parent directory and child node name.
pub fn split_parent_child(path: &str) -> (String, String) {
    let path = path.trim_end_matches('/');
    if let Some(pos) = path.rfind('/') {
        let parent = &path[..pos];
        let parent = if parent.is_empty() { "/" } else { parent };
        let child = &path[pos + 1..];
        (String::from(parent), String::from(child))
    } else {
        (String::from("/"), String::from(path))
    }
}

/// Prints the structural contents of the virtual file system tree recursively.
pub fn print_vfs_tree(vfs: &virtual_fs::VirtualFileSystem, inode_idx: usize, indent: usize) {
    if inode_idx >= vfs.inodes.len() {
        return;
    }
    let inode = &vfs.inodes[inode_idx];
    let indent_str = "  ".repeat(indent);
    match &inode.inode_type {
        virtual_fs::InodeType::Directory { entries } => {
            if inode_idx == 0 {
                println!("\x1B[38;5;33m/\x1B[0m");
            } else {
                println!("{}{}\x1B[38;5;33m{}/\x1B[0m", indent_str, if indent > 0 { "├── " } else { "" }, inode.name);
            }
            for (_, child_idx) in entries {
                print_vfs_tree(vfs, *child_idx, indent + 1);
            }
        }
        virtual_fs::InodeType::File { size, .. } => {
            println!("{}{}{:<16} \x1B[38;5;246m({} bytes)\x1B[0m", indent_str, if indent > 0 { "├── " } else { "" }, inode.name, size);
        }
    }
}
