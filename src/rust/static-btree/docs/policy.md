
# Rust Static B+Tree (S+Tree) Implementation Plan

## 1. Introduction

This document outlines the implementation strategy and Rust API signatures for a static, implicit B+Tree, often referred to as an S+Tree. The goal is to create a highly performant, read-optimized B+Tree suitable for large, static datasets, emphasizing cache efficiency and minimal memory usage during queries.

The implementation follows the principles described in the [Algorithmica S+Tree article](https://en.algorithmica.org/hpc/data-structures/s-tree/), utilizing an implicit Eytzinger layout for node addressing and storing the entire tree structure contiguously.

## 2. Core Concepts

* **Static:** The tree is built once from sorted data and does not support insertions or deletions afterward.
* **Implicit Layout (Eytzinger):** Nodes are not linked by pointers. Instead, child nodes are located arithmetically based on the parent's index and the branching factor (`B`). The tree data is stored as a flat sequence of nodes, typically level by level.
  * Root node index: `0`
  * Children of node `k`: `k * B + 1` to `k * B + B` (assuming B children per node)
* **Packed Nodes:** Nodes are filled completely with keys (internal nodes) or entries (leaf nodes), maximizing space utilization (near 100%) and improving cache locality. The last node at each level might be partially filled depending on the total number of entries.
* **Read Optimization:** Designed for fast lookups (`find`) and range queries (`range`) by minimizing I/O (reading only necessary nodes) and leveraging CPU cache efficiency.
* **`Read + Seek` Abstraction:** The tree operates over any data source implementing Rust's `std::io::Read` and `std::io::Seek` traits, enabling use with files, memory buffers, and potentially network streams (with an adapter).

## 3. Error Handling

A custom error enum will consolidate potential issues.

```rust
use std::io;

#[derive(Debug)] // Implement std::error::Error later
pub enum Error {
    IoError(io::Error),
    InvalidFormat(String), // Errors related to file structure/magic bytes/version
    KeySerializationError(String),
    KeyDeserializationError(String),
    BuildError(String), // Errors during the build process
    QueryError(String), // Errors during find/range operations
    // Add other specific errors as needed
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::IoError(e)
    }
}
```

## 4. Value Type

The value associated with each key. For the initial `flatcitybuf` use case, this will typically be a byte offset (`u64`).

```rust
use std::mem;

// Using u64 for byte offsets as specified in project context.
pub type Value = u64;

// Constant for value size, assuming Value = u64
const VALUE_SIZE: usize = mem::size_of::<Value>();
```

## 5. Key Abstraction (`trait Key`)

Keys must be comparable and have a fixed serialized size to ensure consistent node layouts. Variable-length types (like strings) must be handled via fixed-size representations (e.g., prefixes).

```rust
use std::io::{Read, Write};
use std::cmp::Ordering;
use std::fmt::Debug;

/// Trait for keys used in the StaticBTree.
/// Keys must have a fixed serialized size. Variable-length types
/// like String must be handled using fixed-size prefixes.
pub trait Key: Sized + Ord + Clone + Debug {
    /// The fixed size of the key when serialized, in bytes.
    const SERIALIZED_SIZE: usize;

    /// Serialize the key into a writer.
    /// Must write exactly `SERIALIZED_SIZE` bytes.
    fn write_to<W: Write>(&self, writer: &mut W) -> Result<(), Error>;

    /// Deserialize the key from a reader.
    /// Must read exactly `SERIALIZED_SIZE` bytes.
    fn read_from<R: Read>(reader: &mut R) -> Result<Self, Error>;
}
```

* **Implementation Strategy:** Provide implementations for common fixed-size types (`i32`, `u32`, `i64`, `u64`, `f32`, `f64` via `ordered-float`) and create strategies for variable types like `String` (e.g., `FixedStringKey<N>`) and date/time types.

## 6. Entry Struct (`struct Entry`)

Represents a key-value pair. Used primarily for leaf nodes and as input during tree construction.

```rust
use std::io::{Read, Write};
use std::cmp::Ordering;
use std::fmt::Debug;

/// Represents a Key-Value pair, primarily for leaf nodes and input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry<K: Key, V: Value> {
    pub key: K,
    pub value: V,
}

impl<K: Key, V: Value> Entry<K, V> {
    // Assuming Value is u64 for now.
    const VALUE_SIZE: usize = mem::size_of::<Value>();
    const SERIALIZED_SIZE: usize = K::SERIALIZED_SIZE + Self::VALUE_SIZE;

    pub fn write_to<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        self.key.write_to(writer)?;
        writer.write_all(&self.value.to_le_bytes())?; // Assuming little-endian for value
        Ok(())
    }

    pub fn read_from<R: Read>(reader: &mut R) -> Result<Self, Error> {
        let key = K::read_from(reader)?;
        let mut value_bytes = [0u8; Self::VALUE_SIZE];
        reader.read_exact(&mut value_bytes)?;
        let value = Value::from_le_bytes(value_bytes);
        Ok(Entry { key, value })
    }
}

// Implement Ord based on Key only for sorting inputs/searching leaves
impl<K: Key, V: Value> PartialOrd for Entry<K, V> {
     fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.key.partial_cmp(&other.key)
    }
}
impl<K: Key, V: Value> Ord for Entry<K, V> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.key.cmp(&other.key)
    }
}
```

## 7. Main Tree Structure (`struct StaticBTree`)

The primary struct for interacting with an existing Static B+Tree. It holds metadata and the `Read + Seek` data source.

```rust
use std::io::{Read, Seek, SeekFrom};
use std::marker::PhantomData;
use std::fmt::Debug;

/// Represents the static B+Tree structure, providing read access.
/// R is the underlying readable and seekable data source.
#[derive(Debug)]
pub struct StaticBTree<K: Key, R: Read + Seek> {
    reader: R,
    branching_factor: u16, // B (number of keys/entries per node)
    num_entries: u64,    // Total key-value entries in the tree
    height: u8,          // Tree height (0=empty, 1=root-only leaf)
    header_size: u64,      // Byte size of the header section
    // Pre-calculated sizes for efficiency
    key_size: usize,
    value_size: usize,     // mem::size_of::<Value>()
    internal_node_byte_size: usize, // branching_factor * key_size
    leaf_node_byte_size: usize,     // branching_factor * Entry::SERIALIZED_SIZE
    // Layout information derived from header/calculation
    num_nodes_per_level: Vec<u64>, // Nodes count at each level [root, ..., leaves]
    level_start_offsets: Vec<u64>, // Byte offset where each level's nodes start (after header)
    _phantom_key: PhantomData<K>,
}

impl<K: Key, R: Read + Seek> StaticBTree<K, R> {
    /// Opens an existing StaticBTree from a reader.
    /// Reads the header to validate and configure the tree structure.
    pub fn open(mut reader: R) -> Result<Self, Error> {
        // Implementation Strategy:
        // 1. Seek reader to start (SeekFrom::Start(0)).
        // 2. Read and validate magic bytes & version identifier.
        // 3. Read metadata: branching_factor, num_entries, height.
        // 4. Calculate/verify: header_size, key_size=K::SERIALIZED_SIZE, value_size.
        // 5. Calculate node byte sizes: internal_node_byte_size, leaf_node_byte_size.
        // 6. Calculate Eytzinger layout:
        //    - Determine num_nodes_per_level based on num_entries and branching_factor (bottom-up or top-down).
        //    - Calculate level_start_offsets based on node counts and sizes for preceding levels.
        // 7. Store all metadata in the `Self` struct fields.
        // 8. Return `Ok(Self)`.
        unimplemented!("StaticBTree::open")
    }

    /// Finds the value associated with a given key. Returns `Ok(None)` if not found.
    pub fn find(&mut self, key: &K) -> Result<Option<Value>, Error> {
        // Implementation Strategy:
        // 1. Handle edge case: If height == 0 (empty tree), return Ok(None).
        // 2. Initialize: current_node_index = 0, current_level = 0.
        // 3. Loop while current_level < height - 1 (i.e., while in internal nodes):
        //    a. `keys = self.read_internal_node_keys(current_node_index)?`.
        //    b. Perform binary search on `keys` to find the index `branch` such that `keys[branch] <= key < keys[branch+1]`. Handle edge cases (key < first, key >= last).
        //    c. Calculate child index using Eytzinger: `child_node_index = current_node_index * B + branch`. (Adjust formula based on exact Eytzinger variant).
        //    d. Update `current_node_index = child_node_index`, increment `current_level`.
        // 4. Now at leaf level (current_level == height - 1):
        //    a. `entries = self.read_leaf_node_entries(current_node_index)?`.
        //    b. Perform binary search on `entries` using `key`.
        //    c. If found at index `i`, return `Ok(Some(entries[i].value))`.
        //    d. If not found, return `Ok(None)`.
        unimplemented!("StaticBTree::find")
    }

    /// Returns an iterator over entries within the specified key range
    /// (inclusive start, exclusive end).
    /// Note: Iterator implementation needs careful handling of borrows.
    pub fn range(&mut self, start_key: &K, end_key: &K) -> Result<impl Iterator<Item = Result<Entry<K, Value>, Error>> + '_, Error> {
        // Implementation Strategy:
        // 1. Descend tree (like `find`) to locate the leaf node and starting index `idx`
        //    where `start_key` would reside (or the first key >= start_key).
        // 2. If no key >= start_key exists, return empty iterator immediately.
        // 3. Initialize iterator state:
        //    - `current_leaf_node_index`: Index of the leaf node currently being processed.
        //    - `current_entries`: `Vec<Entry<K, Value>>` holding the current leaf's data.
        //    - `current_idx_in_node`: Index within `current_entries` to yield next.
        // 4. Iterator `next()` method:
        //    a. If `current_idx_in_node >= current_entries.len()`:
        //       - Increment `current_leaf_node_index`.
        //       - Check if `current_leaf_node_index` is still a valid leaf node index for the last level. If not, return `None`.
        //       - `self.current_entries = self.read_leaf_node_entries(current_leaf_node_index)?`.
        //       - `self.current_idx_in_node = 0`.
        //    b. If `current_entries` is empty (shouldn't happen with valid build unless tree empty), return `None`.
        //    c. Get `entry = &self.current_entries[self.current_idx_in_node]`.
        //    d. If `entry.key >= end_key`, return `None` (end of range).
        //    e. Increment `self.current_idx_in_node`.
        //    f. Return `Some(Ok(entry.clone()))`. // Clone needed to yield owned value.
        // Note: This sketch requires a dedicated iterator struct to manage state and borrows correctly.
        let results: Vec<Result<Entry<K, Value>, Error>> = Vec::new(); // Placeholder
        Ok(results.into_iter())
        // unimplemented!("StaticBTree::range")
    }

    // --- Internal Helpers ---

    /// Reads and deserializes all keys from an internal node.
    fn read_internal_node_keys(&mut self, node_index: u64) -> Result<Vec<K>, Error> {
        // Strategy:
        // 1. `offset = self.calculate_node_offset(node_index)?`.
        // 2. `self.reader.seek(SeekFrom::Start(offset))?`.
        // 3. Read `self.internal_node_byte_size` bytes into a buffer.
        // 4. Create a `Cursor` over the buffer.
        // 5. Loop `self.branching_factor` times, calling `K::read_from` on the cursor.
        // 6. Collect keys into a Vec and return. Handle partial last node if necessary.
        unimplemented!("read_internal_node_keys")
    }

    /// Reads and deserializes all entries from a leaf node.
    fn read_leaf_node_entries(&mut self, node_index: u64) -> Result<Vec<Entry<K, Value>>, Error> {
        // Strategy:
        // 1. `offset = self.calculate_node_offset(node_index)?`.
        // 2. `self.reader.seek(SeekFrom::Start(offset))?`.
        // 3. Read `self.leaf_node_byte_size` bytes into a buffer.
        // 4. Create a `Cursor` over the buffer.
        // 5. Loop `self.branching_factor` times, calling `Entry::<K, Value>::read_from` on the cursor.
        // 6. Collect entries into a Vec and return. Handle partial last node if necessary (read only actual count based on total entries if metadata available).
        unimplemented!("read_leaf_node_entries")
    }

     /// Calculates the absolute byte offset for a given node index based on calculated layout.
    fn calculate_node_offset(&self, node_index: u64) -> Result<u64, Error> {
        // Strategy:
        // 1. Determine the `level` of `node_index` by checking cumulative node counts in `num_nodes_per_level`.
        // 2. Calculate the `start_node_index_of_level`.
        // 3. Calculate `relative_index_in_level = node_index - start_node_index_of_level`.
        // 4. Get `level_start_offset = self.level_start_offsets[level]`.
        // 5. Get `node_size =` if level == height-1 { leaf_node_byte_size } else { internal_node_byte_size }.
        // 6. Calculate final offset: `self.header_size + level_start_offset + relative_index_in_level * node_size`.
        // 7. Return `Ok(offset)`. Handle potential index out of bounds errors.
         unimplemented!("calculate_node_offset")
    }

     /// Returns the branching factor B.
     pub fn branching_factor(&self) -> u16 { self.branching_factor }
     /// Returns the total number of entries.
     pub fn len(&self) -> u64 { self.num_entries }
     /// Returns true if the tree is empty.
     pub fn is_empty(&self) -> bool { self.num_entries == 0 }
     /// Returns the height of the tree.
     pub fn height(&self) -> u8 { self.height }
}
```

## 8. Tree Builder (`struct StaticBTreeBuilder`)

Responsible for constructing the tree file/data from sorted input entries.

```rust
use std::io::{Read, Seek, Write, SeekFrom};
use std::marker::PhantomData;

/// Builder structure for creating a StaticBTree.
pub struct StaticBTreeBuilder<K: Key, W: Write + Seek> {
    writer: W,
    branching_factor: u16,
    num_entries: u64,
    // Internal state for bottom-up build
    // e.g., buffers for keys promoting to the next level
    _phantom_key: PhantomData<K>,
}

impl<K: Key, W: Write + Seek> StaticBTreeBuilder<K, W> {
    /// Creates a new builder targeting the given writer.
    /// `branching_factor` dictates node size (must be > 1).
    pub fn new(mut writer: W, branching_factor: u16) -> Result<Self, Error> {
        // Implementation Strategy:
        // 1. Validate branching_factor.
        // 2. Seek writer to start.
        // 3. Write a placeholder header (or reserve space of known size). Store header_size.
        // 4. Initialize internal state (num_entries = 0, etc.).
        // 5. Return `Ok(Self)`.
        unimplemented!("StaticBTreeBuilder::new")
    }

    /// Builds the entire tree from an iterator providing pre-sorted entries.
    /// This is the primary method for constructing the tree.
    pub fn build_from_sorted<I>(mut self, sorted_entries: I) -> Result<(), Error>
    where
        I: IntoIterator<Item = Result<Entry<K, Value>, Error>>,
    {
        // Implementation Strategy (Bottom-Up):
        // 1. Count entries (or use size_hint). Store in `self.num_entries`. Handle empty input.
        // 2. Phase 1: Write Leaf Nodes
        //    - Iterate through `sorted_entries`.
        //    - Group entries into chunks of `branching_factor`.
        //    - For each chunk:
        //        - Pad the last chunk if necessary to fill the node (S+Tree convention).
        //        - Serialize the `branching_factor` entries into a leaf node buffer.
        //        - Write the buffer to `self.writer`.
        //        - Store the *first key* of this leaf node buffer in `promoted_keys` (for the level above).
        // 3. Phase 2..N: Write Internal Nodes (repeat until root)
        //    - Input is `promoted_keys` from the level below.
        //    - While `promoted_keys.len() > 1`:
        //        - Group `promoted_keys` into chunks of `branching_factor`.
        //        - Create `next_level_promoted_keys` buffer.
        //        - For each chunk:
        //            - Pad the last chunk if needed.
        //            - Serialize the `branching_factor` keys into an internal node buffer.
        //            - Write the buffer to `self.writer`.
        //            - Store the *first key* of this chunk in `next_level_promoted_keys`.
        //        - `promoted_keys = next_level_promoted_keys`.
        // 4. Finalization:
        //    - Calculate final tree height.
        //    - Calculate node counts per level and level start offsets (based on writer position changes or calculation).
        //    - Seek `self.writer` back to `SeekFrom::Start(0)`.
        //    - Write the final, complete header (magic bytes, version, branching_factor, num_entries, height, etc.).
        //    - Flush `self.writer`.
        // 5. Return `Ok(())`.
        unimplemented!("StaticBTreeBuilder::build_from_sorted")
    }

    // `add` method for single entries is complex due to bottom-up nature and buffering.
    // `build_from_sorted` is strongly preferred.
    // pub fn add(&mut self, entry: Entry<K, Value>) -> Result<(), Error> { ... }
}
```

## 9. Serialization Format

The serialized file/data will consist of:

1. **Header:**
    * Magic Bytes (e.g., `b"STREE01"`).
    * Version Number.
    * `branching_factor` (e.g., `u16`).
    * `num_entries` (`u64`).
    * `height` (`u8`).
    * (Optional but useful: Key Size, Value Size, any other global metadata).
    * Total Header Size marker (to easily skip it).
2. **Nodes:**
    * A contiguous block of bytes containing all nodes.
    * Arranged using the implicit Eytzinger layout (typically level by level, starting from root).
    * **Internal Nodes:** Contain exactly `branching_factor` keys (`K`). Total size = `branching_factor * K::SERIALIZED_SIZE`.
    * **Leaf Nodes:** Contain exactly `branching_factor` entries (`Entry<K, Value>`). Total size = `branching_factor * Entry::<K, Value>::SERIALIZED_SIZE`.
    * The last node at each level *might* be implicitly padded if the total number of items doesn't perfectly divide by `branching_factor`. The reading logic needs to be aware of the actual number of valid items based on `num_entries`.

## 10. Key Implementations (Examples)

Provide concrete implementations of the `Key` trait.

```rust
// Example: i32 Key
impl Key for i32 {
    const SERIALIZED_SIZE: usize = 4;
    // ... (write_to, read_from as shown before) ...
}

// Example: Fixed-Prefix String Key
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct FixedStringKey<const N: usize>([u8; N]);

impl<const N: usize> Key for FixedStringKey<N> {
     const SERIALIZED_SIZE: usize = N;
    // ... (write_to, read_from as shown before) ...
}
impl<const N: usize> FixedStringKey<N> {
    pub fn from_str(s: &str) -> Self { /* ... truncate/pad ... */ unimplemented!() }
    pub fn to_string_lossy(&self) -> String { /* ... handle null bytes ... */ unimplemented!() }
}

// Add implementations for u32, i64, u64, f32/f64 (using ordered-float), chrono types etc.
```

## 11. Future Work

* **Caching:** Implement an optional caching layer (e.g., LRU cache) to keep recently accessed nodes in memory, reducing I/O for repeated queries on the same nodes.
* **Prefetching:** Explore strategies to read ahead sibling nodes or child nodes speculatively, potentially improving latency for sequential access patterns (like range queries).
* **HTTP `Read+Seek` Adapter:** Create a struct that implements `Read + Seek` using HTTP Range Requests, allowing `StaticBTree` to operate directly on files hosted remotely.
* **Performance Benchmarking:** Thoroughly benchmark against other B-Tree implementations and tune the branching factor.
* **More Key Types:** Add robust implementations for more complex key types.
* **Advanced Builder Options:** Allow configuration for handling partially filled last nodes.
