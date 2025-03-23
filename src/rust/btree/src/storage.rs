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
            next_offset: block_size as u64, // Start at block_size to ensure first block is non-zero
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

    /// Maximum number of blocks to prefetch
    max_prefetch: usize,

    /// Write buffer to batch writes
    write_buffer: RefCell<HashMap<u64, Vec<u8>>>,

    /// Maximum number of buffered writes before automatic flush
    max_buffered_writes: usize,
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
            max_prefetch: 4, // Default to prefetching 4 blocks
            write_buffer: RefCell::new(HashMap::new()),
            max_buffered_writes: 16, // Default to buffering up to 16 writes
        }
    }

    /// Create a new file-based block storage with custom settings
    pub fn with_config(
        file: File,
        block_size: usize,
        cache_size: usize,
        max_prefetch: usize,
        max_buffered_writes: usize,
    ) -> Self {
        // Convert cache_size to NonZeroUsize for LruCache
        let cache_size = NonZeroUsize::new(cache_size.max(1)).unwrap();

        Self {
            file: RefCell::new(file),
            cache: RefCell::new(LruCache::new(cache_size)),
            block_size,
            max_prefetch,
            write_buffer: RefCell::new(HashMap::new()),
            max_buffered_writes,
        }
    }

    /// Prefetch a range of blocks for sequential access
    pub fn prefetch_blocks(&self, start_offset: u64, count: usize) -> Result<()> {
        let mut file = self.file.borrow_mut();
        let mut cache = self.cache.borrow_mut();

        // Limit the number of blocks to prefetch
        let count = count.min(self.max_prefetch);

        // Allocate buffer for all blocks at once
        let total_size = count * self.block_size;
        let mut buffer = vec![0u8; total_size];

        // Seek to start offset
        file.seek(SeekFrom::Start(start_offset))?;

        // Read all blocks in one operation
        let bytes_read = file.read(&mut buffer)?;

        if bytes_read == 0 {
            return Ok(()); // Nothing to read
        }

        // Split buffer into blocks and add to cache
        let blocks_read = (bytes_read + self.block_size - 1) / self.block_size;

        for i in 0..blocks_read {
            let offset = start_offset + (i * self.block_size) as u64;
            let block_start = i * self.block_size;
            let block_end = block_start + self.block_size.min(bytes_read - block_start);

            // Only cache if we read a full block or reached EOF
            if block_end - block_start == self.block_size || block_end == bytes_read {
                let block_data = buffer[block_start..block_end].to_vec();
                cache.put(offset, block_data);
            }
        }

        Ok(())
    }

    /// Prefetch next leaf node(s) for range query
    pub fn prefetch_next_leaves(&self, node_offset: u64, count: usize) -> Result<()> {
        // Read current node to get next node pointer
        let data = self.read_block(node_offset)?;

        // Parse the node to get the next_node pointer
        if data.len() >= 11 {
            // Extract next_node pointer from header
            // next_node is at offset 3, size 8 bytes
            let next_node_val = u64::from_le_bytes([
                data[3], data[4], data[5], data[6], data[7], data[8], data[9], data[10],
            ]);

            if next_node_val > 0 {
                // Prefetch blocks starting from next_node
                self.prefetch_blocks(next_node_val, count)?;

                // Recursively prefetch more nodes if needed
                if count > 1 {
                    self.prefetch_next_leaves(next_node_val, count - 1)?;
                }
            }
        }

        Ok(())
    }

    /// Flush buffered writes to disk
    fn flush_write_buffer(&self) -> Result<()> {
        let mut write_buffer = self.write_buffer.borrow_mut();

        if write_buffer.is_empty() {
            return Ok(());
        }

        // Sort writes by offset for sequential I/O
        let mut writes: Vec<_> = write_buffer.drain().collect();
        writes.sort_by_key(|(offset, _)| *offset);

        // Perform all writes
        let mut file = self.file.borrow_mut();
        for (offset, data) in writes {
            file.seek(SeekFrom::Start(offset))?;
            file.write_all(&data)?;
        }

        // Ensure data is flushed to disk
        file.flush()?;

        Ok(())
    }

    /// Check if a block is currently in the cache
    pub fn is_cached(&self, offset: u64) -> bool {
        self.cache.borrow().peek(&offset).is_some()
    }

    /// Clear the entire cache - useful for testing
    pub fn clear_cache(&mut self) {
        let mut cache = self.cache.borrow_mut();
        cache.clear();
    }

    /// Set the number of blocks to prefetch
    pub fn set_prefetch_count(&mut self, count: usize) {
        self.max_prefetch = count;
    }
}

impl BlockStorage for CachedFileBlockStorage {
    fn read_block(&self, offset: u64) -> Result<Vec<u8>> {
        // Check alignment
        if offset % self.block_size as u64 != 0 {
            return Err(BTreeError::AlignmentError(offset));
        }

        // Check if the offset is in cache first
        {
            let mut cache = self.cache.borrow_mut();
            if let Some(data) = cache.get(&offset) {
                return Ok(data.clone());
            }
        }

        // Check if the block is in the write buffer (not yet flushed to disk)
        {
            let write_buffer = self.write_buffer.borrow();
            if let Some(data) = write_buffer.get(&offset) {
                // Add to cache to speed up future reads
                let mut cache = self.cache.borrow_mut();
                cache.put(offset, data.clone());
                return Ok(data.clone());
            }
        }

        // Read from file
        let mut file = self.file.borrow_mut();
        let mut buf = vec![0u8; self.block_size];
        file.seek(SeekFrom::Start(offset))?;
        let bytes_read = file.read(&mut buf)?;

        if bytes_read == 0 {
            return Err(BTreeError::BlockNotFound(offset));
        }

        // Resize buffer to actual bytes read
        buf.truncate(bytes_read);

        // Prefetch next few blocks for sequential access
        if bytes_read == self.block_size {
            // Only prefetch if we read a full block (likely not at EOF)
            let next_offset = offset + self.block_size as u64;
            drop(file); // Release file borrow before prefetching

            // Try to prefetch, but ignore errors as this is just an optimization
            let _ = self.prefetch_blocks(next_offset, self.max_prefetch);
        }

        // Update cache
        {
            let mut cache = self.cache.borrow_mut();
            cache.put(offset, buf.clone());
        }

        Ok(buf)
    }

    fn write_block(&mut self, offset: u64, data: &[u8]) -> Result<()> {
        // Check alignment
        if offset % self.block_size as u64 != 0 {
            return Err(BTreeError::AlignmentError(offset));
        }

        // Ensure block is exactly block_size
        let mut data_copy = data.to_vec();
        data_copy.resize(self.block_size, 0);

        // Add to write buffer
        {
            let mut write_buffer = self.write_buffer.borrow_mut();
            write_buffer.insert(offset, data_copy.clone());

            // If buffer is full, flush to disk
            if write_buffer.len() >= self.max_buffered_writes {
                drop(write_buffer); // Release borrow before calling flush
                self.flush_write_buffer()?;
            }
        }

        // Update cache
        {
            let mut cache = self.cache.borrow_mut();
            cache.put(offset, data_copy);
        }

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
        // Flush buffered writes
        self.flush_write_buffer()?;

        // Also flush any file buffers
        self.file.borrow_mut().flush()?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use tempfile::tempfile;

    #[test]
    fn test_memory_block_storage() {
        println!("testing memory block storage...");

        // Create storage with block size 4096
        let mut storage = MemoryBlockStorage::new(4096);

        // First block should be at offset equal to block_size (4096)
        let offset = storage.allocate_block().unwrap();
        assert_eq!(offset, 4096);

        // Write some data
        let data = vec![1, 2, 3, 4, 5];
        storage.write_block(offset, &data).unwrap();

        // Read it back
        let read_data = storage.read_block(offset).unwrap();
        assert_eq!(read_data[0..5], data);

        // Next block should be at offset 8192
        let offset2 = storage.allocate_block().unwrap();
        assert_eq!(offset2, 8192);

        println!("memory block storage test passed");
    }

    #[test]
    fn test_cached_file_storage() {
        println!("testing cached file storage...");

        // Create a temporary file
        let file = tempfile().unwrap();

        // Create cached file storage
        let mut storage = CachedFileBlockStorage::new(file, 128, 10);

        // Allocate a block
        let offset = storage.allocate_block().unwrap();

        // Write some data
        let data = vec![1, 2, 3, 4, 5];
        storage.write_block(offset, &data).unwrap();

        // Flush to ensure data is written to disk
        storage.flush().unwrap();

        // Read the data back
        let read_data = storage.read_block(offset).unwrap();
        assert_eq!(read_data[0..5], data);

        // Allocate another block
        let offset2 = storage.allocate_block().unwrap();

        // Write different data
        let data2 = vec![6, 7, 8, 9, 10];
        storage.write_block(offset2, &data2).unwrap();

        // Read both blocks
        let read_data1 = storage.read_block(offset).unwrap();
        let read_data2 = storage.read_block(offset2).unwrap();

        assert_eq!(read_data1[0..5], data);
        assert_eq!(read_data2[0..5], data2);

        println!("cached file storage passed");
    }

    #[test]
    fn test_cache_eviction() {
        println!("testing cache eviction...");

        // Create a temporary file
        let file = tempfile().unwrap();

        // Create a cached file storage with small cache size (2)
        let mut storage = CachedFileBlockStorage::new(file, 128, 2);

        // Allocate 3 blocks
        let offsets: Vec<u64> = (0..3).map(|_| storage.allocate_block().unwrap()).collect();

        // Write unique data to each block
        for (i, offset) in offsets.iter().enumerate() {
            let data = vec![i as u8 + 1; 5]; // [1,1,1,1,1], [2,2,2,2,2], [3,3,3,3,3]
            storage.write_block(*offset, &data).unwrap();
        }

        // Flush to ensure all blocks are written to disk
        storage.flush().unwrap();

        // Read the blocks in reverse order to populate cache with blocks 2 and 1
        for i in (1..3).rev() {
            let _ = storage.read_block(offsets[i]).unwrap();
        }

        // Now read block 0 - this should cause block 2 to be evicted
        let _ = storage.read_block(offsets[0]).unwrap();

        // Modify the file directly to change block 2's data
        {
            let mut file = storage.file.borrow_mut();
            file.seek(SeekFrom::Start(offsets[2])).unwrap();
            file.write_all(&[9, 9, 9, 9, 9]).unwrap();
            file.flush().unwrap();
        }

        // Read block 2 again - should read from disk with new values
        let data = storage.read_block(offsets[2]).unwrap();
        assert_eq!(data[0..5], [9, 9, 9, 9, 9]);

        println!("cache eviction passed");
    }

    #[test]
    fn test_buffered_writes() {
        println!("testing buffered writes...");

        // Create a temporary file
        let file = tempfile().unwrap();

        // Create storage with 3 buffered writes
        let mut storage = CachedFileBlockStorage::with_config(file, 128, 5, 2, 3);

        // Write to 2 blocks (shouldn't trigger flush)
        for i in 0..2 {
            let offset = i * 128;
            let data = vec![i as u8 + 1; 5];
            storage.write_block(offset, &data).unwrap();
        }

        // Check file size - should still be 0 as nothing is flushed yet
        let file_size = storage.file.borrow().metadata().unwrap().len();
        assert_eq!(file_size, 0);

        // Write to one more block - should trigger auto-flush
        let data = vec![3; 5];
        storage.write_block(2 * 128, &data).unwrap();

        // File should now contain data
        let file_size = storage.file.borrow().metadata().unwrap().len();
        assert!(file_size > 0);

        println!("buffered writes passed");
    }

    #[test]
    fn test_prefetching() {
        println!("testing prefetching...");

        // Create a temporary file
        let file = tempfile().unwrap();

        // Create cached file storage with small cache but prefetching enabled
        let mut storage = CachedFileBlockStorage::new(file, 128, 5);
        storage.set_prefetch_count(3);

        // Allocate several consecutive blocks
        let offsets: Vec<u64> = (0..10).map(|_| storage.allocate_block().unwrap()).collect();

        // Write different data to each block
        for (i, offset) in offsets.iter().enumerate() {
            let data = vec![i as u8; 5];
            storage.write_block(*offset, &data).unwrap();
        }

        // Flush to disk
        storage.flush().unwrap();

        // Clear the cache to ensure next read comes from disk
        storage.clear_cache();

        // Read first block - should trigger prefetching of next 3 blocks
        let _ = storage.read_block(offsets[0]).unwrap();

        // The next 3 blocks should now be in cache
        for i in 1..4 {
            assert!(
                storage.is_cached(offsets[i]),
                "Block at offset {} should be in cache",
                offsets[i]
            );
        }

        // Block 4 should not be in cache yet
        assert!(
            !storage.is_cached(offsets[4]),
            "Block at offset {} should NOT be in cache",
            offsets[4]
        );

        println!("prefetching passed");
    }
}
