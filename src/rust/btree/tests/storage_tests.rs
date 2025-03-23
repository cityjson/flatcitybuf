#[cfg(test)]
mod tests {
    use btree::{BTreeError, BlockStorage, CachedFileBlockStorage, MemoryBlockStorage};
    use std::io::Cursor;
    use tempfile::tempfile;

    // MemoryBlockStorage Tests

    #[test]
    fn test_memory_storage_basic() {
        // Create a new memory storage with 4KB blocks
        let mut storage = MemoryBlockStorage::new(4096);

        // Check block size
        assert_eq!(storage.block_size(), 4096);

        // Allocate a block
        let offset = storage.allocate_block().unwrap();
        assert_eq!(offset, 4096); // First block should be at offset 4096

        // Write to block
        let data = vec![1, 2, 3, 4, 5];
        storage.write_block(offset, &data).unwrap();

        // Read from block
        let read_data = storage.read_block(offset).unwrap();
        assert_eq!(read_data[0..5], data); // First 5 bytes should match
        assert_eq!(read_data.len(), 4096); // Block should be full size

        // Test next allocation
        let offset2 = storage.allocate_block().unwrap();
        assert_eq!(offset2, 8192); // Second block at 8192
    }

    #[test]
    fn test_memory_storage_alignment() {
        let mut storage = MemoryBlockStorage::new(4096);

        // Trying to read/write at misaligned offsets should fail
        let result = storage.read_block(100);
        assert!(matches!(result, Err(BTreeError::AlignmentError(_))));

        let result = storage.write_block(100, &[1, 2, 3]);
        assert!(matches!(result, Err(BTreeError::AlignmentError(_))));
    }

    #[test]
    fn test_memory_storage_missing_block() {
        let storage = MemoryBlockStorage::new(4096);

        // Reading a non-existent block should return an error
        let result = storage.read_block(4096);
        assert!(matches!(result, Err(BTreeError::BlockNotFound(_))));
    }

    #[test]
    fn test_memory_storage_multiple_blocks() {
        let mut storage = MemoryBlockStorage::new(512);

        // Allocate and write to multiple blocks
        let mut offsets = Vec::new();

        for i in 0..10 {
            let offset = storage.allocate_block().unwrap();
            offsets.push(offset);

            // Write different data to each block
            let data = vec![i as u8; 10];
            storage.write_block(offset, &data).unwrap();
        }

        // Read back and verify
        for (i, offset) in offsets.iter().enumerate() {
            let read_data = storage.read_block(*offset).unwrap();
            assert_eq!(read_data[0], i as u8);
            assert_eq!(read_data[9], i as u8);
        }
    }

    // CachedFileBlockStorage Tests

    #[test]
    fn test_file_storage_basic() {
        // Create a temporary file for testing
        let file = tempfile().unwrap();

        // Create a file storage with 256-byte blocks and a small cache
        let mut storage = CachedFileBlockStorage::new(file, 256, 5);

        // Check block size
        assert_eq!(storage.block_size(), 256);

        // Allocate a block
        let offset = storage.allocate_block().unwrap();

        // Write to block
        let data = vec![1, 2, 3, 4, 5];
        storage.write_block(offset, &data).unwrap();

        // Flush to ensure data is written
        storage.flush().unwrap();

        // Read from block
        let read_data = storage.read_block(offset).unwrap();
        assert_eq!(read_data[0..5], data); // First 5 bytes should match
    }

    #[test]
    fn test_file_storage_persistence() {
        // Create a file in memory for testing persistence
        let file = tempfile().unwrap();

        // Write data using one storage instance
        {
            let mut storage = CachedFileBlockStorage::new(file.try_clone().unwrap(), 128, 2);
            let offset = storage.allocate_block().unwrap();
            storage.write_block(offset, &[10, 20, 30, 40, 50]).unwrap();
            storage.flush().unwrap();
        }

        // Create a new storage instance on the same file and read data back
        {
            let storage = CachedFileBlockStorage::new(file, 128, 2);
            let read_data = storage.read_block(0).unwrap();
            assert_eq!(read_data[0..5], [10, 20, 30, 40, 50]);
        }
    }

    #[test]
    fn test_file_storage_cache() {
        // Create a file for testing
        let file = tempfile().unwrap();

        // Create storage with small cache (only 2 blocks)
        let mut storage = CachedFileBlockStorage::new(file, 128, 2);

        // Allocate and write to 3 blocks
        let offset1 = storage.allocate_block().unwrap();
        let offset2 = storage.allocate_block().unwrap();
        let offset3 = storage.allocate_block().unwrap();

        storage.write_block(offset1, &[1; 10]).unwrap();
        storage.write_block(offset2, &[2; 10]).unwrap();
        storage.write_block(offset3, &[3; 10]).unwrap();
        storage.flush().unwrap();

        // Clear the cache to start fresh
        storage.clear_cache();

        // Read blocks 1 and 2, which should now be in cache
        let _ = storage.read_block(offset1).unwrap();
        let _ = storage.read_block(offset2).unwrap();

        // Both blocks should be cached
        assert!(storage.is_cached(offset1));
        assert!(storage.is_cached(offset2));
        assert!(!storage.is_cached(offset3));

        // Now read block 3, which should evict block 1 (LRU policy)
        let _ = storage.read_block(offset3).unwrap();

        // Block 1 should be evicted, 2 and 3 should be in cache
        assert!(!storage.is_cached(offset1));
        assert!(storage.is_cached(offset2));
        assert!(storage.is_cached(offset3));
    }

    #[test]
    fn test_file_storage_prefetch() {
        // Create a file for testing
        let file = tempfile().unwrap();

        // Create storage with prefetching enabled
        let mut storage = CachedFileBlockStorage::with_config(file, 128, 5, 3, 2);

        // Allocate and write to sequential blocks
        let mut offsets = Vec::new();
        for i in 0..5 {
            let offset = storage.allocate_block().unwrap();
            offsets.push(offset);
            storage.write_block(offset, &[i as u8; 10]).unwrap();
        }
        storage.flush().unwrap();

        // Clear the cache
        storage.clear_cache();

        // Read first block, which should trigger prefetch of next 3 blocks
        let _ = storage.read_block(offsets[0]).unwrap();

        // First 4 blocks should be cached due to prefetching
        assert!(storage.is_cached(offsets[0])); // The one we read
        assert!(storage.is_cached(offsets[1])); // Prefetched
        assert!(storage.is_cached(offsets[2])); // Prefetched
        assert!(storage.is_cached(offsets[3])); // Prefetched
        assert!(!storage.is_cached(offsets[4])); // Not prefetched
    }

    #[test]
    fn test_file_storage_write_buffer() {
        // Create a file for testing
        let file = tempfile().unwrap();

        // Create storage with a write buffer of 2 entries
        let mut storage =
            CachedFileBlockStorage::with_config(file.try_clone().unwrap(), 128, 5, 2, 2);

        // Allocate blocks
        let offset1 = storage.allocate_block().unwrap();
        let offset2 = storage.allocate_block().unwrap();
        let offset3 = storage.allocate_block().unwrap();

        // Write to 2 blocks (shouldn't trigger auto-flush yet)
        storage.write_block(offset1, &[1; 10]).unwrap();
        storage.write_block(offset2, &[2; 10]).unwrap();

        // Get the current file size
        let size_before = file.metadata().unwrap().len();

        // Write to third block (should trigger auto-flush)
        storage.write_block(offset3, &[3; 10]).unwrap();

        // Get the new file size
        let size_after = file.metadata().unwrap().len();

        // File size should have increased after auto-flush
        assert!(size_after > size_before);

        // Verify all blocks are readable
        assert_eq!(storage.read_block(offset1).unwrap()[0], 1);
        assert_eq!(storage.read_block(offset2).unwrap()[0], 2);
        assert_eq!(storage.read_block(offset3).unwrap()[0], 3);
    }

    #[test]
    fn test_file_storage_leaf_prefetch() {
        // Create a file for testing
        let file = tempfile().unwrap();
        let mut storage = CachedFileBlockStorage::new(file, 128, 5);

        // Allocate blocks for leaf nodes in a linked list
        let leaf1 = storage.allocate_block().unwrap();
        let leaf2 = storage.allocate_block().unwrap();
        let leaf3 = storage.allocate_block().unwrap();

        // Manually create a leaf node with next_node pointer
        // Header: node_type(1) + entry_count(2) + next_node(8) + reserved(1)
        let mut node_data = vec![0u8; 128];
        node_data[0] = 1; // Leaf node type
        node_data[1] = 1; // 1 entry
        node_data[2] = 0;
        // Set next_node pointer to leaf2
        node_data[3..11].copy_from_slice(&leaf2.to_le_bytes());
        // Entry (doesn't matter for this test)
        node_data[12] = 1; // Some key data

        // Write leaf1
        storage.write_block(leaf1, &node_data).unwrap();

        // Create leaf2 with next_node pointing to leaf3
        node_data[3..11].copy_from_slice(&leaf3.to_le_bytes());
        node_data[12] = 2; // Different key
        storage.write_block(leaf2, &node_data).unwrap();

        // Create leaf3 with no next_node
        node_data[3..11].copy_from_slice(&0u64.to_le_bytes());
        node_data[12] = 3; // Different key
        storage.write_block(leaf3, &node_data).unwrap();

        storage.flush().unwrap();

        // Clear cache
        storage.clear_cache();

        // Test prefetch_next_leaves with count=2
        storage.prefetch_next_leaves(leaf1, 2).unwrap();

        // Should have prefetched leaf2 and leaf3
        assert!(storage.is_cached(leaf2));
        assert!(storage.is_cached(leaf3));
    }
}
