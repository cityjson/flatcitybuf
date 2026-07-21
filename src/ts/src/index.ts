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

// The reader facade.
export { FcbReader } from './reader.js'
export type { AttrCondition, FeatureCursor, Operator, SelectOptions } from './reader.js'
