# flatcitybuf

a cloud-optimized binary format for storing and retrieving 3d city models based on the cityjson standard.

## overview

flatcitybuf combines the semantic richness of cityjson with the performance benefits of flatbuffers binary serialization and spatial indexing techniques. it addresses several limitations of existing cityjson formats:

- **performance**: enables zero-copy access to specific city objects without parsing the entire file
- **cloud optimization**: supports http range requests for partial data retrieval
- **spatial indexing**: implements a packed r-tree for efficient spatial queries
- **attribute indexing**: uses binary search trees for fast attribute-based filtering
- **size efficiency**: reduces file sizes by 50-70% compared to text-based formats

benchmarks show flatcitybuf is 10-20× faster in data retrieval compared to cityjsonseq.

## getting started

### prerequisites

- rust toolchain (cargo, rustc)
- for wasm builds: wasm-pack

### build

build the core library and cli:

```bash
cargo build --workspace --all-features --exclude fcb_wasm
```

build the wasm module:

```bash
cargo build -p fcb_wasm --target wasm32-unknown-unknown
# or
cd wasm && wasm-pack build --target web --debug --out-dir ../../ts
```

### usage examples

serialize cityjson to flatcitybuf:

```bash
cargo run -p fcb_cli ser -i path/to/input.city.jsonl -o path/to/output.fcb
```

deserialize flatcitybuf to cityjson:

```bash
cargo run -p fcb_cli deser -i path/to/input.fcb -o path/to/output.city.jsonl
```

get information about a flatcitybuf file:

```bash
cargo run -p fcb_cli info -i path/to/file.fcb
```

### run benchmarks

```bash
cargo bench -p fcb_core --bench read
```

## project structure

- **fcb_core**: core library for reading and writing flatcitybuf files
- **fcb_cli**: command-line interface for converting between cityjson and flatcitybuf
- **bst**: binary search tree implementation for attribute indexing
- **packed_rtree**: packed r-tree implementation for spatial indexing
- **fcb_wasm**: webassembly bindings for browser usage

## acknowledgements

this project incorporates code from [flatgeobuf](https://github.com/flatgeobuf/flatgeobuf/tree/master), copyright (c) 2018, björn harrtell, licensed under the bsd 2-clause license.

the flatbuffers schema for citybuf feature format is originally authored by tu delft 3d geoinformation group, ravi peters (3dgi), balazs dukai (3dgi).

## references

- [cityjson specification](https://github.com/cityjson/cityjson-spec)
- [flatbuffers](https://github.com/google/flatbuffers)
- [citybuf](https://github.com/ylannl/citybuf)
- [flatgeobuf](https://github.com/flatgeobuf/flatgeobuf)