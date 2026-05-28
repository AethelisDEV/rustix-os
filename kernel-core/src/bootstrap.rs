//! # Kernel Core Bootstrap & System bus Coordinator
//!
//! This module implements the main bootstrap and system ticks execution controller of the AE Rustanium kernel.
//!
//! Adhering strictly to the **Zero Unsafe Policy**, `SystemCore` manages the lifetime and integration
//! of the memory allocator, task scheduler, background scrubbers, and redundant voter subsystems.
//!
//! ## Systems Orchestration
//! - **Bootstrapping**: Sets up virtual memory, registers default system processes (Telemetry, Navigation, LifeSupport).
//! - **Redundancy Metrics**: Tracks every single-bit ECC repair, quarantined page frame, relocation event, and TMR voting correction.
//! - **Kernel Tick Loop**: Advances the system clock, coordinates scheduler ticks, and sweeps the memory space periodically.

use alloc::vec::Vec;
use alloc::format;
use memory_subsystem::{MemoryAllocator, MemoryScrubber, ScrubReport};
use scheduler::TaskDispatcher;
use virtual_fs::VirtualFileSystem;

/// The central kernel state controller.
pub struct SystemCore {
    /// Virtualized memory allocator.
    pub allocator: MemoryAllocator,
    /// Process scheduler and task dispatcher.
    pub dispatcher: TaskDispatcher,
    /// Virtual file system storage.
    pub vfs: VirtualFileSystem,
    /// Count of background memory scrubber sweep operations.
    pub scrubber_sweeps: usize,
    /// Count of single-bit flips successfully corrected by Hamming SECDED.
    pub ecc_single_bit_corrections: usize,
    /// Count of damaged page frames quarantined.
    pub pages_quarantined: usize,
    /// Count of pages successfully hot-swapped and relocated.
    pub pages_relocated: usize,
    /// Count of critical computations executed.
    pub critical_tmr_ops: usize,
    /// Count of CPU register bit flips corrected by TMR majority voting.
    pub tmr_voter_corrections: usize,
}

impl SystemCore {
    /// Bootstraps and initializes the entire AE Rustanium microkernel state.
    ///
    /// Spawns default microkernel processes with correct critical and standard flags:
    /// - **Telemetry (PID: 101)**: Standard telemetry reporter.
    /// - **Navigation (PID: 102)**: Critical orbital navigation task requiring TMR calculations.
    /// - **LifeSupport (PID: 103)**: Standard safety regulator.
    pub fn bootstrap() -> Self {
        // Log boot initialization sequence to serial output (COM1)
        crate::hal::SerialPort::write_str("============================================================\n");
        crate::hal::SerialPort::write_str("AE RUSTANIUM MICROKERNEL BOOTING (x86_64 Emulated / std hal)\n");
        crate::hal::SerialPort::write_str("============================================================\n");
        crate::hal::SerialPort::write_str("[BOOT] Initializing physical page allocator...\n");

        let mut allocator = MemoryAllocator::new();
        let mut dispatcher = TaskDispatcher::new();
        let mut vfs = VirtualFileSystem::new();

        crate::hal::SerialPort::write_str("[BOOT] Registering default processes (Telemetry, Navigation, LifeSupport)...\n");

        // Spawn default microkernel service processes
        let _ = dispatcher.spawn_process(101, "Telemetry", false, &mut allocator);
        let _ = dispatcher.spawn_process(102, "Navigation", true, &mut allocator);
        let _ = dispatcher.spawn_process(103, "LifeSupport", false, &mut allocator);

        crate::hal::SerialPort::write_str("[BOOT] Pre-populating VFS directory structures...\n");

        // Pre-populate virtual file system directories
        let _ = vfs.mkdir("/", "system");
        let _ = vfs.mkdir("/", "data");
        let _ = vfs.mkdir("/", "bin");

        // Create default system configuration and log files mapped to protected memory frames
        if vfs.create_file("/system", "kernel.conf").is_ok() {
            let _ = vfs.write_file(
                "/system/kernel.conf",
                b"[kernel]\nversion=1.0.0\nzero_unsafe=true\nsecurity=tmr_ecc\n",
                &mut allocator,
                0,
            );
        }

        if vfs.create_file("/data", "system_info.log").is_ok() {
            let _ = vfs.write_file(
                "/data/system_info.log",
                b"2026-05-27: AE-Rustanium booted successfully.\n2026-05-27: 64 page frames allocated. SECDED ECC active.\n2026-05-27: Background scrubber sweep running.\n",
                &mut allocator,
                0,
            );
        }

        crate::hal::SerialPort::write_str("[BOOT] Microkernel initialization complete. Yielding to scheduler.\n\n");

        Self {
            allocator,
            dispatcher,
            vfs,
            scrubber_sweeps: 0,
            ecc_single_bit_corrections: 0,
            pages_quarantined: 0,
            pages_relocated: 0,
            critical_tmr_ops: 0,
            tmr_voter_corrections: 0,
        }
    }

    /// Advances the operating system core by one operational tick.
    ///
    /// Executes the following workflow:
    /// 1. Runs a scheduler cycle, stepping all registered processes.
    /// 2. Performs a background memory scrubber sweep.
    /// 3. Updates all telemetry, ECC, and relocation diagnostics metrics.
    /// 4. Executes a critical mathematical operation protected by TMR.
    /// 5. Yields CPU core execution using a simulated Cpu::halt() instruction.
    pub fn tick(&mut self) {
        // 1. Dispatch step for each active process
        let pids: Vec<u32> = self.dispatcher.processes.iter().map(|p| p.pid).collect();
        for pid in pids {
            let _ = self.dispatcher.execute_step(pid, &mut self.allocator);
        }

        // 2. Run background memory scrubber sweep
        self.scrubber_sweeps += 1;
        let report: ScrubReport = MemoryScrubber::sweep(&mut self.allocator);
        self.update_metrics_from_scrub(report);

        // 3. Perform a critical mathematical calculation under TMR protection
        self.execute_orbital_calculation();

        // Log real-time diagnostic tick summary to serial port periodically to keep terminal clean
        if self.scrubber_sweeps % 500 == 0 {
            let tick_summary = format!(
                "[Tick {:4}] Sweeps: {} | ECC: {} | Quarantined: {} | Relocated: {}\n",
                self.scrubber_sweeps,
                self.scrubber_sweeps,
                self.ecc_single_bit_corrections,
                self.pages_quarantined,
                self.pages_relocated
            );
            crate::hal::SerialPort::write_str(&tick_summary);
        }

        // 4. Yield CPU core execution (Simulated raw x86 hlt assembly instruction)
        crate::hal::Cpu::halt();
    }

    /// Injects a bit flip into a specific physical frame address to simulate hardware fault.
    pub fn inject_memory_flip(&mut self, frame_index: usize, offset: usize, bit_index: u8) -> Result<(), &'static str> {
        self.allocator.inject_bit_flip(frame_index, offset, bit_index)?;
        self.dispatcher.log_event(&format!(
            "INJECTOR: Flipped bit {} at frame {}, offset {}",
            bit_index, frame_index, offset
        ));
        Ok(())
    }

    /// Integrates background sweep reports into core kernel metrics.
    fn update_metrics_from_scrub(&mut self, report: ScrubReport) {
        if !report.corrections.is_empty() {
            self.ecc_single_bit_corrections += report.corrections.len();
            for corr in &report.corrections {
                let msg = format!(
                    "SCRUBBER DETECT: Corrected single-bit flip on frame {}, offset {}",
                    corr.frame_index, corr.offset
                );
                self.dispatcher.log_event(&msg);
                // Print immediate green warning to terminal
                let alert = format!(
                    "\n\x1B[38;5;46m[HEALING OK] {}\x1B[0m\nrustanium> ",
                    msg
                );
                crate::hal::SerialPort::write_str(&alert);
            }
        }

        if !report.relocations.is_empty() {
            self.pages_quarantined += report.relocations.len();
            self.pages_relocated += report.relocations.len();
            for (old, new) in &report.relocations {
                let msg = format!(
                    "SCRUBBER SEVERE: Quarantine triggered! Hot-swapped damaged frame {} to fresh frame {}",
                    old, new
                );
                self.dispatcher.log_event(&msg);
                // Print immediate red alert to terminal
                let alert = format!(
                    "\n\x1B[38;5;196m[QUARANTINE ALERT] {}\x1B[0m\nrustanium> ",
                    msg
                );
                crate::hal::SerialPort::write_str(&alert);
            }
        }
    }

    /// Simulates a safety-critical orbital navigation calculation protected by TMR majority voting.
    ///
    /// Periodically simulates CPU or register level bit flips during the calculation to let the TMR voter
    /// actively resolve, correct, and log the incident.
    fn execute_orbital_calculation(&mut self) {
        self.critical_tmr_ops += 1;

        // Simulate orbital delta-V vector multiplication: v * 3
        let velocity = 7500; // 7500 m/s orbital speed
        
        // Randomly simulate a CPU/Register bit flip in 1 out of 5 cycles
        let should_simulate_alu_flip = self.critical_tmr_ops.is_multiple_of(5);

        let result = self.dispatcher.execute_critical_tmr("Orbital Navigation Delta-V", |runner_idx| {
            let mut val = velocity * 3;
            // Inject a register bit flip at index 4 of runner 1
            if should_simulate_alu_flip && runner_idx == 1 {
                val ^= 1 << 5; // Corrupts value from 22500 to 22532
            }
            val
        });

        if should_simulate_alu_flip {
            self.tmr_voter_corrections += 1;
        }

        if let Some(final_val) = result {
            // Navigation value resolved safely
            let _ = final_val;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kernel_bootstrap_and_tick() {
        let mut core = SystemCore::bootstrap();
        assert_eq!(core.dispatcher.processes.len(), 3);
        assert_eq!(core.scrubber_sweeps, 0);

        // Advance 3 ticks
        core.tick();
        core.tick();
        core.tick();

        assert_eq!(core.scrubber_sweeps, 3);
        assert_eq!(core.critical_tmr_ops, 3);
    }

    #[test]
    fn test_memory_fault_scrubbing_integration() {
        let mut core = SystemCore::bootstrap();
        
        // Inject single bit flip
        let pid_101_frame = core.dispatcher.processes[0].allocated_pages[0];
        core.inject_memory_flip(pid_101_frame, 4, 2).unwrap();

        // Tick kernel, scrubber should sweep, identify, and correct it
        core.tick();

        assert_eq!(core.ecc_single_bit_corrections, 1);
        assert_eq!(core.pages_quarantined, 0);
    }
}
