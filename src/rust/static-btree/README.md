# static-btree

A high-performance, memory-efficient implementation of a static B+tree (S+tree) for FlatCityBuf.

## Overview

`static-btree` provides an immutable B+tree implementation optimized for read-only workloads with a focus on search performance and memory efficiency. Unlike traditional B-trees which allocate space for future insertions and use pointers to navigate between nodes, this implementation uses an implicit tree layout that eliminates pointers and maximizes space utilization.

## Key Features

- **Cache-Friendly Design**: Node structure is optimized for CPU cache lines, resulting in fewer cache misses during traversal
- **SIMD-Accelerated Search**: Uses SIMD instructions for parallel key comparisons within nodes
- **Implicit Structure**: No pointers between nodes, reducing memory usage and improving cache locality
- **Configurable Branching Factor**: Allows tuning of the tree structure based on workload characteristics
- **Multiple Storage Backends**: Works with in-memory data, file system storage, or HTTP-based remote storage
- **Zero-Copy Design**: Minimizes memory allocations during queries

## Implementation Details

This crate implements what we call an S+tree (Static B+tree), based on research from [Algorithmica](https://en.algorithmica.org/hpc/data-structures/s-tree/), which can be up to 15x faster than traditional B-trees for large datasets while using only 6-7% more memory (or even less in some cases).

The key innovations include:

1. **Implicit Node Layout**: Nodes are arranged in memory according to their position in the tree, eliminating the need for child pointers
2. **Dense Packing**: All nodes are completely filled (except possibly the last node at each level)
3. **Vectorized Search**: SIMD instructions are used to compare multiple keys simultaneously
4. **Cache-Optimized Structure**: Node size is aligned with CPU cache lines (typically 64 bytes)

## Use Cases

This implementation is ideal for:

- Read-heavy workloads with rare or no updates
- Applications requiring high search throughput
- Space-constrained environments
- Working with static datasets like geographic information

## Getting Started

```rust
use static_btree::{StaticBTree, KeyEncoder};

// Create a tree builder with a branching factor of 16
let mut builder = StaticBTreeBuilder::new(16, MyKeyEncoder::new());

// Add sorted entries
for (key, value) in sorted_data {
    builder.add_entry(key, value);
}

// Build the tree
let tree = builder.build();

// Query the tree
let result = tree.search(&some_key);
```

## Comparison with Dynamic B-trees

While dynamic B-trees like those in the `btree` crate support efficient insertions and deletions, this static implementation offers:

- Up to 15x faster search performance
- Lower memory usage
- Better cache locality
- Simpler implementation

However, it does not support modifying the tree after construction - it must be rebuilt if the data changes.

## Related FlatCityBuf Components

- `btree`: Dynamic B-tree implementation supporting modifications
- `bst`: Binary search tree implementation
- `packed_rtree`: Packed R-tree implementation for spatial indexing
