/** CityJSON `boundaries` from the five flattened count arrays -- ports
 *  `fcb::decode_boundaries` (src/cpp/src/geometry.cpp), itself a port of
 *  `decode_points`/`decode_rings`/`decode_surfaces`/`decode_shells`/
 *  `decode_solids` (src/rust/fcb_core/src/reader/geom_decoder.rs).
 *
 *  NESTING DEPTH COMES FROM THE GEOMETRY TYPE, NEVER FROM THE ARRAYS.
 *  `solids[i]` is the shell count of solid i, `shells[i]` the surface count of
 *  shell i, `surfaces[i]` the ring count of surface i, `strings[i]` the vertex
 *  count of ring i, and `indices` is the flat vertex-index list. Those arrays
 *  are AMBIGUOUS: a `Solid` with one shell and a `MultiSolid` with one solid
 *  flatten to byte-identical arrays, so no test over them can tell the two
 *  apart -- only `Geometry.type()` can. Inferring depth from which array is
 *  populated is what produced upstream finding #8; every such guard was
 *  deleted on both sides and must not come back.
 *
 *  The depths, from `geomprimitives.schema.json` and CityJSON 2.0 section 6:
 *
 *    MultiPoint                       1
 *    MultiLineString                  2
 *    MultiSurface, CompositeSurface   3
 *    Solid, GeometryInstance          4
 *    MultiSolid, CompositeSolid       5
 *
 *  The encoder writes one redundant count level above the geometry's own depth
 *  (a `MultiSurface` carries a one-entry `shells`, a `Solid` a one-entry
 *  `solids`), except for the 5-deep types. This reader ignores that top count
 *  entirely; the old cascading reader used it as its depth signal, which is
 *  where all the ambiguity came from. */
import { ErrorCode, FcbError } from '../errors.js'
import { GeometryType } from '../generated/geometry-type.js'

/** Any of the count arrays. FlatBuffers hands back a `Uint32Array`; the tests
 *  and other decoders pass plain arrays. */
export type UInts = Uint32Array | readonly number[]

/** CityJSON boundaries: vertex indices nested to the geometry's own depth. */
export type IndexTree = number | IndexTree[]

/** A count array disagreeing with the index array means the file is corrupt.
 *
 *  DELIBERATE DIVERGENCE from the Rust reference, which clamps and yields a
 *  short array (geom_decoder.rs `take_ring`). Clamping would be equally safe
 *  -- the appearance decoders in this module do exactly that -- so this is a
 *  choice about what a reader should tell its caller, not a constraint: a
 *  plausible-looking short geometry hides the corruption, an error reports
 *  it. Only reachable on a file our own writer could not have produced. */
function overrun(what: string): never {
  throw new FcbError(ErrorCode.InvalidFlatbuffer, `geometry boundaries overrun in ${what}`)
}

/** `counts[i]`, or an overrun error when the array has run out. Written as a
 *  helper because `noUncheckedIndexedAccess` makes every element access
 *  `number | undefined`, and the alternative is a non-null assertion that
 *  would silently produce `NaN`-shaped garbage if the bound were ever wrong. */
function at(counts: UInts, i: number, what: string): number {
  const v = counts[i]
  if (v === undefined) overrun(what)
  return v
}

/** Cursors into the parallel count arrays, shared across nesting levels: each
 *  `take*` consumes exactly as much as the level above asked for, so the same
 *  arrays rebuild any depth. */
class Cursor {
  private shell = 0
  private surface = 0
  private ring = 0
  private index = 0

  constructor(
    private readonly shells: UInts,
    private readonly surfaces: UInts,
    private readonly strings: UInts,
    private readonly indices: UInts,
  ) {}

  /** One ring: `strings[ring]` vertex indices taken from `indices`. */
  takeRing(): number[] {
    const size = at(this.strings, this.ring, 'strings')
    this.ring += 1

    if (this.index > this.indices.length || this.indices.length - this.index < size) {
      overrun('indices')
    }
    const ring: number[] = []
    for (let i = 0; i < size; i++) ring.push(at(this.indices, this.index + i, 'indices'))
    this.index += size
    return ring
  }

  /** One surface: `surfaces[surface]` rings. */
  takeSurface(): IndexTree[] {
    const rings = at(this.surfaces, this.surface, 'surfaces')
    this.surface += 1

    const surface: IndexTree[] = []
    for (let i = 0; i < rings; i++) surface.push(this.takeRing())
    return surface
  }

  /** One shell: `shells[shell]` surfaces. */
  takeShell(): IndexTree[] {
    const surfaces = at(this.shells, this.shell, 'shells')
    this.shell += 1

    const shell: IndexTree[] = []
    for (let i = 0; i < surfaces; i++) shell.push(this.takeSurface())
    return shell
  }
}

/** Rebuilds CityJSON's nested `boundaries` at the depth `type` implies.
 *
 *  An unknown `type` never reaches here: the caller names the type first with
 *  `geometryTypeName`, which throws (UNKNOWN-TAG POLICY, see index.ts).
 *  Reading an unknown tag as a `Solid` would decode its boundaries at the
 *  wrong depth and hand the caller a plausible-looking lie. */
export function decodeBoundaries(
  type: GeometryType,
  solids: UInts,
  shells: UInts,
  surfaces: UInts,
  strings: UInts,
  indices: UInts,
): IndexTree[] {
  const c = new Cursor(shells, surfaces, strings, indices)
  const out: IndexTree[] = []

  switch (type) {
    case GeometryType.MultiPoint:
      // Every index is a point of the one and only ring.
      for (let i = 0; i < indices.length; i++) out.push(at(indices, i, 'indices'))
      return out

    case GeometryType.MultiLineString:
      // One ring per `strings` entry, even when there is exactly ONE -- this
      // decoder's own instance of the depth-from-type rule above (never
      // infer depth from which array is populated). The guard that inferred
      // depth from `strings.length > 1` on this SAME shape was upstream
      // finding #8's texture half, but it lived in `decode_textures`, never
      // in this boundary decoder -- there is no `strings.length > 1` guard
      // to remove here because there never was one. `surfaces` holds one
      // redundant entry (== strings.length); it is ignored.
      for (let i = 0; i < strings.length; i++) out.push(c.takeRing())
      return out

    case GeometryType.MultiSurface:
    case GeometryType.CompositeSurface:
      // One surface per `surfaces` entry; `shells` is the redundant one. The
      // outermost level is never collapsed, not even for one surface.
      for (let i = 0; i < surfaces.length; i++) out.push(c.takeSurface())
      return out

    case GeometryType.MultiSolid:
    case GeometryType.CompositeSolid: {
      // `solids[i]` shells in the i-th solid. Nothing above it.
      for (let i = 0; i < solids.length; i++) {
        const n = at(solids, i, 'solids')
        const solid: IndexTree[] = []
        for (let k = 0; k < n; k++) solid.push(c.takeShell())
        out.push(solid)
      }
      return out
    }

    default:
      // Solid and GeometryInstance: one shell per `shells` entry; `solids` is
      // the redundant one, and is NOT consulted -- a Solid drops the solid
      // level even when `solids === [1]`. This decoder's own instance of the
      // depth-from-type rule above; the guard that inferred depth from
      // `solids.length == 1` on this SAME shape was upstream finding #8's
      // material half, but it lived in `decode_materials`, never in this
      // boundary decoder. Do not reintroduce a `solids[0] > 1` guard here.
      for (let i = 0; i < shells.length; i++) out.push(c.takeShell())
      return out
  }
}
