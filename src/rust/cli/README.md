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

Input and output are positional: the input comes first, the output second.

### Commands

#### `ser` - Serialize CityJSON to FCB

Convert CityJSON files to FlatCityBuf format with optional indexing.

```bash
fcb ser [OPTIONS] <INPUT>... <OUTPUT>
```

**Arguments:**

- `<INPUT>...` - Input file(s) or glob patterns (supports multiple files, use '-' for stdin)
- `<OUTPUT>` - Output file, always the last positional (use '-' for stdout)

**Options:**

- `-a, --attr-index ATTRIBUTES` - Comma-separated list of attributes to create index for
- `-A, --index-all-attributes` - Index all attributes found in the dataset
- `-s, --no-spatial-index` - Disable the spatial index (it is written by default)
- `--attr-branching-factor FACTOR` - Branching factor for attribute index (default: 256)
- `--index-node-size SIZE` - Node size of the spatial R-tree index (default: 16)
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

# from stdin to stdout
cat input.city.jsonl | fcb ser - - > output.fcb
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

#### `info` - Show FCB file information

Display metadata and statistics about an FCB file.

```bash
fcb info <INPUT>
```

**Example:**

```bash
fcb info delft.fcb
```

**Output includes:**

- File size in MB
- FCB version
- Feature count
- Bounding box coordinates
- Indexed attributes
- Title (if present)
- Geographical extent

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
fcb info dataset.fcb

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
- [FlatCityBuf WASM](../wasm/)
- [CityJSON Specification](https://cityjson.org/)
