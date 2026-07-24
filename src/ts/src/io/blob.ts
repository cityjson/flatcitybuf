/** RangeReader over a `Blob` (or `File`, which extends it) -- the source used
 *  by browser file-picker and drag-drop flows. `Blob.slice`/`arrayBuffer` are
 *  Web-platform APIs available in every modern browser and in Node, so this
 *  file imports nothing from the Node runtime and is safe in a browser
 *  bundle. */
import { ErrorCode, FcbError } from '../errors.js'
import { checkAborted, type ReadOpts, type RangeReader, validateArgs, validateBounds } from './range-reader.js'

/** {@link RangeReader} over a `Blob` -- or a `File`, which extends it, so this
 *  is what backs `FcbReader.fromBlob` for a file picker or a drop event.
 *
 *  Holds only a reference: each `read` is a `Blob.slice().arrayBuffer()`, so a
 *  multi-gigabyte upload is never materialised in memory. There is no OS
 *  resource behind it and nothing to close. */
export class BlobRangeReader implements RangeReader {
  private readonly blob: Blob

  constructor(blob: Blob) {
    this.blob = blob
  }

  size(): number {
    return this.blob.size
  }

  async read(offset: number, length: number, opts?: ReadOpts): Promise<Uint8Array> {
    validateArgs(offset, length)
    checkAborted(opts)
    if (length === 0) return new Uint8Array(0)
    validateBounds(offset, length, this.blob.size)

    let buf: ArrayBuffer
    try {
      buf = await this.blob.slice(offset, offset + length).arrayBuffer()
    } catch (err) {
      throw new FcbError(ErrorCode.IoError, `failed to read blob range [${offset}, ${offset + length}): ${err}`)
    }
    return new Uint8Array(buf)
  }
}
