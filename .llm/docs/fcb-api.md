# API operational contract

See `src/rust/fcb_api/README.md` for configuration and response migration.
Application construction is fallible and accepts explicit `AppConfig` for tests.
Metadata is an immutable startup snapshot decoded with the Rust oracle.
Datasets must not change during a process lifetime; restart for replacement.
`/readyz` performs fresh dataset header access rather than inspecting the cache.

API tests generate actual FCB files through FcbWriter and serve identical bytes
via local files and a loopback HTTP range server. Tests compare complete JSON
values with Rust reader output and shared corpus metadata. No public dataset
is required. CRS tests use standard EPSG:4326 latitude/longitude axis order and
CRS84 longitude/latitude. Storage support is EPSG:7415 or EPSG:28992; geometry
output reprojection and combined bbox/filter selection are unsupported.

This work deliberately excludes local concurrency refactoring and the
advertised documentation endpoints/schema, so it makes no new formal OGC
conformance claim. See docs/upstream-findings.md for reader defects found by
the API tests and their fixes.
