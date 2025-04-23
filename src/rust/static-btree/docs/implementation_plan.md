# Static B+Tree (S+Tree) Implementation Plan

**Project:** Implement the `static-btree` Rust Crate

**Goal:** Create a Rust crate for a Static B+Tree (S+Tree) optimized for read performance.

## 1. Introduction

This document outlines the implementation strategy and detailed Rust API signatures for a static, implicit B+Tree (S+Tree). The goal is to create a highly performant, read-optimized B+Tree suitable for large, static datasets, emphasizing cache efficiency and minimal memory usage during queries.

The implementation follows the principles described in the [Algorithmica S+Tree article](https://en.algorithmica.org/hpc/data-structures/s-tree/), utilizing an implicit Eytzinger layout for node addressing and storing the entire tree structure contiguously.
YOU SHOULD READ THIS ARTICLE!

## 2. Core Concepts

* **Static:** Built once, read many times. No modifications after build.
* **Implicit Layout (Eytzinger):** Nodes located arithmetically, not via pointers. Stored contiguously, often level-by-level.
* **Packed Nodes:** Nodes are fully utilized (except potentially the last one per level) for better space and cache efficiency.
* **Read Optimization:** Designed for fast lookups and range scans by minimizing I/O (reading only needed nodes).
* **`Read + Seek` Abstraction:** Operates on standard Rust I/O traits, enabling use with files, memory, etc.

## 3. Implementation Policy

The following policy dictates how the Static B+Tree will be **built**, **stored**, and **queried** inside FlatCityBuf.  All decisions reconcile the Algorithmica S+Tree paper with our own storage constraints.

### 3.1 Terminology & Symbols

| Symbol | Meaning                          |
|--------|----------------------------------|
| `B`    | The number of pointers to children in a node. The number of items in a node is `B-1`. |
| `N`    | Total number of entries          |
| `H`    | Height of the tree               |
| `K`    | Key type implementing `Key`      |
| `O`    | Offset (`u64`) – value payload. This is the offset of the value in the payload. |
| `payload` | The buffer where actual data offsets are stored. When there are duplicates, the payload is is the array of offsets of the duplicate values. |

### 3.2 Node Layout

1. **Leaf Nodes**
   * Store **up to `B`-1 keys** followed by their offsets.
   * No explicit *next‑leaf* pointer is required – leaves are stored **contiguously**; the next leaf is `index + 1` except for the right‑most leaf.
2. **Internal Nodes**
   * Store keys and offsets. The offset is the offset index of the first (leftmost) item in the child node. For the parent, its children nodes have smaller keys than the parent. All node items in the child node can be retrieved by the offset and the number of items in the child node (B-1).
   * Each key `i` equals the **smallest key in the `(i+1)`‑th child subtree** (standard B+ invariant).
   * No padding is required for the last child node. Tree is packed.
3. **Layer Packing**
   * Layers are written **top‑down (root → leaves)** in both memory & on‑disk representation.
   * Items in each layer are stored as single array. e.g. the root node is the first B-1 items, the next level is the next B items, etc. `[root node item1, root node item2, ..., root node itemB-1, child node item1, child node item2, ..., child node itemB-1]`.

### 3.3 Layer layout calculation

* `generate_level_bounds` function in `stree.rs` calculates the layout of the layers. `level_bounds` is a vector of ranges. The start of each range is the offset of the first item in the layer.
* `generate_nodes` function in `stree.rs` generates the internal nodes for the layers. Internal nodes are generated from bottom to top.

### 3.4 Construction Algorithm

1. **Copy leaves** – With given array of entries (`Entry<K>`), create subset of entries which have unique keys. In this building process, we group the duplicate keys together and store the offsets of the duplicate values in the payload. `Entry<K>` stored in the tree has pointer to the payload so we can return multiple offsets in the query.
2. **Tree layout** – Calculate the layout of the tree. `level_bounds` is a vector of ranges. The start of each range is the offset of the first item in the layer. Branching factor and the number of items in the tree are used to calculate the layout. (No duplicate keys in the tree.)
3. **Build internal layers** – for each level (layer), generate the internal nodes from the previous level. (bottom up)
4. **Serialize** – the resulting contiguous `Vec<Entry<K>>` is our byte‑layout; writing it to an `io::Write` is zero‑copy.
5. **Deserialize** – the serialized byte array is deserialized into the `Vec<Entry<K>>` structure so the tree can be built.
6. **Query** – query the tree with the given key.

***Complexities***
• **Build**: `O(N)` time, `O(N)` space.
• **Search**: `O(log_{B} N)` node touches (each touch requires reading a single node from the underlying reader, not the whole tree).

## 4. Safety & Error Handling

* **No `unsafe`** is required; index math uses `usize` and is bounds‑checked in debug builds.
* All fallible functions use `crate::error::Result` (`std::result::Result<T, Error>`).  `Error::IoError` is propagated verbatim.

## 5. Future Work

1. **Prefetch hints** – explore prefetching for sequential range scans.
