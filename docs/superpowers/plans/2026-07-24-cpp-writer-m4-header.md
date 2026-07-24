# C++ writer M4: header FlatBuffer serialization — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans.

**Goal:** Port `to_fcb_header` and its helpers (`writer/serializer.rs`, header half) plus `HeaderWriterOptions` (`writer/header_writer.rs`) to C++.

**Architecture:** `include/fcb/writer/header_serializer.hpp` + `src/writer/header_serializer.cpp`. Reuses M1's `to_columns`, M3's `to_appearance`/`to_geometry`/`to_geographical_extent`.

## Global Constraint learned the hard way in M3, applies doubly here

**Every `fbb.CreateString`/`CreateVector` call must be a separately sequenced named statement, in the EXACT order Rust's own `let` bindings run, never an inline call argument.** `to_fcb_header` has ~15 such calls in sequence (version; columns; semantic_columns; attribute_index; extensions; appearance; templates_vertices then templates; then, if metadata present: reference_system, identifier, reference_date, title, then point-of-contact's own internal sequence of 6 scalar fields then 5 address fields). `HeaderBuilder::add_*` call order, by contrast, does NOT affect the final vtable bytes (FlatBuffers' vtable construction is field-driven, not call-order-driven) so those can be called in any convenient order once every child offset already exists — only the CHILD CREATION order is load-bearing.

`metadata.referenceSystem` is a raw URL string in CityJSON (`https://www.opengis.net/def/crs/{authority}/{version}/{code}`, per cjseq2's `ReferenceSystem::from_url`); this milestone parses it directly (strip either `http://`/`https://www.opengis.net/def/crs/` prefix, split remaining `/`-segments, `authority=segments[0]`, `version`/`code` = segments 1/2 parsed as `i32`, defaulting to 0 on parse failure -- matching Rust's `.parse::<i32>().ok().unwrap_or(0)` exactly, including that Rust's `parse` rejects trailing garbage, so use `std::from_chars` with a full-string-consumed check, not `std::stoi`, which does not). A string not matching either prefix is treated as absent reference_system (Rust's `TryFrom<String>` would fail the WHOLE document's deserialization in that case; replicating that document-level failure is out of scope here, so this milestone is intentionally more lenient on this one malformed-input edge, disclosed rather than silently matched).

## Tasks

1. `HeaderWriterOptions` + `AttributeIndexInfo` structs (plain data, no fbb) + `to_transform` (pure `Transform` struct) + `to_geographical_extent` reused from M3.
2. `parse_reference_system` (URL parsing per above) + `to_reference_system` (builds the FlatBuffers table) + `to_extension` + `to_templates_vertices`.
3. `to_point_of_contact` (contact_name/type/role/phone/email/website in that order, THEN address fields thoroughfare_number/name/locality/postcode-or-postalCode/country -- mirrors `writer/serializer.rs:319-369` exactly, including the "postcode wins over postalCode when both present" `.or_else` order and the non-string-address-member `.to_string()` fallback).
4. `to_fcb_header` main orchestration, in the exact sequenced order above.
5. Byte-exact oracle: extend `test_writer_oracle.cpp` to compare this milestone's header bytes against `conformance/single_feature.fcb`'s header section (bytes `[8+4 .. 8+4+header_size)`), for the SAME options that fixture was written with (features_count=1, no spatial index means `index_node_size=0`, no attribute indices, no `-g` extent flag).

Testing throughout: unit tests reading results back via generated FlatBuffers accessors (matching M1-M3's pattern), plus the Task 5 byte-exact oracle as the final check.
