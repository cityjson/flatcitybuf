# Static B+Tree Implementation Guide for FlatCityBuf

This guide outlines the implementation approach for the Static B+Tree index structure in our FlatCityBuf project. This immutable, balanced tree structure will enable efficient attribute-based queries with minimal I/O operations, making it ideal for cloud-based 3D city model data retrieval.

## Prerequisites

We already have implemented:

- `Key` structure for different attribute types
- `Entry` structure representing key-value pairs in leaf nodes

## Core Implementation Requirements

### 1. Static B+Tree Structure

The Static B+Tree is an immutable B+Tree where:

- All nodes have a fixed size
- Node positions are implicitly calculated rather than using explicit pointers
- The entire structure is stored as a contiguous array of nodes
- Nodes are arranged in level order (root, then level 1, level 2, etc.)

#### Node Structure

Implement two types of nodes:

```rust
struct InternalNode {
    // Number of keys actually stored
    count: u16,

    // Fixed-size array of keys and child node references
    entries: [Entry; MAX_KEYS_PER_NODE],
}

struct LeafNode {
    // Number of keys actually stored
    count: u16,

    // Fixed-size array of key-value entries
    entries: [Entry; MAX_KEYS_PER_NODE],

    // Offset to the next leaf node (for range queries)
    next_leaf: Option<u32>,
}
```

### 2. Streaming Implementation

**Critical requirement**: The tree must be accessed in a streaming fashion:

- Read nodes only when necessary during traversal
- Don't load the entire tree into memory
- Implement a node cache to avoid re-reading frequently accessed nodes

```rust
struct BTreeReader<R: Read + Seek> {
    reader: R,
    tree_offset: u64,
    node_size: usize,
    height: u8,
    branching_factor: u16,
    node_cache: LruCache<usize, Vec<u8>>,
}

impl<R: Read + Seek> BTreeReader<R> {
    fn read_node(&mut self, node_index: usize) -> Result<Vec<u8>> {
        // Check if node is in cache first
        if let Some(cached_node) = self.node_cache.get(&node_index) {
            return Ok(cached_node.clone());
        }

        // Calculate node offset
        let offset = self.tree_offset + (node_index * self.node_size) as u64;

        // Seek and read the node
        self.reader.seek(SeekFrom::Start(offset))?;
        let mut node_data = vec![0u8; self.node_size];
        self.reader.read_exact(&mut node_data)?;

        // Cache the node
        self.node_cache.put(node_index, node_data.clone());

        Ok(node_data)
    }
}
```

### 3. Prefetching Strategy

Implement prefetching to improve performance:

- When reading a node, prefetch its likely child nodes
- For leaf nodes, prefetch the next leaf node for efficient range queries
- Use asynchronous I/O when possible

```rust
fn prefetch_node(&mut self, node_index: usize) {
    // Don't block the main thread - use async or a thread pool
    let reader = self.reader.clone();
    let offset = self.tree_offset + (node_index * self.node_size) as u64;
    let node_size = self.node_size;
    let cache = self.node_cache.clone();

    thread_pool.spawn(move || {
        let mut node_data = vec![0u8; node_size];
        if reader.seek(SeekFrom::Start(offset)).is_ok() &&
           reader.read_exact(&mut node_data).is_ok() {
            cache.put(node_index, node_data);
        }
    });
}
```

### 4. Handling Duplicate Keys

The B+Tree must properly handle duplicate keys:

- When an exact match is found, check adjacent entries for the same key
- For range queries, include all entries with matching keys at range boundaries
- Ensure all queries return arrays of entries, not single values

```rust
fn find_exact(&mut self, key: &Key) -> Result<Vec<Entry>> {
    let mut node_index = 0; // Start at root

    // Navigate down the tree
    while !self.is_leaf(node_index) {
        let node_data = self.read_node(node_index)?;
        let child = self.search_node(&node_data, key);
        node_index = self.child_index(node_index, child);

        // Prefetch the next likely node
        self.prefetch_node(node_index);
    }

    // We're at a leaf node
    let leaf_data = self.read_node(node_index)?;
    let entries = self.extract_matching_entries(&leaf_data, key);

    // If we found entries and there might be more in the next leaf
    if !entries.is_empty() && self.get_next_leaf(&leaf_data).is_some() {
        self.prefetch_node(self.get_next_leaf(&leaf_data).unwrap());
    }

    // Check next leaf nodes for more matches
    let mut result = entries;
    let mut current_leaf = self.get_next_leaf(&leaf_data);

    while let Some(next_leaf_index) = current_leaf {
        let next_leaf_data = self.read_node(next_leaf_index)?;
        let more_entries = self.extract_matching_entries(&next_leaf_data, key);

        if more_entries.is_empty() {
            break;
        }

        result.extend(more_entries);
        current_leaf = self.get_next_leaf(&next_leaf_data);

        // Prefetch next leaf
        if let Some(leaf_index) = current_leaf {
            self.prefetch_node(leaf_index);
        }
    }

    Ok(result)
}
```

### 5. HTTP Streaming Support

The implementation must eventually support HTTP streaming:

- Replace file I/O with HTTP range requests
- Optimize request batching to minimize HTTP overhead
- Implement a connection pool and keepalive
- Handle network errors gracefully with retries

```rust
struct HttpBTreeReader {
    base_url: String,
    tree_offset: u64,
    node_size: usize,
    height: u8,
    branching_factor: u16,
    node_cache: LruCache<usize, Vec<u8>>,
    http_client: Client,
}

impl HttpBTreeReader {
    fn read_node(&mut self, node_index: usize) -> Result<Vec<u8>> {
        // Check cache first
        if let Some(cached_node) = self.node_cache.get(&node_index) {
            return Ok(cached_node.clone());
        }

        // Calculate byte range
        let start = self.tree_offset + (node_index * self.node_size) as u64;
        let end = start + self.node_size as u64 - 1;
        let range_header = format!("bytes={}-{}", start, end);

        // Make HTTP request
        let response = self.http_client
            .get(&self.base_url)
            .header("Range", range_header)
            .send()?;

        if !response.status().is_success() {
            return Err(Error::HttpError(response.status().to_string()));
        }

        let node_data = response.bytes()?.to_vec();

        // Cache the result
        self.node_cache.put(node_index, node_data.clone());

        Ok(node_data)
    }
}
```

## Implementation Steps

1. **Create the B+Tree structure**:
   - Implement node serialization and deserialization
   - Define implicit node addressing functions
   - Create the tree builder for construction from sorted entries

2. **Implement the streaming reader**:
   - Create a node cache with LRU replacement policy
   - Implement read_node with prefetching
   - Add methods for tree traversal (find child, is_leaf, etc.)

3. **Build query operations**:
   - Exact match with duplicate key handling
   - Range queries (>, >=, <, <=)
   - Not-equal queries (!=)
   - Handle special cases (null values, empty ranges)

4. **Add HTTP support**:
   - Replace file I/O with HTTP range requests
   - Implement connection pooling and request batching
   - Add retry logic for network errors
   - Optimize request size for performance

## Construction Algorithm

The construction of the static B+Tree happens once, during the generation of the FlatCityBuf file:

```rust
fn build_static_btree<K: Key, V: Value>(
    entries: Vec<Entry<K, V>>,
    branching_factor: usize
) -> Vec<u8> {
    // 1. Sort entries by key
    let mut sorted_entries = entries;
    sorted_entries.sort_by(|a, b| a.key.cmp(&b.key));

    // 2. Calculate tree dimensions
    let leaf_count = (sorted_entries.len() + MAX_KEYS_PER_LEAF - 1) / MAX_KEYS_PER_LEAF;
    let height = calculate_height(leaf_count, branching_factor);
    let node_count = calculate_node_count(height, branching_factor);

    // 3. Allocate space for all nodes
    let mut tree_data = Vec::with_capacity(node_count * NODE_SIZE);

    // 4. Fill leaf nodes
    distribute_entries_to_leaves(&sorted_entries, &mut tree_data);

    // 5. Build internal nodes bottom-up
    for level in (0..height-1).rev() {
        build_internal_level(level, &mut tree_data, branching_factor);
    }

    // 6. Add tree header with metadata
    let header = BTreeHeader {
        height,
        branching_factor: branching_factor as u16,
        node_size: NODE_SIZE as u16,
        entry_count: sorted_entries.len() as u32,
    };

    let mut result = serialize_header(&header);
    result.extend(tree_data);

    result
}
```

## Search Performance Considerations

1. **Cache Efficiency**:
   - Choose node size to align with common disk block sizes (4KB or 8KB)
   - Keep the most frequently accessed nodes (upper levels) in cache

2. **I/O Optimization**:
   - Minimize the number of reads by prefetching
   - Batch multiple node reads when possible
   - Maintain a sensible cache size based on available memory

3. **HTTP Optimization**:
   - Use HTTP/2 when possible for parallel requests
   - Implement connection pooling and keepalive
   - Consider larger node sizes for HTTP to reduce the number of requests

## Testing Strategy

1. **Unit Tests**:
   - Test node serialization/deserialization
   - Verify tree construction with various data sizes
   - Test duplicate key handling

2. **Integration Tests**:
   - Test all query types with real data
   - Verify HTTP streaming works correctly
   - Test with very large datasets

3. **Performance Tests**:
   - Benchmark query performance with different node sizes
   - Measure impact of prefetching
   - Compare with other index structures

## Conclusion

This implementation approach balances performance with practical considerations for cloud-based operation. By streaming nodes on demand, prefetching strategically, and handling duplicate keys correctly, we can achieve efficient attribute-based querying for the FlatCityBuf format while minimizing both memory usage and network I/O.
