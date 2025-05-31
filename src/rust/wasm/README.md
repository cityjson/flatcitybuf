# FlatCityBuf WASM API

This package provides WebAssembly bindings for the FlatCityBuf library, allowing for efficient CityJSON processing in the browser.

## Features

- Read FlatCityBuf files via HTTP
- Query spatial and attribute data
- Convert CityJSON to OBJ format for 3D visualization

## Usage Examples

### Converting CityJSON to OBJ

```javascript
// Import the wasm module
import * as fcb from 'fcb_wasm';

// Example CityJSON object
const cityJsonObject = {
  "type": "CityJSON",
  "version": "1.1",
  "transform": {
    "scale": [1.0, 1.0, 1.0],
    "translate": [0.0, 0.0, 0.0]
  },
  "vertices": [
    [0, 0, 0],
    [1, 0, 0],
    [1, 1, 0],
    [0, 1, 0],
    [0, 0, 1],
    [1, 0, 1],
    [1, 1, 1],
    [0, 1, 1]
  ],
  "CityObjects": {
    "id-1": {
      "type": "Building",
      "geometry": [{
        "type": "Solid",
        "lod": "2",
        "boundaries": [
          // Cube faces as example
          [[[0, 1, 2, 3]]],  // bottom face
          [[[4, 5, 6, 7]]],  // top face
          [[[0, 1, 5, 4]]],  // front face
          [[[1, 2, 6, 5]]],  // right face
          [[[2, 3, 7, 6]]],  // back face
          [[[3, 0, 4, 7]]]   // left face
        ]
      }]
    }
  }
};

// Convert CityJSON to OBJ
const objContent = fcb.cjToObj(cityJsonObject);

// Create a blob and download link
const blob = new Blob([objContent], { type: 'text/plain' });
const url = URL.createObjectURL(blob);

// Create download link
const a = document.createElement('a');
a.href = url;
a.download = 'citymodel.obj';
a.textContent = 'Download OBJ';
document.body.appendChild(a);
```

The conversion process internally:

1. Takes your JavaScript CityJSON object
2. Deserializes it into a Rust CityJSON struct
3. Processes the 3D geometry to generate OBJ format
4. Returns the OBJ content as a string

### Reading FlatCityBuf via HTTP

```javascript
import * as fcb from 'fcb_wasm';

async function loadFcb() {
  // Create an HTTP FlatCityBuf reader
  const reader = await new fcb.HttpFcbReader("https://example.com/path/to/model.fcb");
  
  // Get CityJSON metadata
  const metadata = await reader.cityjson();
  console.log("CityJSON metadata:", metadata);
  
  // Select all features and iterate
  const iter = await reader.select_all();
  const count = iter.features_count();
  console.log(`Found ${count} features`);
  
  // Process features
  let feature;
  while ((feature = await iter.next()) !== null) {
    console.log("Feature:", feature);
  }
}

loadFcb().catch(console.error);
```

## API Reference

### OBJ Conversion

- `cjToObj(cityJsonObject)`: Converts a CityJSON object to OBJ format string. Expects a valid CityJSON object as input.

### FlatCityBuf Reading

- `HttpFcbReader`: Class for reading FlatCityBuf files over HTTP
- `WasmSpatialQuery`: Spatial query helper class
- `WasmAttrQuery`: Attribute query helper class
