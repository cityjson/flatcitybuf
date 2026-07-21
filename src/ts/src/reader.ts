/** The reader facade -- ports `fcb::FcbReader` and `fcb::FeatureIterator`
 *  (src/cpp/src/reader.cpp), themselves a port of `fcb_core::FcbReader`
 *  (src/rust/fcb_core/src/reader/mod.rs). */
import { toCityJSONFeature, toCityJSONMetadata } from './cityjson/index.js'
import type { Int64Policy } from './cityjson/index.js'
import type { CityJSON, CityJSONFeature } from './cityjson/types.js'
import { ErrorCode, FcbError } from './errors.js'
import { FEATURE_SIZE_PREFIX, readFeature } from './feature/index.js'
import type { Feature } from './feature/index.js'
import { readHeader } from './header/index.js'
import type { HeaderView } from './header/index.js'
import { BlobRangeReader } from './io/blob.js'
import { DEFAULT_FETCH_SIZE, FetchRangeReader, OPEN_PREFETCH_SIZE } from './io/fetch.js'
import type { FetchRangeReaderOpts } from './io/fetch.js'
import { BufferedRangeReader, BytesRangeReader } from './io/range-reader.js'
import type { RangeReader, ReadOpts } from './io/range-reader.js'
import { queryToBBox, searchRtree } from './packed-rtree/index.js'
import type { SearchResultItem, SpatialQuery } from './packed-rtree/index.js'
import { intersectHits, searchAttributes } from './static-btree/index.js'

/** A cursor over features. `featuresCount` is `undefined` when the header
 *  does not know it: the format writes 0 for UNKNOWN, not for empty, so it
 *  must never be reported as a count of zero. */
export interface FeatureCursor extends AsyncIterable<Feature> {
  readonly featuresCount: number | undefined
}

/** Comparison operators for an attribute condition.
 *
 *  Declared HERE, in the R-tree task, even though nothing consumes them until
 *  the attribute-index task: `SelectOptions.where` has to reference them, so
 *  the alternative is a `SelectOptions` whose shape changes later. The
 *  attribute task imports these rather than redeclaring them. */
export type Operator = 'Eq' | 'Ne' | 'Gt' | 'Ge' | 'Lt' | 'Le'

/** One attribute predicate. `value` is `unknown` because the admissible type
 *  depends on the column's declared type, which is only known once the header
 *  has been read. */
export interface AttrCondition {
  field: string
  operator: Operator
  value: unknown
}

/** What to select. Every field is optional; `select()` with no options is
 *  `selectAll()`.
 *
 *  `limit`/`offset` apply AFTER the search, over the sorted result list --
 *  they page the answer, they do not change it. `featuresCount` on the
 *  returned cursor therefore reports the TOTAL number of matches (or the
 *  file's total, for an unfiltered select), regardless of paging. */
export interface SelectOptions {
  spatial?: SpatialQuery
  where?: AttrCondition[]
  limit?: number
  offset?: number
  signal?: AbortSignal
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
 *  clean end of iteration (reader.cpp:168-179).
 *
 *  `opts` is forwarded to every `readFeature` call, so an `AbortSignal`
 *  reaches the actual in-flight reads: the next iteration's `readFeature`
 *  rejects via `checkAborted` as soon as the signal fires, rather than the
 *  scan running to completion regardless. */
async function* scan(
  reader: RangeReader,
  header: HeaderView,
  opts?: ReadOpts,
): AsyncGenerator<Feature, void, undefined> {
  const total = reader.size()
  const declared = header.info.featuresCount
  const columns = header.info.columns
  let at = header.layout.featureBegin
  let produced = 0

  while (at + FEATURE_SIZE_PREFIX <= total) {
    const { feature, next } = await readFeature(
      reader,
      at,
      columns,
      header.layout.featureBegin,
      opts,
    )
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

/** Reads exactly the features the index pointed at, in the order the search
 *  returned them (ascending offset, so the feature section is read forwards).
 *
 *  `hit.offset` is relative to `featureBegin` -- the leaf meaning of a
 *  NodeItem's `offset` field. The signal is re-checked between features:
 *  cancelling a cursor that has already produced its first feature has to
 *  stop the reads that have not happened yet. */
async function* readHits(
  reader: RangeReader,
  header: HeaderView,
  hits: readonly SearchResultItem[],
  opts: ReadOpts | undefined,
): AsyncGenerator<Feature, void, undefined> {
  const featureBegin = header.layout.featureBegin
  for (const hit of hits) {
    if (opts?.signal?.aborted) {
      throw new FcbError(ErrorCode.IoError, 'iteration aborted')
    }
    const { feature } = await readFeature(
      reader,
      featureBegin + hit.offset,
      header.info.columns,
      featureBegin,
      opts,
    )
    yield feature
  }
}

/** Skips `offset` items then yields at most `limit`. Used only on the
 *  unfiltered scan; a spatial result set is a materialised array and is paged
 *  by slicing it, so the skipped features are never read at all. */
async function* paginate<T>(
  src: AsyncIterable<T>,
  offset: number,
  limit: number | undefined,
): AsyncGenerator<T, void, undefined> {
  if (limit === 0) return
  let seen = 0
  let produced = 0
  for await (const value of src) {
    if (seen++ < offset) continue
    yield value
    produced++
    if (limit !== undefined && produced >= limit) return
  }
}

/** `limit`/`offset` must be non-negative integers. Checked before anything
 *  else so a bad argument costs no I/O. */
function validatePageArg(value: number | undefined, what: string): number | undefined {
  if (value === undefined) return undefined
  if (!Number.isInteger(value) || value < 0) {
    throw new FcbError(
      ErrorCode.InvalidArgument,
      `invalid ${what}: ${value} (must be a non-negative integer)`,
    )
  }
  return value
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
   *  `new BufferedRangeReader(source)` itself.
   *
   *  A sequential scan through the resulting cursor issues TWO `read` calls
   *  per feature, not one: `readFeature` (src/feature/index.ts) reads the
   *  4-byte size prefix, then re-reads those same 4 bytes as part of a second,
   *  `4 + len`-byte read for the body. See `readFeature`'s docstring for why.
   *  This reader does nothing to hide that -- a request-count assertion
   *  against `reader.reads.length` should expect `2 * featuresCount` (plus
   *  the header's own reads), not `featuresCount`. `fromFile` inherits this
   *  as-is: `FileRangeReader` has no internal buffering, so a sequential scan
   *  costs two `pread` syscalls per feature. Callers for whom request count
   *  matters -- HTTP chief among them -- should wrap their source in
   *  `new BufferedRangeReader(source)` before calling `fromReader`, exactly as
   *  the paragraph above already says for caching in general. */
  static async fromReader(reader: RangeReader): Promise<FcbReader> {
    return new FcbReader(reader, await readHeader(reader))
  }

  static async fromBytes(bytes: Uint8Array): Promise<FcbReader> {
    return FcbReader.fromReader(new BytesRangeReader(bytes))
  }

  static async fromBlob(blob: Blob): Promise<FcbReader> {
    return FcbReader.fromReader(new BlobRangeReader(blob))
  }

  /** Opens a remote `.fcb` file over `fetch`, with strict validation of the
   *  server's Range support (see `io/fetch.ts`).
   *
   *  Unlike `fromReader`, this DOES wrap the source in a
   *  `BufferedRangeReader` -- `fromReader`'s docstring explains why it
   *  itself does not (matching the C++ reference, and leaving request
   *  counting visible to callers who need it), and says a chatty transport
   *  should compose one. HTTP is exactly that transport: without buffering,
   *  the two `read()` calls `readFeature` makes per feature (a 4-byte size
   *  prefix, then a `4 + len` body read that re-reads those same 4 bytes)
   *  would each become a separate HTTP request.
   *
   *  The buffer starts at `OPEN_PREFETCH_SIZE` so its first miss -- forced
   *  by `readHeader`'s very first `read()` call -- asks the underlying
   *  `FetchRangeReader` for EXACTLY the window that reader already cached
   *  during `open()` (`io/fetch.ts`'s `prefetch`), which is why opening
   *  costs one physical request rather than two. Once the header is parsed,
   *  the window is widened to `fetchSize` (`DEFAULT_FETCH_SIZE`, 1 MB,
   *  unless overridden) for the feature scan that follows -- mirrors
   *  `http_reader/mod.rs`'s own reset of `min_req_size` after `_open`. */
  static async fromUrl(url: string, opts?: FetchRangeReaderOpts): Promise<FcbReader> {
    const source = await FetchRangeReader.open(url, opts)
    const buffered = new BufferedRangeReader(source, OPEN_PREFETCH_SIZE)
    const reader = await FcbReader.fromReader(buffered)
    buffered.setMinRequestSize(opts?.fetchSize ?? DEFAULT_FETCH_SIZE)
    return reader
  }

  get header(): HeaderView {
    return this.headerView
  }

  /** Every feature in the file, in stored order. Async because later
   *  selection modes (spatial, attribute) must read an index before they can
   *  produce their first feature; `selectAll` has nothing to read, so it
   *  resolves immediately, but the signature is shared.
   *
   *  Takes `ReadOpts` directly (rather than a `SelectOptions`-shaped object)
   *  because it has nothing else to validate -- no spatial query, no paging
   *  -- so the only thing worth threading is the signal, straight into
   *  `scan`'s reads. */
  async selectAll(opts?: ReadOpts): Promise<FeatureCursor> {
    if (this.closed) {
      throw new FcbError(ErrorCode.IoError, 'selectAll on a closed FcbReader')
    }
    const gen = scan(this.reader, this.headerView, opts)
    const declared = this.headerView.info.featuresCount
    return {
      featuresCount: declared === 0 ? undefined : declared,
      [Symbol.asyncIterator]: () => gen,
    }
  }

  /** The general query entry point: a spatial filter, paging, or both.
   *
   *  Order of operations, and it matters:
   *   1. Validate every argument -- `limit`, `offset`, and the query geometry
   *      -- BEFORE touching the reader, so a caller mistake never costs a
   *      request.
   *   2. Run the search. A spatial query descends the packed R-tree with the
   *      header's OWN `index_node_size`; a hardcoded 16 mis-traverses any file
   *      written with another node size. An attribute query descends one
   *      static B+tree per condition. For a `String` column the B+tree
   *      answers with CANDIDATES, because its keys are truncated to 50 bytes
   *      -- Task 15's post-filter is what turns those into answers.
   *   3. Page the sorted result list. `featuresCount` still reports the total
   *      match count, not the page size.
   *
   *  A `where` and a `spatial` given together are AND-intersected on feature
   *  offset: both index searches return their hits sorted ascending and
   *  de-duplicated, so the intersection is a sorted merge.
   *
   *  The `signal` is threaded into the actual reads on BOTH paths, not merely
   *  held here: into the R-tree traversal and each hit's feature read when
   *  `spatial` is given, and into `scan`'s per-feature reads (re-checked
   *  between features) when it is not. A signal that only lived on this
   *  facade would cancel nothing -- the reads are where the in-flight work
   *  is. */
  async select(opts?: SelectOptions): Promise<FeatureCursor> {
    if (this.closed) {
      throw new FcbError(ErrorCode.IoError, 'select on a closed FcbReader')
    }

    const limit = validatePageArg(opts?.limit, 'limit')
    const offset = validatePageArg(opts?.offset, 'offset') ?? 0

    // Validates the geometry and rejects `nearest`, before any I/O.
    if (opts?.spatial !== undefined) queryToBBox(opts.spatial)

    const readOpts: ReadOpts | undefined =
      opts?.signal === undefined ? undefined : { signal: opts.signal }
    const info = this.headerView.info
    const where = opts?.where !== undefined && opts.where.length > 0 ? opts.where : undefined

    if (opts?.spatial !== undefined || where !== undefined) {
      let hits: SearchResultItem[] | undefined
      if (opts?.spatial !== undefined) {
        if (this.headerView.layout.rtreeSize === 0) {
          throw new FcbError(ErrorCode.NoIndex, 'file has no spatial index')
        }
        hits = await searchRtree(
          this.reader,
          this.headerView.layout.rtreeBegin,
          info.featuresCount,
          info.indexNodeSize,
          opts.spatial,
          readOpts,
        )
      }
      if (where !== undefined) {
        // Attribute hits are already sorted ascending by offset and
        // de-duplicated, which is what `intersectHits` (a sorted merge)
        // requires of both sides; `searchRtree` sorts its own the same way.
        const attr = await searchAttributes(this.reader, this.headerView, where, readOpts)
        hits = hits === undefined ? attr : intersectHits(hits, attr)
      }
      const all = hits ?? []
      const page = limit === undefined ? all.slice(offset) : all.slice(offset, offset + limit)
      const gen = readHits(this.reader, this.headerView, page, readOpts)
      return {
        featuresCount: all.length,
        [Symbol.asyncIterator]: () => gen,
      }
    }

    const declared = info.featuresCount
    const gen = paginate(scan(this.reader, this.headerView, readOpts), offset, limit)
    return {
      // 0 means UNKNOWN, never "empty" -- same rule as selectAll.
      featuresCount: declared === 0 ? undefined : declared,
      [Symbol.asyncIterator]: () => gen,
    }
  }

  /** The whole file as a CityJSONSeq stream: the metadata line first, then
   *  one line per feature, in stored order.
   *
   *  An async generator rather than an array, for the same reason `selectAll`
   *  is a cursor: a city model does not have to fit in memory, and a caller
   *  writing `.jsonl` wants to emit each line as it arrives. Callers who do
   *  want it all can `Array.fromAsync`. */
  async *cityjson(opts?: Int64Policy): AsyncGenerator<CityJSON | CityJSONFeature, void, undefined> {
    yield toCityJSONMetadata(this.headerView, opts)
    for await (const feature of await this.selectAll()) {
      yield toCityJSONFeature(feature, this.headerView, opts)
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
