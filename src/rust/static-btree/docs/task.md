## AI Task Prompt: Implement Rust Static B+Tree (S+Tree) Crate

**Goal:**

Implement a Rust crate for a Static B+Tree (S+Tree), optimized for read performance and efficient storage.

**Core Requirement:**

Your implementation **must** strictly follow the detailed plan, API signatures, data structures, algorithms, and serialization format outlined in the provided reference document: `policy_gemini.md`. Adherence to this plan is critical.

**Reference Document:**

*(Assume the full content of the `policy_gemini.md` artifact is provided alongside this prompt).*

**Key Implementation Tasks:**

1. **Implement Core Types & Traits:**
    * Define the `Error` enum exactly as specified for error handling.
    * Use the `Value = u64` type alias for entry values.
    * Implement the `trait Key` with its required methods (`SERIALIZED_SIZE`, `write_to`, `read_from`).
    * Implement the `struct Entry<K: Key, V: Value>` including its `Ord` implementation (based on key only) and serialization methods (`write_to`, `read_from`).

2. **Implement `Key` Trait for Standard Types:**
    * Provide `Key` implementations for standard integer types: `i32`, `u32`, `i64`, `u64`. Ensure correct fixed-size serialization (e.g., using `to_le_bytes`/`from_le_bytes`).
    * Provide `Key` implementations for float types: `f32`, `f64`. **Crucially, use the `ordered-float` crate** (or a similar mechanism) to ensure total ordering (handling NaN correctly) for reliable use in the B-Tree.
    * Implement the `struct FixedStringKey<const N: usize>` and its `Key` implementation as described in the plan, correctly handling fixed-size truncation and padding with null bytes. Include the `from_str` and `to_string_lossy` helper methods.

3. **Implement `StaticBTreeBuilder`:**
    * Implement the `new` constructor, handling branching factor validation and writing/reserving space for the header.
    * Implement the `build_from_sorted` method. This is the core construction logic and **must** follow the **bottom-up approach** detailed in the plan and the Mermaid diagram:
        * Accept an iterator yielding *sorted* `Result<Entry<K, Value>, Error>`. Include checks for sorted order if feasible.
        * Process entries to write fully packed leaf nodes to the `Write + Seek` target.
        * Collect the *first key* of each written leaf node.
        * Recursively build parent levels by writing fully packed internal nodes using the collected keys from the level below.
        * Handle the padding of the *last node* at each level to ensure all nodes (except potentially the very last one overall if padding isn't desired) occupy the full calculated node byte size.
        * Calculate final metadata (height, node counts per level).
        * Seek back to the beginning and write the final, complete header information.

4. **Implement `StaticBTree` Reader:**
    * Implement the `open` method: Read the header from the `Read + Seek` source, validate magic bytes/version, and calculate/store all layout parameters (`num_nodes_per_level`, `level_start_offsets`, node sizes, etc.).
    * Implement the `find` method using the tree traversal logic specified:
        * Navigate from the root using calculated node offsets and the Eytzinger formula.
        * Use `reader.seek` and `reader.read_exact` to load **only the bytes for the single node** being inspected at each level.
        * Deserialize keys (for internal nodes) or entries (for leaf nodes) from the read buffer.
        * Perform binary search within the deserialized node data.
    * Implement the `range` method:
        * Locate the starting leaf node and position for the `start_key`.
        * Return an iterator that reads subsequent leaf nodes *on demand* using `read_leaf_node_entries`.
        * The iterator should yield `Result<Entry<K, Value>, Error>` for entries within the `[start_key, end_key)` range. Pay close attention to iterator state management and borrowing the `reader`.
    * Implement the internal helper methods (`read_internal_node_keys`, `read_leaf_node_entries`, `calculate_node_offset`) to accurately reflect the logic needed for seeking, reading, deserializing nodes, and calculating offsets based on the defined layout.

5. **Adhere to Serialization Format:**
    * Ensure the `StaticBTreeBuilder` writes data precisely matching the Header + Nodes Data format described in the plan.
    * Ensure the `StaticBTree::open` and node reading methods correctly interpret this format.

6. **Testing:**
    * Provide comprehensive unit tests covering:
        * `Key` trait implementations (serialization/deserialization, comparison).
        * `Entry` struct (serialization/deserialization, comparison).
        * `StaticBTreeBuilder` logic (test building small trees with known structure).
        * `StaticBTree` reader logic (`open` validation, `find` for keys present/absent/at boundaries, `range` queries covering various scenarios like empty ranges, full ranges, partial ranges).
    * Include integration tests where a tree is built to an in-memory buffer (`std::io::Cursor`) and then read back using `StaticBTree::open` for verification.

7. **Code Quality:**
    * Write clear, well-commented, idiomatic Rust code.
    * Use `Result<T, Error>` extensively for error handling; avoid `panic!` for recoverable errors.
    * Ensure code is formatted using `rustfmt`.
    * Add documentation comments (`///`) explaining public APIs (structs, traits, methods).

**Deliverable:**

Provide the complete source code for the Rust crate (`static-btree`), organized into logical modules (e.g., `src/key.rs`, `src/entry.rs`, `src/builder.rs`, `src/tree.rs`, `src/error.rs`, `src/lib.rs`). Include a basic `Cargo.toml` file specifying dependencies (like `ordered-float`).

Once you have completed the each step, you should mark :check: in this document.
