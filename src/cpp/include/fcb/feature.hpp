#pragma once

#include <fcb/span.hpp>

#include <array>
#include <cstdint>
#include <memory>
#include <string>
#include <vector>

namespace fcb {
struct ColumnInfo;
}

// Generated FlatBuffers types are in the GLOBAL namespace. Forward declare
// so this public header does not drag them into consumers' scope.
struct CityFeature;

namespace fcb {

namespace detail {
struct FeatureAccess;
}

/// One decoded feature that OWNS the bytes it points into.
///
/// Copying a Feature copies a shared_ptr, not the buffer, so features stay
/// valid after the iterator and reader that produced them are destroyed.
///
/// The generated ::CityFeature pointer is deliberately not public: a caller
/// could retain it past this object's destruction. Internal decoders reach
/// it via detail::FeatureAccess.
class Feature {
  public:
    Feature() = default;
    Feature(std::shared_ptr<const std::vector<std::uint8_t>> buffer, std::uint64_t byte_offset,
            std::size_t body_offset);

    /// True for a default-constructed Feature.
    bool empty() const { return buffer_ == nullptr; }

    /// The feature's CityJSON id. Empty when `empty()`.
    std::string id() const;

    /// How many CityObjects this feature carries. Zero when `empty()`.
    std::size_t city_object_count() const;

    /// Raw attribute blob of CityObject `i`, or empty if it has none.
    /// Objects commonly differ: in the 3DBAG data a Building parent carries
    /// no attributes while its BuildingPart child carries them all.
    bytes_view object_attributes(std::size_t i) const;

    /// Whether CityObject `i` declares an attributes vector at all.
    /// Distinct from `object_attributes(i).empty()`: a present-but-empty
    /// vector is emitted as `"attributes": {}`, an absent one is omitted.
    bool object_has_attributes(std::size_t i) const;

    /// CityObject `i`'s own bounding box, if it declares one.
    /// Returns false when absent.
    bool object_extent(std::size_t i, std::array<double, 6>& out) const;

    /// CityObject `i`'s own column schema, if it declares one.
    ///
    /// feature.fbs documents CityObject.columns as overriding the header
    /// schema when set. Decoding with the wrong schema silently
    /// desynchronises the blob -- records are not self-delimiting -- so this
    /// must be consulted before decode_attributes(). Empty means "use the
    /// header's columns".
    /// Whether CityObject `i` DECLARES its own column schema. FlatBuffers
    /// presence is what selects the override, not vector non-emptiness: an
    /// explicitly empty schema must not silently fall back to the header's.
    bool object_has_columns(std::size_t i) const;

    std::vector<ColumnInfo> object_columns(std::size_t i) const;

    /// CityObject `i`'s id.
    std::string object_id(std::size_t i) const;

    /// Byte offset of this feature RELATIVE to the start of the features
    /// section, matching the offsets stored in the R-tree leaves.
    std::uint64_t byte_offset() const { return byte_offset_; }

    /// The generated CityFeature table behind this Feature.
    ///
    /// This is the supported way to reach the ENCODED geometry -- the format's
    /// own five count arrays (`solids`/`shells`/`surfaces`/`strings` plus the
    /// flat `boundaries` index list) and the quantised `vertices` they index
    /// into -- for analysis that does not want CityJSON built first. Nothing
    /// has to be nested or allocated to compute over them. See
    /// examples/geometry_analysis.cpp.
    ///
    /// NESTING DEPTH COMES FROM `Geometry::type()`, NEVER FROM THE ARRAYS: a
    /// Solid with one shell and a MultiSolid with one solid flatten to
    /// byte-identical arrays. Inferring depth from which array is populated is
    /// upstream finding #8.
    ///
    /// Include `<fcb/generated/feature_generated.h>` for the complete type;
    /// this header only forward-declares it.
    const ::CityFeature* raw() const;

  private:
    friend struct detail::FeatureAccess;

    std::shared_ptr<const std::vector<std::uint8_t>> buffer_;
    std::uint64_t byte_offset_ = 0;
    std::size_t body_offset_ = 0;
};

}  // namespace fcb
