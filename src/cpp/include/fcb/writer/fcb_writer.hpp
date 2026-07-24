#pragma once

#ifdef FCB_WITH_JSON

#    include <fcb/layout.hpp>
#    include <fcb/writer/attribute.hpp>
#    include <fcb/writer/header_serializer.hpp>

#    include <nlohmann/json.hpp>

#    include <array>
#    include <cstdint>
#    include <optional>
#    include <string>
#    include <vector>

namespace fcb {

/// Configuration for `write_fcb`. Mirrors `HeaderWriterOptions`
/// (writer/header_writer.rs) minus the CLI-only knobs this library has no
/// use for (there is no `-g`/bbox-filter/`--no-feature-count` equivalent:
/// `feature_count` is always `features.size()`).
struct FcbWriterOptions {
    /// `false` forces `index_node_size` to 0 in the written header (no
    /// R-tree at all), regardless of `index_node_size` below -- mirrors
    /// `HeaderWriter::new_with_options` (header_writer.rs:87-94).
    bool write_index = true;
    std::uint16_t index_node_size = kDefaultNodeSize;
    /// (attribute name, branching factor); `std::nullopt` branching factor
    /// means `static_btree`'s own default. Empty means no attribute index
    /// is built for any column.
    std::vector<std::pair<std::string, std::optional<std::uint16_t>>> attribute_indices;
    std::optional<std::array<double, 6>> geographical_extent;
};

/// Writes a complete `.fcb` file: magic bytes, header, spatial index (if
/// any), attribute indices (if any), and every feature -- byte-identical
/// to what `FcbWriter::write` (writer/mod.rs:191-278) produces for the
/// same CityJSON input and options.
///
/// `cj` is the CityJSONSeq's metadata line (first line: `type`/`version`/
/// `transform`/`metadata`/etc, WITHOUT `CityObjects`/`vertices` mattering --
/// M4 ignores them). `features` are every subsequent `CityJSONFeature`
/// line, in the document's own order (the order features are ADDED in;
/// the file's final on-disk order is hilbert-sorted by bbox, computed
/// here, not by the caller). `attr_schema`/`semantic_attr_schema` must
/// already reflect every feature (typically built via `add_attributes`
/// over each feature's `CityObjects` before calling this).
std::vector<std::uint8_t> write_fcb(const nlohmann::ordered_json& cj,
                                    const std::vector<nlohmann::ordered_json>& features,
                                    const FcbWriterOptions& options,
                                    const AttributeSchema& attr_schema,
                                    const AttributeSchema* semantic_attr_schema);

}  // namespace fcb

#endif  // FCB_WITH_JSON
