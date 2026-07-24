import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'
import { ColumnType } from '../src/generated/column-type.js'
import { readHeader } from '../src/header/index.js'
import { BytesRangeReader } from '../src/io/range-reader.js'
import { FcbReader } from '../src/reader.js'
import type { AttrCondition } from '../src/reader.js'
import {
  PAYLOAD_TAG, decodePayloadEntry, isTagged, searchAttributes, streeNumNodes, stripTag,
} from '../src/static-btree/index.js'
import { featureBounds } from './fixtures/feature-bounds.js'

// `__dirname` does not exist under ESM; `import.meta.dirname` is its
// replacement (Node >= 22.12, which package.json already requires).
const CORPUS = resolve(import.meta.dirname, '../../../conformance')
const DATA = resolve(import.meta.dirname, '../../../examples/data')
const bytes = (p: string) => new Uint8Array(readFileSync(p))
const corpus = (n: string) => bytes(resolve(CORPUS, n))
const ids = async (c: AsyncIterable<{ id: string }>) => {
  const out: string[] = []
  for await (const f of c) out.push(f.id)
  return out
}
const sorted = (v: readonly string[]) => [...v].sort()

describe('level bounds and node count', () => {
  it('breaks at n < branchingFactor, not at n === 1', () => {
    // Pinned against the C++ reader's own test (tests/test_stree.cpp:18-26),
    // which is the conformant reference for this asymmetry.
    expect(streeNumNodes(100, 16)).toBe(107)
    expect(streeNumNodes(16, 16)).toBe(17)
    expect(streeNumNodes(10, 16)).toBe(11)
    expect(streeNumNodes(1000, 16)).toBe(1067)
    expect(() => streeNumNodes(10, 1)).toThrow()
  })
})

describe('payload tag', () => {
  it('is a bigint literal, because 1 << 63 in JS is -2147483648', () => {
    expect(PAYLOAD_TAG).toBe(0x8000000000000000n)
    expect(1 << 63).toBe(-2147483648) // the trap, pinned
  })

  it('survives Number() only when stripped in bigint first', () => {
    const tagged = PAYLOAD_TAG | 12345n
    // Number() ROUNDS the low bits: at 2^63 the double spacing is 2048, so
    // the payload offset comes back as 12288 -- close enough to look right,
    // wrong enough to read the wrong payload entry.
    expect(Number(tagged) - Number(PAYLOAD_TAG)).toBe(12288)
    expect(Number(PAYLOAD_TAG | 1n)).toBe(Number(PAYLOAD_TAG)) // 1 vanishes
    // Stripping in bigint first is exact:
    expect(Number(tagged & (PAYLOAD_TAG - 1n))).toBe(12345)
  })

  it('recognises a tagged offset without losing the untagged case', () => {
    expect(isTagged(PAYLOAD_TAG | 7n)).toBe(true)
    expect(isTagged(7n)).toBe(false)
    expect(stripTag(PAYLOAD_TAG | 7n)).toBe(7n)
  })

  it('decodes an entry as u32 count then count x u64, all LE', () => {
    const raw = new Uint8Array([
      0x02, 0x00, 0x00, 0x00,
      0x0a, 0, 0, 0, 0, 0, 0, 0,
      0x14, 0, 0, 0, 0, 0, 0, 0,
    ])
    expect(decodePayloadEntry(raw)).toEqual([10, 20])
    expect(() => decodePayloadEntry(new Uint8Array([5, 0, 0, 0, 1, 2, 3]))).toThrow()
  })
})

// ---------------------------------------------------------------------------
// STEP 0 ORACLE -- every list below was printed by the C++ reader
// (src/cpp/src/stree.cpp via FcbReader::select_attr) from a temporary probe
// in src/cpp/tests/test_stree.cpp, which was then reverted. Nothing here was
// produced by the code under test: two implementations can be identically
// wrong, so an expectation derived from this port would prove nothing.
//
// conformance/inputs/multi_object_attrs.city.jsonl:
//   lo    -> one CityObject, h = 1
//   mid   -> one CityObject, h = 5
//   hi    -> one CityObject, h = 9
//   multi -> two BuildingParts, h = 1 AND h = 9   <-- the finding-#1 shape
// `h` is inferred as ULong by the writer (ColumnType 8), so its key kind is
// `u64` and the query value is a bigint under the hood.
// ---------------------------------------------------------------------------
const H = 'h'
const PIVOT = 5
const ORACLE = {
  Eq: ['mid'],
  Ne: ['hi', 'lo', 'multi'],
  Gt: ['hi', 'multi'],
  Ge: ['hi', 'mid', 'multi'],
  Lt: ['lo', 'multi'],
  Le: ['lo', 'mid', 'multi'],
} as const

// The both-values regression needs a pivot a feature ALSO holds, alongside a
// larger value. `multi` holds 1 and 9, so that pivot is 1, not 5 -- with
// PIVOT = 5 no feature holds both 5 and something larger, and the subtraction
// bug would not fire. Pinned from the same probe run:
//   PROBE h Gt 1 -> hi mid multi
const GT_PIVOT = 1
const ORACLE_GT_PIVOT = ['hi', 'mid', 'multi']
const BOTH_VALUES_FEATURE_ID = 'multi'

// `Gt` alone does not close the hole: a regression that reintroduced Rust's
// subtraction for only `Lt` or only `Ne` would still pass everything above,
// because with PIVOT = 5 no feature holds 5 as well as something on the other
// side. These three pivots each make `multi` (which holds 1 AND 9) a genuine
// match through one of its values while equalling the pivot through the
// other, which is precisely what the subtraction removes. Pinned from the
// same probe run:
//   PROBE15M [Lt 9] -> lo mid multi
//   PROBE15M [Ne 1] -> mid hi multi
//   PROBE15M [Ne 9] -> lo mid multi
//   PROBE15M [Le 1] -> lo multi
const ORACLE_BOTH_VALUES = [
  { op: 'Lt' as const, value: 9, ids: ['lo', 'mid', 'multi'] },
  { op: 'Ne' as const, value: 1, ids: ['hi', 'mid', 'multi'] },
  { op: 'Ne' as const, value: 9, ids: ['lo', 'mid', 'multi'] },
  { op: 'Le' as const, value: 1, ids: ['lo', 'multi'] },
]

describe('attribute queries', () => {
  it.each(Object.keys(ORACLE) as Array<keyof typeof ORACLE>)(
    '%s matches the C++ reader exactly', async (op) => {
      const r = await FcbReader.fromBytes(corpus('multi_object_attrs.fcb'))
      const hit = await ids(await r.select({
        where: [{ field: H, operator: op, value: PIVOT }],
      }))
      expect(sorted(hit)).toEqual(sorted(ORACLE[op]))
      expect(hit.length).toBeGreaterThan(0) // an empty list proves nothing
    })

  it('Gt keeps a feature whose OTHER CityObject holds a smaller value', async () => {
    // Upstream finding #1, stated as the concrete regression. Rust computes
    // Gt as range-minus-exact over FEATURE offsets, so a feature carrying
    // both 1 and 9 is returned by the range (via 9), found by find_exact(1),
    // and then subtracted away -- a false negative for a genuine match.
    // multi_object_attrs.fcb exists to have exactly one such feature.
    const r = await FcbReader.fromBytes(corpus('multi_object_attrs.fcb'))
    const hit = await ids(await r.select({
      where: [{ field: H, operator: 'Gt', value: GT_PIVOT }],
    }))
    expect(sorted(hit)).toEqual(sorted(ORACLE_GT_PIVOT))
    expect(hit).toContain(BOTH_VALUES_FEATURE_ID)
  })

  it.each(ORACLE_BOTH_VALUES)(
    '$op($value) keeps a feature that ALSO holds the pivot exactly', async (row) => {
      // The same finding-#1 regression, for the operators `Gt(1)` alone does
      // not exercise. Under range-minus-exact, `multi` is found by the range
      // (through its other value), found by find_exact(pivot), and subtracted
      // away -- a false negative for a genuine match.
      const r = await FcbReader.fromBytes(corpus('multi_object_attrs.fcb'))
      const hit = await ids(await r.select({
        where: [{ field: H, operator: row.op, value: row.value }],
      }))
      expect(sorted(hit)).toEqual(sorted(row.ids))
      expect(hit).toContain(BOTH_VALUES_FEATURE_ID)
    })

  it('Eq on the type maximum does not walk off the end of the level', async () => {
    // Separator entries with no right sibling carry K::max_value(), whose
    // offset ALREADY points at the last child group; adding node_size runs
    // past it. inferable_types' a_bool index has exactly one unique key, so
    // its root separator IS `true` and Eq(true) takes the clamp branch.
    // (stree.cpp:213-222). Oracle: PROBE6 bool Eq(true) -> t
    const r = await FcbReader.fromBytes(corpus('inferable_types.fcb'))
    const b = r.header.info.columns.find((c) => c.type === ColumnType.Bool)
    expect(b, 'fixture must contain a Bool column').toBeDefined()
    const hit = await ids(await r.select({
      where: [{ field: b!.name, operator: 'Eq', value: true }],
    }))
    expect(hit).toEqual(['t'])
  })

  it('AND-intersects multiple conditions', async () => {
    const r = await FcbReader.fromBytes(corpus('multi_object_attrs.fcb'))
    const both = await ids(await r.select({
      where: [
        { field: H, operator: 'Ge', value: PIVOT },
        { field: H, operator: 'Le', value: PIVOT },
      ],
    }))
    // NOT the same as Eq under existential semantics in general, but here
    // Ge(5) n Le(5) = {hi,mid,multi} n {lo,mid,multi} = {mid,multi}. Pinned
    // from the oracle lists above, NOT from a third query.
    expect(sorted(both)).toEqual(['mid', 'multi'])
  })

  it('intersects a spatial query with an attribute query', async () => {
    // NOT on multi_object_attrs.fcb: all four of its features carry the same
    // extent [0,0]-[1,1], so no box can separate them and the original
    // version of this test (bbox [-1,-1,1000,1000], expecting exactly
    // ORACLE.Ge) passed for a reader that ignored the spatial half entirely.
    // small.fcb is the smallest corpus file whose features are spatially
    // distinct AND whose attributes are indexed.
    //
    // Oracles, both independent of this reader's query path:
    //  * attribute side, from the C++ reader (probe reverted):
    //      b3_h_dak_50p Ge 2.0 -> ...016459  ...005156  ...012869  (all three)
    //  * spatial side, from the brute-force `featureBounds` oracle below,
    //    which recomputes each extent from the feature's own vertices.
    // BOX excludes ...012869 (its maxX is 84597.5), so the intersection is a
    // PROPER subset of the attribute answer and the box demonstrably cut.
    const BOX: [number, number, number, number] = [84700, 446600, 85600, 446900]
    const ORACLE_H50P_GE_2 = [
      'NL.IMBAG.Pand.0503100000016459',
      'NL.IMBAG.Pand.0503100000005156',
      'NL.IMBAG.Pand.0503100000012869',
    ]

    const r = await FcbReader.fromBytes(corpus('small.fcb'))
    const brute: string[] = []
    for await (const f of await r.selectAll()) {
      const b = featureBounds(f, r.header)
      if (b.maxX >= BOX[0] && b.minX <= BOX[2] && b.maxY >= BOX[1] && b.minY <= BOX[3]) {
        brute.push(f.id)
      }
    }
    expect(sorted(brute)).toEqual(sorted([
      'NL.IMBAG.Pand.0503100000016459', 'NL.IMBAG.Pand.0503100000005156',
    ]))

    const where = [{ field: 'b3_h_dak_50p', operator: 'Ge' as const, value: 2.0 }]
    expect(sorted(await ids(await r.select({ where })))).toEqual(sorted(ORACLE_H50P_GE_2))

    const hit = await ids(await r.select({ spatial: { kind: 'bbox', value: BOX }, where }))
    expect(sorted(hit)).toEqual(sorted(ORACLE_H50P_GE_2.filter((id) => brute.includes(id))))
    expect(hit.length).toBe(2)
    expect(hit.length).toBeLessThan(ORACLE_H50P_GE_2.length) // the bbox really cut
  })

  it('rejects a query on a column with no attribute index', async () => {
    const r = await FcbReader.fromBytes(corpus('small.fcb'))
    await expect(r.select({ where: [{ field: 'nope', operator: 'Eq', value: 1 }] }))
      .rejects.toThrow(/index/i)
  })

  it('rejects Json and Binary index queries, as Rust does', async () => {
    // Deliberate divergence #2. NOTE the enum values: String is 11, Json is
    // 12, DateTime is 13, Binary is 14 (src/fbs/header.fbs:9-26). Using a
    // literal 13 here would search for DateTime and silently skip the test.
    const r = await FcbReader.fromBytes(corpus('inferable_types.fcb'))
    const json = r.header.info.columns.find((c) => c.type === ColumnType.Json)
    expect(json, 'fixture must contain a Json column').toBeDefined()
    await expect(r.select({
      where: [{ field: json!.name, operator: 'Eq', value: '{}' }],
    })).rejects.toThrow(/unsupported column type/i)
  })

  it('aborts a traversal when the signal fires', async () => {
    const r = await FcbReader.fromBytes(corpus('multi_object_attrs.fcb'))
    const ac = new AbortController()
    ac.abort()
    await expect(r.select({
      where: [{ field: H, operator: 'Ge', value: PIVOT }], signal: ac.signal,
    })).rejects.toThrow()
  })
})

// ---------------------------------------------------------------------------
// Fixed-width string keys: the index answers with CANDIDATES, and the
// strictness of Gt/Lt is deliberately INVERTED so the equal-prefix band
// survives for Task 15's post-filter. Oracle: the C++ reader run with
// `exact_index_only = true`, i.e. its raw index result with the post-filter
// switched off -- which is exactly what this task produces.
//
// conformance/inputs/colliding_strings.city.jsonl, `label`:
//   long_a     -> 'k'*50 + 'alpha'   (truncates to 'k'*50)
//   long_b     -> 'k'*50 + 'beta'    (truncates to 'k'*50)
//   long_exact -> 'k'*50
//   short_a    -> 'a'
//   short_ab   -> 'ab'
// ---------------------------------------------------------------------------
const K50 = 'k'.repeat(50)
const ORACLE_STR = {
  Eq: ['long_a', 'long_b', 'long_exact'],
  Ne: ['long_a', 'long_b', 'long_exact', 'short_a', 'short_ab'],
  Gt: ['long_a', 'long_b', 'long_exact'],
  Ge: ['long_a', 'long_b', 'long_exact'],
  Lt: ['long_a', 'long_b', 'long_exact', 'short_a', 'short_ab'],
  Le: ['long_a', 'long_b', 'long_exact', 'short_a', 'short_ab'],
} as const

/** The candidate ids for one condition, taken from the INDEX LAYER
 *  (`searchAttributes`) rather than from `FcbReader.select`.
 *
 *  Task 15 attached a post-filter to `select`, so `select` no longer answers
 *  with candidates -- but the traversal's deliberate over-return is exactly
 *  what the lists below pin, and it is the property the post-filter depends
 *  on. Reading it here keeps the C++ `exact_index_only` oracle meaningful
 *  instead of silently re-pinning it to the filtered answer. Offsets are
 *  mapped back to ids through a full scan, which touches no index. */
async function candidateIds(file: string, cond: AttrCondition): Promise<string[]> {
  const buf = corpus(file)
  const reader = new BytesRangeReader(buf)
  const header = await readHeader(reader)
  const hits = await searchAttributes(reader, header, [cond])
  const byOffset = new Map<number, string>()
  const r = await FcbReader.fromBytes(buf)
  for await (const f of await r.selectAll()) byOffset.set(f.byteOffset, f.id)
  return hits.map((h) => byOffset.get(h.offset) ?? `?@${h.offset}`)
}

describe('string keys are truncated, so the index returns candidates', () => {
  it.each(Object.keys(ORACLE_STR) as Array<keyof typeof ORACLE_STR>)(
    '%s widens to the equal-prefix band, matching C++ exact_index_only',
    async (op) => {
      const hit = await candidateIds('colliding_strings.fcb',
        { field: 'label', operator: op, value: K50 })
      expect(sorted(hit)).toEqual(sorted(ORACLE_STR[op]))
    })

  it('Gt is NON-strict for strings: long_exact survives to be post-filtered', () => {
    // The whole point of the inversion. `long_exact` compares EQUAL to the
    // query in the tree, and `long_a`/`long_b` do too -- their extra bytes
    // are past the 50-byte truncation. A strict bound would drop all three,
    // including the two that genuinely are greater. Task 15 removes
    // long_exact; nothing could recover long_a/long_b.
    expect(ORACLE_STR.Gt).toContain('long_exact')
    expect(ORACLE_STR.Gt).toContain('long_a')
  })
})

// ---------------------------------------------------------------------------
// Trap #4, on the only fixture whose B+tree is deep enough to have real
// separator keys: delft.fcb's `b3_h_dak_50p` (Double, 545 unique keys,
// branching factor 256). Probe output:
//   PROBE3 root[0] key=7.380000114440918 off=3
//   PROBE4 leafnode[258] key=7.380000114440918   <-- off(3) + nodeSize(255)
//   PROBE5 Eq count=2 NL.IMBAG.Pand.0503100000019509 NL.IMBAG.Pand.0503100000019817
//   PROBE5 Lt count=532
//   PROBE5 Le count=534
// find_partition descends LEFT on that exact hit, returning leaf index 3, so
// an un-widened scan ends at 3 + 255 = 258 EXCLUSIVE and drops the matching
// entry that sits at exactly 258. (stree.cpp:282-292)
// ---------------------------------------------------------------------------
const SEPARATOR_COLUMN = 'b3_h_dak_50p'
const SEPARATOR_VALUE = 7.380000114440918
const ORACLE_LE_SEPARATOR_COUNT = 534
const ORACLE_LT_SEPARATOR_COUNT = 532
const ORACLE_EQ_SEPARATOR = [
  'NL.IMBAG.Pand.0503100000019509',
  'NL.IMBAG.Pand.0503100000019817',
]

describe('a separator-valued upper bound', () => {
  it('is not dropped one node past the scan end', async () => {
    const r = await FcbReader.fromBytes(bytes(resolve(DATA, 'delft.fcb')))
    const where = async (operator: 'Eq' | 'Lt' | 'Le') =>
      ids(await r.select({
        where: [{ field: SEPARATOR_COLUMN, operator, value: SEPARATOR_VALUE }],
      }))

    expect(sorted(await where('Eq'))).toEqual(sorted(ORACLE_EQ_SEPARATOR))

    const le = await where('Le')
    expect(le.length).toBe(ORACLE_LE_SEPARATOR_COUNT)
    // The two features keyed exactly at the separator are the ones an
    // un-widened scan loses; Lt (which must NOT contain them) pins that the
    // count difference is really those two and not an off-by-two elsewhere.
    for (const id of ORACLE_EQ_SEPARATOR) expect(le).toContain(id)
    const lt = await where('Lt')
    expect(lt.length).toBe(ORACLE_LT_SEPARATOR_COUNT)
    for (const id of ORACLE_EQ_SEPARATOR) expect(lt).not.toContain(id)
  })
})
