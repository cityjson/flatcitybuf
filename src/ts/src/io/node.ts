/** RangeReader over a `node:fs` file handle -- the source used by CLI and
 *  server consumers reading a local file. This is the ONLY file in the
 *  package allowed to import `node:*`: a browser bundle that resolves
 *  `node:fs` is a build failure for every other module, so this reader is
 *  reachable only through the package's separate `"./node"` subpath export
 *  (see package.json `exports`), never through the package root. */
import { type FileHandle, open as openFile } from 'node:fs/promises'
import { ErrorCode, FcbError } from '../errors.js'
import { FcbReader } from '../reader.js'
import { checkAborted, type RangeReader, type ReadOpts, validateArgs, validateBounds } from './range-reader.js'

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

/** Opens a local `.fcb` file for reading. The file handle stays open for the
 *  reader's lifetime -- later queries seek back into the indices -- so the
 *  caller owns closing it, via `FcbReader.close()` or `await using`. */
export async function fromFile(path: string): Promise<FcbReader> {
  return FcbReader.fromReader(await FileRangeReader.open(path))
}
