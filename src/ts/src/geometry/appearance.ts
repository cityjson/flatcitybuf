/** `material.<theme>.values` and `texture.<theme>.values` -- ports
 *  `fcb::decode_material_values` and `fcb::decode_texture_values`
 *  (src/cpp/src/geometry.cpp), themselves ports of `decode_materials` and
 *  `decode_textures` (src/rust/fcb_core/src/reader/geom_decoder.rs).
 *
 *  Depth comes from the geometry type, exactly as in boundaries.ts, and for
 *  the same reason: these two decoders are where upstream finding #8 lived.
 *  Material indices sit TWO levels shallower than the boundaries (one index
 *  per surface, so there is no `surfaces`/`strings` argument); texture indices
 *  sit at the same depth as the boundaries, with `[texture index, uv index...]`
 *  at the leaf.
 *
 *  Both decoders CLAMP when a count array over-claims, rather than throwing as
 *  the boundary decoder does; that difference is deliberate and mirrored from
 *  the reference. */
import { GeometryType } from '../generated/geometry-type.js'
import type { UInts } from './boundaries.js'
import { countAt, indexOrNull } from './semantics.js'

/** An appearance values array: indices nested to the geometry's depth, with
 *  `null` for the u32::MAX sentinel at any level that permits it. */
export type AppearanceValue = number | null | AppearanceValue[]

/** The u32::MAX sentinel, spelled out for the count arrays, where it means a
 *  whole null shell or solid rather than a null index. Compared with `===`;
 *  see indexOrNull for why `| 0` and `~v` are banned. */
const NULL_COUNT = 0xffffffff

/** Reads the OPTIONAL shared-material scalar off a `MaterialMapping`.
 *
 *  A `value` colours the whole object and has no depth at all; it is a
 *  DIFFERENT field from the `vertices` values vector `decodeMaterialValues`
 *  reads (src/fbs/geometry.fbs:51). Returns `undefined` when the field is
 *  absent and `0` when it is present and zero -- a distinction
 *  `if (m.value())` destroys, because a shared material index of 0 is
 *  perfectly real and falsy in JS. Optional scalars are checked with
 *  `!== null`, never for truthiness. */
export function sharedMaterialValue(m: { value(): number | null }): number | undefined {
  const v = m.value()
  return v === null ? undefined : v
}

/** Rebuilds `material.<theme>.values` at the depth `type` implies.
 *
 *  `material.values` is nullable at EVERY level (geomprimitives.schema.json),
 *  and a u32::MAX entry in `shells` or `solids` is how the format says so: it
 *  comes back as `null`, never as an empty array. */
export function decodeMaterialValues(
  type: GeometryType,
  solids: UInts,
  shells: UInts,
  vertices: UInts,
): AppearanceValue[] {
  let vertex = 0
  const takeShell = (n: number): AppearanceValue[] => {
    const end = Math.min(vertex + n, vertices.length)
    const out: AppearanceValue[] = []
    for (; vertex < end; vertex++) out.push(indexOrNull(countAt(vertices, vertex)))
    return out
  }

  const out: AppearanceValue[] = []
  switch (type) {
    case GeometryType.Solid:
      // One array per shell; a u32::MAX count is a whole null shell. The
      // solid level is dropped even when `solids === [1]` -- `solids` is not
      // read here at all. Do NOT reintroduce a `solids[0] > 1` guard: that
      // guard IS finding #8, and it sent every one-shell Solid down the
      // MultiSolid branch, a level too deep.
      for (let i = 0; i < shells.length; i++) {
        const n = countAt(shells, i)
        out.push(n === NULL_COUNT ? null : takeShell(n))
      }
      return out

    case GeometryType.MultiSolid:
    case GeometryType.CompositeSolid: {
      // One array per shell, per solid -- one level deeper than a Solid built
      // from byte-identical arrays. Null at either level.
      let shellCursor = 0
      for (let i = 0; i < solids.length; i++) {
        const count = countAt(solids, i)
        if (count === NULL_COUNT) {
          out.push(null)
          continue
        }
        const solid: AppearanceValue[] = []
        for (let k = 0; k < count; k++) {
          const n = countAt(shells, shellCursor++)
          solid.push(n === NULL_COUNT ? null : takeShell(n))
        }
        out.push(solid)
      }
      return out
    }

    default:
      // MultiSurface and CompositeSurface get one index per surface.
      // MultiPoint, MultiLineString and GeometryInstance cannot carry a
      // material at all; if one is somehow present it has no depth of its own,
      // so it is read as the shallowest thing it could be.
      for (let i = 0; i < vertices.length; i++) out.push(indexOrNull(countAt(vertices, i)))
      return out
  }
}

/** The texture equivalent of the boundary cursor: the same four count arrays,
 *  but the leaf holds `[texture index, uv index, ...]`, is nullable, and a
 *  missing count reads as zero instead of throwing. */
class TextureCursor {
  private shell = 0
  private surface = 0
  private string = 0
  private vertex = 0

  constructor(
    private readonly shells: UInts,
    private readonly surfaces: UInts,
    private readonly strings: UInts,
    private readonly vertices: UInts,
  ) {}

  takeRing(): AppearanceValue[] {
    const size = countAt(this.strings, this.string++)
    const end = Math.min(this.vertex + size, this.vertices.length)
    const ring: AppearanceValue[] = []
    for (; this.vertex < end; this.vertex++) {
      ring.push(indexOrNull(countAt(this.vertices, this.vertex)))
    }
    return ring
  }

  takeSurface(): AppearanceValue[] {
    const rings = countAt(this.surfaces, this.surface++)
    const out: AppearanceValue[] = []
    for (let i = 0; i < rings; i++) out.push(this.takeRing())
    return out
  }

  takeShell(): AppearanceValue[] {
    const n = countAt(this.shells, this.shell++)
    const out: AppearanceValue[] = []
    for (let i = 0; i < n; i++) out.push(this.takeSurface())
    return out
  }
}

/** Rebuilds `texture.<theme>.values` at the depth `type` implies. */
export function decodeTextureValues(
  type: GeometryType,
  solids: UInts,
  shells: UInts,
  surfaces: UInts,
  strings: UInts,
  vertices: UInts,
): AppearanceValue[] {
  const c = new TextureCursor(shells, surfaces, strings, vertices)

  const out: AppearanceValue[] = []
  switch (type) {
    case GeometryType.MultiSurface:
    case GeometryType.CompositeSurface:
      // Per surface, per ring.
      for (let i = 0; i < surfaces.length; i++) out.push(c.takeSurface())
      return out

    case GeometryType.Solid:
      // ... per shell.
      for (let i = 0; i < shells.length; i++) out.push(c.takeShell())
      return out

    case GeometryType.MultiSolid:
    case GeometryType.CompositeSolid:
      // ... per solid.
      for (let i = 0; i < solids.length; i++) {
        const n = countAt(solids, i)
        const solid: AppearanceValue[] = []
        for (let k = 0; k < n; k++) solid.push(c.takeShell())
        out.push(solid)
      }
      return out

    default: {
      // MultiPoint, MultiLineString and GeometryInstance cannot carry a
      // texture; read whatever is there at the shallowest legal depth. A
      // single-string MultiLineString keeps its depth here -- no
      // `strings.length > 1` guard, which is the texture half of finding #8.
      // The `max(1)` is the reference's (geom_decoder.rs:542) and is what
      // makes a textureless count array still produce one empty surface.
      const n = Math.max(surfaces.length, 1)
      for (let i = 0; i < n; i++) out.push(c.takeSurface())
      return out
    }
  }
}
