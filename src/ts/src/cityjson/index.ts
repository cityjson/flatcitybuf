/** CityJSON emission -- ports `fcb_core::reader::deserializer::to_cj_metadata`
 *  and `::to_cj_feature` (src/rust/fcb_core/src/reader/deserializer.rs) via
 *  `fcb::to_cityjson_metadata` / `fcb::to_cityjson_feature`
 *  (src/cpp/src/cityjson.cpp).
 *
 *  This is the module the conformance suite grades: every `.expected.jsonl`
 *  line in `conformance/` was produced by the Rust reader, so where the Rust
 *  and the C++ readers disagree THE RUST ONE WINS and the divergence is
 *  called out in a comment at the site.
 *
 *  ABSENT IS NOT EMPTY, everywhere in this file. serde drops an `Option::None`
 *  member entirely, so an absent member must be absent from the emitted
 *  object -- not present with the value `undefined`. `toEqual` distinguishes
 *  those two; `JSON.stringify` does not, which is exactly how such a bug
 *  survives a round trip through a file. Hence `put` below: this module never
 *  assigns `undefined` to a key. */
import { ErrorCode, FcbError } from '../errors.js'
import type { AttrValue } from '../feature/attribute.js'
import type { CityObjectView, Feature } from '../feature/index.js'
import { Appearance as FbAppearance } from '../generated/appearance.js'
import { DoubleVertex } from '../generated/double-vertex.js'
import { Extension } from '../generated/extension.js'
import { Geometry as FbGeometry } from '../generated/geometry.js'
import type { GeometryInstance } from '../generated/geometry-instance.js'
import type { Header as FbHeader } from '../generated/header.js'
import { Material as FbMaterial } from '../generated/material.js'
import { MaterialMapping } from '../generated/material-mapping.js'
import { Texture as FbTexture } from '../generated/texture.js'
import { TextureMapping } from '../generated/texture-mapping.js'
import { TextureFormat } from '../generated/texture-format.js'
import { TextureType } from '../generated/texture-type.js'
import { Vec2 } from '../generated/vec2.js'
import { WrapMode } from '../generated/wrap-mode.js'
import type { ColumnInfo, HeaderView } from '../header/index.js'
import {
  decodeBoundaries, decodeMaterialValues, decodeSemantics, decodeTextureValues,
  geometryTypeName, sharedMaterialValue,
} from '../geometry/index.js'
import type {
  Appearance, CityJSON, CityJSONFeature, CityObject, Geometry, GeometryTemplates,
  IndexValues, JsonValue, MaterialObject, MaterialReference, Metadata, PointOfContact,
  Semantics, SemanticSurface, TextureObject, TextureReference,
} from './types.js'

export type * from './types.js'

/** How a `Long`/`ULong` attribute -- a `bigint` after decoding -- is spelled
 *  in the emitted, JSON-serializable document. See `emitInt64`. */
export interface Int64Policy {
  /** `'lossy-number'` (the default) emits a JS number, rounding beyond
   *  `Number.MAX_SAFE_INTEGER`; it is what makes whole-line comparison against
   *  the conformance oracle meaningful. `'decimal-string'` keeps every digit
   *  at the cost of changing the JSON type from number to string.
   *  `'error'` throws `InvalidAttributeValue` rather than lose a digit
   *  silently. No policy ever leaks a `bigint` into the emitted object. */
  int64?: 'lossy-number' | 'decimal-string' | 'error'
}

/** Assigns `key` only when `value` is defined, so an absent member is absent
 *  from the object rather than present-and-`undefined`. */
function put<T, K extends keyof T>(target: T, key: K, value: T[K] | undefined): void {
  if (value !== undefined) target[key] = value
}

/** True iff the table actually STORES the field, as opposed to omitting it
 *  and letting the accessor return a default. The generated `*Length()`
 *  accessors return 0 for "absent" and for "present but empty" alike, and the
 *  Rust reader keys real behaviour off the difference -- `header.columns()`
 *  being `None` suppresses a City Object's attributes entirely
 *  (deserializer.rs:551-556). `__offset` is the generated code's own presence
 *  primitive; every generated getter starts with it, so this is the same
 *  check the accessors make and not a raw byte read. The same helper exists
 *  in feature/index.ts and geometry/semantics.ts for their own tables. */
function fieldPresent(
  table: { bb: { __offset(pos: number, slot: number): number } | null; bb_pos: number },
  slot: number,
): boolean {
  const bb = table.bb
  if (bb === null) throw new FcbError(ErrorCode.InvalidFlatbuffer, 'unbound FlatBuffers table')
  return bb.__offset(table.bb_pos, slot) !== 0
}

/** `Header.columns` is field 2, `semantic_columns` field 3, `templates`
 *  field 12 and `templates_vertices` field 13 (src/fbs/header.fbs); field n
 *  lives at vtable slot 4 + 2n. */
const HEADER_COLUMNS_SLOT = 8
const HEADER_SEMANTIC_COLUMNS_SLOT = 10
const HEADER_TEMPLATES_SLOT = 28
const HEADER_TEMPLATES_VERTICES_SLOT = 30

/** `CityObject.children` is field 8, `children_roles` field 9 and `parents`
 *  field 10 (src/fbs/feature.fbs). The Rust reader keys all three off
 *  PRESENCE (`co.children().map(...)`), so a present-but-empty vector is
 *  `"children": []` and not a missing member -- the C++ reader's
 *  `size() > 0` guard is the divergence, and the oracle is Rust. */
const CITY_OBJECT_CHILDREN_SLOT = 20
const CITY_OBJECT_CHILDREN_ROLES_SLOT = 22
const CITY_OBJECT_PARENTS_SLOT = 24

/** The largest magnitude a JS number represents exactly. */
const MAX_SAFE = BigInt(Number.MAX_SAFE_INTEGER)

/** A decoded `Long`/`ULong` attribute, made JSON-serializable.
 *
 *  `Long` and `ULong` columns decode to `bigint` (feature/attribute.ts), which
 *  `JSON.stringify` refuses outright. Every emission path therefore goes
 *  through here, and no `bigint` ever reaches the emitted object.
 *
 *  Three policies, because there is no single right answer:
 *   * `'lossy-number'` (the default) matches what the Rust reader's
 *     `serde_json::Number` does once the document is written out and read
 *     back by a JSON parser with double semantics -- which is what the
 *     conformance oracle is -- so it is the only policy under which whole-line
 *     comparison against `.expected.jsonl` is meaningful.
 *   * `'decimal-string'` keeps every digit, at the cost of changing the JSON
 *     type. For a caller that must not lose data.
 *   * `'error'` refuses rather than lose data silently.
 *
 *  The safe-range test is done in `bigint` arithmetic. Converting first and
 *  comparing afterwards cannot work: `Number(2n**53n + 1n)` is already
 *  rounded, so it tests equal to the safe boundary it just left. */
export function emitInt64(
  value: bigint,
  policy: 'lossy-number' | 'decimal-string' | 'error',
): number | string {
  if (policy === 'decimal-string') return value.toString()
  if (policy === 'error' && (value > MAX_SAFE || value < -MAX_SAFE)) {
    throw new FcbError(
      ErrorCode.InvalidAttributeValue,
      `int64 value ${value.toString()} is not exactly representable as a JSON number`,
    )
  }
  return Number(value)
}

/** One decoded attribute value, made JSON-serializable.
 *
 *  Two conversions, both of which the emitted document needs and the decoder
 *  deliberately does not do:
 *   * `bigint` -> `emitInt64` (see above).
 *   * `Uint8Array` -> an array of byte values, which is what the Rust reader
 *     emits for a `Binary` column (deserializer.rs:413-425). Anything else
 *     would not survive `JSON.stringify` as data. */
function attrToJson(value: AttrValue, policy: Required<Int64Policy>['int64']): JsonValue {
  if (typeof value === 'bigint') return emitInt64(value, policy)
  if (value instanceof Uint8Array) return Array.from(value)
  return value as JsonValue
}

function attrsToJson(
  attrs: Record<string, AttrValue>,
  policy: Required<Int64Policy>['int64'],
): Record<string, JsonValue> {
  const out: Record<string, JsonValue> = {}
  for (const [k, v] of Object.entries(attrs)) out[k] = attrToJson(v, policy)
  return out
}

/** A material colour. `appearance.schema.json` fixes `diffuseColor`,
 *  `emissiveColor` and `specularColor` at exactly three numbers; a stored
 *  vector of any other length is not a colour and is DROPPED rather than
 *  padded or truncated (deserializer.rs `to_color`). */
function color(v: Float64Array | null): [number, number, number] | undefined {
  if (v === null || v.length !== 3) return undefined
  return [v[0]!, v[1]!, v[2]!]
}

/** `borderColor` is the one CityJSON colour not fixed at three numbers:
 *  `"minItems": 3, "maxItems": 4`. Any other length is dropped. */
function borderColor(v: Float64Array | null): number[] | undefined {
  if (v === null || (v.length !== 3 && v.length !== 4)) return undefined
  return Array.from(v)
}

/** UNKNOWN-TAG POLICY, the appearance-enum site. Unlike a City Object type or
 *  a semantic surface type, `type`/`wrapMode`/`textureType` have no
 *  `+`-prefixed extension form, so an unrecognised tag has no legal spelling
 *  and this refuses. Falling back to the first table entry is what made a
 *  texture written `"wrapMode": "wrap"` come back as `"none"` -- a defect only
 *  a conformance corpus caught. Both other readers throw here too. */
function enumName<T extends string>(names: readonly T[], tag: number, member: string): T {
  const name = names[tag]
  if (name === undefined) {
    throw new FcbError(ErrorCode.InvalidFlatbuffer, `unknown ${member} tag ${tag}`)
  }
  return name
}

/** CityJSON's three appearance enumerations use two different casings: `type`
 *  is UPPER, `wrapMode` and `textureType` are lower. From
 *  `appearance.schema.json`, not from memory. Declaration order matches
 *  src/fbs/header.fbs. */
const TEXTURE_FORMAT_NAMES = ['PNG', 'JPG'] as const
const WRAP_MODE_NAMES = ['none', 'wrap', 'mirror', 'clamp', 'border'] as const
const TEXTURE_TYPE_NAMES = ['unknown', 'specific', 'typical'] as const

/** The per-feature `appearance`: the materials, textures and UV vertices this
 *  feature's geometry mappings index into. Without it the mappings reference
 *  nothing a consumer can resolve. The C++ port forgot this entirely and its
 *  conformance test did not catch it, because that test compared only
 *  selected keys -- which is why this suite compares whole lines. */
function appearanceToJson(a: FbAppearance): Appearance {
  const out: Appearance = {}

  // Presence, not emptiness, decides each member: `Some(vec![])` serializes
  // as `[]` and the corpus contains exactly that (geom_temp's last feature
  // carries `"materials": [], "textures": [], "vertices-texture": []`).
  if (fieldPresent(a, 4)) {
    const materials: MaterialObject[] = []
    for (let i = 0; i < a.materialsLength(); i++) {
      const m = a.materials(i, new FbMaterial())
      if (m === null) continue
      // `name` is the schema's one required member; every other one is
      // optional and stays absent rather than reappearing as `null`.
      const j: MaterialObject = { name: m.name() ?? '' }
      put(j, 'ambientIntensity', m.ambientIntensity() ?? undefined)
      put(j, 'diffuseColor', color(m.diffuseColorArray()))
      put(j, 'emissiveColor', color(m.emissiveColorArray()))
      put(j, 'specularColor', color(m.specularColorArray()))
      put(j, 'shininess', m.shininess() ?? undefined)
      put(j, 'transparency', m.transparency() ?? undefined)
      put(j, 'isSmooth', m.isSmooth() ?? undefined)
      materials.push(j)
    }
    out.materials = materials
  }

  if (fieldPresent(a, 6)) {
    const textures: TextureObject[] = []
    for (let i = 0; i < a.texturesLength(); i++) {
      const t = a.textures(i, new FbTexture())
      if (t === null) continue
      const j: TextureObject = {
        type: enumName(TEXTURE_FORMAT_NAMES, t.type() as TextureFormat, 'type'),
      }
      // `image` is mandatory in header.fbs but optional in CityJSON, so the
      // writer stores "" both for an absent `image` and for a schema-valid
      // `"image": ""`. The two are indistinguishable on the wire and one of
      // them must be lost; decoding "" back to ABSENT is a deliberate choice
      // mirrored from deserializer.rs, not something derivable from the
      // schema.
      const image = t.image()
      if (image !== null && image.length > 0) j.image = image
      const wrap = t.wrapMode()
      if (wrap !== null) j.wrapMode = enumName(WRAP_MODE_NAMES, wrap as WrapMode, 'wrapMode')
      const tt = t.textureType()
      if (tt !== null) j.textureType = enumName(TEXTURE_TYPE_NAMES, tt as TextureType, 'textureType')
      put(j, 'borderColor', borderColor(t.borderColorArray()))
      textures.push(j)
    }
    out.textures = textures
  }

  if (fieldPresent(a, 8)) {
    const uvs: [number, number][] = []
    for (let i = 0; i < a.verticesTextureLength(); i++) {
      const v = a.verticesTexture(i, new Vec2())
      if (v === null) continue
      uvs.push([v.u(), v.v()])
    }
    out['vertices-texture'] = uvs
  }

  put(out, 'default-theme-texture', a.defaultThemeTexture() ?? undefined)
  put(out, 'default-theme-material', a.defaultThemeMaterial() ?? undefined)
  return out
}

/** `material`: theme -> a shared index or a nested values array.
 *
 *  ABSENT-VS-EMPTY, the mapping site. A mapping with NO `vertices` vector at
 *  all is `"values": null` -- a member present with a null value, which the
 *  schema requires and permits (geom_decoder.rs:403-413). A mapping whose
 *  `vertices` is present but empty is `"values": []`. Collapsing the two onto
 *  "skip the theme" drops an explicit null. */
function materialsToJson(g: FbGeometry): Record<string, MaterialReference> {
  const out: Record<string, MaterialReference> = {}
  for (let i = 0; i < g.materialLength(); i++) {
    const m = g.material(i, new MaterialMapping())
    if (m === null) continue
    const theme = m.theme() ?? 'theme'

    // A `value` colours the whole object and has no depth at all. Read with
    // `!== undefined`, never for truthiness: a shared material index of 0 is
    // real and falsy.
    const shared = sharedMaterialValue(m)
    if (shared !== undefined) {
      out[theme] = { value: shared }
      continue
    }

    const vertices = m.verticesArray()
    if (vertices === null) {
      out[theme] = { values: null }
      continue
    }
    out[theme] = {
      values: decodeMaterialValues(
        g.type(), m.solidsArray() ?? [], m.shellsArray() ?? [], vertices,
      ) as IndexValues[],
    }
  }
  return out
}

/** `texture`: theme -> a nested values array.
 *
 *  ABSENT-VS-EMPTY again, and with a DIFFERENT answer from materials: the
 *  per-theme texture object carries no `required` keyword, so a theme with no
 *  `values` member at all is valid CityJSON and is written with no `vertices`
 *  vector; it decodes to an EMPTY OBJECT, not to `"values": null` and not to
 *  a missing theme (geom_decoder.rs:511-520). */
function texturesToJson(g: FbGeometry): Record<string, TextureReference> {
  const out: Record<string, TextureReference> = {}
  for (let i = 0; i < g.textureLength(); i++) {
    const m = g.texture(i, new TextureMapping())
    if (m === null) continue
    const theme = m.theme() ?? 'theme'

    const vertices = m.verticesArray()
    if (vertices === null) {
      out[theme] = {}
      continue
    }
    out[theme] = {
      values: decodeTextureValues(
        g.type(),
        m.solidsArray() ?? [], m.shellsArray() ?? [], m.surfacesArray() ?? [],
        m.stringsArray() ?? [], vertices,
      ) as IndexValues[],
    }
  }
  return out
}

/** One geometry, at the depth its TYPE implies -- never at the depth its count
 *  arrays suggest (upstream finding #8; see geometry/boundaries.ts).
 *
 *  `geometryTypeName` is called FIRST, before anything is decoded, so that the
 *  UNKNOWN-TAG POLICY applies: a tag outside the eight CityJSON geometry types
 *  has no legal spelling and must be refused rather than decoded at a guessed
 *  depth. Every decoder below is then reached only for a known tag, so their
 *  `default:` arms are fallbacks, not unknown-tag handlers. */
function geometryToJson(
  g: FbGeometry,
  semanticColumns: readonly ColumnInfo[] | null,
  policy: Required<Int64Policy>['int64'],
): Geometry {
  const type = geometryTypeName(g.type())

  const out: Geometry = { type, boundaries: [] }
  put(out, 'lod', g.lod() ?? undefined)

  out.boundaries = decodeBoundaries(
    g.type(),
    g.solidsArray() ?? [], g.shellsArray() ?? [], g.surfacesArray() ?? [],
    g.stringsArray() ?? [], g.boundariesArray() ?? [],
  )

  const semantics = decodeSemantics(g, semanticColumns)
  if (semantics !== null) {
    // A semantic surface's own attributes are merged in alongside `type`,
    // `parent` and `children`, and they are ordinary attributes: a `Long`
    // column on a semantic surface decodes to a `bigint` exactly as a feature
    // attribute does, so the same int64 policy applies here.
    const surfaces: SemanticSurface[] = semantics.surfaces.map((s) => {
      // `type`, `parent` and `children` are already JSON values; everything
      // else came out of the surface's attribute blob and may be a `bigint` or
      // a `Uint8Array`, so it goes through the same conversion a feature
      // attribute does.
      const j: Record<string, JsonValue> = { type: s.type }
      for (const [k, v] of Object.entries(s)) {
        if (k === 'type') continue
        j[k] = k === 'parent' || k === 'children'
          ? (v as JsonValue)
          : attrToJson(v as AttrValue, policy)
      }
      return j as SemanticSurface
    })
    const sem: Semantics = { surfaces, values: semantics.values as IndexValues[] | null }
    out.semantics = sem
  }

  // An EMPTY mapping vector omits the key entirely: the reference returns
  // `None` for an empty slice (geom_decoder.rs:343, :472) and serde drops the
  // member. Every mapping in a NON-empty vector yields a theme -- an absent
  // `vertices` is a null or an empty `values`, not a theme to skip.
  if (g.materialLength() > 0) out.material = materialsToJson(g)
  if (g.textureLength() > 0) out.texture = texturesToJson(g)

  return out
}

/** A `GeometryInstance` carries no lod, semantics, material or texture: those
 *  live on the template it refers to. Its `boundaries` array holds exactly one
 *  vertex index -- CityGML's reference point for the placement. */
function geometryInstanceToJson(gi: GeometryInstance): Geometry {
  const boundaries = gi.boundariesArray()
  if (boundaries === null) {
    throw new FcbError(ErrorCode.MissingRequiredField, 'geometryinstance boundaries')
  }
  if (boundaries.length !== 1) {
    throw new FcbError(
      ErrorCode.InvalidAttributeValue,
      `geometryinstance boundaries should contain exactly one vertex index, found ${boundaries.length}`,
    )
  }
  const m = gi.transformation()
  if (m === null) {
    throw new FcbError(ErrorCode.MissingRequiredField, 'geometryinstance transformation field')
  }
  return {
    type: 'GeometryInstance',
    boundaries: [boundaries[0]!],
    template: gi.template(),
    transformationMatrix: [
      m.m00(), m.m01(), m.m02(), m.m03(),
      m.m10(), m.m11(), m.m12(), m.m13(),
      m.m20(), m.m21(), m.m22(), m.m23(),
      m.m30(), m.m31(), m.m32(), m.m33(),
    ],
  }
}

/** Every string of a FlatBuffers string vector, or `undefined` when the vector
 *  is absent. Presence, not emptiness: see CITY_OBJECT_CHILDREN_SLOT. */
function stringVector(
  present: boolean, length: number, at: (i: number) => string | null,
): string[] | undefined {
  if (!present) return undefined
  const out: string[] = []
  for (let i = 0; i < length; i++) out.push(at(i) ?? '')
  return out
}

function cityObjectToJson(
  view: CityObjectView,
  headerHasColumns: boolean,
  semanticColumns: readonly ColumnInfo[] | null,
  policy: Required<Int64Policy>['int64'],
): CityObject {
  const raw = view.rawObject()
  const out: CityObject = { type: view.type }

  const extent = raw.geographicalExtent()
  if (extent !== null) {
    const min = extent.min()!
    const max = extent.max()!
    out.geographicalExtent = [min.x(), min.y(), min.z(), max.x(), max.y(), max.z()]
  }

  // WHICH SCHEMA GOVERNS, and what happens when there is none.
  //
  // The Rust reader drops a City Object's attributes ENTIRELY when neither
  // the header nor the object declares a column vector -- `if
  // root_attr_schema.is_none() && co.columns().is_none() { None }`
  // (deserializer.rs:551-556) -- rather than failing on the first column
  // index it cannot resolve. That is the case this port used to throw on,
  // because `decodeAttributes([] , blob)` reports an unknown column index.
  // Resolved in the reference's favour: no schema is not a corrupt file, it
  // is a file that says nothing about its attribute names, and there is
  // nothing a reader could name them. A schema that IS declared but does not
  // cover an index still throws -- that one really is corruption.
  if (view.hasAttributes() && (headerHasColumns || view.hasColumns())) {
    out.attributes = attrsToJson(view.attributes(), policy)
  }

  const geometry: Geometry[] = []
  for (let i = 0; i < raw.geometryLength(); i++) {
    const g = raw.geometry(i, new FbGeometry())
    if (g !== null) geometry.push(geometryToJson(g, semanticColumns, policy))
  }
  for (let i = 0; i < raw.geometryInstancesLength(); i++) {
    const gi = raw.geometryInstances(i)
    if (gi !== null) geometry.push(geometryInstanceToJson(gi))
  }
  if (geometry.length > 0) out.geometry = geometry

  put(out, 'children', stringVector(
    fieldPresent(raw, CITY_OBJECT_CHILDREN_SLOT), raw.childrenLength(), (i) => raw.children(i),
  ))
  // An unspecified role is `null` in CityJSON, and the writer spells that as
  // the empty string (deserializer.rs:574-579).
  if (fieldPresent(raw, CITY_OBJECT_CHILDREN_ROLES_SLOT)) {
    const roles: (string | null)[] = []
    for (let i = 0; i < raw.childrenRolesLength(); i++) {
      const s = raw.childrenRoles(i)
      roles.push(s === null || s.length === 0 ? null : s)
    }
    out.childrenRoles = roles
  }
  put(out, 'parents', stringVector(
    fieldPresent(raw, CITY_OBJECT_PARENTS_SLOT), raw.parentsLength(), (i) => raw.parents(i),
  ))

  return out
}

/** The columns a `SemanticObject`'s attribute blob is decoded against, or
 *  `null` when the header declares no `semantic_columns` vector at all.
 *
 *  `null` is not the same as `[]`. The Rust reader DROPS a semantic surface's
 *  attributes when the schema is `None`
 *  (`semantic_attr_schema.as_ref().and_then(...)`, geom_decoder.rs:162-164)
 *  and would fail on an unresolvable index when the schema is present; `[]`
 *  here would mean "a declared, empty schema" and throw on the first
 *  attribute. Same policy as for City Object attributes above. */
function semanticColumnsOf(header: HeaderView): readonly ColumnInfo[] | null {
  return fieldPresent(header.raw, HEADER_SEMANTIC_COLUMNS_SLOT)
    ? header.info.semanticColumns
    : null
}

/** `metadata.pointOfContact`, emitted only when `poc_contact_name` is stored
 *  (deserializer.rs:300-303). `contactName` and `emailAddress` are the
 *  schema's two required members. */
function pointOfContact(hdr: FbHeader): PointOfContact | undefined {
  const name = hdr.pocContactName()
  if (name === null) return undefined

  const email = hdr.pocEmail()
  if (email === null) {
    throw new FcbError(ErrorCode.MissingRequiredField, 'email_address')
  }

  const poc: PointOfContact = { contactName: name, emailAddress: email }
  put(poc, 'contactType', hdr.pocContactType() ?? undefined)
  put(poc, 'role', hdr.pocRole() ?? undefined)
  put(poc, 'phone', hdr.pocPhone() ?? undefined)
  put(poc, 'website', hdr.pocWebsite() ?? undefined)

  // The address is deliberately free-form -- `metadata.schema.json` types it
  // as a bare object with no `properties` and no `required` -- and an empty
  // stored string counts as nothing to write, so it is filtered out rather
  // than emitted as "". `postcode`, not `postalCode`: that is the key the
  // reference writes and the spec's own examples use.
  const address: Record<string, string> = {}
  const addressFields: [string, string | null][] = [
    ['thoroughfareNumber', hdr.pocAddressThoroughfareNumber()],
    ['thoroughfareName', hdr.pocAddressThoroughfareName()],
    ['locality', hdr.pocAddressLocality()],
    ['postcode', hdr.pocAddressPostcode()],
    ['country', hdr.pocAddressCountry()],
  ]
  for (const [key, value] of addressFields) {
    if (value !== null && value.length > 0) address[key] = value
  }
  if (Object.keys(address).length > 0) poc.address = address

  return poc
}

/** `metadata.referenceSystem`, as the OGC Name Type Specification URL
 *  `{base}/{authority}/{version}/{code}`.
 *
 *  Built from the three stored fields directly, NOT from `FileInfo`'s
 *  `EPSG:7415` short form: that form drops `version`, and it is `undefined`
 *  when `code` is 0, whereas the reference emits a URL whenever a
 *  `reference_system` table is stored at all
 *  (`CjReferenceSystem::new(None, authority, version, code)`,
 *  deserializer.rs:257-264). The C++ reader reconstructs the URL from the
 *  short form and so hard-codes `/0/` for the version; this does not. */
const CRS_BASE_URL = 'https://www.opengis.net/def/crs'

function referenceSystem(hdr: FbHeader): string | undefined {
  const rs = hdr.referenceSystem()
  if (rs === null) return undefined
  return `${CRS_BASE_URL}/${rs.authority() ?? ''}/${rs.version()}/${rs.code()}`
}

function extensions(hdr: FbHeader): Record<string, { url: string; version: string }> | undefined {
  const out: Record<string, { url: string; version: string }> = {}
  for (let i = 0; i < hdr.extensionsLength(); i++) {
    const e = hdr.extensions(i, new Extension())
    if (e === null) continue
    const name = e.name()
    // Keyed by name; an extension without one has nowhere to go.
    if (name === null) continue
    out[name] = { url: e.url() ?? '', version: e.version() ?? '' }
  }
  return Object.keys(out).length > 0 ? out : undefined
}

/** Geometry templates: shapes shared by every `GeometryInstance` in the file,
 *  with their own vertex list. Emitted only when BOTH vectors are stored -- a
 *  template list without vertices indexes nothing. */
function geometryTemplates(
  header: HeaderView, policy: Required<Int64Policy>['int64'],
): GeometryTemplates | undefined {
  const hdr = header.raw
  if (!fieldPresent(hdr, HEADER_TEMPLATES_SLOT)) return undefined
  if (!fieldPresent(hdr, HEADER_TEMPLATES_VERTICES_SLOT)) return undefined

  const semanticColumns = semanticColumnsOf(header)
  const templates: Geometry[] = []
  for (let i = 0; i < hdr.templatesLength(); i++) {
    const g = hdr.templates(i, new FbGeometry())
    if (g !== null) templates.push(geometryToJson(g, semanticColumns, policy))
  }

  // Template vertices are ABSOLUTE doubles: the header transform does not
  // apply to them, unlike a feature's quantised integer vertices.
  const vertices: [number, number, number][] = []
  for (let i = 0; i < hdr.templatesVerticesLength(); i++) {
    const v = hdr.templatesVertices(i, new DoubleVertex())
    if (v === null) continue
    vertices.push([v.x(), v.y(), v.z()])
  }

  return { templates, 'vertices-templates': vertices }
}

/** The first line of a CityJSONSeq stream: the document's metadata, an empty
 *  `CityObjects` map and an empty `vertices` array.
 *
 *  This is what carries `transform` -- the `scale`/`translate` that maps a
 *  feature's quantised integer vertices back to world coordinates -- plus
 *  `version`, `metadata`, and, when the file has them, `geometry-templates`
 *  and `extensions`. Pure computation over an already-parsed header: it issues
 *  no reads and can be called any number of times.
 *
 *  The header's own `appearance` table IS emitted here. It used to be dropped,
 *  on the since disproved grounds that the reference did not emit it either
 *  and that each feature carried the slice of the palette it used; in
 *  `geom_temp` the header and per-feature palettes are in fact disjoint, and
 *  the header's geometry templates index the header palette specifically.
 *  Dropping it left those template mappings dangling -- upstream finding #31.
 *
 *  @param header `reader.header`.
 *  @param opts how to spell `Long`/`ULong` attribute values; see
 *  {@link Int64Policy}. Defaults to `'lossy-number'`. */
export function toCityJSONMetadata(header: HeaderView, opts?: Int64Policy): CityJSON {
  const policy = opts?.int64 ?? 'lossy-number'
  const hdr = header.raw
  const info = header.info

  // An absent `transform` is CityJSON's identity-ish default, not zeros:
  // `Transform::new()` is scale [1,1,1], translate [0,0,0]. Defaulting scale
  // to zeros instead would collapse every coordinate onto the origin.
  const transform = info.hasTransform
    ? { scale: info.scale!, translate: info.translate! }
    : { scale: [1, 1, 1] as [number, number, number], translate: [0, 0, 0] as [number, number, number] }

  // `metadata` is emitted unconditionally, and `geographicalExtent` inside it
  // always -- the reference builds it with `.unwrap_or_default()`, so a header
  // with no extent yields six zeros rather than no member
  // (deserializer.rs:287-297, and degenerate_extent.expected.jsonl pins the
  // all-zero form).
  const metadata: Metadata = {
    geographicalExtent: info.geographicalExtent ?? [0, 0, 0, 0, 0, 0],
  }
  put(metadata, 'identifier', info.identifier)
  put(metadata, 'pointOfContact', pointOfContact(hdr))
  put(metadata, 'referenceDate', hdr.referenceDate() ?? undefined)
  put(metadata, 'referenceSystem', referenceSystem(hdr))
  put(metadata, 'title', info.title)

  const cj: CityJSON = {
    type: 'CityJSON',
    version: info.version,
    transform,
    CityObjects: {},
    vertices: [],
    metadata,
  }
  // The header's own appearance palette. The geometry templates below index
  // straight into it -- a template belongs to no feature, so its
  // `material`/`texture` mapping can refer to nothing else. Emitting the
  // templates while dropping this left those mappings dangling: upstream
  // finding #31.
  const headerAppearance = hdr.appearance(new FbAppearance())
  put(cj, 'appearance', headerAppearance !== null ? appearanceToJson(headerAppearance) : undefined)
  put(cj, 'geometry-templates', geometryTemplates(header, policy))
  put(cj, 'extensions', extensions(hdr))
  return cj
}

/** One CityJSONFeature line: every CityObject the feature holds, keyed by id,
 *  with its attributes decoded, its geometry boundaries re-nested to the depth
 *  the geometry type implies, and its semantics / material / texture index
 *  arrays rebuilt (a stored `u32::MAX` becomes `null`).
 *
 *  `vertices` stays QUANTISED: they are the integers as stored, and the
 *  `transform` from {@link toCityJSONMetadata} is what maps them to world
 *  coordinates. The reference reader does not apply it here either, and the
 *  conformance corpus pins that.
 *
 *  Pure computation over an already-read `Feature`; it issues no reads.
 *
 *  @param feature one feature from a cursor -- `reader.select()` or
 *  `reader.selectAll()`.
 *  @param header the header of the SAME file the feature came from; it
 *  supplies the fallback attribute schema and the semantic-surface schema.
 *  @param opts how to spell `Long`/`ULong` attribute values; see
 *  {@link Int64Policy}. Defaults to `'lossy-number'`. */
export function toCityJSONFeature(
  feature: Feature, header: HeaderView, opts?: Int64Policy,
): CityJSONFeature {
  const policy = opts?.int64 ?? 'lossy-number'
  const headerHasColumns = fieldPresent(header.raw, HEADER_COLUMNS_SLOT)
  const semanticColumns = semanticColumnsOf(header)

  const objects: Record<string, CityObject> = {}
  for (const view of feature.cityObjects()) {
    objects[view.id] = cityObjectToJson(view, headerHasColumns, semanticColumns, policy)
  }

  // Vertices stay QUANTISED integers; the header transform is what maps them
  // back to world coordinates, and the reference does not apply it here.
  const flat = feature.vertices()
  const vertices: [number, number, number][] = []
  for (let i = 0; i < flat.length; i += 3) {
    vertices.push([flat[i]!, flat[i + 1]!, flat[i + 2]!])
  }

  const out: CityJSONFeature = {
    type: 'CityJSONFeature',
    id: feature.id,
    CityObjects: objects,
    vertices,
  }

  const appearance = feature.rawFeature().appearance(new FbAppearance())
  if (appearance !== null) out.appearance = appearanceToJson(appearance)
  return out
}
