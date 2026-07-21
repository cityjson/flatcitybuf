import { describe, expect, it } from 'vitest'
import { FcbError } from '../src/errors.js'
import { BufferedRangeReader, BytesRangeReader } from '../src/io/range-reader.js'
import { CountingReader } from './fixtures/counting-reader.js'

const ramp = (n: number) => Uint8Array.from({ length: n }, (_, i) => i & 0xff)

describe('BytesRangeReader', () => {
  it('serves exact ranges and reports its size', async () => {
    const r = new BytesRangeReader(ramp(256))
    expect(r.size()).toBe(256)
    expect(Array.from(await r.read(4, 3))).toEqual([4, 5, 6])
  })

  it('copies its input, so later mutation cannot corrupt it', async () => {
    const src = ramp(16)
    const r = new BytesRangeReader(src)
    src.fill(0xff)
    expect(Array.from(await r.read(0, 2))).toEqual([0, 1])
  })

  it('rejects a read past the end rather than returning a short buffer', async () => {
    const r = new BytesRangeReader(ramp(16))
    await expect(r.read(12, 8)).rejects.toThrow(FcbError)
  })

  it('rejects non-integer and negative arguments', async () => {
    const r = new BytesRangeReader(ramp(16))
    await expect(r.read(-1, 4)).rejects.toThrow(FcbError)
    await expect(r.read(0.5, 4)).rejects.toThrow(FcbError)
    await expect(r.read(0, -4)).rejects.toThrow(FcbError)
  })
})

describe('BufferedRangeReader', () => {
  it('serves sequential reads from one underlying fetch', async () => {
    const inner = new CountingReader(ramp(2048))
    const r = new BufferedRangeReader(inner, 512)
    expect(Array.from(await r.read(0, 4))).toEqual([0, 1, 2, 3])
    expect(Array.from(await r.read(4, 4))).toEqual([4, 5, 6, 7])
    expect(inner.reads).toHaveLength(1)
    expect(inner.reads[0]).toEqual({ offset: 0, length: 512 })
  })

  it('refetches when the request leaves the buffered window', async () => {
    const inner = new CountingReader(ramp(2048))
    const r = new BufferedRangeReader(inner, 512)
    await r.read(0, 4)
    await r.read(1024, 4)
    expect(inner.reads).toHaveLength(2)
  })

  it('never over-fetches past the end of the file', async () => {
    const inner = new CountingReader(ramp(100))
    const r = new BufferedRangeReader(inner, 512)
    await r.read(90, 10)
    expect(inner.reads[0]!.offset + inner.reads[0]!.length).toBeLessThanOrEqual(100)
  })

  it('satisfies a request larger than minRequestSize in one read', async () => {
    const inner = new CountingReader(ramp(2048))
    const r = new BufferedRangeReader(inner, 16)
    await r.read(0, 1000)
    expect(inner.reads).toHaveLength(1)
    expect(inner.reads[0]!.length).toBeGreaterThanOrEqual(1000)
  })
})
