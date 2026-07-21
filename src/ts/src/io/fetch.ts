/** RangeReader over `fetch`, with strict validation of the server's `206`
 *  response -- this is the one correctness question this file exists to get
 *  right. The wasm binding this port replaces accepts a `200` response to a
 *  Range request and reads garbage from every later offset; a server can
 *  also answer `206` with a `Content-Range` that does not match what was
 *  asked, which is otherwise indistinguishable from success. Every physical
 *  fetch this reader issues goes through `fetchRange`, so both checks are
 *  enforced exactly once no matter which call site triggered the request.
 *
 *  Mirrors `fcb::CurlRangeReader` (src/cpp/include/fcb/http/curl_range_reader.hpp)
 *  and Python's `HttpRangeReader` (src/py/flatcitybuf/http_reader.py) in
 *  spirit, but is the RAW adapter of this port's layering: like
 *  `FileRangeReader`, it issues exactly what a caller asks for on a cache
 *  miss and does no adaptive, growing buffering of its own. The one
 *  exception is the one-time prefetch established at `open()` -- see its
 *  docstring, and `FcbReader.fromUrl`'s (src/ts/src/reader.ts) for how a
 *  caller composes a `BufferedRangeReader` around this for everything
 *  after. */
import { ErrorCode, FcbError } from '../errors.js'
import {
  checkAborted, type ReadOpts, type RangeReader, validateArgs, validateBounds,
} from './range-reader.js'

/** Bounds a single physical fetch. A `read()` call longer than this is split
 *  into sequential Range requests rather than ever issuing one unbounded
 *  request -- mirrors `http_reader/mod.rs:42`'s `DEFAULT_HTTP_FETCH_SIZE`,
 *  commented there as "the largest request we'll speculatively make": a
 *  CAP, not a floor. Format Reference, "HTTP constants". */
export const DEFAULT_FETCH_SIZE = 1_048_576

/** `2024 + (1 + 16 + 256) * 40 = 12944` bytes: an assumed 2 KB header plus
 *  the top three R-tree levels at the format's default branching factor of
 *  16. Format Reference, "HTTP constants"; `http_reader/mod.rs:80-98`. One
 *  request at `open()` for this many bytes buys magic + header + those
 *  three levels without a second round trip, for files whose header and
 *  index top are that shallow. */
export const OPEN_PREFETCH_SIZE = 12944

export interface FetchRangeReaderOpts {
  /** Caps a single physical fetch issued on a cache miss past the open
   *  prefetch. Defaults to `DEFAULT_FETCH_SIZE` (1 MB). */
  fetchSize?: number
  /** Aborts every request this reader issues, at any point in its
   *  lifetime -- the one made by `open()` and every later `read()`. Wired
   *  directly into the underlying `fetch()` call so a signal that fires
   *  mid-flight actually cancels the in-progress request; an
   *  already-aborted signal is also honoured before any work starts. */
  signal?: AbortSignal
  /** Injection point for tests (a counting wrapper) and environments
   *  without a global `fetch`. Defaults to `globalThis.fetch`. */
  fetch?: typeof globalThis.fetch
}

// RFC 7233 Content-Range, as sent on a 206: "bytes <start>-<end>/<total>".
const CONTENT_RANGE_RE = /^bytes (\d+)-(\d+)\/(\d+)$/

interface FetchedRange {
  bytes: Uint8Array
  total: number
}

/** Combines every applicable `AbortSignal` into one. Always includes a
 *  fresh, reader-owned controller signal so the request can be cancelled
 *  from inside `fetchRange` itself (the 200-response case), independent of
 *  whatever the caller passed in. `AbortSignal.any` of zero signals is a
 *  signal that never fires, so this degrades cleanly when neither the
 *  reader-level nor the per-call signal is set. */
function combineSignals(own: AbortSignal, ...external: Array<AbortSignal | undefined>): AbortSignal {
  const present = external.filter((s): s is AbortSignal => s !== undefined)
  return present.length === 0 ? own : AbortSignal.any([own, ...present])
}

/** Merges two possibly-absent signals into one, without wrapping when only
 *  one (or neither) is actually present. Used to combine the reader-level
 *  signal from `open()`'s opts with a per-call `read()` signal before
 *  either ever reaches `fetchRange`, which then layers its own
 *  abort-on-200 controller on top of the result. */
function mergeSignals(a: AbortSignal | undefined, b: AbortSignal | undefined): AbortSignal | undefined {
  if (a === undefined) return b
  if (b === undefined) return a
  return AbortSignal.any([a, b])
}

/** One physical Range GET for the half-open byte range
 *  `[offset, offset + length)`, with the full validation this file exists
 *  for. Every call site -- `open()`'s prefetch and `read()`'s cache-miss
 *  chunks -- routes through here so the checks below run exactly once. */
async function fetchRange(
  url: string,
  fetchImpl: typeof globalThis.fetch,
  offset: number,
  length: number,
  externalSignal: AbortSignal | undefined,
): Promise<FetchedRange> {
  const last = offset + length - 1
  const controller = new AbortController()
  const signal = combineSignals(controller.signal, externalSignal)

  let response: Response
  try {
    response = await fetchImpl(url, { headers: { Range: `bytes=${offset}-${last}` }, signal })
  } catch (err) {
    if (signal.aborted) {
      throw new FcbError(ErrorCode.IoError, `request to ${url} aborted: ${err}`)
    }
    throw new FcbError(ErrorCode.HttpError, `request to ${url} failed: ${err}`)
  }

  if (response.status === 200) {
    // The server ignored Range and is about to hand back the whole
    // representation, which may be gigabytes. Abort BEFORE ever awaiting
    // the body -- reading it first and rejecting after would still pay
    // for downloading it, exactly the bug this reader exists to avoid.
    controller.abort()
    throw new FcbError(
      ErrorCode.RangeNotSupported,
      `${url} ignored the Range request and answered 200 with the full body; ` +
        'for a file known to fit in memory, read it yourself and use FcbReader.fromBytes instead',
    )
  }

  if (response.status !== 206) {
    throw new FcbError(ErrorCode.HttpError, `unexpected HTTP status ${response.status} for ${url}`)
  }

  const contentRange = response.headers.get('Content-Range')
  if (contentRange === null) {
    // Either genuinely absent, or present on the wire but not exposed to a
    // cross-origin caller by CORS (`Access-Control-Expose-Headers`) -- the
    // two are indistinguishable from here, and both must fail loudly
    // rather than silently guess a size from Content-Length (which, on a
    // 206, is only the slice length, not the resource's).
    throw new FcbError(
      ErrorCode.RangeHeadersNotExposed,
      `${url} sent a 206 response without an accessible Content-Range header ` +
        '(if this is cross-origin, the server must send ' +
        'Access-Control-Expose-Headers: Content-Range)',
    )
  }

  const match = CONTENT_RANGE_RE.exec(contentRange.trim())
  if (!match) {
    throw new FcbError(
      ErrorCode.HttpError,
      `malformed Content-Range on a 206 response from ${url}: ${JSON.stringify(contentRange)}`,
    )
  }
  const start = Number(match[1])
  const end = Number(match[2])
  const total = Number(match[3])

  // The server is untrusted input: a `wrong_offset`-style response answers
  // A range the client did not ask for, which is otherwise indistinguishable
  // from success unless start AND end are both checked against what was
  // requested (clamped to the resource's own end, which `total` -- from
  // this same response -- tells us).
  const expectedEnd = Math.min(last, total - 1)
  if (start !== offset || end !== expectedEnd) {
    throw new FcbError(
      ErrorCode.HttpError,
      `server returned range bytes ${start}-${end}/${total} for ${url}, ` +
        `expected ${offset}-${expectedEnd}/${total}`,
    )
  }

  let buf: ArrayBuffer
  try {
    buf = await response.arrayBuffer()
  } catch (err) {
    throw new FcbError(ErrorCode.HttpError, `failed to read response body from ${url}: ${err}`)
  }
  const bytes = new Uint8Array(buf)
  const want = end - start + 1
  if (bytes.length !== want) {
    throw new FcbError(
      ErrorCode.HttpError,
      `truncated response body for ${url} bytes=${offset}-${last}: got ${bytes.length} bytes, expected ${want}`,
    )
  }
  return { bytes, total }
}

export class FetchRangeReader implements RangeReader {
  private readonly url: string
  private readonly fetchImpl: typeof globalThis.fetch
  private readonly readerSignal: AbortSignal | undefined
  private readonly fetchSize: number
  private readonly totalSize: number
  /** The window fetched once at `open()`: always `[0, prefetch.length)`.
   *  A `read()` fully inside it is served without touching the network
   *  again -- this is what keeps `FcbReader.fromUrl`'s open (which issues
   *  three separate `read()` calls while parsing the header: magic, size
   *  prefix, header body -- see `header/index.ts`'s `readHeader`) down to
   *  exactly the one physical request made here. */
  private readonly prefetch: Uint8Array

  private constructor(
    url: string,
    fetchImpl: typeof globalThis.fetch,
    readerSignal: AbortSignal | undefined,
    fetchSize: number,
    totalSize: number,
    prefetch: Uint8Array,
  ) {
    this.url = url
    this.fetchImpl = fetchImpl
    this.readerSignal = readerSignal
    this.fetchSize = fetchSize
    this.totalSize = totalSize
    this.prefetch = prefetch
  }

  /** Opens `url` and learns its size from the `Content-Range` of a single
   *  `OPEN_PREFETCH_SIZE`-byte Range request -- there is no separate HEAD
   *  probe, so this one request both establishes `size()` (synchronous for
   *  the rest of this reader's life, per the `RangeReader` contract) and
   *  warms the header/index prefetch window described on `prefetch` above.
   *  Every one of this task's four failure modes (200, malformed
   *  Content-Range, wrong range, unexposed Content-Range) is caught right
   *  here, before a caller ever sees a reader that might be lying about
   *  its size. */
  static async open(url: string, opts?: FetchRangeReaderOpts): Promise<FetchRangeReader> {
    const fetchImpl = opts?.fetch ?? globalThis.fetch
    const fetchSize = opts?.fetchSize ?? DEFAULT_FETCH_SIZE
    if (!Number.isInteger(fetchSize) || fetchSize <= 0) {
      throw new FcbError(ErrorCode.InvalidArgument, `fetchSize must be a positive integer, got ${fetchSize}`)
    }
    // `FetchRangeReaderOpts` is structurally assignable to `ReadOpts` (both
    // carry an optional `signal`), so the same already-aborted pre-flight
    // check `read()` uses below is reused here rather than re-implemented --
    // one fewer copy of a check whose only job is to fail fast before any
    // I/O starts.
    checkAborted(opts)
    const { bytes, total } = await fetchRange(url, fetchImpl, 0, OPEN_PREFETCH_SIZE, opts?.signal)
    return new FetchRangeReader(url, fetchImpl, opts?.signal, fetchSize, total, bytes)
  }

  size(): number {
    return this.totalSize
  }

  async read(offset: number, length: number, opts?: ReadOpts): Promise<Uint8Array> {
    validateArgs(offset, length)
    checkAborted(opts)
    if (this.readerSignal?.aborted) {
      throw new FcbError(ErrorCode.IoError, 'read aborted: reader-level signal already fired')
    }
    if (length === 0) return new Uint8Array(0)
    validateBounds(offset, length, this.totalSize)

    if (offset + length <= this.prefetch.length) {
      return this.prefetch.subarray(offset, offset + length)
    }

    // `fetchSize` bounds any single physical request this reader makes, so
    // a `length` beyond the open prefetch is split into sequential chunks
    // rather than ever issuing one unbounded Range GET.
    const signal = mergeSignals(this.readerSignal, opts?.signal)
    const out = new Uint8Array(length)
    let at = 0
    let pos = offset
    let remaining = length
    while (remaining > 0) {
      const chunkLen = Math.min(remaining, this.fetchSize)
      const { bytes, total } = await fetchRange(this.url, this.fetchImpl, pos, chunkLen, signal)
      if (total !== this.totalSize) {
        throw new FcbError(
          ErrorCode.HttpError,
          `resource size changed between requests to ${this.url} (was ${this.totalSize}, now ${total})`,
        )
      }
      out.set(bytes, at)
      at += bytes.length
      pos += chunkLen
      remaining -= chunkLen
    }
    return out
  }
}
