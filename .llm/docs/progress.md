# FlatCityBuf Go & Node.js Bindings - Progress

## Status: Phase 1 & 2 Implementation Complete

### Phase 1: Node.js Native Bindings (napi-rs) - DONE

| Task | Status | Files |
|------|--------|-------|
| Add workspace deps & member | Done | `src/rust/Cargo.toml` |
| Create nodejs crate | Done | `src/rust/nodejs/Cargo.toml` |
| Implement error handling | Done | `src/rust/nodejs/src/error.rs` |
| Implement type conversions | Done | `src/rust/nodejs/src/types.rs` |
| Implement FcbReader (HTTP) | Done | `src/rust/nodejs/src/reader.rs` |
| Implement FeatureIter | Done | `src/rust/nodejs/src/iter.rs` |
| Implement query types | Done | `src/rust/nodejs/src/query.rs` |
| Main entry point | Done | `src/rust/nodejs/src/lib.rs` |
| Update package.json | Done | `src/ts/package.json` |
| Build targets | Done | `justfile` |

**Design decisions:**
- Used `Mutex<AsyncFeatureIter>` for thread-safe async iteration (napi-rs doesn't allow `&mut self` in async)
- Reader caches metadata on open, re-opens HTTP connection per query (matches Python async pattern)
- Uses `serde-json` feature for seamless Rust ↔ JS object conversion

### Phase 2: Go Bindings (CGO + C staticlib) - DONE

| Task | Status | Files |
|------|--------|-------|
| Add fcb_go workspace member | Done | `src/rust/Cargo.toml` |
| Create fcb_go FFI crate | Done | `src/rust/fcb_go/Cargo.toml` |
| Implement C FFI layer | Done | `src/rust/fcb_go/src/lib.rs` |
| cbindgen config | Done | `src/rust/fcb_go/cbindgen.toml` |
| Auto-generate C header | Done | `src/go/include/fcb_core.h` |
| Go module | Done | `src/go/go.mod` |
| Go FFI wrapper | Done | `src/go/fcb/fcb.go` |
| Go types | Done | `src/go/fcb/types.go` |
| Go tests | Done | `src/go/fcb/fcb_test.go` |
| Build targets | Done | `justfile` |

**Design decisions:**
- Type-erased iterators (same pattern as C++ bindings) to avoid exposing Rust generics
- Error handling via error_out pointer parameters (idiomatic C FFI)
- Reader consumed on selection (ownership transfer to iterator)
- Go tests use standard `testing` package with table-driven patterns

### Phase 3: Tests, Examples & Documentation - DONE

| Task | Status | Files |
|------|--------|-------|
| Node.js integration tests | Done | `src/ts/test/node.test.mjs` |
| Node.js basic example | Done | `src/ts/examples/node-basic.mjs` |
| Node.js reference implementation | Done | `src/ts/examples/node-reference.mjs` |
| Node.js README | Done | `src/rust/nodejs/README.md` |
| Go tests | Done | `src/go/fcb/fcb_test.go` |
| Go basic example | Done | `src/go/cmd/example/main.go` |
| Go reference implementation | Done | `src/go/cmd/reference/main.go` |
| Go README | Done | `src/go/README.md` |

### Phase 4: Remaining Work

- [ ] Install napi-cli and test Node.js build (`napi build`)
- [ ] Install Go and run Go tests (`go test ./...`)
- [ ] Add GitHub Actions CI for both bindings
- [ ] Test WASM + Node.js conditional exports work correctly
- [ ] Benchmark performance vs Python bindings
- [ ] Go HTTP reader (requires async FFI - separate effort)
