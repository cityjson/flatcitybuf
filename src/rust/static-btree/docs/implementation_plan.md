# Static B+Tree Implementation Guide for FlatCityBuf

This guide outlines the implementation approach for the Static B+Tree index structure in our FlatCityBuf project. This immutable, balanced tree structure will enable efficient attribute-based queries with minimal I/O operations, making it ideal for cloud-based 3D city model data retrieval.

For detailed explanations of implicit B-tree concepts, refer to:

- [Algorithmica: Implicit B-tree](https://en.algorithmica.org/hpc/data-structures/s-tree/#implicit-b-tree-1)
- [Curious Coding: Static Search Tree](https://curiouscoding.nl/posts/static-search-tree/#s-trees-and-b-trees)

## 1. Overview

The Static B+Tree (also known as an Implicit B+Tree) provides efficient attribute indexing with optimized performance for cloud-based access patterns. This implementation will be immutable after construction, have a fixed structure, and optimize for read-only access patterns with efficient HTTP Range Request support.

Key differences from a traditional B+Tree:

- Immutable after construction
- Fixed, predetermined structure
- Optimized for read-only access patterns
- Efficient access via HTTP Range Requests

## 2. Core Components

### 2.1 Node Structure

We'll implement two types of nodes:

#### 2.1.1 Internal Node

```rust
pub struct InternalNode<K: Key> {
    /// Keys for guiding the search
    pub keys: Vec<K>,
    /// Offsets to child nodes
    pub child_offsets: Vec<u64>,
}
```

Memory layout:

```
[count: u16][key1][ptr1][key2][ptr2]...[keyN][ptrN]
```

#### 2.1.2 Leaf Node

```rust
pub struct LeafNode<K: Key> {
    /// Entries (key-value pairs)
    pub entries: Vec<Entry<K>>,
    /// Pointer to the next leaf node (for range queries)
    pub next_leaf_offset: Option<u64>,
}
```

Memory layout:

```
[count: u16][next_leaf_ptr: u64][entry1][entry2]...[entryN]
```

### 2.2 Tree Structure

```rust
pub struct StaticBTree<K: Key> {
    /// Offset to the root node
    root_offset: u64,
    /// Height of the tree
    height: u8,
    /// Node size in bytes (power of 2)
    node_size: u16,
    /// Total number of entries
    num_entries: u64,
    /// Phantom data for key type
    _phantom: PhantomData<K>,
}
```

### 2.3 Builder

```rust
pub struct StaticBTreeBuilder<K: Key> {
    /// Entries to be inserted into the tree
    entries: Vec<Entry<K>>,
    /// Node size in bytes (power of 2)
    node_size: u16,
    /// Branching factor (default: 16)
    branching_factor: u16,
}
```

### 2.4 Node Reader Interface

```rust
pub trait NodeReader<K: Key> {
    /// Read a node from storage
    fn read_node(&self, offset: u64) -> Result<Box<dyn Node<K>>>;

    /// Prefetch a node (optional optimization)
    fn prefetch_node(&self, offset: u64);
}
```

## 3. Node Size Calculation

We'll use a dynamic approach to calculate the optimal node size based on key size, branching factor, and offset byte size:

```rust
fn calculate_optimal_node_size<K: Key>(branching_factor: u16) -> u16 {
    let key_size = K::SERIALIZED_SIZE;
    let entry_size = Entry::<K>::SERIALIZED_SIZE;

    // Calculate sizes for leaf and internal nodes
    let leaf_header_size = 10; // count(2) + next_ptr(8)
    let leaf_node_size = leaf_header_size + (branching_factor as usize * entry_size);

    let internal_header_size = 2; // count(2)
    let internal_entry_size = key_size + 8; // key + child ptr
    let internal_node_size = internal_header_size + (branching_factor as usize * internal_entry_size);

    // Use the larger of the two sizes
    let node_size = leaf_node_size.max(internal_node_size);

    // Round up to the next power of 2
    let mut power_of_2 = 1;
    while power_of_2 < node_size {
        power_of_2 *= 2;
    }

    power_of_2 as u16
}
```

This approach ensures:

- Nodes are sized appropriately for the key type
- Node size is a power of 2 for alignment with disk and memory pages
- Both leaf and internal nodes fit within the same size

## 4. Tree Construction (Bottom-Up)

We'll use a bottom-up approach to construct the tree:

1. Sort all entries by key
2. Create leaf nodes by grouping sorted entries
3. Build internal nodes recursively upward
4. Ensure all nodes except possibly the rightmost at each level are full

```rust
impl<K: Key> StaticBTreeBuilder<K> {
    pub fn build(&self) -> Result<StaticBTree<K>> {
        // 1. Sort entries
        let mut sorted_entries = self.entries.clone();
        sorted_entries.sort_by(|a, b| a.key.cmp(&b.key));

        // 2. Calculate node parameters
        let node_size = self.node_size;
        let entries_per_leaf = self.calculate_entries_per_leaf();

        // 3. Create leaf nodes
        let leaf_nodes = self.create_leaf_nodes(&sorted_entries, entries_per_leaf);

        // 4. Build internal nodes bottom-up
        let (root_offset, height) = self.build_internal_levels(leaf_nodes);

        // 5. Create the tree
        Ok(StaticBTree {
            root_offset,
            height,
            node_size,
            num_entries: sorted_entries.len() as u64,
            _phantom: PhantomData,
        })
    }

    fn create_leaf_nodes(&self, entries: &[Entry<K>], entries_per_leaf: usize) -> Vec<LeafNode<K>> {
        let mut leaf_nodes = Vec::new();
        let mut i = 0;

        while i < entries.len() {
            let mut leaf = LeafNode::new();
            let end = (i + entries_per_leaf).min(entries.len());

            for j in i..end {
                leaf.entries.push(entries[j].clone());
            }

            leaf_nodes.push(leaf);
            i = end;
        }

        // Link leaf nodes together
        for i in 0..leaf_nodes.len() - 1 {
            leaf_nodes[i].next_leaf_offset = Some((i + 1) as u64);
        }

        leaf_nodes
    }

    fn build_internal_levels(&self, leaf_nodes: Vec<LeafNode<K>>) -> (u64, u8) {
        // Implementation details for building internal nodes
        // ...
    }
}
```

## 5. Handling Duplicate Keys

Our implementation will support duplicate keys with the following approach:

### 5.1 Storage

- Duplicate keys will be stored sequentially in leaf nodes
- The original order of duplicate keys will be preserved during construction

### 5.2 Exact Match Queries

When performing an exact match query, we'll return all entries with matching keys:

```rust
fn find_all_matches(&self, node: &LeafNode<K>, key: &K) -> Vec<u64> {
    let mut results = Vec::new();

    // Binary search to find any match
    match node.entries.binary_search_by(|e| e.key.cmp(key)) {
        Ok(idx) => {
            // Found a match, now collect all duplicates

            // Scan backward
            let mut i = idx;
            while i > 0 && node.entries[i-1].key == *key {
                i -= 1;
            }

            // Scan forward and collect all matches
            while i < node.entries.len() && node.entries[i].key == *key {
                results.push(node.entries[i].offset);
                i += 1;
            }
        },
        Err(_) => {} // No match found
    }

    results
}
```

### 5.3 Range Queries

For range queries, we'll ensure all duplicates at range boundaries are properly included:

```rust
fn collect_entries_in_range(
    &self,
    node: &LeafNode<K>,
    start: &K,
    end: &K,
    include_start: bool,
    include_end: bool
) -> Vec<u64> {
    let mut results = Vec::new();

    for entry in &node.entries {
        let in_range = match (include_start, include_end) {
            (true, true) => entry.key >= *start && entry.key <= *end,
            (true, false) => entry.key >= *start && entry.key < *end,
            (false, true) => entry.key > *start && entry.key <= *end,
            (false, false) => entry.key > *start && entry.key < *end,
        };

        if in_range {
            results.push(entry.offset);
        }
    }

    results
}
```

## 6. Query Operations

### 6.1 Exact Match Query

```rust
impl<K: Key> StaticBTree<K> {
    pub fn find(&self, key: &K, reader: &dyn NodeReader<K>) -> Result<Vec<u64>> {
        // Start at root node
        let mut node_offset = self.root_offset;

        // Traverse the tree
        for _ in 0..self.height - 1 {
            let node = reader.read_node(node_offset)?;
            if let Some(internal) = node.as_internal() {
                // Find child node to follow
                let idx = internal.binary_search(key);
                node_offset = internal.child_offsets[idx];
            } else {
                return Err(Error::InvalidNodeType);
            }
        }

        // Search in leaf node
        let node = reader.read_node(node_offset)?;
        if let Some(leaf) = node.as_leaf() {
            // Find all entries with matching key
            Ok(self.find_all_matches(leaf, key))
        } else {
            Err(Error::InvalidNodeType)
        }
    }
}
```

### 6.2 Range Query

```rust
impl<K: Key> StaticBTree<K> {
    pub fn range(
        &self,
        start: &K,
        end: &K,
        include_start: bool,
        include_end: bool,
        reader: &dyn NodeReader<K>
    ) -> Result<Vec<u64>> {
        // Find leaf containing start key
        let leaf_offset = self.find_leaf_containing(start, reader)?;
        let mut results = Vec::new();

        // Scan leaf nodes
        let mut current_offset = Some(leaf_offset);
        while let Some(offset) = current_offset {
            let node = reader.read_node(offset)?;
            if let Some(leaf) = node.as_leaf() {
                // Add entries in range
                let mut entries = self.collect_entries_in_range(
                    leaf, start, end, include_start, include_end
                );
                results.append(&mut entries);

                // Stop if we've passed the end
                if leaf.entries.last().map_or(false, |e| e.key > *end) {
                    break;
                }

                // Move to next leaf
                current_offset = leaf.next_leaf_offset;
            } else {
                return Err(Error::InvalidNodeType);
            }
        }

        Ok(results)
    }
}
```

## 7. Streaming Implementation

### 7.1 File-Based Reader

```rust
pub struct FileNodeReader<K: Key> {
    file: File,
    cache: LruCache<u64, Box<dyn Node<K>>>,
    node_size: u16,
}

impl<K: Key> NodeReader<K> for FileNodeReader<K> {
    fn read_node(&self, offset: u64) -> Result<Box<dyn Node<K>>> {
        // Check cache first
        if let Some(node) = self.cache.get(&offset) {
            return Ok(node.clone());
        }

        // Read from file
        self.file.seek(SeekFrom::Start(offset))?;
        let mut buffer = vec![0u8; self.node_size as usize];
        self.file.read_exact(&mut buffer)?;

        // Parse node
        let node = Node::read_from(&mut Cursor::new(buffer))?;

        // Cache the node
        self.cache.insert(offset, node.clone());

        Ok(node)
    }

    fn prefetch_node(&self, offset: u64) {
        // Simple implementation - just read the node into cache
        let _ = self.read_node(offset);
    }
}
```

### 7.2 HTTP-Based Reader

```rust
pub struct HttpNodeReader<K: Key> {
    client: HttpClient,
    cache: LruCache<u64, Box<dyn Node<K>>>,
    base_url: String,
    node_size: u16,
}

impl<K: Key> NodeReader<K> for HttpNodeReader<K> {
    fn read_node(&self, offset: u64) -> Result<Box<dyn Node<K>>> {
        // Check cache first
        if let Some(node) = self.cache.get(&offset) {
            return Ok(node.clone());
        }

        // Calculate range
        let end = offset + self.node_size as u64 - 1;
        let range = format!("bytes={}-{}", offset, end);

        // Make HTTP request
        let response = self.client.get(&self.base_url)
            .header("Range", range)
            .send()?;

        // Parse node
        let node = Node::read_from(&mut Cursor::new(response.bytes()))?;

        // Cache the node
        self.cache.insert(offset, node.clone());

        Ok(node)
    }

    fn prefetch_node(&self, offset: u64) {
        // Simple implementation - just read the node into cache
        let _ = self.read_node(offset);
    }
}
```

## 8. Performance Optimizations

### 8.1 Binary Search Optimization

```rust
impl<K: Key> InternalNode<K> {
    fn binary_search(&self, key: &K) -> usize {
        let mut low = 0;
        let mut high = self.keys.len();

        while low < high {
            let mid = low + (high - low) / 2;
            match self.keys[mid].cmp(key) {
                Ordering::Less => low = mid + 1,
                _ => high = mid,
            }
        }

        low
    }
}
```

### 8.2 Cache-Friendly Layout

Ensure node layout is optimized for CPU cache lines:

```rust
impl<K: Key> InternalNode<K> {
    fn write_to<W: Write>(&self, writer: &mut W) -> Result<()> {
        // Write count
        writer.write_u16::<LittleEndian>(self.keys.len() as u16)?;

        // Write interleaved keys and pointers for better cache locality
        for i in 0..self.keys.len() {
            self.keys[i].write_to(writer)?;
            writer.write_u64::<LittleEndian>(self.child_offsets[i])?;
        }

        Ok(())
    }
}
```

## 9. Testing Strategy

### 9.1 Unit Tests

1. Test node serialization/deserialization
2. Test binary search in nodes
3. Test tree construction with various node sizes
4. Test exact match queries
5. Test range queries
6. Test with different key types
7. Test handling of duplicate keys

### 9.2 Integration Tests

1. Test with large datasets
2. Test with HTTP range requests
3. Test with various access patterns

### 9.3 Performance Tests

1. Measure query latency
2. Benchmark HTTP performance
3. Compare with dynamic B-tree implementation

## 10. Implementation Timeline

1. **Week 1**: Implement node structures and serialization
2. **Week 2**: Implement tree construction algorithm
3. **Week 3**: Implement query operations
4. **Week 4**: Implement streaming access and HTTP optimizations
5. **Week 5**: Performance optimizations and testing

## 11. Future Enhancements

1. Implement compression for keys and nodes
2. Add support for bulk loading from external sources
3. Implement parallel construction for large datasets
4. Add statistics collection for query optimization
5. Implement more advanced HTTP optimizations:
   - Multi-range requests
   - Progressive loading
   - Access pattern analysis
   - Compression negotiation
   - Connection pooling
