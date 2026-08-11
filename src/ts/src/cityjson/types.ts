/** The CityJSON / CityJSONSeq document shapes this reader emits -- the
 *  TypeScript spelling of the `cjseq` types the Rust reader builds
 *  (`cjseq::CityJSON`, `cjseq::CityJSONFeature` and friends), which are what
 *  produced every `.expected.jsonl` in `conformance/`.
 *
 *  Every member typed `?:` here is one serde drops with
 *  `skip_serializing_if = "Option::is_none"`, so an absent member must be
 *  ABSENT from the emitted object and not present-and-`undefined`: the
 *  conformance suite compares whole parsed lines, and `JSON.stringify` drops
 *  `undefined` members but `toEqual` does not distinguish them. This module
 *  therefore never assigns `undefined` -- see `index.ts`'s `put`. */

/** A CityJSON value: whatever survives `JSON.parse`. */
export type JsonValue =
  | null
  | boolean
  | number
  | string
  | JsonValue[]
  | { [k: string]: JsonValue }

/** `boundaries`, at whatever depth the geometry type implies. */
export type Boundaries = number | Boundaries[]

/** `semantics.values` / `material.values` / `texture.values`: indices nested
 *  to the geometry's depth, `null` wherever the wire held u32::MAX. */
export type IndexValues = number | null | IndexValues[]

export interface Transform {
  scale: [number, number, number]
  translate: [number, number, number]
}

/** `metadata.pointOfContact` (CityJSON section 5.3). `contactName` and
 *  `emailAddress` are the schema's only required members. */
export interface PointOfContact {
  contactName: string
  contactType?: string
  role?: string
  phone?: string
  emailAddress: string
  website?: string
  /** Deliberately free-form: `metadata.schema.json` types `address` as a bare
   *  object with no `properties` and no `required`. */
  address?: Record<string, string>
}

export interface Metadata {
  geographicalExtent?: [number, number, number, number, number, number]
  identifier?: string
  pointOfContact?: PointOfContact
  referenceDate?: string
  /** An OGC Name Type Specification URL, not the `EPSG:7415` short form. */
  referenceSystem?: string
  title?: string
}

export interface SemanticSurface {
  type: string
  parent?: number
  children?: number[]
  /** A semantic surface may carry arbitrary further members (`slope`,
   *  `direction`, ...); they come from its attribute blob. */
  [key: string]: JsonValue | undefined
}

export interface Semantics {
  surfaces: SemanticSurface[]
  /** Required by the schema and permitted to be null, so it is always emitted
   *  -- `null` when the mapping carried no values vector at all. */
  values: IndexValues[] | null
}

/** One theme's material assignment. Exactly one of `value` / `values` is
 *  normally present; `values` may itself be `null`. */
export interface MaterialReference {
  value?: number
  values?: IndexValues[] | null
}

/** One theme's texture assignment. `{}` is legal CityJSON: the per-theme
 *  texture object carries no `required` keyword at all. */
export interface TextureReference {
  values?: IndexValues[]
}

export interface Geometry {
  type: string
  lod?: string
  boundaries: Boundaries[]
  semantics?: Semantics
  material?: Record<string, MaterialReference>
  texture?: Record<string, TextureReference>
  /** GeometryInstance only. */
  template?: number
  transformationMatrix?: number[]
}

export interface CityObject {
  type: string
  geographicalExtent?: [number, number, number, number, number, number]
  attributes?: Record<string, JsonValue>
  geometry?: Geometry[]
  children?: string[]
  /** CityObjectGroup only; an unspecified role is `null`, not absent. */
  childrenRoles?: (string | null)[]
  parents?: string[]
}

export interface MaterialObject {
  name: string
  ambientIntensity?: number
  diffuseColor?: [number, number, number]
  emissiveColor?: [number, number, number]
  specularColor?: [number, number, number]
  shininess?: number
  transparency?: number
  isSmooth?: boolean
}

export interface TextureObject {
  type?: string
  image?: string
  wrapMode?: string
  textureType?: string
  borderColor?: number[]
}

export interface Appearance {
  materials?: MaterialObject[]
  textures?: TextureObject[]
  'vertices-texture'?: [number, number][]
  'default-theme-texture'?: string
  'default-theme-material'?: string
}

export interface GeometryTemplates {
  templates: Geometry[]
  'vertices-templates': [number, number, number][]
}

/** The first line of a CityJSONSeq stream: everything but the features. */
export interface CityJSON {
  type: 'CityJSON'
  version: string
  transform: Transform
  CityObjects: Record<string, CityObject>
  vertices: number[][]
  metadata?: Metadata
  /** The document-level material/texture palette. The `material`/`texture`
   *  mappings inside `geometry-templates` index into THIS palette, not into
   *  any feature's -- a template belongs to no feature. */
  appearance?: Appearance
  'geometry-templates'?: GeometryTemplates
  /** `name -> { url, version }`, emitted only when at least one extension is
   *  named. */
  extensions?: Record<string, { url: string; version: string }>
}

/** Every subsequent line. */
export interface CityJSONFeature {
  type: 'CityJSONFeature'
  id: string
  CityObjects: Record<string, CityObject>
  vertices: [number, number, number][]
  appearance?: Appearance
}
