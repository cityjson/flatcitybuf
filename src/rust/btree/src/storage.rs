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

/// Generic block storage that can work with any type implementing Read + Write + Seek
///
/// This storage implementation allows B-trees to be embedded in any byte buffer
/// or specific sections of a file, making it ideal for composite data formats.
#[derive(Debug)]
pub struct GenericBlockStorage<T: Read + Write + Seek> {
    /// Underlying reader/writer/seeker
    source: RefCell<T>,

    /// Base offset where this storage begins
    base_offset: u64,

    /// End offset limit (None means no limit)
    end_offset: Option<u64>,

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

impl<T: Read + Write + Seek> GenericBlockStorage<T> {
    /// Create a new generic block storage with default settings
    pub fn new(source: T, block_size: usize, cache_size: usize) -> Self {
        Self::with_config(source, 0, None, block_size, cache_size, 4, 16)
    }

    /// Create a new generic block storage for a section of the source
    pub fn with_bounds(
        source: T,
        base_offset: u64,
        end_offset: Option<u64>,
        block_size: usize,
        cache_size: usize,
    ) -> Self {
        Self::with_config(
            source,
            base_offset,
            end_offset,
            block_size,
            cache_size,
            4,
            16,
        )
    }

    /// Create a new generic block storage with custom settings
    pub fn with_config(
        source: T,
        base_offset: u64,
        end_offset: Option<u64>,
        block_size: usize,
        cache_size: usize,
        max_prefetch: usize,
        max_buffered_writes: usize,
    ) -> Self {
        // Convert cache_size to NonZeroUsize for LruCache
        let cache_size = NonZeroUsize::new(cache_size.max(1)).unwrap();

        Self {
            source: RefCell::new(source),
            base_offset,
            end_offset,
            cache: RefCell::new(LruCache::new(cache_size)),
            block_size,
            max_prefetch,
            write_buffer: RefCell::new(HashMap::new()),
            max_buffered_writes,
        }
    }

    /// Prefetch a range of blocks for sequential access
    pub fn prefetch_blocks(&self, offset: u64, count: usize) -> Result<()> {
        let mut source = self.source.borrow_mut();
        let mut cache = self.cache.borrow_mut();

        // Limit the number of blocks to prefetch
        let count = count.min(self.max_prefetch);

        // Allocate buffer for all blocks at once
        let total_size = count * self.block_size;
        let mut buffer = vec![0u8; total_size];

        // Convert to absolute offset
        let absolute_offset = self.base_offset + offset;

        // Check if beyond end offset
        if let Some(end) = self.end_offset {
            if absolute_offset >= end {
                return Ok(());
            }
        }

        // Seek to start offset
        source.seek(SeekFrom::Start(absolute_offset))?;

        // Read all blocks in one operation
        let bytes_read = source.read(&mut buffer)?;

        if bytes_read == 0 {
            return Ok(()); // Nothing to read
        }

        // Split buffer into blocks and add to cache
        let blocks_read = bytes_read.div_ceil(self.block_size);

        for i in 0..blocks_read {
            let block_offset = offset + (i * self.block_size) as u64;
            let block_start = i * self.block_size;
            let block_end = block_start + self.block_size.min(bytes_read - block_start);

            // Only cache if we read a full block or reached EOF
            if block_end - block_start == self.block_size || block_end == bytes_read {
                let block_data = buffer[block_start..block_end].to_vec();
                cache.put(block_offset, block_data);
            }
        }

        Ok(())
    }

    /// Prefetch next leaf node(s) for range query optimization
    pub fn prefetch_next_leaves(&self, node_offset: u64, count: usize) -> Result<()> {
        // Read current node to get next node pointer
        let data = self.read_block(node_offset)?;

        // Parse the node to get the next_node pointer
        if data.len() >= 11 {
            // Extract next_node pointer from header (offset 3, size 8 bytes)
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

    /// Flush buffered writes to underlying storage
    fn flush_write_buffer(&self) -> Result<()> {
        let mut write_buffer = self.write_buffer.borrow_mut();

        if write_buffer.is_empty() {
            return Ok(());
        }

        // Sort writes by offset for sequential I/O
        let mut writes: Vec<_> = write_buffer.drain().collect();
        writes.sort_by_key(|(offset, _)| *offset);

        // Perform all writes
        let mut source = self.source.borrow_mut();
        for (offset, data) in writes {
            // Convert to absolute offset
            let absolute_offset = self.base_offset + offset;

            // Check if beyond end offset
            if let Some(end) = self.end_offset {
                if absolute_offset >= end {
                    return Err(BTreeError::IoError(format!(
                        "Write offset {} beyond storage bounds",
                        absolute_offset
                    )));
                }
            }

            source.seek(SeekFrom::Start(absolute_offset))?;
            source.write_all(&data)?;
        }

        // Ensure data is flushed
        source.flush()?;

        Ok(())
    }

    /// Check if a block is currently in the cache
    pub fn is_cached(&self, offset: u64) -> bool {
        self.cache.borrow().peek(&offset).is_some()
    }

    /// Clear the entire cache - useful for testing
    pub fn clear_cache(&mut self) {
        self.cache.borrow_mut().clear();
    }

    /// Get the base offset of this storage
    pub fn base_offset(&self) -> u64 {
        self.base_offset
    }

    /// Get the end offset of this storage, if limited
    pub fn end_offset(&self) -> Option<u64> {
        self.end_offset
    }
}

impl<T: Read + Write + Seek> BlockStorage for GenericBlockStorage<T> {
    fn read_block(&self, offset: u64) -> Result<Vec<u8>> {
        // Check alignment
        if offset % self.block_size as u64 != 0 {
            return Err(BTreeError::AlignmentError(offset));
        }

        // Calculate absolute offset
        let absolute_offset = self.base_offset + offset;

        // Check if beyond end offset
        if let Some(end) = self.end_offset {
            if absolute_offset >= end {
                return Err(BTreeError::BlockNotFound(offset));
            }
        }

        // Check if the offset is in cache first
        {
            let mut cache = self.cache.borrow_mut();
            if let Some(data) = cache.get(&offset) {
                return Ok(data.clone());
            }
        }

        // Check if the block is in the write buffer (not yet flushed)
        {
            let write_buffer = self.write_buffer.borrow();
            if let Some(data) = write_buffer.get(&offset) {
                // Add to cache to speed up future reads
                let mut cache = self.cache.borrow_mut();
                cache.put(offset, data.clone());
                return Ok(data.clone());
            }
        }

        // Read from underlying storage
        let mut source = self.source.borrow_mut();
        let mut buf = vec![0u8; self.block_size];
        source.seek(SeekFrom::Start(absolute_offset))?;
        let bytes_read = source.read(&mut buf)?;

        if bytes_read == 0 {
            return Err(BTreeError::BlockNotFound(offset));
        }

        // Resize buffer to actual bytes read
        buf.truncate(bytes_read);

        // Prefetch next blocks for sequential access
        if bytes_read == self.block_size {
            // Only prefetch if we read a full block (likely not at EOF)
            let next_offset = offset + self.block_size as u64;
            drop(source); // Release borrow before prefetching

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

        // Calculate absolute offset
        let absolute_offset = self.base_offset + offset;

        // Check if beyond end offset
        if let Some(end) = self.end_offset {
            if absolute_offset >= end {
                return Err(BTreeError::IoError(format!(
                    "Write offset {} beyond storage bounds",
                    absolute_offset
                )));
            }
        }

        // Ensure block is exactly block_size
        let mut data_copy = data.to_vec();
        data_copy.resize(self.block_size, 0);

        // Add to write buffer
        {
            let mut write_buffer = self.write_buffer.borrow_mut();
            write_buffer.insert(offset, data_copy.clone());

            // If buffer is full, flush to storage
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
        let mut source = self.source.borrow_mut();

        // Get the current position of the cursor
        let current_pos = source.stream_position()?;

        // Determine the starting position for allocation
        let start_pos = if current_pos < self.base_offset {
            self.base_offset
        } else {
            current_pos
        };

        // Calculate relative offset within our storage area
        let relative_offset = start_pos - self.base_offset;

        // For bounded storage, we need to respect the limits
        if let Some(end) = self.end_offset {
            // Calculate remaining space
            let max_relative_offset = end.saturating_sub(self.base_offset);

            // Check if we have enough space for another block
            if relative_offset + self.block_size as u64 > max_relative_offset {
                return Err(BTreeError::IoError("Exceeded storage limit".into()));
            }
        }

        // Round up to next block_size boundary if needed
        let aligned_relative =
            (relative_offset + self.block_size as u64 - 1) & !(self.block_size as u64 - 1);

        // Calculate the absolute position to seek to
        let absolute_position = self.base_offset + aligned_relative;

        // Ensure we're not exceeding our end boundary after alignment
        if let Some(end) = self.end_offset {
            if absolute_position + self.block_size as u64 > end {
                return Err(BTreeError::IoError(
                    "Exceeded storage limit after alignment".into(),
                ));
            }
        }

        // If needed, pad the storage up to the absolute position
        if absolute_position > current_pos {
            let padding_size = (absolute_position - current_pos) as usize;
            if padding_size > 0 {
                let padding = vec![0u8; padding_size];
                source.seek(SeekFrom::Start(current_pos))?;
                source.write_all(&padding)?;
            }
        }

        // Update cursor position
        source.seek(SeekFrom::Start(absolute_position + self.block_size as u64))?;

        Ok(aligned_relative)
    }

    fn block_size(&self) -> usize {
        self.block_size
    }

    fn flush(&mut self) -> Result<()> {
        // Flush buffered writes
        self.flush_write_buffer()?;
        // Also flush any source buffers
        self.source.borrow_mut().flush()?;
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
        let mut storage = GenericBlockStorage::new(file, 128, 10);

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

        // Initialize storage with a cache size of 2 blocks
        let mut storage = GenericBlockStorage::with_config(file, 0, None, 128, 2, 0, 0);

        // Allocate 3 blocks (0, 1, 2)
        let offsets: Vec<u64> = (0..3).map(|i| i * 128).collect();

        // Write unique data to each block
        for (i, &offset) in offsets.iter().enumerate() {
            let data = vec![i as u8; 5];
            storage.write_block(offset, &data).unwrap();
        }

        // Flush to ensure all blocks are written to disk
        storage.flush().unwrap();

        // Clear the cache to start fresh
        storage.clear_cache();

        // Verify no blocks are in cache
        for &offset in &offsets {
            assert!(
                !storage.is_cached(offset),
                "Block at offset {} should NOT be in cache",
                offset
            );
        }

        // Read block 0 and 1, which should fill the cache
        let _ = storage.read_block(offsets[0]).unwrap();
        let _ = storage.read_block(offsets[1]).unwrap();

        // Verify blocks 0 and 1 are in cache
        assert!(
            storage.is_cached(offsets[0]),
            "Block at offset {} should be in cache",
            offsets[0]
        );
        assert!(
            storage.is_cached(offsets[1]),
            "Block at offset {} should be in cache",
            offsets[1]
        );

        // Block 2 should not be in cache
        assert!(
            !storage.is_cached(offsets[2]),
            "Block at offset {} should NOT be in cache",
            offsets[2]
        );

        // Read block 2, which should cause block 0 to be evicted (LRU)
        let _ = storage.read_block(offsets[2]).unwrap();

        // Verify block 0 is no longer in cache
        assert!(
            !storage.is_cached(offsets[0]),
            "Block at offset {} should have been evicted",
            offsets[0]
        );

        // Blocks 1 and 2 should be in cache
        assert!(
            storage.is_cached(offsets[1]),
            "Block at offset {} should be in cache",
            offsets[1]
        );
        assert!(
            storage.is_cached(offsets[2]),
            "Block at offset {} should be in cache",
            offsets[2]
        );

        println!("cache eviction passed");
    }

    #[test]
    fn test_buffered_writes() {
        println!("testing buffered writes...");

        // Create a buffer instead of a file for more predictable testing
        let buffer = vec![0u8; 1024];
        let cursor = Cursor::new(buffer);

        // Create storage with max_buffered_writes=2 (will flush after 2 writes)
        let mut storage = GenericBlockStorage::with_config(cursor, 0, None, 128, 2, 0, 2);

        // Get the initial cursor position
        let pos_before = storage.source.borrow().position();

        // Write to 2 blocks
        for i in 0..2 {
            let offset = i * 128;
            let data = vec![i as u8 + 1; 5];
            storage.write_block(offset, &data).unwrap();
        }

        // Manually trigger flush to ensure write buffer is empty
        storage.flush().unwrap();

        // Get position after writes (should have advanced)
        let pos_after = storage.source.borrow().position();
        assert!(
            pos_after > pos_before,
            "Cursor position should have advanced after flush: {} -> {}",
            pos_before,
            pos_after
        );

        // Read back the data to verify it was written correctly
        let data1 = storage.read_block(0).unwrap();
        let data2 = storage.read_block(128).unwrap();

        assert_eq!(&data1[0..5], &[1, 1, 1, 1, 1]);
        assert_eq!(&data2[0..5], &[2, 2, 2, 2, 2]);

        println!("buffered writes passed");
    }

    #[test]
    fn test_prefetching() {
        println!("testing prefetching...");

        // Create a temporary file
        let file = tempfile().unwrap();

        // Create cached file storage with prefetching explicitly configured
        let mut storage = GenericBlockStorage::with_config(file, 0, None, 128, 5, 3, 2);

        // Allocate several consecutive blocks
        let offsets: Vec<u64> = (0..5).map(|i| i * 128).collect();

        // Write different data to each block
        for (i, offset) in offsets.iter().enumerate() {
            let data = vec![i as u8; 5];
            storage.write_block(*offset, &data).unwrap();
        }

        // Flush to disk
        storage.flush().unwrap();

        // Clear the cache to ensure next read comes from disk
        storage.clear_cache();

        // Verify no blocks are in cache
        for &offset in &offsets {
            assert!(
                !storage.is_cached(offset),
                "Block at offset {} should NOT be in cache",
                offset
            );
        }

        // Read block at index 1 - should trigger prefetching of blocks 2, 3, 4
        let _ = storage.read_block(offsets[1]).unwrap();

        // Block 1 should be in cache (the one we read)
        assert!(
            storage.is_cached(offsets[1]),
            "Block at offset {} should be in cache",
            offsets[1]
        );

        // Blocks 2, 3, 4 should be prefetched
        for i in 2..5 {
            assert!(
                storage.is_cached(offsets[i]),
                "Block at offset {} should be in cache",
                offsets[i]
            );
        }

        // Block 0 should NOT be in cache (it's before the one we read)
        assert!(
            !storage.is_cached(offsets[0]),
            "Block at offset {} should NOT be in cache",
            offsets[0]
        );

        println!("prefetching passed");
    }

    #[test]
    fn test_generic_block_storage_with_cursor() {
        println!("testing generic block storage with cursor...");

        // Create a Vec<u8> with some initial capacity
        let buffer = vec![0u8; 1024];
        let cursor = Cursor::new(buffer);

        // Create generic block storage with the cursor
        let mut storage = GenericBlockStorage::new(cursor, 128, 10);

        // Allocate a block
        let offset = storage.allocate_block().unwrap();

        // Write some data
        let data = vec![1, 2, 3, 4, 5];
        storage.write_block(offset, &data).unwrap();

        // Flush to ensure data is written
        storage.flush().unwrap();

        // Read the data back
        let read_data = storage.read_block(offset).unwrap();
        assert_eq!(read_data[0..5], data);

        println!("generic block storage with cursor passed");
    }

    #[test]
    fn test_generic_block_storage_with_bounds() {
        println!("testing generic block storage with bounds...");

        // Create a buffer of 2048 bytes
        let mut buffer = vec![0u8; 2048];

        // Pre-fill the buffer to simulate existing data up to the base offset
        for i in 0..512 {
            buffer[i] = 0xFF; // Fill with non-zero data
        }

        let mut cursor = Cursor::new(buffer);

        // Position cursor at the base offset
        cursor.set_position(512);

        // Create a storage with bounds starting at offset 512 with limit at 1024
        // This gives us space for 4 blocks of 128 bytes each (512 bytes total)
        let mut storage = GenericBlockStorage::with_bounds(
            cursor,
            512,        // Base offset
            Some(1024), // End offset (space for 4 blocks of 128 bytes)
            128,        // Block size
            10,         // Cache size
        );

        // Allocate two blocks - should succeed
        let offset1 = storage.allocate_block().unwrap();
        let offset2 = storage.allocate_block().unwrap();

        // The offsets should be relative to the base
        assert_eq!(offset1, 0);
        assert_eq!(offset2, 128);

        // Write data to both blocks
        storage.write_block(offset1, &vec![1, 2, 3]).unwrap();
        storage.write_block(offset2, &vec![4, 5, 6]).unwrap();

        // Flush the data
        storage.flush().unwrap();

        // Read the data back
        let data1 = storage.read_block(offset1).unwrap();
        let data2 = storage.read_block(offset2).unwrap();

        assert_eq!(data1[0..3], vec![1, 2, 3]);
        assert_eq!(data2[0..3], vec![4, 5, 6]);

        // Get the underlying cursor to verify the actual positions of data
        let inner_cursor = storage.source.borrow();
        let buffer = inner_cursor.get_ref();

        // Check that data was written at the correct absolute positions
        assert_eq!(buffer[512..515], vec![1, 2, 3]);
        assert_eq!(buffer[640..643], vec![4, 5, 6]);

        println!("generic block storage with bounds passed");
    }

    #[test]
    fn test_generic_block_storage_bounds_exceeded() {
        println!("testing generic block storage bounds check...");

        // Create a buffer of 1024 bytes
        let mut buffer = vec![0u8; 1024];

        // Pre-fill the buffer to simulate existing data up to base offset
        for i in 0..512 {
            buffer[i] = 0xFF;
        }

        let mut cursor = Cursor::new(buffer);

        // Position cursor at the base offset
        cursor.set_position(512);

        // Create a storage with tight bounds: 512-640 (just enough for 1 block)
        let mut storage = GenericBlockStorage::with_bounds(
            cursor,
            512,       // Base offset
            Some(640), // End offset (just enough for 1 block)
            128,       // Block size
            10,        // Cache size
        );

        // First block allocation should succeed
        match storage.allocate_block() {
            Ok(offset) => {
                assert_eq!(offset, 0);

                // Write to the first block should succeed
                storage.write_block(offset, &vec![1, 2, 3]).unwrap();

                // Second block allocation should fail since we're at the limit
                let result = storage.allocate_block();
                assert!(
                    result.is_err(),
                    "Should have failed to allocate beyond bounds"
                );

                if let Err(e) = result {
                    match e {
                        BTreeError::IoError(msg) => {
                            assert!(
                                msg.contains("Exceeded storage limit"),
                                "Expected 'Exceeded storage limit' error, got: {}",
                                msg
                            );
                        }
                        _ => panic!(
                            "Expected IoError with 'Exceeded storage limit' message, got: {:?}",
                            e
                        ),
                    }
                }

                // Direct write beyond bounds should fail
                let result = storage.write_block(128, &vec![4, 5, 6]);
                assert!(result.is_err(), "Should have failed to write beyond bounds");
            }
            Err(e) => {
                panic!(
                    "First block allocation should succeed, but got error: {:?}",
                    e
                );
            }
        }

        println!("generic block storage bounds check passed");
    }
}
