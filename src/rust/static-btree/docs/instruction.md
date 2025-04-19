 # Static B+Tree Policy Change & Implementation Instructions

 ## Background
 The original Static B+Tree implementation allowed duplicate keys by storing each duplicate inline in leaf nodes and performing per-node duplicate scans during `lower_bound` and range queries. While this supported duplicates, it introduced variable-length logic into the core tree layout and required extra scanning across node boundaries, complicating both the builder and the query algorithms.

 ## Rationale for Change
 We want to simplify the tree index so that all in-tree entries are fixed-size and uniquely keyed, enabling:
 1. Purely arithmetic, pointer-free node layouts (constant-time address computation).
 2. O(log_B N) node touches without per-leaf duplicate loops.
 3. Cleaner builder logic (one entry per unique key) and clearer separation of concerns.

 This approach directly corresponds to the classic “secondary‐index indirection” (Textbook Option 3): each index entry `<K, P>` points to a block (or chain of blocks) that holds the actual record pointers for key K. It trades one extra block‐level indirection per key for simpler, fixed-size index structures.

 ## New Two-Region Layout
 1. **Index Region** (implicit Eytzinger layout, fixed-size):
    - Entry: `{ key: K, block_ptr: u64 }`
    - One entry per unique key, packed into leaf nodes then internal layers as before.
    - Fast binary search / B+Tree traversal touches only fixed-size nodes.

 2. **Payload Region** (chained blocks, fixed-size blocks of offsets):
    - Each block contains:
      ```text
      u32 count       // number of valid offsets in this block
      u64 next_ptr    // file offset of next block (0 if last)
      u64 offsets[M]  // record pointers
      ```
    - For each key, its `block_ptr` names the first block in a chain that contains *all* record offsets for that key.

## Implementation Steps
 1. **Refactor `entry.rs` & `builder.rs`**
    - Change `Entry` to `(K, block_ptr: u64)`.
    - In the builder, group duplicate keys and emit chained payload blocks (with capacity `M`) before constructing the index entries.
    - Serialize index region first, then payload region.
 2. **Extend `tree.rs`**
    - Add `read_block(ptr) -> (Vec<Offset>, next_ptr)`.
    - Add `read_all_offsets(ptr) -> Vec<Offset>` that follows the chain.
    - Update `read_entry` to return `(key, block_ptr)`.
 3. **Update `query.rs`**
    - Drop inline duplicate scanning across leaf nodes.
    - For each operator, locate index entries, then call `read_all_offsets(block_ptr)` to retrieve final offsets.
 4. **Testing & Debugging**
    - Unit tests for raw index reads (`lower_bound_index` + `read_entry`).
    - Tests for single‐block and multi‐block payload reads.
    - Operator tests (`find_eq`, `find_ne`, etc.) over varied duplicate scenarios.
 5. **Documentation**
    - Incorporate this policy into `implementation_plan.md`.
    - Update `progress.md` with current status and new milestones.
    - Use this `instruction.md` to onboard collaborators.

## References
 - Algorithmica S+Tree article: https://en.algorithmica.org/hpc/data-structures/s-tree/
 - Database Systems textbook, Option 3 secondary-index indirection.