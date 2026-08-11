# FlatCityBuf — guidance for coding agents

`AGENTS.md` is a symlink to this file, so both paths serve the same guidance.

FlatCityBuf is a cloud-optimized binary format for 3D city models: CityJSON's
semantics in FlatBuffers, with a packed Hilbert R-tree for spatial queries, a
static B+tree for attribute queries, and HTTP range requests so a client fetches
only the bytes it needs.

## The one thing to understand first

The repo holds **four independent reader implementations of the same format**:

| Implementation | Location | Status |
|---|---|---|
| **Rust** — the origin, and the **authoritative oracle** | `src/rust/fcb_core` | reader + writer |
| C++ | `src/cpp` | reader + writer, conformant |
| Python (pure, `py3-none-any`) | `src/py` | reader, conformant |
| TypeScript | `src/ts` | reader, conformant |

Rust and C++ write files; Python and TypeScript are read-only. All four are
from-scratch ports with **no FFI** — they parse (and, for Rust/C++, produce)
the bytes directly. When two implementations disagree, **Rust is right by
definition**, and the disagreement is a defect in the other one until proven
otherwise — the C++ writer's own test suite enforces this by comparing its
output against real Rust-written files byte-for-byte
(`src/cpp/tests/test_writer_oracle.cpp`), not just by decoding correctly.

`conformance/` is the shared oracle corpus: hand-authored inputs, the `.fcb`
binaries, and `.expected.jsonl` holding *the Rust reader's own output* for each.
Every port validates against it. The `.fcb` files are tracked so a clean
checkout can run the suites with no Rust toolchain.

`docs/upstream-findings.md` is the permanent record of defects found across the
implementations — each cited to source, with a reproduction and its fix state.
Porting has repeatedly surfaced real bugs in the *existing* readers; when you
find one, it goes there.

## Commands

`just` is the task runner, and **every language directory has its own justfile
exposing the same five verbs**. The root justfile fans each one out across
`src/rust`, `src/cpp`, `src/py`, `src/ts`, `examples/web` — in that order,
because `examples/web` consumes `src/ts/dist`.

```bash
just check    # lint + type + test + build, everywhere, read-only
just test     # tests only, everywhere
just lint     # linters and format checks only
just type     # cargo check / compiler / mypy --strict / tsc --noEmit
just build    # builds only
just fix      # rustfmt, clippy --fix, ruff, clang-format — MUTATES the tree
```

The same verbs work inside any one language:

```bash
cd src/py  && just test        # just this suite
cd src/cpp && just test-http   # the libcurl adapter, in its own build tree
cd src/ts  && just test-browser
```

`just --list` (root or any subdirectory) shows everything, including the
per-language extras (`src/rust`: `ser`/`deser`/`inspect`/`bench`; `src/cpp`:
`tidy`/`harden`; `src/py`: `test-no-numpy`).

Never gate on `just fix` output: it is the only recipe that rewrites source.
`check` never modifies a file.

Python tooling is `uv`; type-checking is `mypy --strict`; linting is `ruff` at
line length 79 with `E501` enabled.

Full manual verification procedure, local and remote: `docs/TESTING.md`.

## Conventions that bite

- **Little-endian, always.** Use explicit `<` formats in every `struct` call —
  never native `@`. Signed vs unsigned must be right at decode time: a `u64`
  read as signed goes negative past 2^63 and indexes backwards.
- **`u32::MAX` (4294967295) means null** in semantics values and appearance
  index arrays. Nothing in Python or TypeScript makes that obvious.
- **Optional FlatBuffers scalars need a vtable check.** Where a `.fbs` field is
  `= null`, generated Python/TS accessors return the *default* on absence, so
  "value 0" and "absent" look identical. Getting this wrong silently invents
  data.
- **Attribute schemas are per object.** `CityObject.columns` overrides
  `Header.columns` and that is the normal case. Attribute blobs are not
  self-delimiting, so a wrong schema yields plausible garbage, not an error.
- **The conformance corpus is not byte-reproducible.** `cjseq2` iterates
  CityObjects from a `HashMap`, so every regeneration produces different bytes
  for identical data. Never assert on a physical byte offset in a fixture;
  derive it at runtime.

## Working style

- If a test fails more than twice in a row, stop and form a hypothesis with the
  user. No trial-and-error.
- Compare against the corpus **and** round-trip through the Rust writer. Two
  readers agreeing on a wrong answer is a real failure mode here — it has
  happened, and comparing only selected JSON keys is what hid it. Compare whole
  lines.
- Plans for multi-step work live in `docs/superpowers/plans/`; design documents
  in `docs/superpowers/specs/`. Delete them once the work ships, moving anything
  durable into `.llm/docs/` first.
- `gh` CLI is available — use it for PRs and issues rather than an MCP server.

## Reference documents

- `docs/specification.md` — the format, from schema level down to byte
  offsets, constants and formulas, each cited to the Rust line that proves it.
  **Read this before writing any decoder; do not re-derive the format.**
- `.llm/docs/projectStructure.md` — folder layout and component relationships.
- `.llm/docs/productContext.md` — why the project exists and what it optimizes
  for.
