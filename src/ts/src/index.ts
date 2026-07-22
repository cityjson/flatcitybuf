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
// `ColumnInfo.type` is a `ColumnType`, so a consumer that wants to switch on a
// column's type -- to build a query, or label a UI -- needs the enum by name.
// Re-exported from the generated bindings, which are otherwise internal.
export { ColumnType } from './generated/column-type.js'

// Features and attributes.
export { CityObjectView, Feature, decodeAttributes } from './feature/index.js'
export type { AttrValue, JsonValue } from './feature/index.js'

// CityJSON emission.
export { emitInt64, toCityJSONFeature, toCityJSONMetadata } from './cityjson/index.js'
export type { Int64Policy } from './cityjson/index.js'
export type * from './cityjson/types.js'

// Packed R-tree spatial index. `queryToBBox`, `generateLevelBounds`,
// `rtreeNumNodes`, `decodeNodeItem`, `containsPoint` and `NODE_ITEM_SIZE` are
// traversal internals, not public API -- Task 12's brief never asked for
// them, and once published they are hard to withdraw. Tests reach them via
// the deep import `../src/packed-rtree/index.js` instead. `intersects` and
// `searchRtree` stay: `searchRtree` is the actual spatial-query entry point
// (a caller who wants raw hits without the `FcbReader` facade needs it), and
// `intersects` is the bbox predicate a caller would otherwise have to
// reimplement to post-filter or reason about `SearchResultItem`s. `BBox`,
// `NodeItem`, `SearchResultItem` and `SpatialQuery` stay because they are the
// types `SelectOptions.spatial` and `searchRtree`'s own signature are spelled
// in -- a consumer cannot name a query or a hit without them.
export { intersects, searchRtree } from './packed-rtree/index.js'
export type { BBox, NodeItem, SearchResultItem, SpatialQuery } from './packed-rtree/index.js'

// Attribute B+tree. Only the query surface is published, on the same rule the
// R-tree block above states: `searchAttributes` is the attribute-query entry
// point for a caller who wants raw hits without the `FcbReader` facade, and
// `searchStree` runs a single condition against a single column's index.
// `KeyKind`/`keyKindForColumn` are what `searchStree`'s signature is spelled
// in, and `DateTimeKey` is the only way to spell a condition value for a
// DateTime column (a JS `Date` cannot carry nanoseconds). Traversal internals
// -- entries, level bounds, payload decoding -- stay unexported; the tests
// reach them through `../src/static-btree/index.js`.
//
// The four deliberate divergences from the Rust reader that a query caller
// must know about are documented in `static-btree/query.ts`'s module
// docstring.
export { keyKindForColumn, searchAttributes, searchStree } from './static-btree/index.js'
export type { DateTimeKey, KeyKind } from './static-btree/index.js'

// The string post-filter. Published for exactly the callers the block above
// describes: `searchAttributes` answers a `String` condition with CANDIDATES
// (its keys are truncated to 50 bytes and zero-padded), so a caller who
// bypasses `FcbReader.select` needs `postFilterCandidates` to turn those into
// answers -- and `requiresPostFilter` to know whether it has to.
// `compareFullStrings` is the UTF-8-byte ordering the verification uses,
// which a caller must not re-spell as JS `<`.
export {
  compareFullStrings, postFilterCandidates, requiresPostFilter,
} from './post-filter.js'

// The reader facade.
export { FcbReader } from './reader.js'
export type { AttrCondition, FeatureCursor, Operator, SelectOptions } from './reader.js'
