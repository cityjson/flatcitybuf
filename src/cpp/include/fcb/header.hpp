#pragma once

#include <fcb/error.hpp>
#include <fcb/layout.hpp>
#include <fcb/range_reader.hpp>

#include <array>
#include <cstdint>
#include <memory>
#include <optional>
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

    /// Derived from Header.reference_system, so it has no absent-vs-empty
    /// distinction of its own to preserve: it is assembled from an authority
    /// plus a code, and stays empty when the header carries no reference
    /// system at all. (Its gating is upstream finding #20.10, still open.)
    std::string crs;
    /// `(required)` in header.fbs, so never absent in a valid file.
    std::string cityjson_version;

    /// Schema-optional strings, held as std::optional because the Rust oracle
    /// distinguishes ABSENT from PRESENT-BUT-EMPTY for every one of them:
    /// `identifier: header.identifier().map(|i| i.to_string())` and the same
    /// shape for reference_date/title (deserializer.rs:86-93), and `.map()`
    /// over the poc members (deserializer.rs:184-192). `Some("")` is emitted
    /// as `""`; only `None` omits the key. A plain std::string would flatten
    /// both cases to `""` and silently drop a key the oracle keeps (upstream
    /// finding #20.11).
    std::optional<std::string> identifier;
    std::optional<std::string> title;
    std::optional<std::string> reference_date;

    /// Point of contact, all optional (header.fbs:151-161). Presence of
    /// pointOfContact in CityJSON metadata hinges on poc_contact_name being
    /// SET -- not on it being non-empty -- matching Rust's
    /// `match header.poc_contact_name() { Some(_) => ..., None => None }`
    /// (deserializer.rs:81-84). `poc_email` is likewise required only in the
    /// presence sense: `poc_email().ok_or(...)` succeeds on `Some("")` and
    /// throws only on `None`, so `poc_email.has_value()` is the gate.
    std::optional<std::string> poc_contact_name;
    std::optional<std::string> poc_contact_type;
    std::optional<std::string> poc_role;
    std::optional<std::string> poc_phone;
    std::optional<std::string> poc_email;
    std::optional<std::string> poc_website;
    /// Address sub-object: each member is emitted iff NON-EMPTY, independently
    /// of the others, and the sub-object is omitted only when every member is
    /// empty (`to_cj_address`, deserializer.rs:195-216). Emptiness -- not
    /// presence -- IS the contract here, so these stay plain strings.
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
