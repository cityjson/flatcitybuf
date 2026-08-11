# Python examples

Eight self-contained scripts, one per capability. Every command below was run
against `examples/data/delft.fcb` (1115 features, EPSG:7415) from `src/py`, and
the output is what it actually printed.

They are executed by `tests/test_examples.py` on every `just test`, so an API
change that breaks one breaks the suite rather than rotting silently.

## Run

```bash
cd src/py
just sync                        # once: creates the dev environment
uv run python examples/inspect_header.py ../../examples/data/delft.fcb
```

No build step — this is a pure-Python reader. `flatbuffers` is the only
required dependency; `numpy` is optional and only makes bulk decoding faster.

## The examples

| Script | Shows |
|---|---|
| `inspect_header.py` | Header only: extent, CRS, transform, and **which columns are queryable** |
| `read_local.py` | Whole file (or a bbox) out as CityJSONSeq |
| `to_cityjson.py` | The decoded CityJSON dicts, and how to reach into them |
| `query_attributes.py` | Attribute queries through the static B+tree |
| `read_features.py` | Raw feature access, and the **per-object attribute schema** |
| `custom_reader.py` | Implementing `RangeReader` over your own byte source |
| `geometry_analysis.py` | **Walking the encoded geometry directly, for analysis** |
| `read_http.py` | Byte-range reads over HTTP |

Start with `inspect_header.py` on an unfamiliar file: opening reads the header
and nothing else, so it costs one small read even on a 68 GB file.

### `inspect_header.py <file.fcb>`

```
$ uv run python examples/inspect_header.py ../../examples/data/delft.fcb
file          ../../examples/data/delft.fcb
features      1115
CityJSON      2.0
title         3DBAG
CRS           EPSG:7415
extent        [84501.555 445805.031 -3.747] .. [85675.234 446983.469 95.042]
transform     scale [0.001 0.001 0.001] translate [85088.391 446394.250 45.648]
R-tree        yes (node size 16)

columns (44; * = queryable via select_attr)
  * b3_bag_bag_overlap                 Double
  * b3_dak_type                        String
  ...
44 of 44 columns are queryable
semantic columns: 1
```

The `*` matters: `select_attr` only answers on columns that were given a B+tree
at write time. Everything else is readable but not queryable.

### `read_local.py <file.fcb> [minx miny maxx maxy]`

```
$ uv run python examples/read_local.py ../../examples/data/delft.fcb > delft.jsonl
1115 features, CityJSON 2.0, EPSG:7415
wrote 1115 feature(s)

$ uv run python examples/read_local.py ../../examples/data/delft.fcb \
      84500 445800 85000 446500 | wc -l
     171
```

171 = one CityJSON header line plus 170 features. The C++ reader's R-tree query
and the Rust writer's own bbox filter both return the same 170.

Progress goes to stderr, so stdout stays a clean CityJSONSeq stream you can
redirect straight into a file.

### `to_cityjson.py <file.fcb> [feature_index]`

`to_cityjson_metadata` and `to_cityjson_feature` return plain dicts, so
everything after them is ordinary dict access:

```
$ uv run python examples/to_cityjson.py ../../conformance/geom_temp.fcb 0
== metadata (to_cityjson_metadata) ==
  version   2.0
  scale     [0.001, 0.001, 0.001]
  translate [0.56, 0.64, 7.579]
  extent    [0.56, 0.64, 7.579, 12.64, 7.68, 9.103]
  templates 3
  palette   2 material(s), 2 texture(s)

== feature 0 (to_cityjson_feature) ==
  id        GMLID_0598627_75956_700
  vertices  85
```

`geom_temp.fcb` is the fixture whose header carries both geometry templates and
the appearance palette. They belong together: a template's `material`/`texture`
mapping indexes the **header's** palette, because a template belongs to no
feature. Emitting one without the other leaves dangling indices.

### `query_attributes.py <file.fcb> <field> <op> <value> [field op value]...`

```
$ uv run python examples/query_attributes.py ../../examples/data/delft.fcb \
      b3_h_dak_50p gt 20
condition: b3_h_dak_50p gt 20 (column type Double)
4 of 1115 features matched

$ uv run python examples/query_attributes.py ../../examples/data/delft.fcb \
      b3_h_dak_50p gt 20 b3_dak_type eq slanted
condition: b3_h_dak_50p gt 20 (column type Double)
condition: b3_dak_type eq slanted (column type String)
1 of 1115 features matched
  NL.IMBAG.Pand.0503100000032914
```

Operators: `eq ne gt ge lt le`. Several conditions are ANDed.

The comparison value is a typed `KeyValue`, and **its type must match the
column's type on disk** — a mismatch does not throw, it reinterprets bytes and
returns plausible garbage. So the example looks the column up in the header and
builds the `KeyValue` from its declared type rather than guessing from how the
argument looks.

### `read_features.py <file.fcb> [count]`

```
$ uv run python examples/read_features.py ../../examples/data/delft.fcb 1
feature NL.IMBAG.Pand.0503100000031902  (2 CityObjects)
  object NL.IMBAG.Pand.0503100000031902-0
    schema   header (44 columns)
    (attribute blob present but empty)
  object NL.IMBAG.Pand.0503100000031902
    schema   own (43 columns)
    b3_bag_bag_overlap = 0.0
    b3_dak_type = 'slanted'
    ...

1 feature(s) shown; 1 object(s) carried their own schema
```

The thing most easily got wrong: **attribute schemas are per object**. A
CityObject with its own `columns` overrides the header's, and that is the
normal case. The check is on *presence*, not emptiness — an object declaring an
empty column list still overrides. Attribute blobs are not self-delimiting, so
the wrong schema yields plausible garbage, not an error.

### `custom_reader.py <file.fcb> [minx miny maxx maxy]`

```
$ uv run python examples/custom_reader.py ../../examples/data/delft.fcb \
      84500 445800 85000 446500
opened: 1115 features, 7666308 bytes on disk, 1 read(s) so far
  NL.IMBAG.Pand.0503100000032946
  ...
170 hit(s): 2 read(s), 1310720 bytes (17.1% of the file)
```

`RangeReader` is a `Protocol` — two methods, no base class to inherit.
Implement it and every reader, index and query works over whatever transport
you have: S3, a database blob, an mmap, a test double.

Note what the numbers say: 170 of 1115 features cost 2 reads and 17% of the
file. The percentage is high here only because the buffering window (1 MiB) is
a large fraction of a 7.6 MB file; on a real cloud-sized file it is negligible,
which is what `read_http.py` shows.

### `read_http.py <url> [minx miny maxx maxy]`

```
$ uv run python examples/read_http.py \
    https://flatcitybuf.open3d.city/data/3dbag_all_index.fcb \
    120000 486000 121000 487000
10771547 features, CityJSON 2.0
opened in 2 HTTP request(s), 0.3s
2762 feature(s) in the query bbox, 10 HTTP request(s), 1.4s
decoded all 2762, 39 HTTP request(s) total, +7.8s
```

Opening a 68 GB file cost **2 requests**; the whole query, **39** — the point
of the format. The C++ reader spends 37 on the same query and the TypeScript
one 34.

**Always pass a bbox** on a file this size: with no bbox there is nothing to
narrow the scan, and the example refuses rather than pulling tens of GB.

### `geometry_analysis.py <file.fcb> [count] [lod]`

```
$ uv run python examples/geometry_analysis.py ../../examples/data/delft.fcb 20
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
