# FlatCityBuf CLI

A command-line interface for converting between CityJSON and FlatCityBuf (FCB) formats, with support for spatial and attribute indexing.

## Installation

```bash
cargo install fcb_cli
```

Or build from source:

```bash
cargo build --release
```

## Usage

```bash
fcb <COMMAND> [OPTIONS]
```

### Commands

#### `ser` - Serialize CityJSON to FCB

Convert CityJSON files to FlatCityBuf format with optional indexing.

```bash
fcb ser -i INPUT -o OUTPUT [OPTIONS]
```

**Options:**

- `-i, --input INPUT` - Input file (use '-' for stdin)
- `-o, --output OUTPUT` - Output file (use '-' for stdout)
- `-a, --attr-index ATTRIBUTES` - Comma-separated list of attributes to create index for
- `-A, --index-all-attributes` - Index all attributes found in the dataset
- `-s, --spatial-index` - Enable spatial indexing (default: true)
- `--attr-branching-factor FACTOR` - Branching factor for attribute index (default: 256)
- `-b, --bbox BBOX` - Bounding box filter in format "minx,miny,maxx,maxy"
- `-g, --ge` - Automatically calculate and set geospatial extent in header (default: true)

**Examples:**

```bash
# basic conversion
fcb ser -i input.city.jsonl -o output.fcb

# with attribute indexing
fcb ser -i delft.city.jsonl -o delft_attr.fcb \
  --attr-index identificatie,tijdstipregistratie,b3_is_glas_dak,b3_h_dak_50p \
  --attr-branching-factor 256

# index all attributes
fcb ser -i data.city.jsonl -o data.fcb --index-all-attributes

# with bounding box filter
fcb ser -i large_dataset.city.jsonl -o filtered.fcb \
  --bbox "4.35,52.0,4.4,52.1"

# from stdin to stdout
cat input.city.jsonl | fcb ser -i - -o - > output.fcb
```

#### `deser` - Deserialize FCB to CityJSON

Convert FlatCityBuf files back to CityJSON format.

```bash
fcb deser -i INPUT -o OUTPUT
```

**Options:**

- `-i, --input INPUT` - Input FCB file (use '-' for stdin)
- `-o, --output OUTPUT` - Output file (use '-' for stdout)

**Examples:**

```bash
# basic conversion
fcb deser -i input.fcb -o output.city.jsonl

# from stdin to stdout
cat input.fcb | fcb deser -i - -o - > output.city.jsonl
```

#### `info` - Show FCB file information

Display metadata and statistics about an FCB file.

```bash
fcb info -i INPUT
```

**Example:**

```bash
fcb info -i delft.fcb
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
fcb cbor -i INPUT -o OUTPUT
```

#### `bson` - Convert CityJSON to BSON

Convert CityJSON to Binary JSON format.

```bash
fcb bson -i INPUT -o OUTPUT
```

## Format Support

### Input Formats

- **CityJSON Text Sequences** (`.city.jsonl`) - Line-delimited CityJSON features
- **FCB** (`.fcb`) - FlatCityBuf binary format

### Output Formats

- **FCB** (`.fcb`) - FlatCityBuf binary format with optional indexing
- **CityJSON Text Sequences** (`.city.jsonl`) - Line-delimited CityJSON features
- **CBOR** - Concise Binary Object Representation
- **BSON** - Binary JSON

## Examples Workflow

```bash
# 1. convert cityjson to fcb with attribute indexing
fcb ser -i dataset.city.jsonl -o dataset.fcb \
  --attr-index "building_type,height,year_built" \
  --attr-branching-factor 256

# 2. check file information
fcb info -i dataset.fcb

# 3. convert back to cityjson
fcb deser -i dataset.fcb -o output.city.jsonl

# 4. filter by bounding box and index all attributes
fcb ser -i large_city.city.jsonl -o filtered_city.fcb \
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
