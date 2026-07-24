#pragma once

#include <fcb/error.hpp>
#include <fcb/layout.hpp>
#include <fcb/range_reader.hpp>

#include <array>
#include <cstdint>
#include <memory>
#include <string>
#include <vector>

// The generated FlatBuffers types live in the GLOBAL namespace (every
// `namespace FlatCityBuf;` in src/fbs/*.fbs is commented out). Forward
// declare rather than including the generated headers, so consumers of
// this public header never inherit them.
struct Header;

namespace fcb {

namespace detail {
struct HeaderAccess;
}

/// One attribute column's schema, copied out of the header.
struct ColumnInfo {
    std::uint16_t index;
    std::string name;
    std::uint8_t type;  // ::ColumnType, as its underlying ubyte
    bool nullable;
};

/// Where one column's B+tree index lives, and how it is shaped.
struct AttrIndexInfo {
    std::uint16_t column_index;
    std::uint32_t length;  // whole blob, INCLUDING its payload section
    std::uint16_t branching_factor;
    std::uint32_t num_unique_items;  // unique KEYS, not features
    std::uint64_t begin;             // absolute byte offset in the file
};

/// Everything a caller normally wants from the header, as owned values.
struct FileInfo {
    std::uint64_t features_count = 0;
    std::uint16_t index_node_size = 0;
    std::vector<ColumnInfo> columns;
    /// Schema for SemanticObject.attributes, which is separate from the
    /// feature attribute schema (Header.semantic_columns in header.fbs).
    std::vector<ColumnInfo> semantic_columns;

    bool has_extent = false;
    std::array<double, 6> geographical_extent{};  // minx,miny,minz,maxx,maxy,maxz

    bool has_transform = false;
    std::array<double, 3> scale{};
    std::array<double, 3> translate{};

    std::string crs;
    std::string cityjson_version;
    std::string identifier;
    std::string title;
    std::string reference_date;

    /// Point of contact, all optional (header.fbs:151-161). Presence of
    /// pointOfContact in CityJSON metadata hinges on poc_contact_name being
    /// set (deserializer.rs:77-78); the rest are its optional members.
    std::string poc_contact_name;
    std::string poc_contact_type;
    std::string poc_role;
    std::string poc_phone;
    std::string poc_email;
    /// Whether `poc_email` was actually present in the header, as opposed to
    /// absent. `poc_email` alone cannot tell those apart: a present-but-empty
    /// flatbuffer string and an absent one both flatten to `""`. This flag
    /// exists solely so the required-field check in
    /// `point_of_contact_to_json` (cityjson.cpp) can match Rust's
    /// `poc_email().ok_or(...)`, which succeeds on `Some("")`. It does not
    /// extend to the other `poc_*`/top-level string fields, whose
    /// absent-vs-empty conflation remains a disclosed, out-of-scope
    /// limitation (they only ever cause a silently omitted key, not a
    /// throw).
    bool has_poc_email = false;
    std::string poc_website;
    /// Address sub-object: emitted only when ALL FIVE are present AND the
    /// thoroughfare number parses as an integer (deserializer.rs:172-182,
    /// `to_cj_address`'s chained `?`/`and_then`).
    std::string poc_address_thoroughfare_number;
    std::string poc_address_thoroughfare_name;
    std::string poc_address_locality;
    std::string poc_address_postcode;
    std::string poc_address_country;
};

/// A parsed header that OWNS its backing bytes.
///
/// The raw ::Header pointer is deliberately not exposed: a caller could
/// retain it past this object's destruction and read freed memory. Internal
/// decoders reach it through detail::HeaderAccess.
class HeaderView {
  public:
    HeaderView() = default;

    const FileInfo& info() const { return info_; }
    const FileLayout& layout() const { return layout_; }
    const std::vector<AttrIndexInfo>& attr_indices() const { return attr_indices_; }

  private:
    friend struct detail::HeaderAccess;
    friend HeaderView read_header(std::shared_ptr<RangeReader> reader);

    const ::Header* raw() const;

    std::shared_ptr<const std::vector<std::uint8_t>> buffer_;
    FileInfo info_;
    FileLayout layout_{};
    std::vector<AttrIndexInfo> attr_indices_;
};

/// Read and validate the file preamble and header.
///
/// Takes SHARED ownership so it can construct its own per-query
/// BufferedRangeReader internally (over-fetching the header plus the top
/// R-tree levels in one request, as the Rust HTTP reader does).
HeaderView read_header(std::shared_ptr<RangeReader> reader);

}  // namespace fcb
