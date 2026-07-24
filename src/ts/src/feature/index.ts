/** Feature framing and the decoded-feature handle -- ports
 *  `fcb_core::reader::FeatureIter`'s framing step (src/rust/fcb_core/src/
 *  reader/mod.rs) via `fcb::FeatureIterator::advance` and `fcb::Feature`
 *  (src/cpp/src/reader.cpp:150-220, src/cpp/include/fcb/feature.hpp). */
import * as flatbuffers from 'flatbuffers'
import { ErrorCode, FcbError } from '../errors.js'
import { CityFeature } from '../generated/city-feature.js'
import { CityObject } from '../generated/city-object.js'
import { CityObjectType } from '../generated/city-object-type.js'
// Runtime import of cityjson/ from here, type-only the other way round: the
// emitter takes `Feature` as a TYPE, so there is no runtime import cycle.
import { toCityJSONFeature } from '../cityjson/index.js'
import type { Int64Policy } from '../cityjson/index.js'
import type { CityJSONFeature } from '../cityjson/types.js'
import type { ColumnInfo, HeaderView } from '../header/index.js'
import type { RangeReader, ReadOpts } from '../io/range-reader.js'
import { MAX_FEATURE_SIZE } from '../layout.js'
import { readU32 } from '../le.js'
import { decodeAttributes } from './attribute.js'
import type { AttrValue } from './attribute.js'

export type { AttrValue, JsonValue } from './attribute.js'
export { decodeAttributes } from './attribute.js'

/** The 4-byte LE u32 length that precedes every feature body. */
export const FEATURE_SIZE_PREFIX = 4

/** The CityJSON spelling of a City Object tag this reader cannot name --
 *  cityjson.cpp's kUnknownCityObjectName. Reached only for a tag outside the
 *  generated enum's named range, or for `ExtensionObject` with no
 *  `extension_type` string to use instead. */
const UNKNOWN_CITY_OBJECT = '+UnknownCityObject'

/** FlatBuffers vtable slots of the CityObject fields whose PRESENCE (not
 *  emptiness) this module has to distinguish. Field n lives at 4 + 2n:
 *  `attributes` is field 6, `columns` is field 7 (src/fbs/feature.fbs).
 *  There is no generated accessor for "is this field present at all" on a
 *  vector, so the vtable is consulted directly -- see fieldPresent. */
const CITY_OBJECT_ATTRIBUTES_SLOT = 16
const CITY_OBJECT_COLUMNS_SLOT = 18

/** True iff the table actually stores the field, as opposed to omitting it
 *  and letting the accessor return the default. `columnsLength()` and
 *  `attributesLength()` both return 0 for "absent" AND for "present but
 *  empty", and this port must tell those apart: an absent attributes vector
 *  is omitted from the output entirely while a present-but-empty one becomes
 *  `{}`, and an explicitly empty `columns` overrides the header schema rather
 *  than falling back to it (feature.hpp:68-71). `__offset` is the generated
 *  code's own presence primitive -- every accessor above starts with it -- so
 *  this is the same check the generated getters make, not a raw byte read. */
function fieldPresent(obj: CityObject, slot: number): boolean {
  return obj.bb!.__offset(obj.bb_pos, slot) !== 0
}

/** One CityObject inside a feature, with the attribute schema that governs
 *  it already resolved. */
export class CityObjectView {
  /** The CityObject's id -- the key it appears under in the emitted
   *  `CityObjects` map. `''` for a malformed object that omits it. */
  readonly id: string
  /** The CityJSON type name (`'Building'`, `'BuildingPart'`, ...). An
   *  extension object's own `extension_type` string wins verbatim; a tag
   *  outside the generated enum's range becomes `'+UnknownCityObject'` rather
   *  than throwing. */
  readonly type: string
  private readonly raw: CityObject
  private readonly headerColumns: readonly ColumnInfo[]

  constructor(raw: CityObject, headerColumns: readonly ColumnInfo[]) {
    this.raw = raw
    this.headerColumns = headerColumns
    this.id = raw.id() ?? ''
    // An `extension_type` string, when present, wins verbatim over the tag
    // (cityjson.cpp:505-507). CityObjectType is a numeric enum, so the
    // reverse lookup names any tag in range; anything else, including a tag a
    // newer encoder added, falls back to the unknown-tag spelling instead of
    // throwing -- the same policy as city_object_type_name.
    this.type = raw.extensionType()
      ?? (CityObjectType[raw.type()] as string | undefined)
      ?? UNKNOWN_CITY_OBJECT
  }

  /** The parsed FlatBuffers table behind this view.
   *
   *  For callers that need a field this view does not surface -- the CityJSON
   *  emitter reads `geometry`, `geometry_instances`, `children`,
   *  `children_roles`, `parents` and `geographical_extent` off it. Deliberately
   *  a method rather than a public field, so it reads as "reach through to the
   *  raw table" at every call site. */
  rawObject(): CityObject {
    return this.raw
  }

  /** Whether this object DECLARES an attributes vector. Distinct from that
   *  vector being empty: present-but-empty must serialize as `"attributes":
   *  {}` while absent is omitted, and the corpus contains both. */
  hasAttributes(): boolean {
    return fieldPresent(this.raw, CITY_OBJECT_ATTRIBUTES_SLOT)
  }

  /** Whether this object declares its OWN column schema, which then overrides
   *  the header's. Presence is what selects the override, not non-emptiness:
   *  an explicitly empty schema must not silently fall back to the header's. */
  hasColumns(): boolean {
    return fieldPresent(this.raw, CITY_OBJECT_COLUMNS_SLOT)
  }

  /** The schema that governs THIS object's attribute blob. Resolving this per
   *  object is not an optimisation, it is correctness: attribute records are
   *  not self-delimiting, so decoding with the header's schema when the object
   *  declares its own desynchronises the blob and yields plausible garbage
   *  rather than an error. In examples/data/delft.fcb every object with
   *  attributes declares its own columns and the header's 44 are never used. */
  columns(): readonly ColumnInfo[] {
    if (!this.hasColumns()) return this.headerColumns
    const out: ColumnInfo[] = []
    for (let i = 0; i < this.raw.columnsLength(); i++) {
      const c = this.raw.columns(i)
      if (c === null) continue
      out.push({ index: c.index(), name: c.name() ?? '', type: c.type(), nullable: c.nullable() })
    }
    return out
  }

  /** The raw attribute bytes, or null when the object declares no attributes
   *  vector at all. */
  attributesBlob(): Uint8Array | null {
    if (!this.hasAttributes()) return null
    // `attributesArray()` returns null for an absent vector but a zero-length
    // array for a present-empty one; hasAttributes() has already ruled out
    // the former, so the ?? is only a type-level fallback.
    return this.raw.attributesArray() ?? new Uint8Array(0)
  }

  /** This object's attributes, decoded against {@link CityObjectView.columns}.
   *  `{}` both for an object that declares no attributes vector and for one
   *  that declares an empty one -- use {@link CityObjectView.hasAttributes} to
   *  tell those apart. Decoded fresh on every call, not cached. */
  attributes(): Record<string, AttrValue> {
    const blob = this.attributesBlob()
    if (blob === null) return {}
    return decodeAttributes(blob, this.columns())
  }
}

/** One decoded feature that OWNS the bytes it points into.
 *
 *  The buffer handed to the constructor must be a private copy starting at
 *  index 0 of its own ArrayBuffer -- readFeature guarantees that. See
 *  readFeature for why both halves of that matter. */
export class Feature {
  /** The feature's id -- the `id` member of the emitted CityJSONFeature.
   *  `''` for a malformed feature that omits the required field. */
  readonly id: string
  /** Byte offset of this feature RELATIVE to the start of the features
   *  section, matching the offsets stored in the R-tree leaves. */
  readonly byteOffset: number
  private readonly raw: CityFeature
  private readonly headerColumns: readonly ColumnInfo[]
  private objects: CityObjectView[] | undefined

  constructor(bytes: Uint8Array, headerColumns: readonly ColumnInfo[], byteOffset: number) {
    const bb = new flatbuffers.ByteBuffer(bytes)
    // Size-prefixed root: the accessor reads and skips the 4-byte prefix
    // itself, so `bytes` must START at the prefix, not after it (pinned in
    // test/generated.test.ts).
    this.raw = CityFeature.getSizePrefixedRootAsCityFeature(bb)
    this.headerColumns = headerColumns
    this.byteOffset = byteOffset
    // `id` is a required field, but a malformed file can still omit it and
    // the generated getter is typed string|null; mirror header.cpp's policy
    // of defaulting to "" rather than throwing.
    this.id = this.raw.id() ?? ''
  }

  /** The parsed FlatBuffers table behind this feature -- for the fields this
   *  handle does not surface, chiefly the per-feature `appearance`. */
  rawFeature(): CityFeature {
    return this.raw
  }

  /** This feature as one CityJSONSeq line. A convenience over
   *  `toCityJSONFeature(feature, header)`; the emission itself lives in
   *  cityjson/, which is where the CityJSON document model lives. */
  toCityJSON(header: HeaderView, opts?: Int64Policy): CityJSONFeature {
    return toCityJSONFeature(this, header, opts)
  }

  /** Built once and cached: the views are immutable and callers commonly
   *  iterate them alongside `attributes(i)`, which indexes the same list. */
  cityObjects(): CityObjectView[] {
    if (this.objects === undefined) {
      const out: CityObjectView[] = []
      for (let i = 0; i < this.raw.objectsLength(); i++) {
        const o = this.raw.objects(i)
        if (o === null) continue
        out.push(new CityObjectView(o, this.headerColumns))
      }
      this.objects = out
    }
    return this.objects
  }

  /** Shorthand for `cityObjects()[objectIndex].attributes()`, decoded against
   *  the schema that governs THAT object -- its own `columns` when it declares
   *  them, the header's otherwise.
   *
   *  @throws `FcbError` with `code` `InvalidArgument` when `objectIndex` is out
   *  of range. */
  attributes(objectIndex: number): Record<string, AttrValue> {
    const obj = this.cityObjects()[objectIndex]
    if (obj === undefined) {
      throw new FcbError(
        ErrorCode.InvalidArgument,
        `city object index ${objectIndex} out of range (${this.cityObjects().length} objects)`,
      )
    }
    return obj.attributes()
  }

  /** All vertices flattened to x,y,z triples of quantized integers -- apply
   *  the header's scale/translate to get world coordinates.
   *
   *  Built element by element through the generated `Vertex` accessors rather
   *  than as a typed-array view over the struct vector: an Int32Array view
   *  would decode in the HOST's byte order, which is only accidentally the
   *  little-endian the format specifies. */
  vertices(): Int32Array {
    const n = this.raw.verticesLength()
    const out = new Int32Array(n * 3)
    for (let i = 0; i < n; i++) {
      const v = this.raw.vertices(i)
      if (v === null) continue
      out[i * 3] = v.x()
      out[i * 3 + 1] = v.y()
      out[i * 3 + 2] = v.z()
    }
    return out
  }
}

/** Reads the one feature that begins at absolute offset `at`, and reports
 *  where the next one begins.
 *
 *  Framing, exactly: read the 4-byte LE prefix; reject a length of 0 or one
 *  above MAX_FEATURE_SIZE BEFORE allocating anything (a crafted 0xFFFFFFFF
 *  prefix would otherwise ask for ~4 GiB); check `at + 4 + len` against the
 *  resource size; then read and COPY all `4 + len` bytes, prefix included.
 *
 *  TWO `reader.read` calls per feature, deliberately: a 4-byte prefix read at
 *  step 1, then a `4 + len` body read at step 5 that starts at the SAME
 *  offset `at` and so re-reads those same 4 bytes. The length has to be
 *  known before the body read can be sized, and re-reading the prefix (rather
 *  than splicing the already-read 4 bytes onto a second `[at+4, at+4+len)`
 *  read) keeps the body a single contiguous read starting at `at`, which is
 *  what makes the size-prefixed FlatBuffers accessor in step 6 work directly
 *  on `bytes` with no reassembly. The cost is real: a sequential scan issues
 *  2x as many `read` calls as there are features (plus the header's own
 *  reads), not 1x -- doubled syscalls over `fromFile`, doubled HTTP requests
 *  over a raw (unbuffered) network reader. `FcbReader.fromReader`'s docstring
 *  says the same thing from the caller's side; see it for what to do about
 *  the HTTP case.
 *
 *  `opts` is forwarded to BOTH `reader.read` calls, unchanged, so a caller's
 *  `AbortSignal` reaches the actual in-flight read rather than stopping at
 *  some facade above it -- an already-aborted signal is caught by the reader
 *  at the START of either read (`checkAborted`), including between the
 *  prefix read and the body read.
 *
 *  The copy is load-bearing twice over:
 *   1. Durability. `RangeReader.read` may return a view into a buffer the
 *      reader reuses or overwrites on its next call (BufferedRangeReader
 *      documents exactly this), so a Feature that aliased it would decay into
 *      garbage as soon as the scan moved on.
 *   2. Alignment. The copy starts at index 0 of a FRESH ArrayBuffer, so every
 *      FlatBuffers-internal alignment assumption holds. Handing FlatBuffers a
 *      `subarray` at an arbitrary byteOffset makes the generated `*Array()`
 *      accessors -- which build typed-array views over the underlying
 *      ArrayBuffer -- throw RangeError on a misaligned start. */
export async function readFeature(
  reader: RangeReader,
  at: number,
  headerColumns: readonly ColumnInfo[],
  featureBegin: number,
  opts?: ReadOpts,
): Promise<{ feature: Feature; next: number }> {
  const total = reader.size()
  if (at + FEATURE_SIZE_PREFIX > total) {
    throw new FcbError(ErrorCode.IoError, `feature offset ${at} past end of resource`)
  }

  const prefix = await reader.read(at, FEATURE_SIZE_PREFIX, opts)
  const len = readU32(new DataView(prefix.buffer, prefix.byteOffset, prefix.byteLength), 0)
  if (len === 0 || len > MAX_FEATURE_SIZE) {
    throw new FcbError(ErrorCode.InvalidFlatbuffer, `implausible feature size: ${len}`)
  }

  const want = FEATURE_SIZE_PREFIX + len
  if (at + want > total) {
    throw new FcbError(
      ErrorCode.IoError,
      `truncated feature body: [${at}, ${at + want}) exceeds resource size ${total}`,
    )
  }

  // `.slice()` on a Uint8Array allocates a fresh ArrayBuffer of exactly this
  // length and copies into it at index 0 -- both properties are required.
  const bytes = (await reader.read(at, want, opts)).slice()
  return {
    feature: new Feature(bytes, headerColumns, at - featureBegin),
    next: at + want,
  }
}
