import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'
import { FcbReader } from '../src/reader.js'
import { BufferedRangeReader } from '../src/io/range-reader.js'
import { CountingReader } from './fixtures/counting-reader.js'
import { featureBounds } from './fixtures/feature-bounds.js'
import { featureById } from './fixtures/feature-by-id.js'

// `__dirname` does not exist under ESM (this package is "type": "module");
// the port-wide convention is `import.meta.dirname`, as in every other test.
const CORPUS = resolve(import.meta.dirname, '../../../conformance')
const bytes = (n: string) => new Uint8Array(readFileSync(resolve(CORPUS, n)))
const ids = async (c: AsyncIterable<{ id: string }>) => {
  const out: string[] = []
  for await (const f of c) out.push(f.id)
  return out
}

describe('pointNearest', () => {
  it('returns exactly one feature for a point inside the extent', async () => {
    const r = await FcbReader.fromBytes(bytes('small.fcb'))
    const e = r.header.info.geographicalExtent!
    const cursor = await r.select({
      spatial: { kind: 'nearest', value: [(e[0] + e[3]) / 2, (e[1] + e[4]) / 2] },
    })
    expect(cursor.featuresCount).toBe(1)
    expect(await ids(cursor)).toHaveLength(1)
  })

  it('still returns one feature for a point far outside the extent', async () => {
    // Nothing prunes it away: min-distance ordering is a lower bound, not a
    // rejection test. An empty result here means the termination is wrong.
    const r = await FcbReader.fromBytes(bytes('small.fcb'))
    expect((await ids(await r.select({
      spatial: { kind: 'nearest', value: [1e9, 1e9] },
    })))).toHaveLength(1)
  })

  it('returns the ONE feature of a single-feature file', async () => {
    const r = await FcbReader.fromBytes(bytes('single_feature.fcb'))
    const all = await ids(await r.selectAll())
    expect(all).toHaveLength(1)
    const hit = await ids(await r.select({
      spatial: { kind: 'nearest', value: [0, 0] },
    }))
    expect(hit).toEqual(all)
  })

  it('agrees with a brute-force scan over every feature CENTROID', async () => {
    // The oracle that does not depend on heap order: actually compute the
    // nearest centroid by scanning every feature, then compare DISTANCE --
    // not identity -- so an exact tie does not make the test flaky.
    //
    // Note the metric: leaves are scored by distance to the bbox CENTROID,
    // not to the nearest point of the bbox. Scoring by min-distance here
    // would make this test disagree with all three Rust forms.
    const r = await FcbReader.fromBytes(bytes('small.fcb'))
    const e = r.header.info.geographicalExtent!
    for (const [px, py] of [
      [(e[0] + e[3]) / 2, (e[1] + e[4]) / 2],   // middle
      [e[0], e[1]],                              // a corner
      [1e9, 1e9],                                // far outside
    ] as Array<[number, number]>) {
      let best = Number.POSITIVE_INFINITY
      for await (const f of await r.selectAll()) {
        const b = featureBounds(f, r.header)
        const cx = (b.minX + b.maxX) / 2, cy = (b.minY + b.maxY) / 2
        best = Math.min(best, (cx - px) ** 2 + (cy - py) ** 2)
      }
      const hit = await ids(await r.select({
        spatial: { kind: 'nearest', value: [px, py] },
      }))
      expect(hit).toHaveLength(1)
      const f = await featureById(r, hit[0]!)
      const b = featureBounds(f, r.header)
      const cx = (b.minX + b.maxX) / 2, cy = (b.minY + b.maxY) / 2
      expect((cx - px) ** 2 + (cy - py) ** 2).toBeCloseTo(best, 6)
    }
  })

  it('takes the STREAMING path for an index above the threshold', async () => {
    // The whole-index fast path would hide every bug in the best-first
    // traversal. delft.fcb's rtree is 47 KB, still under 256 KB, so force
    // the streaming path by lowering the threshold for this test.
    const r = await FcbReader.fromBytes(bytes('small.fcb'))
    const e = r.header.info.geographicalExtent!
    const px = (e[0] + e[3]) / 2, py = (e[1] + e[4]) / 2
    const fast = await ids(await r.select({ spatial: { kind: 'nearest', value: [px, py] } }))
    const streamed = await ids(await r.select({
      spatial: { kind: 'nearest', value: [px, py] }, wholeIndexThreshold: 0,
    }))
    expect(streamed).toEqual(fast)
  })

  it('agrees on a multi-level tree across both paths, against the centroid oracle', async () => {
    // small.fcb is a 2-level tree (3 features): every leaf lives directly
    // under the root, so a bug in descending an INTERMEDIATE level cannot
    // show up. appearance_depths_node8.fcb is 12 items at node size 8 --
    // level bounds [12, 2, 1], three levels -- so the streaming best-first
    // walk actually descends through a middle level here. Assert both paths
    // agree with the brute-force nearest CENTROID (distance, not identity).
    const r = await FcbReader.fromBytes(bytes('appearance_depths_node8.fcb'))
    const e = r.header.info.geographicalExtent!
    const px = (e[0] + e[3]) / 2, py = (e[1] + e[4]) / 2
    let best = Number.POSITIVE_INFINITY
    for await (const f of await r.selectAll()) {
      const b = featureBounds(f, r.header)
      const cx = (b.minX + b.maxX) / 2, cy = (b.minY + b.maxY) / 2
      best = Math.min(best, (cx - px) ** 2 + (cy - py) ** 2)
    }
    for (const threshold of [262144, 0]) {
      const hit = await ids(await r.select({
        spatial: { kind: 'nearest', value: [px, py] }, wholeIndexThreshold: threshold,
      }))
      expect(hit).toHaveLength(1)
      const f = await featureById(r, hit[0]!)
      const b = featureBounds(f, r.header)
      const cx = (b.minX + b.maxX) / 2, cy = (b.minY + b.maxY) / 2
      expect((cx - px) ** 2 + (cy - py) ** 2).toBeCloseTo(best, 6)
    }
  })

  it('reads the whole small index in ONE request', async () => {
    // The fast path: rtree_size is under the 256 KB threshold, so nearest
    // must not degenerate into one round trip per heap pop.
    const inner = new CountingReader(bytes('small.fcb'))
    const r = await FcbReader.fromReader(new BufferedRangeReader(inner))
    inner.reads.length = 0
    await r.select({ spatial: { kind: 'nearest', value: [0, 0] } })
    expect(inner.reads.length).toBeLessThanOrEqual(2)
  })

  it('threads the abort signal into the streaming traversal', async () => {
    // A signal that only lived on the facade would cancel nothing: the reads
    // are in the traversal. Force the streaming path (threshold 0) so a real
    // node read is issued, and pre-abort so the very first one rejects.
    const r = await FcbReader.fromBytes(bytes('small.fcb'))
    const ac = new AbortController()
    ac.abort()
    await expect(r.select({
      spatial: { kind: 'nearest', value: [0, 0] },
      wholeIndexThreshold: 0,
      signal: ac.signal,
    })).rejects.toThrow(/abort/i)
  })

  it('rejects a non-finite nearest point before any I/O', async () => {
    const inner = new CountingReader(bytes('small.fcb'))
    const r = await FcbReader.fromReader(inner)
    inner.reads.length = 0
    await expect(r.select({ spatial: { kind: 'nearest', value: [NaN, 0] } }))
      .rejects.toThrow(/invalid/i)
    expect(inner.reads.length).toBe(0)
  })
})
