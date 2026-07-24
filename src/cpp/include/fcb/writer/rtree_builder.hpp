#pragma once

#ifdef FCB_WITH_JSON

#    include <fcb/packed_rtree.hpp>

#    include <cstdint>
#    include <vector>

namespace fcb {

/// The Hilbert curve index of point (x, y) on a 65536x65536 grid (16 bits
/// per axis). A fixed bit-twiddling function with no branches or loops --
/// every operation is `^`/`&`/`|`/`<<`/`>>` on `uint32_t`, none of which can
/// overflow in the sense Rust's debug-mode overflow checks mean (those only
/// instrument `+`/`-`/`*`), so this is a direct transliteration of `hilbert`
/// (packed_rtree/mod.rs:236-289) with no wraparound-semantics concerns.
std::uint32_t hilbert(std::uint32_t x, std::uint32_t y);

/// A `NodeItem`'s Hilbert index: its bbox center, scaled into
/// `[0, hilbert_max]` against `extent`, fed through `hilbert`. Mirrors
/// `hilbert_bbox` (packed_rtree/mod.rs:291-298); `hilbert_max` is always
/// `(1 << 16) - 1` in practice (`HILBERT_MAX`), taken as a parameter only to
/// keep the two constants visible together at the call site, as in Rust.
std::uint32_t hilbert_bbox(const NodeItem& r, std::uint32_t hilbert_max, const NodeItem& extent);

/// Sorts `items` in place by descending Hilbert index (the item furthest
/// along the curve first) -- a STABLE sort, matching Rust's slice `sort_by`
/// guarantee. Mirrors `hilbert_sort` (packed_rtree/mod.rs:300-306).
void hilbert_sort(std::vector<NodeItem>& items, const NodeItem& extent);

/// The bbox union of every item, via repeated `NodeItem::expand` starting
/// from `NodeItem::empty(0)`. Mirrors `calc_extent`
/// (packed_rtree/mod.rs:308-313). `nodes` must be non-empty: the identity
/// element (+inf/+inf/-inf/-inf) is never a meaningful result on its own
/// and no caller here needs it (M7 only calls this when features exist).
NodeItem calc_extent(const std::vector<NodeItem>& nodes);

/// Builds the full flat packed-R-tree node array (leaves first in
/// `nodes`'s own order at the array's tail per `rtree_level_bounds`, then
/// every internal level above it, bottom-up) from already Hilbert-sorted
/// leaf nodes with their FINAL byte offsets already set. Mirrors
/// `PackedRTree::build`+`init`+`generate_nodes`
/// (packed_rtree/mod.rs:327-397,432-451). Throws `fcb::Error` for
/// `node_size < 2`, matching Rust's own `assert!(node_size >= 2, ...)` --
/// its subsequent `.clamp(2, 65535)` only narrows the upper bound (a no-op
/// for `u16`), it does not round an invalid low value up. `nodes` must be
/// non-empty.
std::vector<NodeItem> build_packed_rtree(const std::vector<NodeItem>& nodes, const NodeItem& extent,
                                         std::uint16_t node_size);

/// Serializes every node in `tree` (as returned by `build_packed_rtree`) in
/// array order, 40 bytes each. Mirrors `PackedRTree::stream_write`
/// (packed_rtree/mod.rs:900-906).
std::vector<std::uint8_t> encode_packed_rtree(const std::vector<NodeItem>& tree);

}  // namespace fcb

#endif  // FCB_WITH_JSON
