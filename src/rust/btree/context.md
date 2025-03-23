# B-tree Index for FlatCityBuf

## Overview

The B-tree implementation provides an efficient attribute indexing system for FlatCityBuf, replacing the previous Binary Search Tree (BST) approach. This change significantly improves query performance, memory usage, and supports more efficient operation over HTTP range requests.

## Why B-trees instead of BST?

### Performance Advantages
- **Reduced I/O Operations**: B-trees minimize disk reads by storing multiple keys per node, reducing the height of the tree.
- **Better Cache Utilization**: Fixed-size nodes align with operating system page sizes, improving cache performance.
- **Range Query Efficiency**: Linked leaf nodes enable efficient range scans without traversing back up the tree.
- **Bulk Loading**: Bottom-up construction enables efficient bulk loading from sorted data.

### Memory Efficiency
- **Block-Based Storage**: Uses fixed-size blocks (typically 4KB) that align with page sizes.
- **Compact Representation**: Stores multiple entries per node, reducing overhead.
- **Progressive Loading**: Loads only needed parts of the index when accessed over HTTP.

### HTTP Optimization
- **Range Request Efficiency**: Fetches entire nodes in single requests, reducing HTTP overhead.
- **Caching**: Client-side caching of frequently accessed nodes improves performance.
- **Reduced Request Count**: B-trees require fewer nodes to search, reducing the number of HTTP requests.

## Design Decisions

### Storage Abstraction
- Implemented a `BlockStorage` trait to abstract storage operations, enabling both:
  - File-based storage for local operation
  - HTTP-based storage for remote operation
  - Memory-based storage for testing
- Block size fixed at 4KB to align with typical page sizes and HTTP range request efficiency

### Type-Safe Key Encoding
- Implemented a `KeyEncoder` trait to handle different attribute types:
  - Integers: Encoded with proper byte ordering
  - Floating point: Special handling for NaN, +/-Infinity
  - Strings: Fixed-width prefix with overflow handling
  - Timestamps: Normalized representation

### Node Structure
- **Internal Nodes**: Store keys and child pointers
- **Leaf Nodes**: Store keys and values (feature offsets)
- Both node types stored in fixed-size blocks
- Linked leaf nodes for efficient range queries

### HTTP Implementation
- Integrated with the existing `AsyncHttpRangeClient` interface
- Implemented block-level LRU caching
- Added metrics collection for performance analysis
- Designed to work with the FlatCityBuf remote access pattern

## Comparison with Previous BST Implementation

| Factor | B-tree (New) | BST (Previous) |
|--------|--------------|----------------|
| **I/O Efficiency** | Multiple entries per node | One entry per node |
| **Tree Height** | Log_B(N) - much shorter | Log_2(N) |
| **Cache Locality** | Excellent - fixed blocks | Poor - variable nodes |
| **HTTP Requests** | Fewer, larger requests | Many small requests |
| **Memory Usage** | Lower - compact representation | Higher - more pointers |
| **Range Queries** | Linear scan of leaf nodes | Tree traversal required |
| **Bulk Loading** | Efficient bottom-up construction | Requires rebalancing |
| **Implementation Complexity** | Higher | Lower |

## Integration with FlatCityBuf

The B-tree is integrated with FlatCityBuf in several ways:

1. **Attribute Index Structure**: B-trees provide the index for each attribute type (e.g., building height, name).
2. **Query System**: A unified `QueryExecutor` combines B-tree and R-tree queries.
3. **HTTP Range Requests**: Both B-tree and R-tree indices support partial fetching over HTTP.
4. **File Format**: The B-tree structure is stored in the FlatCityBuf file alongside the R-tree and feature data.

## Future Work

1. **Node Compression**: Investigate compression techniques for B-tree nodes.
2. **Hybrid Approaches**: Combine B-tree structure with columnar storage for certain attribute types.
3. **Advanced Caching**: Implement predictive prefetching based on access patterns.
4. **Distributed Operation**: Support for distributed B-tree sharding across multiple files.
5. **WASM Optimization**: Special optimizations for WebAssembly environments.

## References

- [B-tree - Wikipedia](https://en.wikipedia.org/wiki/B-tree)
- [Cache-Oblivious B-Trees](https://www.cs.cmu.edu/~guyb/papers/jacm06.pdf) - Bender, Demaine, Farach-Colton
- [Efficient Locking for Concurrent Operations on B-Trees](https://www.csd.uoc.gr/~hy460/pdf/p650-lehman.pdf) - Lehman, Yao
- [The Ubiquitous B-Tree](https://dl.acm.org/doi/10.1145/356770.356776) - Comer
