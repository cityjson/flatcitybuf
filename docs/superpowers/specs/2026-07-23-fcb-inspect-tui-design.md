# `fcb inspect` — interactive terminal UI for dataset overview

**Date:** 2026-07-23
**Branch:** `feat/fcb-inspect-tui` (worktree, based on `develop`)
**Status:** design approved, pending spec review

## Motivation

Inspecting an `.fcb` dataset today means running `fcb info`, which prints a
static block to stdout. Inspired by [`fgbdump`](https://github.com/C-Loftus/fgbdump),
we add `fcb inspect <path|url>`: an interactive terminal UI that reads only the
header and presents a navigable overview — metadata, the column schema, and a
world map of the dataset's extent. It reads the header only, so it stays fast
even for large files and works over HTTP range requests without downloading the
body.

## Scope

A new subcommand `fcb inspect <source>` where `<source>` is a local path **or**
an HTTP(S) URL. It opens a full-screen TUI with three tabs:

- **Metadata** — title/identifier (when present), FCB version, features count,
  column count, bounds (geographical extent min/max + width×height×depth),
  reference date, spatial-index R-tree node size, attribute-index count,
  transform (scale/translate), and the CRS block. Sourced from `Header`,
  `geographical_extent`, `transform`, and `ReferenceSystem`.

  **The CRS block shows only what the FCB header actually stores:** authority,
  code (rendered `AUTHORITY:CODE`, e.g. `EPSG:4326`), version, and code string.
  FCB's `ReferenceSystem` has **no** CRS name, WKT, or description fields, and
  the header has **no** geometry-type or M/Z/T/TM dimension flags (those are
  per-object, not header-level). The fgbdump screenshots show FlatGeobuf fields
  that do not exist in FCB; we do not invent them.
- **Columns** — a scrollable table with columns: Name / Type / Description /
  Nullable / Primary Key / Unique. Backed by the `Column` accessors
  (`name`, `type_`, `description`, `nullable`, `primary_key`, `unique`).
- **Map** — a world-coastline background with the file's extent drawn as a green
  box, **rendered only when the CRS is geographic** (lat/lon). For a projected
  CRS, the tab shows a "map unavailable — projected CRS" note plus the numeric
  extent. No reprojection library (PROJ) is pulled in.

The existing `fcb info` command is **unchanged**. `inspect` is the interactive
companion; `info` remains the static, pipeable output for scripts.

### Explicitly out of scope (YAGNI)

- Reprojection of arbitrary projected CRS onto the WGS84 map (would require a
  PROJ dependency). Deferred; geographic CRS only for v1.
- Reading or visualizing feature data / feature-level density. The map shows the
  header extent box only, matching `fgbdump`.
- Editing, querying, or exporting from the TUI. Read-only overview.

## Architecture

All code lives in the existing `cli` crate (the format library `fcb_core` stays
TUI-free and already exposes everything needed via `Header`). New module
`cli/src/inspect/`, dispatched from a new `Commands::Inspect { source: String }`
arm in `main.rs`.

Dependencies added to the workspace `Cargo.toml` and referenced by the `cli`
crate with `{ workspace = true }`: `ratatui` and `crossterm` (the terminal
backend; `ratatui` re-exports it).

### Module layout

| File | Responsibility | Tested |
|---|---|---|
| `inspect/mod.rs` | Entry `run_inspect(&str)`: resolve source → build model → run event loop → restore terminal. Thin orchestration. | smoke |
| `inspect/source.rs` | Path-vs-URL detection; fetch the header. Local: `FcbReader::open`. URL: `HttpFcbReader::open` inside a short-lived `tokio` runtime via `block_on`. | path detection |
| `inspect/model.rs` | `InspectModel` — an **owned** snapshot built from the borrowed `Header` (metadata scalars, `Vec<ColumnInfo>`, CRS struct, extent). Decouples rendering from FlatBuffers lifetimes. | from corpus fixture |
| `inspect/app.rs` | App state: active tab, column-table scroll/selection offset; key handling and state transitions. | pure state transitions |
| `inspect/ui.rs` | `ratatui` rendering of the three tabs from `InspectModel` + `App` state. | `TestBackend` snapshot |
| `inspect/map.rs` | Pure geometry: geographic-CRS gate, equirectangular projection (lon/lat → canvas cell), coastline plotting, extent-box drawing. | unit |
| `cli/assets/coastline.csv` | Decimated Natural Earth 110m coastline as `lon,lat` lines, embedded via `include_str!`. | — |

### Lifetime handling

`Header` borrows the reader's underlying buffer. `model.rs` copies every field
it needs into an owned `InspectModel` **before the reader is dropped**. This is
what lets the local and HTTP code paths converge on a single owned model and
keeps `ui.rs` and `map.rs` free of borrow entanglement.

### Async handling

The TUI event loop is fully synchronous (`crossterm` events). Only the URL
header-fetch is async, wrapped in a one-shot
`tokio::runtime::Runtime::new()?.block_on(...)` in `source.rs`. Local-file
inspection never constructs a runtime.

## Data flow

```
resolve source ──► fetch Header (local: sync | URL: block_on)
                        │
                        ▼
               build owned InspectModel ──► drop reader
                        │
                        ▼
  enter alt-screen/raw mode ──► loop { draw(model, app); handle key } ──► restore
```

## Interactivity

Keybindings:

- `Tab` / `←` `→` / `h` `l` — switch tab
- `↑` `↓` / `j` `k` — scroll the Columns table
- `g` / `G` — jump to top / bottom of the Columns table
- `q` / `Esc` / `Ctrl-C` — quit

**Non-TTY guard:** if stdout is not a terminal, `inspect` prints
"interactive TUI requires a terminal; use `fcb info` for static output" and
exits non-zero, rather than emitting escape sequences into a pipe.

## Map rendering (geographic CRS only)

`map.rs` holds pure, unit-testable functions:

- **CRS gate:** treat the dataset as geographic when the `ReferenceSystem` code
  is a known geographic EPSG (e.g. 4326, 4979) **and** the extent lies within
  lon/lat bounds (`|lon| ≤ 180`, `|lat| ≤ 90`). If there is no reference system,
  fall back to the extent-bounds check alone. Otherwise render the projected-CRS
  fallback note.
- **Projection:** equirectangular. `lon ∈ [-180, 180] → x`,
  `lat ∈ [90, -90] → y`, mapped onto a braille canvas sized to the panel. Plot
  the embedded coastline points first, then draw the file's extent as a green
  rectangle, clamped to the canvas. Implemented with `ratatui`'s `Canvas`
  widget and braille markers.

## Error handling

Errors flow through the existing `CliError` (`thiserror`). Add variants as
needed — at least `NotATerminal`; reuse existing IO/HTTP error variants for
fetch failures. Terminal state is **always restored** on error via an RAII
guard (restore raw mode + leave alt-screen on drop), so a mid-run failure never
leaves the user's terminal broken.

## Testing

- `model.rs`: build an `InspectModel` from a conformance-corpus `.fcb` fixture
  and assert the owned fields match the header (extent, column count, CRS,
  indices).
- `map.rs`: projection of known lon/lat points to expected canvas cells;
  extent-box corner placement; coastline points all fall within canvas bounds;
  geographic-vs-projected gate decisions.
- `app.rs`: key-transition tests (tab cycling wraps correctly) and
  scroll-clamp tests (can't scroll past the ends of the column list).
- `ui.rs`: one `TestBackend` render smoke test asserting the tab titles and a
  couple of known field labels appear in the rendered buffer.
- `main.rs`: the existing `verify_cli()` `debug_assert` covers the new clap arm.

## Assets

`cli/assets/coastline.csv` is a decimated Natural Earth 110m coastline as
newline-delimited `lon,lat` records, embedded with `include_str!`. A short note
in the file header records its provenance and how to regenerate it. Target size:
tens of KB.
