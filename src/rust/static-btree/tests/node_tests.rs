use static_btree::entry::Entry;
use static_btree::node::{child_index, parent_index, Node, NodeType};

#[test]
fn test_node_creation() {
    let node = Node::new_internal();
    assert_eq!(node.node_type, NodeType::Internal);
    assert!(node.entries.is_empty());

    let node = Node::new_leaf();
    assert_eq!(node.node_type, NodeType::Leaf);
    assert!(node.entries.is_empty());
}

#[test]
fn test_node_add_entry() {
    let branching_factor = 4;
    let max_entries = branching_factor;

    let mut node = Node::new_leaf();

    // Add entries
    for i in 0..max_entries {
        let entry = Entry::new(vec![i as u8], i as u64);
        assert!(node.add_entry(entry, max_entries).is_ok());
    }

    // Node should be full
    let entry = Entry::new(vec![5], 5);
    assert!(node.add_entry(entry, max_entries).is_err());

    // Node should have max_entries entries
    assert_eq!(node.entries.len(), max_entries);
}

#[test]
fn test_node_serialization() {
    let key_size = 2;
    let branching_factor = 4;
    let max_entries = branching_factor;

    let mut node = Node::new_internal();

    // Add entries
    for i in 0..max_entries {
        let key = vec![0, i as u8]; // 2-byte keys
        let entry = Entry::new(key, i as u64);
        node.add_entry(entry, max_entries).unwrap();
    }

    // Encode node
    let encoded = node.encode(key_size).unwrap();

    // Header (4 bytes) + 4 entries (2+8 bytes each) = 44 bytes
    assert_eq!(encoded.len(), 4 + max_entries * (key_size + 8));

    // First byte should be NodeType::Internal
    assert_eq!(encoded[0], NodeType::Internal as u8);

    // Second and third bytes should be entry count (little-endian)
    assert_eq!(
        u16::from_le_bytes([encoded[1], encoded[2]]),
        max_entries as u16
    );

    // Decode node
    let decoded = Node::decode(&encoded, key_size).unwrap();

    // Check node properties
    assert_eq!(decoded.node_type, NodeType::Internal);
    assert_eq!(decoded.entries.len(), max_entries);

    // Check entries
    for i in 0..max_entries {
        assert_eq!(decoded.entries[i].key, vec![0, i as u8]);
        assert_eq!(decoded.entries[i].value, i as u64);
    }
}

#[test]
fn test_node_find_entry() {
    let mut node = Node::new_leaf();

    // Add entries
    for i in 0..4 {
        let entry = Entry::new(vec![i as u8], i as u64);
        node.add_entry(entry, 4).unwrap();
    }

    // Compare function
    let compare = |a: &[u8], b: &[u8]| a.cmp(b);

    // Find existing entries
    for i in 0..4 {
        let key = vec![i as u8];
        assert_eq!(node.find_entry(&key, compare), Some(i));
    }

    // Try to find non-existent entry
    let key = vec![5];
    assert_eq!(node.find_entry(&key, compare), None);
}

#[test]
fn test_node_find_lower_bound() {
    let mut node = Node::new_leaf();

    // Add entries with gaps
    for i in [0, 2, 4, 6].iter() {
        let entry = Entry::new(vec![*i], *i as u64);
        node.add_entry(entry, 4).unwrap();
    }

    // Compare function
    let compare = |a: &[u8], b: &[u8]| a.cmp(b);

    // Test exact matches
    assert_eq!(node.find_lower_bound(&vec![0], compare), 0);
    assert_eq!(node.find_lower_bound(&vec![2], compare), 1);
    assert_eq!(node.find_lower_bound(&vec![4], compare), 2);
    assert_eq!(node.find_lower_bound(&vec![6], compare), 3);

    // Test lower bounds
    assert_eq!(node.find_lower_bound(&vec![1], compare), 1); // points to 2
    assert_eq!(node.find_lower_bound(&vec![3], compare), 2); // points to 4
    assert_eq!(node.find_lower_bound(&vec![5], compare), 3); // points to 6
    assert_eq!(node.find_lower_bound(&vec![7], compare), 4); // points past end
}

#[test]
fn test_implicit_indexing() {
    // Test child index calculation
    let branching_factor = 4;

    // Root node (index 0) children
    assert_eq!(child_index(0, 0, branching_factor), 1);
    assert_eq!(child_index(0, 1, branching_factor), 2);
    assert_eq!(child_index(0, 2, branching_factor), 3);
    assert_eq!(child_index(0, 3, branching_factor), 4);

    // Node at index 1 children
    assert_eq!(child_index(1, 0, branching_factor), 6);
    assert_eq!(child_index(1, 1, branching_factor), 7);
    assert_eq!(child_index(1, 2, branching_factor), 8);
    assert_eq!(child_index(1, 3, branching_factor), 9);

    // Test parent index calculation
    assert_eq!(parent_index(0, branching_factor), None); // Root has no parent

    // Children of root
    assert_eq!(parent_index(1, branching_factor), Some((0, 0)));
    assert_eq!(parent_index(2, branching_factor), Some((0, 1)));
    assert_eq!(parent_index(3, branching_factor), Some((0, 2)));
    assert_eq!(parent_index(4, branching_factor), Some((0, 3)));

    // Children of node 1
    assert_eq!(parent_index(6, branching_factor), Some((1, 0)));
    assert_eq!(parent_index(7, branching_factor), Some((1, 1)));
    assert_eq!(parent_index(8, branching_factor), Some((1, 2)));
    assert_eq!(parent_index(9, branching_factor), Some((1, 3)));
}
