import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'
import { FcbReader } from '../src/reader.js'

// `__dirname` does not exist under ESM (this package is "type": "module");
// the port-wide convention is `import.meta.dirname`, as in sources.test.ts.
const CORPUS = resolve(import.meta.dirname, '../../../conformance')
const DATA = resolve(import.meta.dirname, '../../../examples/data')
const open = async (p: string) =>
  FcbReader.fromBytes(new Uint8Array(readFileSync(p)))

/** The conformance oracle: one JSON line per CityJSONFeature, after the
 *  leading CityJSON metadata line. This is where an expected feature count
 *  comes from -- never from running this port's own reader. */
const expectedFeatureCount = (name: string) =>
  readFileSync(resolve(CORPUS, `${name}.expected.jsonl`), 'utf8')
    .split('\n')
    .filter((l) => l.includes('"type":"CityJSONFeature"'))
    .length

describe('sequential scan', () => {
  it('iterates a single-feature file exactly once', async () => {
    const r = await open(resolve(CORPUS, 'single_feature.fcb'))
    const cursor = await r.selectAll()
    const seen = []
    for await (const f of cursor) seen.push(f)
    expect(seen).toHaveLength(1)
  })

  it('yields exactly featuresCount features for small.fcb', async () => {
    const r = await open(resolve(CORPUS, 'small.fcb'))
    const cursor = await r.selectAll()
    let n = 0
    for await (const _ of cursor) n++
    expect(n).toBe(r.header.info.featuresCount)
  })

  it('scans no_count.fcb to EOF despite a declared count of 0', async () => {
    // featuresCount 0 means UNKNOWN, not empty: the scan must run to the end
    // of the file. no_count.fcb also has no R-tree, so its feature order is
    // the write order and differs from Hilbert-sorted small.fcb -- hence its
    // own oracle file rather than small's.
    const r = await open(resolve(CORPUS, 'no_count.fcb'))
    expect(r.header.info.featuresCount).toBe(0)
    const seen: string[] = []
    for await (const f of await r.selectAll()) seen.push(f.id)
    expect(seen).toHaveLength(expectedFeatureCount('no_count'))
    expect(seen).toEqual([
      'NL.IMBAG.Pand.0503100000012869',
      'NL.IMBAG.Pand.0503100000016459',
      'NL.IMBAG.Pand.0503100000005156',
    ])
  })

  it('yields durable handles that survive a reader which REUSES its buffer', async () => {
    // fromBytes owns an immutable whole-file copy, so subarray-backed
    // features would stay valid there and this test would pass even without
    // per-feature copying. Go through a reader whose buffer is replaced on
    // every read -- that is the case the copy exists for.
    const raw = new Uint8Array(readFileSync(resolve(CORPUS, 'small.fcb')))
    const churning = {
      size: () => raw.length,
      async read(offset: number, length: number): Promise<Uint8Array> {
        // A fresh buffer each time, then scribbled over on the NEXT read.
        const b = new Uint8Array(length)
        b.set(raw.subarray(offset, offset + length))
        churning.last?.fill(0xdd)
        churning.last = b
        return b
      },
      last: undefined as Uint8Array | undefined,
    }
    const r = await FcbReader.fromReader(churning)
    const held = []
    for await (const f of await r.selectAll()) held.push(f)
    expect(held.length).toBeGreaterThan(1)
    // Touch the FIRST feature after the cursor has moved far past it, and
    // touch a generated array accessor, which is what aliasing breaks.
    expect(held[0]!.id).not.toBe(held[held.length - 1]!.id)
    expect(held[0]!.vertices().length).toBeGreaterThan(0)
    expect(held[0]!.cityObjects().length).toBeGreaterThan(0)
  })

  it('serializes overlapping next() calls instead of interleaving position', async () => {
    // A native async generator gives this for free. Both must resolve to
    // DIFFERENT features -- interleaved position updates would return the
    // same one twice or skip one.
    const r = await open(resolve(CORPUS, 'small.fcb'))
    const it = (await r.selectAll())[Symbol.asyncIterator]()
    const [a, b] = await Promise.all([it.next(), it.next()])
    expect(a.value.id).not.toBe(b.value.id)
  })

  it('releases a closeable underlying reader, including via await using', async () => {
    const bytes = new Uint8Array(readFileSync(resolve(CORPUS, 'small.fcb')))
    let closes = 0
    const closeable = {
      size: () => bytes.length,
      read: async (o: number, l: number) => bytes.subarray(o, o + l),
      close: async () => { closes++ },
    }
    {
      // `await using` is the disposal path Symbol.asyncDispose exists for.
      await using r = await FcbReader.fromReader(closeable)
      expect(r.header.info.featuresCount).toBeGreaterThan(0)
    }
    expect(closes).toBe(1)
    // A reader with nothing to release still resolves, and close() is
    // idempotent so a manual close() before scope exit is not an error.
    const plain = await FcbReader.fromBytes(bytes)
    await expect(plain.close()).resolves.toBeUndefined()
    await expect(plain.close()).resolves.toBeUndefined()
  })
})

describe('attribute schema resolution', () => {
  it('uses each object OWN columns when it declares them', async () => {
    const r = await open(resolve(DATA, 'delft.fcb'))
    let checked = 0
    for await (const f of await r.selectAll()) {
      f.cityObjects().forEach((o, i) => {
        if (!o.hasAttributes() || !o.hasColumns()) return
        // A wrong schema shows up as a nonsense key, not an exception:
        // during the C++ port it surfaced as column index 28777, which is
        // ASCII "ip" from the middle of a string value.
        for (const key of Object.keys(f.attributes(i))) {
          expect(key).toMatch(/^[\x20-\x7e]+$/)
          checked++
        }
      })
    }
    expect(checked).toBeGreaterThan(0)
  })

  it("pins one CityObject's decoded attributes against the CityJSONSeq oracle", async () => {
    // The regex test above only proves keys are printable, which is true of
    // ANY schema (including a wrongly-fallen-back-to header one) because
    // column names are printable by construction. This test pins a concrete
    // decoded result against an independent oracle, so a wrong schema shows
    // up as a value/key MISMATCH, not just non-garbage text.
    //
    // Chosen object: the `Building` CityObject of feature
    // `NL.IMBAG.Pand.0503100000012869` (the parent, sharing the feature's
    // id -- delft.fcb's first feature). It declares its OWN 43-entry column
    // schema, which differs from the header's 44 columns -- see the
    // "discriminates" assertion below, and the deliberate-fallback check
    // documented in the task-8 report. Decoding this object's attribute blob
    // against the header's 44-column schema instead of its own would
    // desynchronise the record stream (columns are not self-delimiting) and
    // either throw on an out-of-range column index or, worse, silently
    // produce different garbage keys/values -- either way this assertion
    // would fail, which is what makes it a real discriminator and not just
    // another printability check.
    const featureId = 'NL.IMBAG.Pand.0503100000012869'
    const oracleLine = readFileSync(resolve(DATA, 'delft.city.jsonl'), 'utf8')
      .split('\n')
      .find((l) => l.includes(`"id":"${featureId}"`))
    if (oracleLine === undefined) throw new Error(`oracle line for ${featureId} not found`)
    const oracleCo = (JSON.parse(oracleLine) as {
      CityObjects: Record<string, { attributes?: Record<string, unknown> }>
    }).CityObjects[featureId]
    if (oracleCo === undefined) throw new Error(`oracle CityObject ${featureId} not found`)
    // A record with a JSON `null` value has no attribute record at all in the
    // binary blob (nothing to decode for "absent"), so it is not expected to
    // come back out of `decodeAttributes` either.
    const expected: Record<string, unknown> = {}
    for (const [k, v] of Object.entries(oracleCo.attributes ?? {})) {
      if (v !== null) expected[k] = v
    }
    expect(Object.keys(expected).length).toBeGreaterThan(0)

    const r = await open(resolve(DATA, 'delft.fcb'))
    let actual: Record<string, unknown> | undefined
    let ownColumnCount: number | undefined
    for await (const f of await r.selectAll()) {
      if (f.id !== featureId) continue
      const idx = f.cityObjects().findIndex((o) => o.id === featureId)
      const obj = f.cityObjects()[idx]
      if (obj === undefined) continue
      ownColumnCount = obj.columns().length
      // Normalize bigint (the Long/ULong policy) back to number so this can
      // be compared structurally against oracle JSON, which has no bigint.
      actual = Object.fromEntries(
        Object.entries(f.attributes(idx)).map(([k, v]) => [k, typeof v === 'bigint' ? Number(v) : v]),
      )
      break
    }
    if (actual === undefined) throw new Error(`feature ${featureId} not found in delft.fcb`)

    // Discriminates: the object's own schema (43 columns) really does differ
    // from the header's (44) -- so a fallback to the header schema is not a
    // vacuous no-op on this file.
    expect(ownColumnCount).toBe(43)
    expect(r.header.info.columns.length).toBe(44)
    expect(ownColumnCount).not.toBe(r.header.info.columns.length)

    expect(actual).toStrictEqual(expected)
  })

  it('distinguishes an absent attributes vector from an empty one', async () => {
    const r = await open(resolve(CORPUS, 'small.fcb'))
    for await (const f of await r.selectAll()) {
      f.cityObjects().forEach((o, i) => {
        if (!o.hasAttributes()) return
        expect(f.attributes(i)).toBeTypeOf('object')
      })
    }
  })
})
