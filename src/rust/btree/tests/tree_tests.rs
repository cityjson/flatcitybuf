#[cfg(test)]
mod tests {
    use btree::{BTree, BTreeIndex, I64KeyEncoder, MemoryBlockStorage};
    use std::collections::HashMap;

    // Helper function to create a test tree with integer keys
    fn create_test_tree() -> BTree<i64, MemoryBlockStorage> {
        let storage = MemoryBlockStorage::new(4096);
        let key_encoder = Box::new(I64KeyEncoder);
        BTree::new(storage, key_encoder).unwrap()
    }

    // Helper function to create a tree with some preset data
    fn create_populated_tree() -> BTree<i64, MemoryBlockStorage> {
        let storage = MemoryBlockStorage::new(4096);
        let key_encoder = Box::new(I64KeyEncoder);

        // Create sorted entries
        let entries = vec![
            (10, 100),
            (20, 200),
            (30, 300),
            (40, 400),
            (50, 500),
            (60, 600),
            (70, 700),
            (80, 800),
            (90, 900),
        ];

        BTree::build(storage, key_encoder, entries).unwrap()
    }

    #[test]
    fn test_new_tree_creation() {
        let tree = create_test_tree();

        // A new tree should have a root node
        assert!(tree.root_offset() > 0);
    }

    #[test]
    fn test_insert_and_search() {
        let mut tree = create_test_tree();

        // Insert some values
        tree.insert(&10, 100).unwrap();
        tree.insert(&20, 200).unwrap();
        tree.insert(&30, 300).unwrap();

        // Search for existing values
        assert_eq!(tree.search(&10).unwrap(), Some(100));
        assert_eq!(tree.search(&20).unwrap(), Some(200));
        assert_eq!(tree.search(&30).unwrap(), Some(300));

        // Search for non-existing value
        assert_eq!(tree.search(&40).unwrap(), None);

        // Update an existing value
        tree.insert(&20, 250).unwrap();
        assert_eq!(tree.search(&20).unwrap(), Some(250));
    }

    #[test]
    fn test_build_from_entries() {
        let tree = create_populated_tree();

        // Verify all entries are in the tree
        assert_eq!(tree.search(&10).unwrap(), Some(100));
        assert_eq!(tree.search(&50).unwrap(), Some(500));
        assert_eq!(tree.search(&90).unwrap(), Some(900));

        // Verify non-existing entries are not found
        assert_eq!(tree.search(&15).unwrap(), None);
        assert_eq!(tree.search(&95).unwrap(), None);
    }

    #[test]
    fn test_range_query() {
        let tree = create_populated_tree();

        // Query for range 20-60
        let results = tree.range_query(&20, &60).unwrap();
        assert_eq!(results.len(), 5); // Should include 20, 30, 40, 50, 60

        // Verify all expected values are in the results
        let expected = vec![200, 300, 400, 500, 600];
        for value in expected {
            assert!(results.contains(&value));
        }

        // Empty range query
        let results = tree.range_query(&25, &28).unwrap();
        assert_eq!(results.len(), 0);

        // Single-value range query
        let results = tree.range_query(&30, &30).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results.contains(&300));

        // Min to max range query
        let results = tree.range_query(&10, &90).unwrap();
        assert_eq!(results.len(), 9); // All 9 values
    }

    #[test]
    fn test_remove() {
        let mut tree = create_populated_tree();

        // Verify initial state
        assert_eq!(tree.search(&30).unwrap(), Some(300));

        // Remove an existing entry
        let result = tree.remove(&30).unwrap();
        assert!(result);

        // Verify entry is removed
        assert_eq!(tree.search(&30).unwrap(), None);

        // Range query should not include removed entry
        let results = tree.range_query(&20, &40).unwrap();
        assert_eq!(results.len(), 2); // Should include 20, 40
        assert!(!results.contains(&300));

        // Remove a non-existing entry
        let result = tree.remove(&35).unwrap();
        assert!(!result);

        // Size should include all entries except removed ones
        assert_eq!(tree.size().unwrap(), 8);
    }

    #[test]
    fn test_btree_index_trait() {
        // Create a tree and cast it to the BTreeIndex trait
        let tree = create_populated_tree();
        let tree_index: &dyn BTreeIndex = &tree;

        // Test exact_match via BTreeIndex trait
        let encoded_key = tree.key_encoder().encode(&50).unwrap();
        let result = tree_index.exact_match(&encoded_key).unwrap();
        assert_eq!(result, Some(500));

        // Test range_query via BTreeIndex trait
        let start_key = tree.key_encoder().encode(&20).unwrap();
        let end_key = tree.key_encoder().encode(&60).unwrap();

        let results = tree_index.range_query(&start_key, &end_key).unwrap();
        assert_eq!(results.len(), 5); // Should include 20, 30, 40, 50, 60

        // Verify all expected values are in the results
        for value in &[200, 300, 400, 500, 600] {
            assert!(results.contains(value));
        }
    }

    #[test]
    fn test_large_inserts() {
        let mut tree = create_test_tree();
        let mut expected = HashMap::new();

        // Insert a large number of entries
        for i in 0..100 {
            let key = i * 10; // 0, 10, 20, ... 990
            let value = (i * 100) as u64; // 0, 100, 200, ... 9900
            tree.insert(&key, value).unwrap();
            expected.insert(key, value);
        }

        // Verify all entries can be found
        for (key, expected_value) in &expected {
            let result = tree.search(key).unwrap();
            assert_eq!(result, Some(*expected_value));
        }

        // Check size
        assert_eq!(tree.size().unwrap(), 100);
    }

    #[test]
    fn test_tree_node_splitting() {
        // Create a tree with a small block size to force splitting
        let storage = MemoryBlockStorage::new(128); // Smaller block size
        let key_encoder = Box::new(I64KeyEncoder);

        // Create sorted entries
        let mut entries = Vec::new();
        for i in 0..20 {
            entries.push((i, i as u64 * 10));
        }

        // Build the tree from sorted entries
        let tree = BTree::build(storage, key_encoder, entries).unwrap();

        // Verify all entries are still accessible
        for i in 0..20 {
            let result = tree.search(&i).unwrap();
            assert_eq!(result, Some(i as u64 * 10), "Failed to find key {}", i);
        }
    }

    #[test]
    fn test_random_operations() {
        let mut tree = create_test_tree();
        let mut expected = HashMap::new();

        // Insert some random values
        let inserts = vec![
            (42, 420),
            (17, 170),
            (99, 990),
            (5, 50),
            (37, 370),
            (63, 630),
        ];

        for (key, value) in &inserts {
            tree.insert(key, *value).unwrap();
            expected.insert(*key, *value);
        }

        // Verify all values are there
        for (key, expected_value) in &expected {
            assert_eq!(tree.search(key).unwrap(), Some(*expected_value));
        }

        // Remove some values
        tree.remove(&17).unwrap();
        expected.remove(&17);

        tree.remove(&63).unwrap();
        expected.remove(&63);

        // Update some values
        tree.insert(&42, 421).unwrap();
        expected.insert(42, 421);

        // Verify the final state
        for (key, expected_value) in &expected {
            assert_eq!(tree.search(key).unwrap(), Some(*expected_value));
        }

        // Verify removed values are gone
        assert_eq!(tree.search(&17).unwrap(), None);
        assert_eq!(tree.search(&63).unwrap(), None);
    }
}
