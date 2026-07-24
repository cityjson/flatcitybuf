/** Error categories. The first thirteen mirror fcb::ErrorCode
 *  (src/cpp/include/fcb/error.hpp) so the implementations report the same
 *  failures under the same names; the rest exist only in this port. */
export enum ErrorCode {
  /** The first 8 bytes are not the FlatCityBuf magic: not an `.fcb` file. */
  MissingMagicBytes = 'missing magic bytes',
  /** The header's size prefix exceeds the 512 MB cap, or runs past EOF. */
  IllegalHeaderSize = 'illegal header size',
  /** A size prefix or table is structurally impossible -- e.g. a feature whose
   *  declared length is 0 or above the 256 MB cap. */
  InvalidFlatbuffer = 'invalid flatbuffer',
  /** A spatial query was run against a file that carries no R-tree. */
  NoIndex = 'no index',
  /** A `where` names a column that does not exist, or one the writer did not
   *  build an attribute index for. This reader queries indices; it never falls
   *  back to a scan. */
  AttributeIndexNotFound = 'attribute index not found',
  /** Part of the shared taxonomy; this port never raises it. */
  NoColumnsInHeader = 'no columns in header',
  /** A field the `.fbs` schema marks required is absent from the table -- e.g.
   *  a `GeometryInstance` with no boundaries, or a point of contact with no
   *  email address. */
  MissingRequiredField = 'missing required field',
  /** A column type this reader cannot query or decode -- notably `Json` and
   *  `Binary`, whose index is a truncated blob and whose hits would be
   *  near-meaningless. */
  UnsupportedColumnType = 'unsupported column type',
  /** An attribute value could not be decoded, or could not be emitted under
   *  the requested `Int64Policy`. */
  InvalidAttributeValue = 'invalid attribute value',
  /** The query itself is ill-formed or the index cannot answer it -- an empty
   *  `where`, a degenerate bbox, an inconsistent index node. */
  QueryExecutionError = 'query execution error',
  /** Any transport-level failure, and the catch-all for a truncated or
   *  misframed file: a short read, an offset past EOF, a use-after-close, or
   *  an aborted read. */
  IoError = 'io error',
  /** A non-2xx status, a missing or malformed `Content-Range`, or a body
   *  shorter than the range that was granted. */
  HttpError = 'http error',
  /** Part of the shared taxonomy; this port never raises it. */
  JsonError = 'json error',
  /** The server answered a Range request with 200 and a whole body. */
  RangeNotSupported = 'range not supported',
  /** A cross-origin 206 whose Content-Range is not exposed by CORS. */
  RangeHeadersNotExposed = 'range headers not exposed',
  /** e.g. `nearest` combined with `where`. */
  UnsupportedQueryCombination = 'unsupported query combination',
  /** A caller argument failed validation before any I/O. */
  InvalidArgument = 'invalid argument',
}

/** Every error this package raises. Catch this one type and switch on
 *  {@link FcbError.code} -- no reader path throws a bare `Error`, a `TypeError`
 *  from a transport, or a Node `ENOENT`; those are all wrapped. */
export class FcbError extends Error {
  /** Which category of failure this is. `message` is always prefixed with the
   *  code's own text, so a logged message stays self-describing. */
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
