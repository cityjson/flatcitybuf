# FlatCityBuf CLI

A command-line interface for converting between CityJSON and FlatCityBuf (FCB) formats, with support for spatial and attribute indexing.

## Installation

### Option 1: Install from crates.io (Recommended)

```bash
cargo install fcb_cli --locked
```

This installs the `fcb` binary to your Cargo bin directory (usually `~/.cargo/bin/`).

### Option 2: Build from Source

```bash
# Clone the repository
git clone https://github.com/cityjson/flatcitybuf.git
cd flatcitybuf/src/rust

# Build in release mode
cargo build --release -p fcb_cli

```

### Option 3: Run with Cargo (Development)

```bash
cd flatcitybuf/src/rust
cargo run -p fcb_cli -- <command> [args]

# Example: convert CityJSONSeq to FCB
cargo run -p fcb_cli -- ser input.city.jsonl output.fcb
```

## Usage

```bash
fcb <COMMAND> [OPTIONS] <INPUT> <OUTPUT>
```

Input and output are positional: the input comes first, the output second. The
read-only command `inspect` takes a single positional argument.

### Commands

#### `ser` - Serialize CityJSON to FCB

Convert CityJSON files to FlatCityBuf format with optional indexing.

```bash
fcb ser [OPTIONS] <INPUT>... <OUTPUT>
```

**Arguments:**

- `<INPUT>...` - Input file(s) or glob patterns (supports multiple files; each path must end in `.json` or `.jsonl` -- `ser` does not read from stdin)
- `<OUTPUT>` - Output file, always the last positional (use '-' for stdout)

**Options:**

- `-a, --attr-index ATTRIBUTES` - Comma-separated list of attributes to create index for
- `-A, --index-all-attributes` - Index all attributes found in the dataset
- `-s, --no-spatial-index` - Disable the spatial index (it is written by default)
- `--attr-branching-factor FACTOR` - Branching factor for attribute index (default: 16 with `--attr-index`, 256 with `--index-all-attributes`)
- `--index-node-size SIZE` - Node size of the spatial R-tree index (default: 16)
- `--no-feature-count` - Write a `features_count` of 0, meaning "unknown", which forces readers to scan to EOF (conformance fixtures only)
- `-b, --bbox BBOX` - Bounding box filter in format "minx,miny,maxx,maxy"
- `-g, --ge` - Automatically calculate and set geospatial extent in header

**Examples:**

```bash
# basic conversion from CityJSONSeq
fcb ser input.city.jsonl output.fcb

# convert CityJSON file (standard .json format)
fcb ser city.city.json output.fcb

# multiple input files -- the last positional is the output
fcb ser file1.city.jsonl file2.city.jsonl merged.fcb

# glob patterns to process all matching files
fcb ser 'data/*.city.jsonl' output.fcb
fcb ser 'cities/**/*.city.json' all_cities.fcb

# with attribute indexing
fcb ser delft.city.jsonl delft_attr.fcb \
  --attr-index identificatie,tijdstipregistratie,b3_is_glas_dak,b3_h_dak_50p \
  --attr-branching-factor 256

# index all attributes
fcb ser data.city.jsonl data.fcb --index-all-attributes

# with bounding box filter
fcb ser large_dataset.city.jsonl filtered.fcb \
  --bbox "4.35,52.0,4.4,52.1"

# to stdout (input must still be a real file path)
fcb ser input.city.jsonl - > output.fcb
```

#### `deser` - Deserialize FCB to CityJSON

Convert FlatCityBuf files back to CityJSON format.

```bash
fcb deser <INPUT> <OUTPUT>
```

**Arguments:**

- `<INPUT>` - Input FCB file (use '-' for stdin)
- `<OUTPUT>` - Output file (use '-' for stdout)

**Examples:**

```bash
# basic conversion
fcb deser input.fcb output.city.jsonl

# from stdin to stdout
cat input.fcb | fcb deser - - > output.city.jsonl
```

#### `inspect` - Inspect an FCB file

Show what an FCB dataset's header declares, either in a full-screen terminal UI
or as a static text report. Only the header is read -- feature bytes are never
fetched -- so inspecting a remote file over HTTP range requests costs a couple
of small requests regardless of dataset size.

```bash
fcb inspect [--static] <SOURCE>
```

**Arguments:**

- `<SOURCE>` - Local path or HTTP(S) URL to an FCB file

**Options:**

- `--static` - print the static report instead of the terminal UI

**Output mode:** with stdout on a terminal you get the interactive UI; with
stdout piped or redirected `inspect` prints the static report and exits 0.
`--static` forces the report even on a terminal, which is what scripts and the
`just inspect` recipe use.

**The static report includes:**

- Source (the path or URL) and file size (remote sources report `unknown`)
- CityJSON version, title, identifier and reference date, when present
- Feature count and number of attribute columns
- Geographical extent (min/max and dimensions), when set
- Index summary: whether a spatial R-tree is present and its node size, plus
  the names of the indexed attributes
- Coordinate reference system (code, version, code string), when set
- Coordinate transform (scale and translate), when present

**Tabs (terminal UI):**

- **Metadata** - version, feature count, index sizes, extent, transform, CRS
- **Columns** - the header attribute schema, one row per column
- **Map** - the dataset extent drawn on a world map; shown only for a geographic CRS, otherwise the projected extent is printed instead

**Key bindings:**

- `q`, `Esc`, `Ctrl-C` - quit
- `Tab`, `→`, `l` - next tab
- `Shift-Tab`, `←`, `h` - previous tab
- `↓`, `j` / `↑`, `k` - scroll the column list
- `g` / `G` - jump to the first / last column

**Examples:**

```bash
# local file
fcb inspect delft.fcb

# remote file over HTTP range requests -- only the header is fetched
fcb inspect https://example.com/data.fcb

# static report, for scripts and captured output
fcb inspect --static delft.fcb
```

> **No terminal, no problem.** The terminal UI needs an interactive TTY, so with stdout redirected or piped `inspect` falls back to the static report and exits 0. Pass `--static` to get that same report on a terminal; both paths print byte-identical, colour-free text.

#### `cbor` - Convert CityJSON to CBOR

Convert CityJSON to Concise Binary Object Representation format.

```bash
fcb cbor <INPUT> <OUTPUT>
```

#### `bson` - Convert CityJSON to BSON

Convert CityJSON to Binary JSON format.

```bash
fcb bson <INPUT> <OUTPUT>
```

## Format Support

### Input Formats

- **CityJSON** (`.city.json`) - Standard CityJSON files
- **CityJSON Text Sequences** (`.city.jsonl`) - Line-delimited CityJSON features
- **FCB** (`.fcb`) - FlatCityBuf binary format

> **Remote Input:** `inspect` also accepts an `http://` or `https://` URL and reads the header over HTTP range requests. The other commands (`ser`, `deser`, `cbor`, `bson`) take local paths only, plus `-` for stdin/stdout where documented.

> **Multi-file Support:** The `ser` command accepts multiple input files and glob patterns; the last positional argument is always the output. When merging files with different coordinate transforms, vertices are automatically aligned to the first file's transform.

### Output Formats

- **FCB** (`.fcb`) - FlatCityBuf binary format with optional indexing
- **CityJSON Text Sequences** (`.city.jsonl`) - Line-delimited CityJSON features
- **CBOR** - Concise Binary Object Representation
- **BSON** - Binary JSON

## Examples Workflow

```bash
# 1. convert cityjson to fcb with attribute indexing
fcb ser dataset.city.jsonl dataset.fcb \
  --attr-index "building_type,height,year_built" \
  --attr-branching-factor 256

# 2. check file information
fcb inspect --static dataset.fcb

# 3. convert back to cityjson
fcb deser dataset.fcb output.city.jsonl

# 4. filter by bounding box and index all attributes
fcb ser large_city.city.jsonl filtered_city.fcb \
  --bbox "4.35,52.0,4.4,52.1" \
  --index-all-attributes
```

## Error Handling

The CLI provides detailed error messages for common issues:

- Invalid file formats
- Missing input files
- Malformed bounding box coordinates
- Memory limitations for large datasets

## License

MIT License - see LICENSE file for details.

## Related

- [FlatCityBuf Core Library](../fcb_core/)
- [CityJSON Specification](https://cityjson.org/)
