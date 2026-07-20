# FlatCityBuf Workspace Justfile
#
# Unified task runner for entire FlatCityBuf workspace (Rust, C++, Python, WASM)

# Default recipe - list all available commands
default:
    @just --list

# ============================================================================
# Workspace Commands
# ============================================================================

# Run all pre-commit checks (Rust format, clippy, WASM, Python)
pre-commit: check-common check-wasm check-py pre-commit-cpp

# Common workspace checks (Rust workspace format, clippy, test, build)
check-common:
    cd src/rust && cargo fmt
    cd src/rust && cargo clippy --fix --allow-dirty --workspace --all-targets --all-features --exclude fcb_wasm --exclude fcb_py
    cd src/rust && cargo clippy --fix --allow-dirty -p fcb_wasm --target wasm32-unknown-unknown
    cd src/rust && cargo nextest run --all-features --workspace --exclude fcb_wasm --exclude fcb_py
    cd src/rust && cargo check --all-features --workspace --exclude fcb_wasm --exclude fcb_py
    cd src/rust && cargo build --workspace --all-features --exclude fcb_wasm --exclude fcb_py

# Run WASM-specific checks
check-wasm:
    cd src/rust && cargo clippy --fix --allow-dirty -p fcb_wasm --target wasm32-unknown-unknown
    cd src/rust && cargo check -p fcb_wasm --target wasm32-unknown-unknown
    cd src/rust && cargo build -p fcb_wasm --target wasm32-unknown-unknown

# Run Python-specific checks
check-py:
    cd src/rust/fcb_py && uv sync --extra dev
    cd src/rust/fcb_py && uv run maturin develop
    cd src/rust/fcb_py && uv run ruff check --fix .
    cd src/rust/fcb_py && uv run ruff format .
    cd src/rust/fcb_py && uv run pytest tests/

# Run tests for the pure-Python package (src/py)
py-test:
    cd src/py && uv run --extra dev pytest

# Run linter for the pure-Python package (src/py)
py-lint:
    cd src/py && uv run --extra dev ruff check .

# Run C++ checks (native implementation)
pre-commit-cpp: check-cpp

# Build and test the native C++ core
check-cpp:
    cd src/cpp && cmake -B build-native -S . -DFCB_BUILD_TESTS=ON
    cd src/cpp && cmake --build build-native
    cd src/cpp && ctest --test-dir build-native --output-on-failure

# Regenerate the committed FlatBuffers C++ headers
gen-cpp-fbs:
    ./scripts/gen_cpp_flatbuffers.sh

# Regenerate the C++ conformance corpus (needs the Rust CLI)
gen-conformance:
    ./scripts/gen_conformance.sh

# Build and test the native C++ core with the HTTP adapter
check-cpp-http:
    cd src/cpp && cmake -B build-curl -S . -DFCB_WITH_CURL=ON -DFCB_BUILD_TESTS=ON
    cd src/cpp && cmake --build build-curl
    cd src/cpp && ./tests/run_http_tests.sh python3 tests/range_server.py \
        ../../examples/data ./build-curl/tests/fcb_tests

# Run all generation scripts in scripts directory
gen-all:
    @echo "Running all shell scripts in scripts..."
    @for script in scripts/*.sh; do \
        echo "Executing $script..."; \
        bash "$script"; \
    done
    @echo "All scripts executed."

# Build entire workspace (Rust + C++ + Python)
build-all: gen-all build check-cpp build-py

# ============================================================================
# Rust Commands
# ============================================================================

# Build Rust workspace
build:
    cd src/rust && cargo build

# Build Rust workspace with release optimizations
build-release:
    cd src/rust && cargo build --release

# Clean Rust build artifacts
clean:
    cd src/rust && cargo clean

# Run tests
test:
    cd src/rust && cargo test

# Run tests with output
test-verbose:
    cd src/rust && cargo test -- --nocapture

# Run clippy linter
clippy:
    cd src/rust && cargo clippy -- -D warnings

# Format code
fmt:
    cd src/rust && cargo fmt

# Check formatting without making changes
fmt-check:
    cd src/rust && cargo fmt --check

# Full CI check (format, clippy, test)
ci: fmt-check clippy test

# Update dependencies
update:
    cd src/rust && cargo update

# Check for security vulnerabilities
audit:
    cd src/rust && cargo audit

# Generate documentation
docs:
    cd src/rust && cargo doc --no-deps --open

# Install development tools
install-tools:
    cd src/rust && cargo install just cargo-watch wasm-pack cargo-audit

# Start dev container
devcon:
    devcontainer up --workspace-folder .
    devcontainer exec --workspace-folder . bash

# Rebuild dev container from scratch
devcon-build:
    devcontainer build --workspace-folder . --no-cache
    just devcon

# ============================================================================
# C++ Commands
# ============================================================================

# Clean and rebuild C++ bindings
clean-cpp:
    cd src/cpp && rm -rf build build-native build-curl build-asan

# ============================================================================
# Python Commands
# ============================================================================

# Sync Python dependencies
py-sync:
    cd src/rust/fcb_py && uv sync --extra dev

# Run Python development
py-dev:
    cd src/rust/fcb_py && uv run maturin develop

# Run Python linter (PyO3 extension, src/rust/fcb_py; retired in Task 13)
fcb-py-lint:
    cd src/rust/fcb_py && uv run ruff check --fix .

# Format Python code
py-fmt:
    cd src/rust/fcb_py && uv run ruff format .

# Run Python tests (PyO3 extension, src/rust/fcb_py; retired in Task 13)
fcb-py-test:
    cd src/rust/fcb_py && uv run pytest tests/

# Clean Python build artifacts
py-clean:
    cd src/rust/fcb_py && cargo clean

# Install Python package in development mode
py-develop:
    cd src/rust/fcb_py && maturin develop

build-py:
    cd src/rust/fcb_py && maturin build --release

# ============================================================================
# WASM Commands
# ============================================================================

# Build the WASM package into src/ts (debug). Same script CI publishes with,
# so a local build and a release produce the same tree. The output is
# gitignored; only src/ts/package.json is tracked.
build-wasm:
    ./scripts/build_wasm.sh --dev

# Build WASM for production
build-wasm-release:
    ./scripts/build_wasm.sh

# ============================================================================
# CLI Commands
# ============================================================================

# Run FCB info command on test data
fcb_info:
    cd src/rust && cargo run -p fcb_cli info -i fcb_core/tests/data/delft.fcb

# Generate file statistics (CSV output)
file-stats:
    cd src/rust && cargo run -p fcb_core --bin stats -- -d fcb_core/benchmark_data/ -f csv

# Run benchmarks
bench:
    cd src/rust && cargo bench -p fcb_core --bench read -- --release

# Build fcb_core release binary
build-fcb_core:
    cd src/rust && cargo build --release -p fcb_core
