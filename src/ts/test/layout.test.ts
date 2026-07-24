import { describe, expect, it } from 'vitest'
import { FcbError } from '../src/errors.js'
import { readF64, readU32, readU64, toSafeNumber } from '../src/le.js'
import {
  checkMagicBytes, computeLayout, rtreeIndexSize, validateLayoutAgainstSize,
} from '../src/layout.js'

describe('le', () => {
  it('reads little-endian, which is the OPPOSITE of DataView defaults', () => {
    const dv = new DataView(new Uint8Array([1, 2, 3, 4]).buffer)
    expect(readU32(dv, 0)).toBe(0x04030201)
    expect(dv.getUint32(0)).toBe(0x01020304) // the trap, pinned
  })

  it('reads u64 as bigint and converts only within safe range', () => {
    const buf = new Uint8Array(8)
    new DataView(buf.buffer).setBigUint64(0, 2n ** 53n, true)
    const dv = new DataView(buf.buffer)
    expect(readU64(dv, 0)).toBe(2n ** 53n)
    expect(() => toSafeNumber(2n ** 60n, 'offset')).toThrow(FcbError)
    expect(toSafeNumber(12345n, 'offset')).toBe(12345)
  })

  it('reads f64 little-endian', () => {
    const buf = new Uint8Array(8)
    new DataView(buf.buffer).setFloat64(0, -1.5, true)
    expect(readF64(new DataView(buf.buffer), 0)).toBe(-1.5)
  })
})

describe('magic bytes', () => {
  it('ignores byte seven, which is written but never validated', () => {
    // lib.rs:56-58 validates b[0..3], b[4..7] and b[3] <= 1 only.
    expect(checkMagicBytes(new TextEncoder().encode('fcb\x01fcb\x00'))).toBe(true)
    expect(checkMagicBytes(new TextEncoder().encode('fcb\x01fcb\xff'))).toBe(true)
    expect(checkMagicBytes(new TextEncoder().encode('xcb\x01fcb\x00'))).toBe(false)
  })

  it('rejects a future version', () => {
    expect(checkMagicBytes(new TextEncoder().encode('fcb\x02fcb\x00'))).toBe(false)
  })

  it('rejects a buffer shorter than the magic', () => {
    expect(checkMagicBytes(new Uint8Array(4))).toBe(false)
  })
})

describe('rtreeIndexSize', () => {
  it('counts a root node even for a single item', () => {
    // The loop DIVIDES FIRST and only then tests n === 1, so a one-feature
    // file stores a leaf AND a root: 2 nodes, 80 bytes. Asserting 40 here
    // misplaces every section of single_feature.fcb.
    // (packed_rtree/mod.rs:888, src/cpp/src/layout.cpp:36-44)
    expect(rtreeIndexSize(1, 16)).toBe(80)
    expect(rtreeIndexSize(16, 16)).toBe((16 + 1) * 40)
    expect(rtreeIndexSize(17, 16)).toBe((17 + 2 + 1) * 40)
  })

  it('REJECTS a node size below 2 rather than clamping it', () => {
    // layout.cpp:25-29: "reject rather than clamp, so we never invent a
    // layout." A clamping reader silently reads a corrupt file as if it
    // were well formed. 0 means "no index" only at computeLayout.
    expect(() => rtreeIndexSize(4, 0)).toThrow(FcbError)
    expect(() => rtreeIndexSize(4, 1)).toThrow(FcbError)
  })

  it('rejects a zero item count, which would never terminate', () => {
    expect(() => rtreeIndexSize(0, 16)).toThrow(FcbError)
  })
})

describe('computeLayout', () => {
  it('places sections back to back with no padding', () => {
    const l = computeLayout({
      headerSize: 64, featuresCount: 1, indexNodeSize: 16, attrIndexSize: 0,
    })
    expect(l.headerLen).toBe(8 + 4 + 64)
    expect(l.rtreeBegin).toBe(l.headerLen)
    expect(l.rtreeSize).toBe(80)          // leaf + root, see above
    expect(l.attrIndexBegin).toBe(l.headerLen + 80)
    expect(l.featureBegin).toBe(l.headerLen + 80)
  })

  it('places the feature section after the attribute index', () => {
    const l = computeLayout({
      headerSize: 64, featuresCount: 1, indexNodeSize: 16, attrIndexSize: 256,
    })
    expect(l.attrIndexBegin).toBe(l.headerLen + 80)
    expect(l.featureBegin).toBe(l.headerLen + 80 + 256)
  })

  it('has no rtree when the node size or the feature count is zero', () => {
    expect(computeLayout({
      headerSize: 64, featuresCount: 0, indexNodeSize: 16, attrIndexSize: 0,
    }).rtreeSize).toBe(0)
    expect(computeLayout({
      headerSize: 64, featuresCount: 5, indexNodeSize: 0, attrIndexSize: 0,
    }).rtreeSize).toBe(0)
  })

  it('rejects a header larger than the file', () => {
    const l = computeLayout({
      headerSize: 64, featuresCount: 1, indexNodeSize: 16, attrIndexSize: 0,
    })
    expect(() => validateLayoutAgainstSize(l, 10)).toThrow(FcbError)
  })

  it('rejects a header size outside the legal window', () => {
    expect(() => computeLayout({
      headerSize: 4, featuresCount: 1, indexNodeSize: 16, attrIndexSize: 0,
    })).toThrow(FcbError)
    expect(() => computeLayout({
      headerSize: 536870913, featuresCount: 1, indexNodeSize: 16, attrIndexSize: 0,
    })).toThrow(FcbError)
  })
})
