#pragma once

#ifdef FCB_WITH_JSON

#    include <fcb/generated/extension_generated.h>
#    include <fcb/generated/header_generated.h>
#    include <fcb/layout.hpp>
#    include <fcb/writer/attribute.hpp>
#    include <fcb/writer/feature_serializer.hpp>

#    include <nlohmann/json.hpp>

#    include <array>
#    include <optional>
#    include <string>
#    include <vector>

namespace fcb {

/// One attribute column's B+tree index metadata, as recorded in the header
/// (`AttributeIndex`, `header.fbs`). Built by M6 once the actual index
/// bytes exist; this milestone only needs the TYPE, to build `to_fcb_header`
/// against.
struct AttributeIndexInfo {
    std::uint16_t index;
    std::uint32_t length;
    std::uint16_t branching_factor;
    std::uint32_t num_unique_items;
};

/// Configuration for header writing. Mirrors `HeaderWriterOptions`
/// (writer/header_writer.rs). `write_index == false` forces
/// `index_node_size` to 0 at the point the header is actually built (M7),
/// which is how the header says "no R-tree" -- this struct itself does not
/// enforce that, matching Rust's `HeaderWriterOptions::new_with_options`.
struct HeaderWriterOptions {
    bool write_index = true;
    std::uint64_t feature_count = 0;
    std::uint16_t index_node_size = kDefaultNodeSize;
    /// (attribute name, branching factor); `std::nullopt` branching factor
    /// means the default. Empty means Rust's `attribute_indices: None`.
    std::vector<std::pair<std::string, std::optional<std::uint16_t>>> attribute_indices;
    std::optional<std::array<double, 6>> geographical_extent;
};

/// Builds the `Transform` struct (scale + translate) from CityJSON's
/// top-level `transform` member. Mirrors `to_transform`
/// (writer/serializer.rs:257-265).
::Transform to_transform(const nlohmann::ordered_json& transform);

/// A `metadata.referenceSystem` URL, parsed into its OGC three-element form.
struct ParsedReferenceSystem {
    std::string authority;
    std::int32_t version = 0;
    std::int32_t code = 0;
};

/// Parses a `referenceSystem` URL
/// (`https://www.opengis.net/def/crs/{authority}/{version}/{code}`, per
/// cjseq2's `ReferenceSystem::from_url`) into its three OGC elements.
/// `version`/`code` default to 0 when their segment is absent OR fails to
/// parse as a whole `int32` (matching Rust's `.parse::<i32>().ok().
/// unwrap_or(0)` exactly -- a segment like "7415x" does NOT parse as 7415;
/// it fails whole, same as Rust). Returns `std::nullopt` when `url` matches
/// neither the `http://` nor `https://` OGC prefix -- Rust's
/// `TryFrom<String>` would fail the WHOLE document's deserialization in
/// that case instead, which this milestone does not replicate (out of
/// scope; disclosed here rather than silently matched).
std::optional<ParsedReferenceSystem> parse_reference_system(const std::string& url);

/// Builds the `ReferenceSystem` table from a parsed `metadata.referenceSystem`
/// URL. Mirrors `to_reference_system` (writer/serializer.rs:273-301);
/// `code_string` is always absent, matching Rust's own `None` there.
::flatbuffers::Offset<::ReferenceSystem> to_reference_system(::flatbuffers::FlatBufferBuilder& fbb,
                                                             const ParsedReferenceSystem& ref_sys);

/// Builds one `extensions` entry: only `name`/`url`/`version` are written
/// (the schema document itself is never fetched or embedded). Mirrors
/// `to_extension` (writer/serializer.rs:378-397).
::flatbuffers::Offset<::Extension> to_extension(::flatbuffers::FlatBufferBuilder& fbb,
                                                const std::string& name, const std::string& url,
                                                const std::string& version);

/// Builds `Header.templates_vertices` from CityJSON's
/// `geometry-templates.vertices-templates` (f64 precision, unlike a
/// feature's own int32 vertices). A non-3-number-array element is skipped,
/// not guessed at. Mirrors `to_templates_vertices`
/// (writer/serializer.rs:1107-1125).
::flatbuffers::Offset<::flatbuffers::Vector<const ::DoubleVertex*>>
to_templates_vertices(::flatbuffers::FlatBufferBuilder& fbb,
                      const nlohmann::ordered_json& vertices_templates);

/// Builds the `pointOfContact` fields of a header from CityJSON
/// `metadata.pointOfContact`. Returns the six scalar POC offsets and five
/// address offsets, all `std::nullopt` when absent from the source object
/// -- one struct rather than eleven out-parameters, to keep the exact
/// creation order (contact_name, contact_type, role, phone, email, website,
/// THEN thoroughfare_number, thoroughfare_name, locality, postcode, country)
/// visible in one place, matching `to_point_of_contact`
/// (writer/serializer.rs:319-369) exactly. `address.postcode` wins over
/// `address.postalCode` when a document carries both (`.or_else` order).
struct PocOffsets {
    std::optional<::flatbuffers::Offset<::flatbuffers::String>> contact_name;
    std::optional<::flatbuffers::Offset<::flatbuffers::String>> contact_type;
    std::optional<::flatbuffers::Offset<::flatbuffers::String>> role;
    std::optional<::flatbuffers::Offset<::flatbuffers::String>> phone;
    std::optional<::flatbuffers::Offset<::flatbuffers::String>> email;
    std::optional<::flatbuffers::Offset<::flatbuffers::String>> website;
    std::optional<::flatbuffers::Offset<::flatbuffers::String>> address_thoroughfare_number;
    std::optional<::flatbuffers::Offset<::flatbuffers::String>> address_thoroughfare_name;
    std::optional<::flatbuffers::Offset<::flatbuffers::String>> address_locality;
    std::optional<::flatbuffers::Offset<::flatbuffers::String>> address_postcode;
    std::optional<::flatbuffers::Offset<::flatbuffers::String>> address_country;
};
PocOffsets to_point_of_contact(::flatbuffers::FlatBufferBuilder& fbb,
                               const nlohmann::ordered_json& poc);

/// Builds the whole `Header` table from the CityJSON metadata line (the
/// first line of a CityJSONSeq: `type`/`version`/`transform`/`metadata`/
/// `geometry-templates`/`appearance`/`extensions`), the writer options, and
/// the attribute schema(s). `attribute_indices_info` is `nullptr` before M6
/// exists / when no attribute indices were configured. Mirrors
/// `to_fcb_header` (writer/serializer.rs:52-231).
///
/// EVERY `fbb.CreateString`/`CreateVector` call this function makes --
/// directly or through a helper -- happens in the exact sequence Rust's own
/// `let` bindings run in, never as an inline call argument: FlatBuffers
/// builder calls are side-effecting and append to the buffer in call
/// order, so call order is part of the wire format (a real bug caught by
/// M3's byte-exact oracle, documented in feature_serializer.cpp).
///
/// The table itself is assembled via the generated `CreateHeader` free
/// function, not a hand-sequenced `HeaderBuilder`: flatc emits `Create*` in
/// a fixed, width-sorted `add_*` order (identical across every language
/// backend), and a field's byte offset WITHIN the table is determined by
/// that call order, not by which vtable slot it occupies. Building the
/// `HeaderBuilder` calls by hand -- even with every field present and
/// correct -- lays the table out differently from Rust's output and fails
/// byte-exactness despite decoding to identical values; this was a real
/// regression caught by this milestone's own oracle test
/// (test_writer_oracle.cpp).
::flatbuffers::Offset<::Header>
to_fcb_header(::flatbuffers::FlatBufferBuilder& fbb, const nlohmann::ordered_json& cj,
              const HeaderWriterOptions& options, const AttributeSchema& attr_schema,
              const AttributeSchema* semantic_attr_schema,
              const std::vector<AttributeIndexInfo>* attribute_indices_info);

}  // namespace fcb

#endif  // FCB_WITH_JSON
