/** The `@cityjson/flatcitybuf/node` entry point: reading a local `.fcb` file
 *  from Node.
 *
 *  ```ts
 *  import { fromFile } from '@cityjson/flatcitybuf/node'
 *
 *  await using reader = await fromFile('./city.fcb')
 *  for await (const feature of await reader.selectAll()) console.log(feature.id)
 *  ```
 *
 *  This is the ONLY file in the package allowed to import `node:*`: a browser
 *  bundle that resolves `node:fs` is a build failure for every other module, so
 *  it is reachable only through this separate subpath (see package.json
 *  `exports`), never through the package root. Everything else -- `FcbReader`,
 *  the CityJSON emitters, the query types -- comes from the root entry point
 *  and works unchanged in both runtimes.
 *
 *  @module
 */
import { type FileHandle, open as openFile } from 'node:fs/promises'
import { ErrorCode, FcbError } from '../errors.js'
import { FcbReader } from '../reader.js'
import { checkAborted, type RangeReader, type ReadOpts, validateArgs, validateBounds } from './range-reader.js'

/** `RangeReader` over a `node:fs` file handle -- the source used by CLI and
 *  server consumers reading a local file, and what {@link fromFile} builds.
 *
 *  Owns an open file descriptor for its whole lifetime, because later queries
 *  seek back into the header and the indices; call `close()` (or
 *  `FcbReader.close`, or `await using`) to release it.
 *  Node's own `open`/`stat`/`read` errors are never surfaced raw: they are
 *  wrapped as `FcbError` with `code` `IoError`, so a caller of this package
 *  only ever has to catch `FcbError`.
 *
 *  Unbuffered: a sequential scan costs two `pread` syscalls per feature. Wrap
 *  it in a `BufferedRangeReader` if that matters. */
export class FileRangeReader implements RangeReader {
  private readonly handle: FileHandle
  private readonly fileSize: number
  private closed = false

  private constructor(handle: FileHandle, fileSize: number) {
    this.handle = handle
    this.fileSize = fileSize
  }

  /** Opens the file and stats it up front so `size()` can stay synchronous
   *  for the reader's whole lifetime, matching every other RangeReader.
   *  Node's own open/stat errors (ENOENT, EACCES, ...) are never surfaced
   *  raw -- callers of this package should only ever have to catch
   *  `FcbError`. */
  static async open(path: string): Promise<FileRangeReader> {
    let handle: FileHandle
    try {
      handle = await openFile(path, 'r')
    } catch (err) {
      throw new FcbError(ErrorCode.IoError, `failed to open ${path}: ${err}`)
    }
    try {
      const stat = await handle.stat()
      return new FileRangeReader(handle, stat.size)
    } catch (err) {
      await handle.close()
      throw new FcbError(ErrorCode.IoError, `failed to stat ${path}: ${err}`)
    }
  }

  size(): number {
    return this.fileSize
  }

  async close(): Promise<void> {
    if (this.closed) return
    this.closed = true
    await this.handle.close()
  }

  async read(offset: number, length: number, opts?: ReadOpts): Promise<Uint8Array> {
    validateArgs(offset, length)
    checkAborted(opts)
    // A read after close() is refused outright, even for length 0: once the
    // handle is closed the reader is dead, and a silent no-op read would let
    // a use-after-close bug hide behind an empty array instead of failing
    // loudly at the call site that still thinks the reader is live.
    if (this.closed) {
      throw new FcbError(ErrorCode.IoError, 'read on a closed FileRangeReader')
    }
    if (length === 0) return new Uint8Array(0)
    validateBounds(offset, length, this.fileSize)

    // filehandle.read() is permitted to return fewer bytes than requested
    // without erroring (e.g. interrupted by a signal). The RangeReader
    // contract requires EXACTLY `length` bytes, so loop until the buffer is
    // full or fail loudly on an unexpected EOF instead of silently handing
    // back a short read.
    const buf = new Uint8Array(length)
    let filled = 0
    while (filled < length) {
      let bytesRead: number
      try {
        ;({ bytesRead } = await this.handle.read(buf, filled, length - filled, offset + filled))
      } catch (err) {
        throw new FcbError(ErrorCode.IoError, `failed to read [${offset}, ${offset + length}): ${err}`)
      }
      if (bytesRead === 0) {
        throw new FcbError(
          ErrorCode.IoError,
          `unexpected EOF reading [${offset}, ${offset + length}): got ${filled} of ${length} bytes`,
        )
      }
      filled += bytesRead
    }
    return buf
  }
}

/** Opens a local `.fcb` file for reading and returns a fully-constructed
 *  `FcbReader` -- its header already read and validated.
 *
 *  The file handle stays open for the reader's lifetime -- later queries seek
 *  back into the indices -- so the caller owns closing it, via
 *  `FcbReader.close()` or `await using`.
 *
 *  @param path anything `fs.open` accepts, relative to `process.cwd()`.
 *  @throws `FcbError` with `code` `IoError` if the path cannot be opened or
 *  stat'ed, and the header-validation errors of `FcbReader.fromReader`
 *  (`MissingMagicBytes`, `IllegalHeaderSize`, ...) for a file that is not a
 *  well-formed `.fcb`. */
export async function fromFile(path: string): Promise<FcbReader> {
  return FcbReader.fromReader(await FileRangeReader.open(path))
}
