#pragma once

#include <cstdint>
#include <string>
#include <vector>

#include <fcb/error.hpp>
#include <fcb/header.hpp>
#include <fcb/key.hpp>
#include <fcb/range_reader.hpp>

namespace fcb {

struct SearchResultItem;

/// Comparison operators the attribute index supports.
enum class Operator { Eq, Ne, Gt, Ge, Lt, Le };

/// One condition of an attribute query.
struct AttrCondition {
    std::string field;
    Operator op;
    KeyValue value;
};

using AttrQuery = std::vector<AttrCondition>;

struct AttrQueryOptions {
    /// Return raw index candidates without verifying them against the
    /// decoded attribute. Faster, but may include non-matching features for
    /// fixed-string columns, whose keys are truncated to 50 or 100 bytes.
    /// Default false: verify.
    bool exact_index_only = false;
};

/// The MSB of a leaf offset marks a payload reference rather than a direct
/// feature offset (stree.rs:15-17).
constexpr std::uint64_t kPayloadTag = 1ULL << 63;
constexpr std::uint64_t kPayloadMask = ~kPayloadTag;

inline bool is_payload_ref(std::uint64_t off) { return (off & kPayloadTag) != 0; }
inline std::uint64_t payload_offset(std::uint64_t off) { return off & kPayloadMask; }

/// Total node count. NOTE the loop breaks at `n < branching_factor`, unlike
/// the R-tree's `n == 1` (stree.rs:462-497). The asymmetry is deliberate in
/// the reference; do not "fix" it.
std::uint64_t stree_num_nodes(std::uint64_t num_items, std::uint16_t branching_factor);

/// Decode a payload entry: u32 count then count x u64, all little-endian.
std::vector<std::uint64_t> decode_payload_entry(bytes_view b);

/// Run one condition against one column's index blob, returning candidate
/// feature offsets (relative to the features section).
std::vector<SearchResultItem> stree_query(RangeReader& reader,
                                          const AttrIndexInfo& index,
                                          KeyKind kind,
                                          Operator op,
                                          const KeyValue& value);

}  // namespace fcb
