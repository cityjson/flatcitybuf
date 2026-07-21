/** Error categories. The first thirteen mirror fcb::ErrorCode
 *  (src/cpp/include/fcb/error.hpp) so the implementations report the same
 *  failures under the same names; the rest exist only in this port. */
export enum ErrorCode {
  MissingMagicBytes = 'missing magic bytes',
  IllegalHeaderSize = 'illegal header size',
  InvalidFlatbuffer = 'invalid flatbuffer',
  NoIndex = 'no index',
  AttributeIndexNotFound = 'attribute index not found',
  NoColumnsInHeader = 'no columns in header',
  MissingRequiredField = 'missing required field',
  UnsupportedColumnType = 'unsupported column type',
  InvalidAttributeValue = 'invalid attribute value',
  QueryExecutionError = 'query execution error',
  IoError = 'io error',
  HttpError = 'http error',
  JsonError = 'json error',
  /** The server answered a Range request with 200 and a whole body. */
  RangeNotSupported = 'range not supported',
  /** A cross-origin 206 whose Content-Range is not exposed by CORS. */
  RangeHeadersNotExposed = 'range headers not exposed',
  /** e.g. `nearest` combined with `where`. */
  UnsupportedQueryCombination = 'unsupported query combination',
  /** Two overlapping next() calls on one cursor. */
  ReentrantIteration = 'reentrant iteration',
  /** A caller argument failed validation before any I/O. */
  InvalidArgument = 'invalid argument',
}

/** Every error this package raises. */
export class FcbError extends Error {
  readonly code: ErrorCode

  constructor(code: ErrorCode, message: string) {
    super(`${code}: ${message}`)
    // Restores the prototype chain, which subclassing Error otherwise loses
    // under some downlevel targets. Without it `instanceof FcbError` is false.
    Object.setPrototypeOf(this, new.target.prototype)
    this.name = 'FcbError'
    this.code = code
  }
}
