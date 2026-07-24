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

}  // namespace fcb

#endif  // FCB_WITH_JSON
