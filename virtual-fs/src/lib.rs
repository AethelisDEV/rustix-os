#![no_std]

//! # AE Rustanium Virtual File System (VFS) Crate
//!
//! This crate provides a modular, safe, and fault-tolerant sanal dosya sistemi (VFS) for the kernel.
//! It maps files to SECDED protected page frames in the memory-subsystem.
//!
//! Fully written under a **Zero Unsafe Policy**.

extern crate alloc;

pub mod inode;
pub mod vfs;

// Re-export core types for simplified external usage
pub use inode::{Inode, InodeType};
pub use vfs::VirtualFileSystem;
