# Next task: appearance + geometry templates in the C++ reader

Everything else in the native C++ reader is done and verified. Three things
remain undecoded. Start here.

## Branch

`develop` (21 commits, local-only, clean). Build and test:

```bash
cd src/cpp && cmake -B build-native -S . && cmake --build build-native
./build-native/tests/fcb_tests          # 105 cases, all green today
```

## What is missing

| Gap | Where it should be emitted |
|---|---|
| `geometry-templates` (templates + vertices-templates) | `to_cityjson_metadata()` in `src/cityjson.cpp` |
| Geometry `texture` mappings | `geometry_to_json()` in `src/cityjson.cpp` |
| Geometry `material` mappings | same |

The fixture that exercises all three is **`geom_temp`** — 3 templates, 338
template vertices, 2 materials, 2 textures, 1015 texture vertices, 15
`GeometryInstance`s. It is already in the conformance corpus.

## How you will know you are done

`src/cpp/tests/test_conformance.cpp` currently **strips** `texture` and
`material` from both sides before comparing (search for `strip_appearance`).
Delete that lambda and its two calls. The `geom_temp` case must then pass
against the Rust reader's output unchanged.

Line 0 of each `.expected.jsonl` is the metadata envelope; the test only
compares `type`/`version`/`transform` today. Extend it to compare
`geometry-templates` once you emit it.

## Reference implementation

Port from Rust, do not invent:

- `fcb_core/src/reader/geom_decoder.rs:416` — `decode_materials`
- `fcb_core/src/reader/geom_decoder.rs:595` — `decode_textures`
- `fcb_core/src/reader/deserializer.rs` — where templates land in the envelope

Schemas (`src/fbs/geometry.fbs`):

```
table MaterialMapping { theme, solids, shells, vertices, value }
table TextureMapping  { theme, solids, shells, surfaces, strings, vertices }
```

Header carries `templates: [Geometry]` and `templates_vertices: [DoubleVertex]`.

## Three things that will bite

1. **Same nesting machinery as boundaries.** `decode_boundaries()` in
   `src/geometry.cpp` already implements the dimensional hierarchy and is
   tested — reuse its shape rather than writing a second walker. Texture
   values nest exactly like `boundaries`; material values nest one level
   shallower (one index per surface, not per ring).

2. **Collapse applies only at the OUTERMOST level.** Inner levels always wrap.
   Getting this backwards produces output one level off that still looks
   structurally plausible. This already cost a round of failing tests on
   `decode_boundaries`; see the comment there.

3. **`u32::MAX` means "no value here" and must become JSON `null`**, not
   4294967295. Applies to both texture and material index arrays.

## Scope note

Rust does **not** emit the header `appearance` object (the `materials`,
`textures`, `vertices-texture` arrays) in its CityJSONSeq output, only the
per-geometry mappings that reference them. Match that for conformance. If you
want the arrays too, that is a deliberate extension beyond parity — say so
explicitly rather than letting the conformance test drift.

## Not in scope

Writing `.fcb` files. That still requires the Rust CLI.
