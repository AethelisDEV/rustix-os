#![no_std]

//! # AE Rustanium Memory Subsystem Crate
//!
//! This crate provides complete, safe, and fault-tolerant virtual memory management for the kernel.
//! It contains SECDED (Hamming) error-correction algorithms, page frame allocation, error logs, and
//! an active memory scrubbing daemon.
//!
//! All components operate without any `unsafe` code under a strict safe design.

extern crate alloc;

pub mod ecc;
pub mod allocator;
pub mod scrubber;

// Re-export core types for simplified external usage
pub use ecc::{encode, decode, DecodeResult};
pub use allocator::{
    MemoryAllocator, PhysicalFrame, PageStatus, CorrectionEvent, PAGE_SIZE, TOTAL_PAGES,
};
pub use scrubber::{MemoryScrubber, ScrubReport};
