// Add #[cfg(test)] to ensure this is only compiled during tests
#[cfg(test)]
mod tests {
    use btree::{BTreeError, Entry, Node, NodeType};

    #[test]
    fn test_node_type_conversion() {
        // Test NodeType::from_u8
        assert_eq!(NodeType::from_u8(0).unwrap(), NodeType::Internal);
        assert_eq!(NodeType::from_u8(1).unwrap(), NodeType::Leaf);
        assert!(matches!(
            NodeType::from_u8(2),
            Err(BTreeError::InvalidNodeType { .. })
        ));

        // Test NodeType::to_u8
        assert_eq!(NodeType::Internal.to_u8(), 0);
        assert_eq!(NodeType::Leaf.to_u8(), 1);
    }

    #[test]
    fn test_node_creation() {
        // Test creation of internal and leaf nodes
        let internal = Node::new_internal();
        let leaf = Node::new_leaf();

        assert_eq!(internal.node_type, NodeType::Internal);
        assert_eq!(leaf.node_type, NodeType::Leaf);

        assert!(internal.entries.is_empty());
        assert!(leaf.entries.is_empty());

        assert_eq!(internal.next_node, None);
        assert_eq!(leaf.next_node, None);

        // Test is_leaf method
        assert!(!internal.is_leaf());
        assert!(leaf.is_leaf());
    }

    #[test]
    fn test_add_entry_and_find() {
        let mut node = Node::new_leaf();

        // Create some test entries
        let entry1 = Entry::new(vec![1, 2, 3], 100);
        let entry2 = Entry::new(vec![4, 5, 6], 200);
        let entry3 = Entry::new(vec![7, 8, 9], 300);

        // Add entries to node
        node.add_entry(entry1.clone());
        node.add_entry(entry2.clone());
        node.add_entry(entry3.clone());

        // Verify entries were added
        assert_eq!(node.entries.len(), 3);
        assert_eq!(node.entries[0].key, vec![1, 2, 3]);
        assert_eq!(node.entries[0].value, 100);
        assert_eq!(node.entries[1].key, vec![4, 5, 6]);
        assert_eq!(node.entries[1].value, 200);
        assert_eq!(node.entries[2].key, vec![7, 8, 9]);
        assert_eq!(node.entries[2].value, 300);

        // Test find_entry with a custom comparator
        let compare = |a: &[u8], b: &[u8]| a.cmp(b);

        // Find existing entry
        let idx = node.find_entry(&vec![4, 5, 6], compare);
        assert_eq!(idx, Some(1));

        // Try to find non-existing entry
        let idx = node.find_entry(&vec![9, 9, 9], compare);
        assert_eq!(idx, None);
    }

    #[test]
    fn test_node_encode_decode() {
        // Create a node with some entries
        let mut node = Node::new_leaf();
        node.next_node = Some(12345);

        // Add some entries
        // For encode/decode testing, ensure all keys are the same size
        node.add_entry(Entry::new(vec![1, 2, 3, 4], 100));
        node.add_entry(Entry::new(vec![5, 6, 7, 8], 200));
        node.add_entry(Entry::new(vec![9, 10, 11, 12], 300));

        // Encode the node
        let node_size = 256; // Choose a small size for testing
        let key_size = 4; // Our test keys are 4 bytes
        let encoded = node.encode(node_size, key_size).unwrap();

        // Make sure encoded data has expected size
        assert_eq!(encoded.len(), node_size);

        // Decode the node
        let decoded = Node::decode(&encoded, key_size).unwrap();

        // Verify node properties were preserved
        assert_eq!(decoded.node_type, NodeType::Leaf);
        assert_eq!(decoded.next_node, Some(12345));
        assert_eq!(decoded.entries.len(), 3);

        // Verify all entries were preserved
        assert_eq!(decoded.entries[0].key, vec![1, 2, 3, 4]);
        assert_eq!(decoded.entries[0].value, 100);
        assert_eq!(decoded.entries[1].key, vec![5, 6, 7, 8]);
        assert_eq!(decoded.entries[1].value, 200);
        assert_eq!(decoded.entries[2].key, vec![9, 10, 11, 12]);
        assert_eq!(decoded.entries[2].value, 300);
    }

    #[test]
    fn test_node_encode_too_many_entries() {
        // Create a node with too many entries for the node size
        let mut node = Node::new_leaf();

        // Key size = 10, value size = 8, entry size = 18
        // Header size = 12
        // With node_size = 48, max entries = (48 - 12) / 18 = 2

        // Add 3 entries (which is too many)
        node.add_entry(Entry::new(vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10], 100));
        node.add_entry(Entry::new(
            vec![11, 12, 13, 14, 15, 16, 17, 18, 19, 20],
            200,
        ));
        node.add_entry(Entry::new(
            vec![21, 22, 23, 24, 25, 26, 27, 28, 29, 30],
            300,
        ));

        // Attempt to encode
        let node_size = 48;
        let key_size = 10;
        let result = node.encode(node_size, key_size);

        // This should fail with a serialization error
        assert!(matches!(result, Err(BTreeError::Serialization(_))));
    }

    #[test]
    fn test_decode_incomplete_node() {
        // Test decoding with insufficient bytes
        let bytes = vec![1, 0, 0]; // Only 3 bytes, not enough for header
        let key_size = 4;
        let result = Node::decode(&bytes, key_size);

        // This should fail with a deserialization error
        assert!(matches!(result, Err(BTreeError::Deserialization(_))));
    }

    #[test]
    fn test_decode_insufficient_entries() {
        // Create bytes for node header but insufficient for entries
        let mut bytes = vec![0; 20]; // Only enough for header and partial entry
        bytes[0] = 1; // Leaf node
        bytes[1] = 2; // 2 entries (entry_count low byte)
        bytes[2] = 0; // entry_count high byte

        let key_size = 8;
        let result = Node::decode(&bytes, key_size);

        // This should fail with a deserialization error
        assert!(matches!(result, Err(BTreeError::Deserialization(_))));
    }

    #[test]
    fn test_next_node_roundtrip() {
        // Test that next_node gets properly encoded/decoded for leaf nodes

        // Test with next_node = None
        let mut node1 = Node::new_leaf();
        node1.add_entry(Entry::new(vec![1, 2, 3, 4], 100));
        let encoded1 = node1.encode(128, 4).unwrap();
        let decoded1 = Node::decode(&encoded1, 4).unwrap();
        assert_eq!(decoded1.next_node, None);

        // Test with next_node = Some(value)
        let mut node2 = Node::new_leaf();
        node2.next_node = Some(0xDEADBEEF);
        node2.add_entry(Entry::new(vec![1, 2, 3, 4], 100));
        let encoded2 = node2.encode(128, 4).unwrap();
        let decoded2 = Node::decode(&encoded2, 4).unwrap();
        assert_eq!(decoded2.next_node, Some(0xDEADBEEF));
    }
}
