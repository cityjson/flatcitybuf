import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'
import { FcbError } from '../src/errors.js'
import { BlobRangeReader } from '../src/io/blob.js'
import { FileRangeReader } from '../src/io/node.js'

// `__dirname` does not exist under ESM (this package is "type": "module").
const CORPUS = resolve(import.meta.dirname, '../../../conformance')
const PATH = resolve(CORPUS, 'small.fcb')
const BYTES = new Uint8Array(readFileSync(PATH))

describe('BlobRangeReader', () => {
  it('reports size synchronously and serves exact ranges', async () => {
    const r = new BlobRangeReader(new Blob([BYTES]))
    expect(r.size()).toBe(BYTES.length)
    expect(Array.from(await r.read(8, 4))).toEqual(Array.from(BYTES.subarray(8, 12)))
  })

  it('rejects a read past the end', async () => {
    const r = new BlobRangeReader(new Blob([BYTES]))
    await expect(r.read(BYTES.length - 2, 8)).rejects.toThrow(FcbError)
  })
})

describe('FileRangeReader', () => {
  it('serves the same bytes as reading the whole file', async () => {
    const r = await FileRangeReader.open(PATH)
    try {
      expect(r.size()).toBe(BYTES.length)
      expect(Array.from(await r.read(8, 4))).toEqual(Array.from(BYTES.subarray(8, 12)))
    } finally {
      await r.close()
    }
  })

  it('reports a missing file as an FcbError, not a raw ENOENT', async () => {
    await expect(FileRangeReader.open(resolve(CORPUS, 'nope.fcb'))).rejects.toThrow(FcbError)
  })
})
