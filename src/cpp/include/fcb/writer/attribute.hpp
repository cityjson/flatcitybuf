#pragma once

#ifdef FCB_WITH_JSON

#    include <fcb/generated/header_generated.h>
#    include <fcb/key.hpp>

#    include <nlohmann/json.hpp>

#    include <cstdint>
#    include <map>
#    include <string>
#    include <utility>
#    include <vector>

#    include <flatbuffers/flatbuffers.h>

namespace fcb {

/// Attribute schema: name -> (column index, column type).
///
/// `std::map` here is just a lookup table -- its own key order is never
/// read; every consumer (`to_columns`, `encode_attributes_with_schema`)
/// sorts by the stored column INDEX before emitting anything. That index
/// comes from `schema.size()` at insert time, so what actually determines
/// column numbering is the ITERATION ORDER of the `attrs` object passed to
/// `add_attributes` below -- which is why that parameter is
/// `nlohmann::ordered_json`, not the library's default `nlohmann::json`.
///
/// `nlohmann::json`'s default object type is `std::map` (alphabetical), and
/// the original design here assumed `serde_json::Map` was equivalently
/// alphabetical (its documented default, a `BTreeMap`, absent the
/// `preserve_order` feature). Empirically, it is not: this workspace's
/// `bson` dependency (used by `fcb_core` and `fcb_cli`) transitively
/// activates serde_json's `preserve_order` feature for the WHOLE build, so
/// `serde_json::Map` is actually insertion-ordered here, and Rust's
/// `add_attributes` assigns column indices in DOCUMENT order, not
/// alphabetical order (confirmed against real `fcb`-CLI output in the M3
/// oracle tests -- e.g. `single_feature.city.jsonl`'s `{"name":...,"n":...}`
/// gets `name`->0, `n`->1, the reverse of alphabetical). `ordered_json`
/// preserves that same document order, so this stays byte-compatible.
using AttributeSchema = std::map<std::string, std::pair<std::uint16_t, ::ColumnType>>;

/// Adds every member of a JSON object to `schema`, assigning each new,
/// non-null name the next free column index, in the order `attrs`'s own
/// members appear (see the `ordered_json` note above) -- NOT alphabetically.
/// A non-object `attrs` becomes a single "json" column, matching the
/// writer's fallback for untyped attribute payloads. Existing names and
/// null values are left alone.
void add_attributes(AttributeSchema& schema, const nlohmann::ordered_json& attrs);

/// Byte width one value of `coltype` occupies in the attribute blob,
/// EXCLUDING the 2-byte column-index prefix every record also carries.
std::size_t attr_size(::ColumnType coltype, const nlohmann::ordered_json& colval);

/// Encodes `attr` (a CityJSON attributes object) against `schema`: repeated
/// `[u16 LE column index][value]` records, one per schema member present
/// and non-null in `attr`, in ascending column-index order (NOT `attr`'s own
/// JSON key order). A schema member absent from `attr`, or explicitly null,
/// is skipped -- not zero-filled. Returns an empty vector for a non-object
/// or empty `attr`.
std::vector<std::uint8_t> encode_attributes_with_schema(const nlohmann::ordered_json& attr,
                                                        const AttributeSchema& schema);

/// Builds the `Column` vector for `Header.columns` or `CityObject.columns`,
/// in ascending column-index order.
::flatbuffers::Offset<::flatbuffers::Vector<::flatbuffers::Offset<::Column>>>
to_columns(::flatbuffers::FlatBufferBuilder& fbb, const AttributeSchema& schema);

/// One indexable (column, value) pair pulled out of a feature for the
/// static B+tree builder (M6). `value.kind()` always matches
/// `key_kind_for_column(schema column type)` for `index`.
struct AttributeIndexEntry {
    std::uint16_t index;
    KeyValue value;
};

/// Extracts index entries for `indexing_attr` from one CityJSON attributes
/// object. Only Bool, Int, UInt, Long, ULong, Float, Double, String and
/// DateTime columns produce entries -- Byte, UByte, Short, UShort, Json and
/// Binary are silently skipped, matching the Rust writer's
/// `attribute_to_index_entries` exactly (a known, deliberate gap: those
/// types ARE supported by the B+tree builder itself, just never reached
/// through this normal extraction path). A name in `indexing_attr` absent
/// from `attr` or from `schema` is skipped.
std::vector<AttributeIndexEntry>
attribute_to_index_entries(const nlohmann::ordered_json& attr, const AttributeSchema& schema,
                           const std::vector<std::string>& indexing_attr);

/// Same, over every object in one CityJSONFeature's `CityObjects`, visited
/// in ascending object-id order (not JSON key order, which need not be
/// stable) so that duplicate-key payload ordering in the eventual B+tree is
/// reproducible. An object with no `attributes` member, or an explicit
/// `"attributes": null`, contributes nothing.
std::vector<AttributeIndexEntry>
cityfeature_to_index_entries(const nlohmann::ordered_json& city_feature,
                             const AttributeSchema& schema,
                             const std::vector<std::string>& indexing_attr);

}  // namespace fcb

#endif  // FCB_WITH_JSON
