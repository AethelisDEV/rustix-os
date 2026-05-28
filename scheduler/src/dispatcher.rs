//! # Safe Task Dispatcher & Adaptive Process Relocation Module
//!
//! This module implements the task scheduler and dynamic process dispatcher of the AE Rustanium kernel.
//!
//! One of the core features of AE Rustanium is **adaptiveness** under hardware faults.
//! The dispatcher coordinates closely with the `MemoryAllocator` to manage processes.
//!
//! ## Fault Mitigation & Hot-Swapping
//! 1. The dispatcher schedules processes to perform steps (simulated reads/writes in their allocated pages).
//! 2. If a process experiences an uncorrectable Double-Bit Flip (which raises a virtual MMU Page Fault),
//!    the scheduler intercepts the fault.
//! 3. Instead of crashing (kernel panic), the dispatcher:
//!    - Requests the memory subsystem to **quarantine** the faulty physical frame.
//!    - **Relocates** the process's data to a new healthy physical frame.
//!    - **Hot-swaps** the page table mapping inside the `KernelProcess` state.
//!    - Logs the self-healing event.
//!    - Seamlessly retries and resumes the execution step.

use alloc::string::String;
use alloc::vec::Vec;
use alloc::vec;
use alloc::format;
use memory_subsystem::MemoryAllocator;
use crate::tmr::{TmrVoter, TmrResult};

/// A virtual process running in the kernel.
#[derive(Debug, Clone)]
pub struct KernelProcess {
    /// Unique Process ID (PID).
    pub pid: u32,
    /// Human-readable name of the process.
    pub name: String,
    /// Physical frame indices allocated to this process in the memory subsystem.
    pub allocated_pages: Vec<usize>,
    /// Indicates whether this process is safety-critical and requires TMR arithmetic protection.
    pub is_critical: bool,
    /// Simulated instruction pointer / step index.
    pub instruction_step: usize,
}

impl KernelProcess {
    /// Creates a new virtual process.
    pub fn new(pid: u32, name: &str, is_critical: bool) -> Self {
        Self {
            pid,
            name: String::from(name),
            allocated_pages: Vec::new(),
            is_critical,
            instruction_step: 0,
        }
    }
}

/// The kernel scheduler and process dispatcher.
pub struct TaskDispatcher {
    /// The list of registered virtual processes.
    pub processes: Vec<KernelProcess>,
    /// A rolling log of dispatcher events for real-time visualization.
    pub event_logs: Vec<String>,
}

impl Default for TaskDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskDispatcher {
    /// Initializes the task dispatcher.
    pub fn new() -> Self {
        Self {
            processes: Vec::new(),
            event_logs: vec![String::from("Dispatcher initialized.")],
        }
    }

    /// Spawns a process, allocating an initial physical page frame for it.
    ///
    /// Returns `Ok(pid)` on success, or an error string if out of memory.
    pub fn spawn_process(
        &mut self,
        pid: u32,
        name: &str,
        is_critical: bool,
        allocator: &mut MemoryAllocator,
    ) -> Result<u32, &'static str> {
        let mut process = KernelProcess::new(pid, name, is_critical);

        // Allocate a physical page for this process
        match allocator.allocate_page(pid) {
            Some(frame_idx) => {
                process.allocated_pages.push(frame_idx);
                self.processes.push(process);
                self.log_event(&format!(
                    "SPAWN: Process '{}' (PID: {}) spawned on physical page frame {}",
                    name, pid, frame_idx
                ));
                Ok(pid)
            }
            None => {
                self.log_event(&format!("SPAWN FAIL: Out of memory spawning process '{}'", name));
                Err("Out of physical memory frames.")
            }
        }
    }

    /// Executes a scheduling step for a given process, simulating load, read/write cycles,
    /// and actively handling uncorrectable page faults.
    pub fn execute_step(&mut self, pid: u32, allocator: &mut MemoryAllocator) -> Result<(), &'static str> {
        // Retrieve the process
        let process_idx = match self.processes.iter().position(|p| p.pid == pid) {
            Some(idx) => idx,
            None => return Err("Process not found."),
        };

        let mut process = self.processes[process_idx].clone();
        process.instruction_step += 1;

        if process.allocated_pages.is_empty() {
            return Err("Process has no memory allocated.");
        }

        let frame_index = process.allocated_pages[0];
        let offset = (process.instruction_step * 7) % memory_subsystem::PAGE_SIZE; // Dummy simulated address offset
        let simulated_val = (process.instruction_step * 17) as u8;

        self.log_event(&format!(
            "EXEC: '{}' (PID: {}) writing {:#04X} to frame {}, offset {}",
            process.name, process.pid, simulated_val, frame_index, offset
        ));

        // 1. Perform Write
        if let Err(e) = allocator.frames[frame_index].write_byte(offset, simulated_val) {
            self.log_event(&format!(
                "FAULT: Write error for PID {}: {}. Relocating...",
                pid, e
            ));
            let new_frame_idx = self.handle_fault(process_idx, frame_index, allocator)?;
            // Retry write on the fresh frame
            allocator.frames[new_frame_idx].write_byte(offset, simulated_val)?;
            self.processes[process_idx].allocated_pages[0] = new_frame_idx;
            return Ok(());
        }

        // We read from the previous step's offset to verify historical memory persistence!
        let read_offset = if process.instruction_step > 1 {
            ((process.instruction_step - 1) * 7) % memory_subsystem::PAGE_SIZE
        } else {
            offset
        };

        // 2. Perform Read
        match allocator.frames[frame_index].read_byte(read_offset) {
            Ok((val, correction_opt)) => {
                if let Some(event) = correction_opt {
                    self.log_event(&format!(
                        "ECC REPAIR: Corrected single-bit flip on frame {}, offset {}, flipped bit {}",
                        event.frame_index, event.offset, event.bit_position
                    ));
                }
                self.log_event(&format!(
                    "READ VERIFY: PID {} read back value {:#04X} from offset {}",
                    pid, val, read_offset
                ));
            }
            Err(damaged_frame_idx) => {
                // Double-bit flip (uncorrectable) page fault detected!
                self.log_event(&format!(
                    "CRITICAL FAULT: Double-bit flip on frame {} at offset {}. Hot-swapping and relocating PID {}...",
                    damaged_frame_idx, read_offset, pid
                ));

                // Perform Hot-Swap Relocation!
                let new_frame_idx = self.handle_fault(process_idx, damaged_frame_idx, allocator)?;

                // Retry execution: we read the relocated value (which falls back to safe zero for the damaged byte)
                let (val, _) = allocator.frames[new_frame_idx].read_byte(read_offset).map_err(|_| "Retry read failed.")?;
                self.log_event(&format!(
                    "RESUME: PID {} successfully recovered on healthy frame {}. Read byte: {:#04X}",
                    pid, new_frame_idx, val
                ));
            }
        }

        // Update process step counter
        self.processes[process_idx].instruction_step = process.instruction_step;
        Ok(())
    }

    /// Executes a critical mathematical operation protected by Triple Modular Redundancy (TMR).
    ///
    /// If register or compiler level bit flips occur in one runner, they are corrected and logged.
    pub fn execute_critical_tmr<T, F>(&mut self, name: &str, task: F) -> Option<T>
    where
        T: Clone + PartialEq + core::fmt::Debug,
        F: Fn(usize) -> T,
    {
        self.log_event(&format!("TMR ACTIVE: Initiating redundant execution for '{}'", name));
        let res: TmrResult<T> = TmrVoter::execute_redundant(task);

        match res.status {
            crate::tmr::TmrStatus::Perfect => {
                self.log_event(&format!("TMR SUCCESS: Perfect match across all 3 runners for '{}'", name));
            }
            crate::tmr::TmrStatus::Corrected { faulty_runner, faulty_value, majority_value } => {
                self.log_event(&format!(
                    "TMR WARNING: Divergence detected in runner {} (produced '{}'). Corrected by voter to majority '{}'!",
                    faulty_runner, faulty_value, majority_value
                ));
            }
            crate::tmr::TmrStatus::Failed => {
                self.log_event(&format!("TMR CRITICAL FAILURE: Complete divergence for '{}'!", name));
            }
        }

        res.value
    }

    /// Logs an event to the dispatcher's rolling logs.
    pub fn log_event(&mut self, event: &str) {
        if self.event_logs.len() >= 100 {
            self.event_logs.remove(0); // Keep logs bounded
        }
        self.event_logs.push(String::from(event));
    }

    /// Internal helper that quarantines a damaged frame, allocates a new frame, and hot-swaps process pages.
    fn handle_fault(
        &mut self,
        process_idx: usize,
        damaged_frame_idx: usize,
        allocator: &mut MemoryAllocator,
    ) -> Result<usize, &'static str> {
        let pid = self.processes[process_idx].pid;

        // Quarantines the old physical frame and relocates existing page data to a new one
        let new_frame_idx = allocator.relocate_and_quarantine(damaged_frame_idx)?;

        // Update the process page mapping table
        if let Some(pos) = self.processes[process_idx].allocated_pages.iter().position(|&x| x == damaged_frame_idx) {
            self.processes[process_idx].allocated_pages[pos] = new_frame_idx;
        }

        self.log_event(&format!(
            "HOT-SWAP: Frame {} isolated/quarantined. PID {} relocated to frame {}",
            damaged_frame_idx, pid, new_frame_idx
        ));

        Ok(new_frame_idx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spawn_and_normal_step() {
        let mut allocator = MemoryAllocator::new();
        let mut dispatcher = TaskDispatcher::new();

        let pid = dispatcher.spawn_process(1, "Telemetry", false, &mut allocator).unwrap();
        assert_eq!(pid, 1);
        assert_eq!(dispatcher.processes.len(), 1);

        // Execute step (normal read/write)
        dispatcher.execute_step(1, &mut allocator).unwrap();
        assert_eq!(dispatcher.processes[0].instruction_step, 1);
    }

    #[test]
    fn test_step_under_double_bit_flip_triggers_hot_swap() {
        let mut allocator = MemoryAllocator::new();
        let mut dispatcher = TaskDispatcher::new();

        let pid = dispatcher.spawn_process(10, "Guidance", false, &mut allocator).unwrap();
        let initial_frame = dispatcher.processes[0].allocated_pages[0];

        // Simulate write step
        dispatcher.execute_step(pid, &mut allocator).unwrap();

        // Inject double bit flip in the page to trigger uncorrectable fault on read
        let offset = (1 * 7) % memory_subsystem::PAGE_SIZE;
        allocator.inject_bit_flip(initial_frame, offset, 0).unwrap();
        allocator.inject_bit_flip(initial_frame, offset, 4).unwrap();

        // Execute next step. Read should trigger critical fault, quarantine the page, relocate data, and resume!
        dispatcher.execute_step(pid, &mut allocator).unwrap();

        // Verify that the page was hot-swapped
        let final_frame = dispatcher.processes[0].allocated_pages[0];
        assert_ne!(initial_frame, final_frame);
        assert_eq!(allocator.frames[initial_frame].status, memory_subsystem::PageStatus::Quarantined);
        assert_eq!(allocator.allocation_map[final_frame], Some(pid));
    }
}
