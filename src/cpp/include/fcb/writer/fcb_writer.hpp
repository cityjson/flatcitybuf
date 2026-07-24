#pragma once

#ifdef FCB_WITH_JSON

#    include <fcb/layout.hpp>
#    include <fcb/writer/attribute.hpp>
#    include <fcb/writer/header_serializer.hpp>
#    include <fcb/writer/rtree_builder.hpp>

#    include <nlohmann/json.hpp>

#    include <array>
#    include <cstdint>
#    include <cstdio>
#    include <optional>
#    include <ostream>
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
/// (writer/mod.rs) shape: each `add_feature` call encodes and spools that
/// feature's bytes to a private temporary file rather than keeping every
/// encoded feature in memory at once -- the same reason Rust's own writer
/// uses the `tempfile` crate. This keeps PEAK memory during accumulation to
/// roughly one feature at a time, regardless of how many features are
/// added, so a caller that itself streams its `CityJSONSeq` input (parsing
/// one line, calling `add_feature`, discarding the parsed JSON) never needs
/// the whole input in memory either.
///
/// The `write(std::ostream&)` overload preserves that property all the way
/// through finalization: it streams each feature's bytes from the spool
/// straight to `out` in fixed-size chunks, so finalizing never holds more
/// than one chunk of feature data in memory. The `write()` overload
/// returning `std::vector<std::uint8_t>` is a convenience wrapper for
/// smaller files/tests and does NOT have that property -- it necessarily
/// materializes the complete output in memory, since that is what its
/// return type requires. Use the `ostream` overload (writing to an
/// `std::ofstream` opened on the real output path) for anything where
/// output size matters.
///
/// Usage:
/// ```cpp
/// FcbWriter w(std::move(cj_metadata), options, std::move(attr_schema), std::nullopt);
/// for (const auto& feature : features) w.add_feature(feature);
/// std::ofstream out("result.fcb", std::ios::binary);
/// w.write(out);
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
    /// the spatial and attribute indices, and streams the complete byte
    /// sequence -- magic bytes, header, R-tree, attribute indices, features,
    /// in that order, byte-identical to what `FcbWriter::write`
    /// (writer/mod.rs:191-278) produces for the same input -- to `out`,
    /// copying feature bytes from the spool file in fixed-size chunks
    /// rather than materializing them all at once.
    ///
    /// May be called only once (either overload); throws
    /// `fcb::Error{IoError}` otherwise.
    void write(std::ostream& out);

    /// Convenience wrapper around `write(std::ostream&)` that returns the
    /// complete file as one buffer. Does NOT have the streaming overload's
    /// bounded-memory property -- prefer the `ostream` overload for large
    /// output.
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

    // `std::tmpfile()`, not a hand-rolled path under
    // `std::filesystem::temp_directory_path()`: the C standard guarantees
    // it names a file "different from any other existing file" and removes
    // it automatically on close, which is exactly the anonymous,
    // collision-safe temp file Rust's own `tempfile` crate provides --
    // reusing that guarantee is safer than reimplementing it. 64-bit
    // seeking uses `fseeko`/`ftello` (POSIX) or `_fseeki64` (MSVC) instead
    // of `fseek`'s `long` offset, which is only 32 bits wide on some
    // platforms -- a real limit for a writer whose whole point is handling
    // files too large to fit in memory.
    std::FILE* tmp_ = nullptr;
    std::uint64_t tmp_write_pos_ = 0;
    bool written_ = false;

    std::vector<FeatureSlot> feat_offsets_;
    std::vector<NodeItem> feat_nodes_;
    std::vector<std::vector<AttributeIndexEntry>> index_entries_by_feature_;
};

}  // namespace fcb

#endif  // FCB_WITH_JSON
