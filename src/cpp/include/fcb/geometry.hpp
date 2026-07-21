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

/// The FlatBuffers `GeometryType` enumerators, mirrored here so a caller can
/// name a geometry type without pulling in the generated headers. The values
/// are the declaration order in `src/fbs/geometry.fbs` and are checked against
/// the generated enum with a static_assert in geometry.cpp.
enum class GeometryKind : std::uint8_t {
    MultiPoint = 0,
    MultiLineString = 1,
    MultiSurface = 2,
    CompositeSurface = 3,
    Solid = 4,
    MultiSolid = 5,
    CompositeSolid = 6,
    GeometryInstance = 7,
};

#ifdef FCB_WITH_JSON

// ---------------------------------------------------------------------------
// NESTING DEPTH COMES FROM THE GEOMETRY TYPE, NEVER FROM THE ARRAYS.
//
// The format stores a dimensional hierarchy as parallel count arrays:
// `solids[i]` = shells in solid i, `shells[i]` = surfaces in shell i,
// `surfaces[i]` = rings in surface i, `strings[i]` = vertex indices in ring i,
// and `indices` is the flat vertex-index list. Those arrays are AMBIGUOUS: a
// `Solid` with one shell and a `MultiSolid` with one solid flatten to
// byte-identical arrays, so no test over them can tell the two apart. Only
// `Geometry.type()` can, and every decoder below takes it.
//
// The depths, from `geomprimitives.schema.json` and CityJSON 2.0 §6 --
// identical to the table at the top of geom_decoder.rs:
//
//   type                               boundaries  semantics  material  texture
//   MultiPoint                                  1          1  forbidden forbidden
//   MultiLineString                             2          1  forbidden forbidden
//   MultiSurface, CompositeSurface              3          1          1        3
//   Solid                                       4          2          2        4
//   MultiSolid, CompositeSolid                  5          3          3        5
//
// An earlier version of this file dispatched on the outermost populated array
// and collapsed a single-element level into that element. That inference cost
// a nesting level on every one-solid MultiSolid and CompositeSolid, in both
// `material.values` and `texture.values`; see tests/conformance/inputs/
// appearance_depths.city.jsonl, which fails for any reader that infers.
// ---------------------------------------------------------------------------

/// Rebuild CityJSON's nested `boundaries` from the five flattened arrays, at
/// the depth `type` implies.
///
/// The encoder always writes one redundant count level above the geometry's
/// own depth (a `MultiSurface` carries a one-entry `shells`, a `Solid` a
/// one-entry `solids`), except for the 5-deep types. This reader ignores that
/// top count entirely; the old cascading reader used it as its depth signal,
/// which is where all the ambiguity came from.
///
/// Mirrors `decode_points`/`decode_rings`/`decode_surfaces`/`decode_shells`/
/// `decode_solids` (geom_decoder.rs:106-146), except for one DELIBERATE
/// divergence: a cursor overrun throws here, where the reference clamps and
/// yields a short array.
///
/// This is a choice, not a constraint. Clamping is perfectly safe in C++ --
/// the three appearance decoders below do exactly that, and do not throw.
/// We choose to error because count arrays that disagree with the index array
/// mean the file is corrupt, and reporting corruption is more useful to a
/// caller than a plausible-looking short geometry. Only reachable on a file
/// our own writer could not have produced.
///
/// The same reasoning settles an unknown `type`: it never reaches this
/// function, because `geometry_type_name` rejects it first. The `default:`
/// arm in the switch is a C++ formality with no fallback semantics -- see the
/// policy note at the top of cityjson.cpp.
nlohmann::json decode_boundaries(GeometryKind type,
                                 UIntView solids,
                                 UIntView shells,
                                 UIntView surfaces,
                                 UIntView strings,
                                 UIntView indices);

/// Rebuild `semantics.<...>.values` from the flat run of semantic indices.
///
/// `semantics.values` is one level shallower than the boundaries. A semantics
/// mapping carries no count arrays of its own, so the group sizes come from
/// the *boundary* `solids`/`shells`.
///
/// `UINT32_MAX` is `null` at the leaf. There is no wire encoding for a `null`
/// shell or solid in semantics; the encoder expands one to a `null` per
/// surface. Mirrors `decode_semantics` (geom_decoder.rs:226).
nlohmann::json decode_semantics_values(GeometryKind type,
                                       UIntView solids,
                                       UIntView shells,
                                       UIntView values);

/// Rebuild a `material.<theme>.values` array from a MaterialMapping.
///
/// Material indices sit two levels shallower than boundaries: one index per
/// SURFACE, not per ring. So there is no `surfaces`/`strings` argument --
/// `shells[i]` is the number of material indices in shell i.
///
/// `material.values` is nullable at EVERY level (verified against
/// `geomprimitives.schema.json`), so a `UINT32_MAX` entry in `shells` or
/// `solids` is a whole `null` shell or solid and comes back as JSON null,
/// never as an empty array.
///
/// Mirrors `decode_materials` (geom_decoder.rs:339). Unlike decode_boundaries
/// this NEVER throws: the reference reads a missing count as zero and clamps
/// every slice to the vertex array, so a mapping that over-claims yields
/// short or empty entries rather than an error.
nlohmann::json decode_material_values(GeometryKind type,
                                      UIntView solids,
                                      UIntView shells,
                                      UIntView vertices);

/// Rebuild a `texture.<theme>.values` array from a TextureMapping.
///
/// Texture values nest exactly as deeply as the boundaries -- solid, shell,
/// surface, ring -- except the innermost list is `[texture index, then one
/// UV-vertex index per ring vertex]` rather than vertex indices.
///
/// Unlike a material, `texture.values` is nullable ONLY at the leaf: the
/// schema types every intermediate level as a plain `"array"`. So nothing
/// here decodes an intermediate `null`.
///
/// Mirrors `decode_textures` (geom_decoder.rs:468). Also never throws; see
/// above.
nlohmann::json decode_texture_values(GeometryKind type,
                                     UIntView solids,
                                     UIntView shells,
                                     UIntView surfaces,
                                     UIntView strings,
                                     UIntView vertices);

#endif  // FCB_WITH_JSON

/// CityJSON name for a GeometryType enumerator, e.g. "MultiSurface".
///
/// Throws on a tag outside the eight the spec defines. CityJSON offers no
/// '+'-prefixed extension mechanism for geometry types (unlike City Object
/// types and semantic surface types, which get "+UnknownCityObject" and
/// "+GenericSurface" respectively), so there is no schema-valid string to
/// return and no honest alternative to an error. Mirrors
/// `GeometryType::to_cj` (geom_decoder.rs); see the policy note at the top of
/// cityjson.cpp.
std::string geometry_type_name(std::uint8_t type);

}  // namespace fcb
