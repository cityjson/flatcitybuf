#pragma once

#include <cstdint>
#include <memory>
#include <string>
#include <vector>

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
    Feature(std::shared_ptr<const std::vector<std::uint8_t>> buffer,
            std::uint64_t byte_offset,
            std::size_t body_offset);

    /// True for a default-constructed Feature.
    bool empty() const { return buffer_ == nullptr; }

    /// The feature's CityJSON id. Empty when `empty()`.
    std::string id() const;

    /// How many CityObjects this feature carries. Zero when `empty()`.
    std::size_t city_object_count() const;

    /// Byte offset of this feature RELATIVE to the start of the features
    /// section, matching the offsets stored in the R-tree leaves.
    std::uint64_t byte_offset() const { return byte_offset_; }

private:
    friend struct detail::FeatureAccess;
    const ::CityFeature* raw() const;

    std::shared_ptr<const std::vector<std::uint8_t>> buffer_;
    std::uint64_t byte_offset_ = 0;
    std::size_t body_offset_ = 0;
};

}  // namespace fcb
