# Implicit B+Tree (S+Tree) Implementation Strategy

This document outlines the implementation strategy for our static B+tree, based on the S+tree concept described in the [Algorithmica article](https://en.algorithmica.org/hpc/data-structures/s-tree/).

## Core Concepts

### Traditional B-trees vs Static B+trees

Traditional B-trees use a node structure with:

- Keys
- Child pointers
- Empty space for future insertions
- Typically 50-70% space utilization

Our static B+tree (S+tree) implementation differs in several key ways:

- No explicit pointers between nodes
- Complete filling of nodes (except possibly the last node at each level)
- Implicit relationship between parent and child nodes based on array indices
- Nearly 100% space utilization

### Implicit Node Numbering

In a traditional B-tree with pointers, each node contains explicit references to its children. In our implicit layout:

1. We number nodes using a generalized Eytzinger layout:
   - The root node is numbered 0
   - For a node with index k and branching factor B, its B child nodes are numbered `{k * (B + 1) + i + 1}` for i ∈ [0, B]

2. This numbering allows us to navigate the tree using simple arithmetic:

   ```rust
   fn child_index(node_index: usize, branch: usize, branch_factor: usize) -> usize {
       node_index * (branch_factor + 1) + branch + 1
   }
   ```

### Memory Layout

The entire tree is stored as a single contiguous array of nodes, with:

1. Nodes arranged by level, with the root node first
2. Each node containing exactly B keys (except possibly the last node at each level)
3. No explicit pointers between nodes
4. Keys within each node sorted in ascending order

This layout offers several advantages:

- Better cache locality
- No pointer indirection
- More efficient memory usage
- Simplified serialization/deserialization

## Node Structure

Each node in our static B+tree consists of:

1. A fixed number of keys (determined by the branching factor)
2. No explicit pointers to child nodes (these are computed)

The physical structure of a node is optimized for cache line efficiency:

- For a branching factor of 16, each node contains 16 fixed-width keys
- Total node size is typically 64 bytes (standard cache line size)
- Keys are stored in a format that facilitates SIMD comparisons

## Search Algorithm

The search algorithm leverages the implicit structure:

1. Start at the root node (index 0)
2. Within each node, find the appropriate branch using:
   - Binary search for basic implementation
   - SIMD-accelerated search for optimized implementation
3. Compute the child node index using the formula
4. Continue until reaching a leaf node
5. In the leaf node, find the exact match for the key

The SIMD-accelerated version uses:

- Vector instructions to compare multiple keys simultaneously
- Optimized branching based on bitmasks
- Techniques to minimize branch mispredictions

### SIMD Search Details

For a 16-key node (which fits in a 64-byte cache line):

1. Load 16 keys into SIMD registers
2. Compare all keys with the search key in parallel
3. Generate a bitmask of comparison results
4. Use bit manipulation to find the correct branch

```rust
// Pseudo-code for SIMD search
fn simd_search_node(node: &Node, key: &[u8]) -> usize {
    // Load keys into SIMD registers
    let keys_vector = load_keys_simd(node);

    // Compare with search key
    let compare_mask = compare_simd(keys_vector, key);

    // Find position of first key not less than search key
    find_first_set_bit(compare_mask)
}
```

## Construction Algorithm

Building the static B+tree involves:

1. Sorting all entries (if not already sorted)
2. Computing the number of nodes needed for each level
3. Allocating a single contiguous array for all nodes
4. Filling nodes in a bottom-up fashion:
   - Fill leaf nodes first
   - Then fill internal nodes with keys that represent the smallest key in each child subtree

```rust
// Pseudo-code for tree construction
fn build_tree(entries: &[Entry], branch_factor: usize) -> StaticBTree {
    // Calculate tree dimensions
    let total_entries = entries.len();
    let height = calculate_height(total_entries, branch_factor);

    // Allocate space for all nodes
    let mut nodes = allocate_nodes(total_entries, branch_factor);

    // Fill the tree bottom-up
    fill_leaf_level(&mut nodes, entries, branch_factor);
    for level in (0..height-1).rev() {
        fill_internal_level(&mut nodes, level, branch_factor);
    }

    StaticBTree { nodes, branch_factor, height }
}
```

## Range Queries

Unlike traditional B+trees which use a linked list of leaf nodes, our implementation:

1. Finds the leaf containing the start key
2. Uses the implicit layout to find neighboring leaves
3. Scans contiguous leaf nodes until reaching the end key

This approach maintains the cache efficiency advantages while supporting efficient range queries.

## Duplicate Key Handling

Our implementation properly handles duplicate keys by:

1. Preserving all entries with the same key (no deduplication)
2. Returning all matching values for exact match queries
3. Checking adjacent nodes for duplicates at node boundaries
4. Ensuring range queries include all duplicate keys

This is particularly important for CityJSON data where multiple features can share the same attribute value.

## Storage Integration

The static B+tree is designed to work with different storage backends through a unified interface:

### BTreeStorage Trait

```rust
pub trait BTreeStorage {
    fn read_node(&self, node_index: usize) -> Result<Vec<u8>>;
    fn write_node(&mut self, node_index: usize, data: &[u8]) -> Result<()>;
    fn node_size(&self) -> usize;
    fn node_count(&self) -> usize;
    fn flush(&mut self) -> Result<()>;
}
```

### In-Memory Storage

The in-memory storage implementation is optimized for performance:

```rust
pub struct MemoryStorage {
    nodes: Vec<Vec<u8>>,
    node_size: usize,
}
```

Key features:

- Direct access to node data
- No serialization overhead
- Limited by available RAM
- Suitable for small to medium datasets

### File-Based Storage

TODO: Implement file-based storage

Key features:

- Support for trees larger than available RAM
- Persistence across program runs
- Optional read-only mode for shared access

### HTTP Storage

For remote storage of large trees:

```rust
pub struct HttpStorage {
    base_url: String,
    client: HttpClient,
    cache: LruCache<usize, Vec<u8>>,
    node_size: usize,
    node_count: usize,
}
```

Key features:

- HTTP range requests to fetch only needed nodes
- Local caching to minimize network traffic
- Support for authentication and compression
- Suitable for distributed access patterns

### Node Size Considerations

Unlike the standard B-tree implementation which uses a fixed 4KB block size, the static B+tree's node size is determined by:

- The branching factor
- The key type's encoded size
- Implementation overhead

Node sizes are calculated to ensure optimal memory usage while maintaining cache efficiency. For typical integer keys with a branching factor of 16, node sizes range from 128-256 bytes.

## Caching System

To optimize storage access, we implement a caching layer:

```rust
pub struct CachedStorage<S: BTreeStorage> {
    storage: S,
    cache: LruCache<usize, Vec<u8>>,
    prefetch_strategy: PrefetchStrategy,
    stats: CacheStats,
}
```

### LRU Cache

The Least Recently Used (LRU) cache maintains frequently accessed nodes in memory:

- Configurable cache size (in nodes or bytes)
- Automatic eviction of least recently used nodes
- Thread-safe implementation for concurrent access

### Prefetching Strategies

Several prefetching strategies are implemented:

1. **Sequential Prefetching**: Loads adjacent nodes for range queries

   ```rust
   fn prefetch_sequential(&self, current_node: usize, count: usize) -> Result<()>;
   ```

2. **Hierarchical Prefetching**: Prefetches nodes across the hierarchy

   ```rust
   fn prefetch_hierarchical(&self, node_index: usize, depth: usize) -> Result<()>;
   ```

3. **Predictive Prefetching**: Uses access patterns to predict future needs

   ```rust
   fn prefetch_predictive(&self, access_history: &[usize]) -> Result<()>;
   ```

### Monitoring and Adaptation

The caching system includes monitoring capabilities:

```rust
pub struct CacheStats {
    hits: AtomicUsize,
    misses: AtomicUsize,
    prefetch_hits: AtomicUsize,
    bytes_transferred: AtomicUsize,
}
```

These statistics enable adaptive strategies that optimize prefetching based on observed access patterns.

## Performance Considerations

Our implementation focuses on maximizing performance through:

1. **Cache Efficiency**:
   - Aligning node size with cache lines
   - Minimizing node access during tree traversal
   - Using prefetching to hide I/O latency

2. **Reduced Memory Usage**:
   - No explicit pointers
   - Nearly 100% space utilization
   - Compact node representation

3. **Concurrency**:
   - Thread-safe storage implementations
   - Lock-free read operations for multiple readers
   - Optional concurrent caching

4. **Adaptation to Access Patterns**:
   - Monitoring of cache performance
   - Adjustment of prefetch strategies
   - Optimization for specific workloads

## Integration with CityJSON

The static B+tree is particularly well-suited for CityJSON use cases:

1. **Attribute Indexing**:
   - Create indexes on commonly queried attributes
   - Support fast filtering of features based on properties
   - Handle duplicate attribute values properly

2. **Spatial Indexing**:
   - Combined with R-tree for spatial queries
   - Two-phase filtering for complex queries

3. **Remote Access**:
   - HTTP-based access to large datasets
   - Minimal data transfer for queries
   - Efficient caching for repeated access patterns

## Comparison with Existing Implementation

Compared to the current btree crate, our static-btree implementation:

1. Offers significantly faster search (up to 15x for large datasets)
2. Uses less memory due to elimination of pointers and better space utilization
3. Has better cache efficiency due to optimized memory layout
4. Supports multiple storage backends with the same interface
5. Properly handles duplicate keys for CityJSON use cases
6. Does not support modifications after construction

These tradeoffs make it ideal for read-heavy workloads with static data, such as CityJSON databases.

## Future Directions

Beyond the current implementation plan, future enhancements might include:

1. **SIMD-Accelerated Search**:
   - Using AVX2/AVX-512 for parallel key comparison
   - Optimizing for specific key types

2. **Hybrid Storage**:
   - Combining multiple backends for different parts of the tree
   - Caching most frequently accessed nodes in memory

3. **Distributed Operation**:
   - Sharding large trees across multiple servers
   - Distributed query processing

4. **Versioned Trees**:
   - Immutable versions for historical data
   - Copy-on-write mechanisms for updates
