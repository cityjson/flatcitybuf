#pragma once

#include <fcb/error.hpp>
#include <fcb/range_reader.hpp>
#include <fcb/span.hpp>

#include <cstdint>
#include <vector>

namespace fcb {

struct SearchResultItem;

/// A 2D query rectangle. The packed R-tree is 2D only: z is carried in the
/// feature data but never indexed, so 3D filtering is a post-step.
struct BBox {
    double min_x;
    double min_y;
    double max_x;
    double max_y;
};

/// One R-tree node entry: 4 doubles then a u64, all little-endian, 40 bytes
/// with no padding (packed_rtree/mod.rs:23-33).
///
/// `offset` means different things by level: for an INTERNAL node it is the
/// child node INDEX; for a LEAF it is a byte offset relative to the start of
/// the features section.
struct NodeItem {
    double min_x;
    double min_y;
    double max_x;
    double max_y;
    std::uint64_t offset;

    static constexpr std::size_t kSize = 40;

    static NodeItem decode(bytes_view b);

    /// The "empty" node used as the fold/aggregation identity: any real
    /// bbox's `expand` widens it. Mirrors `NodeItem::create`
    /// (packed_rtree/mod.rs:46-54); named `empty` here rather than `create`
    /// since it does not take a bbox, only the `offset` to tag it with.
    static NodeItem empty(std::uint64_t offset);

    /// Widens this node's bbox to also cover `r`, leaving `offset`
    /// untouched. Mirrors `NodeItem::expand` (packed_rtree/mod.rs:92-105).
    void expand(const NodeItem& r);

    /// Writes this node's 40 bytes (4 LE f64 then a LE u64), matching
    /// `decode`'s layout exactly. Mirrors `NodeItem::write`
    /// (packed_rtree/mod.rs:70-77).
    void encode(std::uint8_t* out) const;

    /// Mirrors NodeItem::intersects (packed_rtree/mod.rs:122-134), which
    /// uses strict < and >: touching edges DO intersect.
    bool intersects(const BBox& q) const;
};

/// Total node count in the tree, per the Rust level-bounds loop
/// (packed_rtree/mod.rs:342-375). Breaks at n == 1 -- note this differs
/// from the B+tree, which breaks at n < branching_factor.
std::uint64_t rtree_num_nodes(std::uint64_t num_items, std::uint16_t node_size);

/// Half-open [start, end) node index range for one tree level, in the flat
/// node array shared by every level. `rtree_level_bounds()[0]` is the LEAF
/// level; `.back()` is the root (a single node spanning the whole extent).
struct LevelBound {
    std::uint64_t start;
    std::uint64_t end;
};

/// Mirrors `generate_level_bounds` (packed_rtree/mod.rs:342-375). Shared by
/// the reader (`rtree_search_bbox`) and the writer's R-tree builder (M5) --
/// both need the exact same per-level array layout to agree on where a
/// child index or search cursor actually lands.
std::vector<LevelBound> rtree_level_bounds(std::uint64_t num_items, std::uint16_t node_size);

/// Breadth-first bbox search over the packed R-tree, reading nodes through
/// the supplied reader. Results are returned sorted by feature offset so the
/// caller reads forward through the file.
std::vector<SearchResultItem> rtree_search_bbox(RangeReader& reader, std::uint64_t index_begin,
                                                std::uint64_t num_items, std::uint16_t node_size,
                                                const BBox& query);

}  // namespace fcb
