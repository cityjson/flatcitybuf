# C++ examples

Nine self-contained programs, one per capability. Every command below was run
against `examples/data/delft.fcb` (1115 features, EPSG:7415) and the output is
what it actually printed.

## Build

```bash
cd src/cpp
just build                       # -> build-native/fcb_*
```

`fcb_read_http` additionally needs the libcurl adapter, which lives in its own
build tree so the default build stays curl-free and TLS-free:

```bash
cmake -B build-curl -S . -DFCB_WITH_CURL=ON -DFCB_BUILD_TESTS=OFF
cmake --build build-curl         # -> build-curl/fcb_read_http
```

## The examples

| Binary | Source | Shows |
|---|---|---|
| `fcb_inspect_header` | `inspect_header.cpp` | Header only: extent, CRS, transform, and **which columns are queryable** |
| `fcb_read_local` | `read_local.cpp` | Whole file (or a bbox) out as CityJSONSeq |
| `fcb_to_cityjson` | `to_cityjson.cpp` | The CityJSON JSON representation, and **how to reach into its fields** |
| `fcb_query_attributes` | `query_attributes.cpp` | Attribute queries through the static B+tree |
| `fcb_read_features` | `read_features.cpp` | Raw feature access, no CityJSON conversion |
| `fcb_custom_reader` | `custom_reader.cpp` | Implementing `fcb::RangeReader` yourself |
| `fcb_read_http` | `read_http.cpp` | Remote reads over HTTP range requests |
| `fcb_geometry_analysis` | `geometry_analysis.cpp` | **Walking the encoded geometry directly, for analysis** |
| `fcb_write_cityjson` | `write_cityjson.cpp` | **Writing** a CityJSONSeq out as `.fcb` |

Start with `fcb_inspect_header` on an unfamiliar file. If you just want to know
how to pull values out of the data, jump to `fcb_to_cityjson`.

---

### `fcb_inspect_header <file.fcb>`

```
$ ./build-native/fcb_inspect_header ../../examples/data/delft.fcb
features      1115
CityJSON      2.0
CRS           EPSG:7415
extent        [84501.555 445805.031 -3.747] .. [85675.234 446983.469 95.042]
transform     scale [0.001 0.001 0.001] translate [85088.391 446394.250 45.648]
R-tree        yes (node size 16)

columns (44; * = queryable via select_attr)
  * b3_dak_type                        String
  * b3_h_dak_50p                       Double
  ...
44 of 44 columns are queryable
```

The `*` matters: `select_attr` only answers on columns that were given a B+tree
at write time. Everything else is readable but not queryable.

### `fcb_read_local <file.fcb> [minx miny maxx maxy]`

CityJSONSeq on stdout — one metadata line, then one `CityJSONFeature` per line.
Progress goes to stderr, so redirecting stdout gives you a clean file.

```
$ ./build-native/fcb_read_local ../../examples/data/delft.fcb > delft.jsonl
1115 features, CityJSON 2.0, EPSG:7415
$ wc -l < delft.jsonl
1116                                    # 1 metadata + 1115 features

$ ./build-native/fcb_read_local ../../examples/data/delft.fcb \
      84500 445800 85000 446500 | wc -l
171                                     # 1 metadata + 170 features
```

### `fcb_to_cityjson <file.fcb> [feature-index]`

The one to read if the question is *"how do I get at the data?"*. It calls the
two conversion entry points —

```cpp
nlohmann::json meta = fcb::to_cityjson_metadata(reader.header());
nlohmann::json feat = fcb::to_cityjson_feature(iter.current(), reader.header());
```

— and then navigates the resulting JSON tree: the feature id, each
CityObject's `type` and `attributes`, a geometry's `type`/`lod`/`boundaries`,
and the vertices.

```
$ ./build-native/fcb_to_cityjson ../../examples/data/delft.fcb 0
== metadata (to_cityjson_metadata) ==
  version   2.0
  CRS       https://www.opengis.net/def/crs/EPSG/0/7415
  scale     [0.001, 0.001, 0.001]
  translate [85088.391, 446394.250, 45.648]

== feature (to_cityjson_feature) ==
  id            NL.IMBAG.Pand.0503100000031902
  CityObjects   2
  - NL.IMBAG.Pand.0503100000031902     type=Building
  - NL.IMBAG.Pand.0503100000031902-0   type=BuildingPart

  attributes of NL.IMBAG.Pand.0503100000031902 (43):
    status               Pand in gebruik
    b3_h_dak_50p         11.26
    b3_kas_warenhuis     false

  geometry[0]: type=MultiSurface lod=0
  vertices      2265 (quantized integers)
    vertices[0]  raw [468999, -303852, -46777]  ->  real [85557.390, 446090.398, -1.129]
```

It then dumps the whole feature as pretty-printed JSON, so you can see every
field the lines above reached into.

Two things the example makes concrete, because both trip people up:

- **Field access is `nlohmann::json`, not a bespoke API.** Use `.at("k")` (throws
  if absent), `.value("k", default)` (safe for optional fields), `.contains("k")`
  to guard, and `.get<double>()` / `.get<std::string>()` to pull a typed value.
  Attribute *values* keep their CityJSON types — a number stays a number, a bool
  a bool — so read them as such.
- **Vertices are quantized integers.** `feature["vertices"]` holds `[i, j, k]`
  integer triples; the real coordinate is `v[n] * transform.scale[n] +
  transform.translate[n]`. The `transform` lives on the **metadata** object, not
  the feature — which is why the example reads both.

### `fcb_query_attributes <file.fcb> <field> <op> <value> [field op value]...`

`op` is one of `eq ne gt ge lt le`. Extra triples are AND-intersected. Matching
feature ids go to stdout.

```
$ ./build-native/fcb_query_attributes ../../examples/data/delft.fcb b3_h_dak_50p gt 20
condition: b3_h_dak_50p gt 20 (column type Double)
4 of 1115 features matched

$ ./build-native/fcb_query_attributes ../../examples/data/delft.fcb \
      b3_h_dak_50p gt 20 b3_dak_type eq slanted
1 of 1115 features matched
```

The comparison value is a typed `KeyValue`, and **its type must match the
column's type on disk** — a mismatch does not throw, it reinterprets bytes and
returns plausible garbage. The example therefore looks the column up in the
header and dispatches on its declared type rather than hardcoding a factory.

String columns are indexed on keys truncated to 50 bytes (100 for Json/Binary),
so the index returns *candidates*; the default `AttrQueryOptions` verify each
one against the fully decoded attribute. Pass `{true}` to skip that — faster,
and wrong for long strings.

### `fcb_read_features <file.fcb> [max-features]`

The lower-level path, when you want a few attributes and not a whole CityJSON
tree per feature.

```
$ ./build-native/fcb_read_features ../../examples/data/delft.fcb 1
feature NL.IMBAG.Pand.0503100000031902  (2 CityObjects)
  object NL.IMBAG.Pand.0503100000031902-0
    schema   header (44 columns)
    (attribute blob present but empty)
  object NL.IMBAG.Pand.0503100000031902
    extent   [85534.89 446016.75 -1.13] .. [85631.81 446132.19 14.00]
    schema   own (43 columns)
    b3_dak_type                  "slanted"
    b3_h_dak_50p                 11.260000228881836
    ... 38 more
1 feature(s) shown; 1 object(s) carried their own schema
```

Note the two objects use **different schemas** — one the header's 44 columns,
one its own 43. That is the normal case, not an edge case. Attribute blobs are
not self-delimiting (each value's width comes from its column type), so
decoding with the wrong schema silently yields garbage rather than an error.
Always prefer `object_columns(i)` when `object_has_columns(i)`.

### `fcb_custom_reader <file.fcb> [minx miny maxx maxy]`

Implements `fcb::RangeReader` over an in-memory buffer and counts what the
traversal actually asks for. `RangeReader` is the library's only IO seam —
implement it to read from an object store, an engine VFS, an mmap, or a
decrypting layer.

```
$ ./build-native/fcb_custom_reader ../../examples/data/delft.fcb
opened: 1115 features, 7666308 bytes, 1 request(s) so far
1115 feature(s); 7 read(s), 6954960 bytes (90.7% of the file)

$ ./build-native/fcb_custom_reader ../../examples/data/delft.fcb 84500 445800 85000 446500
opened: 1115 features, 7666308 bytes, 1 request(s) so far
170 feature(s); 4 read(s), 2426496 bytes (31.7% of the file)
```

That contrast is the format's whole argument: the bbox query touched 31.7% of
the bytes for 170 of 1115 features, and opening the file cost one read.

The interface is deliberately **synchronous** — batching, not asynchrony, is
the concurrency primitive. Override `read_batch()` to service many ranges at
once; a blocking interface is trivially wrapped by whatever threading model you
already have, whereas an imposed async runtime is not.

### `fcb_read_http <url> [minx miny maxx maxy]`

Needs the `build-curl` tree. Only the intersecting features are fetched, never
the whole file — the request count at the end is the evidence.

Against the published 3DBAG file (~68 GB, EPSG:28992), with a ~1 km bbox over
central Amsterdam:

```
$ ./build-curl/fcb_read_http \
    https://flatcitybuf.open3d.city/data/3dbag_all_index.fcb \
    120000 486000 121000 487000
10771547 features, CityJSON 2.0
opened in 2 HTTP request(s)
2762 feature(s) in the query bbox, 37 HTTP request(s)
```

Opening a 68 GB file cost **2 requests**; the bbox query, **37** — the point of
the format. **Always pass a bbox** for a file this size: with no bbox the
example queries the western half of the extent, which on a national dataset is
tens of GB of transfer.

The same binary works against the small local fixture via the test range
server (`just test-http` starts and stops it for you):

```
$ python3 ../../src/cpp/tests/range_server.py ../../examples/data > /tmp/p.txt &
$ ./build-curl/fcb_read_http "http://127.0.0.1:$(cat /tmp/p.txt)/delft.fcb"
1115 features, CityJSON 2.0
opened in 2 HTTP request(s)
931 feature(s) in the query bbox, 7 HTTP request(s)
```

> Older files at `flatcitybuf.open3d.city/data/` that predate the
> alignment fix (`540772a`) are rejected with `header failed FlatBuffers
> verification` — correct, since this reader re-enabled `check_alignment`.
> `3dbag_all_index.fcb` above has been re-serialized with the current writer,
> so it verifies. If you hit that error on another file, re-serialize it.

### `fcb_write_cityjson <input.jsonl> <output.fcb>`

Writes a CityJSONSeq out as `.fcb`, using this port's own writer
(`fcb::FcbWriter`) — no Rust toolchain involved. Every ordinary attribute
column found gets a B+tree index at branching factor 256 (mirrors the Rust
CLI's `-A`/`--index-all-attributes` default); semantic-surface attributes get
their own separate schema and are encoded, but not indexed — same as `-A`
alone in the CLI.

```
$ ./build-native/fcb_write_cityjson ../../examples/data/delft.city.jsonl delft2.fcb
../../examples/data/delft.city.jsonl -> delft2.fcb
  1115 feature(s)
  44 attribute column(s), each with a B+tree index:
    column 0    b3_bag_bag_overlap
    column 1    b3_dak_type
    ...
    column 43   b3_bouwlagen
  1 semantic-surface attribute column(s) (not indexed):
    on_footprint_edge

$ ./build-native/fcb_query_attributes delft2.fcb b3_h_dak_50p gt 20
condition: b3_h_dak_50p gt 20 (column type Double)
4 of 1115 features matched
```

Same 4 features `fcb_query_attributes` finds against the original
`delft.fcb` above — the rewritten file differs in exact byte size (this
example's schema-building pass is a close but not exact reproduction of the
Rust CLI's own, see the comment at the top of `write_cityjson.cpp`), but is
functionally identical, right down to a semantic-surface attribute
(`on_footprint_edge`) that an earlier version of this example silently
dropped by never building a semantic attribute schema for it.

`FcbWriter::add_feature` spools each feature's encoded bytes to a private
temp file rather than holding every encoded feature in memory at once, and
`write(std::ostream&)` streams the finished file straight to `out` — reading
each feature's bytes back from the spool in fixed-size chunks rather than
materializing the whole feature section, let alone the whole file, as one
buffer. (The `write()` overload returning a `std::vector<std::uint8_t>` is a
convenience for small files and tests; it does not have this property, since
returning the complete bytes by value requires holding them all at once —
use `write(out)` for anything where output size matters.) `FcbWriter`'s
output is validated against real Rust-written files byte-for-byte, not just
by decoding correctly (`src/cpp/tests/test_writer_oracle.cpp`).

## Not covered here

A CLI. `fcb_write_cityjson` above is a minimal demonstration, not a
full-featured conversion tool (no bbox filter, no branching-factor override,
no `--no-spatial-index` equivalent) — for that, the Rust CLI (`cd src/rust &&
just ser <input.jsonl> <output.fcb>`) already covers the same ground this
library exposes, and more.

### `fcb_geometry_analysis <file.fcb> [max-features] [lod]`

```
$ ./build-native/fcb_geometry_analysis ../../examples/data/delft.fcb 20
surface area at lod 2.2 over 20 feature(s), m^2
  RoofSurface          42079.63
  GroundSurface        36770.26
  WallSurface          81615.17
  TOTAL               160465.06

flat walk vs nested CityJSON: AGREE
GroundSurface vs the dataset's own b3_opp_grond: 36770.26 vs 36772.37 m^2 (0.006% apart)
```

Every other example converts to CityJSON first. This one does not: it reads the
format's **own** representation -- five flat count arrays plus a flat
vertex-index list -- and computes over them. That is the representation to use
for analysis, because nothing has to be nested, allocated, or turned into JSON
to get a number out of it.

| array | meaning |
|---|---|
| `solids[i]` | shell count of solid i |
| `shells[i]` | surface count of shell i |
| `surfaces[i]` | ring count of surface i |
| `strings[i]` | vertex count of ring i |
| `boundaries` | the flat vertex-index list |
| `semantics[i]` | semantic-object index of surface i (`u32::MAX` = none) |

**The nesting depth comes from the geometry's `type`, never from the arrays.**
A `Solid` with one shell and a `MultiSolid` with one solid flatten to
byte-identical arrays -- only the type tells them apart. Inferring depth from
which array is populated is upstream finding #8. This example never needs the
depth at all: surface areas sum the same however the surfaces are grouped.
Anything that *does* care about grouping (per-shell volume, say) must switch on
the type.

Vertices are quantised integers shared by the whole feature: multiply by
`transform.scale` and add `transform.translate` for real-world coordinates.

Two things make the output trustworthy rather than merely plausible.

**The reader check** is the flat walk against the nested CityJSON path, compared
per feature. These must agree exactly -- they are two routes through the same
bytes, and any disagreement is a bug in one of them.

**The data sanity check** is the computed `GroundSurface` area against
`b3_opp_grond`, 3DBAG's own published ground area -- a number this library did
not produce. Over the first 20 features they agree to 0.006%. Over 500, 495 of
them still agree to within 1%, but the totals drift to ~4%: a handful of large
multi-part buildings differ materially, because `b3_opp_grond` was derived from
the source geometry by a different pipeline. That is a property of the dataset,
not of the walk -- the flat and nested paths agree exactly on those same
buildings.

Pass a different LoD to see the geometry change: at lod 1.2 and 1.3 the roof
area *equals* the footprint (a flat extrusion), while at lod 2.2 it exceeds it
(sloped roofs). All three implementations -- C++, Python and TypeScript --
produce identical totals for every LoD.
