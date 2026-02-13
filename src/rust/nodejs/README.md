# FlatCityBuf Node.js Native Bindings

Native Node.js bindings for [FlatCityBuf](https://github.com/cityjson/flatcitybuf) — a binary format for CityJSON data with built-in spatial and attribute indexing.

Built with [napi-rs](https://napi.rs/) for near-native performance, these bindings provide HTTP range request support for reading remote FCB files without downloading them entirely.

## Installation

The bindings are part of the `@cityjson/flatcitybuf` package. When running in Node.js, the native bindings are automatically used instead of the WASM fallback.

```bash
npm install @cityjson/flatcitybuf
```

## Quick Start

```javascript
import { FcbReader, NodeSpatialQuery, NodeAttrQuery } from "@cityjson/flatcitybuf";

// Open a remote FCB file
const reader = await FcbReader.open(
  "https://storage.googleapis.com/flatcitybuf/delft.city.fcb"
);

console.log(`Features: ${reader.featuresCount}`);
console.log(`CityJSON version: ${reader.cityjson().version}`);

// Query by bounding box
const query = NodeSpatialQuery.bbox(84400, 447200, 84600, 447400);
const iter = await reader.selectSpatial(query);

let feature;
while ((feature = await iter.next()) !== null) {
  console.log(`Feature: ${feature.id}`);
}
```

## API Reference

### `FcbReader`

The main entry point for reading remote FCB files over HTTP.

#### `FcbReader.open(url: string): Promise<FcbReader>`

Factory method that opens a remote FCB file. Fetches the header and spatial index metadata via HTTP range requests.

```javascript
const reader = await FcbReader.open("https://example.com/city.fcb");
```

#### `reader.featuresCount: number`

Getter that returns the total number of features in the file.

#### `reader.cityjson(): object`

Returns CityJSON metadata (type, version, transform, CRS, metadata) extracted from the FCB header.

```javascript
const cj = reader.cityjson();
// { type: "CityJSON", version: "2.0", transform: { scale: [...], translate: [...] } }
```

#### `reader.selectAll(): Promise<FeatureIter>`

Select all features for iteration.

```javascript
const iter = await reader.selectAll();
const features = await iter.collect(); // get all at once
```

#### `reader.selectSpatial(query: NodeSpatialQuery): Promise<FeatureIter>`

Select features matching a spatial query (bounding box, point intersect, or nearest point).

#### `reader.selectSpatialPaged(query, limit?, offset?): Promise<FeatureIter>`

Select features matching a spatial query with pagination.

```javascript
const query = NodeSpatialQuery.bbox(84400, 447200, 84600, 447400);
const iter = await reader.selectSpatialPaged(query, 10, 0); // first 10
```

#### `reader.selectAttrQuery(query: NodeAttrQuery): Promise<FeatureIter>`

Select features matching an attribute query.

#### `reader.selectAttrQueryPaged(query, limit?, offset?): Promise<FeatureIter>`

Select features matching an attribute query with pagination.

### `FeatureIter`

Async iterator over selected features.

#### `iter.featuresCount(): number`

Returns the number of features matching the query.

#### `iter.next(): Promise<object | null>`

Returns the next CityJSONFeature object, or `null` when iteration is complete.

Each feature has the structure:
```javascript
{
  type: "CityJSONFeature",
  id: "NL.IMBAG.Pand.0503100000012869",
  CityObjects: { ... },
  vertices: [[...], ...]
}
```

#### `iter.collect(): Promise<object[]>`

Collects all remaining features into an array. Useful for small result sets.

```javascript
const features = await iter.collect();
console.log(`Got ${features.length} features`);
```

### `NodeSpatialQuery`

Factory class for creating spatial queries.

#### `NodeSpatialQuery.bbox(minX, minY, maxX, maxY): NodeSpatialQuery`

Create a bounding box query. Coordinates should be in the CRS of the FCB file.

```javascript
const query = NodeSpatialQuery.bbox(84227.77, 445377.33, 85323.23, 446334.69);
```

#### `NodeSpatialQuery.pointIntersects(x, y): NodeSpatialQuery`

Create a point intersection query — finds features whose bounding box contains the given point.

```javascript
const query = NodeSpatialQuery.pointIntersects(84700.0, 446000.0);
```

#### `NodeSpatialQuery.pointNearest(x, y): NodeSpatialQuery`

Create a nearest-point query — finds the feature closest to the given point.

```javascript
const query = NodeSpatialQuery.pointNearest(84700.0, 446000.0);
```

#### `query.queryType: string`

Returns the query type as a string: `"bbox"`, `"pointIntersects"`, or `"pointNearest"`.

### `NodeAttrQuery`

Query features by attribute values.

#### `new NodeAttrQuery(conditions: Array<[field, operator, value]>)`

Create an attribute query from an array of condition tuples.

- **field**: Attribute name (string)
- **operator**: One of `"Eq"`, `"Gt"`, `"Ge"`, `"Lt"`, `"Le"`, `"Ne"`
- **value**: Comparison value (string, number, or boolean)

```javascript
// Find a specific building by ID
const query = new NodeAttrQuery([
  ["identificatie", "Eq", "NL.IMBAG.Pand.0503100000012869"],
]);

// Find tall buildings
const query = new NodeAttrQuery([["b3_h_dak_50p", "Gt", 20.0]]);

// Multiple conditions (AND)
const query = new NodeAttrQuery([
  ["b3_h_dak_50p", "Gt", 10.0],
  ["b3_h_dak_50p", "Lt", 30.0],
]);
```

## Building from Source

Requires [Rust](https://rustup.rs/) and the napi-cli:

```bash
npm install -g @napi-rs/cli

# Debug build
just build-nodejs-dev

# Release build
just build-nodejs
```

## Running Tests

```bash
cd src/ts
node --test test/node.test.mjs
```

## Architecture

The bindings wrap `fcb_core`'s `HttpFcbReader` via napi-rs. Key design decisions:

- **HTTP range requests**: Only fetches the bytes needed for each query, enabling efficient access to large remote files.
- **Connection re-opening**: Each query method re-opens the HTTP connection because `HttpFcbReader::select_*()` consumes `self` (Rust ownership). Metadata is cached on `open()` to avoid redundant fetches.
- **Thread-safe iteration**: `FeatureIter` uses `Mutex<AsyncFeatureIter>` internally because napi-rs requires `&self` (not `&mut self`) for async methods.
- **Conditional exports**: When installed as `@cityjson/flatcitybuf`, Node.js automatically uses these native bindings while browsers use the WASM version.
