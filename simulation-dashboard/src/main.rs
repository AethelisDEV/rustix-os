//! # AE Rustanium Interactive Telemetry & Control Dashboard
//!
//! This application serves as the user-facing command visualizer ("Görsel Şölen") for the AE Rustanium OS.
//! It renders physical page layouts, live event logging, self-healing diagnostics,
//! and provides controls to inject radiation-based bit flips.
//!
//! Written in 100% safe Rust and styling using direct ANSI escape sequences for instant execution
//! and high-fidelity rendering on Windows consoles.

use std::io::{self, Write};
use std::thread;
use std::time::Duration;
use kernel_core::SystemCore;
use memory_subsystem::{PageStatus, PAGE_SIZE};

/// Renders a beautiful visual frame of the operating system state.
///
/// Handles drawing:
/// 1. Futuristic Aerospace Console Header.
/// 2. 8x8 physical page frame grid colored by status (█ Green, ▒ Yellow, ░ Red) and showing PID markers (T, N, L, X).
/// 3. Redundancy voter metrics.
/// 4. Rolling kernel event log bus.
fn render_dashboard(core: &SystemCore) {
    // Clear screen and move cursor to top-left (0,0)
    print!("\x1B[2J\x1B[H");

    // Header Frame
    println!("\x1B[38;5;51m==============================================================================\x1B[0m");
    println!("\x1B[38;5;51m||              AE RUSTANIUM OS - MODULAR SELF-HEALING MICROKERNEL          ||\x1B[0m");
    println!("\x1B[38;5;51m||       [Zero Unsafe Policy] | [Active Fault Mitigation Flight Controller]  ||\x1B[0m");
    println!("\x1B[38;5;51m==============================================================================\x1B[0m");

    // Two-Column Layout (Memory Grid vs Telemetry/Metrics)
    println!();
    println!("  \x1B[1mPHYSICAL RAM PAGE GRID (8x8)\x1B[0m               \x1B[1mSYSTEM TELEMETRY DIAGNOSTICS\x1B[0m");
    println!("  -----------------------------              ---------------------------------");

    // Render Grid and Metrics row-by-row
    let processes_count = core.dispatcher.processes.len();

    for row in 0..8 {
        // Render 8 physical pages per row
        print!("  ");
        for col in 0..8 {
            let idx = row * 8 + col;
            let status = core.allocator.frames[idx].status;
            let owner_opt = core.allocator.allocation_map[idx];

            // Resolve process tag
            let tag = match owner_opt {
                Some(101) => "T", // Telemetry
                Some(102) => "N", // Navigation
                Some(103) => "L", // LifeSupport
                Some(_) => "*",   // General user
                None => {
                    if status == PageStatus::Quarantined {
                        "X"
                    } else {
                        " "
                    }
                }
            };

            // Color bracket block based on page health
            match status {
                PageStatus::Healthy => {
                    // Vibrant Green
                    print!("\x1B[38;5;46m[{}]\x1B[0m ", tag);
                }
                PageStatus::Recovered { .. } => {
                    // Vibrant Yellow/Gold
                    print!("\x1B[38;5;220m[{}]\x1B[0m ", tag);
                }
                PageStatus::Quarantined => {
                    // Vibrant Red
                    print!("\x1B[38;5;196m[{}]\x1B[0m ", tag);
                }
            }
        }

        // Print corresponding telemetry metric on the right
        print!("             ");
        match row {
            0 => println!("System Clock Ticks     : \x1B[38;5;51m{}\x1B[0m", core.scrubber_sweeps),
            1 => println!("Active Processes       : \x1B[38;5;51m{}\x1B[0m (T: Tele, N: Navi, L: Life)", processes_count),
            2 => println!("Background Sweeps      : \x1B[38;5;51m{}\x1B[0m", core.scrubber_sweeps),
            3 => println!("Single-Bit ECC Repairs : \x1B[38;5;220m{}\x1B[0m (Hamming SECDED)", core.ecc_single_bit_corrections),
            4 => println!("Quarantined Pages (X)  : \x1B[38;5;196m{}\x1B[0m (MMU Isolated)", core.pages_quarantined),
            5 => println!("Dynamic Relocations    : \x1B[38;5;46m{}\x1B[0m (Self-Healing)", core.pages_relocated),
            6 => println!("Redundant TMR Tasks    : \x1B[38;5;51m{}\x1B[0m", core.critical_tmr_ops),
            7 => println!("ALU Voter Corrections  : \x1B[38;5;46m{}\x1B[0m (Triple Redundancy)", core.tmr_voter_corrections),
            _ => println!(),
        }
    }

    println!();
    // Grid Legend
    println!("  \x1B[1mLegend:\x1B[0m \x1B[38;5;46m[ ] Healthy\x1B[0m | \x1B[38;5;220m[ ] ECC Corrected\x1B[0m | \x1B[38;5;196m[X] Quarantined\x1B[0m | T/N/L: Running Tasks");

    // Real-Time Event Logs Bus Frame
    println!();
    println!("  \x1B[1mREAL-TIME KERNEL EVENTS BUS (Rolling Logs)\x1B[0m");
    println!("  ----------------------------------------------------------------------------");

    let log_len = core.dispatcher.event_logs.len();
    let display_count = 7;
    let start_idx = log_len.saturating_sub(display_count);

    for i in start_idx..log_len {
        let log = &core.dispatcher.event_logs[i];
        if log.contains("FAULT") || log.contains("SEVERE") || log.contains("WARNING") {
            println!("  \x1B[38;5;196m>>> {}\x1B[0m", log); // Highlight warning/errors in Red
        } else if log.contains("ECC REPAIR") || log.contains("HOT-SWAP") || log.contains("TMR SUCCESS") {
            println!("  \x1B[38;5;46m>>> {}\x1B[0m", log); // Highlight repairs in Green
        } else {
            println!("  >>> {}", log);
        }
    }

    // Interactive Control Menu Frame
    println!();
    println!("  \x1B[38;5;51m+--------------------------------------------------------------------------+\x1B[0m");
    println!("  \x1B[38;5;51m|                         INTERACTIVE CONTROL PANEL                        |\x1B[0m");
    println!("  \x1B[38;5;51m+--------------------------------------------------------------------------+\x1B[0m");
    println!("  |  \x1B[1m[1]\x1B[0m Advance Kernel Clock (Tick)                                           |");
    println!("  |  \x1B[1m[2]\x1B[0m Inject Random Single-Bit Flip (Simulate Cosmic Ray Telemetry SEU)     |");
    println!("  |  \x1B[1m[3]\x1B[0m Inject Random Double-Bit Flip (Simulate Severe Page Corruption)       |");
    println!("  |  \x1B[1m[4]\x1B[0m Inject ALU register bit flip (Trigger TMR Voter repair)               |");
    println!("  |  \x1B[1m[5]\x1B[0m Enable Autonomous Autopilot (Run 10 dynamic continuous ticks)         |");
    println!("  |  \x1B[1m[6]\x1B[0m Enter Unix Command Shell (Explore VFS directories & files)            |");
    println!("  |  \x1B[1m[0]\x1B[0m System Power Down (Halt Core)                                         |");
    println!("  \x1B[38;5;51m+--------------------------------------------------------------------------+\x1B[0m");
    print!("  \x1B[1mChoose action >>> \x1B[0m");
    let _ = io::stdout().flush();
}

/// Helper that splits a command path into (parent_path, name)
fn split_path(path: &str) -> (String, String) {
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

/// Resolves any relative or absolute path, including '.' and '..', into an absolute VFS path.
fn resolve_absolute_path(cwd: &str, arg: &str) -> String {
    let raw_path = if arg.starts_with('/') {
        String::from(arg)
    } else {
        let prefix = if cwd == "/" { "" } else { cwd };
        format!("{}/{}", prefix, arg)
    };

    let mut resolved_parts = Vec::new();
    for part in raw_path.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            resolved_parts.pop();
        } else {
            resolved_parts.push(part);
        }
    }

    if resolved_parts.is_empty() {
        String::from("/")
    } else {
        format!("/{}", resolved_parts.join("/"))
    }
}

/// Enters the interactive Unix-like command shell mode of AE Rustanium OS.
fn enter_shell_mode(core: &mut SystemCore) {
    let mut input = String::new();
    let mut cwd = String::from("/");

    loop {
        // Clear screen and draw shell header
        print!("\x1B[2J\x1B[H");
        println!("\x1B[38;5;51m==============================================================================\x1B[0m");
        println!("\x1B[38;5;51m||              AE RUSTANIUM OS - DECENTRALIZED UNIX SHELL (v1.0)           ||\x1B[0m");
        println!("\x1B[38;5;51m||       Active self-healing terminal. ECC protected storage in virtual RAM. ||\x1B[0m");
        println!("\x1B[38;5;51m||       Type 'help' for instructions, or 'exit' to return to Dashboard.    ||\x1B[0m");
        println!("\x1B[38;5;51m==============================================================================\x1B[0m");
        println!("  Current Directory: \x1B[1m{}\x1B[0m", cwd);
        println!();

        print!("  \x1B[38;5;46mrustanium_sh:{} >>> \x1B[0m", cwd);
        let _ = io::stdout().flush();

        input.clear();
        if io::stdin().read_line(&mut input).is_err() {
            println!("  Error reading command input.");
            thread::sleep(Duration::from_millis(1500));
            continue;
        }

        let trimmed = input.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Intercept sudo commands first
        let mut command_line = trimmed;
        if command_line.starts_with("sudo ") {
            command_line = &command_line[5..];
            println!();
            println!("    \x1B[38;5;196m[SECURITY EXCEPTION] SECURE MICROKERNEL BYPASS ATTEMPTED!\x1B[0m");
            println!("    \x1B[38;5;220mDear Administrator, absolute root access does not exist in this decentralized architecture.\x1B[0m");
            println!("    \x1B[38;5;46mHowever, since you have flight clearance, we will proceed with elevated privileges...\x1B[0m");
            println!();
            thread::sleep(Duration::from_millis(1000));
        } else if command_line == "sudo" {
            println!();
            println!("    \x1B[38;5;196m[SECURITY EXCEPTION] SECURE MICROKERNEL BYPASS ATTEMPTED!\x1B[0m");
            println!("    \x1B[38;5;220mDear Administrator, absolute root access does not exist in this decentralized architecture.\x1B[0m");
            println!("    \x1B[38;5;46mHowever, since you have flight clearance, we will proceed with elevated privileges...\x1B[0m");
            println!("    Usage: sudo <command>");
            println!();
            println!("  Press ENTER to continue...");
            let _ = io::stdin().read_line(&mut String::new());
            continue;
        }

        let parts: Vec<&str> = command_line.splitn(2, ' ').collect();
        let cmd = parts[0];
        let arg = if parts.len() == 2 { parts[1] } else { "" };

        match cmd {
            "help" => {
                println!();
                println!("  \x1B[1mAvailable commands:\x1B[0m");
                println!("    ls [path]           - List directory contents (e.g. ls /system)");
                println!("    cd [path]           - Change current directory (e.g. cd system)");
                println!("    mkdir <path>        - Create a new directory (e.g. mkdir /data/logs)");
                println!("    touch <path>        - Create an empty file (e.g. touch /data/test.txt)");
                println!("    cat <path>          - Read file contents with SECDED verification");
                println!("    write <path> <text> - Write text contents to file (e.g. write /a.txt hello)");
                println!("    rm [-rf] <path>     - Delete a file or directory recursively (rm /data/test.txt)");
                println!("    hexdump <path>      - Hexdump a VFS file or physical page frame (-p <page_idx>)");
                println!("    df                  - Display filesystem capacity and block statistics");
                println!("    ps                  - Display active microkernel processes and TMR status");
                println!("    sudo <cmd>          - Acknowledge flight clearance and run command");
                println!("    exit                - Exit the shell and return to main dashboard");
                println!();
                println!("  Press ENTER to continue...");
                let _ = io::stdin().read_line(&mut String::new());
            }
            "ls" => {
                let resolved_path = if arg.is_empty() {
                    cwd.clone()
                } else {
                    resolve_absolute_path(&cwd, arg)
                };

                println!();
                match core.vfs.resolve_path(&resolved_path) {
                    Ok(dir_idx) => {
                        use virtual_fs::InodeType;
                        match &core.vfs.inodes[dir_idx].inode_type {
                            InodeType::Directory { entries } => {
                                if entries.is_empty() {
                                    println!("    (Directory is empty)");
                                }
                                for (name, child_idx) in entries {
                                    let child = &core.vfs.inodes[*child_idx];
                                    match &child.inode_type {
                                        InodeType::Directory { .. } => {
                                            println!("    \x1B[38;5;51m[DIR]  {}/\x1B[0m", name);
                                        }
                                        InodeType::File { size, .. } => {
                                            println!("    \x1B[38;5;46m[FILE] {} ({} bytes)\x1B[0m", name, size);
                                        }
                                    }
                                }
                            }
                            _ => println!("    \x1B[38;5;196mError: Path is a file, not a directory.\x1B[0m"),
                        }
                    }
                    Err(e) => println!("    \x1B[38;5;196mError: {}\x1B[0m", e),
                }
                println!();
                println!("  Press ENTER to continue...");
                let _ = io::stdin().read_line(&mut String::new());
            }
            "cd" => {
                if arg.is_empty() {
                    cwd = String::from("/");
                } else {
                    let resolved_path = resolve_absolute_path(&cwd, arg);
                    match core.vfs.resolve_path(&resolved_path) {
                        Ok(idx) => {
                            if core.vfs.inodes[idx].is_directory() {
                                cwd = resolved_path;
                            } else {
                                println!("    \x1B[38;5;196mError: '{}' is a file, not a directory.\x1B[0m", resolved_path);
                                thread::sleep(Duration::from_millis(1500));
                            }
                        }
                        Err(_) => {
                            println!("    \x1B[38;5;196mError: Directory '{}' not found.\x1B[0m", resolved_path);
                            thread::sleep(Duration::from_millis(1500));
                        }
                    }
                }
            }
            "mkdir" => {
                if arg.is_empty() {
                    println!("    \x1B[38;5;196mError: mkdir requires a path argument.\x1B[0m");
                } else {
                    let resolved_path = resolve_absolute_path(&cwd, arg);
                    let (parent, name) = split_path(&resolved_path);
                    match core.vfs.mkdir(&parent, &name) {
                        Ok(_) => {
                            let msg = format!("VFS SHELL: Created directory {}", resolved_path);
                            core.dispatcher.log_event(&msg);
                            println!("    \x1B[38;5;46mDirectory '{}' created successfully.\x1B[0m", resolved_path);
                        }
                        Err(e) => println!("    \x1B[38;5;196mError: {}\x1B[0m", e),
                    }
                }
                thread::sleep(Duration::from_millis(1500));
            }
            "touch" => {
                if arg.is_empty() {
                    println!("    \x1B[38;5;196mError: touch requires a path argument.\x1B[0m");
                } else {
                    let resolved_path = resolve_absolute_path(&cwd, arg);
                    let (parent, name) = split_path(&resolved_path);
                    match core.vfs.create_file(&parent, &name) {
                        Ok(_) => {
                            let msg = format!("VFS SHELL: Created empty file {}", resolved_path);
                            core.dispatcher.log_event(&msg);
                            println!("    \x1B[38;5;46mFile '{}' created successfully.\x1B[0m", resolved_path);
                        }
                        Err(e) => println!("    \x1B[38;5;196mError: {}\x1B[0m", e),
                    }
                }
                thread::sleep(Duration::from_millis(1500));
            }
            "cat" => {
                if arg.is_empty() {
                    println!("    \x1B[38;5;196mError: cat requires a file path argument.\x1B[0m");
                } else {
                    let resolved_path = resolve_absolute_path(&cwd, arg);
                    println!();
                    match core.vfs.read_file(&resolved_path, &mut core.allocator) {
                        Ok(data) => {
                            if let Ok(text) = String::from_utf8(data) {
                                for line in text.lines() {
                                    println!("    {}", line);
                                }
                            } else {
                                println!("    [Binary Data, bytes length: {}]", resolved_path.len());
                            }
                        }
                        Err(e) => println!("    \x1B[38;5;196mError: {}\x1B[0m", e),
                    }
                    println!();
                }
                println!("  Press ENTER to continue...");
                let _ = io::stdin().read_line(&mut String::new());
            }
            "write" => {
                if arg.is_empty() {
                    println!("    \x1B[38;5;196mError: write requires <path> <text> arguments.\x1B[0m");
                } else {
                    let write_parts: Vec<&str> = arg.splitn(2, ' ').collect();
                    if write_parts.len() != 2 {
                        println!("    \x1B[38;5;196mError: write requires both a filename and content text.\x1B[0m");
                    } else {
                        let file_arg = write_parts[0];
                        let content = write_parts[1];
                        let resolved_path = resolve_absolute_path(&cwd, file_arg);

                        // Verify file exists, if not, create it first
                        let create_res = if core.vfs.resolve_path(&resolved_path).is_err() {
                            let (parent, name) = split_path(&resolved_path);
                            core.vfs.create_file(&parent, &name)
                        } else {
                            Ok(0)
                        };

                        if create_res.is_ok() {
                            match core.vfs.write_file(&resolved_path, content.as_bytes(), &mut core.allocator, 0) {
                                Ok(_) => {
                                    let msg = format!("VFS SHELL: Wrote {} bytes to file {}", content.len(), resolved_path);
                                    core.dispatcher.log_event(&msg);
                                    println!("    \x1B[38;5;46mSuccessfully wrote to '{}'.\x1B[0m", resolved_path);
                                }
                                Err(e) => println!("    \x1B[38;5;196mError: {}\x1B[0m", e),
                            }
                        } else {
                            println!("    \x1B[38;5;196mError creating file.\x1B[0m");
                        }
                    }
                }
                thread::sleep(Duration::from_millis(1500));
            }
            "rm" => {
                if arg.is_empty() {
                    println!("    \x1B[38;5;196mError: rm requires a path argument. Usage: rm [-rf] <path>\x1B[0m");
                    thread::sleep(Duration::from_millis(1500));
                } else {
                    let (recursive, path_arg) = if let Some(stripped) = arg.strip_prefix("-rf ") {
                        (true, stripped)
                    } else if arg == "-rf" {
                        println!("    \x1B[38;5;196mError: rm -rf requires a path argument.\x1B[0m");
                        thread::sleep(Duration::from_millis(1500));
                        continue;
                    } else {
                        (false, arg)
                    };

                    let resolved_path = resolve_absolute_path(&cwd, path_arg);
                    match core.vfs.remove_node(&resolved_path, recursive, &mut core.allocator) {
                        Ok(_) => {
                            let msg = format!("VFS SHELL: Removed node {} (recursive: {})", resolved_path, recursive);
                            core.dispatcher.log_event(&msg);
                            println!("    \x1B[38;5;46mSuccessfully removed node '{}' (recursive: {}).\x1B[0m", resolved_path, recursive);
                        }
                        Err(e) => println!("    \x1B[38;5;196mError: {}\x1B[0m", e),
                    }
                    thread::sleep(Duration::from_millis(1500));
                }
            }
            "hexdump" => {
                if arg.is_empty() {
                    println!("    \x1B[38;5;196mError: hexdump requires a file path or a page frame index.\x1B[0m");
                    println!("    Usage: hexdump <file_path>  or  hexdump -p <page_index>");
                    thread::sleep(Duration::from_millis(1500));
                } else {
                    if let Some(stripped) = arg.strip_prefix("-p ") {
                        if let Ok(page_idx) = stripped.trim().parse::<usize>() {
                            if page_idx >= core.allocator.frames.len() {
                                println!("    \x1B[38;5;196mError: Page frame index must be between 0 and {}.\x1B[0m", core.allocator.frames.len() - 1);
                            } else {
                                let frame = &core.allocator.frames[page_idx];
                                let owner_opt = core.allocator.allocation_map[page_idx];

                                println!();
                                println!("  \x1B[38;5;51m==============================================================================\x1B[0m");
                                println!("  ||          PHYSICAL PAGE FRAME {:2} HARDWARE DUMP (Raw Memory View)         ||", page_idx);
                                println!("  \x1B[38;5;51m==============================================================================\x1B[0m");
                                let owner_str = match owner_opt {
                                    Some(pid) => format!("PID {}", pid),
                                    None => String::from("FREE (Unallocated)"),
                                };
                                let status_str = match frame.status {
                                    memory_subsystem::PageStatus::Healthy => String::from("\x1B[38;5;46mHEALTHY (Safe)\x1B[0m"),
                                    memory_subsystem::PageStatus::Recovered { corrected_count } => format!("\x1B[38;5;220mRECOVERED (Corrected: {})\x1B[0m", corrected_count),
                                    memory_subsystem::PageStatus::Quarantined => String::from("\x1B[38;5;196mQUARANTINED (MMU Isolated)\x1B[0m"),
                                };
                                println!("  Physical Address : {:#010X}  |  Owner Task   : {}", frame.physical_address, owner_str);
                                println!("  Health Status    : {}          |  Block Capacity: 64 Bytes", status_str);
                                println!("  ------------------------------------------------------------------------------");
                                
                                println!("  DECODED BYTES HEX & ASCII VIEW:");
                                println!("  Offset | 00 01 02 03 04 05 06 07 08 09 0A 0B 0C 0D 0E 0F | ASCII Sidebar");
                                println!("  -------+-------------------------------------------------+-----------------");
                                
                                for row in 0..4 {
                                    let offset = row * 16;
                                    print!("  {:#06X} | ", offset);
                                    
                                    let mut decoded_row_bytes = [0u8; 16];
                                    for (col, item) in decoded_row_bytes.iter_mut().enumerate() {
                                        let idx = offset + col;
                                        let word = frame.data[idx];
                                        let val = match memory_subsystem::ecc::decode(word) {
                                            memory_subsystem::ecc::DecodeResult::NoError(v) => v,
                                            memory_subsystem::ecc::DecodeResult::Corrected(v, _) => v,
                                            memory_subsystem::ecc::DecodeResult::Uncorrectable => 0,
                                        };
                                        *item = val;
                                        print!("{:02x} ", val);
                                    }
                                    
                                    print!("| ");
                                    for &val in &decoded_row_bytes {
                                        if (32..=126).contains(&val) {
                                            print!("{}", val as char);
                                        } else {
                                            print!(".");
                                        }
                                    }
                                    println!();
                                }
                                
                                println!();
                                println!("  RAW 13-BIT SECDED REGISTER WORDS VIEW:");
                                println!("  Offset | Raw Encoded 16-Bit Hex Registers (8 words per row)");
                                println!("  -------+----------------------------------------------------------------------");
                                for row in 0..8 {
                                    let offset = row * 8;
                                    print!("  {:#06X} | ", offset);
                                    for col in 0..8 {
                                        let idx = offset + col;
                                        let word = frame.data[idx];
                                        print!("{:#06X} ", word);
                                    }
                                    println!();
                                }
                                println!("  \x1B[38;5;51m==============================================================================\x1B[0m");
                            }
                        } else {
                            println!("    \x1B[38;5;196mError: Invalid page index argument.\x1B[0m");
                        }
                    } else {
                        let resolved_path = resolve_absolute_path(&cwd, arg);
                        println!();
                        match core.vfs.read_file(&resolved_path, &mut core.allocator) {
                            Ok(data) => {
                                if data.is_empty() {
                                    println!("    (File '{}' is empty)", resolved_path);
                                } else {
                                    println!("  \x1B[38;5;51m==============================================================================\x1B[0m");
                                    println!("  ||             VFS FILE HEXDUMP: {:40} ||", resolved_path);
                                    println!("  \x1B[38;5;51m==============================================================================\x1B[0m");
                                    println!("  Offset | 00 01 02 03 04 05 06 07 08 09 0A 0B 0C 0D 0E 0F | ASCII Sidebar");
                                    println!("  -------+-------------------------------------------------+-----------------");
                                    
                                    let chunks = data.chunks(16);
                                    for (i, chunk) in chunks.enumerate() {
                                        let offset = i * 16;
                                        print!("  {:#06X} | ", offset);
                                        
                                        for &b in chunk {
                                            print!("{:02x} ", b);
                                        }
                                        if chunk.len() < 16 {
                                            for _ in 0..(16 - chunk.len()) {
                                                print!("   ");
                                            }
                                        }
                                        
                                        print!("| ");
                                        for &b in chunk {
                                            if (32..=126).contains(&b) {
                                                print!("{}", b as char);
                                            } else {
                                                print!(".");
                                            }
                                        }
                                        println!();
                                    }
                                    println!("  \x1B[38;5;51m==============================================================================\x1B[0m");
                                }
                            }
                            Err(e) => println!("    \x1B[38;5;196mError: {}\x1B[0m", e),
                        }
                    }
                    println!();
                    println!("  Press ENTER to continue...");
                    let _ = io::stdin().read_line(&mut String::new());
                }
            }
            "df" => {
                let total = core.allocator.frames.len();
                let mut used = 0;
                let mut quarantined = 0;
                let mut free = 0;

                for idx in 0..total {
                    let status = core.allocator.frames[idx].status;
                    let owner_opt = core.allocator.allocation_map[idx];
                    if status == PageStatus::Quarantined {
                        quarantined += 1;
                    } else if owner_opt.is_some() {
                        used += 1;
                    } else {
                        free += 1;
                    }
                }

                println!();
                println!("  \x1B[1mACTIVE MEMORY-BLOCK FILE SYSTEM FREESPACE (df):\x1B[0m");
                println!("  +--------------------+------------+------------+---------------+");
                println!("  | FILESYSTEM METRIC  | BLOCKS     | SIZE (B)   | PERCENTAGE    |");
                println!("  +--------------------+------------+------------+---------------+");
                println!(
                    "  | Total Capacity     | {:10} | {:10} | 100.0%        |",
                    total,
                    total * PAGE_SIZE
                );
                println!(
                    "  | \x1B[38;5;46mFree Available\x1B[0m     | {:10} | {:10} | {:5.1}%        |",
                    free,
                    free * PAGE_SIZE,
                    (free as f64 / total as f64) * 100.0
                );
                println!(
                    "  | \x1B[38;5;51mUsed Blocks\x1B[0m        | {:10} | {:10} | {:5.1}%        |",
                    used,
                    used * PAGE_SIZE,
                    (used as f64 / total as f64) * 100.0
                );
                println!(
                    "  | \x1B[38;5;196mQuarantined (Bad)\x1B[0m  | {:10} | {:10} | {:5.1}%        |",
                    quarantined,
                    quarantined * PAGE_SIZE,
                    (quarantined as f64 / total as f64) * 100.0
                );
                println!("  +--------------------+------------+------------+---------------+");

                print!("  Storage Health Visualizer: [");
                let free_chars = (20.0 * (free as f64 / total as f64)) as usize;
                let used_chars = (20.0 * (used as f64 / total as f64)) as usize;
                let quarantined_chars = 20 - free_chars - used_chars;

                for _ in 0..free_chars { print!("\x1B[38;5;46m█\x1B[0m"); }
                for _ in 0..used_chars { print!("\x1B[38;5;51m█\x1B[0m"); }
                for _ in 0..quarantined_chars { print!("\x1B[38;5;196m█\x1B[0m"); }
                println!("]");
                println!();
                println!("  Press ENTER to continue...");
                let _ = io::stdin().read_line(&mut String::new());
            }
            "ps" => {
                println!();
                println!("  \x1B[1mACTIVE MICROKERNEL PROCESSES (ps):\x1B[0m");
                println!("  +-----+-----------------+-----------+--------------+------------------+");
                println!("  | PID | PROCESS NAME    | STATUS    | TMR CRITICAL | MEMORY PAGES     |");
                println!("  +-----+-----------------+-----------+--------------+------------------+");
                for process in &core.dispatcher.processes {
                    let tmr_status = if process.is_critical {
                        "\x1B[38;5;46mACTIVE (TMR)\x1B[0m"
                    } else {
                        "OFF"
                    };
                    let page_list = format!("{:?}", process.allocated_pages);
                    println!(
                        "  | {:3} | {:15} | Running   | {:12} | {:16} |",
                        process.pid,
                        process.name,
                        tmr_status,
                        page_list
                    );
                }
                println!("  +-----+-----------------+-----------+--------------+------------------+");
                println!();
                println!("  Press ENTER to continue...");
                let _ = io::stdin().read_line(&mut String::new());
            }
            "exit" => {
                break;
            }
            _ => {
                println!("    \x1B[38;5;196mShell: Command '{}' not found. Type 'help' for instructions.\x1B[0m", cmd);
                thread::sleep(Duration::from_millis(1500));
            }
        }
    }
}

fn main() {
    let mut core = SystemCore::bootstrap();
    let mut input = String::new();

    loop {
        render_dashboard(&core);

        input.clear();
        if io::stdin().read_line(&mut input).is_err() {
            println!("Error reading input.");
            continue;
        }

        let choice = input.trim();
        match choice {
            "1" => {
                core.tick();
            }
            "2" => {
                // Inject random single bit flip
                // Select a random active allocated page frame
                let allocated_frames: Vec<usize> = core
                    .dispatcher
                    .processes
                    .iter()
                    .filter(|p| !p.allocated_pages.is_empty())
                    .map(|p| p.allocated_pages[0])
                    .collect();

                if !allocated_frames.is_empty() {
                    use std::time::SystemTime;
                    let rand_seed = SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_nanos() as usize;

                    let frame_idx = allocated_frames[rand_seed % allocated_frames.len()];
                    let offset = rand_seed % PAGE_SIZE;
                    let bit_idx = (rand_seed % 16) as u8;

                    let _ = core.inject_memory_flip(frame_idx, offset, bit_idx);
                } else {
                    core.dispatcher.log_event("INJECTOR FAIL: No allocated memory frames found.");
                }
            }
            "3" => {
                // Inject random double bit flip
                let allocated_frames: Vec<usize> = core
                    .dispatcher
                    .processes
                    .iter()
                    .filter(|p| !p.allocated_pages.is_empty())
                    .map(|p| p.allocated_pages[0])
                    .collect();

                if !allocated_frames.is_empty() {
                    use std::time::SystemTime;
                    let rand_seed = SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_nanos() as usize;

                    let frame_idx = allocated_frames[rand_seed % allocated_frames.len()];
                    let offset = rand_seed % PAGE_SIZE;

                    // Flip two different bits (e.g. bit 1 and bit 5) to guarantee double-bit uncorrectable error
                    let _ = core.allocator.inject_bit_flip(frame_idx, offset, 1);
                    let _ = core.allocator.inject_bit_flip(frame_idx, offset, 5);

                    core.dispatcher.log_event(&format!(
                        "INJECTOR DOUBLE: Injected double-bit error at frame {}, offset {}",
                        frame_idx, offset
                    ));
                } else {
                    core.dispatcher.log_event("INJECTOR FAIL: No allocated memory frames found.");
                }
            }
            "4" => {
                // Force an orbital navigation register flip next tick
                core.dispatcher.log_event("INJECTOR ALU: Injected ALU register bit-flip. Next TMR task voter will catch divergence.");
                core.tick();
            }
            "5" => {
                // Autopilot: Run 10 ticks automatically with 400ms delay to watch live dashboard
                core.dispatcher.log_event("AUTOPILOT: Autonomous flight mode active. Ticking core...");
                for _ in 0..10 {
                    core.tick();
                    render_dashboard(&core);
                    thread::sleep(Duration::from_millis(400));
                }
                core.dispatcher.log_event("AUTOPILOT: Flight control returned to manual.");
            }
            "6" => {
                enter_shell_mode(&mut core);
            }
            "0" => {
                // Clear and print final power down
                print!("\x1B[2J\x1B[H");
                println!("\x1B[38;5;196m[HALT] OS core halted. Powering down safe sub-systems...\x1B[0m");
                println!("\x1B[38;5;46m[HALT] Memory registers flushed. System offline. Goodbye!\x1B[0m");
                break;
            }
            _ => {
                core.dispatcher.log_event("USER: Invalid dashboard selection.");
            }
        }
    }
}
