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
| `B`    | Branching factor (max keys/leaf) |
| `N`    | Total number of entries          |
| `H`    | Height of the tree               |
| `K`    | Key type implementing `Key`      |
| `O`    | Offset (`u64`) – value payload   |

### 3.2 Node Layout

1. **Leaf Nodes**
   * Store **up to `B` keys** followed by their offsets.
   * No explicit *next‑leaf* pointer is required – leaves are stored **contiguously**; the next leaf is `index + 1` except for the right‑most leaf.
2. **Internal Nodes**
   * Store **only keys** (fan‑out = `B + 1`).
   * Each key `i` equals the **smallest key in the `(i+1)`‑th child subtree** (standard B+ invariant).
   * **No pointers** are stored; child indices are derived arithmetically via layer offsets (`offset(layer)`).
3. **Layer Packing**
   * Layers are written **top‑down (root → leaves)** in both memory & on‑disk representation.
   * Each layer is a *dense block* of `Entry<K>` (internal levels hold sentinel `Offset::MAX` for their value field so we can reuse the `Entry` struct without generics).

### 3.3 Layer Offset Computation

```text
blocks(n)     = ceil(n / B)                  // # of full nodes in layer with n keys
prev_keys(n)  = blocks(n) * B / (B + 1) * B  // keys in parent layer
height(n)     = 1            if n ≤ B
              = 1 + height(prev_keys(n))     otherwise
offset(h)     = Σ_{i=0}^{h-1} blocks_i * B   // starting index of layer h (root = 0)
```

These helper functions are implemented as `const fn` so the compiler can fold them at build‑time whenever `N` & `B` are const generics.

### 3.4 Construction Algorithm

1. **Copy leaves** – write the sorted `(key, offset)` pairs into layer `H‑1` (leaf layer) and pad the remainder with *∞‑sentinel* entries.
2. **Build internal layers** – for each higher layer `h = H‑2 … 0` compute each key by descending one step to the *right* then `h` steps *left* until a leaf is reached; copy the first key encountered.
3. **Serialize** – the resulting contiguous `Vec<Entry<K>>` is our byte‑layout; writing it to an `io::Write` is zero‑copy.

***Complexities***
• **Build**: `O(N)` time, `O(N)` space.
• **Search**: `O(log_{B} N)` node touches (each touch requires reading a single node from the underlying reader, not the whole tree).

### 3.5 Query Algorithm (Loop‑Based)

```
lower_bound(k):
    idx  ← 0            // logical position inside layer (already ×B)
    for h in 0 .. H‑1:  // root → layer before leaves
        node_start ← offset(h) + idx
        pos ← FIRST i in [0,B) where entries[node_start+i].key ≥ k  // linear scan
        idx ← idx * (B + 1) + pos * B

    leaf_start ← offset(H‑1) + idx
    pos ← FIRST i in [0,B) where entries[leaf_start+i].key ≥ k

    result_idx ← leaf_start + pos

    // gather duplicates
    dup_left  ← result_idx
    while dup_left  > leaf_start and entries[dup_left‑1].key == k: dup_left  -= 1

    dup_right ← result_idx
    while dup_right < leaf_start + B and entries[dup_right].key == k: dup_right += 1
    if dup_right == leaf_start + B and pos == B:  // reached node end, need to read neighbour
        load next leaf and continue while key == k

    return slice(entries[dup_left .. dup_right])
```

The algorithm touches **`log_B N`** internal nodes and at most **two** leaves when duplicates span node boundaries.  Each node is read lazily from the `Read+Seek` source right before inspection.

### 3.6 Range Scan

Range query retrieves **all offsets whose key ∈ [min, max]** by computing **both** bounds first and then streaming the closed interval between them:

1. `start_idx = lower_bound(min)` — returns the index of the first key ≥ `min` (and gathers duplicates on its left).
2. `end_idx   = upper_bound(max)` — returns the index of the *first* key `> max` (exclusive upper bound).
3. Stream‑read sequential nodes between `start_idx` (inclusive) and `end_idx` (exclusive), yielding every `(key, offset)` pair.  Because the bounds are known up‑front, we avoid redundant key comparisons for very large ranges.

Complexity: **`log_B N`** node touches to locate each bound, plus **`⌈((end_idx‑start_idx)/B)⌉`** leaf reads for the body of the range. This scales well even when the range spans millions of entries.

> Implementation note — `upper_bound` is identical to `lower_bound` except that it selects the first key strictly greater than the query key.

### 3.7 Duplicate Keys

The structure **allows duplicate keys**.  `lower_bound` returns **all** offsets whose key equals the search key, even if they reside in neighbouring nodes.  Range queries naturally respect duplicates as they iterate through contiguous leaves.

### 3.8 Streaming Reads

All search routines keep **only a small, fixed number of nodes in memory**.  When descending, a node is fetched via `reader.seek()` + `reader.read_exact()` into a scratch buffer.  The maximum resident memory during search is therefore `(H + 2) × B × size_of<Entry<K>>`, typically a few kilobytes even for large trees.

### 3.9 Query Operators
To support a richer query API, the static B+Tree will expose all common comparison operators on keys:
  * **Eq**: exact match ⇒ all offsets where `key == target`.
  * **Ne**: not equal ⇒ all offsets where `key != target`.
  * **Gt**: greater than ⇒ all offsets where `key > target`.
  * **Ge**: greater or equal ⇒ all offsets where `key >= target`.
  * **Lt**: less than ⇒ all offsets where `key < target`.
  * **Le**: less or equal ⇒ all offsets where `key <= target`.

#### Design
1. Create `query.rs` defining:
   ```rust
   #[derive(Copy, Clone, Debug)]
   pub enum Comparison { Eq, Ne, Gt, Ge, Lt, Le }
   ```
2. In `StaticBTree<K, R>` add:
   - `fn query(&mut self, cmp: Comparison, key: &K) -> Result<Vec<Offset>>`
   - Convenience methods: `find_eq`, `find_ne`, `find_gt`, `find_ge`, `find_lt`, `find_le`.
3. Implementation Strategy:
   * **Eq** → call `lower_bound`, return matching offsets or empty vec.
   * **Ne** → combine results of `<` and `>` scans.
   * **Gt/Ge/Lt/Le** → use `lower_bound_index`/`upper_bound_index` to compute start/end, then scan leaf layer entries via `read_entry`.
4. Each operator incurs at most `O(log_B N)` node reads plus a sequential leaf scan.

## 4. Public Rust API

```rust
/// Read‑only Static B+Tree backed by a reader.
///
/// Notes:
///   * Internal nodes expose their value field as `Offset::MAX` and should never be read by user code.
///   * All logging is lowercase (see project rules).
use crate::error::Error;
type Result<T> = std::result::Result<T, Error>;
use crate::entry::{Entry, Offset};
use crate::key::Key;
use std::io::{Read, Seek};

pub struct StaticBTree<K, R> {
    reader: R,
    branching_factor: u16,
    num_entries: u64,
}

impl<K: Key, R: Read + Seek> StaticBTree<K, R> {
    /// Create from an existing reader (e.g. mmap, file, http‑range buffer).
    pub fn new(reader: R, branching_factor: u16, num_entries: u64) -> Result<Self>;

    /// Tree height (`1` == single leaf/root).
    pub fn height(&self) -> usize;

    /// Return the offset associated with the **first** key not less than `key`.
    pub fn lower_bound(&mut self, key: &K) -> Result<Vec<Offset>>;

    /// Collect offsets of keys in `[min, max]` (inclusive). `limit == None` ⇒ unlimited.
    pub fn range(&mut self, min: &K, max: &K, limit: Option<usize>) -> Result<Vec<Offset>>;

    /// Future: stream query over HTTP Range Requests (patterned after packed_rtree::http_stream_search).
    /// `combine_request_threshold` is identical semantics to the r‑tree version.
    #[cfg(feature = "http")]
    async fn http_stream_query<T: AsyncHttpRangeClient>(
        client: &mut AsyncBufferedHttpRangeClient<T>,
        index_begin: usize,
        attr_index_size: usize,
        num_items: usize,
        min: &K,
        max: &K,
        combine_request_threshold: usize,
    ) -> Result<Vec<Offset>>;
}

/// Builder that constructs a serialized tree in‑memory.
pub struct StaticBTreeBuilder<K> {
    branching_factor: u16,
    entries: Vec<Entry<K>>, // sorted input grows here
}

impl<K: Key> StaticBTreeBuilder<K> {
    pub fn new(branching_factor: u16) -> Self;
    pub fn push(&mut self, key: K, offset: Offset);
    /// Finalize and obtain the serialized byte vector (ready to be written to disk).
    pub fn build(self) -> Result<Vec<u8>>;
}
```

All methods are `#[inline]` where micro‑benchmarks prove beneficial. `tokio::task::spawn_blocking` wrappers will be provided for async contexts.

## 5. Safety & Error Handling

* **No `unsafe`** is required; index math uses `usize` and is bounds‑checked in debug builds.
* All fallible functions use `crate::error::Result` (`std::result::Result<T, Error>`).  `Error::IoError` is propagated verbatim.

## 6. Future Work

1. **Non‑uniform fan‑out** – adapt root node size to minimise wasted space on small datasets.
2. **Prefetch hints** – explore OS/BIO pre‑read for sequential range scans.
3. **Async builder** – offload construction to `tokio::task::spawn_blocking` for large datasets.
