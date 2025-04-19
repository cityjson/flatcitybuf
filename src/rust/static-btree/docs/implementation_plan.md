 # Static B+Tree (S+Tree) Implementation Plan

 **Project:** Implement the `static-btree` Rust Crate

 **Goal:** Create a Rust crate for a Static B+Tree (S+Tree) optimized for read performance.

 ## 1. Introduction

 This document outlines the implementation strategy and Rust API for a static, implicit B+Tree (S+Tree), emphasizing:
 - **Read‑only**: built once, queried many times
 - **Implicit Eytzinger layout**: pointer arithmetic for node addressing
 - **Fixed‑size index entries**: one per unique key
 - **Payload indirection**: handle duplicate record offsets via chained blocks

 ## 2. Core Concepts

 - **B**: branching factor (# keys per leaf)
 - **N**: number of unique keys
 - **H**: number of index layers (height)
 - **K**: key type implementing `Key`
 - **O**: record offset (`u64`)

 ## 3. Implementation Policy

 > **Revision (Secondary‑Index Indirection)**: keys are unique in the index region; each index entry `<K, block_ptr>` points to a chain of fixed‑size payload blocks that store one or more record offsets for key `K`.  This separates index structure from duplicate handling.

 ### 3.1 Terminology & Symbols

 | Symbol | Meaning                                 |
 |--------|-----------------------------------------|
 | B      | branching factor (max keys per leaf)    |
 | N      | total number of unique keys             |
 | H      | index height (layers)                   |
 | K      | key type (impl. `Key` trait)            |
 | M      | payload block capacity (max offsets)    |
 | O      | record offset (`u64`)                   |

 ### 3.2 Node Layout

 1. **Leaf index entries**: up to `B` entries per node, each is `Entry<K>{ key, block_ptr }` where `block_ptr` is a `u64` file offset into the payload region.
 2. **Internal index entries**: up to `B` keys per node (fan‑out = `B+1`), store only `key`; child indices computed arithmetically.
 3. **Index region**: contiguous layers (root→leaves) of fixed‑size `Entry<K>` records, densely packed and padded to multiples of `B` per layer.

 ### 3.3 Layer Offset Computation

 ```text
 blocks(n)    = ceil(n / B)             // nodes per layer
 prev_keys(n) = blocks(n) * B / (B+1) * B  // keys in parent layer
 height(n)    = 1 if n ≤ B
               = 1 + height(prev_keys(n)) otherwise
 offset(h)    = Σ_{i=0}^{h-1} blocks_i * B   // starting entry index of layer h
 ```

 Layers are numbered bottom‑up (0 = leaf, H‑1 = root).  `offset(h)` yields the base entry index for layer `h` in the index region.

 ### 3.4 Construction Algorithm (Index + Payload Blocks)

 1. **Group input**: from sorted `(K, O)` pairs (duplicates allowed) produce `Vec<(K, Vec<O>)>` of unique keys.
 2. **Emit payload blocks**:
    - Choose block capacity `M` (e.g. equals `B` or other tunable).
    - For each key’s offsets list, split into chunks of ≤ `M` offsets.
    - For each chunk, write a block:
      ```text
      u32 count       // # of valid offsets
      u64 next_ptr    // file offset of next block (0 if last)
      u64 offsets[M]  // record pointers
      ```
    - Chain blocks via `next_ptr`; record first block’s file offset as the key’s `block_ptr`.
 3. **Build index region**:
    - Create in-memory entries `Entry { key, block_ptr }` for each unique key.
    - Pack leaf entries (pad to multiple of `B`), then compute internal layers top-down (copy minimal child keys).
 4. **Serialize**: write index region entries (root→leaves) sequentially, then append all payload blocks.

 ### 3.5 Payload Block Format

 Payload region holds fixed‑size blocks:
 ```text
 u32 count
 u64 next_ptr
 u64 offsets[M]
 ```
 Follow `next_ptr` chains to collect all record offsets for a key.

 ### 3.6 Query Algorithm (with Block Indirection)

 To retrieve offsets for key `k`:
 1. **Index lookup**: compute `lower_bound_index(k)` or `upper_bound_index(k)` in O(log_B N) node touches.
 2. **Read entry**: `read_entry(idx)` → `(key, block_ptr)`.
 3. **Load payload**: call `read_all_offsets(block_ptr)`, following block chain and concatenating all `offsets`.

 ### 3.7 Secondary-Index Indirection (Duplicate Handling)

 Duplicate keys are normalized into payload chains.  The index layer remains strictly unique‑key, fixed‑size.

 ### 3.8 Query Operators

 Comparison operators combine index traversal with payload reads:
 - **Eq**: locate `k`, then read its payload chain.
 - **Ne**: gather payloads for keys `< k` and `> k` and union.
 - **Gt/Ge/Lt/Le**: determine index start/end, scan index entries, and flatten each key’s payload chain.

 Each operator costs O(log_B N) node touches plus payload block reads per matching key.

 ## 4. Public Rust API

 ```rust
 pub struct StaticBTree<K, R> { /* reader, layout, etc. */ }

 impl<K: Key, R: Read + Seek> StaticBTree<K, R> {
     pub fn new(reader: R, branching_factor: u16, num_entries: u64) -> Result<Self, Error>;
     pub fn height(&self) -> usize;
     pub fn len(&self) -> usize;
     pub fn lower_bound(&mut self, key: &K) -> Result<Vec<Offset>>;
     pub fn range(&mut self, min: &K, max: &K) -> Result<Vec<Offset>>;
     pub fn query(&mut self, cmp: Comparison, key: &K) -> Result<Vec<Offset>, Error>;
 }
 ```