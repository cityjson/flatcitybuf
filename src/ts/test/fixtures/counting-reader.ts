import type { RangeReader, ReadOpts } from '../../src/io/range-reader.js'

/** Records every underlying read so tests can assert the REQUEST PATTERN,
 *  not just the bytes. Without these assertions a reader can be correct and
 *  50x chattier than the reference, and nothing notices until it is on a CDN. */
export class CountingReader implements RangeReader {
  readonly reads: Array<{ offset: number; length: number }> = []

  constructor(private readonly data: Uint8Array) {}

  async read(offset: number, length: number, _opts?: ReadOpts): Promise<Uint8Array> {
    this.reads.push({ offset, length })
    return this.data.subarray(offset, offset + length)
  }

  size(): number {
    return this.data.length
  }
}
