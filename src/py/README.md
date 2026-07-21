# flatcitybuf (pure Python)

A from-scratch, pure-Python reader for [FlatCityBuf](../../README.md), a
cloud-optimized binary format for storing and retrieving 3D city models with
full CityJSON compatibility.

This package has no compiled dependency: it builds a `py3-none-any` wheel and
runs on plain CPython, no Rust toolchain required. It replaces the PyO3
extension formerly at `src/rust/fcb_py`, which has been deleted (Task 13).

## Status

Complete. `FcbReader.open_file(path)` / `.select_all()` / `.select_attr(...)`,
`to_cityjson_metadata` / `to_cityjson_feature`, `search_rtree` / `search_stree`,
and `HttpRangeReader` / `FileRangeReader` / `BufferedRangeReader` all exist and
are covered by 247 tests, including a 10/10 conformance suite comparing this
reader's output against the Rust reader's on the same bytes
(`tests/test_conformance.py`).

## Breaking changes from the retired PyO3 extension (`src/rust/fcb_py`)

The Python import name is still `flatcitybuf`, but the API underneath is a
new, from-scratch design, not a drop-in replacement. If you used the old
PyO3 bindings:

- **No async API.** The old `AsyncReader` / `AsyncFeatureIterator`
  (`await reader.open()`, `await async_iter.next()`, persistent HTTP
  connections, async streaming) do not exist here and were deliberately not
  built. `HttpRangeReader` reads over HTTP too, but synchronously (blocking
  `urllib.request`, one or more Range GETs per `read()` call) -- there is no
  `asyncio` story. This is a real capability regression, not an oversight.
- **`Reader(path)` → `FcbReader.open_file(path)`.** A classmethod, not a
  constructor; `Reader.info()` → the `.header` attribute (a `HeaderView`),
  and `Reader.cityjson_header()` → the module-level `to_cityjson_metadata(header)`.
- **`AttrFilter(field, Operator.Eq, value)` → `AttrCondition(column, Operator.EQ, value)`.**
  The operator enum is spelled in upper case (`EQ`/`NE`/`GT`/`GE`/`LT`/`LE`,
  not `Eq`/`Ne`/...), and `value` must be a typed `KeyValue`
  (`KeyValue.from_u64(...)`, `.from_string(...)`, etc.) rather than a raw
  Python `int`/`str`.
- **`BBox(min_x=..., ...)` → a plain `(min_x, min_y, max_x, max_y)` tuple**,
  passed positionally to `search_rtree`.
- **Spatial and attribute queries no longer hand back decoded features.**
  `Reader.query_bbox(...)` / `Reader.query_attr([...])` used to return
  `Iterator[Feature]` directly. The new `search_rtree(...)` /
  `FcbReader.select_attr(...)` both return
  `list[SearchResultItem(offset, index)]` -- feature-section-relative
  offsets, not materialized features. There is currently no public
  `select_bbox` that both filters and decodes in one call.
- **No module-level convenience functions.** The old `fcb.open_file(path)` /
  `fcb.query_bbox(path, ...)` free functions returning `list[Feature]` are
  gone; construct an `FcbReader` explicitly.
- **Feature/CityObject/Geometry are no longer PyO3 classes with attribute
  access.** `to_cityjson_feature(feature, header)` returns a plain CityJSON
  dict (matching the CityJSON spec's own shape) rather than instances of a
  `Feature`/`CityObject`/`Geometry` class hierarchy.
- **Installation is simpler, not harder.** `pip install flatcitybuf` now
  installs one universal `py3-none-any` wheel; there is no Rust toolchain,
  `maturin`, or per-platform wheel to worry about.

## Development

```bash
cd src/py
uv sync --extra dev --extra numpy --extra http
uv run pytest
uv run ruff check .
uv run mypy
```

`numpy` and `http` are optional at runtime (every code path has a working
pure-Python/sync fallback when they are absent, and `pytest` passes either
way), but `mypy --strict` needs the `numpy` extra actually installed to
type-check the bulk-decode branches in `cityjson.py`/`feature.py` --
without it, `import numpy` inside those functions cannot be resolved and
mypy reports `import-not-found` there instead of passing cleanly.

## Requirements

- Python >= 3.9
- `flatbuffers` (pure-Python runtime)

Optional extras:

- `numpy` — bulk vertex decoding
- `http` — async HTTP range reads (via `httpx`)

## Benchmark

The nativeness decision (a pure-Python reader instead of the PyO3 extension at
`src/rust/fcb_py`) was made knowing Python would be slower; the point of this
section is that the factor is *measured*, not guessed. The measurement itself
is reproducible via `test_benchmark.py`, which is excluded from the default
`pytest` run (see "Running the benchmark yourself" below) because it is slow
and timing-sensitive.

**Workload:** a full scan of `examples/data/delft.fcb` (1115 real-world
building features) — open the file, iterate every feature with
`FcbReader.select_all()`, and convert each one to its CityJSON dict with
`to_cityjson_feature`. This is the same work `test_conformance.py` does per
file, just on a bigger file and timed instead of asserted against.

**Machine / method:** Apple M4 Max, macOS 26.5.2, CPython 3.11.14 (the
project's `.venv`), numpy 2.4.6. One untimed warm-up run (to prime the OS page
cache and pay one-time import costs) followed by 5 timed repetitions in the
same process; the table reports both the minimum (least noise from the
scheduler/GC) and the mean.

| Path | min | mean | vs. pure Python |
| --- | --- | --- | --- |
| pure Python | 1.246 s | 1.255 s | 1.0x (baseline) |
| pure Python + numpy | 0.524 s | 0.531 s | ~2.4x faster |
| PyO3 (`src/rust/fcb_py`), release build | 0.039 s | 0.040 s | ~32x faster |

The PyO3 row was **not** measured in this project's own environment: its
compiled extension installs under the Python import name `flatcitybuf` —
the same name this pure-Python package uses — so the two cannot coexist in
one environment; installing it here would have overwritten (or fought with)
the very package under test. It was instead measured in a throwaway venv
(`python3 -m venv`, `pip install maturin`, `maturin develop --release`, a
~30s build against the workspace's already-warm `target/` cache), iterating
the reader and materializing each feature's `id`/`vertices`/`city_objects`.
That is not a byte-for-byte identical workload to `to_cityjson_feature`
(PyO3's reader already hands back live Python objects rather than building
nested dicts/lists the way CityJSON emission does), so treat the PyO3 number
as indicative of the ballpark, not a precise 32.00x. It is skipped entirely
if `fcb_py`/`flatcitybuf`'s compiled extension is not importable and maturin
is not already available — Task 13 retires this crate, so no time went into
making it buildable here.

**Where the time goes (pure Python):** profiling shows the geometry
uint-vector fields (`boundaries`, `solids`, `shells`, `surfaces`, `strings`,
`semantics`) dominate, not vertex decoding — millions of individual
FlatBuffers per-element accessor calls for a handful of large arrays per
feature. The numpy path in `flatcitybuf.cityjson._uint_list` calls the
FlatBuffers-generated `XxxAsNumpy()` accessor for these fields (which already
wraps `numpy.frombuffer` internally) instead of looping; `Feature.vertices()`
in `flatcitybuf.feature` does the equivalent by hand (`Vertex` is a struct,
so flatc does not generate an `AsNumpy()` accessor for it). Both numpy paths
have a working pure-Python fallback when numpy is not installed, and
`test_cityjson.py::test_numpy_and_pure_python_paths_agree*` proves the two
produce byte-identical output rather than assuming it.

### Running the benchmark yourself

```bash
cd src/py
uv sync --extra dev --extra numpy   # numpy is optional; omit to measure
                                     # the pure-Python path only
uv run pytest tests/test_benchmark.py -m benchmark -v -s
```
