# `fcb inspect` TUI — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `fcb inspect <path|url>`, an interactive terminal UI that reads only the FCB header and presents a navigable overview across three tabs — Metadata, Columns, and a geographic-only extent Map.

**Architecture:** All code lives in the `cli` crate under a new `inspect/` module. A borrowed `Header` is copied into an owned `InspectModel` snapshot before the reader is dropped, so local (`FcbReader`) and URL (`HttpFcbReader` via a one-shot `tokio` `block_on`) paths converge on one model. Rendering (`ratatui` + `crossterm`) and map geometry are pure functions over that model; the event loop is synchronous.

**Tech Stack:** Rust, `clap`, `ratatui`, `crossterm`, `tokio` (URL fetch only), `fcb_core` (`Header`, `FcbReader`, `HttpFcbReader`), `thiserror`.

## Global Constraints

- Idiomatic Rust; `snake_case` fns, `PascalCase` types, `SCREAMING_SNAKE_CASE` consts (`src/rust/CLAUDE.md`).
- **No `unwrap()`/`expect()` outside `#[cfg(test)]`.** Return `CliError` instead.
- Errors use `thiserror` via the existing `CliError`. **No `anyhow`.**
- New crates go in the **workspace** `Cargo.toml` `[workspace.dependencies]`; individual crates reference them with `{ workspace = true }`.
- Little-endian only; `u32::MAX` (4294967295) means null. (Not directly exercised here — header accessors already decode — but do not reintroduce these hazards.)
- **Never invent absent data.** FCB's `ReferenceSystem` exposes only `authority`, `code`, `version`, `code_string`. The header has no geometry-type or M/Z/T/TM flags. Do not display fields the header does not store.
- Run all commands from the worktree root: `/Users/hbbaba/tudelft/cityjson/flatcitybuf/.claude/worktrees/feat+fcb-inspect-tui`. Rust commands run from `src/rust`.
- Commit message trailer on every commit:
  `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`

## File Structure

| File | Responsibility |
|---|---|
| `src/rust/Cargo.toml` | Add `ratatui`, `crossterm` to `[workspace.dependencies]` (Task 5) |
| `src/rust/cli/Cargo.toml` | Reference `ratatui`, `crossterm`, `tokio` with `{ workspace = true }` |
| `src/rust/cli/src/lib.rs` | `pub mod inspect;`; add `CliError` variants |
| `src/rust/cli/src/inspect/mod.rs` | `run_inspect`, `TerminalGuard`, event loop, submodule decls |
| `src/rust/cli/src/inspect/model.rs` | Owned `InspectModel`/`ColumnInfo`/`CrsInfo`/`ExtentInfo` + `from_header` |
| `src/rust/cli/src/inspect/source.rs` | `is_url`, `load_model` (local + URL) |
| `src/rust/cli/src/inspect/map.rs` | `is_geographic`, `project`, coastline parsing, embedded asset |
| `src/rust/cli/src/inspect/app.rs` | `Tab`, `App` state + navigation |
| `src/rust/cli/src/inspect/ui.rs` | `ratatui` rendering of the three tabs |
| `src/rust/cli/src/main.rs` | `Commands::Inspect` arm + dispatch |
| `src/rust/cli/assets/coastline.csv` | Embedded coastline (`lon,lat` lines) |
| `src/rust/cli/assets/generate_coastline.py` | One-shot generator for the CSV |

---

### Task 1: Owned `InspectModel` from `Header` (`inspect/model.rs`)

Foundation: a borrow-free snapshot of everything the TUI shows, built from a `Header`. This is the only code that touches FlatBuffers accessors.

**Files:**
- Create: `src/rust/cli/src/inspect/mod.rs`
- Create: `src/rust/cli/src/inspect/model.rs`
- Modify: `src/rust/cli/src/lib.rs` (add `pub mod inspect;`)
- Test: inline `#[cfg(test)]` in `model.rs`

**Interfaces:**
- Consumes: `fcb_core::FcbReader`, `fcb_core::Header`.
- Produces:
  - `pub struct InspectModel { pub title: Option<String>, pub identifier: Option<String>, pub version: String, pub features_count: u64, pub reference_date: Option<String>, pub index_node_size: u16, pub attribute_index_count: usize, pub columns: Vec<ColumnInfo>, pub crs: Option<CrsInfo>, pub extent: Option<ExtentInfo>, pub transform: Option<TransformInfo> }`
  - `pub struct ColumnInfo { pub name: String, pub type_name: String, pub description: Option<String>, pub nullable: bool, pub primary_key: bool, pub unique: bool }`
  - `pub struct CrsInfo { pub authority: Option<String>, pub code: i32, pub version: i32, pub code_string: Option<String> }` with `pub fn code_label(&self) -> String` → `"EPSG:4326"` (or just the code when authority absent).
  - `pub struct ExtentInfo { pub min: [f64; 3], pub max: [f64; 3] }` with `pub fn dimensions(&self) -> [f64; 3]`.
  - `pub struct TransformInfo { pub scale: [f64; 3], pub translate: [f64; 3] }`
  - `pub fn from_header(header: &Header) -> InspectModel`

- [ ] **Step 1: Write the module skeleton so the crate compiles with the new submodule**

In `src/rust/cli/src/lib.rs`, add after `pub mod reader;`:

```rust
pub mod inspect;
```

Create `src/rust/cli/src/inspect/mod.rs`:

```rust
//! Interactive terminal UI for inspecting an FCB dataset header.

pub mod model;
```

- [ ] **Step 2: Write the failing test**

Create `src/rust/cli/src/inspect/model.rs` with only the test first (so it fails to compile → fails):

```rust
//! Owned, borrow-free snapshot of an FCB header for the inspect TUI.

// (implementation added in Step 4)

#[cfg(test)]
mod tests {
    use super::*;
    use fcb_core::FcbReader;
    use std::fs::File;
    use std::io::BufReader;
    use std::path::PathBuf;

    fn corpus(name: &str) -> PathBuf {
        // <workspace>/conformance/<name>. cli crate is at src/rust/cli.
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../conformance")
            .join(name)
    }

    #[test]
    fn builds_model_from_header() {
        let path = corpus("inferable_types.fcb");
        let reader = BufReader::new(File::open(&path).expect("open fixture"));
        let fcb = FcbReader::open(reader).expect("open fcb");
        let model = from_header(&fcb.header());

        // Every FCB header carries a version string.
        assert!(!model.version.is_empty());
        // The fixture has attribute columns; ensure we captured names + types.
        assert!(!model.columns.is_empty());
        for col in &model.columns {
            assert!(!col.name.is_empty());
            assert!(!col.type_name.is_empty());
        }
    }

    #[test]
    fn crs_code_label_formats_authority_and_code() {
        let crs = CrsInfo {
            authority: Some("EPSG".to_string()),
            code: 4326,
            version: 0,
            code_string: None,
        };
        assert_eq!(crs.code_label(), "EPSG:4326");

        let crs_no_auth = CrsInfo {
            authority: None,
            code: 28992,
            version: 0,
            code_string: None,
        };
        assert_eq!(crs_no_auth.code_label(), "28992");
    }

    #[test]
    fn extent_dimensions_are_max_minus_min() {
        let e = ExtentInfo {
            min: [0.0, 0.0, 0.0],
            max: [10.0, 20.0, 5.0],
        };
        assert_eq!(e.dimensions(), [10.0, 20.0, 5.0]);
    }
}
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cd src/rust && cargo test -p fcb_cli inspect::model 2>&1 | tail -20`
Expected: FAIL — compile errors (`from_header`, `CrsInfo`, `ExtentInfo` not found).

- [ ] **Step 4: Write the minimal implementation**

Insert above the `#[cfg(test)]` block in `model.rs`:

```rust
use fcb_core::Header;

/// Borrow-free snapshot of an FCB header for rendering.
#[derive(Debug, Clone)]
pub struct InspectModel {
    pub title: Option<String>,
    pub identifier: Option<String>,
    pub version: String,
    pub features_count: u64,
    pub reference_date: Option<String>,
    pub index_node_size: u16,
    pub attribute_index_count: usize,
    pub columns: Vec<ColumnInfo>,
    pub crs: Option<CrsInfo>,
    pub extent: Option<ExtentInfo>,
    pub transform: Option<TransformInfo>,
}

#[derive(Debug, Clone)]
pub struct ColumnInfo {
    pub name: String,
    pub type_name: String,
    pub description: Option<String>,
    pub nullable: bool,
    pub primary_key: bool,
    pub unique: bool,
}

#[derive(Debug, Clone)]
pub struct CrsInfo {
    pub authority: Option<String>,
    pub code: i32,
    pub version: i32,
    pub code_string: Option<String>,
}

impl CrsInfo {
    /// `"EPSG:4326"` when an authority is present, otherwise the bare code.
    pub fn code_label(&self) -> String {
        match &self.authority {
            Some(auth) => format!("{auth}:{}", self.code),
            None => self.code.to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExtentInfo {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

impl ExtentInfo {
    /// Width, height, depth = max - min per axis.
    pub fn dimensions(&self) -> [f64; 3] {
        [
            self.max[0] - self.min[0],
            self.max[1] - self.min[1],
            self.max[2] - self.min[2],
        ]
    }
}

#[derive(Debug, Clone)]
pub struct TransformInfo {
    pub scale: [f64; 3],
    pub translate: [f64; 3],
}

/// Build an owned snapshot from a borrowed header. All borrowed `&str`/vector
/// data is copied so the reader (and its buffer) can be dropped afterwards.
pub fn from_header(header: &Header) -> InspectModel {
    let columns = header
        .columns()
        .map(|cols| {
            cols.iter()
                .map(|c| ColumnInfo {
                    name: c.name().to_string(),
                    type_name: c
                        .type_()
                        .variant_name()
                        .unwrap_or("Unknown")
                        .to_string(),
                    description: c.description().map(|s| s.to_string()),
                    nullable: c.nullable(),
                    primary_key: c.primary_key(),
                    unique: c.unique(),
                })
                .collect()
        })
        .unwrap_or_default();

    let crs = header.reference_system().map(|rs| CrsInfo {
        authority: rs.authority().map(|s| s.to_string()),
        code: rs.code(),
        version: rs.version(),
        code_string: rs.code_string().map(|s| s.to_string()),
    });

    let extent = header.geographical_extent().map(|e| ExtentInfo {
        min: [e.min().x(), e.min().y(), e.min().z()],
        max: [e.max().x(), e.max().y(), e.max().z()],
    });

    let transform = header.transform().map(|t| TransformInfo {
        scale: [t.scale().x(), t.scale().y(), t.scale().z()],
        translate: [t.translate().x(), t.translate().y(), t.translate().z()],
    });

    InspectModel {
        title: header.title().map(|s| s.to_string()),
        identifier: header.identifier().map(|s| s.to_string()),
        version: header.version().to_string(),
        features_count: header.features_count(),
        reference_date: header.reference_date().map(|s| s.to_string()),
        index_node_size: header.index_node_size(),
        attribute_index_count: header
            .attribute_index()
            .map(|v| v.len())
            .unwrap_or(0),
        columns,
        crs,
        extent,
        transform,
    }
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd src/rust && cargo test -p fcb_cli inspect::model 2>&1 | tail -20`
Expected: PASS — 3 tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/rust/cli/src/lib.rs src/rust/cli/src/inspect/mod.rs src/rust/cli/src/inspect/model.rs
git commit -m "feat(cli): owned InspectModel snapshot from FCB header

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: Source resolution — local + URL (`inspect/source.rs`)

Detect path vs URL and produce an `InspectModel`. Local reads are sync; URL reads run on a one-shot `tokio` runtime.

**Files:**
- Create: `src/rust/cli/src/inspect/source.rs`
- Modify: `src/rust/cli/src/inspect/mod.rs` (add `pub mod source;`)
- Modify: `src/rust/cli/src/lib.rs` (add `CliError::NotATerminal` — used later — and rely on existing `FcbCore`/`Io` variants for fetch errors)
- Modify: `src/rust/cli/Cargo.toml` (add `tokio = { workspace = true }`)
- Test: inline `#[cfg(test)]` in `source.rs`

**Interfaces:**
- Consumes: `crate::inspect::model::{InspectModel, from_header}`, `fcb_core::{FcbReader, HttpFcbReader}`, `crate::CliError`.
- Produces:
  - `pub fn is_url(source: &str) -> bool`
  - `pub fn load_model(source: &str) -> Result<InspectModel, CliError>`

- [ ] **Step 1: Add the tokio dependency to the cli crate**

In `src/rust/cli/Cargo.toml`, under `[dependencies]`, add:

```toml
tokio = { workspace = true }
```

- [ ] **Step 2: Add the `CliError::NotATerminal` variant**

In `src/rust/cli/src/lib.rs`, inside `enum CliError`, add:

```rust
    #[error("inspect requires an interactive terminal; use `fcb info` for static output")]
    NotATerminal,
```

- [ ] **Step 3: Write the failing test**

Create `src/rust/cli/src/inspect/source.rs`:

```rust
//! Resolve an inspect source (local path or HTTP URL) into an `InspectModel`.

// (implementation added in Step 5)

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn detects_http_and_https_urls() {
        assert!(is_url("http://example.com/a.fcb"));
        assert!(is_url("https://example.com/a.fcb"));
    }

    #[test]
    fn treats_local_paths_as_non_urls() {
        assert!(!is_url("./data/a.fcb"));
        assert!(!is_url("/abs/a.fcb"));
        assert!(!is_url("a.fcb"));
        // Windows drive letters must not be mistaken for a scheme.
        assert!(!is_url("C:/data/a.fcb"));
    }

    #[test]
    fn loads_model_from_local_file() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../conformance/inferable_types.fcb");
        let model = load_model(path.to_str().unwrap()).expect("load model");
        assert!(!model.version.is_empty());
    }

    #[test]
    fn missing_local_file_is_an_error() {
        let err = load_model("/no/such/file.fcb");
        assert!(err.is_err());
    }
}
```

- [ ] **Step 4: Run the test to verify it fails**

Run: `cd src/rust && cargo test -p fcb_cli inspect::source 2>&1 | tail -20`
Expected: FAIL — `is_url` / `load_model` not found.

- [ ] **Step 5: Write the minimal implementation**

Insert above the `#[cfg(test)]` block in `source.rs`:

```rust
use std::fs::File;
use std::io::BufReader;

use fcb_core::{FcbReader, HttpFcbReader};

use crate::inspect::model::{from_header, InspectModel};
use crate::CliError;

/// True when `source` is an `http://` or `https://` URL. A bare Windows drive
/// letter (`C:/...`) is deliberately not treated as a scheme.
pub fn is_url(source: &str) -> bool {
    source.starts_with("http://") || source.starts_with("https://")
}

/// Load an `InspectModel` from a local path or an HTTP(S) URL. Only the header
/// is read; feature bytes are never fetched.
pub fn load_model(source: &str) -> Result<InspectModel, CliError> {
    if is_url(source) {
        load_model_http(source)
    } else {
        let reader = BufReader::new(File::open(source)?);
        let fcb = FcbReader::open(reader)?;
        Ok(from_header(&fcb.header()))
    }
}

/// Fetch just the header over HTTP on a short-lived current-thread runtime.
fn load_model_http(url: &str) -> Result<InspectModel, CliError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let reader = HttpFcbReader::open(url).await?;
        Ok(from_header(&reader.header()))
    })
}
```

Add `pub mod source;` to `src/rust/cli/src/inspect/mod.rs` (below `pub mod model;`).

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cd src/rust && cargo test -p fcb_cli inspect::source 2>&1 | tail -20`
Expected: PASS — 4 tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/rust/cli/Cargo.toml src/rust/cli/src/lib.rs src/rust/cli/src/inspect/mod.rs src/rust/cli/src/inspect/source.rs src/rust/Cargo.lock
git commit -m "feat(cli): resolve inspect source (local path or HTTP URL) to model

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: Map geometry + embedded coastline (`inspect/map.rs`)

Pure geometry: decide if a dataset is geographic, project lon/lat to canvas coordinates, and load the embedded coastline. No `ratatui` here — this is math + data only.

**Files:**
- Create: `src/rust/cli/assets/generate_coastline.py`
- Create: `src/rust/cli/assets/coastline.csv` (generated, committed)
- Create: `src/rust/cli/src/inspect/map.rs`
- Modify: `src/rust/cli/src/inspect/mod.rs` (add `pub mod map;`)
- Test: inline `#[cfg(test)]` in `map.rs`

**Interfaces:**
- Consumes: `crate::inspect::model::{CrsInfo, ExtentInfo}`.
- Produces:
  - `pub const GEOGRAPHIC_EPSG: [i32; 3] = [4326, 4979, 4258];`
  - `pub fn is_geographic(crs: Option<&CrsInfo>, extent: &ExtentInfo) -> bool`
  - `pub fn project(lon: f64, lat: f64, w: f64, h: f64) -> (f64, f64)` — equirectangular into `[0,w) × [0,h)`, y flipped (north at top).
  - `pub fn coastline_points() -> &'static [(f64, f64)]` — parsed once from the embedded CSV (returns `(lon, lat)`).

- [ ] **Step 1: Create the coastline generator script**

Create `src/rust/cli/assets/generate_coastline.py`:

```python
#!/usr/bin/env python3
"""Generate assets/coastline.csv (lon,lat per line) from Natural Earth 110m.

Run once; the produced CSV is committed so builds need no network. Source:
Natural Earth 1:110m coastline (public domain).

Usage:
    python3 generate_coastline.py \
        https://raw.githubusercontent.com/nvkelso/natural-earth-vector/master/geojson/ne_110m_coastline.geojson

Points are decimated to at most one per ~0.75 degrees along each line so the
file stays a few tens of KB and reads well at terminal resolution.
"""
import json
import sys
import urllib.request

STEP_DEG = 0.75


def emit(coords, out):
    last = None
    for lon, lat in coords:
        if last is None or abs(lon - last[0]) + abs(lat - last[1]) >= STEP_DEG:
            out.append((round(lon, 3), round(lat, 3)))
            last = (lon, lat)


def main() -> None:
    url = sys.argv[1]
    with urllib.request.urlopen(url) as resp:
        gj = json.load(resp)
    out: list[tuple[float, float]] = []
    for feat in gj["features"]:
        geom = feat["geometry"]
        if geom["type"] == "LineString":
            emit(geom["coordinates"], out)
        elif geom["type"] == "MultiLineString":
            for line in geom["coordinates"]:
                emit(line, out)
    with open("coastline.csv", "w", encoding="utf-8") as f:
        f.write("# lon,lat decimated Natural Earth 110m coastline (public domain)\n")
        for lon, lat in out:
            f.write(f"{lon},{lat}\n")
    print(f"wrote {len(out)} points")


if __name__ == "__main__":
    main()
```

- [ ] **Step 2: Generate the committed CSV**

Run (from the assets dir):

```bash
cd src/rust/cli/assets && python3 generate_coastline.py \
  https://raw.githubusercontent.com/nvkelso/natural-earth-vector/master/geojson/ne_110m_coastline.geojson
```

Expected: prints `wrote <N> points` (N in the low thousands) and creates `coastline.csv`.

If offline, substitute any public-domain coastline GeoJSON URL with the same schema; the test in Step 4 only requires ≥ 1000 valid in-bounds points.

- [ ] **Step 3: Write the failing test**

Create `src/rust/cli/src/inspect/map.rs`:

```rust
//! Pure map geometry for the inspect Map tab: geographic gate, equirectangular
//! projection, and the embedded world coastline.

// (implementation added in Step 5)

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspect::model::{CrsInfo, ExtentInfo};

    fn geo_extent() -> ExtentInfo {
        ExtentInfo { min: [4.0, 52.0, 0.0], max: [5.0, 53.0, 10.0] }
    }
    fn projected_extent() -> ExtentInfo {
        // Typical EPSG:28992 (metres) values, far outside lon/lat range.
        ExtentInfo { min: [84000.0, 447000.0, 0.0], max: [85000.0, 448000.0, 10.0] }
    }

    #[test]
    fn geographic_epsg_with_lonlat_extent_is_geographic() {
        let crs = CrsInfo { authority: Some("EPSG".into()), code: 4326, version: 0, code_string: None };
        assert!(is_geographic(Some(&crs), &geo_extent()));
    }

    #[test]
    fn projected_epsg_is_not_geographic() {
        let crs = CrsInfo { authority: Some("EPSG".into()), code: 28992, version: 0, code_string: None };
        assert!(!is_geographic(Some(&crs), &projected_extent()));
    }

    #[test]
    fn no_crs_falls_back_to_extent_bounds() {
        assert!(is_geographic(None, &geo_extent()));
        assert!(!is_geographic(None, &projected_extent()));
    }

    #[test]
    fn projects_corners_to_canvas_edges() {
        // Top-left of the world map is (-180, 90); bottom-right is (180, -90).
        let (x0, y0) = project(-180.0, 90.0, 100.0, 50.0);
        assert!(x0.abs() < 1e-9 && y0.abs() < 1e-9);
        let (x1, y1) = project(180.0, -90.0, 100.0, 50.0);
        assert!((x1 - 100.0).abs() < 1e-9 && (y1 - 50.0).abs() < 1e-9);
        // Equator/prime meridian maps to the canvas centre.
        let (xc, yc) = project(0.0, 0.0, 100.0, 50.0);
        assert!((xc - 50.0).abs() < 1e-9 && (yc - 25.0).abs() < 1e-9);
    }

    #[test]
    fn coastline_points_are_within_lonlat_bounds() {
        let pts = coastline_points();
        assert!(pts.len() >= 1000, "expected a substantial coastline, got {}", pts.len());
        for &(lon, lat) in pts {
            assert!((-180.0..=180.0).contains(&lon), "lon out of range: {lon}");
            assert!((-90.0..=90.0).contains(&lat), "lat out of range: {lat}");
        }
    }
}
```

- [ ] **Step 4: Run the test to verify it fails**

Run: `cd src/rust && cargo test -p fcb_cli inspect::map 2>&1 | tail -20`
Expected: FAIL — `is_geographic` / `project` / `coastline_points` not found.

- [ ] **Step 5: Write the minimal implementation**

Insert above the `#[cfg(test)]` block in `map.rs`:

```rust
use std::sync::OnceLock;

use crate::inspect::model::{CrsInfo, ExtentInfo};

/// EPSG codes we treat as geographic (lon/lat): WGS84 2D, WGS84 3D, ETRS89.
pub const GEOGRAPHIC_EPSG: [i32; 3] = [4326, 4979, 4258];

/// Embedded, decimated world coastline (`lon,lat` per line, `#` comments).
const COASTLINE_CSV: &str = include_str!("../../assets/coastline.csv");

fn extent_in_lonlat(extent: &ExtentInfo) -> bool {
    extent.min[0] >= -180.0
        && extent.max[0] <= 180.0
        && extent.min[1] >= -90.0
        && extent.max[1] <= 90.0
}

/// Geographic when the CRS is a known geographic EPSG *and* the extent lies in
/// lon/lat range; with no CRS, fall back to the extent-bounds check alone.
pub fn is_geographic(crs: Option<&CrsInfo>, extent: &ExtentInfo) -> bool {
    match crs {
        Some(c) => GEOGRAPHIC_EPSG.contains(&c.code) && extent_in_lonlat(extent),
        None => extent_in_lonlat(extent),
    }
}

/// Equirectangular projection into `[0,w] × [0,h]`. Longitude increases left to
/// right; latitude is flipped so north is at the top.
pub fn project(lon: f64, lat: f64, w: f64, h: f64) -> (f64, f64) {
    let x = (lon + 180.0) / 360.0 * w;
    let y = (90.0 - lat) / 180.0 * h;
    (x, y)
}

/// Parse the embedded coastline once, skipping `#` comment and blank lines.
pub fn coastline_points() -> &'static [(f64, f64)] {
    static POINTS: OnceLock<Vec<(f64, f64)>> = OnceLock::new();
    POINTS
        .get_or_init(|| {
            COASTLINE_CSV
                .lines()
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .filter_map(|l| {
                    let (lon, lat) = l.split_once(',')?;
                    Some((lon.trim().parse().ok()?, lat.trim().parse().ok()?))
                })
                .collect()
        })
        .as_slice()
}
```

Add `pub mod map;` to `src/rust/cli/src/inspect/mod.rs`.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cd src/rust && cargo test -p fcb_cli inspect::map 2>&1 | tail -20`
Expected: PASS — 5 tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/rust/cli/assets/generate_coastline.py src/rust/cli/assets/coastline.csv src/rust/cli/src/inspect/mod.rs src/rust/cli/src/inspect/map.rs
git commit -m "feat(cli): map geometry + embedded coastline for inspect

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 4: App state + navigation (`inspect/app.rs`)

Pure UI state: which tab is active and the Columns scroll offset, with clamped navigation. No rendering, no deps.

**Files:**
- Create: `src/rust/cli/src/inspect/app.rs`
- Modify: `src/rust/cli/src/inspect/mod.rs` (add `pub mod app;`)
- Test: inline `#[cfg(test)]` in `app.rs`

**Interfaces:**
- Produces:
  - `pub enum Tab { Metadata, Columns, Map }` (derives `Clone, Copy, PartialEq, Eq, Debug`)
  - `pub struct App { pub tab: Tab, pub column_offset: usize, pub column_count: usize, pub should_quit: bool }`
  - `App::new(column_count: usize) -> App`
  - `App::next_tab(&mut self)`, `App::prev_tab(&mut self)` (wrap around)
  - `App::scroll_down(&mut self)`, `App::scroll_up(&mut self)` (clamped to `0..column_count.saturating_sub(1)`)
  - `App::to_top(&mut self)`, `App::to_bottom(&mut self)`

- [ ] **Step 1: Write the failing test**

Create `src/rust/cli/src/inspect/app.rs`:

```rust
//! Interaction state for the inspect TUI: active tab and column scrolling.

// (implementation added in Step 3)

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_navigation_wraps_both_directions() {
        let mut app = App::new(10);
        assert_eq!(app.tab, Tab::Metadata);
        app.next_tab();
        assert_eq!(app.tab, Tab::Columns);
        app.next_tab();
        assert_eq!(app.tab, Tab::Map);
        app.next_tab();
        assert_eq!(app.tab, Tab::Metadata); // wrapped forward
        app.prev_tab();
        assert_eq!(app.tab, Tab::Map); // wrapped backward
    }

    #[test]
    fn scroll_is_clamped_to_column_range() {
        let mut app = App::new(3); // valid offsets 0..=2
        app.scroll_up(); // already at top, stays 0
        assert_eq!(app.column_offset, 0);
        app.scroll_down();
        app.scroll_down();
        app.scroll_down(); // would be 3, clamps to 2
        assert_eq!(app.column_offset, 2);
        app.to_top();
        assert_eq!(app.column_offset, 0);
        app.to_bottom();
        assert_eq!(app.column_offset, 2);
    }

    #[test]
    fn scroll_on_empty_columns_stays_zero() {
        let mut app = App::new(0);
        app.scroll_down();
        app.to_bottom();
        assert_eq!(app.column_offset, 0);
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd src/rust && cargo test -p fcb_cli inspect::app 2>&1 | tail -20`
Expected: FAIL — `App` / `Tab` not found.

- [ ] **Step 3: Write the minimal implementation**

Insert above the `#[cfg(test)]` block in `app.rs`:

```rust
/// The three inspect tabs, in display order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Metadata,
    Columns,
    Map,
}

const TAB_ORDER: [Tab; 3] = [Tab::Metadata, Tab::Columns, Tab::Map];

/// Interaction state, independent of any terminal backend.
pub struct App {
    pub tab: Tab,
    pub column_offset: usize,
    pub column_count: usize,
    pub should_quit: bool,
}

impl App {
    pub fn new(column_count: usize) -> Self {
        App { tab: Tab::Metadata, column_offset: 0, column_count, should_quit: false }
    }

    fn tab_index(&self) -> usize {
        TAB_ORDER.iter().position(|t| *t == self.tab).unwrap_or(0)
    }

    pub fn next_tab(&mut self) {
        self.tab = TAB_ORDER[(self.tab_index() + 1) % TAB_ORDER.len()];
    }

    pub fn prev_tab(&mut self) {
        self.tab = TAB_ORDER[(self.tab_index() + TAB_ORDER.len() - 1) % TAB_ORDER.len()];
    }

    /// Largest valid scroll offset (0 when there are no columns).
    fn max_offset(&self) -> usize {
        self.column_count.saturating_sub(1)
    }

    pub fn scroll_down(&mut self) {
        self.column_offset = (self.column_offset + 1).min(self.max_offset());
    }

    pub fn scroll_up(&mut self) {
        self.column_offset = self.column_offset.saturating_sub(1);
    }

    pub fn to_top(&mut self) {
        self.column_offset = 0;
    }

    pub fn to_bottom(&mut self) {
        self.column_offset = self.max_offset();
    }
}
```

Add `pub mod app;` to `src/rust/cli/src/inspect/mod.rs`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src/rust && cargo test -p fcb_cli inspect::app 2>&1 | tail -20`
Expected: PASS — 3 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/rust/cli/src/inspect/mod.rs src/rust/cli/src/inspect/app.rs
git commit -m "feat(cli): inspect app state with tab + scroll navigation

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 5: Rendering (`inspect/ui.rs`)

Render the three tabs from `InspectModel` + `App` using `ratatui`. This task introduces the `ratatui`/`crossterm` dependencies.

**Files:**
- Modify: `src/rust/Cargo.toml` (add `ratatui`, `crossterm` to `[workspace.dependencies]`)
- Modify: `src/rust/cli/Cargo.toml` (reference both with `{ workspace = true }`)
- Create: `src/rust/cli/src/inspect/ui.rs`
- Modify: `src/rust/cli/src/inspect/mod.rs` (add `pub mod ui;`)
- Test: inline `#[cfg(test)]` in `ui.rs` using `ratatui::backend::TestBackend`

**Interfaces:**
- Consumes: `crate::inspect::model::InspectModel`, `crate::inspect::app::{App, Tab}`, `crate::inspect::map`.
- Produces:
  - `pub fn draw(frame: &mut ratatui::Frame, model: &InspectModel, app: &App)` — top-level: tab bar + active tab body.

- [ ] **Step 1: Add the workspace dependencies**

In `src/rust/Cargo.toml` under `[workspace.dependencies]`, add:

```toml
ratatui = "0.29"
crossterm = "0.28"
```

In `src/rust/cli/Cargo.toml` under `[dependencies]`, add:

```toml
ratatui = { workspace = true }
crossterm = { workspace = true }
```

- [ ] **Step 2: Write the failing test**

Create `src/rust/cli/src/inspect/ui.rs`:

```rust
//! `ratatui` rendering of the three inspect tabs.

// (implementation added in Step 4)

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspect::app::App;
    use crate::inspect::model::{ColumnInfo, InspectModel};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn sample_model() -> InspectModel {
        InspectModel {
            title: Some("Sample City".into()),
            identifier: None,
            version: "2.0".into(),
            features_count: 42,
            reference_date: None,
            index_node_size: 16,
            attribute_index_count: 1,
            columns: vec![ColumnInfo {
                name: "building_height".into(),
                type_name: "Double".into(),
                description: None,
                nullable: true,
                primary_key: false,
                unique: false,
            }],
            crs: None,
            extent: None,
            transform: None,
        }
    }

    fn rendered_text(app: &App) -> String {
        let model = sample_model();
        let backend = TestBackend::new(90, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &model, app)).unwrap();
        let buf = terminal.backend().buffer().clone();
        buf.content().iter().map(|c| c.symbol()).collect()
    }

    #[test]
    fn metadata_tab_shows_title_and_tab_bar() {
        let app = App::new(1); // defaults to Metadata
        let text = rendered_text(&app);
        assert!(text.contains("Metadata"));
        assert!(text.contains("Columns"));
        assert!(text.contains("Map"));
        assert!(text.contains("Sample City"));
    }

    #[test]
    fn columns_tab_shows_column_name() {
        let mut app = App::new(1);
        app.next_tab(); // Columns
        let text = rendered_text(&app);
        assert!(text.contains("building_height"));
    }
}
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cd src/rust && cargo test -p fcb_cli inspect::ui 2>&1 | tail -20`
Expected: FAIL — `draw` not found (and `ratatui` newly linked).

- [ ] **Step 4: Write the minimal implementation**

Insert above the `#[cfg(test)]` block in `ui.rs`:

```rust
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::canvas::{Canvas, Points, Rectangle};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, Tabs};
use ratatui::Frame;

use crate::inspect::app::{App, Tab};
use crate::inspect::map;
use crate::inspect::model::InspectModel;

/// Render the tab bar plus the body of the active tab.
pub fn draw(frame: &mut Frame, model: &InspectModel, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(frame.area());

    draw_tab_bar(frame, chunks[0], app);
    match app.tab {
        Tab::Metadata => draw_metadata(frame, chunks[1], model),
        Tab::Columns => draw_columns(frame, chunks[1], model, app),
        Tab::Map => draw_map(frame, chunks[1], model),
    }
}

fn draw_tab_bar(frame: &mut Frame, area: Rect, app: &App) {
    let titles = ["Metadata", "Columns", "Map"];
    let selected = match app.tab {
        Tab::Metadata => 0,
        Tab::Columns => 1,
        Tab::Map => 2,
    };
    let tabs = Tabs::new(titles.iter().map(|t| Line::from(*t)).collect::<Vec<_>>())
        .block(Block::default().borders(Borders::ALL).title("Header Categories"))
        .select(selected)
        .highlight_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
    frame.render_widget(tabs, area);
}

fn kv(label: &str, value: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label}: "), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        Span::raw(value),
    ])
}

fn draw_metadata(frame: &mut Frame, area: Rect, model: &InspectModel) {
    let mut lines: Vec<Line> = Vec::new();
    if let Some(t) = &model.title {
        lines.push(kv("Title", t.clone()));
    }
    if let Some(id) = &model.identifier {
        lines.push(kv("Identifier", id.clone()));
    }
    lines.push(kv("FCB Version", model.version.clone()));
    lines.push(kv("Features", model.features_count.to_string()));
    lines.push(kv("Columns", model.columns.len().to_string()));
    lines.push(kv("Spatial Index R-Tree Node Size", model.index_node_size.to_string()));
    lines.push(kv("Attribute Indices", model.attribute_index_count.to_string()));
    if let Some(d) = &model.reference_date {
        lines.push(kv("Reference Date", d.clone()));
    }
    if let Some(e) = &model.extent {
        lines.push(kv(
            "Bounds",
            format!("[{:.4}, {:.4}, {:.4}] .. [{:.4}, {:.4}, {:.4}]",
                e.min[0], e.min[1], e.min[2], e.max[0], e.max[1], e.max[2]),
        ));
        let d = e.dimensions();
        lines.push(kv("Dimensions", format!("{:.2} x {:.2} x {:.2}", d[0], d[1], d[2])));
    }
    if let Some(t) = &model.transform {
        lines.push(kv("Scale", format!("[{:.6}, {:.6}, {:.6}]", t.scale[0], t.scale[1], t.scale[2])));
        lines.push(kv("Translate", format!("[{:.3}, {:.3}, {:.3}]", t.translate[0], t.translate[1], t.translate[2])));
    }
    match &model.crs {
        Some(c) => {
            lines.push(kv("CRS Code", c.code_label()));
            if c.version != 0 {
                lines.push(kv("CRS Version", c.version.to_string()));
            }
            if let Some(cs) = &c.code_string {
                lines.push(kv("CRS Code String", cs.clone()));
            }
        }
        None => lines.push(kv("CRS", "Not set".into())),
    }

    let para = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("Metadata"));
    frame.render_widget(para, area);
}

fn draw_columns(frame: &mut Frame, area: Rect, model: &InspectModel, app: &App) {
    let header = Row::new(["Name", "Type", "Description", "Nullable", "Primary Key", "Unique"])
        .style(Style::default().add_modifier(Modifier::BOLD));
    let rows = model.columns.iter().enumerate().map(|(i, c)| {
        let style = if i == app.column_offset {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        Row::new(vec![
            Cell::from(c.name.clone()),
            Cell::from(c.type_name.clone()),
            Cell::from(c.description.clone().unwrap_or_else(|| "-".into())),
            Cell::from(c.nullable.to_string()),
            Cell::from(c.primary_key.to_string()),
            Cell::from(c.unique.to_string()),
        ])
        .style(style)
    });
    let widths = [
        Constraint::Percentage(28),
        Constraint::Percentage(12),
        Constraint::Percentage(28),
        Constraint::Percentage(11),
        Constraint::Percentage(12),
        Constraint::Percentage(9),
    ];
    let title = format!("Columns ({} of {})", app.column_offset + 1, model.columns.len().max(1));
    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::ALL).title(title));
    frame.render_widget(table, area);
}

fn draw_map(frame: &mut Frame, area: Rect, model: &InspectModel) {
    let extent = match &model.extent {
        Some(e) => e,
        None => {
            let para = Paragraph::new("No geographical extent in header.")
                .block(Block::default().borders(Borders::ALL).title("Map"));
            frame.render_widget(para, area);
            return;
        }
    };

    if !map::is_geographic(model.crs.as_ref(), extent) {
        let crs = model
            .crs
            .as_ref()
            .map(|c| c.code_label())
            .unwrap_or_else(|| "unknown".into());
        let msg = format!(
            "Map unavailable: projected CRS ({crs}).\nExtent: [{:.2}, {:.2}] .. [{:.2}, {:.2}]",
            extent.min[0], extent.min[1], extent.max[0], extent.max[1]
        );
        let para = Paragraph::new(msg)
            .block(Block::default().borders(Borders::ALL).title("Map"));
        frame.render_widget(para, area);
        return;
    }

    let coast: Vec<(f64, f64)> = map::coastline_points().to_vec();
    let (min_x, min_y, max_x, max_y) =
        (extent.min[0], extent.min[1], extent.max[0], extent.max[1]);
    let canvas = Canvas::default()
        .block(Block::default().borders(Borders::ALL).title("Extent of Data (EPSG:4326)"))
        .x_bounds([-180.0, 180.0])
        .y_bounds([-90.0, 90.0])
        .paint(move |ctx| {
            ctx.draw(&Points { coords: &coast, color: Color::Rgb(200, 90, 40) });
            ctx.draw(&Rectangle {
                x: min_x,
                y: min_y,
                width: (max_x - min_x).max(0.5),
                height: (max_y - min_y).max(0.5),
                color: Color::Green,
            });
        });
    frame.render_widget(canvas, area);
}
```

Note: the Map canvas uses `ratatui`'s own `x_bounds`/`y_bounds` (in lon/lat), so `map::project` is exercised by unit tests and reserved for any future non-canvas rendering; drawing here delegates to the canvas coordinate system. Add `pub mod ui;` to `src/rust/cli/src/inspect/mod.rs`.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd src/rust && cargo test -p fcb_cli inspect::ui 2>&1 | tail -20`
Expected: PASS — 2 tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/rust/Cargo.toml src/rust/cli/Cargo.toml src/rust/cli/src/inspect/mod.rs src/rust/cli/src/inspect/ui.rs src/rust/Cargo.lock
git commit -m "feat(cli): ratatui rendering for inspect tabs

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 6: Event loop, terminal guard, and subcommand wiring (`inspect/mod.rs`, `main.rs`)

Tie it together: an RAII terminal guard that always restores the terminal, the synchronous key loop, the non-TTY guard, and the `fcb inspect` subcommand.

**Files:**
- Modify: `src/rust/cli/src/inspect/mod.rs` (add `run_inspect`, `TerminalGuard`, event loop)
- Modify: `src/rust/cli/src/main.rs` (add `Commands::Inspect` + dispatch)
- Test: inline `#[cfg(test)]` in `inspect/mod.rs`; existing `verify_cli` in `main.rs`

**Interfaces:**
- Consumes: `crate::inspect::{app::App, model, source, ui}`, `crate::CliError`, `crossterm`, `ratatui`.
- Produces:
  - `pub fn run_inspect(source: &str) -> Result<(), CliError>`
  - `pub fn run_inspect_with_tty(source: &str, is_tty: bool) -> Result<(), CliError>` — testable seam; `run_inspect` calls it with `std::io::stdout().is_terminal()`.
  - `fn handle_key(app: &mut App, key: crossterm::event::KeyCode)` — key → state transition (pure, testable).

- [ ] **Step 1: Write the failing test**

Add to `src/rust/cli/src/inspect/mod.rs` (module declarations stay at the top):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::CliError;

    #[test]
    fn non_tty_is_rejected_before_touching_the_terminal() {
        // A valid local file, but no TTY: must fail fast with NotATerminal,
        // never entering raw mode.
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../conformance/inferable_types.fcb");
        let err = run_inspect_with_tty(path, false);
        assert!(matches!(err, Err(CliError::NotATerminal)));
    }

    #[test]
    fn quit_keys_set_should_quit() {
        use crate::inspect::app::App;
        use crossterm::event::KeyCode;

        let mut app = App::new(3);
        handle_key(&mut app, KeyCode::Char('q'));
        assert!(app.should_quit);

        let mut app = App::new(3);
        handle_key(&mut app, KeyCode::Esc);
        assert!(app.should_quit);
    }

    #[test]
    fn arrow_and_vim_keys_drive_navigation() {
        use crate::inspect::app::{App, Tab};
        use crossterm::event::KeyCode;

        let mut app = App::new(3);
        handle_key(&mut app, KeyCode::Tab);
        assert_eq!(app.tab, Tab::Columns);
        handle_key(&mut app, KeyCode::Char('j'));
        assert_eq!(app.column_offset, 1);
        handle_key(&mut app, KeyCode::Char('k'));
        assert_eq!(app.column_offset, 0);
        handle_key(&mut app, KeyCode::Char('G'));
        assert_eq!(app.column_offset, 2);
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd src/rust && cargo test -p fcb_cli inspect:: 2>&1 | tail -20`
Expected: FAIL — `run_inspect_with_tty` / `handle_key` not found.

- [ ] **Step 3: Write the minimal implementation**

Replace the contents of `src/rust/cli/src/inspect/mod.rs` above the test module with:

```rust
//! Interactive terminal UI for inspecting an FCB dataset header.

pub mod app;
pub mod map;
pub mod model;
pub mod source;
pub mod ui;

use std::io::{self, IsTerminal, Stdout};
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::execute;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::inspect::app::App;
use crate::CliError;

/// Restores the terminal (leave alt-screen, disable raw mode) on drop, so an
/// error or panic never leaves the user's terminal broken.
struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalGuard {
    fn enter() -> Result<Self, CliError> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let terminal = Terminal::new(CrosstermBackend::new(stdout))?;
        Ok(TerminalGuard { terminal })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

/// Apply a single key press to the app state.
fn handle_key(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
        KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => app.next_tab(),
        KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => app.prev_tab(),
        KeyCode::Down | KeyCode::Char('j') => app.scroll_down(),
        KeyCode::Up | KeyCode::Char('k') => app.scroll_up(),
        KeyCode::Char('g') => app.to_top(),
        KeyCode::Char('G') => app.to_bottom(),
        _ => {}
    }
}

/// Public entry: inspect a local path or URL, driving a full-screen TUI.
pub fn run_inspect(source: &str) -> Result<(), CliError> {
    run_inspect_with_tty(source, io::stdout().is_terminal())
}

/// Testable seam: `is_tty` is injected so tests can assert the non-TTY guard
/// without a real terminal.
pub fn run_inspect_with_tty(source: &str, is_tty: bool) -> Result<(), CliError> {
    // Load the model first: cheap failures (bad path, bad URL) surface as plain
    // stderr errors rather than after switching into the alternate screen.
    let model = source::load_model(source)?;

    if !is_tty {
        return Err(CliError::NotATerminal);
    }

    let mut app = App::new(model.columns.len());
    let mut guard = TerminalGuard::enter()?;

    while !app.should_quit {
        guard.terminal.draw(|f| ui::draw(f, &model, &app))?;
        // Poll so a resize/redraw stays responsive without busy-spinning.
        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    handle_key(&mut app, key.code);
                }
            }
        }
    }
    Ok(()) // guard's Drop restores the terminal
}
```

- [ ] **Step 4: Wire the subcommand in `main.rs`**

In `src/rust/cli/src/main.rs`, add a variant to `enum Commands` (after `Info { .. }`):

```rust
    /// Interactively inspect an FCB file or URL in a terminal UI
    Inspect {
        /// Local path or HTTP(S) URL to an FCB file
        source: String,
    },
```

Add a match arm in `main()` (after the `Commands::Info` arm):

```rust
        Commands::Inspect { source } => fcb_cli::inspect::run_inspect(&source)?,
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd src/rust && cargo test -p fcb_cli 2>&1 | tail -25`
Expected: PASS — all `inspect::*` tests plus the existing `verify_cli` pass; 0 failures.

- [ ] **Step 6: Full build + clippy + fmt**

Run: `cd src/rust && cargo build -p fcb_cli && cargo clippy -p fcb_cli --all-targets -- -D warnings && cargo fmt -p fcb_cli -- --check`
Expected: clean build, no clippy warnings, formatting OK. Fix any issues and re-run.

- [ ] **Step 7: Manual smoke test**

Run: `cd src/rust && cargo run -p fcb_cli -- inspect ../../conformance/inferable_types.fcb`
Expected: TUI opens; `Tab`/arrows switch tabs; `j`/`k` scroll Columns; `q` quits and the terminal is restored cleanly. Then confirm the non-TTY guard:

Run: `cd src/rust && cargo run -p fcb_cli -- inspect ../../conformance/inferable_types.fcb | cat`
Expected: prints the "requires an interactive terminal" message and exits non-zero (no escape-code garbage in the pipe).

- [ ] **Step 8: Commit**

```bash
git add src/rust/cli/src/inspect/mod.rs src/rust/cli/src/main.rs
git commit -m "feat(cli): wire fcb inspect subcommand with event loop + tty guard

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Final Verification

- [ ] `cd src/rust && cargo test -p fcb_cli` — all tests pass.
- [ ] `cd src/rust && cargo clippy -p fcb_cli --all-targets -- -D warnings` — clean.
- [ ] `cd src/rust && cargo fmt --check` — clean.
- [ ] `fcb inspect <local .fcb>` opens the TUI; all three tabs render; quit restores the terminal.
- [ ] `fcb inspect <https url to .fcb>` opens the TUI (header fetched over range requests).
- [ ] `fcb info` still works unchanged.
- [ ] Spec parity: Metadata/Columns/Map tabs match the (corrected) spec; no invented CRS/geometry fields.

## Notes for the implementer

- **Ratatui/crossterm versions:** the plan pins `ratatui = "0.29"` / `crossterm = "0.28"` (compatible pair). If the workspace resolves a newer compatible minor, keep the APIs used here (`Frame::area`, `Tabs::select`, `Canvas` with `Points`/`Rectangle`, `TestBackend`) — all stable across 0.29.x. If a version bump breaks an API, prefer adjusting the call site over downgrading.
- **`is_terminal`:** `std::io::IsTerminal` is stable (Rust 1.70+); no extra crate needed.
- **Coastline asset:** if Step 2 of Task 3 cannot reach the network, any public-domain coastline GeoJSON with `LineString`/`MultiLineString` features works; the test only requires ≥ 1000 in-bounds points. The CSV is committed so no later build needs the network.
