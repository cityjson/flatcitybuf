// Ports src/rust/fcb_core/src/packed_rtree/mod.rs (via src/cpp/src/packed_rtree.cpp)
export {
  NODE_ITEM_SIZE,
  centroidDistanceSquared,
  containsPoint,
  decodeNodeItem,
  intersects,
  minDistanceSquared,
} from './node-item.js'
export type { BBox, NodeItem } from './node-item.js'
export { WHOLE_INDEX_THRESHOLD, searchNearest, validateNearestPoint } from './nearest.js'
export { generateLevelBounds, queryToBBox, rtreeNumNodes, searchRtree } from './search.js'
export type { SearchResultItem, SpatialQuery } from './search.js'
