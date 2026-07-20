# WASM demo page

A hand-written page for exercising the WASM reader against a remote `.fcb`:
HTTP range reads, attribute and spatial queries, and the CityJSON -> OBJ and
CityJSONSeq -> CityJSON conversions.

The WASM package it imports is generated, not committed, so build it first:

```bash
just build-wasm                  # writes src/ts/fcb_wasm*.{js,wasm,d.ts}
python3 -m http.server 8080      # from the REPO ROOT, not this directory
```

Then open <http://localhost:8080/examples/wasm/>. The import path reaches out
to `src/ts/`, so serving this directory alone will 404.
