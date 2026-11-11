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

A `pand` item includes all its building parts. The shared `build_feature_links`
advertises only self and collection: there are no independently addressable
building-part resources. Object children/parents relationships remain in the
returned CityJSONFeature. Self retains the requested ID and CRS query parameter;
IDs are escaped as individual URL path segments. `/conformance` matches Flask's
CityJSON 2.0.1 declaration. The existing identificatie-based building lookup
contract remains unchanged.

The root response advertises only self, conformance, and collections; the
unimplemented documentation endpoints are not linked.

This work deliberately excludes local concurrency refactoring and the
planned documentation endpoints/schema, so it makes no new formal OGC
conformance claim. See docs/upstream-findings.md for reader defects found by
the API tests and their fixes.
