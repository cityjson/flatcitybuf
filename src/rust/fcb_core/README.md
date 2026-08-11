# fcb_core

Read and write [FlatCityBuf](https://github.com/cityjson/flatcitybuf) (`.fcb`)
— a cloud-optimized binary encoding of [CityJSON](https://www.cityjson.org/).
It carries the standard's semantics in [FlatBuffers](https://flatbuffers.dev/),
with a packed Hilbert R-tree for spatial queries and static B+tree indices for
attribute queries, laid out so a client reads only the bytes it actually needs
— from a local file, a non-seekable stream, or a remote URL over HTTP range
requests. `fcb_core` is the reference implementation of the format and the only
one that *writes* it.

## Install

```bash
cargo add fcb_core
```

The `http` feature is enabled by default and brings in `reqwest` and
`http-range-client`. For a dependency-light, purely local reader:

```toml
[dependencies]
fcb_core = { version = "0.7", default-features = false }
```

## Reading a file

```rust
use fcb_core::{deserializer::to_cj_metadata, FcbReader};
use std::fs::File;
use std::io::BufReader;

let file = BufReader::new(File::open("delft.fcb")?);
let mut features = FcbReader::open(file)?.select_all()?;

// The CityJSON metadata object (version, transform, CRS, extent) is the
// header; it is the first line of the equivalent CityJSONSeq document.
let cj = to_cj_metadata(&features.header())?;
println!("CityJSON {}, {} features", cj.version, features.header().features_count());

while let Some(feature) = features.next()? {
    let cj_feature = feature.cur_cj_feature()?;
    println!("{}: {} city object(s)", cj_feature.id, cj_feature.city_objects.len());
}
```

Spatial queries (`select_query`), attribute queries (`select_attr_query`),
HTTP streaming (`HttpFcbReader`) and writing (`FcbWriter`) are all covered in
the crate documentation on [docs.rs](https://docs.rs/fcb_core).

## Documentation

- [API reference on docs.rs](https://docs.rs/fcb_core)
- [Rust guide](https://github.com/cityjson/flatcitybuf/blob/main/docs/rust.md)
  — workspace layout, build and test commands, worked examples
- [Format specification](https://github.com/cityjson/flatcitybuf/blob/main/docs/specification.md)
  — the byte layout, cited to source
- [`fcb_cli`](https://crates.io/crates/fcb_cli) — the `fcb` command-line tool
  for converting CityJSON to and from `.fcb`
- [Repository](https://github.com/cityjson/flatcitybuf)

## Attribution

Portions of this software are derived from
[FlatGeobuf](https://github.com/flatgeobuf/flatgeobuf) (BSD 2-Clause License),
copyright (c) 2018-2024 Björn Harrtell and contributors — specifically the
packed R-tree spatial index, the HTTP range request handling, and parts of the
binary format design. See
[ATTRIBUTION.md](https://github.com/cityjson/flatcitybuf/blob/main/src/rust/fcb_core/ATTRIBUTION.md)
for details.

## License

MIT — see [LICENSE](https://github.com/cityjson/flatcitybuf/blob/main/src/rust/fcb_core/LICENSE).
FlatGeobuf portions remain under their original BSD 2-Clause License.
