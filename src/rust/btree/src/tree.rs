use crate::entry::Entry;
use crate::errors::{BTreeError, Result};
use crate::key::KeyEncoder;
use crate::node::{Node, NodeType};
use crate::storage::BlockStorage;
use std::cmp::Ordering;
use std::marker::PhantomData;

/// Interface for B-tree index operations for use in queries
pub trait BTreeIndex {
    /// Execute an exact match query
    fn exact_match(&self, key: &[u8]) -> Result<Option<u64>>;

    /// Execute a range query
    fn range_query(&self, start: &[u8], end: &[u8]) -> Result<Vec<u64>>;

    /// Get encoded size of keys in this index
    fn key_size(&self) -> usize;
}

/// B-tree index structure
pub struct BTree<K, S> {
    /// Root node offset
    root_offset: u64,

    /// Storage for blocks
    storage: S,

    /// Key encoder
    key_encoder: Box<dyn KeyEncoder<K>>,

    /// Phantom data for key type
    _phantom: PhantomData<K>,

    /// Size of the tree
    size: usize,
}

impl<K, S: BlockStorage> BTree<K, S> {
    /// Create a new empty B-tree.
    pub fn new(mut storage: S, key_encoder: Box<dyn KeyEncoder<K>>) -> Result<Self> {
        let block_size = storage.block_size();
        let key_size = key_encoder.encoded_size();

        // Create a new root node (initially empty leaf)
        let root = Node::new_leaf();

        // Allocate block for root node
        let root_offset = storage.allocate_block()?;

        // Encode and store the root node
        let encoded = root.encode(block_size, key_size)?;
        storage.write_block(root_offset, &encoded)?;

        Ok(Self {
            storage,
            key_encoder,
            root_offset,
            size: 0,
            _phantom: PhantomData,
        })
    }

    /// Open an existing B-tree from storage
    pub fn open(storage: S, key_encoder: Box<dyn KeyEncoder<K>>, root_offset: u64) -> Self {
        // Open and initialize an existing B-tree
        Self {
            root_offset,
            storage,
            key_encoder,
            _phantom: PhantomData,
            size: 0,
        }
    }

    /// Get the root node offset
    pub fn root_offset(&self) -> u64 {
        self.root_offset
    }

    /// Get the key encoder
    pub fn key_encoder(&self) -> &dyn KeyEncoder<K> {
        self.key_encoder.as_ref()
    }

    /// Build a new B-tree from sorted entries
    pub fn build<I>(storage: S, key_encoder: Box<dyn KeyEncoder<K>>, entries: I) -> Result<Self>
    where
        I: IntoIterator<Item = (K, u64)>,
    {
        let mut builder = BTreeBuilder::new(storage, key_encoder);

        // Process all entries and build the tree
        for (key, value) in entries {
            builder.add_entry(key, value)?;
        }

        // Finalize and return the built tree
        builder.finalize()
    }

    /// Search for a key in the B-tree
    pub fn search(&self, key: &K) -> Result<Option<u64>> {
        // Encode the key
        let encoded_key = self.key_encoder.encode(key)?;

        // Start from root node
        let mut node_offset = self.root_offset;

        loop {
            // Read current node
            let node_data = self.storage.read_block(node_offset)?;
            let node = Node::decode(&node_data, self.key_encoder.encoded_size())?;

            // Process node based on type
            match node.node_type {
                NodeType::Internal => {
                    // Find child node to follow
                    match self.find_child_node(&node, &encoded_key)? {
                        Some(child_offset) => node_offset = child_offset,
                        None => return Ok(None), // Key not found
                    }
                }
                NodeType::Leaf => {
                    // Search for key in leaf node
                    return Ok(self.find_key_in_leaf(&node, &encoded_key));
                }
            }
        }
    }

    /// Range query to find all keys between start and end (inclusive)
    pub fn range_query(&self, start: &K, end: &K) -> Result<Vec<u64>> {
        // Encode start and end keys
        let encoded_start = self.key_encoder.encode(start)?;
        let encoded_end = self.key_encoder.encode(end)?;

        // Results vector
        let mut results = Vec::new();

        // Find leaf containing start key
        let leaf_offset = self.find_leaf_containing(&encoded_start)?;
        let mut current_offset = Some(leaf_offset);

        // Scan leaf nodes until we find the end key or run out of leaves
        while let Some(offset) = current_offset {
            // Read current leaf node
            let node_data = self.storage.read_block(offset)?;
            let node = Node::decode(&node_data, self.key_encoder.encoded_size())?;

            // Verify node is a leaf
            if node.node_type != NodeType::Leaf {
                return Err(BTreeError::InvalidStructure(
                    "Expected leaf node".to_string(),
                ));
            }

            // Add entries in range to results
            for entry in &node.entries {
                if entry.key.as_slice() >= encoded_start.as_slice()
                    && entry.key.as_slice() <= encoded_end.as_slice()
                {
                    results.push(entry.value);
                }

                // If we've passed the end key, we're done
                if entry.key.as_slice() > encoded_end.as_slice() {
                    return Ok(results);
                }
            }

            // Move to next leaf if there is one
            current_offset = node.next_node;
        }

        Ok(results)
    }

    /// Find the appropriate child node in an internal node
    fn find_child_node(&self, node: &Node, key: &[u8]) -> Result<Option<u64>> {
        // Binary search to find the right child
        if node.entries.is_empty() {
            return Ok(None);
        }

        let mut low = 0;
        let mut high = node.entries.len();

        while low < high {
            let mid = low + (high - low) / 2;
            let entry = &node.entries[mid];

            match self.key_encoder.compare(&entry.key, key) {
                Ordering::Less => low = mid + 1,
                _ => high = mid,
            }
        }

        // If we're at the end, use the last entry's child
        if low == node.entries.len() {
            low = node.entries.len() - 1;
        }

        Ok(Some(node.entries[low].value))
    }

    /// Find a key in a leaf node
    fn find_key_in_leaf(&self, node: &Node, key: &[u8]) -> Option<u64> {
        // Binary search for exact match
        node.entries
            .binary_search_by(|entry| self.key_encoder.compare(&entry.key, key))
            .ok()
            .map(|idx| node.entries[idx].value)
    }

    /// Find the leaf node containing the given key
    fn find_leaf_containing(&self, key: &[u8]) -> Result<u64> {
        let mut node_offset = self.root_offset;

        loop {
            let node_data = self.storage.read_block(node_offset)?;
            let node = Node::decode(&node_data, self.key_encoder.encoded_size())?;

            match node.node_type {
                NodeType::Internal => {
                    // Find child node that may contain the key
                    let child_offset = self.find_child_node(&node, key)?;
                    match child_offset {
                        Some(offset) => node_offset = offset,
                        None => {
                            // If no child found, use the rightmost child
                            if node.entries.is_empty() {
                                return Err(BTreeError::InvalidStructure(
                                    "Empty internal node".to_string(),
                                ));
                            }
                            node_offset = node.entries.last().unwrap().value;
                        }
                    }
                }
                NodeType::Leaf => {
                    // Found the leaf node that would contain this key if it exists
                    return Ok(node_offset);
                }
            }
        }
    }

    /// Insert a key-value pair into the B-tree
    pub fn insert(&mut self, key: &K, value: u64) -> Result<()> {
        // Encode the key
        let encoded_key = self.key_encoder.encode(key)?;

        // First, check if the key already exists (update value if it does)
        if let Some(existing) = self.search_for_update(&encoded_key)? {
            // Update existing entry (this will recursively update nodes)
            return self.update_entry(existing.0, existing.1, encoded_key, value);
        }

        // If we get here, we need to insert a new entry
        // Start from root and find the leaf node where this key belongs
        let mut path = Vec::new(); // Stack to track the path from root to leaf
        let mut current_offset = self.root_offset;

        // Traverse to the appropriate leaf node
        loop {
            let node_data = self.storage.read_block(current_offset)?;
            let node = Node::decode(&node_data, self.key_encoder.encoded_size())?;

            path.push((current_offset, node.clone()));

            if node.node_type == NodeType::Leaf {
                break; // Found the leaf node
            }

            // Find the appropriate child node
            let child_offset = self.find_child_node(&node, &encoded_key)?.ok_or_else(|| {
                BTreeError::InvalidStructure("Unable to find child node for insertion".to_string())
            })?;

            current_offset = child_offset;
        }

        // Calculate max entries per node
        let node_size = self.storage.block_size();
        let header_size = 12; // node_type(1) + entry_count(2) + next_node(8) + reserved(1)
        let entry_size = self.key_encoder.encoded_size() + 8; // key size + value size (u64)
        let max_entries_per_node = (node_size - header_size) / entry_size;

        // Get the leaf node (last in the path)
        let (leaf_offset, mut leaf_node) = path.pop().unwrap();

        // Create the new entry
        let new_entry = Entry::new(encoded_key, value);

        // Insert entry into leaf node, maintaining sort order
        let insert_pos = leaf_node
            .entries
            .binary_search_by(|entry| self.key_encoder.compare(&entry.key, &new_entry.key))
            .unwrap_or_else(|pos| pos); // If key doesn't exist, get the insertion position

        leaf_node.entries.insert(insert_pos, new_entry);

        // If leaf node is not full, just update it and return
        if leaf_node.entries.len() <= max_entries_per_node {
            let node_data = leaf_node.encode(node_size, self.key_encoder.encoded_size())?;
            self.storage.write_block(leaf_offset, &node_data)?;
            self.storage.flush()?;
            return Ok(());
        }

        // Otherwise, we need to split the node
        self.split_node(leaf_offset, leaf_node, path)?;

        Ok(())
    }

    /// Split a node and propagate the split up the tree if necessary
    fn split_node(
        &mut self,
        node_offset: u64,
        mut node: Node,
        mut path: Vec<(u64, Node)>,
    ) -> Result<()> {
        let node_size = self.storage.block_size();
        let key_size = self.key_encoder.encoded_size();

        // Calculate split point (midpoint)
        let split_pos = node.entries.len() / 2;

        // Create new right node with the second half of entries
        let mut right_node = Node::new(node.node_type);
        right_node.entries = node.entries.split_off(split_pos);

        // If this is a leaf node, maintain the linked list of leaves
        if node.node_type == NodeType::Leaf {
            // Update next pointers
            right_node.next_node = node.next_node;
            node.next_node = None; // Will be set after allocating the right node
        }

        // Allocate a block for the right node
        let right_offset = self.storage.allocate_block()?;

        // Update the left node's next_node pointer if it's a leaf
        if node.node_type == NodeType::Leaf {
            node.next_node = Some(right_offset);
        }

        // Get the first key of the right node to use as separator
        let separator_key = right_node.entries[0].key.clone();

        // Write the updated left node
        let left_data = node.encode(node_size, key_size)?;
        self.storage.write_block(node_offset, &left_data)?;

        // Write the new right node
        let right_data = right_node.encode(node_size, key_size)?;
        self.storage.write_block(right_offset, &right_data)?;

        // If the path is empty, we need to create a new root
        if path.is_empty() {
            // Create a new root node
            let mut new_root = Node::new_internal();

            // Add entries for both child nodes
            // The first entry uses a zeroed key (representing "everything less than separator_key")
            let left_entry = Entry::new(vec![0u8; key_size], node_offset);
            new_root.add_entry(left_entry);

            // The second entry uses the separator key
            let right_entry = Entry::new(separator_key, right_offset);
            new_root.add_entry(right_entry);

            // Allocate and write the new root
            let root_offset = self.storage.allocate_block()?;
            let root_data = new_root.encode(node_size, key_size)?;
            self.storage.write_block(root_offset, &root_data)?;

            // Update the tree's root offset
            self.root_offset = root_offset;
        } else {
            // Get the parent node
            let (parent_offset, mut parent) = path.pop().unwrap();

            // Create a new entry for the right child
            let new_entry = Entry::new(separator_key, right_offset);

            // Insert the new entry into the parent, maintaining sort order
            let insert_pos = parent
                .entries
                .binary_search_by(|entry| self.key_encoder.compare(&entry.key, &new_entry.key))
                .unwrap_or_else(|pos| pos);

            parent.entries.insert(insert_pos, new_entry);

            // Calculate max entries for parent
            let header_size = 12;
            let entry_size = key_size + 8;
            let max_entries = (node_size - header_size) / entry_size;

            // If parent isn't full, update it and return
            if parent.entries.len() <= max_entries {
                let parent_data = parent.encode(node_size, key_size)?;
                self.storage.write_block(parent_offset, &parent_data)?;
            } else {
                // Otherwise, recursively split the parent
                self.split_node(parent_offset, parent, path)?;
            }
        }

        Ok(())
    }

    /// Search for a key for updating its value
    fn search_for_update(&self, key: &[u8]) -> Result<Option<(u64, usize)>> {
        // Start from the root
        let mut node_offset = self.root_offset;

        loop {
            // Read the current node
            let node_data = self.storage.read_block(node_offset)?;
            let node = Node::decode(&node_data, self.key_encoder.encoded_size())?;

            match node.node_type {
                NodeType::Internal => {
                    // Find child node to follow
                    match self.find_child_node(&node, key)? {
                        Some(child_offset) => node_offset = child_offset,
                        None => return Ok(None), // Key not found
                    }
                }
                NodeType::Leaf => {
                    // Search for the key in this leaf
                    match node
                        .entries
                        .binary_search_by(|entry| self.key_encoder.compare(&entry.key, key))
                    {
                        Ok(index) => return Ok(Some((node_offset, index))),
                        Err(_) => return Ok(None), // Key not found
                    }
                }
            }
        }
    }

    /// Update an existing entry's value
    fn update_entry(
        &mut self,
        node_offset: u64,
        entry_index: usize,
        key: Vec<u8>,
        value: u64,
    ) -> Result<()> {
        // Read the node
        let node_data = self.storage.read_block(node_offset)?;
        let mut node = Node::decode(&node_data, self.key_encoder.encoded_size())?;

        // Update the entry's value
        node.entries[entry_index] = Entry::new(key, value);

        // Write the updated node back to storage
        let updated_data =
            node.encode(self.storage.block_size(), self.key_encoder.encoded_size())?;
        self.storage.write_block(node_offset, &updated_data)?;
        self.storage.flush()?;

        Ok(())
    }

    /// Remove a key-value pair from the B-tree
    pub fn remove(&mut self, key: &K) -> Result<bool> {
        // Encode the key
        let encoded_key = self.key_encoder.encode(key)?;

        // First, check if the key exists
        let found = match self.search_for_update(&encoded_key)? {
            Some((node_offset, index)) => {
                // Remove entry from leaf node
                self.remove_from_leaf(node_offset, index)?;
                true
            }
            None => false,
        };

        Ok(found)
    }

    /// Remove an entry from a leaf node
    fn remove_from_leaf(&mut self, node_offset: u64, entry_index: usize) -> Result<()> {
        // Read the node
        let node_data = self.storage.read_block(node_offset)?;
        let mut node = Node::decode(&node_data, self.key_encoder.encoded_size())?;

        // Ensure this is a leaf node
        if node.node_type != NodeType::Leaf {
            return Err(BTreeError::InvalidNodeType {
                expected: "Leaf".to_string(),
                actual: format!("{:?}", node.node_type),
            });
        }

        // Remove the entry
        node.entries.remove(entry_index);

        // Calculate minimum number of entries (for now, allow empty nodes)
        // In a real implementation, we might want to merge underfull nodes

        // Write the updated node back to storage
        let updated_data =
            node.encode(self.storage.block_size(), self.key_encoder.encoded_size())?;
        self.storage.write_block(node_offset, &updated_data)?;
        self.storage.flush()?;

        Ok(())
    }

    /// Get the number of entries in the tree (approximate)
    pub fn size(&self) -> Result<usize> {
        let mut count = 0;
        let mut visited = std::collections::HashSet::new();

        // Start from the leftmost leaf
        let leftmost_leaf = self.find_leftmost_leaf(self.root_offset)?;
        let mut current_offset = Some(leftmost_leaf);

        // Traverse all leaf nodes and count entries
        while let Some(offset) = current_offset {
            if visited.contains(&offset) {
                // Cycle detected
                break;
            }
            visited.insert(offset);

            // Read the node
            let node_data = self.storage.read_block(offset)?;
            let node = Node::decode(&node_data, self.key_encoder.encoded_size())?;

            // Count entries in this leaf
            count += node.entries.len();

            // Move to next leaf
            current_offset = node.next_node;
        }

        Ok(count)
    }

    /// Find the leftmost leaf node in the tree
    fn find_leftmost_leaf(&self, node_offset: u64) -> Result<u64> {
        let node_data = self.storage.read_block(node_offset)?;
        let node = Node::decode(&node_data, self.key_encoder.encoded_size())?;

        match node.node_type {
            NodeType::Leaf => Ok(node_offset),
            NodeType::Internal => {
                // Find the leftmost child
                if node.entries.is_empty() {
                    return Err(BTreeError::InvalidStructure(
                        "Empty internal node".to_string(),
                    ));
                }

                // Follow the first entry in the internal node
                self.find_leftmost_leaf(node.entries[0].value)
            }
        }
    }
}

impl<K, S: BlockStorage> BTreeIndex for BTree<K, S> {
    fn exact_match(&self, key: &[u8]) -> Result<Option<u64>> {
        // Start from root node
        let mut node_offset = self.root_offset;

        loop {
            // Read current node
            let node_data = self.storage.read_block(node_offset)?;
            let node = Node::decode(&node_data, self.key_encoder.encoded_size())?;

            // Process node based on type
            match node.node_type {
                NodeType::Internal => {
                    // Find child node to follow
                    match self.find_child_node(&node, key)? {
                        Some(child_offset) => node_offset = child_offset,
                        None => return Ok(None), // Key not found
                    }
                }
                NodeType::Leaf => {
                    // Search for key in leaf node
                    return Ok(self.find_key_in_leaf(&node, key));
                }
            }
        }
    }

    fn range_query(&self, start: &[u8], end: &[u8]) -> Result<Vec<u64>> {
        // Results vector
        let mut results = Vec::new();

        // Find leaf containing start key
        let mut current_offset = self.find_leaf_containing(start)?;

        // Scan leaf nodes until we find the end key or run out of leaves
        loop {
            // Read current leaf node
            let node_data = self.storage.read_block(current_offset)?;
            let node = Node::decode(&node_data, self.key_encoder.encoded_size())?;

            // Verify node is a leaf
            if node.node_type != NodeType::Leaf {
                return Err(BTreeError::InvalidNodeType {
                    expected: "Leaf".to_string(),
                    actual: format!("{:?}", node.node_type),
                });
            }

            // Process entries in this leaf
            for entry in &node.entries {
                match self.key_encoder.compare(&entry.key, end) {
                    // If entry key > end key, we're done
                    Ordering::Greater => return Ok(results),

                    // If entry key >= start key, include it in results
                    _ if self.key_encoder.compare(&entry.key, start) != Ordering::Less => {
                        results.push(entry.value);
                    }

                    // Otherwise, skip this entry (< start key)
                    _ => {}
                }
            }

            // Move to next leaf if available
            match node.next_node {
                Some(next_offset) => current_offset = next_offset,
                None => break, // No more leaves
            }
        }

        Ok(results)
    }

    fn key_size(&self) -> usize {
        self.key_encoder.encoded_size()
    }
}

/// Helper for building a B-tree from sorted entries
struct BTreeBuilder<K, S: BlockStorage> {
    /// Storage for blocks
    storage: S,

    /// Key encoder
    key_encoder: Box<dyn KeyEncoder<K>>,

    /// Leaf nodes being built
    leaf_nodes: Vec<u64>,

    /// Current leaf node being filled
    current_leaf: Node,

    /// Key size in bytes
    key_size: usize,

    /// Current level internal nodes
    current_level: Vec<u64>,

    /// Node size in bytes
    node_size: usize,
}

impl<K, S: BlockStorage> BTreeBuilder<K, S> {
    /// Create a new B-tree builder with the given storage and key encoder
    pub fn new(storage: S, key_encoder: Box<dyn KeyEncoder<K>>) -> Self {
        let key_size = key_encoder.encoded_size();
        let node_size = storage.block_size();

        // Calculate max entries per node based on node size and key size
        // Each entry takes key_size + 8 bytes (for the value)
        // Header takes 12 bytes (node_type + entry_count + next_node + reserved)
        let max_entries_per_node = (node_size - 12) / (key_size + 8);

        Self {
            storage,
            key_encoder,
            leaf_nodes: Vec::new(),
            current_leaf: Node::new_leaf(),
            key_size,
            current_level: Vec::new(),
            node_size,
        }
    }

    /// Add an entry to the B-tree being built
    pub fn add_entry(&mut self, key: K, value: u64) -> Result<()> {
        // Encode the key
        let encoded_key = self.key_encoder.encode(&key)?;

        // Create an entry
        let entry = Entry::new(encoded_key, value);

        // Add entry to current leaf node
        self.current_leaf.add_entry(entry);

        // Check if the node is full
        let max_entries_per_node = (self.node_size - 12) / (self.key_size + 8);
        if self.current_leaf.entries.len() >= max_entries_per_node {
            // Flush current leaf if it's full
            self.flush_current_leaf()?;
        }

        Ok(())
    }

    /// Flush the current leaf node to storage
    fn flush_current_leaf(&mut self) -> Result<()> {
        if self.current_leaf.entries.is_empty() {
            return Ok(());
        }

        // Sort entries by key (should already be sorted if inputs were sorted)
        self.current_leaf
            .entries
            .sort_by(|a, b| self.key_encoder.compare(&a.key, &b.key));

        // Allocate a block for the node
        let offset = self.storage.allocate_block()?;

        // Set the next pointer to None (will be updated later)
        self.current_leaf.next_node = None;

        // Encode the node
        let node_data = self.current_leaf.encode(self.node_size, self.key_size)?;

        // Write the node to storage
        self.storage.write_block(offset, &node_data)?;

        // Add this node to the list of leaf nodes
        self.leaf_nodes.push(offset);

        // Create a new leaf node for subsequent entries
        self.current_leaf = Node::new_leaf();

        Ok(())
    }

    /// Build internal nodes for the current level
    fn build_internal_level(&mut self, nodes: &[u64]) -> Result<Vec<u64>> {
        if nodes.is_empty() {
            return Ok(Vec::new());
        }

        // Max entries per internal node
        let max_entries_per_node = (self.node_size - 12) / (self.key_size + 8);

        let mut parent_nodes = Vec::new();
        let mut current_node = Node::new_internal();
        let mut first_key = true;

        for &node_offset in nodes {
            // Read the node to get its first key
            let node_data = self.storage.read_block(node_offset)?;
            let node = Node::decode(&node_data, self.key_size)?;

            if !node.entries.is_empty() {
                // Skip the first key for the first entry in an internal node
                if !first_key {
                    // Use the first key of the node as separator key
                    let sep_key = node.entries[0].key.clone();
                    current_node.add_entry(Entry::new(sep_key, node_offset));
                } else {
                    // First entry doesn't need a separator key
                    // We'll just store the pointer - the key doesn't matter
                    // as it's implicitly "less than the next key"
                    let dummy_key = vec![0u8; self.key_size];
                    current_node.add_entry(Entry::new(dummy_key, node_offset));
                    first_key = false;
                }

                // Check if node is full
                if current_node.entries.len() >= max_entries_per_node {
                    // Write the node and start a new one
                    let parent_offset = self.storage.allocate_block()?;
                    let node_data = current_node.encode(self.node_size, self.key_size)?;
                    self.storage.write_block(parent_offset, &node_data)?;
                    parent_nodes.push(parent_offset);

                    current_node = Node::new_internal();
                    first_key = true;
                }
            }
        }

        // Write the last node if it has entries
        if !current_node.entries.is_empty() {
            let parent_offset = self.storage.allocate_block()?;
            let node_data = current_node.encode(self.node_size, self.key_size)?;
            self.storage.write_block(parent_offset, &node_data)?;
            parent_nodes.push(parent_offset);
        }

        Ok(parent_nodes)
    }

    /// Link leaf nodes together to form a linked list for efficient range queries
    fn link_leaf_nodes(&mut self) -> Result<()> {
        for i in 0..self.leaf_nodes.len() - 1 {
            let current_offset = self.leaf_nodes[i];
            let next_offset = self.leaf_nodes[i + 1];

            // Read current node
            let node_data = self.storage.read_block(current_offset)?;
            let mut node = Node::decode(&node_data, self.key_size)?;

            // Set next node pointer
            node.next_node = Some(next_offset);

            // Write updated node
            let node_data = node.encode(self.node_size, self.key_size)?;
            self.storage.write_block(current_offset, &node_data)?;
        }

        Ok(())
    }

    /// Finalize the B-tree construction and return the tree
    pub fn finalize(mut self) -> Result<BTree<K, S>> {
        // Flush any remaining entries in the current leaf
        self.flush_current_leaf()?;

        // If no leaves were created, create a single empty leaf
        if self.leaf_nodes.is_empty() {
            let root_node = Node::new_leaf();
            let root_data = root_node.encode(self.node_size, self.key_size)?;
            let root_offset = self.storage.allocate_block()?;
            self.storage.write_block(root_offset, &root_data)?;

            // Create and return the tree
            return Ok(BTree {
                root_offset,
                storage: self.storage,
                key_encoder: self.key_encoder,
                _phantom: PhantomData,
                size: 0,
            });
        }

        // Link leaf nodes together for range queries
        self.link_leaf_nodes()?;

        // Build the internal nodes (bottom-up)
        let mut current_level = self.leaf_nodes.clone();

        // Build internal nodes level by level until we have a single root
        while current_level.len() > 1 {
            current_level = self.build_internal_level(&current_level)?;
        }

        // The single node in the last level is the root
        let root_offset = current_level[0];

        // Flush any pending writes
        self.storage.flush()?;

        // Create and return the tree
        Ok(BTree {
            root_offset,
            storage: self.storage,
            key_encoder: self.key_encoder,
            _phantom: PhantomData,
            size: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::I64KeyEncoder;
    use crate::storage::MemoryBlockStorage;
    use std::collections::HashMap;

    // Helper function to create a test tree with integer keys
    fn create_test_tree() -> Result<BTree<i64, MemoryBlockStorage>> {
        let storage = MemoryBlockStorage::new(4096);
        let key_encoder = Box::new(I64KeyEncoder);
        BTree::new(storage, key_encoder)
    }

    // Helper function to create a test tree with a specific block size
    fn create_test_tree_with_storage(
        storage: MemoryBlockStorage,
    ) -> Result<BTree<i64, MemoryBlockStorage>> {
        let key_encoder = Box::new(I64KeyEncoder);
        BTree::new(storage, key_encoder)
    }

    // Helper function to create a tree with some preset data
    fn create_populated_tree() -> Result<BTree<i64, MemoryBlockStorage>> {
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

        BTree::build(storage, key_encoder, entries)
    }

    #[test]
    fn test_new_tree_creation() {
        println!("testing new tree creation...");
        let tree = create_test_tree().unwrap();

        // A new tree should have a root node
        assert!(tree.root_offset() > 0);
        println!("new tree creation passed");
    }

    #[test]
    fn test_build_tree_from_entries() {
        println!("testing tree building from entries...");
        let tree = create_populated_tree().unwrap();

        // Tree should have entries
        let size = tree.size().unwrap();
        assert_eq!(size, 9);
        println!("tree building from entries passed");
    }

    #[test]
    fn test_search() {
        println!("testing search...");
        let tree = create_populated_tree().unwrap();

        // Search for existing key
        let result = tree.search(&30).unwrap();
        assert_eq!(result, Some(300));

        // Search for non-existing key
        let result = tree.search(&35).unwrap();
        assert_eq!(result, None);

        println!("search passed");
    }

    #[test]
    fn test_range_query() {
        println!("testing range query...");
        let tree = create_populated_tree().unwrap();

        // Query for range 20-60
        let results = tree.range_query(&20, &60).unwrap();
        assert_eq!(results.len(), 5); // Should include 20, 30, 40, 50, 60
        assert!(results.contains(&200));
        assert!(results.contains(&300));
        assert!(results.contains(&400));
        assert!(results.contains(&500));
        assert!(results.contains(&600));

        // Query for empty range
        let results = tree.range_query(&25, &28).unwrap();
        assert_eq!(results.len(), 0);

        println!("range query passed");
    }

    #[test]
    fn test_insert() {
        println!("testing insert...");
        let mut tree = create_test_tree().unwrap();

        // Insert some keys
        tree.insert(&10, 100).unwrap();
        tree.insert(&20, 200).unwrap();
        tree.insert(&30, 300).unwrap();

        // Verify keys were inserted
        let result = tree.search(&10).unwrap();
        assert_eq!(result, Some(100));

        let result = tree.search(&20).unwrap();
        assert_eq!(result, Some(200));

        let result = tree.search(&30).unwrap();
        assert_eq!(result, Some(300));

        // Update an existing key
        tree.insert(&20, 250).unwrap();
        let result = tree.search(&20).unwrap();
        assert_eq!(result, Some(250));

        println!("insert passed");
    }

    #[test]
    fn test_remove() {
        println!("testing remove...");
        let mut tree = create_populated_tree().unwrap();

        // Initial size check
        let size_before = tree.size().unwrap();
        assert_eq!(size_before, 9);

        // Remove an existing key
        let result = tree.remove(&30).unwrap();
        assert!(result);

        // Search for removed key
        let search_result = tree.search(&30).unwrap();
        assert_eq!(search_result, None);

        // Size should be reduced
        let size_after = tree.size().unwrap();
        assert_eq!(size_after, 8);

        // Remove a non-existing key
        let result = tree.remove(&35).unwrap();
        assert!(!result);

        // Size should be unchanged
        let size_after_again = tree.size().unwrap();
        assert_eq!(size_after_again, 8);

        println!("remove passed");
    }

    #[test]
    fn test_large_insert() {
        println!("testing large insert...");

        // Create a tree with small block size to force splitting
        let storage = MemoryBlockStorage::new(128);
        let mut tree = create_test_tree_with_storage(storage).unwrap();

        // Insert 100 entries
        for i in 0..100i64 {
            tree.insert(&i, i as u64 * 10).unwrap();
        }

        // Verify all entries were inserted correctly
        for i in 0..100i64 {
            let result = tree.search(&i).unwrap();
            assert_eq!(result, Some(i as u64 * 10), "Failed to find key {}", i);
        }

        // Verify tree size
        assert_eq!(tree.size().unwrap(), 100, "Tree size should be 100");

        println!("large insert test passed");
    }

    #[test]
    fn test_complex_operations() {
        println!("testing complex operations...");

        // Create a tree with some initial data
        let storage = MemoryBlockStorage::new(512);
        let key_encoder = Box::new(I64KeyEncoder);
        let mut tree = BTree::new(storage, key_encoder).unwrap();

        // Insert some entries
        for i in 1..=10 {
            tree.insert(&i, (i * 10) as u64).unwrap();
        }

        // Remove some entries
        tree.remove(&2).unwrap();
        tree.remove(&4).unwrap();
        tree.remove(&6).unwrap();
        tree.remove(&8).unwrap();

        // Check size (should be 6 entries remaining)
        let size = tree.size().unwrap();
        assert_eq!(size, 6, "Expected 6 entries after removals, got {}", size);

        // Check that removed entries are gone
        assert_eq!(tree.search(&2).unwrap(), None);
        assert_eq!(tree.search(&4).unwrap(), None);
        assert_eq!(tree.search(&6).unwrap(), None);
        assert_eq!(tree.search(&8).unwrap(), None);

        // Check that remaining entries are still there
        assert_eq!(tree.search(&1).unwrap(), Some(10));
        assert_eq!(tree.search(&3).unwrap(), Some(30));
        assert_eq!(tree.search(&5).unwrap(), Some(50));
        assert_eq!(tree.search(&7).unwrap(), Some(70));
        assert_eq!(tree.search(&9).unwrap(), Some(90));
        assert_eq!(tree.search(&10).unwrap(), Some(100));

        // Verify range query
        let results = tree.range_query(&3, &9).unwrap();
        assert_eq!(results.len(), 4); // 3,5,7,9
        assert!(results.contains(&30));
        assert!(results.contains(&50));
        assert!(results.contains(&70));
        assert!(results.contains(&90));

        println!("complex operations passed");
    }
}
