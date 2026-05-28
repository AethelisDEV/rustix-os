//! # Virtual File System Inode Module
//!
//! This module implements the Inode structures for the AE Rustanium Virtual File System.
//!
//! An Inode is the basic building block of our Unix-like VFS. It represents a file or directory node.
//! In keeping with our microkernel architecture:
//! - **Directory Inodes** contain directory entries, mapping names to child inode indices.
//! - **File Inodes** contain block indices. Under our self-healing design, **these block indices map**
//!   **directly to page frames inside the memory-subsystem's physical allocator**.
//!
//! This ensures that all file operations are fully verified, bounds-checked, and protected by Hamming SECDED.

extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;

/// The type of an Inode, defining whether it is a directory or a regular file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InodeType {
    /// A directory node, containing entries mapping file/directory names to Inode indices.
    Directory {
        /// Directory entries represented as a list of tuples containing the item name and its Inode index.
        entries: Vec<(String, usize)>,
    },
    /// A regular file node, whose actual bytes are stored across memory-subsystem physical page frames.
    File {
        /// Sequential list of virtual physical frame indices (pages) where file data is stored.
        blocks: Vec<usize>,
        /// Current size of the file in bytes.
        size: usize,
    },
}

/// A node inside the Virtual File System representing a file or a directory.
///
/// Fully safe structure with no raw pointers, managed in memory by index offsets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inode {
    /// The unique index of this Inode in the filesystem's Inode table.
    pub index: usize,
    /// The name of this specific file or directory.
    pub name: String,
    /// The specific type of file or directory.
    pub inode_type: InodeType,
}

impl Inode {
    /// Creates a new directory Inode.
    pub fn new_directory(index: usize, name: &str) -> Self {
        Self {
            index,
            name: String::from(name),
            inode_type: InodeType::Directory {
                entries: Vec::new(),
            },
        }
    }

    /// Creates a new regular file Inode with zero initial size and empty blocks.
    pub fn new_file(index: usize, name: &str) -> Self {
        Self {
            index,
            name: String::from(name),
            inode_type: InodeType::File {
                blocks: Vec::new(),
                size: 0,
            },
        }
    }

    /// Returns `true` if this Inode is a directory.
    pub fn is_directory(&self) -> bool {
        matches!(self.inode_type, InodeType::Directory { .. })
    }

    /// Returns `true` if this Inode is a regular file.
    pub fn is_file(&self) -> bool {
        matches!(self.inode_type, InodeType::File { .. })
    }
}
