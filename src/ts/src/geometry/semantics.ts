/** CityJSON `semantics` -- the surface list and the values that index into it.
 *  Ports `fcb::decode_semantics_values` (src/cpp/src/geometry.cpp) and the
 *  semantics block of `geometry_to_json` (src/cpp/src/cityjson.cpp:334-380),
 *  themselves ports of `decode_semantics` and `decode_semantics_surfaces`
 *  (src/rust/fcb_core/src/reader/geom_decoder.rs).
 *
 *  `semantics.values` is nested one level LESS deeply than the boundaries: one
 *  value per surface. A semantics mapping carries no count arrays of its own,
 *  so the group sizes come from the BOUNDARY `solids`/`shells` -- and the
 *  depth, as everywhere in this module, from the geometry type alone. */
import { ErrorCode, FcbError } from '../errors.js'
import { decodeAttributes } from '../feature/attribute.js'
import type { AttrValue } from '../feature/attribute.js'
import type { Geometry } from '../generated/geometry.js'
import { GeometryType } from '../generated/geometry-type.js'
import { SemanticObject } from '../generated/semantic-object.js'
import type { ColumnInfo } from '../header/file-info.js'
import type { UInts } from './boundaries.js'

/** `semantics.values`: surface indices nested to the geometry's depth, with
 *  `null` wherever the wire held the u32::MAX sentinel. */
export type SemanticsValue = number | null | SemanticsValue[]

/** One entry of `semantics.surfaces`. `type` is mandatory; `parent` and
 *  `children` are optional, and a surface's own attributes are merged in
 *  alongside them (CityJSON lets a semantic surface carry arbitrary extra
 *  members such as `slope` or `direction`). */
export interface SemanticSurface {
  type: string
  [key: string]: AttrValue | number[] | undefined
}

/** The CityJSON names, in the declaration order of `SemanticSurfaceType` in
 *  src/fbs/geometry.fbs. `ExtraSemanticSurface`, the last enumerator, is
 *  deliberately absent -- see semanticSurfaceTypeName. */
const SURFACE_TYPE_NAMES = [
  'RoofSurface', 'GroundSurface', 'WallSurface', 'ClosureSurface',
  'OuterCeilingSurface', 'OuterFloorSurface', 'Window', 'Door',
  'InteriorWallSurface', 'CeilingSurface', 'FloorSurface',
  'WaterSurface', 'WaterGroundSurface', 'WaterClosureSurface',
  'TrafficArea', 'AuxiliaryTrafficArea', 'TransportationMarking',
  'TransportationHole',
] as const

/** The CityJSON spelling of a semantic surface tag this reader cannot name. */
const GENERIC_SURFACE = '+GenericSurface'

/** u32::MAX is the wire spelling of "no index here" and becomes `null`, not
 *  4294967295.
 *
 *  Compared with `=== 0xffffffff` and never with `| 0` or `~v`: JS bitwise
 *  operators are 32-bit SIGNED, so `4294967295 | 0 === -1` and any such test
 *  turns a real index into a sentinel or the other way round.
 *
 *  Exported because appearance.ts reuses this SAME wire constant under its
 *  own local name (`NULL_COUNT`) for a different role: a count-array entry
 *  meaning "this whole shell/solid is null", not an index-array entry
 *  meaning "no index". One `0xffffffff` literal, two names for its two
 *  jobs. */
export const NULL_INDEX = 0xffffffff

export function indexOrNull(v: number): number | null {
  return v === NULL_INDEX ? null : v
}

/** `counts[i]`, or 0 when the array has run out.
 *
 *  The reference reads every count array with `.get(cursor).unwrap_or(0)`, so
 *  a mapping that claims more shells than it stores yields EMPTY entries at
 *  the right positions rather than losing them. Unlike the boundary decoder,
 *  the semantics and appearance decoders CLAMP rather than throw. */
export function countAt(counts: UInts, i: number): number {
  return counts[i] ?? 0
}

/** UNKNOWN-TAG POLICY, the semantic-surface site.
 *
 *  Unlike a geometry type, a semantic surface type HAS an extension escape
 *  hatch -- CityJSON section 3.3 says "it is possible to define and use other
 *  semantics, but these have to start with a `+`" -- so a tag with no name of
 *  its own still has a schema-valid spelling available and this emits one
 *  rather than throwing. `"+GenericSurface"` and not `"ExtraSemanticSurface"`:
 *  the latter is the FlatBuffers enumerator name, is not a CityJSON surface
 *  type and carries no `+`, so a document containing it fails validation. */
export function semanticSurfaceTypeName(type: number): string {
  return SURFACE_TYPE_NAMES[type] ?? GENERIC_SURFACE
}

/** Regroups the flat run of semantic values at the depth `type` implies. */
export function decodeSemanticsValues(
  type: GeometryType,
  solids: UInts,
  shells: UInts,
  values: UInts,
): SemanticsValue[] {
  let cursor = 0
  const take = (n: number): SemanticsValue[] => {
    const end = Math.min(cursor + n, values.length)
    const out: SemanticsValue[] = []
    for (; cursor < end; cursor++) out.push(indexOrNull(countAt(values, cursor)))
    return out
  }

  const out: SemanticsValue[] = []
  switch (type) {
    case GeometryType.Solid:
      // One array per shell. The solid level is dropped even when
      // `solids === [1]`; `solids` is not consulted at all here.
      for (let i = 0; i < shells.length; i++) out.push(take(countAt(shells, i)))
      return out

    case GeometryType.MultiSolid:
    case GeometryType.CompositeSolid: {
      // One array per shell, per solid -- one level deeper than a Solid built
      // from the very same arrays.
      let shellCursor = 0
      for (let i = 0; i < solids.length; i++) {
        const solid: SemanticsValue[] = []
        const n = countAt(solids, i)
        for (let k = 0; k < n; k++) solid.push(take(countAt(shells, shellCursor++)))
        out.push(solid)
      }
      return out
    }

    default:
      // MultiPoint, MultiLineString, MultiSurface, CompositeSurface: one value
      // per surface, flat. A GeometryInstance carries no semantics of its own;
      // its template does, so the reference reads that flat too.
      for (let i = 0; i < values.length; i++) out.push(indexOrNull(countAt(values, i)))
      return out
  }
}

/** True iff the table actually stores the field, as opposed to omitting it and
 *  letting the accessor return a default. The generated `*Length()` accessors
 *  return 0 for "absent" AND for "present but empty", and this module has to
 *  tell those apart (see decodeSemantics). Field n lives at vtable slot
 *  4 + 2n; `__offset` is the generated code's own presence primitive, which
 *  every generated getter above starts with. */
function fieldPresent(table: { bb: { __offset(pos: number, slot: number): number } | null; bb_pos: number }, slot: number): boolean {
  const bb = table.bb
  if (bb === null) throw new FcbError(ErrorCode.InvalidFlatbuffer, 'unbound FlatBuffers table')
  return bb.__offset(table.bb_pos, slot) !== 0
}

/** `Geometry.semantics_objects` is field 8 (src/fbs/geometry.fbs). */
const GEOMETRY_SEMANTICS_OBJECTS_SLOT = 20
/** `Geometry.semantics` is field 7. */
const GEOMETRY_SEMANTICS_SLOT = 18
/** `SemanticObject.children` is field 2. */
const SEMANTIC_OBJECT_CHILDREN_SLOT = 8

/** Rebuilds a geometry's whole `semantics` member, or `null` when there is
 *  none.
 *
 *  ABSENT-VS-EMPTY, twice over:
 *   * The SURFACES decide whether there is a `semantics` member at all. A
 *     present-but-EMPTY surface vector is `{"surfaces": [], "values": ...}`,
 *     not a missing member -- the writer emits exactly that shape, so a
 *     `length > 0` guard here drops the whole member.
 *   * An absent VALUES vector is `"values": null`: a member present with a
 *     null value, which the schema requires and permits. It is not a missing
 *     member and not `[]`, which is what a present-but-empty vector gives. */
export function decodeSemantics(
  geometry: Geometry,
  semanticColumns: readonly ColumnInfo[] | null,
): { surfaces: SemanticSurface[]; values: SemanticsValue[] | null } | null {
  if (!fieldPresent(geometry, GEOMETRY_SEMANTICS_OBJECTS_SLOT)) return null

  const surfaces: SemanticSurface[] = []
  for (let i = 0; i < geometry.semanticsObjectsLength(); i++) {
    const so = geometry.semanticsObjects(i, new SemanticObject())
    if (so === null) continue

    // A present `extension_type` string wins verbatim over the enum name; the
    // spec requires it to start with '+'.
    const extension = so.extensionType()
    const surface: SemanticSurface = {
      type: extension ?? semanticSurfaceTypeName(so.type()),
    }

    // Semantic surfaces carry their own attributes, decoded against
    // Header.semantic_columns -- a schema separate from the feature attribute
    // columns. Merged inline, as the reference does.
    //
    // `null` (no such schema declared) is NOT `[]` (a declared, empty one):
    // the reference DROPS a surface's attributes when the schema is `None`
    // (`semantic_attr_schema.as_ref().and_then(...)`, geom_decoder.rs:162-164)
    // rather than failing on the first index it cannot resolve, while a
    // declared schema that does not cover an index is corruption and throws.
    // Passing `[]` for an undeclared schema would turn the first case into
    // the second.
    if (semanticColumns !== null) {
      const attributes = so.attributesArray()
      if (attributes !== null) {
        for (const [k, v] of Object.entries(decodeAttributes(attributes, semanticColumns))) {
          surface[k] = v
        }
      }
    }

    // `parent` is an OPTIONAL scalar: the generated accessor returns null when
    // absent, and `0` is a real parent index. Checked with `!== null`, never
    // for truthiness. (The C++ reader omits `parent` entirely; the Rust
    // reference emits it, and so does this port -- see the task report.)
    const parent = so.parent()
    if (parent !== null) surface['parent'] = parent

    // Presence, not emptiness: an absent children vector is no `children` key,
    // a present one is a key even when empty (`s.children().map(...)` in
    // decode_semantics_surfaces).
    if (fieldPresent(so, SEMANTIC_OBJECT_CHILDREN_SLOT)) {
      const children: number[] = []
      for (let k = 0; k < so.childrenLength(); k++) children.push(so.children(k) ?? 0)
      surface['children'] = children
    }

    surfaces.push(surface)
  }

  if (!fieldPresent(geometry, GEOMETRY_SEMANTICS_SLOT)) return { surfaces, values: null }

  return {
    surfaces,
    values: decodeSemanticsValues(
      geometry.type(),
      geometry.solidsArray() ?? [],
      geometry.shellsArray() ?? [],
      geometry.semanticsArray() ?? [],
    ),
  }
}
