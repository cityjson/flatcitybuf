# flatcitybuf

A from-scratch, pure-Python reader for
[FlatCityBuf](https://github.com/cityjson/flatcitybuf), a cloud-optimized
binary format for 3D city models: CityJSON's semantics in FlatBuffers, with a
packed Hilbert R-tree for spatial queries, a static B+tree for attribute
queries, and HTTP range requests so a client fetches only the bytes it needs.
No FFI and no compiled extension — a single `py3-none-any` wheel on CPython
3.9+, with `flatbuffers` as its only required dependency. Reader only: write
`.fcb` files with the Rust CLI or the C++ writer.

```bash
pip install flatcitybuf          # or: uv pip install flatcitybuf
pip install "flatcitybuf[numpy]" # optional: ~2.4x faster bulk decoding
```

```python
import json
import flatcitybuf as fcb

reader = fcb.FcbReader.open_file("city.fcb")

# The CityJSONSeq header line.
print(json.dumps(fcb.to_cityjson_metadata(reader.header)))

# Every feature, in stored (Hilbert) order.
for feature in reader.select_all():
    cj = fcb.to_cityjson_feature(feature, reader.header)

# Over HTTP, byte-range by byte-range (synchronously).
remote = fcb.FcbReader.open(fcb.HttpRangeReader("https://example.com/city.fcb"))
```

Attribute and spatial queries, the optional-numpy story, development commands,
and the migration notes for users of the retired PyO3 extension (0.2.0 and
earlier, whose API this does **not** drop-in replace) are all in the guide:

- **[Python guide](https://github.com/cityjson/flatcitybuf/blob/main/docs/py.md)**
  — install, full API tour, tooling and testing.
- [Format specification](https://github.com/cityjson/flatcitybuf/blob/main/docs/specification.md)
- [Project README](https://github.com/cityjson/flatcitybuf/blob/main/README.md)
- [Issue tracker](https://github.com/cityjson/flatcitybuf/issues)

## License

MIT — see
[LICENSE](https://github.com/cityjson/flatcitybuf/blob/main/LICENSE).
