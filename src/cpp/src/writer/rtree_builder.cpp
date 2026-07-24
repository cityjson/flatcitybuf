#include <fcb/error.hpp>
#include <fcb/writer/rtree_builder.hpp>

#include <algorithm>
#include <cmath>
#include <limits>
#include <string>

namespace fcb {

namespace {
constexpr std::uint32_t kHilbertMax = (1u << 16) - 1;
}  // namespace

// Based on public domain code at https://github.com/rawrunprotected/hilbert_curves,
// ported directly from `hilbert` (packed_rtree/mod.rs:236-289). Every
// operation is bitwise (^, &, |, <<, >>) on uint32_t -- never +, -, or *, so
// there is no Rust-debug-overflow-check behavior to replicate: C++'s
// well-defined unsigned wraparound already matches Rust's release-mode
// `u32` arithmetic here bit-for-bit regardless.
std::uint32_t hilbert(std::uint32_t x, std::uint32_t y) {
    std::uint32_t a = x ^ y;
    std::uint32_t b = 0xFFFF ^ a;
    std::uint32_t c = 0xFFFF ^ (x | y);
    std::uint32_t d = x & (y ^ 0xFFFF);

    std::uint32_t aa = a | (b >> 1);
    std::uint32_t bb = (a >> 1) ^ a;
    std::uint32_t cc = ((c >> 1) ^ (b & (d >> 1))) ^ c;
    std::uint32_t dd = ((a & (c >> 1)) ^ (d >> 1)) ^ d;

    a = aa;
    b = bb;
    c = cc;
    d = dd;
    aa = (a & (a >> 2)) ^ (b & (b >> 2));
    bb = (a & (b >> 2)) ^ (b & ((a ^ b) >> 2));
    cc ^= (a & (c >> 2)) ^ (b & (d >> 2));
    dd ^= (b & (c >> 2)) ^ ((a ^ b) & (d >> 2));

    a = aa;
    b = bb;
    c = cc;
    d = dd;
    aa = (a & (a >> 4)) ^ (b & (b >> 4));
    bb = (a & (b >> 4)) ^ (b & ((a ^ b) >> 4));
    cc ^= (a & (c >> 4)) ^ (b & (d >> 4));
    dd ^= (b & (c >> 4)) ^ ((a ^ b) & (d >> 4));

    a = aa;
    b = bb;
    c = cc;
    d = dd;
    cc ^= (a & (c >> 8)) ^ (b & (d >> 8));
    dd ^= (b & (c >> 8)) ^ ((a ^ b) & (d >> 8));

    a = cc ^ (cc >> 1);
    b = dd ^ (dd >> 1);

    std::uint32_t i0 = x ^ y;
    std::uint32_t i1 = b | (0xFFFF ^ (i0 | a));

    i0 = (i0 | (i0 << 8)) & 0x00FF00FF;
    i0 = (i0 | (i0 << 4)) & 0x0F0F0F0F;
    i0 = (i0 | (i0 << 2)) & 0x33333333;
    i0 = (i0 | (i0 << 1)) & 0x55555555;

    i1 = (i1 | (i1 << 8)) & 0x00FF00FF;
    i1 = (i1 | (i1 << 4)) & 0x0F0F0F0F;
    i1 = (i1 | (i1 << 2)) & 0x33333333;
    i1 = (i1 | (i1 << 1)) & 0x55555555;

    return (i1 << 1) | i0;
}

namespace {
/// Rust's `f64 as u32` saturates: NaN and values <= 0 become 0, values >=
/// `u32::MAX` become `u32::MAX`, everything else truncates toward zero
/// (`f64::floor` was already applied by the caller, so truncation and floor
/// agree here). `static_cast<uint32_t>` from a negative or out-of-range
/// `double` is undefined behavior in C++ (before C++23, and
/// implementation-defined rather than saturating even from C++23 on), so
/// this saturating cast is the correct, defined replacement -- not just a
/// style choice.
std::uint32_t saturating_f64_to_u32(double v) {
    if (!(v > 0.0))  // catches v <= 0.0 and NaN (NaN compares false either way)
        return 0;
    if (v >= static_cast<double>(std::numeric_limits<std::uint32_t>::max()))
        return std::numeric_limits<std::uint32_t>::max();
    return static_cast<std::uint32_t>(v);
}
}  // namespace

std::uint32_t hilbert_bbox(const NodeItem& r, std::uint32_t hilbert_max, const NodeItem& extent) {
    const double x = std::floor(hilbert_max * ((r.min_x + r.max_x) / 2.0 - extent.min_x) /
                                (extent.max_x - extent.min_x));
    const double y = std::floor(hilbert_max * ((r.min_y + r.max_y) / 2.0 - extent.min_y) /
                                (extent.max_y - extent.min_y));
    return hilbert(saturating_f64_to_u32(x), saturating_f64_to_u32(y));
}

void hilbert_sort(std::vector<NodeItem>& items, const NodeItem& extent) {
    std::stable_sort(items.begin(), items.end(), [&extent](const NodeItem& a, const NodeItem& b) {
        const std::uint32_t ha = hilbert_bbox(a, kHilbertMax, extent);
        const std::uint32_t hb = hilbert_bbox(b, kHilbertMax, extent);
        return hb < ha;  // descending: higher Hilbert index sorts first
    });
}

NodeItem calc_extent(const std::vector<NodeItem>& nodes) {
    NodeItem extent = NodeItem::empty(0);
    for (const auto& n : nodes)
        extent.expand(n);
    return extent;
}

std::vector<NodeItem> build_packed_rtree(const std::vector<NodeItem>& nodes, const NodeItem& extent,
                                         std::uint16_t node_size) {
    // Rust's `init()` ASSERTS `node_size >= 2` (a hard panic) before its own
    // `.clamp(2, 65535)` -- that clamp only ever narrows the upper bound
    // (65535 is already `u16::MAX`, so it's a no-op in practice); a caller
    // passing 0 or 1 is a programming error there, not a value to silently
    // round up. Silently clamping up here instead (as this milestone
    // originally did) would build a real branching-factor-2 tree while
    // whatever recorded `index_node_size` the caller intended stays 0 or 1,
    // an index/header disagreement invisible until a reader's own layout
    // math (which trusts the recorded node size) disagrees with these
    // bytes. Found during the M5 codex review.
    if (node_size < 2) {
        throw Error(ErrorCode::IllegalHeaderSize,
                    "rtree node_size must be >= 2, got " + std::to_string(node_size));
    }
    const std::uint16_t branching_factor = node_size;
    const auto level_bounds = rtree_level_bounds(nodes.size(), branching_factor);
    // `level_bounds[0]` is the LEAF level; its `.end` equals the total node
    // count, since the leaf level occupies the array's tail (mirrors Rust's
    // own `init()`: `let num_nodes = self.level_bounds.first().expect(...).end;`).
    const std::uint64_t num_nodes = level_bounds.front().end;

    std::vector<NodeItem> tree(static_cast<std::size_t>(num_nodes), NodeItem::empty(0));

    // Leaf level is stored at the array's TAIL (level_bounds[0], matching
    // Rust's `tree.node_items[num_nodes - num_leaf_nodes + i] = node`).
    const std::uint64_t leaf_start = num_nodes - nodes.size();
    for (std::size_t i = 0; i < nodes.size(); ++i)
        tree[static_cast<std::size_t>(leaf_start) + i] = nodes[i];

    // Bottom-up: every level's parent is the bbox union of its
    // `branching_factor` children, tagged with the index of its FIRST
    // child. Mirrors `generate_nodes` (packed_rtree/mod.rs:377-397).
    for (std::size_t level = 0; level + 1 < level_bounds.size(); ++level) {
        const LevelBound& children_level = level_bounds[level];
        const LevelBound& parent_level = level_bounds[level + 1];

        std::uint64_t parent_idx = parent_level.start;
        std::uint64_t child_idx = children_level.start;
        while (child_idx < children_level.end) {
            NodeItem parent_node = NodeItem::empty(child_idx);
            for (std::uint16_t j = 0; j < branching_factor; ++j) {
                if (child_idx >= children_level.end)
                    break;
                parent_node.expand(tree[static_cast<std::size_t>(child_idx)]);
                ++child_idx;
            }
            tree[static_cast<std::size_t>(parent_idx)] = parent_node;
            ++parent_idx;
        }
    }

    return tree;
}

std::vector<std::uint8_t> encode_packed_rtree(const std::vector<NodeItem>& tree) {
    std::vector<std::uint8_t> out(tree.size() * NodeItem::kSize);
    for (std::size_t i = 0; i < tree.size(); ++i)
        tree[i].encode(out.data() + i * NodeItem::kSize);
    return out;
}

}  // namespace fcb
