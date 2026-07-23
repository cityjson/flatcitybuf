#include <fcb/packed_rtree.hpp>
#include <fcb/reader.hpp>

#include <algorithm>
#include <cstring>
#include <deque>
#include <utility>

#include "detail/checked.hpp"

namespace fcb {

namespace {

double read_f64_le(const std::uint8_t* p) {
    // memcpy rather than a reinterpret_cast load: node items are packed with
    // no padding and land at arbitrary alignments inside a fetched block.
    double d;
    std::memcpy(&d, p, sizeof(double));
    return d;
}

std::uint64_t read_u64_le(const std::uint8_t* p) {
    std::uint64_t v;
    std::memcpy(&v, p, sizeof(std::uint64_t));
    return v;
}

/// Half-open [start, end) node index range for one tree level.
struct LevelBound {
    std::uint64_t start;
    std::uint64_t end;
};

/// Mirrors generate_level_bounds (packed_rtree/mod.rs:342-375).
/// level_bounds[0] is the LEAF level and is stored LAST; back() is the root.
std::vector<LevelBound> generate_level_bounds(std::uint64_t num_items, std::uint16_t node_size) {
    if (node_size < 2) {
        throw Error(ErrorCode::IllegalHeaderSize, "invalid index_node_size");
    }
    if (num_items == 0) {
        throw Error(ErrorCode::NoIndex, "empty rtree");
    }

    std::vector<std::uint64_t> level_num_nodes;
    std::uint64_t n = num_items;
    std::uint64_t num_nodes = n;
    level_num_nodes.push_back(n);
    for (;;) {
        n = detail::ceil_div(n, node_size);
        num_nodes = detail::checked_add(num_nodes, n, "rtree num_nodes");
        level_num_nodes.push_back(n);
        if (n == 1)
            break;
    }

    // Walk backwards accumulating offsets, as the Rust version does.
    std::vector<std::uint64_t> level_offsets;
    std::uint64_t acc = num_nodes;
    for (std::uint64_t size : level_num_nodes) {
        acc -= size;
        level_offsets.push_back(acc);
    }

    std::vector<LevelBound> bounds;
    bounds.reserve(level_num_nodes.size());
    for (std::size_t i = 0; i < level_num_nodes.size(); ++i) {
        bounds.push_back(LevelBound{level_offsets[i], level_offsets[i] + level_num_nodes[i]});
    }
    return bounds;
}

}  // namespace

NodeItem NodeItem::decode(bytes_view b) {
    if (b.size() < kSize) {
        throw Error(ErrorCode::NoIndex, "short rtree node item");
    }
    NodeItem n{};
    n.min_x = read_f64_le(b.data() + 0);
    n.min_y = read_f64_le(b.data() + 8);
    n.max_x = read_f64_le(b.data() + 16);
    n.max_y = read_f64_le(b.data() + 24);
    n.offset = read_u64_le(b.data() + 32);
    return n;
}

bool NodeItem::intersects(const BBox& q) const {
    // Strict comparisons, matching packed_rtree/mod.rs:122-134.
    if (q.max_x < min_x)
        return false;
    if (q.max_y < min_y)
        return false;
    if (q.min_x > max_x)
        return false;
    if (q.min_y > max_y)
        return false;
    return true;
}

std::uint64_t rtree_num_nodes(std::uint64_t num_items, std::uint16_t node_size) {
    if (node_size < 2) {
        throw Error(ErrorCode::IllegalHeaderSize, "invalid index_node_size");
    }
    if (num_items == 0)
        return 0;

    std::uint64_t n = num_items;
    std::uint64_t num_nodes = n;
    for (;;) {
        n = detail::ceil_div(n, node_size);
        num_nodes = detail::checked_add(num_nodes, n, "rtree num_nodes");
        if (n == 1)
            break;
    }
    return num_nodes;
}

std::vector<SearchResultItem> rtree_search_bbox(RangeReader& reader, std::uint64_t index_begin,
                                                std::uint64_t num_items, std::uint16_t node_size,
                                                const BBox& query) {
    std::vector<SearchResultItem> results;
    if (num_items == 0)
        return results;

    const auto level_bounds = generate_level_bounds(num_items, node_size);
    const std::uint64_t num_nodes = rtree_num_nodes(num_items, node_size);
    const std::uint64_t leaf_nodes_offset = level_bounds.front().start;

    // Breadth-first, so node reads run roughly in file order.
    std::deque<std::pair<std::uint64_t, std::size_t>> queue;
    queue.emplace_back(0, level_bounds.size() - 1);

    while (!queue.empty()) {
        const auto [node_index, level] = queue.front();
        queue.pop_front();

        if (level >= level_bounds.size()) {
            throw Error(ErrorCode::NoIndex, "rtree level out of range");
        }
        // Child indices come from the file and are hostile. Prove the node
        // lies within the level we believe we are on BEFORE using it, and
        // derive leaf-ness from the trusted level rather than from the
        // index itself.
        if (node_index < level_bounds[level].start || node_index >= level_bounds[level].end) {
            throw Error(ErrorCode::NoIndex, "rtree node index outside its level");
        }
        const bool is_leaf = (level == 0);
        const std::uint64_t end = std::min<std::uint64_t>(
            detail::checked_add(node_index, node_size, "rtree node end"), level_bounds[level].end);
        if (end <= node_index)
            continue;

        const std::uint64_t length = end - node_index;
        const std::uint64_t byte_offset = detail::checked_add(
            index_begin, detail::checked_mul(node_index, NodeItem::kSize, "rtree node offset"),
            "rtree node base");
        const std::uint64_t byte_len =
            detail::checked_mul(length, NodeItem::kSize, "rtree node span");

        auto block = reader.read(byte_offset, byte_len);
        if (block.size() < byte_len) {
            throw Error(ErrorCode::NoIndex, "truncated rtree node block");
        }

        for (std::uint64_t pos = node_index; pos < end; ++pos) {
            const std::uint64_t slot = pos - node_index;
            NodeItem item = NodeItem::decode(bytes_view(block).subspan(
                static_cast<std::size_t>(slot * NodeItem::kSize), NodeItem::kSize));
            if (!item.intersects(query))
                continue;

            if (is_leaf) {
                results.push_back(SearchResultItem{item.offset, pos - leaf_nodes_offset});
            } else {
                const std::size_t child_level = level - 1;
                if (item.offset < level_bounds[child_level].start ||
                    item.offset >= level_bounds[child_level].end) {
                    throw Error(ErrorCode::NoIndex, "rtree child index outside the child level");
                }
                queue.emplace_back(item.offset, child_level);
            }
        }
    }

    // Read forward through the features section.
    std::sort(
        results.begin(), results.end(),
        [](const SearchResultItem& a, const SearchResultItem& b) { return a.offset < b.offset; });
    return results;
}

}  // namespace fcb
