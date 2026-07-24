import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import * as flatbuffers from 'flatbuffers'
import { describe, expect, it } from 'vitest'
import { emitInt64, toCityJSONFeature, toCityJSONMetadata } from '../src/cityjson/index.js'
import { FcbError } from '../src/errors.js'
import { Feature } from '../src/feature/index.js'
import { CityFeature } from '../src/generated/city-feature.js'
import { CityObject } from '../src/generated/city-object.js'
import { CityObjectType } from '../src/generated/city-object-type.js'
import { Geometry as FbGeometry } from '../src/generated/geometry.js'
import { GeometryType } from '../src/generated/geometry-type.js'
import { MaterialMapping } from '../src/generated/material-mapping.js'
import { FcbReader } from '../src/reader.js'

// `__dirname` does not exist under ESM; the port-wide convention is
// `import.meta.dirname`.
const CORPUS = resolve(import.meta.dirname, '../../../conformance')

/** EVERY corpus case that has a matching `.expected.jsonl`, not just the nine
 *  the plan named: `appearance_depths`, `multi_object_attrs`,
 *  `colliding_strings` and `no_count` were added to the corpus after the plan
 *  was written and are precisely the ones exercising appearance depth,
 *  multi-object attribute schemas, string collisions and an UNKNOWN feature
 *  count. `appearance_depths_node8.fcb` is deliberately absent: it has no
 *  `.expected.jsonl` of its own -- it is the same model re-written with an
 *  R-tree node size of 8, for the index tests of a later task -- and this
 *  suite never invents an expectation. */
const CASES = [
  'appearance_depths',
  'colliding_strings',
  'degenerate_extent',
  'duplicate_keys',
  'empty_appearance',
  'geom_decoder_edges',
  'geom_temp',
  'inferable_types',
  'long_strings',
  'multi_object_attrs',
  'no_count',
  'noise_extension',
  'single_feature',
  'small',
]

describe.each(CASES)('conformance: %s', (name) => {
  it('matches the Rust reader line for line', async () => {
    const expected = readFileSync(resolve(CORPUS, `${name}.expected.jsonl`), 'utf8')
      .split('\n')
      .filter((l) => l.trim())
      .map((l) => JSON.parse(l) as unknown)

    const r = await FcbReader.fromBytes(
      new Uint8Array(readFileSync(resolve(CORPUS, `${name}.fcb`))),
    )
    const actual: unknown[] = [toCityJSONMetadata(r.header)]
    for await (const f of await r.selectAll()) {
      actual.push(toCityJSONFeature(f, r.header))
    }

    expect(actual).toHaveLength(expected.length)

    // Compare the WHOLE line, metadata included. Comparing selected keys is
    // what hid the missing per-feature `appearance` object through the whole
    // C++ port -- and a selected-key metadata check lets an implementation
    // omit the extent, the CRS, the identifier and the title and still pass.
    for (let i = 0; i < actual.length; i++) {
      expect(actual[i], `${name} line ${i}`).toEqual(expected[i])
    }
  })
})

/** A one-object, one-geometry CityFeature carrying exactly the two material
 *  mappings the corpus cannot supply, built through the generated builders so
 *  the emitter is exercised on REAL FlatBuffers bytes rather than a mock.
 *
 *  The corpus does pin a shared material `value`, but only `"value": 2` --
 *  truthy, so a call site written `if (m.value())` instead of
 *  `!== undefined` would still pass every corpus case. `"value": 0` is a
 *  perfectly real material index and is falsy in JS, which is the whole
 *  reason optional scalars are read with `!== null`/`!== undefined` here. */
function syntheticFeature(): Feature {
  const b = new flatbuffers.Builder(1024)

  const zeroTheme = b.createString('zero')
  const zero = MaterialMapping.createMaterialMapping(b, zeroTheme, 0, 0, 0, 0)

  // No `vertices` vector at all: `"values": null`, an explicit null rather
  // than a skipped theme (geom_decoder.rs:403-413).
  const nullTheme = b.createString('nullvalues')
  MaterialMapping.startMaterialMapping(b)
  MaterialMapping.addTheme(b, nullTheme)
  const nulls = MaterialMapping.endMaterialMapping(b)

  const material = FbGeometry.createMaterialVector(b, [zero, nulls])
  const surfaces = FbGeometry.createSurfacesVector(b, [1])
  const strings = FbGeometry.createStringsVector(b, [4])
  const boundaries = FbGeometry.createBoundariesVector(b, [0, 1, 2, 3])
  FbGeometry.startGeometry(b)
  FbGeometry.addType(b, GeometryType.MultiSurface)
  FbGeometry.addSurfaces(b, surfaces)
  FbGeometry.addStrings(b, strings)
  FbGeometry.addBoundaries(b, boundaries)
  FbGeometry.addMaterial(b, material)
  const geometry = FbGeometry.endGeometry(b)

  const objectId = b.createString('o')
  const geometries = CityObject.createGeometryVector(b, [geometry])
  CityObject.startCityObject(b)
  CityObject.addType(b, CityObjectType.Building)
  CityObject.addId(b, objectId)
  CityObject.addGeometry(b, geometries)
  const object = CityObject.endCityObject(b)

  const featureId = b.createString('f')
  const objects = CityFeature.createObjectsVector(b, [object])
  CityFeature.startCityFeature(b)
  CityFeature.addId(b, featureId)
  CityFeature.addObjects(b, objects)
  b.finishSizePrefixed(CityFeature.endCityFeature(b))

  return new Feature(b.asUint8Array().slice(), [], 0)
}

describe('material mapping, at the emitter call site', () => {
  it('keeps a shared material index of 0 and an explicitly null values array', async () => {
    const r = await FcbReader.fromBytes(
      new Uint8Array(readFileSync(resolve(CORPUS, 'empty_appearance.fcb'))),
    )
    const emitted = toCityJSONFeature(syntheticFeature(), r.header)
    expect(emitted.CityObjects['o']?.geometry?.[0]?.material).toEqual({
      zero: { value: 0 },
      nullvalues: { values: null },
    })
  })
})

describe('int64 policy', () => {
  const BIG = 9007199254740993n // 2^53 + 1: NOT representable as a number

  it('defaults to a lossy number, keeping the output JSON-serializable', () => {
    expect(emitInt64(BIG, 'lossy-number')).toBe(9007199254740992) // rounded
    expect(() => JSON.stringify({ v: emitInt64(BIG, 'lossy-number') })).not.toThrow()
  })

  it('emits an exact decimal string when asked', () => {
    expect(emitInt64(BIG, 'decimal-string')).toBe('9007199254740993')
  })

  it('throws on an unsafe value under the error policy', () => {
    expect(() => emitInt64(BIG, 'error')).toThrow(FcbError)
    expect(emitInt64(42n, 'error')).toBe(42) // safe values pass through
  })

  it('never leaks a bigint into the emitted object under ANY policy', () => {
    // The plan's draft of this test `continue`d past 'error', so it exercised
    // only two of the three policies and the one that can throw was never
    // reached. 42n is safe under every policy, so none of the three has an
    // excuse to be skipped.
    for (const p of ['lossy-number', 'decimal-string', 'error'] as const) {
      expect(typeof emitInt64(42n, p)).not.toBe('bigint')
    }
  })

  it('is exact for the largest values each policy can represent', () => {
    // 2^63-1 and -2^63: the extremes of a `Long` column.
    expect(emitInt64(9223372036854775807n, 'decimal-string')).toBe('9223372036854775807')
    expect(emitInt64(-9223372036854775808n, 'decimal-string')).toBe('-9223372036854775808')
    // Safe-range boundaries pass through unchanged under 'error'.
    expect(emitInt64(9007199254740991n, 'error')).toBe(9007199254740991)
    expect(emitInt64(-9007199254740991n, 'error')).toBe(-9007199254740991)
    expect(() => emitInt64(-9007199254740993n, 'error')).toThrow(FcbError)
  })
})
