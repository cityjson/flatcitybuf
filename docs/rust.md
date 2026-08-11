# FlatCityBuf — Rust

The Rust workspace is the origin of FlatCityBuf and the **authoritative oracle
for the format**: it reads *and* writes `.fcb`, and the C++, Python and
TypeScript implementations are validated against its output. When two
implementations disagree, Rust is right by definition. If you need to know what
a byte means, `fcb_core` is the answer — see
[docs/specification.md](specification.md) for the format written down.

**Status:** reader + writer, complete.

## Workspace layout

`src/rust` is a Cargo workspace with three members:

| Crate | Path | What it is |
|---|---|---|
| `fcb_core` | `src/rust/fcb_core` | The library: reader, writer, packed Hilbert R-tree, static B+tree, HTTP range-request reader. Published on crates.io. |
| `fcb_cli` | `src/rust/cli` | The `fcb` command-line tool — `ser`, `deser`, `inspect`, `cbor`, `bson`. Published on crates.io. |
| `fcb_api` | `src/rust/fcb_api` | An OGC API - Features server (Axum) that serves a remote `.fcb` over HTTP range requests. Not published; deployed. |

> The former `fcb_wasm` binding has been retired. Browsers are served by the
> from-scratch TypeScript reader in `src/ts` instead — see
> [docs/ts.md](ts.md).

## Install

The CLI, from crates.io:

```bash
cargo install fcb_cli --locked   # installs the `fcb` binary
```

The library, in your own project:

```bash
cargo add fcb_core
```

The `http` feature is **on by default** and pulls in `reqwest` and
`http-range-client`. For a dependency-light, purely local reader, turn it off:

```toml
[dependencies]
fcb_core = { version = "0.7", default-features = false }
```

## Build and verify

`just` is the task runner. Every language directory in this repo exposes the
same five verbs; from `src/rust`:

```bash
just check    # lint + type + test + build, read-only
just test     # cargo nextest run --all-features --workspace
just lint     # cargo fmt --check + cargo clippy
just type     # cargo check --all-features --workspace
just build    # cargo build --workspace --all-features
just fix      # rustfmt + clippy --fix — the only recipe that MUTATES the tree
```

Plain Cargo works too, of course: `cargo build --workspace --all-features`,
`cargo test`, `cargo run -p fcb_cli -- info file.fcb`.

Extras specific to this workspace:

| Recipe | What it does |
|---|---|
| `just ser <input> <output>` | CityJSONSeq → `.fcb` via the CLI, with `-A -g` (index every attribute, compute the extent). This is how the oracle output for the other readers is produced. |
| `just deser <input> <output>` | `.fcb` → CityJSONSeq. |
| `just inspect [file]` | Static header, extent, CRS and index report for a `.fcb` (`inspect … --static`). Defaults to `examples/data/delft.fcb`. |
| `just bench` | `cargo bench -p fcb_core --bench read`. |
| `just docs` / `just docs-open` | Rustdoc for the workspace, all features on — the same configuration docs.rs builds. |
| `just test-remote` | The opt-in live-3DBAG HTTP tests (~68 GB public bucket). Override the target with `FCB_REMOTE_HTTP_URL`. |
| `just build-release`, `just audit`, `just update`, `just clean`, `just test-verbose`, `just file-stats` | The usual. |

`just --list` shows everything.

## Reading a file

`FcbReader::open` parses and verifies the header; the `select_*` methods return
a fallible streaming iterator over features, holding one feature in memory at a
time.

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

Queries skip straight to the matching features — `select_query` walks the
R-tree, `select_attr_query` walks the B+tree indices:

```rust
use fcb_core::{AttrQuery, FcbReader, KeyType, Operator, SpatialQuery};
use std::fs::File;
use std::io::BufReader;

// Everything inside a bounding box (min_x, min_y, max_x, max_y).
let file = BufReader::new(File::open("delft.fcb")?);
let bbox = SpatialQuery::BBox(84_000.0, 446_000.0, 85_000.0, 447_000.0);
let mut hits = FcbReader::open(file)?.select_query(bbox, None, None)?;
while let Some(feature) = hits.next()? {
    println!("{}", feature.cur_cj_feature()?.id);
}

// Everything whose indexed `b3_h_dak_50p` attribute exceeds 2.0.
let file = BufReader::new(File::open("delft.fcb")?);
let query: AttrQuery = vec![(
    "b3_h_dak_50p".to_string(),
    Operator::Gt,
    KeyType::Float64(2.0.into()),
)];
let mut hits = FcbReader::open(file)?.select_attr_query(query)?;
while let Some(feature) = hits.next()? {
    println!("{}", feature.cur_cj_feature()?.id);
}
```

Each `select_*` has a `_seq` twin (`select_all_seq`, `select_query_seq`,
`select_attr_query_seq`) for a `Read`-only stream that cannot seek.

Over HTTP, `HttpFcbReader` fetches the header, then the index, then only the
byte ranges holding the matching features — typically a handful of range
requests against a multi-gigabyte file:

```rust
use fcb_core::{HttpFcbReader, SpatialQuery};

let reader = HttpFcbReader::open("https://example.com/delft.fcb").await?;
let bbox = SpatialQuery::BBox(84_000.0, 446_000.0, 85_000.0, 447_000.0);
let mut features = reader.select_query(bbox).await?;

while features.next().await?.is_some() {
    println!("{}", features.cur_cj_feature()?.id);
}
```

## Writing a file

`FcbWriter` takes the CityJSON metadata object plus a stream of
`CityJSONFeature`s and assembles header, indices and feature data on `write`.

```rust
use fcb_core::{
    attribute::{AttributeSchema, AttributeSchemaMethods},
    header_writer::HeaderWriterOptions,
    read_cityjson_from_reader, CJType, CJTypeKind, CityJSONSeq, FcbWriter,
};
use std::fs::File;
use std::io::{BufReader, BufWriter};

let input = BufReader::new(File::open("delft.city.jsonl")?);
let CJType::Seq(CityJSONSeq { cj, features }) =
    read_cityjson_from_reader(input, CJTypeKind::Seq)?
else {
    unreachable!("CJTypeKind::Seq always yields CJType::Seq")
};

// Collect the attribute columns. Iterate the city objects in a
// deterministic order: `add_attributes` hands each new name the next free
// column index, so a `HashMap`'s random order would number the columns
// differently on every run.
let mut schema = AttributeSchema::new();
for feature in &features {
    let mut ids: Vec<&String> = feature.city_objects.keys().collect();
    ids.sort_unstable();
    for co in ids.into_iter().filter_map(|id| feature.city_objects.get(id)) {
        if let Some(attributes) = &co.attributes {
            schema.add_attributes(attributes);
        }
    }
}

let options = HeaderWriterOptions {
    write_index: true,
    feature_count: features.len() as u64,
    index_node_size: 16,
    // Build a static B+tree over these columns. `None` = default
    // branching factor.
    attribute_indices: Some(vec![("b3_h_dak_50p".to_string(), None)]),
    geographical_extent: None,
};

let mut fcb = FcbWriter::new(cj, Some(options), Some(schema), None)?;
for feature in &features {
    fcb.add_feature(feature)?;
}
fcb.write(BufWriter::new(File::create("delft.fcb")?))?;
```

For one-off conversions reach for the CLI instead:
`fcb ser input.city.jsonl output.fcb -A`.

## Testing

`cargo nextest` is the runner; `just test` runs the whole workspace with all
features on. Two `fcb_core::http` tests fetch from a public Cloudflare R2
bucket, so the default suite needs network access. The heavier
live-3DBAG tests are `#[ignore]`d and run only via `just test-remote`.

The shared oracle corpus lives in `conformance/`: hand-authored CityJSONSeq
inputs under `conformance/inputs/`, the `.fcb` binaries written by the Rust
writer, and `.expected.jsonl` holding the Rust *reader's* own output for each.
Every other implementation compares its whole output against those files, line
for line. Regenerate the corpus from the repository root with `just
gen-conformance` — but note the corpus is not byte-reproducible (`cjseq2`
iterates CityObjects from a `HashMap`), so diff the parsed JSON before
committing and never assert on a physical byte offset.

Full manual verification procedure, local and remote:
[docs/TESTING.md](TESTING.md).

Clippy is deliberately not gated on `-D warnings` yet — `fcb_core` carries
around 50 pre-existing style lints. See the comment in `src/rust/justfile`.

## Where to go next

- **[src/rust/cli/README.md](../src/rust/cli/README.md)** — full `fcb` CLI
  reference: every command, flag and example.
- **[src/rust/fcb_api/README.md](../src/rust/fcb_api/README.md)** — the OGC API
  server: endpoints, query syntax, configuration, deployment.
- **[docs/specification.md](specification.md)** — the format, from schema level
  down to byte offsets, each claim cited to the Rust line that proves it. Read
  this before writing any decoder.
- **[docs/TESTING.md](TESTING.md)** — the manual verification procedure across
  all four implementations.
- **[docs/upstream-findings.md](upstream-findings.md)** — the permanent record
  of defects found across implementations.
- **API reference:** run `just docs-open`, or read the published rustdoc for
  [`fcb_core`](https://docs.rs/fcb_core) and
  [`fcb_cli`](https://docs.rs/fcb_cli) on docs.rs.
- **Other implementations:** [C++](cpp.md), [Python](py.md),
  [TypeScript](ts.md).

## License and attribution

MIT — see [LICENSE](../LICENSE). Portions of `fcb_core` are derived from
[FlatGeobuf](https://github.com/flatgeobuf/flatgeobuf) (BSD 2-Clause); details
in [src/rust/fcb_core/ATTRIBUTION.md](../src/rust/fcb_core/ATTRIBUTION.md).
