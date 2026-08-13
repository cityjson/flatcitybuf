# @cityjson/flatcitybuf

A pure TypeScript **reader** for [FlatCityBuf](https://github.com/cityjson/flatcitybuf),
a cloud-optimized binary encoding of [CityJSON](https://www.cityjson.org/). It
reads a `.fcb` file from a URL over HTTP range requests, from a `Blob`/`File`
in the browser, from a local path in Node, or from an in-memory `Uint8Array`,
and answers spatial and attribute queries by fetching only the index and the
matching features. No WebAssembly, one runtime dependency (`flatbuffers`), the
same code in Node and the browser.

**Status: reading only — there is no writer here** (only the Rust and C++
implementations produce `.fcb` files) — and this reader is less settled than
those two. It passes all 14 shared conformance cases against the Rust reader's
own output, but JavaScript has no FlatBuffers verifier, so **input files must be
trusted**: framing is bounds-checked, the tables inside are not. Attribute
queries also carry three deliberate divergences from the Rust reader (a
fourth, `Byte` signedness, was resolved upstream). All of it
is spelled out, with citations, in the
**[TypeScript guide](https://github.com/cityjson/flatcitybuf/blob/main/docs/ts.md)**.

**Requires** Node ≥ 22.12 for the Node entry point, and is **ESM only** — import
it, do not `require` it.

## Install

```sh
npm install @cityjson/flatcitybuf
```

## Example

```ts
import { FcbReader, toCityJSONFeature } from '@cityjson/flatcitybuf'

const reader = await FcbReader.fromUrl('https://example.com/city.fcb')

const hits = await reader.select({
  spatial: { kind: 'bbox', value: [minX, minY, maxX, maxY] },
  where: [{ field: 'b3_h_dak_50p', operator: 'Gt', value: 20 }],
  limit: 50,
})

console.log(hits.featuresCount) // total matches, not the page size
for await (const feature of hits) {
  console.log(feature.id, toCityJSONFeature(feature, reader.header))
}
```

Reading a local file in Node lives behind the separate `./node` subpath, so the
package root never imports `node:*` and stays usable in a browser — see
**Entry points** in the guide. Upgrading from the old WebAssembly binding? The
guide has the full API mapping.

## Documentation

- **[TypeScript guide](https://github.com/cityjson/flatcitybuf/blob/main/docs/ts.md)**
  — the canonical reference: full status, entry points, the query API, tooling
  and testing. Start here.
- **[Runnable examples](https://github.com/cityjson/flatcitybuf/blob/main/src/ts/examples/README.md)**
  — nine scripts, one per capability, each with its real output. They run and
  type-check as part of the test suite, so they cannot drift.
- [Format specification](https://github.com/cityjson/flatcitybuf/blob/main/docs/specification.md)
- [Testing guide](https://github.com/cityjson/flatcitybuf/blob/main/docs/TESTING.md)
- [Project README](https://github.com/cityjson/flatcitybuf/blob/main/README.md)
  — the format, and the Rust, C++ and Python implementations
- [Issue tracker](https://github.com/cityjson/flatcitybuf/issues)

## License

MIT. See [LICENSE](https://github.com/cityjson/flatcitybuf/blob/main/LICENSE).
