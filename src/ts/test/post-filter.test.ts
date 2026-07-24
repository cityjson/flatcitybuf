import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'
import { readHeader } from '../src/header/index.js'
import { BytesRangeReader } from '../src/io/range-reader.js'
import { compareFullStrings } from '../src/post-filter.js'
import { FcbReader } from '../src/reader.js'
import type { AttrCondition, Operator } from '../src/reader.js'
import { searchAttributes } from '../src/static-btree/index.js'
import { featureBounds } from './fixtures/feature-bounds.js'

// `__dirname` does not exist under ESM; `import.meta.dirname` is its
// replacement (Node >= 22.12, which package.json already requires).
const CORPUS = resolve(import.meta.dirname, '../../../conformance')
const corpus = (n: string) => new Uint8Array(readFileSync(resolve(CORPUS, n)))
const ids = async (c: AsyncIterable<{ id: string }>) => {
  const out: string[] = []
  for await (const f of c) out.push(f.id)
  return out
}
const sorted = (v: readonly string[]) => [...v].sort()

/** The RAW index answer for one condition -- `searchAttributes` is the
 *  candidate layer, deliberately untouched by the post-filter, so this is how
 *  a test states "the index alone would have returned N". Offsets are mapped
 *  back to ids through a full scan, which never consults an index. */
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

// ---------------------------------------------------------------------------
// STEP 0 ORACLE -- every list below was printed by the C++ reader
// (FcbReader::select_attr, src/cpp/src/reader.cpp:394-436) from a temporary
// probe in src/cpp/tests/test_stree.cpp that was reverted afterwards. `FILT`
// is the default (post-filtered) answer; `RAW` is the same query with
// `AttrQueryOptions::exact_index_only = true`, i.e. the index candidates.
// Nothing here came from the TypeScript under test.
//
// conformance/colliding_strings.fcb, column `label` (ColumnType.String):
//   long_a     -> 'k'*50 + 'alpha'   (55 bytes)
//   long_b     -> 'k'*50 + 'beta'    (54 bytes)
//   long_exact -> 'k'*50             (50 bytes)
//   short_a    -> 'a'
//   short_ab   -> 'ab'
// The first three share an identical 50-byte index key, so the index cannot
// tell them apart at all. Verified from colliding_strings.expected.jsonl by
// grouping values on their first 50 UTF-8 bytes.
// ---------------------------------------------------------------------------
const COL = 'label'
const K50 = 'k'.repeat(50)
const VALUE_AAA = `${K50}alpha` // long_a's full value
const VALUE_BBB = `${K50}beta` // long_b's full value
const IDS_AAA = ['long_a']
const IDS_BBB = ['long_b']
const ALL5 = ['long_a', 'long_b', 'long_exact', 'short_a', 'short_ab']
const LONG3 = ['long_a', 'long_b', 'long_exact']

interface Row {
  pivot: string
  label: string
  op: Operator
  filtered: readonly string[]
  raw: readonly string[]
}

/** One row per probed (pivot, operator). `raw` is what the index alone
 *  returns; `filtered` is what the C++ reader actually answers. Wherever the
 *  two differ, a reader with no post-filter -- or one that gates the filter on
 *  the query's length -- returns `raw` and fails. */
const ORACLE_STRING_OPS: readonly Row[] = [
  // PROBE15 [K50 *]: the equal-prefix band. Every operator's candidate set
  // contains all three long values because their keys are identical.
  { pivot: K50, label: 'K50', op: 'Eq', filtered: ['long_exact'], raw: LONG3 },
  { pivot: K50, label: 'K50', op: 'Ne', filtered: ['long_a', 'long_b', 'short_a', 'short_ab'], raw: ALL5 },
  { pivot: K50, label: 'K50', op: 'Gt', filtered: ['long_a', 'long_b'], raw: LONG3 },
  { pivot: K50, label: 'K50', op: 'Ge', filtered: LONG3, raw: LONG3 },
  { pivot: K50, label: 'K50', op: 'Lt', filtered: ['short_a', 'short_ab'], raw: ALL5 },
  { pivot: K50, label: 'K50', op: 'Le', filtered: ['long_exact', 'short_a', 'short_ab'], raw: ALL5 },
  // PROBE15 [ALPHA/BETA Eq]: the decisive split of the collision group.
  { pivot: VALUE_AAA, label: 'AAA', op: 'Eq', filtered: IDS_AAA, raw: LONG3 },
  { pivot: VALUE_BBB, label: 'BBB', op: 'Eq', filtered: IDS_BBB, raw: LONG3 },
  // PROBE15 [a *]: a ONE-BYTE pivot. Zero padding still makes the index
  // over-return for Gt/Lt/Ne, so a post-filter gated on query length is
  // wrong here -- see the dedicated test below.
  { pivot: 'a', label: 'a', op: 'Eq', filtered: ['short_a'], raw: ['short_a'] },
  { pivot: 'a', label: 'a', op: 'Ne', filtered: [...LONG3, 'short_ab'], raw: ALL5 },
  { pivot: 'a', label: 'a', op: 'Gt', filtered: [...LONG3, 'short_ab'], raw: ALL5 },
  { pivot: 'a', label: 'a', op: 'Ge', filtered: ALL5, raw: ALL5 },
  { pivot: 'a', label: 'a', op: 'Lt', filtered: [], raw: ['short_a'] },
  { pivot: 'a', label: 'a', op: 'Le', filtered: ['short_a'], raw: ['short_a'] },
  // PROBE15 [ab *]: two bytes, same story.
  { pivot: 'ab', label: 'ab', op: 'Eq', filtered: ['short_ab'], raw: ['short_ab'] },
  { pivot: 'ab', label: 'ab', op: 'Gt', filtered: LONG3, raw: [...LONG3, 'short_ab'] },
  { pivot: 'ab', label: 'ab', op: 'Lt', filtered: ['short_a'], raw: ['short_a', 'short_ab'] },
  { pivot: 'ab', label: 'ab', op: 'Le', filtered: ['short_a', 'short_ab'], raw: ['short_a', 'short_ab'] },
]

describe('string post-filtering', () => {
  it('splits a collision group by its FULL value', async () => {
    // The decisive assertion: the raw index cannot tell alpha from beta, so a
    // reader with no post-filter returns all three long values for both.
    const r = await FcbReader.fromBytes(corpus('colliding_strings.fcb'))
    const a = await ids(await r.select({
      where: [{ field: COL, operator: 'Eq', value: VALUE_AAA }],
    }))
    const b = await ids(await r.select({
      where: [{ field: COL, operator: 'Eq', value: VALUE_BBB }],
    }))
    expect(sorted(a)).toEqual(sorted(IDS_AAA))
    expect(sorted(b)).toEqual(sorted(IDS_BBB))
    expect(a.length).toBeGreaterThan(0)
    expect(b.length).toBeGreaterThan(0)
    expect(a.some((id) => b.includes(id))).toBe(false) // disjoint

    // ...and the candidate sets really were identical, so the split above is
    // the post-filter's work and not a lucky index.
    const candA = await candidateIds('colliding_strings.fcb',
      { field: COL, operator: 'Eq', value: VALUE_AAA })
    const candB = await candidateIds('colliding_strings.fcb',
      { field: COL, operator: 'Eq', value: VALUE_BBB })
    expect(sorted(candA)).toEqual(sorted(LONG3))
    expect(sorted(candB)).toEqual(sorted(LONG3))
  })

  it('post-filters SHORT queries too, and is not gated on query length', async () => {
    // A one-byte pivot. Its 50-byte key is 'a' followed by 49 zero bytes, and
    // the index's Gt/Lt bounds are NON-STRICT for string kinds (so the
    // equal-prefix band survives to be judged here). Consequence: Gt('a')
    // hands back short_a itself as a candidate and Lt('a') does too. A
    // post-filter that skipped short queries would return them.
    const r = await FcbReader.fromBytes(corpus('colliding_strings.fcb'))
    const gt = await ids(await r.select({
      where: [{ field: COL, operator: 'Gt', value: 'a' }],
    }))
    expect(sorted(gt)).toEqual(sorted([...LONG3, 'short_ab']))
    expect(gt).not.toContain('short_a')
    expect(sorted(await candidateIds('colliding_strings.fcb',
      { field: COL, operator: 'Gt', value: 'a' }))).toEqual(sorted(ALL5))

    const lt = await ids(await r.select({
      where: [{ field: COL, operator: 'Lt', value: 'a' }],
    }))
    expect(lt).toEqual([])
    expect(await candidateIds('colliding_strings.fcb',
      { field: COL, operator: 'Lt', value: 'a' })).toEqual(['short_a'])
  })

  it('reports featuresCount AFTER post-filtering, not the candidate count', async () => {
    const r = await FcbReader.fromBytes(corpus('colliding_strings.fcb'))
    const cursor = await r.select({
      where: [{ field: COL, operator: 'Eq', value: K50 }],
    })
    // The index answers with all three long values; only long_exact really
    // equals the pivot. A count taken before filtering reports 3.
    expect(cursor.featuresCount).toBe(1)
    expect(await ids(cursor)).toEqual(['long_exact'])
  })

  it('pages the FILTERED list, not the candidate list', async () => {
    const r = await FcbReader.fromBytes(corpus('colliding_strings.fcb'))
    const cursor = await r.select({
      where: [{ field: COL, operator: 'Ne', value: K50 }], limit: 3,
    })
    // Stored order is long_a, long_b, long_exact, short_a, short_ab, and hits
    // are sorted by offset. Candidates: all five. Matches: four (long_exact
    // drops out). So the first page of THREE matches is long_a, long_b,
    // short_a -- whereas slicing the CANDIDATE list first takes long_a,
    // long_b, long_exact and, after filtering, yields only two features.
    // Asserting the page's membership (not merely its length) is what tells
    // those two orderings apart.
    expect(cursor.featuresCount).toBe(4)
    const page = await ids(cursor)
    expect(page).toEqual(['long_a', 'long_b', 'short_a'])
    expect(page).not.toContain('long_exact')
    expect(sorted(await candidateIds('colliding_strings.fcb',
      { field: COL, operator: 'Ne', value: K50 }))).toEqual(sorted(ALL5))
  })

  it.each(ORACLE_STRING_OPS.map((row) => [`${row.label} ${row.op}`, row] as const))(
    'applies %s to the full value, matching the C++ reader', async (_name, row) => {
      // Index bounds for strings are deliberately NON-strict so equal-prefix
      // candidates survive to be judged here; the real operator is applied to
      // the untruncated value. Both lists pinned from C++ in Step 0.
      const r = await FcbReader.fromBytes(corpus('colliding_strings.fcb'))
      const cond: AttrCondition = { field: COL, operator: row.op, value: row.pivot }
      const hit = await ids(await r.select({ where: [cond] }))
      expect(sorted(hit)).toEqual(sorted(row.filtered))
      expect(sorted(await candidateIds('colliding_strings.fcb', cond)))
        .toEqual(sorted(row.raw))
    })

  it('narrows strictly for at least half the probed operators', () => {
    // Guards the oracle table itself: if every row had raw === filtered, the
    // whole it.each above would pass with the post-filter deleted.
    const narrowing = ORACLE_STRING_OPS.filter((r) => r.raw.length > r.filtered.length)
    expect(narrowing.length).toBeGreaterThanOrEqual(ORACLE_STRING_OPS.length / 2)
  })

  it('orders full strings by UTF-8 bytes, not by JS UTF-16 comparison', () => {
    // Same non-BMP hazard as the index keys: JS `<` is UTF-16 code-unit order
    // and disagrees with the byte order the reference compares in. Every
    // ASCII case passes either way, which is what makes this worth pinning.
    expect(compareFullStrings('｡', '\u{10000}')).toBeLessThan(0)
    expect('｡' < '\u{10000}').toBe(false)
    expect(compareFullStrings('a', 'a')).toBe(0)
    expect(compareFullStrings('a', 'ab')).toBeLessThan(0)
    expect(compareFullStrings('ab', 'a')).toBeGreaterThan(0)
    // A NUL byte is a real byte, not a terminator: 'a' < 'a\0'.
    expect(compareFullStrings('a', 'a ')).toBeLessThan(0)
  })
})

// ---------------------------------------------------------------------------
// Composition needs a fixture whose features are spatially DISTINCT.
// multi_object_attrs.fcb, colliding_strings.fcb and every other synthetic
// fixture give all their features the identical extent [0,0]-[1,1], so no
// bbox can separate them and any "spatial n attribute" test on them passes
// with the spatial half ignored. small.fcb is the smallest corpus file with
// three genuinely separated buildings AND indexed attributes.
//
// Attribute sets pinned from the same C++ probe (PROBE15S):
//   b3_dak_type Eq 'horizontal' -> ...016459  ...012869
//   b3_h_dak_50p Ge 2.0         -> ...016459  ...005156  ...012869
// Extents come from the brute-force `featureBounds` oracle, never from the
// R-tree, and never from header.geographicalExtent (which is metadata far
// larger than the features -- see packed-rtree.test.ts).
//   ...016459  x 85563.75..85566.85  y 446828.08..446832.49
//   ...005156  x 84734.81..84746.95  y 446636.54..446651.05
//   ...012869  x 84593.25..84597.51  y 446459.60..446462.77
// ---------------------------------------------------------------------------
const A = 'NL.IMBAG.Pand.0503100000016459'
const B = 'NL.IMBAG.Pand.0503100000005156'
const C = 'NL.IMBAG.Pand.0503100000012869'
const ORACLE_DAK_TYPE_HORIZONTAL = [A, C]
const ORACLE_H50P_GE_2 = [A, B, C]
/** Covers B and C, excludes A. */
const BOX_BC: [number, number, number, number] = [84500, 446400, 84800, 446700]
/** Covers A and B, excludes C. */
const BOX_AB: [number, number, number, number] = [84700, 446600, 85600, 446900]

/** Brute-force spatial oracle: which features really meet a box, computed
 *  from their own vertices with no index involved. */
async function bruteBbox(file: string, q: [number, number, number, number]) {
  const r = await FcbReader.fromBytes(corpus(file))
  const out: string[] = []
  for await (const f of await r.selectAll()) {
    const b = featureBounds(f, r.header)
    if (b.maxX >= q[0] && b.minX <= q[2] && b.maxY >= q[1] && b.minY <= q[3]) out.push(f.id)
  }
  return out
}

describe('composition', () => {
  it('intersects a spatial and an attribute predicate', async () => {
    // The box EXCLUDES A, which the attribute predicate matches, and the
    // attribute predicate excludes B, which the box matches -- so a reader
    // that dropped either half returns two ids where the answer is one.
    const spatial = await bruteBbox('small.fcb', BOX_BC)
    expect(sorted(spatial)).toEqual(sorted([B, C]))

    const r = await FcbReader.fromBytes(corpus('small.fcb'))
    const where: AttrCondition[] = [
      { field: 'b3_dak_type', operator: 'Eq', value: 'horizontal' },
    ]
    const attrOnly = await ids(await r.select({ where }))
    expect(sorted(attrOnly)).toEqual(sorted(ORACLE_DAK_TYPE_HORIZONTAL))

    const both = await ids(await r.select({ spatial: { kind: 'bbox', value: BOX_BC }, where }))
    expect(sorted(both)).toEqual(sorted(
      ORACLE_DAK_TYPE_HORIZONTAL.filter((id) => spatial.includes(id))))
    expect(both).toEqual([C])
    expect(both.length).toBeLessThan(attrOnly.length) // the bbox really cut
    expect(both.length).toBeLessThan(spatial.length) // ...and so did `where`
    expect(both.length).toBeGreaterThan(0)
  })

  it('counts and pages the INTERSECTED list', async () => {
    const spatial = await bruteBbox('small.fcb', BOX_AB)
    expect(sorted(spatial)).toEqual(sorted([A, B]))

    const r = await FcbReader.fromBytes(corpus('small.fcb'))
    const where: AttrCondition[] = [
      { field: 'b3_h_dak_50p', operator: 'Ge', value: 2.0 },
    ]
    expect(sorted(await ids(await r.select({ where })))).toEqual(sorted(ORACLE_H50P_GE_2))

    const paged = await r.select({
      spatial: { kind: 'bbox', value: BOX_AB }, where, limit: 1,
    })
    // Attribute-only is all three; the intersection is two. A count taken on
    // either side alone reports 3 or 2-but-wrong-members.
    expect(paged.featuresCount).toBe(2)
    const page = await ids(paged)
    expect(page).toHaveLength(1)
    expect(spatial).toContain(page[0]!)
  })

  it('post-filters the intersection, not just the attribute side', async () => {
    // A string predicate whose index answer is a superset, intersected with a
    // box. The post-filter has to run after the intersection for the count to
    // be right -- here Ne('slanted') is a full leaf scan in the index.
    const r = await FcbReader.fromBytes(corpus('small.fcb'))
    const where: AttrCondition[] = [
      { field: 'b3_dak_type', operator: 'Ne', value: 'slanted' },
    ]
    const cand = await candidateIds('small.fcb', where[0]!)
    expect(sorted(cand)).toEqual(sorted([A, B, C])) // Ne is a full leaf scan
    const both = await r.select({ spatial: { kind: 'bbox', value: BOX_BC }, where })
    expect(both.featuresCount).toBe(1)
    expect(await ids(both)).toEqual([C])
  })

  it('rejects nearest combined with where', async () => {
    const r = await FcbReader.fromBytes(corpus('small.fcb'))
    await expect(r.select({
      spatial: { kind: 'nearest', value: [0, 0] },
      where: [{ field: 'b3_dak_type', operator: 'Eq', value: 'horizontal' }],
    })).rejects.toThrow(/unsupported query combination/i)
  })

  it('does NOT reject nearest on its own -- the combination check is not over-broad', async () => {
    // The `nearest` + `where` rejection above must fire ONLY on the
    // combination: a plain `nearest` (Task 16) is a valid query and returns
    // one feature. Full nearest behaviour is pinned in test/nearest.test.ts.
    const r = await FcbReader.fromBytes(corpus('small.fcb'))
    const hit = await r.select({ spatial: { kind: 'nearest', value: [0, 0] } })
    expect(hit.featuresCount).toBe(1)
    expect(await ids(hit)).toHaveLength(1)
  })
})
