# C++ examples

Six self-contained programs, one per capability. Every command below was run
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
| `fcb_query_attributes` | `query_attributes.cpp` | Attribute queries through the static B+tree |
| `fcb_read_features` | `read_features.cpp` | Raw feature access, no CityJSON conversion |
| `fcb_custom_reader` | `custom_reader.cpp` | Implementing `fcb::RangeReader` yourself |
| `fcb_read_http` | `read_http.cpp` | Remote reads over HTTP range requests |

Start with `fcb_inspect_header` on an unfamiliar file.

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

### `fcb_read_http <url>`

Needs the `build-curl` tree. Only the intersecting features are fetched.

```
$ python3 ../../src/cpp/tests/range_server.py ../../examples/data > /tmp/p.txt &
$ ./build-curl/fcb_read_http "http://127.0.0.1:$(cat /tmp/p.txt)/delft.fcb"
1115 features, CityJSON 2.0
931 features in the western half, 9 HTTP requests
```

931 of 1115 features in **9 requests**. `just test-http` starts and stops that
server for you as part of the test run.

> The two public demo files at `storage.googleapis.com/flatcitybuf/` are
> rejected with `header failed FlatBuffers verification`. That is correct: they
> predate the alignment fix in `540772a`, and this reader re-enabled
> `check_alignment`. The Python and TypeScript readers accept them only because
> neither has a FlatBuffers verifier.

## Not covered here

Writing. Producing `.fcb` files requires the Rust CLI (`cd src/rust && just ser
<input.jsonl> <output.fcb>`); every other implementation is read-only.
