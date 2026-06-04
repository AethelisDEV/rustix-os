use crate::syscalls::{sys_open, sys_read, sys_write, sys_close, syscall0, sys_mkdir};
use crate::utils::{serial_print, StrbufWriter};
use crate::atlas_font::{draw_text_atlas, AtlasSize, AtlasWeight};

pub const TERM_MAX_ROWS: usize = 14;
pub const TERM_MAX_COLS: usize = 56;
pub static mut TERM_BUFFER: [[char; TERM_MAX_COLS]; TERM_MAX_ROWS] = [[' '; TERM_MAX_COLS]; TERM_MAX_ROWS];
pub static mut TERM_ROW: usize = 0;
pub static mut TERM_COL: usize = 0;

pub fn term_newline() {
    unsafe {
        TERM_COL = 0;
        if TERM_ROW < TERM_MAX_ROWS - 1 {
            TERM_ROW += 1;
        } else {
            for r in 0..(TERM_MAX_ROWS - 1) {
                TERM_BUFFER[r] = TERM_BUFFER[r + 1];
            }
            TERM_BUFFER[TERM_MAX_ROWS - 1] = [' '; TERM_MAX_COLS];
        }
    }
}

pub fn term_print_char(c: char) {
    unsafe {
        if c == '\n' {
            term_newline();
        } else if c == '\x08' {
            let mut prompt_end = 11;
            for col in 0..TERM_COL {
                if col + 1 < TERM_COL && TERM_BUFFER[TERM_ROW][col] == '>' && TERM_BUFFER[TERM_ROW][col + 1] == ' ' {
                    prompt_end = col + 2;
                }
            }
            if TERM_COL > prompt_end {
                TERM_COL -= 1;
                TERM_BUFFER[TERM_ROW][TERM_COL] = ' ';
            }
        } else {
            if TERM_COL < TERM_MAX_COLS {
                TERM_BUFFER[TERM_ROW][TERM_COL] = c;
                TERM_COL += 1;
            }
            if TERM_COL >= TERM_MAX_COLS {
                term_newline();
            }
        }
    }
}

pub fn term_print_str(s: &str) {
    for c in s.chars() {
        term_print_char(c);
    }
}

pub fn term_init() {
    term_print_str("AE Rustanium Flight DE (Ring 3 User Mode)\n");
    term_print_str("======================================\n");
    term_print_str("[OK] Framebuffer pages mapped successfully.\n");
    term_print_str("[OK] Shared System Info page bound (vDSO).\n");
    term_print_str("[OK] Event loop active. 16ms timeouts configured.\n");
    term_print_str("Type in standard serial port COM1 to interact.\n\n");
    term_print_str("desktop:/> ");
}

pub fn term_process_command() {
    unsafe {
        let mut line_buf = [0u8; 128];
        let mut line_len = 0;
        
        let mut start_col = 0;
        for col in 0..TERM_COL {
            if col + 1 < TERM_COL && TERM_BUFFER[TERM_ROW][col] == '>' && TERM_BUFFER[TERM_ROW][col + 1] == ' ' {
                start_col = col + 2;
            }
        }
        if start_col == 0 {
            start_col = if TERM_COL > 11 { 11 } else { TERM_COL };
        }

        if TERM_COL > start_col {
            for col in start_col..TERM_COL {
                if line_len < 128 {
                    line_buf[line_len] = TERM_BUFFER[TERM_ROW][col] as u8;
                    line_len += 1;
                }
            }
        }

        term_newline();

        let cmd_str = match core::str::from_utf8(&line_buf[0..line_len]) {
            Ok(s) => s.trim(),
            Err(_) => "",
        };

        // Debug output to COM1 serial console
        {
            let mut dbg_buf = [0u8; 128];
            let mut w = StrbufWriter::new(&mut dbg_buf[..]);
            let _ = core::fmt::write(&mut w, format_args!("[DE] Command received: '{}' (len={}, bytes={:?})\n", cmd_str, cmd_str.len(), cmd_str.as_bytes()));
            serial_print(w.as_str());
        }

        if cmd_str.eq_ignore_ascii_case("help") {
            term_print_str("Commands: help, about, clear, ls, mkdir, touch, cat, echo, exit\n");
        } else if cmd_str.eq_ignore_ascii_case("about") {
            term_print_str("AE Rustanium Flight Controller OS v1.0\n");
            term_print_str("Composed of Ring 0 Microkernel & Ring 3 DE.\n");
        } else if cmd_str.eq_ignore_ascii_case("clear") {
            TERM_BUFFER = [[' '; TERM_MAX_COLS]; TERM_MAX_ROWS];
            TERM_ROW = 0;
            TERM_COL = 0;
        } else if cmd_str.eq_ignore_ascii_case("exit") {
            term_print_str("Returning control to microkernel...\n");
            sys_close(3);
            syscall0(3);
        } else if cmd_str.eq_ignore_ascii_case("ls") || (cmd_str.len() >= 3 && cmd_str[0..3].eq_ignore_ascii_case("ls ")) {
            let path = if cmd_str.len() >= 3 { cmd_str[3..].trim() } else { "" };
            let path_to_open = if path.is_empty() { "/" } else { path };
            let fd = sys_open(path_to_open.as_ptr(), path_to_open.len(), 0);
            if fd == u64::MAX || fd >= 16 {
                term_print_str("Error: could not open directory.\n");
            } else {
                let mut dir_buf = [0u8; 2048];
                let bytes_read = sys_read(fd, dir_buf.as_mut_ptr(), 2048);
                if bytes_read == u64::MAX {
                    term_print_str("Error: could not read directory.\n");
                } else if bytes_read > 0 {
                    let slice = &dir_buf[..bytes_read as usize];
                    if let Ok(content_str) = core::str::from_utf8(slice) {
                        term_print_str(content_str);
                    }
                }
                sys_close(fd);
            }
        } else if cmd_str.len() >= 6 && cmd_str[0..6].eq_ignore_ascii_case("mkdir ") {
            let path = cmd_str[6..].trim();
            if path.is_empty() {
                term_print_str("Usage: mkdir <directory_path>\n");
            } else {
                let status = sys_mkdir(path.as_ptr(), path.len());
                if status == u64::MAX {
                    term_print_str("Error: mkdir failed.\n");
                } else {
                    term_print_str("Directory created successfully.\n");
                }
            }
        } else if cmd_str.len() >= 6 && cmd_str[0..6].eq_ignore_ascii_case("touch ") {
            let path = cmd_str[6..].trim();
            if path.is_empty() {
                term_print_str("Usage: touch <file_path>\n");
            } else {
                let fd = sys_open(path.as_ptr(), path.len(), 1);
                if fd == u64::MAX || fd >= 16 {
                    term_print_str("Error: touch failed.\n");
                } else {
                    sys_close(fd);
                }
            }
        } else if cmd_str.len() >= 4 && cmd_str[0..4].eq_ignore_ascii_case("cat ") {
            let path = cmd_str[4..].trim();
            if path.is_empty() {
                term_print_str("Usage: cat <file_path>\n");
            } else {
                let fd = sys_open(path.as_ptr(), path.len(), 0);
                if fd == u64::MAX || fd >= 16 {
                    term_print_str("Error: could not open file.\n");
                } else {
                    let mut io_buffer = [0u8; 4096];
                    let io_buf_ptr = io_buffer.as_mut_ptr();
                    core::ptr::write_bytes(io_buf_ptr, 0, 4096);

                    let bytes_read = sys_read(fd, io_buf_ptr, 4096);
                    if bytes_read == u64::MAX {
                        term_print_str("Error: could not read file.\n");
                    } else if bytes_read > 0 {
                        let slice = core::slice::from_raw_parts(io_buf_ptr as *const u8, bytes_read as usize);
                        if let Ok(content_str) = core::str::from_utf8(slice) {
                            term_print_str(content_str);
                            if !content_str.ends_with('\n') {
                                term_print_char('\n');
                            }
                        } else {
                            term_print_str("[Binary data]\n");
                        }
                    }
                    sys_close(fd);
                }
            }
        } else if cmd_str.len() >= 5 && cmd_str[0..5].eq_ignore_ascii_case("echo ") {
            let args = &cmd_str[5..];
            if let Some((content, path)) = args.split_once(" > ") {
                let content = content.trim();
                let path = path.trim();
                if path.is_empty() {
                    term_print_str("Usage: echo <text> > <file_path>\n");
                } else {
                    let fd = sys_open(path.as_ptr(), path.len(), 1);
                    if fd == u64::MAX || fd >= 16 {
                        term_print_str("Error: could not open file for writing.\n");
                    } else {
                        let bytes_written = sys_write(fd, content.as_ptr(), content.len());
                        if bytes_written == u64::MAX {
                            term_print_str("Error: could not write to file.\n");
                        }
                        sys_close(fd);
                    }
                }
            } else {
                term_print_str("Usage: echo <text> > <file_path>\n");
            }
        } else if !cmd_str.is_empty() {
            term_print_str("Command not recognized. Type 'help'.\n");
        }

        if !cmd_str.eq_ignore_ascii_case("clear") {
            term_print_str("desktop:/> ");
        }
    }
}

pub fn draw_console_window(ax: i32, ay: i32) {
    unsafe {
        for row in 0..TERM_MAX_ROWS {
            let ty = ay + 46 + (row as i32) * 18;
            let mut row_str = [0u8; TERM_MAX_COLS];
            for col in 0..TERM_MAX_COLS {
                row_str[col] = TERM_BUFFER[row][col] as u8;
            }
            if let Ok(s) = core::str::from_utf8(&row_str) {
                draw_text_atlas(ax + 18, ty, s, 195, 215, 240, AtlasSize::Small, AtlasWeight::Regular);
            }
        }
    }
}
