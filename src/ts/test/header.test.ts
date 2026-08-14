import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import * as flatbuffers from 'flatbuffers'
import { describe, expect, it } from 'vitest'
import { toCityJSONMetadata } from '../src/cityjson/index.js'
import { Header as FbHeader } from '../src/generated/header.js'
import { BytesRangeReader } from '../src/io/range-reader.js'
import { readHeader } from '../src/header/index.js'

// `__dirname` does not exist under ESM (this package is "type": "module").
const CORPUS = resolve(import.meta.dirname, '../../../conformance')
const open = (name: string) =>
  new BytesRangeReader(new Uint8Array(readFileSync(resolve(CORPUS, name))))

// Pinned from conformance/small.expected.jsonl line 0 -- the Rust writer's
// own metadata output (the CityJSON `transform`/`metadata.geographicalExtent`
// fields, and one line per feature after it). "Six finite numbers" and
// "length 3" pass for a reader that returns the WRONG six numbers, which is
// the whole failure mode here.
//
//   line 0: {"type":"CityJSON","version":"2.0",
//            "transform":{"scale":[0.001,0.001,0.001],
//                         "translate":[85088.390625,446394.25,45.64800262451172]},
//            "metadata":{"geographicalExtent":[84501.5546875,445805.03125,
//              -3.746997833251953,85675.234375,446983.46875,95.04200744628906]}}
//   lines 1..3: three CityJSONFeature records -> featuresCount = 3.
const SMALL = {
  featuresCount: 3,
  scale: [0.001, 0.001, 0.001] as [number, number, number],
  translate: [85088.390625, 446394.25, 45.64800262451172] as [number, number, number],
  extent: [
    84501.5546875, 445805.03125, -3.746997833251953,
    85675.234375, 446983.46875, 95.04200744628906,
  ] as [number, number, number, number, number, number],
}

describe('readHeader', () => {
  it('reads the EXACT version, count and transform of small.fcb', async () => {
    const { info } = await readHeader(open('small.fcb'))
    expect(info.version).toBe('2.0')
    expect(info.featuresCount).toBe(SMALL.featuresCount)
    expect(info.scale).toEqual(SMALL.scale)
    expect(info.translate).toEqual(SMALL.translate)
  })

  it('reads the EXACT geographical extent', async () => {
    const { info } = await readHeader(open('small.fcb'))
    expect(info.geographicalExtent).toEqual(SMALL.extent)
  })

  it('distinguishes an ABSENT transform from a zero transform', async () => {
    // `transform` is not required by the schema (src/fbs/header.fbs:131) and
    // C++ tracks its presence separately (include/fcb/header.hpp:54). A
    // reader that defaults it to zeros makes a missing transform look like a
    // real one that collapses every coordinate to the origin.
    //
    // Pinned from conformance/degenerate_extent.expected.jsonl line 0: this
    // fixture carries a REAL transform whose translate happens to be zero
    // (`"transform":{"scale":[0.001,0.001,0.001],"translate":[0.0,0.0,0.0]}`).
    // Asserting hasTransform === true and translate === [0,0,0] pins exactly
    // the case a reader that special-cases "all zero" as "absent" would get
    // wrong. Every fixture in conformance/*.expected.jsonl carries a
    // transform, so the ABSENT-transform half of this distinction has no
    // corpus fixture and stays untested here.
    const { info } = await readHeader(open('degenerate_extent.fcb'))
    expect(info.hasTransform).toBe(true)
    expect(info.scale).toEqual([0.001, 0.001, 0.001])
    expect(info.translate).toEqual([0, 0, 0])
  })

  it('computes section offsets that fit inside the file', async () => {
    const reader = open('small.fcb')
    const { layout } = await readHeader(reader)
    expect(layout.featureBegin).toBeLessThan(reader.size())
    expect(layout.rtreeBegin).toBe(layout.headerLen)
  })

  it('rejects a file whose magic bytes are wrong', async () => {
    const bad = new Uint8Array(64)
    await expect(readHeader(new BytesRangeReader(bad))).rejects.toThrow(/magic/i)
  })

  it('treats featuresCount 0 as UNKNOWN, and still computes a layout', async () => {
    // Task 2 generates no_count.fcb -- the same input as small.fcb, written
    // with features_count left at 0 -- so this is testable at all. The other
    // half of this fact ("the scan still runs to EOF and yields every
    // feature despite the 0") needs FcbReader.selectAll(), which does not
    // exist until Task 8; that assertion lives in Task 8's test file.
    // no_count.fcb also has NO R-tree (rtree byte size derives from the
    // feature count, so a count-0 file cannot carry one), so its rtreeSize
    // must be 0 and its layout must still validate against the file size.
    const reader = open('no_count.fcb')
    const { info, layout } = await readHeader(reader)
    expect(info.featuresCount).toBe(0)
    expect(layout.rtreeSize).toBe(0)
    expect(layout.featureBegin).toBeLessThanOrEqual(reader.size())
  })
})

describe('AttributeIndex struct', () => {
  // Pinned from the C++ reader (src/cpp) over conformance/duplicate_keys.fcb:
  // a temporary TEST_CASE in src/cpp/tests/test_header.cpp printed each
  // index's column, length and branching factor via fprintf(stderr, ...),
  // built and ran under src/cpp/build-native, then was reverted. Output:
  //   ORACLE column_index=0 length=160 branching_factor=256 num_unique_items=1 begin=560
  //   ORACLE column_index=1 length=96  branching_factor=256 num_unique_items=5 begin=720
  //   ORACLE layout.attr_index_begin=560 layout.feature_begin=816
  // Iterating "whatever entries came back" passes for a reader that returns
  // NONE; these are the exact wire values instead.
  const EXPECTED_INDICES = [
    { columnIndex: 0, length: 160, branchingFactor: 256 },
    { columnIndex: 1, length: 96, branchingFactor: 256 },
  ]

  it('decodes every declared index with its exact fields', async () => {
    // Field order in header.fbs forces 2 bytes of padding after each ushort,
    // making the struct 16 bytes; reading it as 12 walks into the next entry
    // and yields plausible-looking nonsense rather than an error.
    const { info } = await readHeader(open('duplicate_keys.fcb'))
    expect(info.attributeIndices).toHaveLength(EXPECTED_INDICES.length)
    info.attributeIndices.forEach((ai, i) => {
      expect(ai.columnIndex).toBe(EXPECTED_INDICES[i]!.columnIndex)
      expect(ai.length).toBe(EXPECTED_INDICES[i]!.length)
      expect(ai.branchingFactor).toBe(EXPECTED_INDICES[i]!.branchingFactor)
    })
  })

  it('gives each index a begin offset that follows the previous one', async () => {
    const { info, layout } = await readHeader(open('duplicate_keys.fcb'))
    const sorted = [...info.attributeIndices].sort((a, b) => a.columnIndex - b.columnIndex)
    let expected = layout.attrIndexBegin
    for (const ai of sorted) {
      expect(ai.begin).toBe(expected)
      expected += ai.length
    }
    // The cumulative sum must land EXACTLY on the feature section.
    expect(expected).toBe(layout.featureBegin)
  })
})

/** A minimal, well-formed .fcb byte stream carrying nothing but a header.
 *
 *  `undefined` leaves the FlatBuffers field genuinely ABSENT; `''` stores a
 *  PRESENT-but-empty string. No corpus fixture carries the latter -- which is
 *  exactly why the conformance suite cannot pin the behaviour below --
 *  and the generated accessor returns `''` for it against `null` for absent.
 *  `version` is `(required)` in header.fbs, so it is always set.
 */
function headerOnlyFile(fields: { identifier?: string; title?: string }): Uint8Array {
  const b = new flatbuffers.Builder(1024)
  const version = b.createString('2.0')
  const identifier = fields.identifier === undefined ? null : b.createString(fields.identifier)
  const title = fields.title === undefined ? null : b.createString(fields.title)

  FbHeader.startHeader(b)
  FbHeader.addVersion(b, version)
  FbHeader.addIndexNodeSize(b, 16)
  if (identifier !== null) FbHeader.addIdentifier(b, identifier)
  if (title !== null) FbHeader.addTitle(b, title)
  b.finishSizePrefixed(FbHeader.endHeader(b))

  // MAGIC_BYTES = {'f','c','b',0x01,'f','c','b',0x00} (const_vars.rs:5).
  const magic = new Uint8Array([0x66, 0x63, 0x62, 0x01, 0x66, 0x63, 0x62, 0x00])
  const body = b.asUint8Array()
  const out = new Uint8Array(magic.length + body.length)
  out.set(magic, 0)
  out.set(body, magic.length)
  return out
}

describe('present-but-empty header metadata strings', () => {
  // Rust gates these on PRESENCE, not emptiness:
  // `identifier: header.identifier().map(|i| i.to_string())`, likewise
  // reference_date and title (deserializer.rs:86-93). `Some("")` survives the
  // `.map` and serializes as `"identifier": ""`; only `None` makes
  // `skip_serializing_if = "Option::is_none"` drop the key. TypeScript already
  // matches -- `hdr.identifier() ?? undefined` keeps `''` and `put()` gates on
  // `!== undefined` -- but nothing pinned it, and this is upstream finding
  // #20.11, which C++ and Python both got wrong by gating on emptiness.
  it('emits an empty identifier and title as "", not as absent', async () => {
    const { metadata } = toCityJSONMetadata(
      await readHeader(new BytesRangeReader(headerOnlyFile({ identifier: '', title: '' }))),
    )
    expect(metadata).toBeDefined()
    expect(metadata).toHaveProperty('identifier', '')
    expect(metadata).toHaveProperty('title', '')
  })

  it('omits identifier and title entirely when they are absent', async () => {
    // The other half of the distinction: without this, "emit unconditionally"
    // would pass the case above and still diverge from the oracle.
    const { metadata } = toCityJSONMetadata(
      await readHeader(new BytesRangeReader(headerOnlyFile({}))),
    )
    expect(metadata).toBeDefined()
    expect(metadata).not.toHaveProperty('identifier')
    expect(metadata).not.toHaveProperty('title')
  })
})
