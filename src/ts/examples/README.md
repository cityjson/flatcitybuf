# TypeScript examples

Eight self-contained scripts, one per capability. Every command below was run
against `examples/data/delft.fcb` (1115 features, EPSG:7415) from `src/ts`, and
the output is what it actually printed.

They are executed by `test/examples.test.ts` on every `just test`, and
type-checked under `strict` by `just type`, so an API change breaks the suite
rather than rotting the docs silently.

## Run

```bash
cd src/ts
just install                     # once: npm ci
just build                       # -> dist/ (the examples import it)
node examples/inspect-header.ts ../../examples/data/delft.fcb
```

No bundler and no `ts-node`: these are `.ts` files run directly by Node, which
strips the types. That imposes one constraint — **strip-only TypeScript**, so
no `enum`, no `namespace`, and no constructor parameter properties (they emit
code rather than merely declaring types). Plain type annotations, interfaces,
generics and `as` casts are all fine.

The examples `import … from '@cityjson/flatcitybuf'` — the package's own name,
resolved through its `exports` map by Node's self-referencing. That is exactly
the line a consumer writes, which is why `just build` is a prerequisite: they
exercise `dist/`, not `src/`.

## The examples

| Script | Shows |
|---|---|
| `inspect-header.ts` | Header only: extent, CRS, transform, and **which columns are queryable** |
| `read-local.ts` | Whole file (or a bbox) out as CityJSONSeq |
| `to-cityjson.ts` | The decoded CityJSON objects, and how to reach into them |
| `query-attributes.ts` | Attribute queries through the static B+tree |
| `read-features.ts` | Raw feature access, and the **per-object attribute schema** |
| `custom-reader.ts` | Implementing `RangeReader`, and what buffering costs and saves |
| `read-http.ts` | Byte-range reads over HTTP |
| `int64-policy.ts` | **JS-only:** `Long`/`ULong` past `Number.MAX_SAFE_INTEGER` |

Start with `inspect-header.ts` on an unfamiliar file: opening reads the header
and nothing else, so it costs one small read even on a 68 GB file.

### `inspect-header.ts <file.fcb>`

```
$ node examples/inspect-header.ts ../../examples/data/delft.fcb
file          ../../examples/data/delft.fcb
features      1115
CityJSON      2.0
title         3DBAG
CRS           EPSG:7415
extent        [84501.555 445805.031 -3.747] .. [85675.234 446983.469 95.042]
transform     scale [0.001 0.001 0.001] translate [85088.391 446394.250 45.648]
R-tree        yes (node size 16)

columns (44; * = queryable via where)
  * b3_bag_bag_overlap                 Double
  * b3_dak_type                        String
  ...
44 of 44 columns are queryable
semantic columns: 1
```

The `*` matters: a `where` condition is only answerable on a column the writer
gave a B+tree. Everything else is readable but not queryable.

### `read-local.ts <file.fcb> [minX minY maxX maxY]`

```
$ node examples/read-local.ts ../../examples/data/delft.fcb > delft.jsonl
1115 features, CityJSON 2.0, EPSG:7415
wrote 1115 feature(s)

$ node examples/read-local.ts ../../examples/data/delft.fcb \
      84500 445800 85000 446500 | wc -l
     171
```

171 = one CityJSON header line plus 170 features — the same 170 the C++ and
Python readers and the Rust writer's own bbox filter all return.

Progress goes to stderr so stdout stays a clean CityJSONSeq stream.

### `to-cityjson.ts <file.fcb> [featureIndex]`

```
$ node examples/to-cityjson.ts ../../conformance/geom_temp.fcb 0
== metadata (toCityJSONMetadata) ==
  version   2.0
  scale     [0.001, 0.001, 0.001]
  translate [0.56, 0.64, 7.579]
  extent    [0.56, 0.64, 7.579, 12.64, 7.68, 9.103]
  templates 3
  palette   2 material(s), 2 texture(s)

== feature 0 (toCityJSONFeature) ==
  id        GMLID_0598627_75956_700
  vertices  85
```

`geom_temp.fcb` is the fixture whose header carries both geometry templates and
the appearance palette. They belong together: a template's `material`/`texture`
mapping indexes the **header's** palette, because a template belongs to no
feature. Emitting one without the other leaves dangling indices.

### `query-attributes.ts <file.fcb> <field> <op> <value>...`

```
$ node examples/query-attributes.ts ../../examples/data/delft.fcb \
      b3_h_dak_50p Gt 20 b3_dak_type Eq slanted
condition: b3_h_dak_50p Gt 20 (column type Double)
condition: b3_dak_type Eq slanted (column type String)
1 of 1115 features matched
  NL.IMBAG.Pand.0503100000032914
```

Operators: `Eq Ne Gt Ge Lt Le`. Conditions are ANDed, and matching is
**existential** over a feature's CityObjects — the feature matches if any one
of its objects does.

Unlike the C++ and Python ports, no typed key is constructed by hand: `value`
is a plain JS value and the reader coerces it to the column's own type,
throwing `InvalidArgument` *before any I/O* if it cannot. A string compared
against a `Double` is an error here rather than reinterpreted bytes.

### `read-features.ts <file.fcb> [count]`

```
$ node examples/read-features.ts ../../examples/data/delft.fcb 1
feature NL.IMBAG.Pand.0503100000031902  (2 CityObjects)
  object NL.IMBAG.Pand.0503100000031902-0
    schema   header (44 columns)
    (attribute blob present but empty)
  object NL.IMBAG.Pand.0503100000031902
    schema   own (43 columns)
    b3_bag_bag_overlap = 0
    b3_dak_type = "slanted"
    ...

1 feature(s) shown; 1 object(s) carried their own schema
```

The thing most easily got wrong: **attribute schemas are per object**. A
CityObject with its own `columns` overrides the header's, and that is the
normal case. The check is on *presence*, not emptiness — an object declaring an
empty column list still overrides. Attribute blobs are not self-delimiting, so
the wrong schema yields plausible garbage, not an error.

### `custom-reader.ts <file.fcb> [minX minY maxX maxY]`

```
$ node examples/custom-reader.ts ../../examples/data/delft.fcb \
      84500 445800 85000 446500
  NL.IMBAG.Pand.0503100000032946
  ...
raw     : 170 hit(s), 361 read(s), 1119896 bytes (14.6% of the file)
buffered: 170 hit(s), 3 read(s), 2164352 bytes (28.2% of the file)
```

`RangeReader` is a two-method interface — no base class to extend. Implement it
and every reader, index and query works over whatever transport you have.

The two rows are the point. `fromReader` uses the source **exactly as given**
and inserts no buffering, deliberately, so a request count stays honest and
tunable; a sequential read costs two reads per feature (a 4-byte size prefix,
then the body). Wrapping in `BufferedRangeReader` trades bytes for requests:
120x fewer reads, about twice the bytes. Over HTTP that trade is overwhelmingly
worth it, which is why `fromUrl` does it for you.

### `read-http.ts <url> [minX minY maxX maxY]`

```
$ node examples/read-http.ts \
    https://flatcitybuf.open3d.city/data/3dbag_all_index.fcb \
    120000 486000 121000 487000
10771547 features, CityJSON 2.0
opened in 1 HTTP request(s), 0.8s
2762 feature(s) in the query bbox, 5 HTTP request(s), 2.2s
decoded all 2762, 34 HTTP request(s) total, +11.2s
```

Opening a 68 GB file cost **1 request**; the whole query, **34** — the point of
the format. The C++ reader spends 37 on the same query and the Python one 39.

The reader takes any fetch-compatible function, which is also how you add auth
headers, retries or a cache.

**Always pass a bbox** on a file this size: with no bbox there is nothing to
narrow the scan, and the example refuses rather than pulling tens of GB.

### `int64-policy.ts <file.fcb>`

```
$ node examples/int64-policy.ts ../../conformance/inferable_types.fcb
== why the policy exists ==
  Number.MAX_SAFE_INTEGER  9007199254740991
  the i64 value            9007199254740993
  as a JS number           9007199254740992   <- a digit is gone
  round trip is lossless?  false

== this file ==
  Long/ULong columns: a_long, a_ulong
  lossy-number    a_long=-42 a_ulong=42
  decimal-string  a_long="-42" a_ulong="42"
```

This one has no counterpart in the C++ or Python examples, because the hazard is
JavaScript's alone: a JS `number` is a float64 carrying 53 bits of integer
precision, while `Long`/`ULong` columns are full 64-bit.

`toCityJSONFeature` takes an `Int64Policy`: `'lossy-number'` (the default, and
what makes whole-line comparison against the conformance oracle meaningful),
`'decimal-string'` (every digit, at the cost of changing the JSON type), or
`'error'` (throw rather than lose a digit silently). No policy ever leaks a
`bigint` into the emitted object.
