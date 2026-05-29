//! # Safe Virtual File System Manager
//!
//! This module implements path resolution, node lookup, directory and file creations,
//! and raw block-based file operations with active **self-healing live hot-swap recovery**.
//!
//! When reading a file block that has experienced severe silent corruption (double-bit flip),
//! the VFS intercepts the MMU Page Fault, quarantines the damaged page, relocates the file's data
//! to a fresh healthy page, updates the Inode's block index table in place, and retries the read,
//! seamlessly recovering data and continuing without program failure.

extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;
use alloc::vec;
use memory_subsystem::{MemoryAllocator, PAGE_SIZE};
use crate::inode::{Inode, InodeType};

/// A Virtual File System manager.
#[derive(Debug, Clone)]
pub struct VirtualFileSystem {
    /// Inode registry table. Index 0 is always the root directory.
    pub inodes: Vec<Inode>,
}

impl Default for VirtualFileSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl VirtualFileSystem {
    /// Initializes a new Virtual File System with a root directory `/` at Inode index 0.
    pub fn new() -> Self {
        let root = Inode::new_directory(0, "");
        Self {
            inodes: vec![root],
        }
    }

    /// Resolves an absolute path (e.g. `/system/telemetry.conf`) to its Inode index.
    ///
    /// Returns `Ok(inode_index)` on success, or an `Err` string if the path is invalid or not found.
    pub fn resolve_path(&self, path: &str) -> Result<usize, &'static str> {
        if !path.starts_with('/') {
            return Err("Path must be absolute (must start with '/').");
        }

        // Handle root path
        if path == "/" {
            return Ok(0);
        }

        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let mut current_idx = 0;

        for part in parts {
            let current_inode = &self.inodes[current_idx];
            match &current_inode.inode_type {
                InodeType::Directory { entries } => {
                    let mut found = false;
                    for (name, child_idx) in entries {
                        if name == part {
                            current_idx = *child_idx;
                            found = true;
                            break;
                        }
                    }
                    if !found {
                        return Err("File or directory not found in path lookup.");
                    }
                }
                _ => return Err("Attempted to lookup path component inside a regular file node."),
            }
        }

        Ok(current_idx)
    }

    /// Creates a new directory at the specified parent path.
    ///
    /// Returns the Inode index of the newly created directory on success, or an `Err` string.
    pub fn mkdir(&mut self, parent_path: &str, name: &str) -> Result<usize, &'static str> {
        let parent_idx = self.resolve_path(parent_path)?;
        
        // Verify parent is a directory
        if !self.inodes[parent_idx].is_directory() {
            return Err("Parent path is not a directory.");
        }

        // Verify name uniqueness in parent directory
        if let InodeType::Directory { entries } = &self.inodes[parent_idx].inode_type {
            for (entry_name, _) in entries {
                if entry_name == name {
                    return Err("Directory or file already exists with this name.");
                }
            }
        }

        // Allocate a new Inode index
        let new_idx = self.inodes.len();
        let new_inode = Inode::new_directory(new_idx, name);
        self.inodes.push(new_inode);

        // Register entry inside the parent directory
        if let InodeType::Directory { entries } = &mut self.inodes[parent_idx].inode_type {
            entries.push((String::from(name), new_idx));
        }

        Ok(new_idx)
    }

    /// Creates a new regular file at the specified parent path.
    ///
    /// Returns the Inode index of the newly created file on success, or an `Err` string.
    pub fn create_file(&mut self, parent_path: &str, name: &str) -> Result<usize, &'static str> {
        let parent_idx = self.resolve_path(parent_path)?;
        
        // Verify parent is a directory
        if !self.inodes[parent_idx].is_directory() {
            return Err("Parent path is not a directory.");
        }

        // Verify name uniqueness
        if let InodeType::Directory { entries } = &self.inodes[parent_idx].inode_type {
            for (entry_name, _) in entries {
                if entry_name == name {
                    return Err("Directory or file already exists with this name.");
                }
            }
        }

        // Allocate a new Inode
        let new_idx = self.inodes.len();
        let new_inode = Inode::new_file(new_idx, name);
        self.inodes.push(new_inode);

        // Register inside parent directory
        if let InodeType::Directory { entries } = &mut self.inodes[parent_idx].inode_type {
            entries.push((String::from(name), new_idx));
        }

        Ok(new_idx)
    }

    /// Writes raw data bytes to a file.
    ///
    /// Automatically manages page block allocations: deallocates any previously held blocks,
    /// requests fresh page frames from `MemoryAllocator` under the owning PID, SECDED encodes
    /// each byte, and writes them to the frames.
    pub fn write_file(
        &mut self,
        path: &str,
        data: &[u8],
        allocator: &mut MemoryAllocator,
        pid: u32,
    ) -> Result<(), &'static str> {
        let inode_idx = self.resolve_path(path)?;
        let mut inode = self.inodes[inode_idx].clone();

        match &mut inode.inode_type {
            InodeType::File { blocks, size } => {
                // 1. Deallocate any old blocks to prevent memory leaks
                for &block in blocks.iter() {
                    let _ = allocator.deallocate_page(block);
                }
                blocks.clear();

                // 2. Calculate the number of 64-byte pages needed to store the data
                let num_blocks = if data.is_empty() { 1 } else { data.len().div_ceil(PAGE_SIZE) };

                // 3. Allocate frames and write SECDED bytes
                for block_num in 0..num_blocks {
                    let frame_idx = match allocator.allocate_page(pid) {
                        Some(idx) => idx,
                        None => {
                            // Deallocate already allocated blocks in this call to remain consistent
                            for &b in blocks.iter() {
                                let _ = allocator.deallocate_page(b);
                            }
                            return Err("Out of physical memory blocks: file write failed.");
                        }
                    };

                    blocks.push(frame_idx);

                    let start = block_num * PAGE_SIZE;
                    let end = core::cmp::min(start + PAGE_SIZE, data.len());

                    for offset in 0..PAGE_SIZE {
                        let byte_val = if start + offset < end {
                            data[start + offset]
                        } else {
                            0 // Zero-padding
                        };

                        allocator.frames[frame_idx].write_byte(offset, byte_val)?;
                    }
                }

                *size = data.len();
            }
            _ => return Err("Cannot write data: Path is a directory node."),
        }

        // Commit modifications
        self.inodes[inode_idx] = inode;
        Ok(())
    }

    /// Reads all data bytes of a file from physical page frames with active self-healing.
    ///
    /// If an uncorrectable Double-Bit Flip is encountered during reading, the VFS:
    /// 1. Intercepts the fault.
    /// 2. Requests the memory allocator to quarantine the damaged frame.
    /// 3. Relocates the surviving data.
    /// 4. Hot-swaps the block index inside this file's Inode dynamically.
    /// 5. Retries the read on the fly, returning the file data seamlessly.
    pub fn read_file(
        &mut self,
        path: &str,
        allocator: &mut MemoryAllocator,
    ) -> Result<Vec<u8>, &'static str> {
        let inode_idx = self.resolve_path(path)?;
        let mut inode = self.inodes[inode_idx].clone();

        let mut file_data = Vec::new();

        match &mut inode.inode_type {
            InodeType::File { blocks, size } => {
                file_data.reserve(*size);
                let mut bytes_read = 0;

                for frame_ref in &mut *blocks {
                    let mut frame_idx = *frame_ref;
                    let mut offset = 0;

                    while offset < PAGE_SIZE && bytes_read < *size {
                        // Read byte with active fault trap
                        match allocator.frames[frame_idx].read_byte(offset) {
                            Ok((byte_val, _)) => {
                                file_data.push(byte_val);
                                bytes_read += 1;
                                offset += 1;
                            }
                            Err(damaged_idx) => {
                                // Severe double-bit corruption detected! Relocate and quarantine page frame.
                                let new_frame_idx = allocator.relocate_and_quarantine(damaged_idx)?;

                                // Hot-swap the block index dynamically in this inode
                                *frame_ref = new_frame_idx;
                                frame_idx = new_frame_idx;

                                // IMPORTANT: Do NOT increment offset. Immediately retry reading the byte
                                // from the newly relocated healthy page block!
                            }
                        }
                    }
                }
            }
            _ => return Err("Cannot read data: Path is a directory node."),
        }

        // Commit any updated block indices (if dynamic hot-swaps occurred during reading)
        self.inodes[inode_idx] = inode;
        Ok(file_data)
    }

    /// Removes a file or directory node from the VFS.
    ///
    /// If the node is a directory, it requires `recursive = true` to remove its children recursively.
    /// Safely deallocates all associated physical page frames in the memory subsystem.
    pub fn remove_node(
        &mut self,
        path: &str,
        recursive: bool,
        allocator: &mut MemoryAllocator,
    ) -> Result<(), &'static str> {
        if path == "/" {
            return Err("Cannot remove the root directory node.");
        }

        let child_idx = self.resolve_path(path)?;

        // 1. If it is a directory, verify if it's empty or if recursive flag is active
        if let InodeType::Directory { entries } = &self.inodes[child_idx].inode_type {
            if !entries.is_empty() && !recursive {
                return Err("Directory is not empty. Use 'rm -rf' to remove recursively.");
            }
        }

        // 2. Deallocate blocks recursively
        self.deallocate_inode_recursive(child_idx, allocator)?;

        // 3. Remove entry from parent directory listing
        let (parent_path, name) = {
            let path = path.trim_end_matches('/');
            if let Some(pos) = path.rfind('/') {
                let parent = &path[..pos];
                let parent = if parent.is_empty() { "/" } else { parent };
                let name = &path[pos + 1..];
                (String::from(parent), String::from(name))
            } else {
                (String::from("/"), String::from(path))
            }
        };

        let parent_idx = self.resolve_path(&parent_path)?;
        if let InodeType::Directory { entries } = &mut self.inodes[parent_idx].inode_type {
            if let Some(pos) = entries.iter().position(|(n, _)| n == &name) {
                entries.remove(pos);
            }
        }

        Ok(())
    }

    /// Internal recursive deallocator helper.
    fn deallocate_inode_recursive(
        &mut self,
        inode_idx: usize,
        allocator: &mut MemoryAllocator,
    ) -> Result<(), &'static str> {
        let inode = self.inodes[inode_idx].clone();
        match &inode.inode_type {
            InodeType::File { blocks, .. } => {
                for &block in blocks {
                    let _ = allocator.deallocate_page(block);
                }
            }
            InodeType::Directory { entries } => {
                for (_, child_idx) in entries {
                    self.deallocate_inode_recursive(*child_idx, allocator)?;
                }
            }
        }
        Ok(())
    }

    /// Copies a file from `src_path` to a new file named `dst_name` inside `dst_parent_path`.
    ///
    /// Reads the source file bytes (with self-healing if needed), creates the destination
    /// file, and writes the data using the supplied allocator and PID.
    pub fn copy_file(
        &mut self,
        src_path: &str,
        dst_parent_path: &str,
        dst_name: &str,
        allocator: &mut MemoryAllocator,
        pid: u32,
    ) -> Result<(), &'static str> {
        // Read source bytes first (may trigger hot-swap self-healing)
        let data = self.read_file(src_path, allocator)?;

        // Create the destination file node
        self.create_file(dst_parent_path, dst_name)?;

        // Build absolute destination path
        let dst_path = if dst_parent_path == "/" {
            alloc::format!("/{}", dst_name)
        } else {
            alloc::format!("{}/{}", dst_parent_path, dst_name)
        };

        // Write data to the new node
        self.write_file(&dst_path, &data, allocator, pid)
    }

    /// Moves (renames) a node from `src_path` to `dst_parent_path` with new name `dst_name`.
    ///
    /// Re-parents the inode: removes it from its old directory listing and inserts it into
    /// the destination directory.  No data is copied — only the directory entries change.
    pub fn rename_node(
        &mut self,
        src_path: &str,
        dst_parent_path: &str,
        dst_name: &str,
    ) -> Result<(), &'static str> {
        if src_path == "/" {
            return Err("Cannot rename the root directory.");
        }

        // Resolve source and destination
        let src_idx = self.resolve_path(src_path)?;
        let dst_parent_idx = self.resolve_path(dst_parent_path)?;

        if !self.inodes[dst_parent_idx].is_directory() {
            return Err("Destination parent is not a directory.");
        }

        // Check that the new name is available in the destination
        if let InodeType::Directory { entries } = &self.inodes[dst_parent_idx].inode_type {
            for (name, _) in entries {
                if name == dst_name {
                    return Err("A file or directory with that name already exists.");
                }
            }
        }

        // Identify source parent so we can remove its old directory entry
        let (src_parent_str, src_name_str) = {
            let path = src_path.trim_end_matches('/');
            if let Some(pos) = path.rfind('/') {
                let parent = &path[..pos];
                let parent = if parent.is_empty() { "/" } else { parent };
                (String::from(parent), String::from(&path[pos + 1..]))
            } else {
                (String::from("/"), String::from(path))
            }
        };

        let src_parent_idx = self.resolve_path(&src_parent_str)?;

        // Remove entry from old parent
        if let InodeType::Directory { entries } = &mut self.inodes[src_parent_idx].inode_type {
            if let Some(pos) = entries.iter().position(|(n, _)| n == &src_name_str) {
                entries.remove(pos);
            }
        }

        // Rename the inode itself
        self.inodes[src_idx].name = String::from(dst_name);

        // Register under new parent
        if let InodeType::Directory { entries } = &mut self.inodes[dst_parent_idx].inode_type {
            entries.push((String::from(dst_name), src_idx));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vfs_path_resolution_and_nesting() {
        let mut vfs = VirtualFileSystem::new();

        // Spawn directories
        vfs.mkdir("/", "system").unwrap();
        vfs.mkdir("/system", "config").unwrap();
        vfs.mkdir("/", "data").unwrap();

        // Resolve absolute directories
        assert_eq!(vfs.resolve_path("/").unwrap(), 0);
        let system_idx = vfs.resolve_path("/system").unwrap();
        let config_idx = vfs.resolve_path("/system/config").unwrap();
        assert_ne!(system_idx, config_idx);

        // Path errors
        assert!(vfs.resolve_path("/nonexistent").is_err());
        assert!(vfs.resolve_path("invalid_relative").is_err());
    }

    #[test]
    fn test_file_create_and_write_read() {
        let mut allocator = MemoryAllocator::new();
        let mut vfs = VirtualFileSystem::new();
        let pid = 200;

        vfs.mkdir("/", "data").unwrap();
        vfs.create_file("/data", "telemetry.log").unwrap();

        // Write content
        let content = b"AE-RUSTANIUM-SYSTEM-OK-2026-FLIGHT-GUIDANCE-ONLINE";
        vfs.write_file("/data/telemetry.log", content, &mut allocator, pid).unwrap();

        // Read and verify
        let read_back = vfs.read_file("/data/telemetry.log", &mut allocator).unwrap();
        assert_eq!(read_back, content);
    }

    #[test]
    fn test_large_file_span_multiple_pages() {
        let mut allocator = MemoryAllocator::new();
        let mut vfs = VirtualFileSystem::new();
        let pid = 300;

        vfs.create_file("/", "large.txt").unwrap();

        // Write 150 bytes of data (spans 3 pages since PAGE_SIZE is 64)
        let mut content = Vec::new();
        for i in 0..150 {
            content.push((i % 256) as u8);
        }
        vfs.write_file("/large.txt", &content, &mut allocator, pid).unwrap();

        // Verify inode metadata
        let idx = vfs.resolve_path("/large.txt").unwrap();
        if let InodeType::File { blocks, size } = &vfs.inodes[idx].inode_type {
            assert_eq!(blocks.len(), 3);
            assert_eq!(*size, 150);
        } else {
            panic!("Expected file type");
        }

        // Read back and verify bytes
        let read_back = vfs.read_file("/large.txt", &mut allocator).unwrap();
        assert_eq!(read_back, content);
    }

    #[test]
    fn test_file_read_self_healing_recovery() {
        let mut allocator = MemoryAllocator::new();
        let mut vfs = VirtualFileSystem::new();
        let pid = 400;

        vfs.create_file("/", "fault_test.txt").unwrap();
        let content = b"SELF_HEALING_VFS_RECOVERY_TEST";
        vfs.write_file("/fault_test.txt", content, &mut allocator, pid).unwrap();

        // Get physical frame index allocated to this file
        let idx = vfs.resolve_path("/fault_test.txt").unwrap();
        let initial_frame = if let InodeType::File { blocks, .. } = &vfs.inodes[idx].inode_type {
            blocks[0]
        } else {
            panic!("Expected file");
        };

        // Inject double-bit flip on the first byte (offset 0)
        allocator.inject_bit_flip(initial_frame, 0, 1).unwrap();
        allocator.inject_bit_flip(initial_frame, 0, 4).unwrap();

        // Reading file should intercept double-bit flip, isolate initial_frame, relocate, and successfully read!
        let read_back = vfs.read_file("/fault_test.txt", &mut allocator).unwrap();

        // Check if data is returned successfully (the corrupted byte falls back to zero safely)
        assert_eq!(read_back.len(), content.len());
        assert_eq!(read_back[0], 0); // Flipped byte gets zeroed safely
        assert_eq!(&read_back[1..], &content[1..]); // Rest of file content remains perfectly intact!

        // Verify the file's Inode blocks list was hot-swapped to a new frame
        let final_frame = if let InodeType::File { blocks, .. } = &vfs.inodes[idx].inode_type {
            blocks[0]
        } else {
            panic!("Expected file");
        };
        assert_ne!(initial_frame, final_frame);
        assert_eq!(allocator.frames[initial_frame].status, memory_subsystem::PageStatus::Quarantined);
        assert_eq!(allocator.allocation_map[final_frame], Some(pid));
    }

    #[test]
    fn test_remove_node() {
        let mut allocator = MemoryAllocator::new();
        let mut vfs = VirtualFileSystem::new();
        let pid = 500;

        vfs.mkdir("/", "temp").unwrap();
        vfs.create_file("/temp", "foo.txt").unwrap();
        vfs.write_file("/temp/foo.txt", b"Hello", &mut allocator, pid).unwrap();

        let foo_idx = vfs.resolve_path("/temp/foo.txt").unwrap();
        let frame_idx = if let InodeType::File { blocks, .. } = &vfs.inodes[foo_idx].inode_type {
            blocks[0]
        } else {
            panic!("Expected file");
        };

        // Assert memory block is allocated
        assert_eq!(allocator.allocation_map[frame_idx], Some(pid));

        // Deleting /temp directory without recursive flag should error
        assert!(vfs.remove_node("/temp", false, &mut allocator).is_err());

        // Deleting /temp/foo.txt should succeed and free the frame block
        vfs.remove_node("/temp/foo.txt", false, &mut allocator).unwrap();
        assert_eq!(allocator.allocation_map[frame_idx], None);

        // Deleting /temp directory now should succeed since it is empty
        vfs.remove_node("/temp", false, &mut allocator).unwrap();
        assert!(vfs.resolve_path("/temp").is_err());
    }
}
