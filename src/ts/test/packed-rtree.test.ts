import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'
import { FcbReader } from '../src/reader.js'
import { BufferedRangeReader } from '../src/io/range-reader.js'
import { rtreeIndexSize } from '../src/layout.js'
import {
  NODE_ITEM_SIZE, containsPoint, decodeNodeItem, generateLevelBounds, intersects, rtreeNumNodes,
} from '../src/packed-rtree/index.js'
import type { SpatialQuery } from '../src/packed-rtree/index.js'
import { CountingReader } from './fixtures/counting-reader.js'
import { featureBounds } from './fixtures/feature-bounds.js'
import type { Bounds } from './fixtures/feature-bounds.js'

// `__dirname` does not exist under ESM; `import.meta.dirname` is its
// replacement (Node >= 22.12, which package.json already requires).
const CORPUS = resolve(import.meta.dirname, '../../../conformance')
const bytes = (n: string) => new Uint8Array(readFileSync(resolve(CORPUS, n)))
const ids = async (c: AsyncIterable<{ id: string }>) => {
  const out: string[] = []
  for await (const f of c) out.push(f.id)
  return out
}

/** Every feature's id and its own extent, from a full scan that never touches
 *  the index. This is the brute-force oracle everything below is measured
 *  against, and it is also where the query boxes come from.
 *
 *  Query boxes must NOT be derived from `header.geographicalExtent`. That
 *  extent is metadata carried over from the source CityJSON and can be far
 *  larger than the union of the features actually in the file -- in small.fcb
 *  it is a ~1.2 km tile holding three buildings that span ~1 km in x but only
 *  ~370 m in y, all of them in the top-right corner. Halving the header extent
 *  therefore produces a box that contains NOTHING, and a "proper subset" or
 *  "agrees with brute force" assertion built on it passes vacuously
 *  (`[] === []`) no matter how broken the traversal is. */
async function scanBounds(r: FcbReader) {
  const out: Array<{ id: string; b: Bounds }> = []
  for await (const f of await r.selectAll()) out.push({ id: f.id, b: featureBounds(f, r.header) })
  return out
}

/** The union of every feature's own extent -- the box the index actually
 *  covers, as opposed to the one the metadata claims. */
function union(bs: Array<{ b: Bounds }>): Bounds {
  return bs.reduce<Bounds>((a, { b }) => ({
    minX: Math.min(a.minX, b.minX),
    minY: Math.min(a.minY, b.minY),
    maxX: Math.max(a.maxX, b.maxX),
    maxY: Math.max(a.maxY, b.maxY),
  }), { minX: Infinity, minY: Infinity, maxX: -Infinity, maxY: -Infinity })
}

const bruteBbox = (bs: Array<{ id: string; b: Bounds }>, q: [number, number, number, number]) =>
  bs.filter(({ b }) => b.maxX >= q[0] && b.minX <= q[2] && b.maxY >= q[1] && b.minY <= q[3])
    .map((x) => x.id)

describe('NodeItem', () => {
  it('decodes 40 little-endian bytes', async () => {
    const raw = new Uint8Array(40)
    const one = [0, 0, 0, 0, 0, 0, 0xf0, 0x3f] // 1.0 as IEEE-754 LE
    for (let f = 0; f < 4; f++) raw.set(one, f * 8)
    raw[32] = 0x2a // offset = 42
    const n = decodeNodeItem(raw, 0)
    expect(n).toEqual({ minX: 1, minY: 1, maxX: 1, maxY: 1, offset: 42 })
    expect(NODE_ITEM_SIZE).toBe(40)
  })

  it('matches NodeItem::intersects boundary semantics exactly', () => {
    // packed_rtree/mod.rs:122-137 compares with STRICT </>, so boxes that
    // merely touch DO intersect. An inclusive/exclusive slip here silently
    // changes every query result.
    const n = { minX: 0, minY: 0, maxX: 10, maxY: 10, offset: 0 }
    expect(intersects(n, { minX: 5, minY: 5, maxX: 6, maxY: 6 })).toBe(true) // inside
    expect(intersects(n, { minX: -5, minY: -5, maxX: 5, maxY: 5 })).toBe(true) // overlap
    expect(intersects(n, { minX: -5, minY: -5, maxX: 20, maxY: 20 })).toBe(true) // enclosing
    expect(intersects(n, { minX: 10, minY: 10, maxX: 20, maxY: 20 })).toBe(true) // corner touch
    expect(intersects(n, { minX: -5, minY: -5, maxX: 0, maxY: 0 })).toBe(true) // corner touch
    expect(intersects(n, { minX: 10.1, minY: 0, maxX: 20, maxY: 10 })).toBe(false)
    expect(intersects(n, { minX: -20, minY: 0, maxX: -0.1, maxY: 10 })).toBe(false)
    expect(intersects(n, { minX: 0, minY: 10.1, maxX: 10, maxY: 20 })).toBe(false)
  })

  it('is the same predicate as contains_point on a degenerate box', () => {
    // This equivalence is why the traversal lowers a point query to a bbox
    // instead of carrying a second descent (mod.rs:531-560 duplicates it).
    const n = { minX: 0, minY: 0, maxX: 10, maxY: 10, offset: 0 }
    for (const [x, y] of [[5, 5], [0, 0], [10, 10], [-0.1, 5], [5, 10.1], [10.1, 10.1]]) {
      expect(intersects(n, { minX: x!, minY: y!, maxX: x!, maxY: y! }))
        .toBe(containsPoint(n, x!, y!))
    }
  })

  it('orders level bounds leaf-first-in-array, leaf-last-in-storage', () => {
    // generate_level_bounds (mod.rs:342-375). Entry 0 is the LEAF level and
    // holds the HIGHEST node indices; the last entry is the root at node 0.
    // Inverting this traverses without erroring and returns wrong answers.
    expect(generateLevelBounds(3, 16)).toEqual([{ start: 1, end: 4 }, { start: 0, end: 1 }])
    expect(generateLevelBounds(12, 16)).toEqual([{ start: 1, end: 13 }, { start: 0, end: 1 }])
    expect(generateLevelBounds(12, 8)).toEqual([
      { start: 3, end: 15 }, { start: 1, end: 3 }, { start: 0, end: 1 },
    ])
    // ... and the node count each implies matches layout.ts's rtreeIndexSize,
    // which is derived independently from mod.rs:879-898.
    expect(rtreeNumNodes(12, 16) * 40).toBe(rtreeIndexSize(12, 16))
    expect(rtreeNumNodes(12, 8) * 40).toBe(rtreeIndexSize(12, 8))
    expect(rtreeNumNodes(3, 16) * 40).toBe(rtreeIndexSize(3, 16))
  })
})

describe('bbox search', () => {
  it('returns every feature for a bbox covering the whole extent', async () => {
    const r = await FcbReader.fromBytes(bytes('small.fcb'))
    const e = r.header.info.geographicalExtent!
    const all = await ids(await r.selectAll())
    const hit = await ids(await r.select({
      spatial: { kind: 'bbox', value: [e[0], e[1], e[3], e[4]] },
    }))
    expect(hit.sort()).toEqual(all.sort())
  })

  it('returns nothing for a bbox outside the extent', async () => {
    const r = await FcbReader.fromBytes(bytes('small.fcb'))
    const cursor = await r.select({
      spatial: { kind: 'bbox', value: [1e9, 1e9, 1e9 + 1, 1e9 + 1] },
    })
    expect(cursor.featuresCount).toBe(0) // 0, never undefined
    expect(await ids(cursor)).toEqual([])
  })

  it('returns a PROPER SUBSET for a bbox covering part of the extent', async () => {
    // A whole-extent bbox proves nothing: a search that ignores the bbox
    // entirely passes it. The real assertion is that a partial box excludes
    // somebody, and that the survivors are the ones whose own bbox overlaps.
    const r = await FcbReader.fromBytes(bytes('small.fcb'))
    const bs = await scanBounds(r)
    const u = union(bs)
    const half: [number, number, number, number] =
      [u.minX, u.minY, (u.minX + u.maxX) / 2, (u.minY + u.maxY) / 2]
    const hit = await ids(await r.select({ spatial: { kind: 'bbox', value: half } }))
    expect(hit.length).toBeGreaterThan(0)
    expect(hit.length).toBeLessThan(bs.length) // something was excluded
    expect(hit.every((id) => bs.some((x) => x.id === id))).toBe(true)
  })

  it('agrees with a brute-force scan over every feature bbox', async () => {
    // The oracle that cannot be tautological: compute the answer without the
    // R-tree at all, by scanning every feature and testing its own extent.
    const r = await FcbReader.fromBytes(bytes('small.fcb'))
    const bs = await scanBounds(r)
    const u = union(bs)
    const box: [number, number, number, number] =
      [u.minX, u.minY, (u.minX + u.maxX) / 2, (u.minY + u.maxY) / 2]
    const brute = bruteBbox(bs, box)
    expect(brute.length).toBeGreaterThan(0) // guard against a vacuous pass
    expect(brute.length).toBeLessThan(bs.length)
    const hit = await ids(await r.select({ spatial: { kind: 'bbox', value: box } }))
    expect(hit.sort()).toEqual(brute.sort())
  })

  it('agrees with the brute-force scan on a 12-feature, multi-level tree', async () => {
    // small.fcb has 3 features and a 2-level tree: every leaf lives under the
    // root, so a descent bug that mishandles an intermediate level cannot show
    // up there. appearance_depths_node8.fcb is 12 items at node size 8, i.e.
    // level bounds [12, 2, 1] -- three levels, and two sibling leaf ranges.
    const r = await FcbReader.fromBytes(bytes('appearance_depths_node8.fcb'))
    const bs = await scanBounds(r)
    const u = union(bs)
    const box: [number, number, number, number] = [u.minX, u.minY, u.maxX, u.maxY]
    const brute = bruteBbox(bs, box)
    expect(brute.length).toBe(12)
    const hit = await ids(await r.select({ spatial: { kind: 'bbox', value: box } }))
    expect(hit.sort()).toEqual(brute.sort())
    // A duplicated result is the failure mode of EVALUATING the extra node the
    // +1 leaf fetch rule pulls in -- that node belongs to the next parent's
    // sibling group, and its parent matches whenever it does. With two leaf
    // ranges (nodes 3..11 and 11..15) this query walks that boundary, so a
    // duplicate would show up right here. Assert the answer is a SET.
    expect(new Set(hit).size).toBe(hit.length)

    // ... and a box disjoint from every feature must come back empty.
    const away: [number, number, number, number] =
      [u.maxX + 10, u.maxY + 10, u.maxX + 20, u.maxY + 20]
    expect(bruteBbox(bs, away)).toEqual([])
    expect(await ids(await r.select({ spatial: { kind: 'bbox', value: away } }))).toEqual([])
  })

  it('treats pointIntersects as a degenerate bbox, against the brute oracle', async () => {
    // Comparing point search to bbox search lets BOTH be identically wrong.
    const r = await FcbReader.fromBytes(bytes('small.fcb'))
    const bs = await scanBounds(r)
    // The centre of ONE feature, so the expected answer is known to be
    // non-empty. The centre of the header extent lands in open ground and
    // would make this pass on [] === [].
    const target = bs[0]!.b
    const cx = (target.minX + target.maxX) / 2, cy = (target.minY + target.maxY) / 2
    const brute = bs
      .filter(({ b }) => b.minX <= cx && cx <= b.maxX && b.minY <= cy && cy <= b.maxY)
      .map((x) => x.id)
    expect(brute.length).toBeGreaterThan(0)
    const p = await ids(await r.select({ spatial: { kind: 'point', value: [cx, cy] } }))
    expect(p.sort()).toEqual(brute.sort())
  })

  it('finds nothing for a point outside every feature, matching the oracle', async () => {
    const r = await FcbReader.fromBytes(bytes('small.fcb'))
    const bs = await scanBounds(r)
    const u = union(bs)
    const cx = u.maxX + 100, cy = u.maxY + 100
    expect(bs.some(({ b }) => b.minX <= cx && cx <= b.maxX && b.minY <= cy && cy <= b.maxY))
      .toBe(false)
    expect(await ids(await r.select({ spatial: { kind: 'point', value: [cx, cy] } }))).toEqual([])
  })

  it('honours a NON-DEFAULT index_node_size from the header', async () => {
    // Both the wasm binding and fcb_core's HTTP reader hardcode 16 here and
    // silently mis-traverse such files -- upstream finding, Task 18. Without
    // this fixture a hardcoded 16 passes the entire suite.
    // Generated in Task 2 from appearance_depths (12 features) with
    // --index-node-size 8. It must NOT be built from small: at 3 features
    // both node sizes give the identical level bounds [3, 1], so the fixture
    // would pass for a hardcoded-16 reader too.
    const r = await FcbReader.fromBytes(bytes('appearance_depths_node8.fcb'))
    expect(r.header.info.indexNodeSize).toBe(8)
    const e = r.header.info.geographicalExtent!
    const all = await ids(await r.selectAll())
    const hit = await ids(await r.select({
      spatial: { kind: 'bbox', value: [e[0], e[1], e[3], e[4]] },
    }))
    expect(hit.sort()).toEqual(all.sort())
  })

  it('rejects an inverted or non-finite bbox before doing any I/O', async () => {
    const r = await FcbReader.fromBytes(bytes('small.fcb'))
    await expect(r.select({ spatial: { kind: 'bbox', value: [10, 10, 0, 0] } }))
      .rejects.toThrow(/invalid/i)
    await expect(r.select({ spatial: { kind: 'bbox', value: [NaN, 0, 1, 1] } }))
      .rejects.toThrow(/invalid/i)
  })

  it('rejects a non-finite point', async () => {
    const r = await FcbReader.fromBytes(bytes('small.fcb'))
    await expect(r.select({ spatial: { kind: 'point', value: [NaN, 0] } }))
      .rejects.toThrow(/invalid/i)
  })
})

describe('not yet implemented', () => {
  it('rejects an attribute query until Task 14 lands', async () => {
    const r = await FcbReader.fromBytes(bytes('small.fcb'))
    await expect(r.select({ where: [{ field: 'a', operator: 'Eq', value: 1 }] }))
      .rejects.toThrow(/attribute queries not implemented yet/)
  })

  it('rejects a nearest query until Task 16 lands', async () => {
    const r = await FcbReader.fromBytes(bytes('small.fcb'))
    await expect(r.select({ spatial: { kind: 'nearest', value: [0, 0] } }))
      .rejects.toThrow(/nearest queries not implemented yet/)
  })
})

describe('cancellation', () => {
  it('threads the signal into the traversal, not just the facade', async () => {
    const r = await FcbReader.fromBytes(bytes('small.fcb'))
    const e = r.header.info.geographicalExtent!
    const ac = new AbortController()
    ac.abort()
    await expect(r.select({
      spatial: { kind: 'bbox', value: [e[0], e[1], e[3], e[4]] },
      signal: ac.signal,
    })).rejects.toThrow(/abort/i)
  })
})

describe('pagination', () => {
  it('pages results while featuresCount still reports the total', async () => {
    const r = await FcbReader.fromBytes(bytes('small.fcb'))
    const all = await ids(await r.selectAll())
    const cursor = await r.select({ limit: 2, offset: 1 })
    expect(cursor.featuresCount).toBe(all.length)
    expect(await ids(cursor)).toEqual(all.slice(1, 3))
  })

  it('pages a spatial result set while featuresCount reports the match total', async () => {
    const r = await FcbReader.fromBytes(bytes('appearance_depths_node8.fcb'))
    const e = r.header.info.geographicalExtent!
    const whole: SpatialQuery = { kind: 'bbox', value: [e[0]!, e[1]!, e[3]!, e[4]!] }
    const all = await ids(await r.select({ spatial: whole }))
    const page = await r.select({ spatial: whole, limit: 3, offset: 2 })
    expect(page.featuresCount).toBe(all.length)
    expect(await ids(page)).toEqual(all.slice(2, 5))
  })

  it('rejects a negative or fractional limit/offset', async () => {
    const r = await FcbReader.fromBytes(bytes('small.fcb'))
    await expect(r.select({ limit: -1 })).rejects.toThrow(/invalid/i)
    await expect(r.select({ offset: 1.5 })).rejects.toThrow(/invalid/i)
  })
})

describe('request pattern', () => {
  it('reads the rtree in far fewer requests than it has nodes', async () => {
    // Correct-but-chatty is the failure mode nobody notices until it is on a
    // CDN. Assert the request LOG, not just the bytes.
    //
    // CRITICAL: a default 1 MB BufferedRangeReader swallows all of
    // small.fcb (20 KB) on the first read, so clearing inner.reads after
    // open would measure an already-warm cache and ZERO subsequent reads
    // would pass regardless of how bad the traversal planning is. Use a
    // buffer far smaller than the file so misses are real.
    const data = bytes('small.fcb')
    const inner = new CountingReader(data)
    const r = await FcbReader.fromReader(new BufferedRangeReader(inner, 512))
    const e = r.header.info.geographicalExtent!
    expect(inner.reads.reduce((n, x) => n + x.length, 0)).toBeLessThan(data.length)
    inner.reads.length = 0
    await r.select({ spatial: { kind: 'bbox', value: [e[0], e[1], e[3], e[4]] } })
    expect(inner.reads.length).toBeGreaterThan(0) // it really read
    expect(inner.reads.length).toBeLessThan(r.header.info.featuresCount)
  })

  it('issues one read PER NODE RANGE, not per node -- measured unbuffered', async () => {
    // The buffered test above cannot separate a good traversal from a chatty
    // one: small.fcb's whole R-tree is 4 nodes / 160 bytes, so ANY traversal
    // -- even one 40-byte read per node -- is absorbed by the first 512-byte
    // over-fetch and shows up as a single inner read. Handing the reader an
    // UNBUFFERED CountingReader makes every logical read visible, and then the
    // count is a direct measure of traversal planning.
    //
    // appearance_depths_node8.fcb: 12 items at node size 8 -> level bounds
    // [12, 2, 1], 15 nodes. A range-driven descent reads the root range, the
    // level-1 range, then one range per intersecting level-1 node: 4 reads for
    // a whole-extent query. A per-node traversal would read 15.
    const inner = new CountingReader(bytes('appearance_depths_node8.fcb'))
    const r = await FcbReader.fromReader(inner)
    const e = r.header.info.geographicalExtent!
    inner.reads.length = 0
    await r.select({ spatial: { kind: 'bbox', value: [e[0], e[1], e[3], e[4]] } })
    expect(inner.reads.length).toBe(4)
    // Every read must be a whole number of 40-byte node items.
    for (const rd of inner.reads) expect(rd.length % 40).toBe(0)
  })
})
