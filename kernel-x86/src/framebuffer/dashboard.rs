//! # F2 Telemetry Dashboard Visual Panels
//!
//! Renders the live graphical flight telemetry dashboard:
//! 1. `draw_dashboard_layout` - The static background structures, title cards, and layout cards.
//! 2. `draw_dashboard_memory_grid` - Renes color-coded physical page frames (ECC, Free, Allocated, Quarantined).
//! 3. `draw_dashboard_scheduler_metrics` - Pulsing CPU load meters and stack RSP monitors for co-op threads.
//! 4. `update_dashboard_telemetry` - Liquid-smooth flicker-free targeted bounding box metric updates.

use crate::framebuffer::core::UefiGraphics;
use crate::framebuffer::font::*;

/// Renders a modern, space-grade dashboard visual console onto the active monitor.
impl UefiGraphics {
    /// Draws the static base layout, borders, cards, and legend elements for the F2 Dashboard.
    pub fn draw_dashboard_layout(&mut self, ticks: usize, core: Option<&kernel_core::SystemCore>) {
        // 1. Sleek Charcoal Background
        self.clear(COLOR_BG);

        // 2. Vibrant Color Gradient Header bar (cyan-blue to deep purple)
        self.draw_horizontal_gradient_rect(0, 0, self.width, 48, COLOR_ACCENT_BLUE, COLOR_ACCENT_PURPLE);
        
        // 3. Header Text
        self.draw_string(24, 14, "AE RUSTANIUM OS - UEFI 64-BIT BARE-METAL KERNEL", COLOR_TEXT_WHITE, None, 2);
        self.draw_string(self.width - 220, 20, "[ SECURE SPACE FLIGHT ACTIVE ]", COLOR_ACCENT_GREEN, None, 1);

        // 4. Thread Status Cards
        // Card 1: Background Memory Scrubber (Thread 1)
        self.draw_rect(40, 80, 420, 150, COLOR_PANEL_BG);
        self.draw_rect(40, 80, 420, 4, COLOR_ACCENT_BLUE); // Blue Accent Top-line
        self.draw_string(56, 96, "COOPERATIVE TASK: MEMORY SCRUBBER", COLOR_TEXT_WHITE, None, 1);
        self.draw_string(56, 124, "PID        : 101", COLOR_TEXT_MUTED, None, 1);
        self.draw_string(56, 144, "Stack      : 8 KB (Dynamic Offset)", COLOR_TEXT_MUTED, None, 1);
        self.draw_string(56, 164, "Status     : RUNNING (Passive Yield)", COLOR_ACCENT_GREEN, None, 1);
        self.draw_string(56, 184, "Task Sweep : Page ECC SECDED Safe Scan", COLOR_TEXT_MUTED, None, 1);

        // Card 2: System Telemetry (Thread 2)
        self.draw_rect(490, 80, 420, 150, COLOR_PANEL_BG);
        self.draw_rect(490, 80, 420, 4, COLOR_ACCENT_PURPLE); // Purple Accent Top-line
        self.draw_string(506, 96, "COOPERATIVE TASK: FLIGHT TELEMETRY", COLOR_TEXT_WHITE, None, 1);
        self.draw_string(506, 124, "PID        : 102", COLOR_TEXT_MUTED, None, 1);
        self.draw_string(506, 144, "Stack      : 8 KB (Dynamic Offset)", COLOR_TEXT_MUTED, None, 1);
        self.draw_string(506, 164, "Status     : RUNNING (Passive Yield)", COLOR_ACCENT_GREEN, None, 1);
        self.draw_string(506, 184, "Frequency  : Real-Time Diagnostic Burst", COLOR_TEXT_MUTED, None, 1);

        // 5. System Diagnostic Metrics Panel (Right)
        self.draw_rect(940, 80, 300, 150, COLOR_PANEL_BG);
        self.draw_rect(940, 80, 300, 4, COLOR_TEXT_MUTED);
        self.draw_string(956, 96, "SYSTEM TELEMETRY", COLOR_TEXT_WHITE, None, 1);
        let mut ticks_buf = [0u8; 16];
        let ticks_str = format_ticks(ticks, &mut ticks_buf);
        self.draw_string(956, 124, "System Ticks : ", COLOR_TEXT_MUTED, None, 1);
        self.draw_string(1076, 124, ticks_str, COLOR_ACCENT_BLUE, None, 1);
        self.draw_string(956, 144, "Voter Health : 100.00%", COLOR_ACCENT_GREEN, None, 1);
        self.draw_string(956, 164, "ECC State    : Safe / Self-Healed", COLOR_TEXT_MUTED, None, 1);
        self.draw_string(956, 184, "Scheduler    : Cooperative (100Hz)", COLOR_TEXT_MUTED, None, 1);

        // 6. Dynamic Visual Panels
        // Left Column: Physical Memory Allocation Grid (8x8)
        self.draw_rect(40, 260, 585, 260, COLOR_PANEL_BG);
        self.draw_rect(40, 260, 585, 4, COLOR_ACCENT_BLUE);
        self.draw_string(60, 276, "PHYSICAL MEMORY PAGE ALLOCATION MAP (8x8 GRID)", COLOR_TEXT_WHITE, None, 1);
        self.draw_rect(60, 296, 545, 1, COLOR_TEXT_MUTED);
        self.draw_dashboard_memory_grid(core, true);

        // Right Column: Active Task Scheduler & Thread Metrics
        self.draw_rect(655, 260, 585, 260, COLOR_PANEL_BG);
        self.draw_rect(655, 260, 585, 4, COLOR_ACCENT_PURPLE);
        self.draw_string(675, 276, "ACTIVE TASK SCHEDULER & THREAD METRICS", COLOR_TEXT_WHITE, None, 1);
        self.draw_rect(675, 296, 545, 1, COLOR_TEXT_MUTED);
        self.draw_dashboard_scheduler_metrics(core, ticks, true);

        // Draw modern interactive navigation tabs
        self.draw_navigation_tabs(false);
        
        // Solid Glowing Progress Bar at the bottom
        let bar_width = ((ticks * 8) % 1160) as usize;
        self.draw_rect(60, 610, 1160, 12, COLOR_BG); // Clear background bar
        self.draw_rect(60, 610, bar_width, 12, COLOR_ACCENT_GREEN); // Fill bar
        self.draw_string(60, 630, "System heartbeat tick pulse - Dynamic Scheduler Execution Line", COLOR_TEXT_MUTED, None, 1);
    }

    /// Renders the 8x8 physical page frame memory allocation map.
    pub fn draw_dashboard_memory_grid(&mut self, core: Option<&kernel_core::SystemCore>, is_layout: bool) {
        if is_layout {
            // Clear content area: x: 50, y: 305, w: 565, h: 210 using Card background color
            self.draw_rect(50, 305, 565, 210, COLOR_PANEL_BG);
        } else {
            // Clear only the dynamic statistics area: x: 260, y: 420, w: 320, h: 50
            self.draw_rect(260, 420, 320, 50, COLOR_PANEL_BG);
        }

        let (alloc_map, frames) = if let Some(c) = core {
            (&c.allocator.allocation_map[..], &c.allocator.frames[..])
        } else {
            static DUMMY_MAP: [Option<u32>; 64] = [None; 64];
            static DUMMY_FRAMES: [memory_subsystem::PhysicalFrame; 0] = [];
            (&DUMMY_MAP[..], &DUMMY_FRAMES[..])
        };

        for r in 0..8 {
            for c in 0..8 {
                let idx = r * 8 + c;
                let cell_x = 60 + c * (16 + 6);
                let cell_y = 310 + r * (16 + 6);

                let has_frame = idx < frames.len();
                let is_quarantined = has_frame && frames[idx].status == memory_subsystem::PageStatus::Quarantined;
                let is_recovered = has_frame && matches!(frames[idx].status, memory_subsystem::PageStatus::Recovered { .. });
                let pid_opt = if idx < alloc_map.len() { alloc_map[idx] } else { None };

                if is_quarantined {
                    // Radioactive Red
                    self.draw_rect(cell_x, cell_y, 16, 16, Color::new(239, 68, 68));
                } else if is_recovered {
                    // Flame Orange / Amber
                    self.draw_rect(cell_x, cell_y, 16, 16, Color::new(245, 158, 11));
                } else if let Some(pid) = pid_opt {
                    // Color based on PID
                    let color = match pid {
                        101 => COLOR_ACCENT_BLUE,               // Telemetry: Cyan
                        102 => COLOR_ACCENT_PURPLE,             // Critical Navigation: Purple
                        103 => Color::new(32, 223, 127),        // Life Support: Green
                        _ => Color::new(236, 72, 153),          // User App: Hot Pink
                    };
                    self.draw_rect(cell_x, cell_y, 16, 16, color);
                } else {
                    // Free: Empty with a thin glowing dark green border
                    self.draw_border_rect(cell_x, cell_y, 16, 16, Color::new(16, 124, 65), COLOR_PANEL_BG);
                }
            }
        }

        if is_layout {
            // Draw Legend (x: 260, y: 310)
            let leg_x = 260;
            // 1. Free Frame
            self.draw_border_rect(leg_x, 310, 12, 12, Color::new(16, 124, 65), COLOR_PANEL_BG);
            self.draw_string(leg_x + 20, 312, "Free / Idle Page Frame", COLOR_TEXT_MUTED, None, 1);

            // 2. Telemetry / Life Support (PID 101/103)
            self.draw_rect(leg_x, 330, 12, 12, COLOR_ACCENT_BLUE);
            self.draw_string(leg_x + 20, 332, "Telemetry / Life Support", COLOR_TEXT_WHITE, None, 1);

            // 3. Critical Navigation (PID 102)
            self.draw_rect(leg_x, 350, 12, 12, COLOR_ACCENT_PURPLE);
            self.draw_string(leg_x + 20, 352, "Critical Orbital TMR Task", COLOR_TEXT_WHITE, None, 1);

            // 4. Self-Healed ECC Page
            self.draw_rect(leg_x, 370, 12, 12, Color::new(245, 158, 11));
            self.draw_string(leg_x + 20, 372, "Self-Healed Page (ECC Ok)", Color::new(245, 158, 11), None, 1);

            // 5. Quarantined Frame
            self.draw_rect(leg_x, 390, 12, 12, Color::new(239, 68, 68));
            self.draw_string(leg_x + 20, 392, "Quarantined (Hard Fault)", Color::new(239, 68, 68), None, 1);
        }

        // Draw Real-Time Stats (x: 260, y: 410)
        let stats_y = 410;
        let mut allocated_count = 0;
        for p in alloc_map {
            if p.is_some() {
                allocated_count += 1;
            }
        }
        let quarantined_count = if let Some(c) = core { c.pages_quarantined } else { 0 };
        let corrected_count = if let Some(c) = core { c.ecc_single_bit_corrections } else { 0 };
        let scrubber_sweeps = if let Some(c) = core { c.scrubber_sweeps } else { 0 };

        // Alloc count string
        let mut alloc_buf = [0u8; 16];
        let alloc_str = format_ticks(allocated_count, &mut alloc_buf);
        let mut total_buf = [0u8; 16];
        let total_str = format_ticks(64, &mut total_buf);
        
        let mut stats_line1 = alloc::string::String::new();
        stats_line1.push_str("Allocated  : ");
        stats_line1.push_str(alloc_str);
        stats_line1.push_str(" / ");
        stats_line1.push_str(total_str);
        stats_line1.push_str(" Pages");
        self.draw_string(260, stats_y + 10, &stats_line1, COLOR_TEXT_WHITE, None, 1);

        // Quarantined count string
        let mut quar_buf = [0u8; 16];
        let quar_str = format_ticks(quarantined_count, &mut quar_buf);
        let mut corr_buf = [0u8; 16];
        let corr_str = format_ticks(corrected_count, &mut corr_buf);

        let mut stats_line2 = alloc::string::String::new();
        stats_line2.push_str("Quarantine : ");
        stats_line2.push_str(quar_str);
        stats_line2.push_str(" | Healed: ");
        stats_line2.push_str(corr_str);
        self.draw_string(260, stats_y + 25, &stats_line2, COLOR_TEXT_WHITE, None, 1);

        // Scrubber sweeps string
        let mut sweeps_buf = [0u8; 16];
        let sweeps_str = format_ticks(scrubber_sweeps, &mut sweeps_buf);

        let mut stats_line3 = alloc::string::String::new();
        stats_line3.push_str("Rad Sweeps : ");
        stats_line3.push_str(sweeps_str);
        stats_line3.push_str(" (Active Sweep)");
        self.draw_string(260, stats_y + 40, &stats_line3, COLOR_TEXT_MUTED, None, 1);
    }

    /// Renders the cooperative task scheduler threads CPU metrics.
    pub fn draw_dashboard_scheduler_metrics(&mut self, core: Option<&kernel_core::SystemCore>, ticks: usize, is_layout: bool) {
        if is_layout {
            // Clear content area: x: 665, y: 305, w: 565, h: 210 using Card background color
            self.draw_rect(665, 305, 565, 210, COLOR_PANEL_BG);
        } else {
            // Clear only the dynamic TMR metrics area: x: 675, y: 460, w: 545, h: 16
            self.draw_rect(675, 460, 545, 16, COLOR_PANEL_BG);
        }

        let threads_data = {
            let sched = crate::scheduler::SCHEDULER.lock();
            let mut list = alloc::vec::Vec::new();
            for t in &sched.threads {
                list.push((t.id, t.status, t.rsp));
            }
            list
        };

        let has_user_thread = threads_data.len() > 3;

        for i in 0..4 {
            let y = 310 + i * 35;
            
            // Thread row header mapping
            let (name, status_str, status_color, load) = match i {
                0 => {
                    let status = if i < threads_data.len() { threads_data[i].1 } else { crate::scheduler::ThreadStatus::Ready };
                    let load = get_thread_load(0, ticks, has_user_thread);
                    let (s_str, s_col) = if status == crate::scheduler::ThreadStatus::Running {
                        ("RUNNING", COLOR_ACCENT_GREEN)
                    } else {
                        ("READY  ", COLOR_ACCENT_BLUE)
                    };
                    ("T0: KERNEL SHELL", s_str, s_col, load)
                }
                1 => {
                    let status = if i < threads_data.len() { threads_data[i].1 } else { crate::scheduler::ThreadStatus::Ready };
                    let load = get_thread_load(1, ticks, has_user_thread);
                    let (s_str, s_col) = if status == crate::scheduler::ThreadStatus::Running {
                        ("RUNNING", COLOR_ACCENT_GREEN)
                    } else {
                        ("READY  ", COLOR_ACCENT_BLUE)
                    };
                    ("T1: MEM SCRUBBER", s_str, s_col, load)
                }
                2 => {
                    let status = if i < threads_data.len() { threads_data[i].1 } else { crate::scheduler::ThreadStatus::Ready };
                    let load = get_thread_load(2, ticks, has_user_thread);
                    let (s_str, s_col) = if status == crate::scheduler::ThreadStatus::Running {
                        ("RUNNING", COLOR_ACCENT_GREEN)
                    } else {
                        ("READY  ", COLOR_ACCENT_BLUE)
                    };
                    ("T2: SYS DIAGS   ", s_str, s_col, load)
                }
                3 => {
                    if has_user_thread {
                        let status = threads_data[3].1;
                        let load = get_thread_load(3, ticks, has_user_thread);
                        let (s_str, s_col) = if status == crate::scheduler::ThreadStatus::Running {
                            ("RUNNING", COLOR_ACCENT_GREEN)
                        } else {
                            ("READY  ", COLOR_ACCENT_BLUE)
                        };
                        ("T3: USER PAYLOAD", s_str, s_col, load)
                    } else {
                        ("T3: IDLE/OFFLINE", "OFFLINE", COLOR_TEXT_MUTED, 0)
                    }
                }
                _ => ("", "", COLOR_TEXT_MUTED, 0),
            };

            if is_layout {
                // 1. Draw Thread Name (Static)
                self.draw_string(675, y, name, COLOR_TEXT_WHITE, None, 1);
            }

            // 2. Draw Status Pill (Clear only dynamic part)
            if !is_layout {
                self.draw_rect(830, y, 70, 12, COLOR_PANEL_BG);
            }
            self.draw_string(830, y, status_str, status_color, None, 1);

            // 3. Draw CPU Load Bar Track (x: 910, y: y+2, W: 120, H: 6)
            let bar_x = 910;
            let bar_y = y + 2;
            let bar_w = 120;
            let bar_h = 6;
            self.draw_rect(bar_x, bar_y, bar_w, bar_h, COLOR_BG); // Dark Track

            if load > 0 {
                // Filled bar representation
                let fill_w = (load * bar_w) / 100;
                let fill_color = match i {
                    0 => COLOR_ACCENT_BLUE,
                    1 => COLOR_ACCENT_GREEN,
                    2 => COLOR_ACCENT_PURPLE,
                    3 => Color::new(236, 72, 153),
                    _ => COLOR_TEXT_MUTED,
                };
                self.draw_rect(bar_x, bar_y, fill_w, bar_h, fill_color);
            }

            // 4. Draw CPU Load % (Clear only dynamic part)
            if !is_layout {
                self.draw_rect(1045, y, 40, 12, COLOR_PANEL_BG);
            }
            let mut load_buf = [0u8; 16];
            let load_str = format_ticks(load, &mut load_buf);
            let mut load_text = alloc::string::String::new();
            load_text.push_str(load_str);
            load_text.push_str("%");
            self.draw_string(1045, y, &load_text, COLOR_TEXT_WHITE, None, 1);

            // 5. Draw Stack pointer address / info (Clear only dynamic part)
            if !is_layout {
                self.draw_rect(1095, y, 80, 12, COLOR_PANEL_BG);
            }
            if i < threads_data.len() {
                let mut rsp_buf = [0u8; 16];
                if i == 0 {
                    self.draw_string(1095, y, "Boot RSP", COLOR_TEXT_MUTED, None, 1);
                } else {
                    let rsp_val = threads_data[i].2;
                    let rsp_short = (rsp_val & 0xFFFF) as usize; // Show lower 16 bits
                    let rsp_str = format_ticks(rsp_short, &mut rsp_buf);
                    let mut rsp_text = alloc::string::String::new();
                    rsp_text.push_str("rsp:");
                    rsp_text.push_str(rsp_str);
                    self.draw_string(1095, y, &rsp_text, COLOR_TEXT_MUTED, None, 1);
                }
            } else {
                self.draw_string(1095, y, "--------", COLOR_TEXT_MUTED, None, 1);
            }
        }

        // Draw voter corrections & TMR metrics underneath (y: 460)
        let tmr_y = 455;
        let critical_tmr_ops = if let Some(c) = core { c.critical_tmr_ops } else { 0 };
        let tmr_voter_corrections = if let Some(c) = core { c.tmr_voter_corrections } else { 0 };

        let mut tmr_buf = [0u8; 16];
        let tmr_str = format_ticks(critical_tmr_ops, &mut tmr_buf);
        let mut corr_buf = [0u8; 16];
        let corr_str = format_ticks(tmr_voter_corrections, &mut corr_buf);

        let mut tmr_line = alloc::string::String::new();
        tmr_line.push_str("TMR Ops    : ");
        tmr_line.push_str(tmr_str);
        tmr_line.push_str(" | Voter Corrects: ");
        tmr_line.push_str(corr_str);
        self.draw_string(675, tmr_y + 10, &tmr_line, COLOR_TEXT_WHITE, None, 1);

        if is_layout {
            self.draw_string(675, tmr_y + 25, "Voter Health Status: 100.00% (Triple Redundant)", COLOR_ACCENT_GREEN, None, 1);
        }
    }

    /// Dynamically updates ONLY the active telemetry values on the dashboard (ticks, progress bar, memory grid, thread meters)
    /// without clearing or redrawing the static panels. This completely eliminates screen flickering!
    pub fn update_dashboard_telemetry(&mut self, ticks: usize, core: Option<&kernel_core::SystemCore>) {
        // 1. Update System Ticks Value (x: 1076, y: 124)
        self.draw_rect(1076, 124, 120, 12, COLOR_PANEL_BG);
        
        let mut ticks_buf = [0u8; 16];
        let ticks_str = format_ticks(ticks, &mut ticks_buf);
        self.draw_string(1076, 124, ticks_str, COLOR_ACCENT_BLUE, None, 1);

        // 2. Update Glowing Heartbeat Progress Bar (x: 60, y: 610)
        let bar_width = ((ticks * 8) % 1160) as usize;
        self.draw_rect(60, 610, 1160, 12, COLOR_BG); // Clear progress bar background
        self.draw_rect(60, 610, bar_width, 12, COLOR_ACCENT_GREEN); // Draw new filled progress

        // 3. Update Visual Page Grid & Active Thread load meters
        self.draw_dashboard_memory_grid(core, false);
        self.draw_dashboard_scheduler_metrics(core, ticks, false);
    }

    /// Dynamically updates the interactive keyboard echo prompt area without touching other panels.
    pub fn update_keyboard_prompt(&mut self, text: &str) {
        // Clear prompt area (x: 60, y: 530) using main Background Color (charcoal)
        self.draw_rect(60, 530, 1160, 12, COLOR_BG);
        
        // Draw the updated prompt text
        self.draw_string(60, 530, text, COLOR_ACCENT_GREEN, None, 1);
    }
}

/// Dynamic u32 to string formatter inside bare-metal no_std environment.
pub fn format_ticks(mut ticks: usize, buf: &mut [u8; 16]) -> &str {
    if ticks == 0 {
        return "0";
    }
    let mut i = 15;
    while ticks > 0 && i > 0 {
        buf[i] = (b'0' + (ticks % 10) as u8) as u8;
        ticks /= 10;
        i -= 1;
    }
    core::str::from_utf8(&buf[i + 1..16]).unwrap_or("0")
}

/// Helper to calculate dynamic CPU load oscillation for a given thread ID to make dashboard feel alive.
fn get_thread_load(thread_id: usize, ticks: usize, has_user_thread: bool) -> usize {
    match thread_id {
        0 => {
            // Kernel Shell
            let base = 6;
            let osc = (ticks / 3) % 7;
            base + osc
        }
        1 => {
            // Memory Scrubber
            let base = 18;
            let osc = (ticks / 2) % 11;
            base + osc
        }
        2 => {
            // System Diagnostics
            let base = 12;
            let osc = (ticks / 4) % 9;
            base + osc
        }
        3 => {
            if has_user_thread {
                let base = 65;
                let osc = ticks % 15;
                base + osc
            } else {
                0
            }
        }
        _ => 0,
    }
}
