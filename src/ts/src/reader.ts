/** The reader facade -- ports `fcb::FcbReader` and `fcb::FeatureIterator`
 *  (src/cpp/src/reader.cpp), themselves a port of `fcb_core::FcbReader`
 *  (src/rust/fcb_core/src/reader/mod.rs). */
import { ErrorCode, FcbError } from './errors.js'
import { FEATURE_SIZE_PREFIX, readFeature } from './feature/index.js'
import type { Feature } from './feature/index.js'
import { readHeader } from './header/index.js'
import type { HeaderView } from './header/index.js'
import { BlobRangeReader } from './io/blob.js'
import { BytesRangeReader } from './io/range-reader.js'
import type { RangeReader } from './io/range-reader.js'

/** A cursor over features. `featuresCount` is `undefined` when the header
 *  does not know it: the format writes 0 for UNKNOWN, not for empty, so it
 *  must never be reported as a count of zero. */
export interface FeatureCursor extends AsyncIterable<Feature> {
  readonly featuresCount: number | undefined
}

/** Sources that hold an OS resource expose `close`; in-memory ones do not.
 *  Duck-typed rather than made part of the RangeReader interface so a test
 *  double or a future source is not forced to implement a no-op. */
interface Closeable {
  close(): Promise<void> | void
}

function asCloseable(reader: RangeReader): Closeable | undefined {
  const maybe = reader as Partial<Closeable>
  return typeof maybe.close === 'function' ? (maybe as Closeable) : undefined
}

/** Walks the feature section from `featureBegin` to EOF.
 *
 *  A NATIVE async generator, deliberately. The language queues a `next()`
 *  that arrives while a previous one is still pending and resumes the body
 *  only once the earlier call settles, so two overlapping `next()` calls
 *  cannot interleave their updates to `at`. That removes the need for a
 *  hand-rolled in-flight flag and for any reentrancy error path: there is
 *  nothing left for one to detect.
 *
 *  The scan runs to EOF rather than to `featuresCount`, because 0 means
 *  UNKNOWN (conformance/no_count.fcb declares 0 and holds three features).
 *  A declared non-zero count is still used, but only as a lower bound to
 *  catch a truncated file -- reaching EOF early is a cut-off file, not a
 *  clean end of iteration (reader.cpp:168-179). */
async function* scan(
  reader: RangeReader,
  header: HeaderView,
): AsyncGenerator<Feature, void, undefined> {
  const total = reader.size()
  const declared = header.info.featuresCount
  const columns = header.info.columns
  let at = header.layout.featureBegin
  let produced = 0

  while (at + FEATURE_SIZE_PREFIX <= total) {
    const { feature, next } = await readFeature(reader, at, columns, header.layout.featureBegin)
    at = next
    produced++
    yield feature
  }

  if (declared !== 0 && produced < declared) {
    throw new FcbError(
      ErrorCode.IoError,
      `truncated feature section: header declares ${declared} features, found ${produced}`,
    )
  }
}

export class FcbReader {
  private readonly reader: RangeReader
  private readonly headerView: HeaderView
  private closed = false

  private constructor(reader: RangeReader, headerView: HeaderView) {
    this.reader = reader
    this.headerView = headerView
  }

  /** The primitive every other constructor and every later feature builds on
   *  (Task 11's `fromUrl`, Task 12's `select`, the request-log tests).
   *
   *  The reader is used EXACTLY as given -- no buffering decorator is
   *  inserted here. Caching is a property of the source, not of the facade:
   *  wrapping unconditionally would hide how many requests a scan really
   *  makes, which is precisely what the HTTP work needs to be able to see and
   *  tune. A caller over a chatty transport composes
   *  `new BufferedRangeReader(source)` itself. */
  static async fromReader(reader: RangeReader): Promise<FcbReader> {
    return new FcbReader(reader, await readHeader(reader))
  }

  static async fromBytes(bytes: Uint8Array): Promise<FcbReader> {
    return FcbReader.fromReader(new BytesRangeReader(bytes))
  }

  static async fromBlob(blob: Blob): Promise<FcbReader> {
    return FcbReader.fromReader(new BlobRangeReader(blob))
  }

  get header(): HeaderView {
    return this.headerView
  }

  /** Every feature in the file, in stored order. Async because later
   *  selection modes (spatial, attribute) must read an index before they can
   *  produce their first feature; `selectAll` has nothing to read, so it
   *  resolves immediately, but the signature is shared. */
  async selectAll(): Promise<FeatureCursor> {
    if (this.closed) {
      throw new FcbError(ErrorCode.IoError, 'selectAll on a closed FcbReader')
    }
    const gen = scan(this.reader, this.headerView)
    const declared = this.headerView.info.featuresCount
    return {
      featuresCount: declared === 0 ? undefined : declared,
      [Symbol.asyncIterator]: () => gen,
    }
  }

  /** Releases the underlying reader if it holds an OS resource -- `fromFile`
   *  opens a `node:fs` handle that has to stay open for later queries and so
   *  cannot be closed inside `fromFile`. Idempotent, and a no-op that
   *  resolves immediately for in-memory sources. */
  async close(): Promise<void> {
    if (this.closed) return
    this.closed = true
    await asCloseable(this.reader)?.close()
  }

  /** Lets callers write `await using r = await fromFile(path)`. */
  async [Symbol.asyncDispose](): Promise<void> {
    await this.close()
  }
}
