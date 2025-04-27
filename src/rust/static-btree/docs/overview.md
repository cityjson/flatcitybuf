# Static B+Tree (S+Tree) Crate Overview

This document provides a comprehensive overview of the static-btree crate, detailing its internal structure, modules, and workflows for tree construction and search operations.

## 1. File Structure and Module Overview

The static-btree crate is organized into the following key modules:

| File/Module | Description |
|-------------|-------------|
| `lib.rs` | Entry point for the crate, re-exports public modules and types |
| `stree.rs` | Core implementation of the Static B+Tree (S+Tree) structure |
| `error.rs` | Error types and handling for the crate |
| `entry.rs` | Implementation of node entries (`NodeItem<K>` / `Entry<K>`) |
| `key.rs` | Trait definitions and implementations for key types |
| `mocked_http_range_client.rs` | Mock HTTP client for testing HTTP-based operations |

### Key Components

- **`Entry<K>`**: Represents an item in a tree node, containing a key and an offset.
- **`Stree<K>`**: The core tree structure, managing node items and tree layout.
- **`Key` trait**: Defines operations required for key types (comparison, serialization).
- **`HttpSearchResultItem`**: Represents search results when using HTTP-based querying.
- **`SearchResultItem`**: Represents search results when using direct memory or file access.

## 2. Tree Construction Workflow

The static B+Tree construction follows a sequential process designed for efficient read-only operations:

### 2.1. Data Preparation

1. **Collection of Entries**:
   - Starts with a collection of `Entry<K>` items containing keys and their corresponding offsets.
   - Each entry represents a key-value pair, where the value is referenced by an offset.

2. **Duplicate Handling**:
   - Identify and handle duplicate keys.
   - For duplicate keys, offsets are stored in a separate payload area.
   - The tree itself stores only unique keys, with references to the payload for duplicates.

### 2.2. Tree Structure Calculation

1. **Level Bounds Calculation** (`generate_level_bounds`):
   - Calculates the number of nodes per level based on:
     - Total number of entries
     - Branching factor (B)
   - Creates a vector of ranges where each range represents a level in the tree.
   - First range represents leaf nodes, last range represents the root.

2. **Node Structure Calculation**:
   - B-1 keys per node (where B is the branching factor).
   - Nodes are stored contiguously in a level-by-level layout.
   - Level bounds store the start and end index of each level.

### 2.3. Tree Building Process (`generate_nodes`)

1. **Bottom-up Construction**:
   - Starts with leaf nodes (already prepared).
   - Builds internal nodes level by level, moving upward toward the root.

2. **Internal Node Generation**:
   - For each parent node slot, selects the minimum key from the right subtree.
   - Sets the offset to point to the corresponding child node's position.

3. **Packing Strategy**:
   - Tree is built with maximum packing density.
   - Last nodes at each level may be partially filled, but most nodes are fully utilized.
   - Follows the Eytzinger layout to optimize cache efficiency.

### 2.4. Payload Encoding

1. **Standard Entries**:
   - For entries with unique keys, the offset directly points to the value.

2. **Duplicate Key Handling**:
   - When duplicate keys exist, a special bit pattern in the offset (`PAYLOAD_TAG`) indicates it points to the payload area.
   - The actual offset is masked with `PAYLOAD_MASK` to get the position in the payload area.
   - Payload entries store multiple offsets for the duplicate keys.
   - `PayloadEntry` structure contains a list of offsets for duplicate key values.

### 2.5. Serialization (`stream_write`)

1. **Data Layout**:
   - Tree is serialized level by level, starting from the root downward.
   - Nodes and their entries are stored contiguously.
   - Payload area is appended at the end if duplicates exist.

## 3. Search Operations

The crate provides various search operations optimized for different access patterns and data sources:

### 3.1. Direct Memory Search Operations

| Function | Description |
|----------|-------------|
| `find_exact` | Finds entries with an exact key match in memory-loaded tree |
| `find_range` | Finds entries with keys in a specified range in memory-loaded tree |
| `find_partition` | Finds the index where a key would be inserted (useful for determining positions) |

### 3.2. Stream-based Search Operations

| Function | Description |
|----------|-------------|
| `stream_find_exact` | Finds exact matches by reading only necessary nodes from a `Read + Seek` source |
| `stream_find_range` | Finds range matches by efficiently reading nodes within the range from a `Read + Seek` source |
| `stream_find_partition` | Finds insertion position by navigating the tree through streaming |

### 3.3. HTTP-based Search Operations

| Function | Description |
|----------|-------------|
| `http_stream_find_exact` | Performs exact match search using HTTP range requests to fetch only needed nodes |
| `http_stream_find_range` | Performs range search using HTTP range requests, optimizing data transfer |
| `http_stream_find_partition` | Determines insertion position through HTTP requests, useful for binary search |

### 3.4. Search Algorithm

The search process follows these general steps:

1. **Tree Navigation**:
   - Start at the root node.
   - At each internal node, use binary search to find the child node to descend to.
   - Continue until reaching a leaf node.

2. **Leaf Processing**:
   - For exact search: Use binary search to find the exact key.
   - For range search: Scan leaf nodes within the range boundaries.

3. **Result Collection**:
   - For each matching key, determine if it points to a direct value or payload area.
   - If direct, return the offset as a result.
   - If pointing to payload, read all offsets from the payload area and return multiple results.

4. **HTTP Optimization**:
   - HTTP-based operations use range requests to fetch only needed nodes.
   - Implements request batching when adjacent nodes need to be read to reduce HTTP round trips.
   - Uses `AsyncHttpRangeClient` to perform asynchronous HTTP operations.

## 4. Performance Considerations

1. **Cache Efficiency**:
   - Uses Eytzinger layout for better cache locality.
   - Nodes at the same level are stored contiguously.

2. **Minimal I/O**:
   - Only reads nodes necessary for the search path.
   - Uses binary search within nodes to minimize comparisons.

3. **HTTP Optimizations**:
   - Batches adjacent node requests to reduce HTTP overhead.
   - Uses range requests to fetch only needed data.
   - Buffers HTTP responses for better performance.

4. **Memory Efficiency**:
   - Static structure allows for precise memory allocation.
   - Compact representation with minimal overhead.

## 5. Limitations and Constraints

1. **Read-Only Structure**:
   - Tree is immutable after construction.
   - No support for insertions, deletions, or updates.

2. **Construction Cost**:
   - One-time O(N) build cost in both time and space.
   - Must rebuild the entire tree to incorporate new data.

3. **Memory Requirements**:
   - Full tree needs to be loaded in memory during construction.
   - Stream-based operations require minimal memory during queries.
