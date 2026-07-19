#include <fcb/stree.hpp>

#include <fcb/reader.hpp>

#include "detail/checked.hpp"

#include <algorithm>
#include <deque>
#include <utility>

namespace fcb {

namespace {

struct LevelBound {
    std::uint64_t start;
    std::uint64_t end;
};

/// Mirrors Stree::generate_level_bounds (stree.rs:462-497).
///
/// The loop divides by `branching_factor` and stops at `n < branching_factor`
/// -- NOT at `n == 1` as the R-tree does. Searching, meanwhile, uses
/// `branching_factor - 1` entries per node. Both asymmetries are real: the
/// entry count per node is one less than the fan-out because each entry is a
/// separator key, and the level loop stops as soon as a level fits in one
/// node's worth of separators.
std::vector<LevelBound> generate_level_bounds(std::uint64_t num_items,
                                              std::uint16_t branching_factor) {
    if (branching_factor < 2) {
        throw Error(ErrorCode::AttributeIndexNotFound,
                    "invalid branching factor " + std::to_string(branching_factor));
    }
    if (num_items == 0) {
        throw Error(ErrorCode::AttributeIndexNotFound, "empty attribute index");
    }

    std::vector<std::uint64_t> level_num_nodes;
    std::uint64_t n = num_items;
    std::uint64_t num_nodes = n;
    level_num_nodes.push_back(n);
    for (;;) {
        n = detail::ceil_div(n, branching_factor);
        num_nodes = detail::checked_add(num_nodes, n, "stree num_nodes");
        level_num_nodes.push_back(n);
        if (n < branching_factor) break;
    }

    std::vector<LevelBound> bounds;
    bounds.reserve(level_num_nodes.size());
    std::uint64_t acc = num_nodes;
    for (std::uint64_t size : level_num_nodes) {
        acc -= size;
        bounds.push_back(LevelBound{acc, acc + size});
    }
    return bounds;
}

/// One entry: the key then a u64 little-endian offset (entry.rs:25-52).
struct Entry {
    KeyValue key;
    std::uint64_t offset;
};

std::uint64_t entry_size(KeyKind kind) { return key_serialized_size(kind) + 8; }

std::uint64_t read_u64_le(bytes_view b, std::size_t at) {
    std::uint64_t v = 0;
    for (std::size_t i = 0; i < 8; ++i) {
        v |= static_cast<std::uint64_t>(b[at + i]) << (8 * i);
    }
    return v;
}

/// Read entries [first, last) of the flat node array.
std::vector<Entry> read_entries(RangeReader& reader, std::uint64_t index_begin,
                                KeyKind kind, std::uint64_t first, std::uint64_t last) {
    std::vector<Entry> out;
    if (last <= first) return out;

    const std::uint64_t esz = entry_size(kind);
    const std::uint64_t at =
        detail::checked_add(index_begin, detail::checked_mul(first, esz, "entry offset"),
                            "entry base");
    const std::uint64_t len = detail::checked_mul(last - first, esz, "entry span");

    auto block = reader.read(at, len);
    if (block.size() < len) {
        throw Error(ErrorCode::AttributeIndexNotFound, "truncated attribute index node");
    }

    const std::size_t ksz = key_serialized_size(kind);
    out.reserve(static_cast<std::size_t>(last - first));
    for (std::uint64_t i = 0; i < last - first; ++i) {
        const std::size_t base = static_cast<std::size_t>(i * esz);
        Entry e{};
        e.key = decode_key(kind, bytes_view(block).subspan(base, ksz));
        e.offset = read_u64_le(bytes_view(block), base + ksz);
        out.push_back(std::move(e));
    }
    return out;
}

/// std::lower_bound-style search returning Rust's binary_search_by result:
/// `found` plus the index (of the match, or of the insertion point).
struct BinarySearch {
    bool found;
    std::size_t index;
};

BinarySearch binary_search(const std::vector<Entry>& items, const KeyValue& key) {
    std::size_t lo = 0, hi = items.size();
    while (lo < hi) {
        const std::size_t mid = lo + (hi - lo) / 2;
        const int c = compare_keys(items[mid].key, key);
        if (c == 0) return {true, mid};
        if (c < 0) {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    return {false, lo};
}

/// Resolve one leaf offset into feature offsets, following a payload
/// reference when the MSB is set.
void emit_offset(std::uint64_t off, std::uint64_t index, RangeReader& reader,
                 std::uint64_t payload_begin, std::uint64_t payload_size,
                 std::vector<SearchResultItem>& out) {
    if (!is_payload_ref(off)) {
        out.push_back(SearchResultItem{off, index});
        return;
    }

    const std::uint64_t rel = payload_offset(off);
    if (rel + 4 > payload_size) {
        throw Error(ErrorCode::AttributeIndexNotFound, "payload reference out of range");
    }

    auto head = reader.read(detail::checked_add(payload_begin, rel, "payload"), 4);
    if (head.size() < 4) {
        throw Error(ErrorCode::AttributeIndexNotFound, "truncated payload entry");
    }
    std::uint32_t count = 0;
    for (std::size_t i = 0; i < 4; ++i) count |= static_cast<std::uint32_t>(head[i]) << (8 * i);

    const std::uint64_t want = detail::checked_add(4, detail::checked_mul(count, 8, "payload"),
                                                   "payload entry");
    if (rel + want > payload_size) {
        throw Error(ErrorCode::AttributeIndexNotFound, "payload entry overruns its section");
    }

    auto body = reader.read(detail::checked_add(payload_begin, rel, "payload"), want);
    if (body.size() < want) {
        throw Error(ErrorCode::AttributeIndexNotFound, "truncated payload entry body");
    }
    for (std::uint32_t i = 0; i < count; ++i) {
        out.push_back(SearchResultItem{read_u64_le(bytes_view(body), 4 + i * 8), index});
    }
}

/// Shared state for one query against one column's index.
struct Tree {
    RangeReader& reader;
    std::uint64_t index_begin;
    std::uint64_t payload_begin;
    std::uint64_t payload_size;
    KeyKind kind;
    std::uint64_t node_size;  // branching_factor - 1
    std::vector<LevelBound> levels;

    std::uint64_t leaf_start() const { return levels.front().start; }
    std::uint64_t leaf_end() const { return levels.front().end; }

    std::vector<Entry> node_at(std::uint64_t node_index, std::size_t level) const {
        const std::uint64_t end =
            std::min<std::uint64_t>(node_index + node_size, levels[level].end);
        return read_entries(reader, index_begin, kind, node_index, end);
    }
};

/// Mirrors Stree::find_exact (stree.rs:733-816).
std::vector<SearchResultItem> find_exact(const Tree& t, const KeyValue& key) {
    std::vector<SearchResultItem> out;
    std::deque<std::pair<std::uint64_t, std::size_t>> queue;
    queue.emplace_back(0, t.levels.size() - 1);

    while (!queue.empty()) {
        const auto [node_index, level] = queue.front();
        queue.pop_front();

        auto items = t.node_at(node_index, level);
        if (items.empty()) continue;

        const auto hit = binary_search(items, key);

        if (level != 0) {
            // Internal descent. On an exact hit the search key belongs to the
            // RIGHT of that separator, hence the + node_size; find_partition
            // deliberately omits it (see below).
            std::uint64_t child = 0;
            if (hit.found) {
                child = items[hit.index].offset + t.node_size;
            } else if (hit.index == 0) {
                child = items[0].offset;
            } else if (hit.index >= items.size()) {
                child = items.back().offset + t.node_size;
            } else {
                child = items[hit.index].offset;
            }
            queue.emplace_back(child, level - 1);
            continue;
        }

        if (hit.found) {
            emit_offset(items[hit.index].offset,
                        node_index + hit.index - t.leaf_start(), t.reader,
                        t.payload_begin, t.payload_size, out);
        }
    }
    return out;
}

/// Mirrors Stree::find_partition (stree.rs:1086-1128): the same descent as
/// find_exact EXCEPT that an exact hit descends to `offset` with no
/// + node_size. That difference is what makes it return the leftmost
/// position where the key could sit, rather than skipping past equal keys.
std::uint64_t find_partition(const Tree& t, const KeyValue& key) {
    std::uint64_t node_index = 0;
    for (std::size_t level = t.levels.size(); level-- > 1;) {
        auto items = t.node_at(node_index, level);
        if (items.empty()) continue;

        const auto hit = binary_search(items, key);
        if (hit.found) {
            node_index = items[hit.index].offset;
        } else if (hit.index == 0) {
            node_index = items[0].offset;
        } else if (hit.index >= items.size()) {
            node_index = items.back().offset + t.node_size;
        } else {
            node_index = items[hit.index].offset;
        }
    }
    return node_index;
}

/// Mirrors Stree::find_range (stree.rs:923-991). INCLUSIVE on both ends.
std::vector<SearchResultItem> find_range(const Tree& t, const KeyValue& lower,
                                         const KeyValue& upper) {
    if (compare_keys(lower, upper) > 0) return {};
    if (compare_keys(lower, upper) == 0) return find_exact(t, lower);

    const std::uint64_t lower_idx = find_partition(t, lower);
    const std::uint64_t upper_idx = find_partition(t, upper);

    const std::uint64_t start = std::max<std::uint64_t>(lower_idx, t.leaf_start());
    const std::uint64_t end =
        std::min<std::uint64_t>(upper_idx + t.node_size, t.leaf_end());

    std::vector<SearchResultItem> out;
    std::uint64_t cur = start;
    while (cur < end) {
        const std::uint64_t node_end = std::min<std::uint64_t>(cur + t.node_size, end);
        auto items = read_entries(t.reader, t.index_begin, t.kind, cur, node_end);
        for (std::size_t i = 0; i < items.size(); ++i) {
            if (compare_keys(items[i].key, lower) >= 0 &&
                compare_keys(items[i].key, upper) <= 0) {
                emit_offset(items[i].offset, cur + i - t.leaf_start(), t.reader,
                            t.payload_begin, t.payload_size, out);
            }
        }
        cur = node_end;
    }
    return out;
}

void subtract(std::vector<SearchResultItem>& from,
              const std::vector<SearchResultItem>& remove) {
    if (remove.empty()) return;
    std::vector<std::uint64_t> drop;
    drop.reserve(remove.size());
    for (const auto& r : remove) drop.push_back(r.offset);
    std::sort(drop.begin(), drop.end());

    from.erase(std::remove_if(from.begin(), from.end(),
                              [&](const SearchResultItem& s) {
                                  return std::binary_search(drop.begin(), drop.end(),
                                                            s.offset);
                              }),
               from.end());
}

}  // namespace

std::uint64_t stree_num_nodes(std::uint64_t num_items, std::uint16_t branching_factor) {
    if (branching_factor < 2) {
        throw Error(ErrorCode::AttributeIndexNotFound, "invalid branching factor");
    }
    if (num_items == 0) return 0;

    std::uint64_t n = num_items;
    std::uint64_t num_nodes = n;
    for (;;) {
        n = detail::ceil_div(n, branching_factor);
        num_nodes = detail::checked_add(num_nodes, n, "stree num_nodes");
        if (n < branching_factor) break;
    }
    return num_nodes;
}

std::vector<std::uint64_t> decode_payload_entry(bytes_view b) {
    if (b.size() < 4) {
        throw Error(ErrorCode::AttributeIndexNotFound, "short payload entry");
    }
    std::uint32_t count = 0;
    for (std::size_t i = 0; i < 4; ++i) count |= static_cast<std::uint32_t>(b[i]) << (8 * i);

    if (b.size() < 4 + static_cast<std::size_t>(count) * 8) {
        throw Error(ErrorCode::AttributeIndexNotFound, "truncated payload entry");
    }
    std::vector<std::uint64_t> out;
    out.reserve(count);
    for (std::uint32_t i = 0; i < count; ++i) {
        out.push_back(read_u64_le(b, 4 + static_cast<std::size_t>(i) * 8));
    }
    return out;
}

std::vector<SearchResultItem> stree_query(RangeReader& reader,
                                          const AttrIndexInfo& index,
                                          KeyKind kind,
                                          Operator op,
                                          const KeyValue& value) {
    const std::uint64_t num_nodes = stree_num_nodes(index.num_unique_items,
                                                    index.branching_factor);
    const std::uint64_t tree_bytes =
        detail::checked_mul(num_nodes, entry_size(kind), "stree size");
    if (tree_bytes > index.length) {
        throw Error(ErrorCode::AttributeIndexNotFound,
                    "attribute index node region exceeds its declared length");
    }

    Tree t{reader,
           index.begin,
           detail::checked_add(index.begin, tree_bytes, "payload begin"),
           index.length - tree_bytes,
           kind,
           static_cast<std::uint64_t>(index.branching_factor) - 1,
           generate_level_bounds(index.num_unique_items, index.branching_factor)};

    // Operator lowering, mirroring query/stream.rs:161-191. find_range is
    // inclusive at both ends, so the strict operators subtract the equal set.
    switch (op) {
        case Operator::Eq:
            return find_exact(t, value);
        case Operator::Ge:
            return find_range(t, value, key_max(kind));
        case Operator::Le:
            return find_range(t, key_min(kind), value);
        case Operator::Gt: {
            auto r = find_range(t, value, key_max(kind));
            subtract(r, find_exact(t, value));
            return r;
        }
        case Operator::Lt: {
            auto r = find_range(t, key_min(kind), value);
            subtract(r, find_exact(t, value));
            return r;
        }
        case Operator::Ne: {
            auto r = find_range(t, key_min(kind), key_max(kind));
            subtract(r, find_exact(t, value));
            return r;
        }
    }
    throw Error(ErrorCode::QueryExecutionError, "unknown operator");
}

}  // namespace fcb
