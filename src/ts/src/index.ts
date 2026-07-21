// Ports the error taxonomy of src/rust/fcb_core/src/error.rs (via src/cpp/include/fcb/error.hpp)
export { ErrorCode, FcbError } from './errors.js'

// I/O. `io/node.js` is deliberately NOT re-exported here: it imports `node:*`
// and is reachable only through the package's separate "./node" subpath.
export { BufferedRangeReader, BytesRangeReader } from './io/range-reader.js'
export type { RangeReader, ReadOpts } from './io/range-reader.js'
export { BlobRangeReader } from './io/blob.js'
export { DEFAULT_FETCH_SIZE, FetchRangeReader, OPEN_PREFETCH_SIZE } from './io/fetch.js'
export type { FetchRangeReaderOpts } from './io/fetch.js'

// Header.
export { readHeader } from './header/index.js'
export type { AttrIndexInfo, ColumnInfo, FileInfo, HeaderView } from './header/index.js'

// Features and attributes.
export { CityObjectView, Feature, decodeAttributes } from './feature/index.js'
export type { AttrValue, JsonValue } from './feature/index.js'

// CityJSON emission.
export { emitInt64, toCityJSONFeature, toCityJSONMetadata } from './cityjson/index.js'
export type { Int64Policy } from './cityjson/index.js'
export type * from './cityjson/types.js'

// The reader facade.
export { FcbReader } from './reader.js'
export type { FeatureCursor } from './reader.js'
