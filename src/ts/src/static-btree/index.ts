// Ports src/rust/fcb_core/src/static_btree (via src/cpp/src/key.cpp and
// src/cpp/src/stree.cpp). `query.ts`'s module docstring is where the four
// deliberate divergences from the Rust reader are stated for query callers.
export {
  compareKeys, decodeKey, encodeKey, keyKindForColumn, keyMax, keyMin, keySize, needsPostFilter,
} from './key.js'
export type { DateTimeKey, KeyKind } from './key.js'
export { entrySize, readEntries } from './entry.js'
export type { Entry } from './entry.js'
export {
  PAYLOAD_MASK, PAYLOAD_TAG, decodePayloadEntry, emitOffset, isTagged, stripTag,
} from './payload.js'
export { generateStreeLevelBounds, searchStree, streeNumNodes } from './stree.js'
export { intersectHits, searchAttributes } from './query.js'
