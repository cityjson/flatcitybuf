/* tslint:disable */
/* eslint-disable */

export class AsyncFeatureIter {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  cur_cj_feature(): any;
  /**
   * Number of selected features (might be unknown)
   */
  features_count(): number | undefined;
  /**
   * Read next feature
   */
  next(): Promise<any | undefined>;
  header(): any;
}

export class HttpFcbReader {
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Select all features.
   */
  select_all(): Promise<AsyncFeatureIter>;
  /**
   * Select features within a bounding box.
   */
  select_spatial(query: WasmSpatialQuery): Promise<AsyncFeatureIter>;
  select_attr_query(query: WasmAttrQuery): Promise<AsyncFeatureIter>;
  /**
   * Select features within a bounding box with optional pagination.
   * If `limit`/`offset` are provided, only a page of features is returned while
   * `features_count()` on the returned iterator still reflects the total number of matches.
   */
  select_spatial_paged(query: WasmSpatialQuery, limit?: number | null, offset?: number | null): Promise<AsyncFeatureIter>;
  /**
   * Attribute query with optional pagination.
   */
  select_attr_query_paged(query: WasmAttrQuery, limit?: number | null, offset?: number | null): Promise<AsyncFeatureIter>;
  constructor(url: string);
  meta(): any;
  cityjson(): any;
}

export class WasmAttrQuery {
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Creates a new WasmAttrQuery from a JS array of query tuples.
   *
   * Each query tuple must be an array of three elements:
   * [field: string, operator: string, value: number | boolean | string | Date]
   *
   * For example, in JavaScript you could pass:
   * `[ ["b3_h_dak_50p", "Gt", 2.0],
   *   ["identificatie", "Eq", "NL.IMBAG.Pand.0503100000012869"],
   *   ["created", "Ge", new Date("2020-01-01T00:00:00Z")] ]`
   */
  constructor(js_value: any);
  /**
   * Returns the inner AttrQuery as a JsValue (an array of query tuples)
   * useful for debugging.
   */
  readonly inner: any;
}

export class WasmSpatialQuery {
  free(): void;
  [Symbol.dispose](): void;
  constructor(js_value: any);
  to_js(): any;
  readonly query_type: string;
  readonly x: number | undefined;
  readonly y: number | undefined;
  readonly max_x: number | undefined;
  readonly max_y: number | undefined;
  readonly min_x: number | undefined;
  readonly min_y: number | undefined;
}

/**
 * Converts a CityJSON object or CityJSONSeq list to OBJ format.
 *
 * # Arguments
 *
 * * `city_json_js` - JsValue containing either:
 *   - A CityJSON object (for backward compatibility), or
 *   - An array where the first element is a CityJSON object and
 *     the rest are CityJSONFeature objects (CityJSONSeq format)
 *
 * # Returns
 *
 * A string containing the OBJ data or an error
 */
export function cjToObj(city_json_js: any): string;

export function cjseqToCj(base_cj: any, features: any): any;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
  readonly memory: WebAssembly.Memory;
  readonly __wbg_asyncfeatureiter_free: (a: number, b: number) => void;
  readonly __wbg_httpfcbreader_free: (a: number, b: number) => void;
  readonly __wbg_wasmattrquery_free: (a: number, b: number) => void;
  readonly __wbg_wasmspatialquery_free: (a: number, b: number) => void;
  readonly asyncfeatureiter_cur_cj_feature: (a: number) => [number, number, number];
  readonly asyncfeatureiter_features_count: (a: number) => number;
  readonly asyncfeatureiter_header: (a: number) => [number, number, number];
  readonly asyncfeatureiter_next: (a: number) => any;
  readonly httpfcbreader_cityjson: (a: number) => [number, number, number];
  readonly httpfcbreader_meta: (a: number) => [number, number, number];
  readonly httpfcbreader_new: (a: number, b: number) => any;
  readonly httpfcbreader_select_all: (a: number) => any;
  readonly httpfcbreader_select_attr_query: (a: number, b: number) => any;
  readonly httpfcbreader_select_attr_query_paged: (a: number, b: number, c: number, d: number) => any;
  readonly httpfcbreader_select_spatial: (a: number, b: number) => any;
  readonly httpfcbreader_select_spatial_paged: (a: number, b: number, c: number, d: number) => any;
  readonly wasmattrquery_inner: (a: number) => any;
  readonly wasmattrquery_new: (a: any) => [number, number, number];
  readonly wasmspatialquery_max_x: (a: number) => [number, number];
  readonly wasmspatialquery_max_y: (a: number) => [number, number];
  readonly wasmspatialquery_min_x: (a: number) => [number, number];
  readonly wasmspatialquery_min_y: (a: number) => [number, number];
  readonly wasmspatialquery_new: (a: any) => [number, number, number];
  readonly wasmspatialquery_query_type: (a: number) => [number, number];
  readonly wasmspatialquery_to_js: (a: number) => any;
  readonly wasmspatialquery_x: (a: number) => [number, number];
  readonly wasmspatialquery_y: (a: number) => [number, number];
  readonly cjToObj: (a: any) => [number, number, number, number];
  readonly cjseqToCj: (a: any, b: any) => [number, number, number];
  readonly wasm_bindgen__convert__closures_____invoke__h50c0d13c7f731cac: (a: number, b: number, c: any) => void;
  readonly wasm_bindgen__closure__destroy__h7f50e65214019144: (a: number, b: number) => void;
  readonly wasm_bindgen__convert__closures_____invoke__hf2e082c4661e9722: (a: number, b: number) => number;
  readonly wasm_bindgen__convert__closures_____invoke__h8a48821c6d1a93a2: (a: number, b: number, c: any, d: any) => void;
  readonly __wbindgen_malloc: (a: number, b: number) => number;
  readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
  readonly __wbindgen_exn_store: (a: number) => void;
  readonly __externref_table_alloc: () => number;
  readonly __wbindgen_externrefs: WebAssembly.Table;
  readonly __externref_table_dealloc: (a: number) => void;
  readonly __wbindgen_free: (a: number, b: number, c: number) => void;
  readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
* Instantiates the given `module`, which can either be bytes or
* a precompiled `WebAssembly.Module`.
*
* @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
*
* @returns {InitOutput}
*/
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
* If `module_or_path` is {RequestInfo} or {URL}, makes a request and
* for everything else, calls `WebAssembly.instantiate` directly.
*
* @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
*
* @returns {Promise<InitOutput>}
*/
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
