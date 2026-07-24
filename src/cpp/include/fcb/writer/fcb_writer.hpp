#pragma once

#ifdef FCB_WITH_JSON

#    include <fcb/layout.hpp>
#    include <fcb/writer/attribute.hpp>
#    include <fcb/writer/header_serializer.hpp>
#    include <fcb/writer/rtree_builder.hpp>

#    include <nlohmann/json.hpp>

#    include <array>
#    include <cstdint>
#    include <filesystem>
#    include <fstream>
#    include <optional>
#    include <string>
#    include <vector>

namespace fcb {

/// Configuration for `FcbWriter`. Mirrors `HeaderWriterOptions`
/// (writer/header_writer.rs) minus the CLI-only knobs this library has no
/// use for (there is no `-g`/bbox-filter/`--no-feature-count` equivalent:
/// `feature_count` is always however many features were added).
struct FcbWriterOptions {
    /// `false` forces `index_node_size` to 0 in the written header (no
    /// R-tree at all), regardless of `index_node_size` below -- mirrors
    /// `HeaderWriter::new_with_options` (header_writer.rs:87-94).
    bool write_index = true;
    std::uint16_t index_node_size = kDefaultNodeSize;
    /// (attribute name, branching factor); `std::nullopt` branching factor
    /// means `build_static_btree`'s own default. Empty means no attribute
    /// index is built for any column.
    std::vector<std::pair<std::string, std::optional<std::uint16_t>>> attribute_indices;
    std::optional<std::array<double, 6>> geographical_extent;
};

/// Streaming writer for one `.fcb` file, mirroring Rust's own `FcbWriter`
/// (writer/mod.rs) shape and its memory-scalability property: each
/// `add_feature` call encodes and spools that feature's bytes to a private
/// temporary file rather than keeping every feature in memory at once, so
/// a `CityJSONSeq` far larger than available RAM can still be written --
/// Rust's own writer does the same, via the `tempfile` crate.
///
/// Usage:
/// ```cpp
/// FcbWriter w(std::move(cj_metadata), options, std::move(attr_schema), std::nullopt);
/// for (const auto& feature : features) w.add_feature(feature);
/// std::vector<std::uint8_t> bytes = w.write();
/// ```
///
/// Not copyable or movable: it owns a live handle to its temp file for its
/// entire lifetime, closed and removed only in the destructor.
class FcbWriter {
  public:
    /// `cj` is the CityJSONSeq's metadata line (first line: `type`/
    /// `version`/`transform`/`metadata`/etc). `attr_schema`/
    /// `semantic_attr_schema` must already reflect every feature that will
    /// be added (typically built via `add_attributes` over each feature's
    /// `CityObjects`, scanning all features once before constructing this,
    /// exactly as the Rust `fcb` CLI's own two-pass approach does).
    ///
    /// Throws `fcb::Error{IoError}` if a temporary file cannot be created.
    FcbWriter(nlohmann::ordered_json cj, FcbWriterOptions options, AttributeSchema attr_schema,
              std::optional<AttributeSchema> semantic_attr_schema);
    ~FcbWriter();

    FcbWriter(const FcbWriter&) = delete;
    FcbWriter& operator=(const FcbWriter&) = delete;
    FcbWriter(FcbWriter&&) = delete;
    FcbWriter& operator=(FcbWriter&&) = delete;

    /// Encodes and spools one `CityJSONFeature` line. Throws
    /// `fcb::Error{IoError}` if called after `write()`, or if the spool
    /// write itself fails.
    void add_feature(const nlohmann::ordered_json& city_json_feature);

    /// Finalizes the file: hilbert-sorts features by bbox (unless
    /// `options.write_index` is false or no features were added), builds
    /// the spatial and attribute indices, and assembles the complete byte
    /// stream -- magic bytes, header, R-tree, attribute indices, features,
    /// in that order, byte-identical to what `FcbWriter::write`
    /// (writer/mod.rs:191-278) produces for the same input.
    ///
    /// May be called only once; throws `fcb::Error{IoError}` otherwise.
    std::vector<std::uint8_t> write();

  private:
    struct FeatureSlot {
        std::uint64_t offset;
        std::uint64_t size;
    };

    nlohmann::ordered_json cj_;
    FcbWriterOptions options_;
    AttributeSchema attr_schema_;
    std::optional<AttributeSchema> semantic_attr_schema_;
    std::vector<std::string> indexing_attr_;

    double scale_x_;
    double scale_y_;
    double translate_x_;
    double translate_y_;

    std::filesystem::path tmp_path_;
    std::fstream tmp_;
    std::uint64_t tmp_write_pos_ = 0;
    bool written_ = false;

    std::vector<FeatureSlot> feat_offsets_;
    std::vector<NodeItem> feat_nodes_;
    std::vector<std::vector<AttributeIndexEntry>> index_entries_by_feature_;
};

}  // namespace fcb

#endif  // FCB_WITH_JSON
