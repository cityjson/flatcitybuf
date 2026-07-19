#pragma once

#include <fcb/feature.hpp>

struct CityFeature;

namespace fcb {
namespace detail {

/// Internal gateway to Feature's generated FlatBuffers pointer.
/// See header_access.hpp for why this indirection exists.
struct FeatureAccess {
    static const ::CityFeature* get(const Feature& f);
};

}  // namespace detail
}  // namespace fcb
