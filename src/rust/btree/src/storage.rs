use crate::errors::{BTreeError, Result};
use lru::LruCache;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::num::NonZeroUsize;

/// Block storage interface for B-tree nodes
pub trait BlockStorage {
    /// Read a block at the given offset
    fn read_block(&self, offset: u64) -> Result<Vec<u8>>;

    /// Write a block at the given offset
    fn write_block(&mut self, offset: u64, data: &[u8]) -> Result<()>;

    /// Allocate a new block and return its offset
    fn allocate_block(&mut self) -> Result<u64>;

    /// Get the size of blocks in this storage
    fn block_size(&self) -> usize;

    /// Flush any pending writes
    fn flush(&mut self) -> Result<()>;
}

/// Memory-based block storage for testing and small datasets
#[derive(Debug)]
pub struct MemoryBlockStorage {
    /// Map from offset to block data
    blocks: HashMap<u64, Vec<u8>>,

    /// Next offset to allocate
    next_offset: u64,

    /// Size of blocks in bytes (typically 4096)
    block_size: usize,
}

impl MemoryBlockStorage {
    /// Create a new memory-based block storage
    pub fn new(block_size: usize) -> Self {
        Self {
            blocks: HashMap::new(),
            next_offset: 0,
            block_size,
        }
    }
}

impl BlockStorage for MemoryBlockStorage {
    fn read_block(&self, offset: u64) -> Result<Vec<u8>> {
        // Check alignment
        if offset % self.block_size as u64 != 0 {
            return Err(BTreeError::AlignmentError(offset));
        }

        // Retrieve block from memory
        self.blocks
            .get(&offset)
            .cloned()
            .ok_or(BTreeError::BlockNotFound(offset))
    }

    fn write_block(&mut self, offset: u64, data: &[u8]) -> Result<()> {
        // Check alignment
        if offset % self.block_size as u64 != 0 {
            return Err(BTreeError::AlignmentError(offset));
        }

        // Ensure block is exactly block_size
        let mut data_copy = data.to_vec();
        data_copy.resize(self.block_size, 0);

        // Store block in memory
        self.blocks.insert(offset, data_copy);
        Ok(())
    }

    fn allocate_block(&mut self) -> Result<u64> {
        // Allocate a new block
        let offset = self.next_offset;
        self.next_offset += self.block_size as u64;
        Ok(offset)
    }

    fn block_size(&self) -> usize {
        self.block_size
    }

    fn flush(&mut self) -> Result<()> {
        // Nothing to do for memory storage
        Ok(())
    }
}

/// File-based block storage with LRU cache
#[derive(Debug)]
pub struct CachedFileBlockStorage {
    /// Underlying file
    file: RefCell<File>,

    /// LRU cache of blocks
    cache: RefCell<LruCache<u64, Vec<u8>>>,

    /// Size of blocks in bytes (typically 4096)
    block_size: usize,
}

impl CachedFileBlockStorage {
    /// Create a new file-based block storage with cache
    pub fn new(file: File, block_size: usize, cache_size: usize) -> Self {
        // Convert cache_size to NonZeroUsize for LruCache
        let cache_size = NonZeroUsize::new(cache_size.max(1)).unwrap();

        Self {
            file: RefCell::new(file),
            cache: RefCell::new(LruCache::new(cache_size)),
            block_size,
        }
    }

    /// Prefetch next leaf node(s) for range query
    pub fn prefetch_next_leaves(&self, node_offset: u64, count: usize) -> Result<()> {
        // Prefetch next leaf nodes for range queries
        let mut current = node_offset;

        for _ in 0..count {
            // Read current node to get next node pointer
            // This is just a placeholder implementation
            let data = self.read_block(current)?;

            // In a real implementation, this would decode the node,
            // check if it's a leaf with a next pointer, and prefetch that node
            // if it exists

            // For now, just advance to the next block
            current += self.block_size as u64;
        }

        Ok(())
    }
}

impl BlockStorage for CachedFileBlockStorage {
    fn read_block(&self, offset: u64) -> Result<Vec<u8>> {
        // Check alignment
        if offset % self.block_size as u64 != 0 {
            return Err(BTreeError::AlignmentError(offset));
        }

        // Check cache first
        if let Some(data) = self.cache.borrow().get(&offset) {
            return Ok(data.clone());
        }

        // Cache miss - read from file
        let mut buffer = vec![0u8; self.block_size];
        let mut file = self.file.borrow_mut();
        file.seek(SeekFrom::Start(offset))?;
        file.read_exact(&mut buffer)?;

        // Update cache
        self.cache.borrow_mut().put(offset, buffer.clone());

        Ok(buffer)
    }

    fn write_block(&mut self, offset: u64, data: &[u8]) -> Result<()> {
        // Check alignment
        if offset % self.block_size as u64 != 0 {
            return Err(BTreeError::AlignmentError(offset));
        }

        // Ensure block is exactly block_size
        let mut data_copy = data.to_vec();
        data_copy.resize(self.block_size, 0);

        // Write to file
        let mut file = self.file.borrow_mut();
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(&data_copy)?;

        // Update cache
        self.cache.borrow_mut().put(offset, data_copy);
        Ok(())
    }

    fn allocate_block(&mut self) -> Result<u64> {
        // Get file length
        let mut file = self.file.borrow_mut();
        let offset = file.seek(SeekFrom::End(0))?;

        // Round up to next block_size boundary if needed
        let aligned_offset = (offset + self.block_size as u64 - 1) & !(self.block_size as u64 - 1);

        if aligned_offset > offset {
            // Pad file to ensure alignment
            let padding = vec![0u8; (aligned_offset - offset) as usize];
            file.write_all(&padding)?;
        }

        Ok(aligned_offset)
    }

    fn block_size(&self) -> usize {
        self.block_size
    }

    fn flush(&mut self) -> Result<()> {
        // Flush file to disk
        self.file.borrow_mut().flush()?;
        Ok(())
    }
}
