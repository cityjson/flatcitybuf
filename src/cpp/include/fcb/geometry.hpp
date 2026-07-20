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

/// Rebuild a `material.<theme>.values` array from a MaterialMapping.
///
/// Material indices sit one level shallower than boundaries: one index per
/// SURFACE, not per ring. So there is no `surfaces`/`strings` argument --
/// `shells[i]` is the number of material indices in shell i.
///
/// Mirrors decode_materials (geom_decoder.rs:416). Unlike decode_boundaries
/// this NEVER throws: the reference stops walking when a count array runs
/// out and emits the short result, and that truncation is observable in the
/// output we must match.
nlohmann::json decode_material_values(UIntView solids, UIntView shells, UIntView vertices);

/// Rebuild a `texture.<theme>.values` array from a TextureMapping.
///
/// Texture values nest exactly like boundaries -- solid, shell, surface,
/// ring -- except the innermost list is (texture index, then one UV-vertex
/// index per ring vertex) rather than vertex indices.
///
/// Mirrors decode_textures (geom_decoder.rs:595), including its branch
/// order: which of the count arrays are populated selects the nesting, and
/// several branches special-case a length of one. Only the OUTERMOST level
/// collapses, and only in the solids branch. Also never throws; see above.
nlohmann::json decode_texture_values(UIntView solids,
                                     UIntView shells,
                                     UIntView surfaces,
                                     UIntView strings,
                                     UIntView vertices);

#endif  // FCB_WITH_JSON

/// CityJSON name for a GeometryType enumerator, e.g. "MultiSurface".
std::string geometry_type_name(std::uint8_t type);

}  // namespace fcb
