/* tslint:disable */
/* eslint-disable */
export class AsyncFeatureIter {
  private constructor();
  free(): void;
  header(): any;
  /**
   * Number of selected features (might be unknown)
   */
  features_count(): number | undefined;
  /**
   * Read next feature
   */
  next(): Promise<any | undefined>;
  cur_cj_feature(): any;
}
/**
 * FlatCityBuf dataset HTTP reader
 */
export class HttpFcbReader {
  free(): void;
  constructor(url: string);
  header(): any;
  /**
   * Select all features.
   */
  select_all(): Promise<AsyncFeatureIter>;
  /**
   * Select features within a bounding box.
   */
  select_bbox(min_x: number, min_y: number, max_x: number, max_y: number): Promise<AsyncFeatureIter>;
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
  readonly memory: WebAssembly.Memory;
  readonly __wbg_httpfcbreader_free: (a: number, b: number) => void;
  readonly __wbg_asyncfeatureiter_free: (a: number, b: number) => void;
  readonly httpfcbreader_new: (a: number, b: number) => any;
  readonly httpfcbreader_header: (a: number) => [number, number, number];
  readonly httpfcbreader_select_all: (a: number) => any;
  readonly httpfcbreader_select_bbox: (a: number, b: number, c: number, d: number, e: number) => any;
  readonly asyncfeatureiter_header: (a: number) => [number, number, number];
  readonly asyncfeatureiter_features_count: (a: number) => number;
  readonly asyncfeatureiter_next: (a: number) => any;
  readonly asyncfeatureiter_cur_cj_feature: (a: number) => [number, number, number];
  readonly __wbindgen_malloc: (a: number, b: number) => number;
  readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
  readonly __wbindgen_exn_store: (a: number) => void;
  readonly __externref_table_alloc: () => number;
  readonly __wbindgen_export_4: WebAssembly.Table;
  readonly __wbindgen_free: (a: number, b: number, c: number) => void;
  readonly __wbindgen_export_6: WebAssembly.Table;
  readonly __externref_table_dealloc: (a: number) => void;
  readonly closure283_externref_shim: (a: number, b: number, c: any) => void;
  readonly closure384_externref_shim: (a: number, b: number, c: any, d: any) => void;
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
