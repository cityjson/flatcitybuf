#pragma once

#ifdef FCB_WITH_JSON

#    include <fcb/key.hpp>
#    include <fcb/stree.hpp>

#    include <cstdint>
#    include <vector>

namespace fcb {

/// One (key, feature byte offset) pair to be indexed. Several entries may
/// share the same key (a column value repeated across features); the
/// builder groups those into one payload entry rather than one leaf slot
/// each. Mirrors `Entry<K>`/`NodeItem<K>` as used as BUILD input in
/// `Stree::build` (stree.rs:638).
struct BtreeEntry {
    KeyValue key;
    std::uint64_t offset;
};

/// The finished index: the flat node array concatenated with the payload
/// section (mirrors `Stree::stream_write`, stree.rs:1575-1589), plus the
/// two header fields (`AttributeIndexInfo.branching_factor`/
/// `.num_unique_items`, M4's `header_serializer.hpp`) that only the builder
/// itself can compute -- `branching_factor` here is just the caller's own
/// value echoed back (kept alongside the bytes so a caller building several
/// columns' indices doesn't have to separately remember what it asked for
/// each one), and `num_unique_items` counts distinct KEYS, not input
/// entries.
struct BuiltBtreeIndex {
    std::vector<std::uint8_t> bytes;
    std::uint16_t branching_factor;
    std::uint32_t num_unique_items;
};

/// Builds one column's complete attribute index blob from its
/// `(key, offset)` entries. Every entry's `key.kind()` must be the same
/// `kind`. Throws `fcb::Error` for `branching_factor < 2` or an empty
/// `entries` (mirrors `Stree::init`'s `Err(Error::InvalidFormat(...))`
/// checks, stree.rs:451-460 -- a library error, not Rust's `assert!`,
/// since an attribute with zero indexable values is a normal condition
/// this writer's caller (M7) is expected to skip before ever calling this,
/// not a programmer bug to panic on).
BuiltBtreeIndex build_static_btree(const std::vector<BtreeEntry>& entries, KeyKind kind,
                                   std::uint16_t branching_factor);

}  // namespace fcb

#endif  // FCB_WITH_JSON
