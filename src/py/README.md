# flatcitybuf (pure Python)

A from-scratch, pure-Python reader for [FlatCityBuf](../../README.md), a
cloud-optimized binary format for storing and retrieving 3D city models with
full CityJSON compatibility.

This package has no compiled dependency: it builds a `py3-none-any` wheel and
runs on plain CPython, no Rust toolchain required. It replaces the PyO3
extension at `src/rust/fcb_py`.

## Status

Under construction. Only the error taxonomy (`flatcitybuf.errors`) exists so
far; layout parsing, feature iteration, and spatial/attribute queries land in
later tasks.

## Development

```bash
cd src/py
uv sync --extra dev
uv run pytest
uv run ruff check .
uv run mypy
```

## Requirements

- Python >= 3.9
- `flatbuffers` (pure-Python runtime)

Optional extras:

- `numpy` — bulk vertex decoding
- `http` — async HTTP range reads (via `httpx`)
