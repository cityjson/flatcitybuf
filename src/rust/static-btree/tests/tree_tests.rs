use static_btree::key::KeyType;
use static_btree::tree::StaticBTree;
use std::cmp::Ordering;

#[test]
fn test_tree_builder() {
    // Create a tree with branching factor 4
    let builder = StaticBTree::<i32>::builder(4, KeyType::I32);
    assert!(builder.build().is_err()); // Empty tree should error
}

#[test]
fn test_small_tree() {
    // Create a small tree with branching factor 4
    let mut builder = StaticBTree::<i32>::builder(4, KeyType::I32);

    // Add some entries with i32 keys
    for i in 0..10 {
        let key = i.to_le_bytes();
        builder.add_entry(&key, i as u64);
    }

    let tree = builder.build().unwrap();

    // Tree properties
    assert_eq!(tree.len(), 10);
    assert!(!tree.is_empty());
    assert_eq!(tree.branching_factor(), 4);

    // The height should be 2 for this small tree
    assert_eq!(tree.height(), 2);

    // Find values
    for i in 0..10 {
        let key = i.to_le_bytes();
        let result = tree.find(&key).unwrap();
        assert_eq!(result, Some(i as u64));
    }

    // Try to find a key that doesn't exist
    let non_existent = 100i32.to_le_bytes();
    assert_eq!(tree.find(&non_existent).unwrap(), None);
}

#[test]
fn test_medium_tree() {
    // Create a tree with branching factor 8
    let mut builder = StaticBTree::<i64>::builder(8, KeyType::I64);

    // Add 100 entries with i64 keys
    for i in 0..100 {
        let key = i.to_le_bytes();
        builder.add_entry(&key, (i * 2) as u64); // Value is key * 2
    }

    let tree = builder.build().unwrap();

    // Tree properties
    assert_eq!(tree.len(), 100);
    assert!(!tree.is_empty());
    assert_eq!(tree.branching_factor(), 8);

    // Find some values
    for i in 0..100 {
        let key = i.to_le_bytes();
        let result = tree.find(&key).unwrap();
        assert_eq!(result, Some((i * 2) as u64));
    }
}

#[test]
fn test_tree_with_duplicates() {
    // Create a tree with branching factor 4
    let mut builder = StaticBTree::<i32>::builder(4, KeyType::I32);

    // Add entries with some duplicates
    for i in 0..10 {
        let key = i.to_le_bytes();
        builder.add_entry(&key, i as u64);

        // Add a duplicate with a different value
        if i % 2 == 0 {
            builder.add_entry(&key, (i + 100) as u64);
        }
    }

    let tree = builder.build().unwrap();

    // Tree should deduplicate and keep the last value for each key
    assert_eq!(tree.len(), 10);

    // Check that the duplicates have the correct (last) values
    for i in 0..10 {
        let key = i.to_le_bytes();
        let expected = if i % 2 == 0 { i + 100 } else { i } as u64;
        let result = tree.find(&key).unwrap();
        assert_eq!(result, Some(expected));
    }
}

#[test]
fn test_range_query() {
    // Create a tree with branching factor 4
    let mut builder = StaticBTree::<i32>::builder(4, KeyType::I32);

    // Add 20 entries
    for i in 0..20 {
        let key = i.to_le_bytes();
        builder.add_entry(&key, i as u64);
    }

    let tree = builder.build().unwrap();

    // Range query for keys 5 to 15
    let start = 5i32.to_le_bytes();
    let end = 15i32.to_le_bytes();
    let results = tree.range(&start, &end).unwrap();

    // Should have 11 results (5 to 15 inclusive)
    assert_eq!(results.len(), 11);

    // Check that the keys and values are correct
    for (i, (key, value)) in results.iter().enumerate() {
        let expected_key = (i as i32 + 5).to_le_bytes();
        assert_eq!(key, &expected_key);
        assert_eq!(*value, (i as u64 + 5));
    }

    // Test empty range query (end < start)
    let start = 15i32.to_le_bytes();
    let end = 5i32.to_le_bytes();
    let results = tree.range(&start, &end).unwrap();
    assert_eq!(results.len(), 0);

    // Test range query outside of the tree bounds
    let start = 100i32.to_le_bytes();
    let end = 110i32.to_le_bytes();
    let results = tree.range(&start, &end).unwrap();
    assert_eq!(results.len(), 0);
}

#[test]
fn test_large_tree() {
    const N: usize = 1000;
    const BRANCHING_FACTOR: usize = 16;

    // Create a tree with branching factor 16
    let mut builder = StaticBTree::<i32>::builder(BRANCHING_FACTOR, KeyType::I32);

    // Add entries in reverse order to test sorting
    for i in (0..N).rev() {
        let key = i.to_le_bytes();
        builder.add_entry(&key, i as u64);
    }

    let tree = builder.build().unwrap();

    // Check tree properties
    assert_eq!(tree.len(), N);
    assert_eq!(tree.branching_factor(), BRANCHING_FACTOR);

    // Find all values to ensure tree is correctly built
    for i in 0..N {
        let key = i.to_le_bytes();
        let result = tree.find(&key).unwrap();
        assert_eq!(result, Some(i as u64), "Failed to find key {}", i);
    }

    // Check that non-existent keys return None
    let non_existent = (N + 100) as i32;
    let result = tree.find(&non_existent.to_le_bytes()).unwrap();
    assert_eq!(result, None);
}

#[test]
fn test_tree_with_unsigned_keys() {
    // Create a tree with u32 keys
    let mut builder = StaticBTree::<u32>::builder(8, KeyType::U32);

    // Add entries with u32 keys
    for i in 0..50 {
        let key = (i * 2).to_le_bytes(); // Use even numbers
        builder.add_entry(&key, i as u64);
    }

    let tree = builder.build().unwrap();

    // Find values
    for i in 0..50 {
        let key = (i * 2).to_le_bytes();
        let result = tree.find(&key).unwrap();
        assert_eq!(result, Some(i as u64));
    }

    // Check that odd numbers are not in the tree
    for i in 0..25 {
        let key = (i * 2 + 1).to_le_bytes();
        let result = tree.find(&key).unwrap();
        assert_eq!(result, None);
    }
}
