use crate::entry::Entry;
use crate::errors::{BTreeError, Result};
use crate::key::KeyEncoder;
use crate::node::{Node, NodeType};
use crate::query::BTreeIndex;
use crate::storage::BlockStorage;
use std::cmp::Ordering;
use std::marker::PhantomData;

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
}

impl<K, S: BlockStorage> BTree<K, S> {
    /// Create a new B-tree with the given storage and key encoder
    pub fn new(storage: S, key_encoder: Box<dyn KeyEncoder<K>>) -> Self {
        // Initialize a new, empty B-tree
        unimplemented!()
    }

    /// Open an existing B-tree from storage
    pub fn open(storage: S, key_encoder: Box<dyn KeyEncoder<K>>, root_offset: u64) -> Self {
        // Open and initialize an existing B-tree
        Self {
            root_offset,
            storage,
            key_encoder,
            _phantom: PhantomData,
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
        let mut current_offset = self.find_leaf_containing(&encoded_start)?;

        // Scan leaf nodes until we find the end key or run out of leaves
        loop {
            // Read current leaf node
            let node_data = self.storage.read_block(current_offset)?;
            let node = Node::decode(&node_data, self.key_encoder.encoded_size())?;

            // Verify node is a leaf
            if node.node_type != NodeType::Leaf {
                return Err(BTreeError::InvalidNodeType {
                    expected: "Leaf",
                    actual: format!("{:?}", node.node_type),
                });
            }

            // Process entries in this leaf
            for entry in &node.entries {
                match self.key_encoder.compare(&entry.key, &encoded_end) {
                    // If entry key > end key, we're done
                    Ordering::Greater => return Ok(results),

                    // If entry key >= start key, include it in results
                    _ if self.key_encoder.compare(&entry.key, &encoded_start) != Ordering::Less => {
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
        let mut current_offset = self.root_offset;

        loop {
            let node_data = self.storage.read_block(current_offset)?;
            let node = Node::decode(&node_data, self.key_encoder.encoded_size())?;

            match node.node_type {
                NodeType::Internal => {
                    current_offset = self.find_child_node(&node, key)?.ok_or_else(|| {
                        BTreeError::InvalidStructure("Unable to find child node".to_string())
                    })?;
                }
                NodeType::Leaf => {
                    return Ok(current_offset);
                }
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
                    expected: "Leaf",
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
    /// Create a new B-tree builder
    pub fn new(storage: S, key_encoder: Box<dyn KeyEncoder<K>>) -> Self {
        let key_size = key_encoder.encoded_size();
        let node_size = storage.block_size();

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

    /// Add an entry to the B-tree
    pub fn add_entry(&mut self, key: K, value: u64) -> Result<()> {
        // Calculate max entries per node
        let header_size = 12; // node_type(1) + entry_count(2) + next_node(8) + reserved(1)
        let entry_size = self.key_size + 8; // key size + value size (u64)
        let max_entries_per_node = (self.node_size - header_size) / entry_size;

        // Encode key
        let encoded_key = self.key_encoder.encode(&key)?;

        // Create entry
        let entry = Entry::new(encoded_key, value);

        // Add to current leaf node
        self.current_leaf.add_entry(entry);

        // If leaf node is full, write it to storage and create a new one
        if self.current_leaf.entries.len() >= max_entries_per_node {
            self.flush_current_leaf()?;
        }

        Ok(())
    }

    /// Flush the current leaf node to storage
    fn flush_current_leaf(&mut self) -> Result<()> {
        // Allocate a block for the node
        let offset = self.storage.allocate_block()?;

        // Link to previous leaf if exists
        if !self.leaf_nodes.is_empty() {
            // Set the next_node pointer on the previous leaf
            // to point to this new leaf
            // Implement this when required
        }

        // Encode and write node
        let encoded_node = self.current_leaf.encode(self.node_size, self.key_size)?;
        self.storage.write_block(offset, &encoded_node)?;

        // Add to leaf nodes
        self.leaf_nodes.push(offset);

        // Create a new leaf
        self.current_leaf = Node::new_leaf();

        Ok(())
    }

    /// Finalize the B-tree construction and return the resulting tree
    pub fn finalize(mut self) -> Result<BTree<K, S>> {
        // Flush any remaining entries in the current leaf
        if !self.current_leaf.entries.is_empty() {
            self.flush_current_leaf()?;
        }

        // Build internal nodes from leaf nodes, then from those internal nodes, etc.
        // until we have a single root node
        let mut current_level = self.leaf_nodes;

        // Calculate max entries per internal node
        let header_size = 12; // node_type(1) + entry_count(2) + next_node(8) + reserved(1)
        let entry_size = self.key_size + 8; // key size + value size (u64)
        let max_entries_per_node = (self.node_size - header_size) / entry_size;

        while current_level.len() > 1 {
            let mut next_level = Vec::new();
            let mut current_node = Node::new_internal();

            // First entry of each child node becomes the key for internal nodes
            for &child_offset in &current_level {
                // Read child node
                let child_data = self.storage.read_block(child_offset)?;
                let child_node = Node::decode(&child_data, self.key_size)?;

                // Use first key of child as separator key
                if !child_node.entries.is_empty() {
                    let separator_key = child_node.entries[0].key.clone();

                    // Add entry to current internal node
                    let entry = Entry::new(separator_key, child_offset);
                    current_node.add_entry(entry);

                    // If node is full, write it and start a new one
                    if current_node.entries.len() >= max_entries_per_node {
                        let node_offset = self.storage.allocate_block()?;
                        let encoded_node = current_node.encode(self.node_size, self.key_size)?;
                        self.storage.write_block(node_offset, &encoded_node)?;

                        next_level.push(node_offset);
                        current_node = Node::new_internal();
                    }
                }
            }

            // Write last node if it has entries
            if !current_node.entries.is_empty() {
                let node_offset = self.storage.allocate_block()?;
                let encoded_node = current_node.encode(self.node_size, self.key_size)?;
                self.storage.write_block(node_offset, &encoded_node)?;

                next_level.push(node_offset);
            }

            // Update for next iteration
            current_level = next_level;
        }

        // Root offset is the only node in the last level
        let root_offset = if current_level.is_empty() {
            // Special case: empty tree
            // Create an empty root node
            let root = Node::new_leaf();
            let node_offset = self.storage.allocate_block()?;
            let encoded_node = root.encode(self.node_size, self.key_size)?;
            self.storage.write_block(node_offset, &encoded_node)?;

            node_offset
        } else {
            // Use the single node in the last level as root
            current_level[0]
        };

        // Flush storage
        self.storage.flush()?;

        // Create and return the B-tree
        Ok(BTree {
            root_offset,
            storage: self.storage,
            key_encoder: self.key_encoder,
            _phantom: PhantomData,
        })
    }
}
