/** The single I/O abstraction the whole reader sits on. Every source --
 *  `fetch`, `Blob`, the Node filesystem, or a test double -- implements this
 *  and nothing downstream needs to know which.
 *
 *  Mirrors fcb::RangeReader (src/cpp/include/fcb/range_reader.hpp) with two
 *  deliberate differences: `read` is async here (there is no way to make an
 *  HTTP fetch synchronous in a browser), and `read` THROWS on a range that
 *  crosses the end of the resource rather than silently clamping -- a short
 *  read must never look like success, because every downstream offset would
 *  then be wrong.
 *
 *  CONTRACT -- implementors must honour all of it:
 *   * size() is synchronous: every source learns its size when it opens (a
 *     stat, blob.size, or a Content-Range at open time), so layout
 *     arithmetic never has to await.
 *   * read(offset, length) resolves to EXACTLY `length` bytes, or rejects
 *     with FcbError. It never returns a short buffer.
 *   * offset and length must be non-negative integers; violations reject
 *     with ErrorCode.InvalidArgument before any I/O is attempted.
 *   * read(offset, 0) resolves to an empty Uint8Array without touching the
 *     underlying transport, even if offset is at or past size(). This lets
 *     callers compute a zero-width range (e.g. an empty attribute index)
 *     without special-casing it against size().
 */
import { ErrorCode, FcbError } from '../errors.js'

export interface ReadOpts {
  /** Best-effort cancellation. An already-aborted signal is honoured before
   *  any work happens; an in-memory source cannot usefully abort mid-read
   *  because there is no "mid" -- the read is synchronous under the hood. */
  signal?: AbortSignal
}

export interface RangeReader {
  read(offset: number, length: number, opts?: ReadOpts): Promise<Uint8Array>
  size(): number
}

/** Shared by every RangeReader implementation so the argument contract is
 *  enforced once instead of re-implemented per backend (Task 7's Blob and
 *  Node filesystem sources, Task 11's HTTP source). Checks shape only -- it
 *  does not need `size()`, so it runs before any I/O regardless of how
 *  expensive a given source's size lookup is. */
export function validateArgs(offset: number, length: number): void {
  if (!Number.isInteger(offset) || offset < 0) {
    throw new FcbError(ErrorCode.InvalidArgument, `read offset must be a non-negative integer, got ${offset}`)
  }
  if (!Number.isInteger(length) || length < 0) {
    throw new FcbError(ErrorCode.InvalidArgument, `read length must be a non-negative integer, got ${length}`)
  }
}

/** Bounds check against a synchronously-known size. Failing this is still
 *  "a caller argument failed validation before any I/O" (ErrorCode.
 *  InvalidArgument): size() never awaits, so the caller could have checked
 *  this itself before calling read(). */
export function validateBounds(offset: number, length: number, size: number): void {
  if (offset + length > size) {
    throw new FcbError(
      ErrorCode.InvalidArgument,
      `read [${offset}, ${offset + length}) exceeds resource size ${size}`,
    )
  }
}

export function checkAborted(opts: ReadOpts | undefined): void {
  if (opts?.signal?.aborted) {
    throw new FcbError(ErrorCode.IoError, 'read aborted before it started')
  }
}

/** In-memory RangeReader. COPIES its input on construction: later mutation
 *  of the caller's array, or an ArrayBuffer transfer/detach (e.g. handing
 *  the original off to a worker), must not be able to corrupt an already-
 *  open reader. */
export class BytesRangeReader implements RangeReader {
  private readonly bytes: Uint8Array

  constructor(bytes: Uint8Array) {
    this.bytes = bytes.slice()
  }

  size(): number {
    return this.bytes.length
  }

  async read(offset: number, length: number, opts?: ReadOpts): Promise<Uint8Array> {
    validateArgs(offset, length)
    checkAborted(opts)
    if (length === 0) return new Uint8Array(0)
    validateBounds(offset, length, this.bytes.length)
    return this.bytes.subarray(offset, offset + length)
  }
}

const DEFAULT_MIN_REQUEST_SIZE = 1048576

/** Caching decorator: over-fetches to `minRequestSize` and serves subsequent
 *  reads inside the cached window without touching the inner reader. This is
 *  what makes traversal over a chatty transport (HTTP) cheap while leaving a
 *  local source's behaviour effectively unchanged.
 *
 *  Buffering policy (exact, so request patterns are predictable from it):
 *  the reader holds one window `[bufOffset, bufOffset + buf.length)`. A
 *  request is a HIT iff `offset >= bufOffset` and
 *  `offset + length <= bufOffset + buf.length`; anything else is a MISS.
 *  On a MISS it issues exactly one inner read of
 *  `min(max(length, minRequestSize), size() - offset)` bytes starting at
 *  `offset` -- i.e. over-fetch to minRequestSize but never past size() --
 *  and that read replaces the window. A HIT never touches the inner reader.
 *  `setMinRequestSize` only changes the over-fetch size used by future
 *  misses; it does not invalidate or resize the current window.
 *
 *  The buffer returned by `read` is a `subarray` of the decorator's own
 *  buffer, not a copy: callers that need bytes to survive past the next
 *  `read` call must copy them out (Task 8 does this for features). */
export class BufferedRangeReader implements RangeReader {
  private readonly inner: RangeReader
  private minRequestSize: number
  private bufOffset = 0
  private buf: Uint8Array = new Uint8Array(0)

  constructor(inner: RangeReader, minRequestSize: number = DEFAULT_MIN_REQUEST_SIZE) {
    this.inner = inner
    this.minRequestSize = minRequestSize
  }

  setMinRequestSize(bytes: number): void {
    this.minRequestSize = bytes
  }

  size(): number {
    return this.inner.size()
  }

  private covers(offset: number, length: number): boolean {
    if (this.buf.length === 0 || offset < this.bufOffset) return false
    return offset + length <= this.bufOffset + this.buf.length
  }

  async read(offset: number, length: number, opts?: ReadOpts): Promise<Uint8Array> {
    validateArgs(offset, length)
    checkAborted(opts)
    if (length === 0) return new Uint8Array(0)
    const total = this.size()
    validateBounds(offset, length, total)

    if (!this.covers(offset, length)) {
      const want = Math.max(length, this.minRequestSize)
      const fetchLength = Math.min(want, total - offset)
      this.buf = await this.inner.read(offset, fetchLength, opts)
      this.bufOffset = offset
    }

    const rel = offset - this.bufOffset
    return this.buf.subarray(rel, rel + length)
  }
}
