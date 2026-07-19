#pragma once

#include <cstdint>
#include <string>
#include <vector>

#include <fcb/error.hpp>
#include <fcb/span.hpp>

#ifdef FCB_WITH_JSON
#include <nlohmann/json.hpp>
#endif

namespace fcb {

using UIntView = span<const std::uint32_t>;

#ifdef FCB_WITH_JSON

/// Rebuild CityJSON's nested `boundaries` from the five flattened arrays.
///
/// The format stores a dimensional hierarchy as parallel count arrays:
/// `solids[i]` = shells in solid i, `shells[i]` = surfaces in shell i,
/// `surfaces[i]` = rings in surface i, `strings[i]` = vertex indices in
/// ring i, and `indices` is the flat vertex-index list. Which arrays are
/// populated determines the nesting depth, so the decoder dispatches on the
/// outermost non-empty array rather than on the geometry type.
///
/// Mirrors geom_decoder.rs:30-160, including its collapse rule: when a level
/// yields exactly one element, that element replaces the array rather than
/// being wrapped in it. Dropping that rule produces boundaries one level too
/// deep, which still looks structurally plausible.
///
/// Throws on any cursor overrun rather than reading out of bounds.
nlohmann::json decode_boundaries(UIntView solids,
                                 UIntView shells,
                                 UIntView surfaces,
                                 UIntView strings,
                                 UIntView indices);

#endif  // FCB_WITH_JSON

/// CityJSON name for a GeometryType enumerator, e.g. "MultiSurface".
std::string geometry_type_name(std::uint8_t type);

}  // namespace fcb
