# Static B+Tree (S+Tree) Implementation Policy

## Overview

This document outlines the implementation policy for a Static B+Tree (S+Tree) in Rust, designed specifically for the Cloud-Optimized CityJSON project. The implementation follows the S-tree concept described in the [Algorithmica article](https://en.algorithmica.org/hpc/data-structures/s-tree/), providing an immutable, cache-efficient B+Tree optimized for read-heavy workloads with static data.

## Core Design Principles

1. **Immutability**: Once constructed, the tree cannot be modified
2. **Memory Efficiency**: Nearly 100% space utilization compared to traditional B-trees
3. **Cache Optimization**: Nodes arranged to maximize cache locality
4. **Minimal Memory Usage**: Support for lazy loading to reduce memory footprint
5. **Cloud Optimization**: Designed for efficient HTTP range requests
6. **Type Flexibility**: Support for various key types including integers, floats, strings, and dates

## Core Components

### Key Trait

The `Key` trait defines the interface for types that can be used as keys in the Static B+Tree:

```rust
/// Trait for key types that can be used in the Static B+Tree.
/// Implementations must provide serialization/deserialization and comparison operations.
pub trait Key: Clone + Ord + Send + Sync {
    /// Convert key to bytes for serialization.
    /// This method must produce a consistent byte representation for comparison and storage.
    /// For fixed-size types, the output length should always be the same.
    /// For variable-size types like strings, consider prefix-based serialization.
    fn to_bytes(&self) -> Vec<u8>;

    /// Create key from bytes for deserialization.
    /// This method must correctly reconstruct the key from the bytes produced by to_bytes.
    /// Should handle potential truncation for variable-length types.
    fn from_bytes(bytes: &[u8]) -> Self;

    /// Get the fixed size of the key in bytes, if applicable.
    /// Returns None for variable-sized keys like strings.
    /// This helps optimize node layout and memory allocation.
    fn size_hint() -> Option<usize>;
}
```

### Entry Structure

An Entry represents a key-value pair in the B+Tree:

```rust
/// Represents a key-value pair in the B+Tree.
/// Used during tree construction and as the result of queries.
pub struct Entry<K: Key, V> {
    /// The key, which must implement the Key trait for comparison and serialization
    pub key: K,

    /// The value associated with this key
    /// This is typically an offset or identifier rather than the full value itself
    pub value: V,
}

impl<K: Key, V> Entry<K, V> {
    /// Create a new Entry with the given key and value.
    ///
    /// # Parameters
    /// * `key` - The key for this entry
    /// * `value` - The value associated with the key
    pub fn new(key: K, value: V) -> Self {
        Entry { key, value }
    }

    /// Serialize the entry to bytes.
    /// First serializes the key using its to_bytes method, then the value.
    ///
    /// # Returns
    /// A vector of bytes representing the serialized entry
    pub fn to_bytes(&self) -> Vec<u8>
    where V: Serialize {
        // Implementation will serialize key and value
    }

    /// Deserialize an entry from bytes.
    ///
    /// # Parameters
    /// * `bytes` - The byte slice containing the serialized entry
    ///
    /// # Returns
    /// The deserialized Entry if successful
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, std::io::Error>
    where V: Deserialize {
        // Implementation will deserialize key and value
    }
}
```

### Node Structure

Nodes store keys and implicitly reference child nodes:

```rust
/// Represents a node in the Static B+Tree.
/// Nodes contain keys and implicitly reference child nodes through the tree's layout.
pub struct Node<K: Key> {
    /// Keys stored in this node, sorted in ascending order.
    /// Internal nodes will have B keys, leaf nodes may have fewer.
    keys: Vec<K>,

    /// Indicates whether this is a leaf node (true) or an internal node (false).
    /// Leaf nodes contain actual data entries while internal nodes guide the search.
    is_leaf: bool,
}

impl<K: Key> Node<K> {
    /// Create a new leaf node with the given keys.
    ///
    /// # Parameters
    /// * `keys` - Vector of keys to store in this node, must be pre-sorted
    ///
    /// # Returns
    /// A new leaf Node containing the specified keys
    pub fn new_leaf(keys: Vec<K>) -> Self {
        Node { keys, is_leaf: true }
    }

    /// Create a new internal node with the given keys.
    ///
    /// # Parameters
    /// * `keys` - Vector of keys to store in this node, must be pre-sorted
    ///
    /// # Returns
    /// A new internal Node containing the specified keys
    pub fn new_internal(keys: Vec<K>) -> Self {
        Node { keys, is_leaf: false }
    }

    /// Find the appropriate branch index for the given key in this node.
    /// Uses binary search to find the index where the key should be inserted.
    ///
    /// # Parameters
    /// * `key` - The key to search for
    ///
    /// # Returns
    /// The index of the branch to follow when searching for this key
    pub fn find_branch(&self, key: &K) -> usize {
        // Uses binary search to find the branch index
    }

    /// Serialize the node to bytes.
    /// First writes a flag indicating leaf/internal status,
    /// then writes the number of keys, then each key's bytes.
    ///
    /// # Returns
    /// A vector of bytes representing the serialized node
    pub fn to_bytes(&self) -> Vec<u8> {
        // Implementation will serialize node type and keys
    }

    /// Deserialize a node from bytes.
    ///
    /// # Parameters
    /// * `bytes` - The byte slice containing the serialized node
    ///
    /// # Returns
    /// The deserialized Node if successful
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, std::io::Error> {
        // Implementation will deserialize node type and keys
    }
}
```

### STree Structure

The main Static B+Tree implementation:

```rust
/// The main Static B+Tree implementation.
/// Provides an immutable, cache-efficient B-Tree structure optimized for reads.
pub struct STree<K: Key, V> {
    /// All nodes in the tree, laid out in a single flat array.
    /// Nodes are arranged in a generalized Eytzinger layout for cache efficiency.
    nodes: Vec<Node<K>>,

    /// Values corresponding to the keys, stored in the same order as the keys in leaf nodes.
    /// For a cloud-optimized implementation, these might just be offsets or identifiers.
    values: Vec<V>,

    /// Branching factor of the tree (maximum number of children per node).
    /// This determines how many keys are stored in each node (B-1 for internal nodes).
    branching_factor: usize,

    /// Height of the tree (number of levels including leaf level).
    /// Used for various internal calculations.
    height: usize,
}

impl<K: Key, V> STree<K, V> {
    /// Create a new Static B+Tree from sorted entries.
    ///
    /// # Parameters
    /// * `entries` - Vector of key-value pairs, must be pre-sorted by key
    /// * `branching_factor` - Number of branches per node (typically aligned with cache line size)
    ///
    /// # Returns
    /// A new STree constructed from the provided entries
    ///
    /// # Process
    /// 1. Verify entries are sorted
    /// 2. Calculate tree height based on entry count and branching factor
    /// 3. Allocate space for nodes and values
    /// 4. Build the tree bottom-up, filling leaf nodes first
    /// 5. Fill internal nodes with separator keys
    pub fn new(entries: Vec<Entry<K, V>>, branching_factor: usize) -> Self {
        // Implementation will build the tree from sorted entries
    }

    /// Search for a value by key.
    ///
    /// # Parameters
    /// * `key` - The key to search for
    ///
    /// # Returns
    /// Some(&V) if the key is found, None otherwise
    ///
    /// # Process
    /// 1. Start at the root node (index 0)
    /// 2. Use binary search within the node to find the right branch
    /// 3. Calculate the index of the child node using the implicit formula
    /// 4. Continue until reaching a leaf node
    /// 5. Check if the key exists in the leaf node
    pub fn search(&self, key: &K) -> Option<&V> {
        // Implementation will traverse the tree to find the key
    }

    /// Perform a range query, returning all key-value pairs in the given range.
    ///
    /// # Parameters
    /// * `start_key` - The lower bound of the range (inclusive)
    /// * `end_key` - The upper bound of the range (inclusive)
    ///
    /// # Returns
    /// A vector of references to key-value pairs in the specified range
    ///
    /// # Process
    /// 1. Find the leaf node containing start_key
    /// 2. Collect all qualifying entries from this leaf
    /// 3. Move to the next leaf node (if needed) using the implicit layout
    /// 4. Continue until reaching end_key or the end of the tree
    pub fn range_query(&self, start_key: &K, end_key: &K) -> Vec<(&K, &V)> {
        // Implementation will find all entries in the given range
    }

    /// Serialize the tree to bytes.
    ///
    /// # Returns
    /// A vector of bytes representing the serialized tree
    ///
    /// # Process
    /// 1. Write header information (branching factor, height, etc.)
    /// 2. Write all nodes in order
    /// 3. Write the values array
    pub fn to_bytes(&self) -> Vec<u8> where V: Serialize {
        // Implementation will serialize the tree structure to bytes
    }

    /// Deserialize the tree from bytes.
    ///
    /// # Parameters
    /// * `bytes` - The byte slice containing the serialized tree
    ///
    /// # Returns
    /// The deserialized STree if successful
    ///
    /// # Process
    /// 1. Read and validate header information
    /// 2. Read all nodes
    /// 3. Read values array
    /// 4. Construct and return the tree
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, std::io::Error> where V: Deserialize {
        // Implementation will recreate the tree from bytes
    }

    /// Calculate the index of a child node.
    /// Using the generalized Eytzinger layout formula.
    ///
    /// # Parameters
    /// * `node_index` - Index of the parent node
    /// * `branch` - Which branch to follow (0 to branching_factor-1)
    ///
    /// # Returns
    /// The index of the child node in the flat array
    #[inline]
    fn child_index(&self, node_index: usize, branch: usize) -> usize {
        node_index * (self.branching_factor + 1) + branch + 1
    }

    /// Calculate the parent index of a node.
    /// Inverse of the Eytzinger layout formula.
    ///
    /// # Parameters
    /// * `node_index` - Index of the child node
    ///
    /// # Returns
    /// Some(parent_index) if this node has a parent, None for the root
    ///
    /// # Process
    /// Use integer arithmetic to find parent index
    #[inline]
    fn parent_index(&self, node_index: usize) -> Option<usize> {
        // Implementation will calculate parent index using arithmetic
        if node_index == 0 {
            None // Root node has no parent
        } else {
            Some((node_index - 1) / (self.branching_factor + 1))
        }
    }

    /// Get the level of a node in the tree.
    ///
    /// # Parameters
    /// * `node_index` - Index of the node
    ///
    /// # Returns
    /// The level of the node (0 for root, increasing downward)
    #[inline]
    fn node_level(&self, node_index: usize) -> usize {
        // Implementation will calculate level using arithmetic
    }

    /// Check if a node is a leaf node.
    ///
    /// # Parameters
    /// * `node_index` - Index of the node
    ///
    /// # Returns
    /// True if the node is a leaf, false otherwise
    #[inline]
    fn is_leaf_node(&self, node_index: usize) -> bool {
        self.node_level(node_index) == self.height - 1
    }
}
```

### NodeReader Trait

The `NodeReader` trait enables different strategies for loading nodes:

```rust
/// Trait for reading nodes from different storage backends.
/// Implementations include memory-based, file-based, and HTTP-based readers.
pub trait NodeReader<K: Key> {
    /// The error type returned by this reader
    type Error;

    /// Read a node at the given index.
    ///
    /// # Parameters
    /// * `index` - The index of the node to read
    ///
    /// # Returns
    /// The node if successful, or an error
    ///
    /// # Process
    /// 1. Calculate the byte offset for the node
    /// 2. Read the raw bytes from the storage medium
    /// 3. Deserialize the bytes into a Node structure
    fn read_node(&mut self, index: usize) -> Result<Node<K>, Self::Error>;

    /// Prefetch a node (optional optimization).
    /// This allows implementations to load nodes in advance to reduce latency.
    ///
    /// # Parameters
    /// * `index` - The index of the node to prefetch
    ///
    /// # Returns
    /// Success or error indicator
    ///
    /// # Process
    /// 1. Calculate byte offset for the node
    /// 2. Begin asynchronous loading without waiting for completion
    /// 3. Store in a cache for future access
    fn prefetch_node(&mut self, index: usize) -> Result<(), Self::Error>;

    /// Read multiple nodes at once (batch reading).
    /// This is an optimization for readers that can efficiently batch requests.
    ///
    /// # Parameters
    /// * `indices` - The indices of the nodes to read
    ///
    /// # Returns
    /// A vector of nodes if successful, or an error
    ///
    /// # Process
    /// 1. Calculate byte ranges for all requested nodes
    /// 2. Merge adjacent ranges to reduce request count
    /// 3. Read all bytes in as few operations as possible
    /// 4. Deserialize each node from the read bytes
    fn read_nodes_batch(&mut self, indices: &[usize]) -> Result<Vec<Node<K>>, Self::Error> {
        // Default implementation falls back to reading nodes individually
        indices.iter()
               .map(|&index| self.read_node(index))
               .collect()
    }
}
```

### Reader Implementations

```rust
/// Memory-based node reader for in-memory trees.
/// Provides the fastest access but requires loading the entire tree into memory.
pub struct MemoryNodeReader<K: Key> {
    /// All nodes of the tree stored in memory
    nodes: Vec<Node<K>>,

    /// Optional cache statistics for performance monitoring
    cache_stats: Option<CacheStatistics>,
}

impl<K: Key> MemoryNodeReader<K> {
    /// Create a new memory-based reader from a vector of nodes.
    ///
    /// # Parameters
    /// * `nodes` - Vector of all nodes in the tree
    ///
    /// # Returns
    /// A new MemoryNodeReader containing the nodes
    pub fn new(nodes: Vec<Node<K>>) -> Self {
        // Implementation stores nodes in memory
    }

    /// Create a new memory-based reader from a byte buffer.
    ///
    /// # Parameters
    /// * `bytes` - Byte slice containing serialized nodes
    /// * `node_count` - Number of nodes to deserialize
    ///
    /// # Returns
    /// A new MemoryNodeReader with deserialized nodes
    ///
    /// # Process
    /// 1. Determine node size from the first few bytes
    /// 2. Deserialize each node from its section of the buffer
    /// 3. Construct and return the reader
    pub fn from_bytes(bytes: &[u8], node_count: usize) -> Result<Self, std::io::Error> {
        // Implementation deserializes nodes from bytes
    }
}

impl<K: Key> NodeReader<K> for MemoryNodeReader<K> {
    type Error = std::io::Error;

    /// Read a node from memory.
    /// This is a simple vector lookup and should be very fast.
    fn read_node(&mut self, index: usize) -> Result<Node<K>, Self::Error> {
        // Implementation returns the node at the given index
    }

    /// Prefetch a node (No-op for memory reader as all nodes are already in memory).
    fn prefetch_node(&mut self, _index: usize) -> Result<(), Self::Error> {
        // No-op for memory reader
        Ok(())
    }

    /// Read multiple nodes at once (optimized for memory reader).
    /// Since all nodes are in memory, this is a simple batch of lookups.
    fn read_nodes_batch(&mut self, indices: &[usize]) -> Result<Vec<Node<K>>, Self::Error> {
        // Optimized implementation for batch reading from memory
    }
}

/// File-based node reader for accessing trees stored on disk.
/// Provides efficient random access to nodes in a file.
pub struct FileNodeReader<K: Key> {
    /// File handle for the tree data
    file: std::fs::File,

    /// Buffer for reading data from file
    buffer: Vec<u8>,

    /// Node offsets in the file
    node_offsets: Vec<u64>,

    /// Node size in bytes (fixed for the whole tree)
    node_size: usize,

    /// LRU cache for recently accessed nodes
    node_cache: Option<LruCache<usize, Node<K>>>,
}

impl<K: Key> FileNodeReader<K> {
    /// Create a new file-based reader.
    ///
    /// # Parameters
    /// * `file_path` - Path to the file containing the serialized tree
    /// * `cache_size` - Number of nodes to cache (0 to disable caching)
    ///
    /// # Returns
    /// A new FileNodeReader if the file can be opened
    ///
    /// # Process
    /// 1. Open the file
    /// 2. Read header to determine node count and sizes
    /// 3. Initialize buffer and cache
    /// 4. Return the configured reader
    pub fn new(file_path: &str, cache_size: usize) -> Result<Self, std::io::Error> {
        // Implementation opens file and sets up reader
    }
}

impl<K: Key> NodeReader<K> for FileNodeReader<K> {
    type Error = std::io::Error;

    /// Read a node from the file.
    ///
    /// # Process
    /// 1. Check cache for the node
    /// 2. If not found, calculate file offset for the node
    /// 3. Seek to that position in the file
    /// 4. Read the node bytes
    /// 5. Deserialize into a Node
    /// 6. Store in cache
    /// 7. Return the node
    fn read_node(&mut self, index: usize) -> Result<Node<K>, Self::Error> {
        // Implementation reads node from file
    }

    /// Prefetch a node into the cache.
    ///
    /// # Process
    /// 1. Check if node is already in cache
    /// 2. If not, read it from file
    /// 3. Store in cache without returning
    fn prefetch_node(&mut self, index: usize) -> Result<(), Self::Error> {
        // Implementation prefetches node from file
    }

    /// Read multiple nodes at once (optimized for file reader).
    ///
    /// # Process
    /// 1. Group requested nodes by proximity in file
    /// 2. Read each group with a single larger read
    /// 3. Deserialize individual nodes from the read buffer
    /// 4. Update cache with all new nodes
    fn read_nodes_batch(&mut self, indices: &[usize]) -> Result<Vec<Node<K>>, Self::Error> {
        // Optimized implementation for batch reading from file
    }
}

/// HTTP-based node reader for accessing trees stored in the cloud.
/// Optimized for HTTP range requests to minimize latency and bandwidth.
pub struct HttpNodeReader<K: Key> {
    /// HTTP client for making requests
    client: reqwest::Client,

    /// URL of the tree file
    url: String,

    /// Node offsets in the file
    node_offsets: Vec<u64>,

    /// Node size in bytes (fixed for the whole tree)
    node_size: usize,

    /// Header size in bytes
    header_size: usize,

    /// LRU cache for recently accessed nodes
    node_cache: LruCache<usize, Node<K>>,

    /// Configuration for batch sizes, timeouts, etc.
    config: HttpReaderConfig,
}

impl<K: Key> HttpNodeReader<K> {
    /// Create a new HTTP-based reader.
    ///
    /// # Parameters
    /// * `url` - URL of the serialized tree file
    /// * `config` - Configuration for HTTP parameters
    ///
    /// # Returns
    /// A new HttpNodeReader if the URL can be accessed
    ///
    /// # Process
    /// 1. Create HTTP client
    /// 2. Fetch file header to determine tree structure
    /// 3. Initialize node offsets and cache
    /// 4. Return the configured reader
    pub fn new(url: &str, config: HttpReaderConfig) -> Result<Self, reqwest::Error> {
        // Implementation sets up HTTP client and fetches header info
    }
}

impl<K: Key> NodeReader<K> for HttpNodeReader<K> {
    type Error = reqwest::Error;

    /// Read a node via HTTP range request.
    ///
    /// # Process
    /// 1. Check cache for the node
    /// 2. If not found, calculate byte range for the node
    /// 3. Make HTTP range request for that byte range
    /// 4. Deserialize response into a Node
    /// 5. Store in cache
    /// 6. Return the node
    fn read_node(&mut self, index: usize) -> Result<Node<K>, Self::Error> {
        // Implementation reads node via HTTP
    }

    /// Prefetch a node via HTTP.
    ///
    /// # Process
    /// 1. Check if node is already in cache
    /// 2. If not, initiate asynchronous HTTP request
    /// 3. Store in cache when request completes
    fn prefetch_node(&mut self, index: usize) -> Result<(), Self::Error> {
        // Implementation prefetches node via HTTP
    }

    /// Read multiple nodes at once (optimized for HTTP reader).
    ///
    /// # Process
    /// 1. Check which nodes are already in cache
    /// 2. Group missing nodes by proximity to minimize range requests
    /// 3. Make batch HTTP range requests for each group
    /// 4. Deserialize nodes from responses
    /// 5. Update cache with all new nodes
    fn read_nodes_batch(&mut self, indices: &[usize]) -> Result<Vec<Node<K>>, Self::Error> {
        // Optimized implementation for batch reading via HTTP
        // Merges adjacent ranges to minimize HTTP requests
    }
}
```

## Key Type Implementations

```rust
/// Implementation of Key trait for 32-bit integers.
impl Key for i32 {
    /// Convert integer to little-endian bytes.
    ///
    /// # Returns
    /// 4-byte array containing the integer in little-endian format
    fn to_bytes(&self) -> Vec<u8> {
        self.to_le_bytes().to_vec()
    }

    /// Create integer from little-endian bytes.
    ///
    /// # Parameters
    /// * `bytes` - Byte slice containing the serialized integer (must be 4 bytes)
    ///
    /// # Returns
    /// The deserialized i32 value
    fn from_bytes(bytes: &[u8]) -> Self {
        let arr = [bytes[0], bytes[1], bytes[2], bytes[3]];
        i32::from_le_bytes(arr)
    }

    /// Get the fixed size of an i32 (always 4 bytes).
    fn size_hint() -> Option<usize> {
        Some(4)
    }
}

/// Wrapper for safely using floating point numbers as keys.
/// Handles NaN values appropriately for correct ordering.
#[derive(Clone, Debug, PartialEq)]
pub struct OrderedFloat(pub f64);

impl Ord for OrderedFloat {
    // Implementation ensures NaN values have a consistent ordering
}

impl Key for OrderedFloat {
    /// Convert float to bytes with special handling for NaN.
    ///
    /// # Process
    /// 1. Handle special values (NaN) by mapping to a consistent byte pattern
    /// 2. For normal values, use IEEE 754 representation with adjustments
    ///    for correct ordering of negative values
    fn to_bytes(&self) -> Vec<u8> {
        // Implementation handles special IEEE 754 ordering concerns
    }

    /// Create float from bytes.
    ///
    /// # Process
    /// 1. Reverse the encoding process
    /// 2. Check for special byte patterns representing NaN
    /// 3. Reconstruct IEEE 754 representation
    fn from_bytes(bytes: &[u8]) -> Self {
        // Implementation properly handles NaN and ordering issues
    }

    /// Get the fixed size of a float (always 8 bytes).
    fn size_hint() -> Option<usize> {
        Some(8)
    }
}

/// Implementation of Key trait for Strings.
/// Uses prefix approach to handle variable-length strings efficiently.
impl Key for String {
    /// Convert string to bytes using a fixed prefix approach.
    ///
    /// # Process
    /// 1. Take up to the first N bytes of the UTF-8 string (configurable)
    /// 2. Add a flag byte if the string was truncated
    /// 3. Pad with zeros to the fixed length if shorter
    ///
    /// # Returns
    /// Fixed-length byte array for comparing and storing the string key
    fn to_bytes(&self) -> Vec<u8> {
        // Implementation of prefix-based string serialization
    }

    /// Create string from bytes.
    ///
    /// # Process
    /// 1. Remove any zero padding
    /// 2. Check truncation flag
    /// 3. Decode UTF-8 bytes into String
    ///
    /// # Note
    /// For truncated strings, this does not recover the original full string,
    /// only the prefix used for comparison.
    fn from_bytes(bytes: &[u8]) -> Self {
        // Implementation of string deserialization from prefix
    }

    /// String keys have variable size, so return None.
    fn size_hint() -> Option<usize> {
        None
    }
}

/// Custom DateTime type for chronological ordering.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct DateTime {
    /// Year component (0-9999)
    year: u16,
    /// Month component (1-12)
    month: u8,
    /// Day component (1-31)
    day: u8,
    /// Hour component (0-23)
    hour: u8,
    /// Minute component (0-59)
    minute: u8,
    /// Second component (0-59)
    second: u8,
    /// Nanosecond component (0-999,999,999)
    nanosecond: u32,
}

impl Key for DateTime {
    /// Convert DateTime to bytes in a format that preserves chronological order.
    ///
    /// # Process
    /// 1. Pack components in order of significance (year, month, day, etc.)
    /// 2. Ensure each component uses a fixed number of bytes
    ///
    /// # Returns
    /// Fixed-length byte array (12 bytes) representing the DateTime
    fn to_bytes(&self) -> Vec<u8> {
        // Implementation of datetime serialization
    }

    /// Create DateTime from bytes.
    ///
    /// # Process
    /// 1. Unpack each component from the byte array
    /// 2. Validate ranges for each component
    /// 3. Construct DateTime from components
    fn from_bytes(bytes: &[u8]) -> Self {
        // Implementation of datetime deserialization
    }

    /// Get the fixed size of a DateTime (always 12 bytes).
    fn size_hint() -> Option<usize> {
        Some(12)
    }
}
```

## Implementation Guidelines

### Tree Construction Process

The tree construction process follows these steps:

1. **Input Preparation**:
   - Ensure entries are sorted by key
   - Choose an appropriate branching factor (typically 16-64)

2. **Calculate Tree Parameters**:
   - Determine the number of leaf nodes needed
   - Calculate tree height from leaf count and branching factor
   - Allocate space for the total number of nodes

3. **Build Leaf Level**:
   - Distribute the sorted entries across leaf nodes
   - Fill each leaf node to capacity, except possibly the last one
   - Store the values array in the same order

4. **Build Internal Levels Bottom-Up**:
   - For each level, starting from the bottom:
     - Create parent nodes for groups of child nodes
     - For each parent, select separator keys from child nodes
     - Each parent has B-1 keys for B children

5. **Finalize Tree Structure**:
   - Set the root node at index 0
   - Ensure the implicit node relationships are correct
   - Verify tree parameters (height, node counts, etc.)

### Search Algorithm Process

The search process follows these steps:

1. **Initialize**:
   - Start at the root node (index 0)
   - Set current level to 0

2. **Traverse Internal Nodes**:
   - While not at a leaf node:
     - Use binary search to find the appropriate branch
     - Calculate child node index using the formula
     - Move to the child node

3. **Search Leaf Node**:
   - Once at a leaf node, use binary search to find the exact key
   - If found, return the corresponding value
   - If not found, return None

4. **Performance Optimization**:
   - Use branchless binary search where appropriate
   - Take advantage of CPU cache locality
   - Consider prefetching the next likely node

### Range Search Algorithm

The range search process follows these steps:

1. **Find Start Leaf**:
   - Use the search algorithm to find the leaf containing the start key
   - If the start key isn't found exactly, find the smallest key ≥ start_key

2. **Collect Matching Entries**:
   - Scan through the keys in the current leaf
   - Add all key-value pairs within the range to the result

3. **Move to Next Leaf**:
   - Determine the next leaf node using the tree's structure
   - For implicit layout, calculate the next leaf's index

4. **Continue Scanning**:
   - Scan each leaf node, adding matching entries to the result
   - Stop when reaching a key > end_key or the last leaf node

5. **Optimization**:
   - Use sequential memory access patterns
   - Consider batch loading of consecutive leaf nodes

### Serialization Format

The binary serialization format consists of:

1. **Header**:
   - Magic bytes for format identification (4 bytes)
   - Format version (1 byte)
   - Branching factor (2 bytes)
   - Tree height (1 byte)
   - Number of nodes (4 bytes)
   - Number of entries (4 bytes)
   - Key type information (4 bytes)

2. **Node Array**:
   - Each node serialized consecutively
   - Node format:
     - Is leaf flag (1 byte)
     - Number of keys (2 bytes)
     - Keys data (variable, depends on key type)

3. **Values Array**:
   - Each value serialized consecutively
   - Format depends on value type
   - For cloud optimization, these are typically offsets or identifiers

4. **Alignment**:
   - Nodes aligned to optimize memory access patterns
   - Padding may be added for alignment purposes

5. **Optimization**:
   - Fixed-size nodes for easier random access
   - Consistent node format for simpler navigation
