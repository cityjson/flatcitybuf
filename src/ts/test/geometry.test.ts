import * as flatbuffers from 'flatbuffers'
import { describe, expect, it } from 'vitest'
import { FcbError } from '../src/errors.js'
import type { ColumnInfo } from '../src/header/index.js'
import { ColumnType } from '../src/generated/column-type.js'
import { Geometry } from '../src/generated/geometry.js'
import { GeometryType } from '../src/generated/geometry-type.js'
import { SemanticObject } from '../src/generated/semantic-object.js'
import { SemanticSurfaceType } from '../src/generated/semantic-surface-type.js'
import {
  decodeBoundaries,
  decodeMaterialValues,
  decodeSemantics,
  decodeSemanticsValues,
  decodeTextureValues,
  geometryTypeName,
  semanticSurfaceTypeName,
  sharedMaterialValue,
} from '../src/geometry/index.js'

// EVERY expected value below is pinned from src/cpp/tests/test_geometry.cpp,
// whose own values were produced by RUNNING the Rust reference through a
// temporary `oracle_dump` test (see that file's header comment). Nothing here
// is hand-derived, and nothing here was read back out of this port's own
// output. The C++ test case each block came from is named above it.
//
// The one place this file deviates from the brief's sample test block: every
// decoder takes the GEOMETRY TYPE. The brief's snippet infers depth from which
// count arrays are populated, which is precisely the inference finding #8
// deleted -- a Solid and a one-solid MultiSolid flatten to byte-identical
// arrays, so no test over those arrays can tell them apart. See the task
// report.

const NONE = 0xffffffff // u32::MAX, the "no index here" sentinel

// ------------------------------------------------------------- boundaries ---

// C++: "MultiPoint boundaries are the flat index list"
describe('boundaries: MultiPoint', () => {
  it('is the flat index list', () => {
    expect(decodeBoundaries(GeometryType.MultiPoint, [], [], [], [], [0, 1, 2]))
      .toEqual([0, 1, 2])
  })
})

// C++: "MultiLineString boundaries are one ring per string"
describe('boundaries: MultiLineString', () => {
  it('keeps the depth of a SINGLE string', () => {
    // A single string is the interesting case: by the arrays alone it is
    // indistinguishable from a one-ring MultiSurface. No `strings.length > 1`
    // guard here -- that guard, on the SAME shape, was finding #8's texture
    // half, but it lived in `decode_textures`, never in this boundary
    // decoder.
    expect(decodeBoundaries(GeometryType.MultiLineString, [], [], [], [4], [0, 1, 2, 3]))
      .toEqual([[0, 1, 2, 3]])
  })

  it('is one ring per string', () => {
    expect(decodeBoundaries(
      GeometryType.MultiLineString, [], [], [], [3, 3], [0, 1, 2, 3, 4, 5],
    )).toEqual([[0, 1, 2], [3, 4, 5]])
  })
})

// C++: "MultiSurface boundaries are three levels deep, never collapsed"
//      "a MultiSurface with an inner ring keeps both rings in one surface"
describe('boundaries: MultiSurface and CompositeSurface', () => {
  it('is three levels deep, never collapsed', () => {
    // The old decoder collapsed the outermost single-element level and
    // returned [[0,1,2]] -- two levels, one short.
    expect(decodeBoundaries(GeometryType.MultiSurface, [], [], [1], [3], [0, 1, 2]))
      .toEqual([[[0, 1, 2]]])
    expect(decodeBoundaries(GeometryType.CompositeSurface, [], [], [1], [3], [0, 1, 2]))
      .toEqual([[[0, 1, 2]]])
  })

  it('keeps an inner ring in the same surface as its outer ring', () => {
    expect(decodeBoundaries(
      GeometryType.MultiSurface, [], [], [2], [4, 3], [0, 1, 2, 3, 10, 11, 12],
    )).toEqual([[[0, 1, 2, 3], [10, 11, 12]]])
  })
})

// C++: "Solid boundaries are four levels deep and ignore the solids array"
//      "the SAME arrays give a MultiSolid one more level than a Solid"
//      "two solids each keep their own shell list"
describe('boundaries: the solid types', () => {
  const solids = [1]
  const shells = [2]
  const surfaces = [1, 1]
  const strings = [3, 3]
  const idx = [0, 1, 2, 3, 4, 5]

  it('reads a Solid four levels deep and ignores the redundant solids array', () => {
    expect(decodeBoundaries(GeometryType.Solid, solids, shells, surfaces, strings, idx))
      .toEqual([[[[0, 1, 2]], [[3, 4, 5]]]])
  })

  it('gives a MultiSolid one more level than a Solid from the SAME arrays', () => {
    // The boundary decoder's own instance of the depth-from-type rule, in
    // one assertion -- not finding #8 itself, which was material/texture-
    // specific, but the same ambiguity it exploited. `solids === [1]` is
    // what both shapes write; only the type tells them apart.
    expect(decodeBoundaries(GeometryType.MultiSolid, solids, shells, surfaces, strings, idx))
      .toEqual([[[[[0, 1, 2]], [[3, 4, 5]]]]])
    expect(decodeBoundaries(GeometryType.CompositeSolid, solids, shells, surfaces, strings, idx))
      .toEqual([[[[[0, 1, 2]], [[3, 4, 5]]]]])
  })

  it('gives each of two solids its own shell list', () => {
    expect(decodeBoundaries(
      GeometryType.CompositeSolid, [1, 1], [1, 1], [1, 1], [3, 3], [0, 1, 2, 3, 4, 5],
    )).toEqual([[[[[0, 1, 2]]]], [[[[3, 4, 5]]]]])
  })
})

// C++: "a ring claiming more indices than exist throws"
//      "a surface claiming more rings than exist throws"
describe('boundaries: a corrupt count array', () => {
  it('throws when a ring claims more indices than exist', () => {
    // A DELIBERATE divergence from the Rust reference, which clamps: count
    // arrays that disagree with the index array mean the file is corrupt, and
    // reporting that is more useful than a plausible-looking short geometry.
    expect(() => decodeBoundaries(GeometryType.MultiLineString, [], [], [], [99], [0, 1, 2]))
      .toThrow(FcbError)
  })

  it('throws when a surface claims more rings than exist', () => {
    expect(() => decodeBoundaries(GeometryType.MultiSurface, [], [], [5], [3], [0, 1, 2]))
      .toThrow(FcbError)
  })
})

// -------------------------------------------------------- semantics values ---

// C++: "semantics values are flat for every surface-level type"
describe('semantics values: the surface-level types', () => {
  it('are flat, with u32::MAX as null', () => {
    for (const t of [
      GeometryType.MultiPoint, GeometryType.MultiLineString,
      GeometryType.MultiSurface, GeometryType.CompositeSurface,
    ]) {
      expect(decodeSemanticsValues(t, [], [], [0, NONE, 1]), GeometryType[t])
        .toEqual([0, null, 1])
    }
  })
})

// C++: "a Solid groups semantics values by shell"
//      "the SAME arrays give MultiSolid semantics one more level than Solid"
//      "a CompositeSolid groups semantics values by shell, then by solid"
describe('semantics values: the solid types', () => {
  it('groups a Solid by shell', () => {
    expect(decodeSemanticsValues(GeometryType.Solid, [2], [2, 1], [0, NONE, 1]))
      .toEqual([[0, null], [1]])
  })

  it('gives a MultiSolid one more level than a Solid from the SAME arrays', () => {
    expect(decodeSemanticsValues(GeometryType.Solid, [1], [2], [0, 1])).toEqual([[0, 1]])
    expect(decodeSemanticsValues(GeometryType.MultiSolid, [1], [2], [0, 1])).toEqual([[[0, 1]]])
    expect(decodeSemanticsValues(GeometryType.CompositeSolid, [1], [2], [0, 1]))
      .toEqual([[[0, 1]]])
  })

  it('groups a CompositeSolid by shell, then by solid', () => {
    expect(decodeSemanticsValues(GeometryType.CompositeSolid, [2, 1], [1, 1, 1], [0, 1, NONE]))
      .toEqual([[[0], [1]], [[null]]])
  })
})

// C++: "semantics values clamp rather than throw when the counts over-claim"
describe('semantics values: over-claiming counts', () => {
  it('clamps when the values run out inside a shell', () => {
    expect(decodeSemanticsValues(GeometryType.Solid, [2], [3, 3], [1, 2]))
      .toEqual([[1, 2], []])
  })

  it('keeps an empty shell for a trailing solid whose shells ran out', () => {
    expect(decodeSemanticsValues(GeometryType.MultiSolid, [1, 1], [1], [9]))
      .toEqual([[[9]], [[]]])
  })
})

// ------------------------------------------------------ semantics surfaces ---

/** Builds a real `Geometry` table so `decodeSemantics` is exercised through
 *  the generated accessors -- including the ABSENT-vs-EMPTY vector cases,
 *  which no plain array can express. */
function buildGeometry(spec: {
  type: GeometryType
  solids?: readonly number[]
  shells?: readonly number[]
  semantics?: readonly number[]
  objects?: ReadonlyArray<{
    type: SemanticSurfaceType
    children?: readonly number[]
    parent?: number
    extensionType?: string
    attributes?: Uint8Array
  }>
}): Geometry {
  const b = new flatbuffers.Builder(1024)

  let objectsOffset: number | null = null
  if (spec.objects !== undefined) {
    const offsets = spec.objects.map((o) => {
      const ext = o.extensionType === undefined ? 0 : b.createString(o.extensionType)
      const attrs = o.attributes === undefined
        ? 0
        : SemanticObject.createAttributesVector(b, o.attributes)
      const kids = o.children === undefined
        ? 0
        : SemanticObject.createChildrenVector(b, [...o.children])
      SemanticObject.startSemanticObject(b)
      SemanticObject.addType(b, o.type)
      if (attrs !== 0) SemanticObject.addAttributes(b, attrs)
      if (kids !== 0) SemanticObject.addChildren(b, kids)
      if (o.parent !== undefined) SemanticObject.addParent(b, o.parent)
      if (ext !== 0) SemanticObject.addExtensionType(b, ext)
      return SemanticObject.endSemanticObject(b)
    })
    objectsOffset = Geometry.createSemanticsObjectsVector(b, offsets)
  }

  const solids = spec.solids === undefined
    ? null
    : Geometry.createSolidsVector(b, [...spec.solids])
  const shells = spec.shells === undefined
    ? null
    : Geometry.createShellsVector(b, [...spec.shells])
  const values = spec.semantics === undefined
    ? null
    : Geometry.createSemanticsVector(b, [...spec.semantics])

  Geometry.startGeometry(b)
  Geometry.addType(b, spec.type)
  if (solids !== null) Geometry.addSolids(b, solids)
  if (shells !== null) Geometry.addShells(b, shells)
  if (values !== null) Geometry.addSemantics(b, values)
  if (objectsOffset !== null) Geometry.addSemanticsObjects(b, objectsOffset)
  b.finish(Geometry.endGeometry(b))
  return Geometry.getRootAsGeometry(b.dataBuffer())
}

/** An `Int` attribute record: u16 column index, then the i32 value. */
function intAttribute(columnIndex: number, value: number): Uint8Array {
  const blob = new Uint8Array(6)
  const dv = new DataView(blob.buffer)
  dv.setUint16(0, columnIndex, true)
  dv.setInt32(2, value, true)
  return blob
}

const SEMANTIC_COLUMNS: ColumnInfo[] = [
  { index: 0, name: 'slope', type: ColumnType.Int, nullable: true },
]

describe('decodeSemantics', () => {
  it('returns null when the surface vector is ABSENT -- no semantics member', () => {
    // The SURFACES decide whether there is a `semantics` member at all
    // (cityjson.cpp:341, deserializer.rs:699).
    expect(decodeSemantics(buildGeometry({ type: GeometryType.MultiSurface }), [])).toBeNull()
  })

  it('is a semantics member with no surfaces when the vector is present but EMPTY', () => {
    const g = buildGeometry({ type: GeometryType.MultiSurface, objects: [] })
    expect(decodeSemantics(g, [])).toEqual({ surfaces: [], values: null })
  })

  it('reports an absent values vector as null, not as an empty array', () => {
    // ABSENT-VS-EMPTY: `"values": null` is a member with a null value, not a
    // missing member and not `[]` (cityjson.cpp:373).
    const g = buildGeometry({
      type: GeometryType.MultiSurface,
      objects: [{ type: SemanticSurfaceType.RoofSurface }],
    })
    expect(decodeSemantics(g, [])).toEqual({ surfaces: [{ type: 'RoofSurface' }], values: null })
  })

  it('decodes surfaces, their attributes and children alongside the values', () => {
    const g = buildGeometry({
      type: GeometryType.MultiSurface,
      semantics: [0, NONE, 1],
      objects: [
        { type: SemanticSurfaceType.RoofSurface, attributes: intAttribute(0, 42), children: [1] },
        { type: SemanticSurfaceType.WallSurface, parent: 0 },
      ],
    })
    expect(decodeSemantics(g, SEMANTIC_COLUMNS)).toEqual({
      surfaces: [
        { type: 'RoofSurface', slope: 42, children: [1] },
        { type: 'WallSurface', parent: 0 },
      ],
      values: [0, null, 1],
    })
  })

  it('keeps a PRESENT-but-EMPTY children vector as `children: []`, not as an absent key', () => {
    // Pins a DELIBERATE divergence from the C++ reader, which guards on
    // `size() > 0` (cityjson.cpp:359) and so drops `children` entirely for
    // this exact input. This port follows the Rust reference instead, which
    // keys on FIELD PRESENCE alone (`s.children().map(...)`,
    // geom_decoder.rs:158-160): a present-but-empty vector still becomes
    // `Some(vec![])`, i.e. a `children` key with an empty array. The
    // `children: [1]` case above cannot pin this choice -- a `size() > 0`
    // guard would accept a non-empty vector the same way the presence check
    // does, so only an EMPTY-but-present vector tells the two rules apart.
    const g = buildGeometry({
      type: GeometryType.MultiSurface,
      objects: [{ type: SemanticSurfaceType.RoofSurface, children: [] }],
    })
    expect(decodeSemantics(g, [])).toEqual({
      surfaces: [{ type: 'RoofSurface', children: [] }],
      values: null,
    })
  })

  it('prefers a surface extension type over the enum name', () => {
    const g = buildGeometry({
      type: GeometryType.MultiSurface,
      objects: [{ type: SemanticSurfaceType.ExtraSemanticSurface, extensionType: '+Thermal' }],
    })
    expect(decodeSemantics(g, [])?.surfaces).toEqual([{ type: '+Thermal' }])
  })

  it('groups the values by the GEOMETRY type, not by the arrays', () => {
    const spec = { solids: [1], shells: [2], semantics: [0, 1] } as const
    const objects = [{ type: SemanticSurfaceType.RoofSurface }]
    const solid = buildGeometry({ type: GeometryType.Solid, ...spec, objects })
    const multi = buildGeometry({ type: GeometryType.MultiSolid, ...spec, objects })
    expect(decodeSemantics(solid, [])?.values).toEqual([[0, 1]])
    expect(decodeSemantics(multi, [])?.values).toEqual([[[0, 1]]])
  })
})

// --------------------------------------------------------- material values ---

// C++: "material values are one index per surface for the surface types"
//      "a material on a type that cannot carry one is read flat"
describe('material values: the flat types', () => {
  it('is one index per surface for MultiSurface and CompositeSurface', () => {
    expect(decodeMaterialValues(GeometryType.MultiSurface, [], [], [0, 1, NONE, 2]))
      .toEqual([0, 1, null, 2])
    expect(decodeMaterialValues(GeometryType.CompositeSurface, [], [], [0, 1, NONE, 2]))
      .toEqual([0, 1, null, 2])
  })

  it('reads a material on a type that cannot carry one flat', () => {
    // MultiPoint and MultiLineString name no `material` and declare
    // additionalProperties: false, so this is not valid CityJSON; it is read
    // as the shallowest thing it could be rather than at a guessed depth.
    expect(decodeMaterialValues(GeometryType.MultiPoint, [], [], [7])).toEqual([7])
  })

  it('keeps a material index of 0, which is falsy in JS', () => {
    expect(decodeMaterialValues(GeometryType.MultiSurface, [], [], [0])).toEqual([0])
  })
})

// C++: "a Solid's material values are one array per shell"
//      "the SAME arrays give MultiSolid material values one more level than Solid"
//      "a CompositeSolid's material values nest solid -> shell -> index"
describe('material values: the solid types', () => {
  it('gives a Solid one array per shell', () => {
    expect(decodeMaterialValues(GeometryType.Solid, [2], [3, 3], [0, 1, NONE, 2, 3, 4]))
      .toEqual([[0, 1, null], [2, 3, 4]])
  })

  it('gives a MultiSolid one more level than a Solid from the SAME arrays', () => {
    // THE regression (finding #8). `solids = [1]` is what a one-shell Solid
    // AND a one-solid MultiSolid both write. Any guard over these arrays gets
    // one of the two wrong: do NOT reintroduce a `solids[0] > 1` guard.
    expect(decodeMaterialValues(GeometryType.Solid, [1], [2], [0, 1])).toEqual([[0, 1]])
    expect(decodeMaterialValues(GeometryType.MultiSolid, [1], [2], [0, 1])).toEqual([[[0, 1]]])
    expect(decodeMaterialValues(GeometryType.CompositeSolid, [1], [2], [0, 1]))
      .toEqual([[[0, 1]]])
  })

  it('nests a CompositeSolid solid -> shell -> index', () => {
    expect(decodeMaterialValues(
      GeometryType.CompositeSolid, [2, 1], [3, 3, 3],
      [0, 1, NONE, 2, NONE, NONE, 3, 4, NONE],
    )).toEqual([[[0, 1, null], [2, null, null]], [[3, 4, null]]])
  })
})

// C++: "a null shell or solid in material values decodes as null, not []"
describe('material values: a null shell or solid', () => {
  it('decodes a u32::MAX count as null, not as an empty array', () => {
    // material.values is nullable at EVERY level
    // (geomprimitives.schema.json), and u32::MAX in a count array says so.
    expect(decodeMaterialValues(GeometryType.Solid, [2], [2, NONE], [0, 1]))
      .toEqual([[0, 1], null])
    expect(decodeMaterialValues(GeometryType.CompositeSolid, [1, NONE], [2], [0, 1]))
      .toEqual([[[0, 1]], null])
  })
})

// C++: "material values clamp rather than throw when the counts over-claim"
describe('material values: over-claiming counts', () => {
  it('leaves a Solid shell list short when the shells run out', () => {
    expect(decodeMaterialValues(GeometryType.Solid, [3], [1, 1], [1, 2]))
      .toEqual([[1], [2]])
  })

  it('keeps a missing shell inside a solid as empty rather than dropping it', () => {
    expect(decodeMaterialValues(GeometryType.MultiSolid, [3], [1, 1], [1, 2]))
      .toEqual([[[1], [2], []]])
  })

  it('keeps an empty shell for a trailing solid whose shells ran out', () => {
    expect(decodeMaterialValues(GeometryType.MultiSolid, [1, 1], [1], [9]))
      .toEqual([[[9]], [[]]])
  })

  it('leaves the shell short and the next empty when the vertices run out', () => {
    expect(decodeMaterialValues(GeometryType.Solid, [2], [3, 3], [1, 2]))
      .toEqual([[1, 2], []])
  })

  it('gives a solid with no shells one empty shell, not a flat list', () => {
    expect(decodeMaterialValues(GeometryType.MultiSolid, [1], [], [7])).toEqual([[[]]])
  })

  it('still produces the shell structure for an empty vertices vector', () => {
    expect(decodeMaterialValues(GeometryType.Solid, [1], [2], [])).toEqual([[]])
  })
})

describe('the OPTIONAL MaterialMapping.value scalar', () => {
  // This is a DIFFERENT field from the values vector above -- a nullable
  // uint on the mapping itself (src/fbs/geometry.fbs:51), handled outside
  // decodeMaterialValues (cf. src/cpp/src/cityjson.cpp:215). Testing the
  // vector does not exercise it, so `if (mapping.value())` can stay broken
  // while every test above passes.
  it('distinguishes an absent shared value from a shared value of 0', () => {
    expect(sharedMaterialValue({ value: () => null })).toBeUndefined()
    expect(sharedMaterialValue({ value: () => 0 })).toBe(0) // falsy but PRESENT
    expect(sharedMaterialValue({ value: () => 3 })).toBe(3)
  })
})

// ---------------------------------------------------------- texture values ---

// C++: "a MultiSurface's texture values nest surface -> ring"
describe('texture values: the surface types', () => {
  it('nests surface -> ring', () => {
    expect(decodeTextureValues(
      GeometryType.MultiSurface, [], [3], [1, 1, 1], [4, 4, 4],
      [0, 10, 20, 30, 1, 11, 21, NONE, 2, 12, NONE, 32],
    )).toEqual([[[0, 10, 20, 30]], [[1, 11, 21, null]], [[2, 12, null, 32]]])
  })
})

// C++: "the SAME arrays give MultiSolid texture values one more level than Solid"
//      "a Solid's texture values are one entry per shell"
//      "a CompositeSolid's texture values nest solid -> shell -> surface -> ring"
describe('texture values: the solid types', () => {
  it('gives a MultiSolid one more level than a Solid from the SAME arrays', () => {
    // The texture half of the regression: four levels for a Solid, five for a
    // one-solid MultiSolid, from identical arrays.
    expect(decodeTextureValues(GeometryType.Solid, [1], [1], [1], [3], [0, 10, 20]))
      .toEqual([[[[0, 10, 20]]]])
    expect(decodeTextureValues(GeometryType.MultiSolid, [1], [1], [1], [3], [0, 10, 20]))
      .toEqual([[[[[0, 10, 20]]]]])
    expect(decodeTextureValues(GeometryType.CompositeSolid, [1], [1], [1], [3], [0, 10, 20]))
      .toEqual([[[[[0, 10, 20]]]]])
  })

  it('gives a Solid one entry per shell', () => {
    expect(decodeTextureValues(
      GeometryType.Solid, [2], [2, 1], [1, 1, 1], [3, 3, 3],
      [0, 10, 20, 1, 11, NONE, 2, 12, 22],
    )).toEqual([[[[0, 10, 20]], [[1, 11, null]]], [[[2, 12, 22]]]])
  })

  it('nests a CompositeSolid solid -> shell -> surface -> ring', () => {
    expect(decodeTextureValues(
      GeometryType.CompositeSolid, [2, 1], [2, 2, 2], [1, 1, 1, 1, 1, 1], [3, 3, 3, 3, 3, 3],
      [0, 10, 20, 1, 11, NONE, 2, 12, 22, 3, NONE, 23, 4, 14, 24, 5, 15, 25],
    )).toEqual([
      [[[[0, 10, 20]], [[1, 11, null]]], [[[2, 12, 22]], [[3, null, 23]]]],
      [[[[4, 14, 24]], [[5, 15, 25]]]],
    ])
  })
})

// C++: "a texture on a type that cannot carry one falls back to one surface"
describe('texture values: a type that cannot carry a texture', () => {
  it('falls back to max(surfaces, 1) surfaces', () => {
    // The reference reads `max(surfaces.len(), 1)` surfaces, so even a mapping
    // with no count arrays produces one (empty) surface rather than a flat
    // index list. Not valid CityJSON either way; pinned so the readers agree.
    expect(decodeTextureValues(
      GeometryType.MultiLineString, [], [], [2], [3, 3], [0, 10, 20, 1, 11, 21],
    )).toEqual([[[0, 10, 20], [1, 11, 21]]])

    expect(decodeTextureValues(GeometryType.MultiPoint, [], [], [], [], [0, NONE, 2]))
      .toEqual([[]])
  })
})

// C++: "texture values clamp rather than throw when the counts over-claim"
describe('texture values: over-claiming counts', () => {
  it('keeps the later rings and surfaces, empty, when the strings run out', () => {
    expect(decodeTextureValues(GeometryType.MultiSurface, [], [], [2, 1], [3], [0, 1]))
      .toEqual([[[0, 1], []], [[]]])
  })

  it('gives a solid with no shells one empty shell', () => {
    expect(decodeTextureValues(GeometryType.MultiSolid, [1], [], [], [], [7]))
      .toEqual([[[]]])
  })

  it('keeps an empty shell for a trailing solid whose shells ran out', () => {
    expect(decodeTextureValues(GeometryType.MultiSolid, [1, 1], [1], [1], [1], [9]))
      .toEqual([[[[[9]]]], [[]]])
  })

  it('still produces the full structure for an empty vertices vector', () => {
    expect(decodeTextureValues(GeometryType.Solid, [1], [1], [1], [3], []))
      .toEqual([[[[]]]])
  })
})

// ------------------------------------------------------------------ names ---

// C++: "geometry type names match CityJSON spelling"
describe('geometryTypeName', () => {
  it('matches CityJSON spelling', () => {
    expect(geometryTypeName(0)).toBe('MultiPoint')
    expect(geometryTypeName(2)).toBe('MultiSurface')
    expect(geometryTypeName(4)).toBe('Solid')
    expect(geometryTypeName(6)).toBe('CompositeSolid')
  })

  it('throws on a tag it cannot name', () => {
    // UNKNOWN-TAG POLICY. A geometry type is the one unnameable tag with no
    // '+'-prefixed extension form available (CityJSON section 3 enumerates
    // exactly eight), so an unknown tag is an error rather than a placeholder
    // -- and specifically NOT a silent fallback to "Solid", which would read
    // the boundaries at the wrong depth.
    expect(() => geometryTypeName(8)).toThrow(FcbError)
    expect(() => geometryTypeName(99)).toThrow(FcbError)
  })
})

// C++ (test_cityjson.cpp): "semantic surface type names cover the enum"
//      "an unnameable semantic surface tag becomes a schema-valid Extension name"
describe('semanticSurfaceTypeName', () => {
  it('covers the enum', () => {
    expect(semanticSurfaceTypeName(0)).toBe('RoofSurface')
    expect(semanticSurfaceTypeName(6)).toBe('Window')
    expect(semanticSurfaceTypeName(17)).toBe('TransportationHole')
  })

  it('spells an unnameable tag as a schema-valid extension name', () => {
    for (const tag of [18, 19, 200]) {
      expect(semanticSurfaceTypeName(tag)).toBe('+GenericSurface')
    }
    expect(semanticSurfaceTypeName(18)).not.toBe('ExtraSemanticSurface')
  })
})
