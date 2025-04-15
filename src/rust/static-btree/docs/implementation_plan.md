# Static B+Tree (S+Tree) Implementation Plan

**Project:** Implement the `static-btree` Rust Crate

**Goal:** Create a Rust crate for a Static B+Tree (S+Tree) optimized for read performance, strictly adhering to the specifications in `policy_gemini.md` and `task.md`, within the existing crate structure at `src/rust/static-btree/`.

**Plan:**

1. **Step 1: Core Types**
    * **Actions:**
        * Define modules within `src/lib.rs` (e.g., `mod error;`, `mod key;`, `mod entry;`).
        * Define the `Error` enum in `src/error.rs` exactly as specified in `policy_gemini.md`. Implement `From<io::Error>` and derive `Debug`. Consider implementing `std::error::Error`.
        * Define the `Value` type alias (`pub type Value = u64;`) likely in `src/lib.rs` or a new `src/types.rs`. Define `VALUE_SIZE` constant.
        * Define the `trait Key` in `src/key.rs` with `SERIALIZED_SIZE`, `write_to`, and `read_from` methods. Ensure trait bounds (`Sized + Ord + Clone + Debug`).
        * Implement the `struct Entry<K: Key, V: Value>` in `src/entry.rs`. Include `key` and `value` fields, `SERIALIZED_SIZE` constant, `write_to`, `read_from` methods, and derive necessary traits (`Debug`, `Clone`, `PartialEq`, `Eq`). Crucially, implement `PartialOrd` and `Ord` based *only* on the key.
        * Add initial unit tests in `src/entry.rs` (or `tests/entry_tests.rs`) covering `Entry` serialization/deserialization and comparison logic.
    * **Review Point 1:** Review the basic module structure, the definitions of `Error`, `Value`, `Key`, `Entry`, and the initial unit tests for `Entry`.

2. **Step 2: `Key` Trait Implementations**
    * **Actions:**
        * Implement `Key` for standard integer types (`i32`, `u32`, `i64`, `u64`) in `src/key.rs` (or a new `src/impls.rs`), using little-endian byte conversion (`to_le_bytes`/`from_le_bytes`).
        * Add the `ordered-float` crate as a dependency in `Cargo.toml`.
        * Implement `Key` for `f32` and `f64` using `OrderedFloat` from the `ordered-float` crate to ensure total ordering.
        * Implement the `struct FixedStringKey<const N: usize>` and its `Key` implementation (fixed-size byte array, handling padding/truncation). Include the `from_str` and `to_string_lossy` helper methods as described.
        * Add comprehensive unit tests in `src/key.rs` (or `tests/key_tests.rs`) for *all* `Key` implementations, covering serialization, deserialization, comparison logic, and the specific behavior of `FixedStringKey`.
    * **Review Point 2:** Review the `Key` implementations for built-in types and `FixedStringKey`, along with their unit tests.

3. **Step 3: `StaticBTreeBuilder` Implementation**
    * **Actions:**
        * Create `src/builder.rs` and declare the module in `src/lib.rs`.
        * Define the `StaticBTreeBuilder<K: Key, W: Write + Seek>` struct with fields for `writer`, `branching_factor`, `num_entries`, and necessary internal state buffers (`promoted_keys_buffer`, `current_node_buffer`, `items_in_current_node`, `current_offset`, `first_keys_of_current_level`, `nodes_per_level_build`).
        * Implement `StaticBTreeBuilder::new`, handling branching factor validation (`> 1`) and writing/reserving space for the header.
        * Implement the main `StaticBTreeBuilder::build_from_sorted` method. This is the core logic and must strictly follow the **bottom-up approach** detailed in `policy_gemini.md` and the Mermaid diagram:
            * Iterate through sorted input `Result<Entry<K, Value>, Error>`.
            * Write fully packed leaf nodes, padding the last one if necessary.
            * Collect the first key of each written leaf node.
            * Recursively build parent levels using the collected keys, writing packed internal nodes (padding the last one per level).
            * Track node counts per level (`nodes_per_level_build`).
        * Implement the finalization logic: calculate height, seek back to the start, and write the complete header (magic bytes, version, metadata).
        * Add unit tests in `src/builder.rs` (or `tests/builder_tests.rs`) focusing on the builder logic. Test building small trees with known inputs and expected outputs (e.g., byte structure, header values).
    * **Review Point 3:** Review the complete `StaticBTreeBuilder` implementation, focusing on the correctness of the bottom-up build algorithm and the header writing, along with its unit tests.

4. **Step 4: `StaticBTree` Reader Implementation**
    * **Actions:**
        * Create `src/tree.rs` and declare the module in `src/lib.rs`.
        * Define the `StaticBTree<K: Key, R: Read + Seek>` struct with fields for `reader`, metadata (`branching_factor`, `num_entries`, `height`, `header_size`), calculated sizes (`key_size`, `value_size`, node sizes), layout info (`num_nodes_per_level`, `level_start_offsets`), and `PhantomData`.
        * Implement `StaticBTree::open`: read the header, validate magic bytes/version, read metadata, calculate node sizes, and crucially, calculate the Eytzinger layout parameters (`num_nodes_per_level`, `level_start_offsets`) based on the header info and branching factor.
        * Implement internal helper methods:
            * `calculate_node_offset(node_index)`: Determines the byte offset of a node using the Eytzinger layout and level offsets.
            * `read_internal_node_keys(node_index)`: Seeks, reads the correct number of bytes for an internal node, and deserializes the keys. Handle potentially partially filled nodes if applicable (though S+Tree often assumes full).
            * `read_leaf_node_entries(node_index)`: Seeks, reads bytes for a leaf node, and deserializes entries. Handle potentially partially filled last leaf node based on `num_entries`.
        * Implement `StaticBTree::find`: Traverse the tree from the root using `read_internal_node_keys` and binary search, calculate child indices using Eytzinger logic, and finally use `read_leaf_node_entries` and binary search on the target leaf.
        * Implement `StaticBTree::range`: Locate the start leaf/position, then return an iterator struct (needs to be defined) that holds reader state and fetches subsequent leaf nodes (`read_leaf_node_entries`) on demand, yielding `Result<Entry<K, Value>, Error>` within the range. Pay close attention to iterator state, borrowing, and range boundary checks (`>= start_key`, `< end_key`).
        * Implement accessor methods (`branching_factor`, `len`, `is_empty`, `height`).
        * Add unit tests in `src/tree.rs` (or `tests/tree_tests.rs`) covering `open` validation, `find` (key present, absent, boundaries), and `range` (empty, full, partial ranges, edge cases).
    * **Review Point 4:** Review the `StaticBTree` reader implementation, including header parsing, layout calculation, node reading helpers, `find`, `range`, and associated unit tests.

5. **Step 5: Integration Testing & Finalization**
    * **Actions:**
        * Create integration tests (e.g., in `tests/integration_tests.rs`).
        * In these tests, use `StaticBTreeBuilder` to build trees with various key types (`i32`, `FixedStringKey`, etc.) and data distributions into an in-memory buffer (`std::io::Cursor`).
        * Use `StaticBTree::open` to read the buffer back and perform extensive `find` and `range` queries, asserting correctness against the known input data. This step implicitly verifies the serialization format.
        * Review all public APIs and add comprehensive documentation comments (`///`).
        * Ensure all code is formatted using `rustfmt`.
        * Add explanatory comments for complex sections of the code.
        * Go back through `task.md` and add `:check:` markers for completed items (or we can do this collaboratively).
    * **Review Point 5:** Final review of the entire crate, integration tests, documentation, code quality, and readiness.
