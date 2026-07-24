#include <fcb/error.hpp>
#include <fcb/writer/btree_builder.hpp>

#include <algorithm>
#include <unordered_map>

namespace fcb {

namespace {

/// A key with its unique-leaf-slot offset. `.offset` is either a real
/// feature byte offset (a key with exactly one entry) or `kPayloadTag |
/// <byte offset into the payload section>` (a key with more than one).
struct UniqueLeaf {
    KeyValue key;
    std::uint64_t offset;
};

/// Sorts `entries` by key (stable, so duplicate-key entries keep their
/// input relative order -- Rust's `sort_by_key` is also a stable sort) and
/// groups consecutive equal keys into one `UniqueLeaf` each, writing any
/// group of more than one into the payload section. Mirrors the grouping
/// loop in `Stree::build` (stree.rs:640-673).
std::pair<std::vector<UniqueLeaf>, std::vector<std::uint8_t>>
group_duplicates(std::vector<BtreeEntry> entries) {
    std::stable_sort(entries.begin(), entries.end(), [](const BtreeEntry& a, const BtreeEntry& b) {
        return compare_keys(a.key, b.key) < 0;
    });

    std::vector<UniqueLeaf> unique_leaves;
    std::vector<std::uint8_t> payload_data;
    std::size_t i = 0;
    while (i < entries.size()) {
        std::size_t j = i + 1;
        while (j < entries.size() && compare_keys(entries[j].key, entries[i].key) == 0)
            ++j;

        if (j - i == 1) {
            unique_leaves.push_back(UniqueLeaf{entries[i].key, entries[i].offset});
        } else {
            std::vector<std::uint64_t> offsets;
            offsets.reserve(j - i);
            for (std::size_t k = i; k < j; ++k)
                offsets.push_back(entries[k].offset);
            const std::uint64_t rel = payload_data.size();
            encode_payload_entry(payload_data, offsets);
            unique_leaves.push_back(UniqueLeaf{entries[i].key, kPayloadTag | rel});
        }
        i = j;
    }
    return {unique_leaves, payload_data};
}

/// Builds every internal level, bottom-up, from an array whose leaf slots
/// are already filled in. Traced by hand against both `generate_nodes`
/// (stree.rs:510-603) and the read side's `find_exact` together -- see the
/// M6 plan doc for the derived invariant. `tree[i].key`/`.offset` for
/// i in the leaf range must already be set by the caller; every other slot
/// is written here.
void generate_nodes(std::vector<UniqueLeaf>& tree, const std::vector<StreeLevelBound>& level_bounds,
                    std::uint16_t branching_factor, std::size_t num_leaf_nodes, KeyKind kind) {
    const std::uint64_t node_size = static_cast<std::uint64_t>(branching_factor) - 1;
    const std::uint64_t skip_size = static_cast<std::uint64_t>(branching_factor) * node_size;
    const std::uint64_t bf2 = static_cast<std::uint64_t>(branching_factor) * branching_factor;
    const std::uint64_t num_nodes = tree.size();
    const std::uint64_t leaf_start = num_nodes - num_leaf_nodes;

    // Keyed by flat array index: the TRUE minimum key covered by that
    // index's subtree (as opposed to `tree[index].key`, which for an
    // INTERNAL index is a separator, not that subtree's minimum). Only
    // ever read one level below where it was written; each level's pass
    // must run to completion before the next level's pass starts.
    std::unordered_map<std::uint64_t, KeyValue> parent_min_key;

    auto require_min_key = [&](std::uint64_t idx) -> const KeyValue& {
        auto it = parent_min_key.find(idx);
        if (it == parent_min_key.end()) {
            throw Error(ErrorCode::InvalidAttributeValue,
                        "static B+tree builder: missing parent_min_key entry -- this is a bug in "
                        "the builder itself, not malformed input");
        }
        return it->second;
    };

    for (std::size_t level = 0; level + 1 < level_bounds.size(); ++level) {
        const StreeLevelBound& children_level = level_bounds[level];
        const StreeLevelBound& parent_level = level_bounds[level + 1];

        std::uint64_t parent_idx = parent_level.start;
        std::uint64_t child_idx = children_level.start;

        while (child_idx < children_level.end) {
            if (parent_idx >= parent_level.end)
                break;

            const std::uint64_t child_idx_diff = child_idx - children_level.start;
            const std::uint64_t m = child_idx_diff % skip_size;
            const bool is_right_most_child = (node_size * node_size <= m) && (m < bf2);
            const bool has_next_node = child_idx + node_size < children_level.end;

            if (is_right_most_child) {
                child_idx += node_size;
                continue;
            }

            if (!has_next_node) {
                const KeyValue parent_key = key_max(kind);
                tree[parent_idx] = UniqueLeaf{parent_key, child_idx};

                // `min(tree[child_idx].key, parent_min_key[child_idx] or max)`
                // WITHOUT branching on whether `child_idx` is itself a leaf --
                // and that omission is deliberate, not a shortcut this port
                // is taking. For a LEAF child, `tree[child_idx].key` already
                // IS that leaf's true min, and no `parent_min_key` entry
                // exists for it (falls back to `key_max`, so `min` picks the
                // real key). For an INTERNAL child, `tree[child_idx].key` is
                // a right-sibling separator -- provably >= that subtree's
                // true min (a separator is always the min of something to
                // its OWN right, hence >= its own subtree's min) -- so `min`
                // always resolves to the correct value already sitting in
                // `parent_min_key`. Branching explicitly on `is_leaf_node`
                // here would also be correct, but "simplifying" this by
                // inverting to `max` (or by only handling one case) is the
                // exact mistake this comment exists to head off. Confirmed
                // during Fable consultation on this milestone.
                const KeyValue& candidate =
                    parent_min_key.count(child_idx) ? parent_min_key.at(child_idx) : key_max(kind);
                const KeyValue& own_min = compare_keys(tree[child_idx].key, candidate) < 0
                                              ? tree[child_idx].key
                                              : candidate;
                parent_min_key.insert_or_assign(parent_idx, own_min);
                ++parent_idx;
                child_idx += node_size;
                continue;
            }

            const std::uint64_t right_node_idx = child_idx + node_size;
            const bool is_leaf_node = child_idx >= leaf_start;

            if (is_leaf_node) {
                const KeyValue parent_key =
                    right_node_idx < children_level.end ? tree[right_node_idx].key : key_max(kind);
                tree[parent_idx] = UniqueLeaf{parent_key, child_idx};
                parent_min_key.insert_or_assign(parent_idx, tree[child_idx].key);
                ++parent_idx;
                child_idx += node_size;
                continue;
            }

            const KeyValue parent_key = right_node_idx < children_level.end
                                            ? require_min_key(child_idx + node_size)
                                            : key_max(kind);
            tree[parent_idx] = UniqueLeaf{parent_key, child_idx};
            parent_min_key.insert_or_assign(parent_idx, require_min_key(child_idx));
            ++parent_idx;
            child_idx += node_size;
        }
    }
}

}  // namespace

BuiltBtreeIndex build_static_btree(const std::vector<BtreeEntry>& entries, KeyKind kind,
                                   std::uint16_t branching_factor) {
    // `Stree::build` (stree.rs:638-640) clamps `branching_factor` to
    // `[2, 65535]` BEFORE ever calling `init()` -- so `init()`'s own
    // `if branching_factor < 2 { return Err(...) }` (stree.rs:451-455) is
    // unreachable from `build`'s call path and a sub-2 value silently
    // becomes 2, NOT an error. This is the OPPOSITE of the packed R-tree's
    // `PackedRTree::build`, which `assert!`s (panics) on a sub-2 node size
    // instead of pre-clamping -- a real asymmetry in Rust itself between
    // the two builders, not a typo to "fix" into symmetry. Originally
    // ported as a throw here (copying the R-tree's M5 behavior without
    // re-verifying against the B+tree's OWN entry point); caught by the
    // M6 codex review.
    branching_factor = std::clamp<std::uint16_t>(branching_factor, 2, 65535);

    // `init()`'s OTHER check -- `if self.num_leaf_nodes == 0 { return
    // Err(...) }` -- is NOT preempted by anything in `build()`, so it DOES
    // still throw through this call path; `num_leaf_nodes` there is
    // `unique_leaves.len()` (post-dedup), but an empty `entries` can never
    // produce a non-empty `unique_leaves` either way.
    if (entries.empty()) {
        throw Error(ErrorCode::AttributeIndexNotFound,
                    "cannot build a static B+tree index with no entries");
    }
    // Rust's `Stree<K>` is generic over ONE concrete key type `K`, so a
    // kind mismatch is unrepresentable there; this port's `KeyValue` is a
    // runtime-tagged union, so nothing stops a caller from mixing kinds
    // unless checked here explicitly. `compare_keys` (used below, during
    // sorting) throws when it's given two DIFFERENT kinds to compare
    // against EACH OTHER, but every entry sharing one (wrong) kind that
    // simply differs from `kind` would sail through sorting undetected and
    // silently encode a mismatched-width blob instead. Found during the
    // M6 codex review.
    for (const auto& e : entries) {
        if (e.key.kind() != kind) {
            throw Error(ErrorCode::UnsupportedColumnType,
                        "static B+tree: entry key kind does not match the column's declared kind");
        }
    }

    auto [unique_leaves, payload_data] = group_duplicates(entries);

    const auto level_bounds = stree_level_bounds(unique_leaves.size(), branching_factor);
    const std::uint64_t num_nodes = level_bounds.front().end;

    std::vector<UniqueLeaf> tree(static_cast<std::size_t>(num_nodes), UniqueLeaf{KeyValue{}, 0});
    const std::uint64_t leaf_start = num_nodes - unique_leaves.size();
    for (std::size_t i = 0; i < unique_leaves.size(); ++i)
        tree[static_cast<std::size_t>(leaf_start) + i] = unique_leaves[i];

    generate_nodes(tree, level_bounds, branching_factor, unique_leaves.size(), kind);

    std::vector<std::uint8_t> bytes;
    bytes.reserve(tree.size() * (key_serialized_size(kind) + 8) + payload_data.size());
    for (const auto& node : tree) {
        auto key_bytes = encode_key(node.key);
        bytes.insert(bytes.end(), key_bytes.begin(), key_bytes.end());
        for (int i = 0; i < 8; ++i)
            bytes.push_back(static_cast<std::uint8_t>((node.offset >> (8 * i)) & 0xFF));
    }
    bytes.insert(bytes.end(), payload_data.begin(), payload_data.end());

    return BuiltBtreeIndex{std::move(bytes), branching_factor,
                           static_cast<std::uint32_t>(unique_leaves.size())};
}

}  // namespace fcb
