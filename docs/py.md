# FlatCityBuf — Python

A from-scratch, pure-Python **reader** for FlatCityBuf. It parses the bytes
directly: no FFI, no compiled extension, no Rust toolchain — a single
`py3-none-any` wheel on CPython 3.9+, with `flatbuffers` as its only required
dependency. It replaces the PyO3 extension that used to live at
`src/rust/fcb_py`.

Source: [`src/py`](../src/py). Format: [specification.md](specification.md).

## Status

- **Reader only.** There is no Python writer; produce `.fcb` files with the
  Rust CLI ([rust.md](rust.md)) or the C++ writer ([cpp.md](cpp.md)).
- **Conformant.** The suite replays the ten shared corpus cases listed in
  `CASES` (`src/py/tests/test_conformance.py:23-34`) and compares this
  reader's *whole output line for line* against the Rust reader's own output
  on the same bytes — header line included, not a selected set of keys.
- **Synchronous only.** `HttpRangeReader` reads over HTTP range requests with
  stdlib `urllib.request`; there is no `asyncio` API, and the retired PyO3
  extension's `AsyncReader` was deliberately not ported. That is a real
  capability regression, recorded here rather than glossed over.

## Install

The distribution name is `flatcitybuf` (`src/py/pyproject.toml:6`).

```bash
pip install flatcitybuf     # or: uv pip install flatcitybuf
```

PyPI serves the pure-Python reader from `0.3.1` onwards; `0.2.0` and `0.1.2`
are the *retired PyO3 extension* — platform wheels with a different API.

From a checkout, with [uv](https://docs.astral.sh/uv/):

```bash
git clone https://github.com/cityjson/flatcitybuf
cd flatcitybuf/src/py
uv sync --extra dev --extra numpy   # dev environment; `just sync` does the same
uv run python -c "import flatcitybuf; print(flatcitybuf.__version__)"
```

## Reading a file

Everything below is re-exported from the top-level package; `flatcitybuf.__all__`
(`src/py/flatcitybuf/__init__.py:110-153`) is the public surface, and anything
not in it is internal and may change without notice.

```python
import json
import flatcitybuf as fcb

reader = fcb.FcbReader.open_file("city.fcb")

# The CityJSONSeq header line.
print(json.dumps(fcb.to_cityjson_metadata(reader.header)))

# Every feature, in stored (Hilbert) order.
for feature in reader.select_all():
    cj = fcb.to_cityjson_feature(feature, reader.header)

# Attribute query -> offsets -> features.
hits = reader.select_attr(
    [
        fcb.AttrCondition(
            "b3_dak_type",
            fcb.Operator.EQ,
            fcb.KeyValue.from_string(fcb.KeyKind.STRING50, "horizontal"),
        )
    ]
)
for hit in hits:
    cj = fcb.to_cityjson_feature(reader.feature_at(hit), reader.header)

# Spatial query, over the same opened file.
info, layout = reader.header.info, reader.header.layout
hits = fcb.search_rtree(
    reader.range_reader,
    layout.rtree_begin,
    info.features_count,
    info.index_node_size,
    (min_x, min_y, max_x, max_y),
)

# Over HTTP, byte-range by byte-range (synchronously).
remote = fcb.FcbReader.open(fcb.HttpRangeReader("https://example.com/city.fcb"))
```

Verified against `conformance/small.fcb` with the checked-out reader.
Entry points: `FcbReader.open_file` (`src/py/flatcitybuf/reader.py:133`),
`FcbReader.open` (`reader.py:146`), `range_reader` (`reader.py:162`),
`feature_at` (`reader.py:175`), `select_attr` (`reader.py:195`),
`select_all` (`reader.py:286`), `HttpRangeReader.__init__`
(`src/py/flatcitybuf/http_reader.py:111`).

### The shape of the API

- `select_all` scans sequentially and yields decoded `Feature` objects. The two
  **indexed** paths do not: `search_rtree` and `FcbReader.select_attr` both
  return `list[SearchResultItem(offset, index)]` — feature-section-relative
  byte offsets, sorted ascending — and `FcbReader.feature_at(hit)` is the
  public inverse that decodes one. There is no `select_bbox` that filters and
  decodes in one call.
- `search_rtree` is a **free function over a `RangeReader`**, not a method, so
  pass it `reader.range_reader` plus the geometry from `reader.header`.
- Bytes come from any `RangeReader`: `FileRangeReader`, `HttpRangeReader`, or
  `BufferedRangeReader` as a caching decorator over either.
- `to_cityjson_feature(feature, header)` returns a plain CityJSON dict, not a
  class hierarchy; `to_cityjson_metadata(header)` returns the CityJSONSeq
  header line.
- An `FcbReader` is **not thread-safe** (the underlying `RangeReader` is not):
  one reader per thread, or serialize access.
- `u32::MAX` (4294967295) means **null** in `semantics.values` and in the
  appearance index arrays. `to_cityjson_feature` already maps it to JSON
  `null`; code reading those arrays by any other route must do so itself.
- String index keys are truncated to 50 bytes, so `search_stree` returns
  *candidates* for string columns. `select_attr` re-checks each candidate
  against the full untruncated value — pass `exact_index_only=True` to skip
  that and take the raw candidates.

## Optional dependencies

Only three extras exist (`src/py/pyproject.toml:12-35`): `numpy`, `dev`,
`docs`. There is **no `http` extra** — `HttpRangeReader` uses stdlib
`urllib.request`, so remote reads need no third-party package. (An earlier
`http = ["httpx>=0.27"]` extra existed for the async reader that was never
built; it installed a dependency nothing imported.)

**numpy is genuinely optional.** It is never imported at module scope: both
call sites funnel through an `_import_numpy()` helper that returns `None` when
it is absent — `src/py/flatcitybuf/cityjson.py:133` (bulk uint-vector decoding
via the generated `…AsNumpy()` accessors) and
`src/py/flatcitybuf/feature.py:39` (`Feature.vertices`, hand-rolled because
`Vertex` is a struct and flatc emits no `AsNumpy()` for it). Every path has a
working pure-Python fallback, and
`tests/test_cityjson.py::test_numpy_and_pure_python_paths_agree*` proves the
two produce identical output. Installing it is a speed choice: on a full scan
of `examples/data/delft.fcb` (1115 features) the numpy path is roughly 2.4x
faster than pure Python (≈0.52 s vs ≈1.25 s, Apple M4 Max / CPython 3.11);
reproduce with `just bench` (`pytest -m benchmark`, excluded from the default
run because it is timing-sensitive).

`mypy --strict` passes in **both** states: the `ignore_missing_imports`
override for `numpy` at `src/py/pyproject.toml:71-73` is what lets a
numpy-less environment (CI's pure-Python job, `just test-no-numpy`) type-check
the same source without an `import-not-found` on `_import_numpy`'s body.

## Tooling and testing

Tooling is `uv`; type-checking is `mypy --strict` (`pyproject.toml:50-52`);
linting is `ruff` at line length 79 with `E501` explicitly enabled
(`pyproject.toml:75-84`). The machine-generated FlatBuffers bindings under
`flatcitybuf/generated` are excluded from both.

```bash
cd src/py
just check          # lint + type + test + test-no-numpy, read-only
just test           # pytest
just type           # mypy --strict
just lint           # ruff check + ruff format --check
just build          # the py3-none-any wheel
just test-no-numpy  # re-run mypy + pytest with numpy uninstalled, then restore it
just test-remote    # opt-in: the live 3DBAG HTTP test (~68 GB bucket)
just bench          # the timing suite
just docs           # pdoc -> src/py/docs/api (gitignored)
just fix            # ruff --fix + ruff format — the only recipe that MUTATES source
```

`test-no-numpy` exists because numpy is optional in earnest: whichever state
happens to be installed is otherwise the only one ever exercised. Both states
must pass, and CI runs both. Measured on this checkout: `255 passed, 1 skipped,
1 deselected` with numpy, `251 passed, 5 skipped, 1 deselected` without (the
numpy-parity tests `importorskip`; the deselected one is the benchmark).

Conformance fixtures live in the shared corpus at `conformance/` — `.fcb`
binaries written by the Rust writer plus `.expected.jsonl` holding the Rust
reader's own output. They are tracked in git, so the suite runs on a clean
checkout with no Rust toolchain. The Python suite reads them via
`src/py/tests/test_conformance.py`.

Full manual verification, local and remote: [TESTING.md](TESTING.md).

## Migrating from the PyO3 extension (0.2.0 and earlier)

The import name is still `flatcitybuf`, but the API underneath is a new design,
not a drop-in replacement. Anyone upgrading from PyPI 0.2.0 hits all of these:

| Old (PyO3, ≤0.2.0) | New (pure Python, 0.3.0) |
| --- | --- |
| `Reader(path)` | `FcbReader.open_file(path)` (classmethod) |
| `Reader.info()` | `reader.header` (a `HeaderView`) |
| `Reader.cityjson_header()` | `to_cityjson_metadata(reader.header)` |
| `AttrFilter(field, Operator.Eq, 1)` | `AttrCondition(column, Operator.EQ, KeyValue.from_u64(1))` |
| `BBox(min_x=…, …)` | a plain `(min_x, min_y, max_x, max_y)` tuple |
| `Reader.query_bbox(…)` → `Iterator[Feature]` | `search_rtree(…)` → offsets, then `feature_at` |
| `Reader.query_attr([…])` → `Iterator[Feature]` | `select_attr([…])` → offsets, then `feature_at` |
| `fcb.open_file(path)`, `fcb.query_bbox(path, …)` | gone; construct an `FcbReader` |
| `Feature`/`CityObject`/`Geometry` objects | `to_cityjson_feature(...)` → plain CityJSON dict |
| `AsyncReader`, `await async_iter.next()` | **no async API** (see Status) |
| per-platform wheels, `maturin`, Rust toolchain | one `py3-none-any` wheel |

Operator names are upper case now (`EQ`/`NE`/`GT`/`GE`/`LT`/`LE`), and query
values must be typed `KeyValue`s (`KeyValue.from_u64`, `.from_string(kind, …)`,
`.from_bool`, …) rather than raw Python `int`/`str`
(`src/py/flatcitybuf/keys.py:192-246`).

## See also

- [specification.md](specification.md) — the format, down to byte offsets.
- [TESTING.md](TESTING.md) — manual verification procedure.
- [upstream-findings.md](upstream-findings.md) — cross-implementation defects.
- [rust.md](rust.md) · [cpp.md](cpp.md) · [ts.md](ts.md) — the other implementations.
- [`src/py/README.md`](../src/py/README.md) — the PyPI registry page.
