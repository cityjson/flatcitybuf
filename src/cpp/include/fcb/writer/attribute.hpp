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
/// `std::map`, not `std::unordered_map`, mirrors Rust's `BTreeMap`
/// (writer/attribute.rs): the column INDEX comes from `schema.size()` at
/// insert time, independent of the map's own order, but both nlohmann's
/// default object type and serde_json's default `Map` iterate a JSON
/// object's keys alphabetically -- so when several new attributes appear
/// together in one `add_attributes` call, both languages assign indices in
/// the same (alphabetical) order without any extra sorting.
using AttributeSchema = std::map<std::string, std::pair<std::uint16_t, ::ColumnType>>;

/// Adds every member of a JSON object to `schema`, assigning each new,
/// non-null name the next free column index. A non-object `attrs` becomes a
/// single "json" column, matching the writer's fallback for untyped
/// attribute payloads. Existing names and null values are left alone.
void add_attributes(AttributeSchema& schema, const nlohmann::json& attrs);

/// Byte width one value of `coltype` occupies in the attribute blob,
/// EXCLUDING the 2-byte column-index prefix every record also carries.
std::size_t attr_size(::ColumnType coltype, const nlohmann::json& colval);

/// Encodes `attr` (a CityJSON attributes object) against `schema`: repeated
/// `[u16 LE column index][value]` records, one per schema member present
/// and non-null in `attr`, in ascending column-index order (NOT `attr`'s own
/// JSON key order). A schema member absent from `attr`, or explicitly null,
/// is skipped -- not zero-filled. Returns an empty vector for a non-object
/// or empty `attr`.
std::vector<std::uint8_t> encode_attributes_with_schema(const nlohmann::json& attr,
                                                        const AttributeSchema& schema);

}  // namespace fcb

#endif  // FCB_WITH_JSON
