//! # Safe Memory Allocator & Virtual Physical Memory Module
//!
//! This module implements the virtualized physical RAM management layer of the AE Rustanium kernel.
//! Adhering strictly to the **Zero Unsafe Policy**, physical memory frames are virtualized using safe,
//! bounds-checked, and typed structures.
//!
//! Every simulated physical page frame is backed by an array of SECDED-encoded words. The allocator
//! is responsible for:
//! 1. Tracking the allocation state (free, allocated to a PID, or quarantined).
//! 2. Executing safe, verified reads and writes (calculating/verifying ECC on the fly).
//! 3. Isolating and hot-swapping damaged frames when double-bit flips occur.

use crate::ecc::{self, DecodeResult};
use alloc::vec::Vec;

/// The size of a single virtual physical page frame in bytes.
pub const PAGE_SIZE: usize = 64;

/// The total number of pages in the simulated physical memory pool (8x8 Grid).
pub const TOTAL_PAGES: usize = 64;

/// Diagnostic event details emitted during a memory read correction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CorrectionEvent {
    /// Index of the physical frame where the flip occurred.
    pub frame_index: usize,
    /// Offset within the page (0..PAGE_SIZE).
    pub offset: usize,
    /// The bit position (1..=13) that was corrected.
    pub bit_position: usize,
}

/// The status of a virtual physical memory frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageStatus {
    /// Perfectly healthy memory frame.
    Healthy,
    /// Has experienced single-bit flips that were corrected by ECC. Still usable.
    Recovered {
        /// Total number of corrected single-bit flips on this page.
        corrected_count: usize,
    },
    /// Has experienced an uncorrectable double-bit flip or exceeded error tolerance.
    /// Quarantined and barred from future allocations.
    Quarantined,
}

/// A simulated physical memory page frame.
///
/// To prevent unsafe operations, this structure maintains clean bounds checks and safe buffers.
#[derive(Debug, Clone)]
pub struct PhysicalFrame {
    /// The physical index of this frame in the allocator (0..64).
    pub index: usize,
    /// The virtualized physical start address.
    pub physical_address: usize,
    /// SECDED encoded 13-bit words. Length is `PAGE_SIZE` (64 words).
    pub data: [u16; PAGE_SIZE],
    /// Current health status of the frame.
    pub status: PageStatus,
}

impl PhysicalFrame {
    /// Creates a new, healthy, empty physical memory frame.
    pub fn new(index: usize, physical_address: usize) -> Self {
        Self {
            index,
            physical_address,
            data: [0; PAGE_SIZE], // Initialized with zeros (which are validly encoded ECC values for 0 data)
            status: PageStatus::Healthy,
        }
    }

    /// Safely writes a raw byte to a specific offset on the page, automatically encoding it using SECDED.
    ///
    /// Returns `Ok(())` on success, or an error string if out of bounds or quarantined.
    pub fn write_byte(&mut self, offset: usize, value: u8) -> Result<(), &'static str> {
        if self.status == PageStatus::Quarantined {
            return Err("Cannot write to a quarantined physical page.");
        }
        if offset >= PAGE_SIZE {
            return Err("Write index out of bounds.");
        }
        self.data[offset] = ecc::encode(value);
        Ok(())
    }

    /// Safely reads a byte from a specific offset, decoding and verifying it against SECDED.
    ///
    /// If a single-bit flip is detected, it is corrected inside the frame buffer, the frame status
    /// is updated to `Recovered`, and the correction event is returned along with the data.
    /// If a double-bit flip is detected, it returns an `Err(index)` which triggers a page fault.
    pub fn read_byte(&mut self, offset: usize) -> Result<(u8, Option<CorrectionEvent>), usize> {
        if offset >= PAGE_SIZE {
            return Err(self.index); // Treat bounds error as page fault
        }

        let encoded_word = self.data[offset];
        match ecc::decode(encoded_word) {
            DecodeResult::NoError(value) => Ok((value, None)),
            DecodeResult::Corrected(value, bit_pos) => {
                // Self-healing: repair the bit flip in place!
                self.data[offset] = ecc::encode(value);

                // Update page health status
                match &mut self.status {
                    PageStatus::Healthy => {
                        self.status = PageStatus::Recovered { corrected_count: 1 };
                    }
                    PageStatus::Recovered { corrected_count } => {
                        *corrected_count += 1;
                        if *corrected_count >= 5 {
                            // Too unstable, quarantine the page!
                            self.status = PageStatus::Quarantined;
                        }
                    }
                    PageStatus::Quarantined => {}
                }

                let event = CorrectionEvent {
                    frame_index: self.index,
                    offset,
                    bit_position: bit_pos,
                };
                Ok((value, Some(event)))
            }
            DecodeResult::Uncorrectable => {
                // Double-bit flip! Immediately quarantine the page to prevent further corruption.
                self.status = PageStatus::Quarantined;
                Err(self.index)
            }
        }
    }
}

/// The physical memory allocator responsible for page lifecycle, allocation map, and quarantining.
#[derive(Debug, Clone)]
pub struct MemoryAllocator {
    /// Array of simulated physical pages.
    pub frames: Vec<PhysicalFrame>,
    /// Allocation table: maps physical frame index to the owning PID. `None` is free.
    pub allocation_map: Vec<Option<u32>>,
}

impl Default for MemoryAllocator {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryAllocator {
    /// Initializes the physical memory allocator with 64 pages (4KB total physical RAM).
    pub fn new() -> Self {
        let mut frames = Vec::with_capacity(TOTAL_PAGES);
        let mut allocation_map = Vec::with_capacity(TOTAL_PAGES);

        for idx in 0..TOTAL_PAGES {
            let start_address = idx * PAGE_SIZE;
            frames.push(PhysicalFrame::new(idx, start_address));
            allocation_map.push(None);
        }

        Self { frames, allocation_map }
    }

    /// Allocates a healthy physical page frame to a process (identified by PID).
    ///
    /// Returns the physical frame index on success, or `None` if out of memory.
    pub fn allocate_page(&mut self, pid: u32) -> Option<usize> {
        for idx in 0..TOTAL_PAGES {
            if self.allocation_map[idx].is_none() && self.frames[idx].status != PageStatus::Quarantined {
                self.allocation_map[idx] = Some(pid);
                return Some(idx);
            }
        }
        None
    }

    /// Deallocates a physical page frame, freeing it for future use.
    pub fn deallocate_page(&mut self, frame_index: usize) -> Result<(), &'static str> {
        if frame_index >= TOTAL_PAGES {
            return Err("Invalid frame index.");
        }
        self.allocation_map[frame_index] = None;
        Ok(())
    }

    /// Manually injects a bit flip at a physical page offset to simulate hardware corruption.
    ///
    /// Useful for active visual testing in the dashboard.
    pub fn inject_bit_flip(&mut self, frame_index: usize, offset: usize, bit_index: u8) -> Result<(), &'static str> {
        if frame_index >= TOTAL_PAGES {
            return Err("Frame index out of bounds.");
        }
        if offset >= PAGE_SIZE {
            return Err("Offset out of bounds.");
        }
        if bit_index >= 16 {
            return Err("Bit index must be between 0 and 15.");
        }

        // Apply bit flip
        self.frames[frame_index].data[offset] ^= 1 << bit_index;
        Ok(())
    }

    /// Quarantines a page frame immediately and relocates its data to a healthy page.
    ///
    /// Returns the index of the newly allocated page where data was successfully migrated,
    /// or `Err` if relocation was impossible due to out-of-memory.
    pub fn relocate_and_quarantine(&mut self, damaged_frame_index: usize) -> Result<usize, &'static str> {
        if damaged_frame_index >= TOTAL_PAGES {
            return Err("Damaged frame index out of bounds.");
        }

        let pid = match self.allocation_map[damaged_frame_index] {
            Some(p) => p,
            None => return Err("Cannot relocate a page that is not allocated."),
        };

        // Quarantine the damaged frame
        self.frames[damaged_frame_index].status = PageStatus::Quarantined;
        self.allocation_map[damaged_frame_index] = None;

        // Allocate a new healthy page for the process
        let new_frame_index = match self.allocate_page(pid) {
            Some(idx) => idx,
            None => {
                // Out of memory! Relocation failed. We must keep the system aware.
                return Err("Out of memory: Unable to relocate quarantined page data.");
            }
        };

        // Reconstruct whatever data we can. For a single-bit flip, we decode/repair.
        // For double-bit flips, some data is lost (we write 0 or safe fallback for damaged bytes).
        // Adhering to the Safe Relocation protocol, we attempt to read and decode each byte.
        for offset in 0..PAGE_SIZE {
            let encoded_word = self.frames[damaged_frame_index].data[offset];
            let data_byte = match ecc::decode(encoded_word) {
                DecodeResult::NoError(val) => val,
                DecodeResult::Corrected(val, _) => val, // Grab corrected value
                DecodeResult::Uncorrectable => 0,       // Safe zero-fallback for destroyed memory bits
            };
            let _ = self.frames[new_frame_index].write_byte(offset, data_byte);
        }

        Ok(new_frame_index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allocation_lifecycle() {
        let mut allocator = MemoryAllocator::new();
        let pid = 42;

        let frame_idx = allocator.allocate_page(pid).expect("Should allocate");
        assert_eq!(allocator.allocation_map[frame_idx], Some(pid));

        allocator.deallocate_page(frame_idx).unwrap();
        assert_eq!(allocator.allocation_map[frame_idx], None);
    }

    #[test]
    fn test_single_bit_flip_repair_on_read() {
        let mut allocator = MemoryAllocator::new();
        let pid = 1;
        let frame_idx = allocator.allocate_page(pid).unwrap();

        // Write value
        allocator.frames[frame_idx].write_byte(10, 0xAA).unwrap();

        // Inject bit flip
        allocator.inject_bit_flip(frame_idx, 10, 2).unwrap();

        // Read and verify self-healing
        let result = allocator.frames[frame_idx].read_byte(10);
        assert!(result.is_ok());
        let (val, event) = result.unwrap();
        assert_eq!(val, 0xAA);
        let ev = event.expect("Expected a correction event");
        assert_eq!(ev.frame_index, frame_idx);
        assert_eq!(ev.offset, 10);

        // Verify that the page status updated to Recovered
        assert!(matches!(allocator.frames[frame_idx].status, PageStatus::Recovered { corrected_count: 1 }));
    }

    #[test]
    fn test_double_bit_flip_leads_to_quarantine_on_read() {
        let mut allocator = MemoryAllocator::new();
        let pid = 1;
        let frame_idx = allocator.allocate_page(pid).unwrap();

        allocator.frames[frame_idx].write_byte(5, 0x55).unwrap();

        // Inject double bit flip (flip bits at index 0 and 4)
        allocator.inject_bit_flip(frame_idx, 5, 0).unwrap();
        allocator.inject_bit_flip(frame_idx, 5, 4).unwrap();

        // Read should result in an Err(frame_idx) pointing to the page fault
        let result = allocator.frames[frame_idx].read_byte(5);
        assert_eq!(result, Err(frame_idx));

        // Page status should now be Quarantined
        assert_eq!(allocator.frames[frame_idx].status, PageStatus::Quarantined);
    }

    #[test]
    fn test_relocation_on_quarantine() {
        let mut allocator = MemoryAllocator::new();
        let pid = 99;
        let damaged_idx = allocator.allocate_page(pid).unwrap();

        allocator.frames[damaged_idx].write_byte(0, 0x11).unwrap();
        allocator.frames[damaged_idx].write_byte(1, 0x22).unwrap();

        // Trigger relocation
        let new_idx = allocator.relocate_and_quarantine(damaged_idx).unwrap();
        assert_ne!(damaged_idx, new_idx);

        // Damaged frame should be quarantined and unallocated
        assert_eq!(allocator.frames[damaged_idx].status, PageStatus::Quarantined);
        assert_eq!(allocator.allocation_map[damaged_idx], None);

        // New frame should hold the data
        assert_eq!(allocator.allocation_map[new_idx], Some(pid));
        let (val0, _) = allocator.frames[new_idx].read_byte(0).unwrap();
        let (val1, _) = allocator.frames[new_idx].read_byte(1).unwrap();
        assert_eq!(val0, 0x11);
        assert_eq!(val1, 0x22);
    }
}
