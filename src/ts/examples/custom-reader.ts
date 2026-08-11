/** Plugging in your own byte source by implementing RangeReader.
 *
 *      node examples/custom-reader.ts in.fcb [minX minY maxX maxY]
 *
 *  `RangeReader` is a two-method interface -- no base class to extend.
 *  Implement it and every reader, index and query works unchanged over
 *  whatever transport you have: S3, a database blob, IndexedDB, a test
 *  double. The one below wraps a Uint8Array and counts what the library
 *  asks for, which is how the request counts in the docs were measured.
 *
 *  The contract, in full (src/io/range-reader.ts):
 *
 *  * `read` resolves to EXACTLY `length` bytes at `offset`, or rejects
 *    with an `FcbError`. It never returns a short buffer and never
 *    clamps a range that runs past the end.
 *  * `size()` is SYNCHRONOUS by contract: every source learns its size
 *    when it opens, so the reader's layout arithmetic never awaits.
 */
import { readFileSync } from 'node:fs'
import { BufferedRangeReader, FcbReader, type RangeReader } from '@cityjson/flatcitybuf'

class CountingBytesReader implements RangeReader {
  readonly reads: Array<[number, number]> = []
  // Written out rather than declared as a constructor parameter
  // property: `node file.ts` strips types only, and a parameter
  // property would generate an assignment, so it is rejected.
  private readonly bytes: Uint8Array

  constructor(bytes: Uint8Array) {
    this.bytes = bytes
  }

  read(offset: number, length: number): Promise<Uint8Array> {
    this.reads.push([offset, length])
    // EXACTLY length bytes, or an error -- never a short buffer.
    if (offset + length > this.bytes.length) {
      return Promise.reject(new RangeError(`read past end: ${offset}+${length}`))
    }
    return Promise.resolve(this.bytes.subarray(offset, offset + length))
  }

  size(): number {
    return this.bytes.length
  }

  get bytesRead(): number {
    return this.reads.reduce((n, [, len]) => n + len, 0)
  }
}

const [path, ...rest] = process.argv.slice(2)
if (path === undefined || (rest.length !== 0 && rest.length !== 4)) {
  console.log('usage: node examples/custom-reader.ts <file.fcb> [minX minY maxX maxY]')
  process.exit(2)
}

const bytes = new Uint8Array(readFileSync(path))
const box =
  rest.length === 4 ? (rest.map(Number) as [number, number, number, number]) : undefined

/** Runs the query and reports what the source was actually asked for.
 *  `wrap` decides whether the raw counter is handed to the reader
 *  directly or through a BufferedRangeReader. */
async function run(label: string, wrap: boolean): Promise<void> {
  const source = new CountingBytesReader(bytes)
  const reader = await FcbReader.fromReader(
    wrap ? new BufferedRangeReader(source) : source,
  )
  try {
    const info = reader.header.info
    if (box === undefined) {
      console.log(
        `opened: ${info.featuresCount} features, ${source.size()} bytes, ` +
          `${source.reads.length} read(s)`,
      )
      console.log('pass a bbox to see how little a query actually reads')
      return
    }

    const before: [number, number] = [source.reads.length, source.bytesRead]
    let n = 0
    for await (const feature of await reader.select({ spatial: { kind: 'bbox', value: box } })) {
      if (n < 5 && !wrap) console.log(`  ${feature.id}`)
      n += 1
    }
    if (n > 5 && !wrap) console.log(`  ... ${n - 5} more`)

    const nReads = source.reads.length - before[0]
    const nBytes = source.bytesRead - before[1]
    const pct = ((100 * nBytes) / source.size()).toFixed(1)
    console.log(
      `${label}: ${n} hit(s), ${nReads} read(s), ${nBytes} bytes (${pct}% of the file)`,
    )
  } finally {
    await reader.close()
  }
}

// `fromReader` uses the source EXACTLY as given -- it inserts no
// buffering decorator, deliberately, so a request count stays honest and
// tunable. A sequential read costs TWO reads per feature (a 4-byte size
// prefix, then the body), which is why the raw number below is roughly
// 2x the hit count. Callers over a chatty transport -- HTTP above all --
// wrap the source themselves, exactly as `fromUrl` does internally.
await run('raw     ', false)
await run('buffered', true)
