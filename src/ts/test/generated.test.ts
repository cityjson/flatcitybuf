import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import * as flatbuffers from 'flatbuffers'
import { describe, expect, it } from 'vitest'
import { Header } from '../src/generated/header.js'
import { CityFeature } from '../src/generated/city-feature.js'

// `__dirname` does not exist under ESM (this package is "type": "module").
// `import.meta.dirname` is the modern replacement, stable since Node 20.11 /
// 21.2, well within this package's engines floor of Node >=22.12. It works
// identically under `vitest run` and `tsc --noEmit -p tsconfig.test.json`.
const CORPUS = resolve(import.meta.dirname, '../../../conformance')

describe('generated bindings', () => {
  it('actually exports the Header CLASS, not a re-export of itself', () => {
    // With default flatc flags this is a circular re-export and Header is
    // undefined at runtime while still type-checking. See gen_ts_fbs.sh.
    expect(typeof Header).toBe('function')
    expect(typeof Header.getRootAsHeader).toBe('function')
    expect(typeof CityFeature).toBe('function')
  })

  it('reads a real header as a size-prefixed root', () => {
    // Pins HOW this runtime exposes size-prefixed roots: confirmed empirically
    // that `Header.getSizePrefixedRootAsHeader` is the correct static accessor
    // for flatbuffers@25.9.23 (it strips the 4-byte size prefix internally).
    const raw = readFileSync(resolve(CORPUS, 'small.fcb'))
    const headerSize = raw.readUInt32LE(8)
    // The prefix is INCLUDED in the slice, per the Format Reference: bytes
    // [8, 12) are the little-endian u32 size prefix, and the slice passed to
    // the accessor starts at that prefix (offset 8), not after it (offset 12).
    const slice = raw.subarray(8, 12 + headerSize)
    const bb = new flatbuffers.ByteBuffer(new Uint8Array(slice))
    const header = Header.getSizePrefixedRootAsHeader(bb)
    expect(header.version()).toBe('2.0')
    expect(header.featuresCount()).toBeGreaterThan(0n)
  })
})
