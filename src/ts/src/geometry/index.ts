/** Geometry decoding -- ports `fcb_core::reader::geom_decoder` (Rust) via
 *  `src/cpp/src/geometry.cpp`. Boundaries, semantics and appearance values;
 *  CityJSON assembly itself lives in cityjson/.
 *
 *  This module's one invariant, stated once for all four files: THE NESTING
 *  DEPTH OF EVERYTHING HERE COMES FROM THE GEOMETRY TYPE. Nothing infers it
 *  from which of `solids`/`shells`/`surfaces`/`strings` happen to be
 *  populated -- that inference is upstream finding #8, and it is why a
 *  one-shell `Solid`'s materials and a single-string `MultiLineString`'s
 *  textures each came back one level too deep. */
import { ErrorCode, FcbError } from '../errors.js'

export { decodeBoundaries } from './boundaries.js'
export type { IndexTree, UInts } from './boundaries.js'
export {
  decodeSemantics, decodeSemanticsValues, semanticSurfaceTypeName,
} from './semantics.js'
export type { SemanticsValue, SemanticSurface } from './semantics.js'
export { decodeMaterialValues, decodeTextureValues, sharedMaterialValue } from './appearance.js'
export type { AppearanceValue } from './appearance.js'

/** The CityJSON names, in the declaration order of `GeometryType` in
 *  src/fbs/geometry.fbs. */
const GEOMETRY_TYPE_NAMES = [
  'MultiPoint', 'MultiLineString', 'MultiSurface', 'CompositeSurface',
  'Solid', 'MultiSolid', 'CompositeSolid', 'GeometryInstance',
] as const

/** UNKNOWN-TAG POLICY, the geometry site -- the one tag with no extension
 *  escape hatch.
 *
 *  Unlike a City Object type or a semantic surface type, a geometry type has
 *  no `+`-prefixed extension form: CityJSON section 3 enumerates exactly these
 *  eight `type` values and `geomprimitives.schema.json` admits no others, so
 *  there is no name a reader could legally emit for a tag it does not know.
 *  That leaves mislabelling the geometry or refusing the file, and this
 *  refuses: calling an unknown tag a `Solid` -- which the reference used to do
 *  -- decodes its boundaries at the wrong depth and hands the caller a
 *  plausible-looking lie. Both other readers throw here too
 *  (geom_decoder.rs `GeometryType::to_cj`, geometry.cpp `geometry_type_name`).
 *
 *  Callers name the type BEFORE decoding, so an unknown tag never reaches the
 *  decoders' `default:` arms. */
export function geometryTypeName(type: number): string {
  const name = GEOMETRY_TYPE_NAMES[type]
  if (name === undefined) {
    throw new FcbError(ErrorCode.InvalidFlatbuffer, `unknown geometry type ${type}`)
  }
  return name
}
