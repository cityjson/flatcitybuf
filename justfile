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
    cargo fmt
    cargo clippy --fix --allow-dirty --workspace --all-targets --all-features --exclude fcb_wasm --exclude fcb_py
    cargo clippy --fix --allow-dirty -p fcb_wasm --target wasm32-unknown-unknown
    cargo nextest run --all-features --workspace --exclude fcb_wasm --exclude fcb_py
    cargo check --all-features --workspace --exclude fcb_wasm --exclude fcb_py
    cargo build --workspace --all-features --exclude fcb_wasm --exclude fcb_py

# Run WASM-specific checks
check-wasm:
    cargo clippy --fix --allow-dirty -p fcb_wasm --target wasm32-unknown-unknown
    cargo check -p fcb_wasm --target wasm32-unknown-unknown
    cargo build -p fcb_wasm --target wasm32-unknown-unknown

# Run Python-specific checks
check-py:
    cd fcb_py && uv sync --extra dev
    cd fcb_py && uv run maturin develop
    cd fcb_py && uv run ruff check --fix .
    cd fcb_py && uv run ruff format .
    cd fcb_py && uv run pytest tests/

# Run C++ binding checks
pre-commit-cpp:
    cd src/cpp && cmake -B build -S . && cmake --build build

# Run all generation scripts in scripts directory
gen-all:
    @echo "Running all shell scripts in scripts..."
    @for script in scripts/*.sh; do \
        echo "Executing $script..."; \
        bash "$script"; \
    done
    @echo "All scripts executed."

# Build entire workspace (Rust + C++ + Python)
build-all: gen-all build build-cpp build-py

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
    cargo install just cargo-watch wasm-pack cargo-audit

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

# Build C++ bindings
build-cpp:
    cd src/cpp && cmake -B build -S . && cmake --build build

# Clean and rebuild C++ bindings
clean-cpp:
    cd src/cpp && rm -rf build

# Run C++ roundtrip tests
test-cpp:
    cd src/cpp/build && ./fcb_roundtrip_comprehensive ../rust/fcb_core/tests/data

# ============================================================================
# Python Commands
# ============================================================================

# Sync Python dependencies
py-sync:
    cd fcb_py && uv sync --extra dev

# Run Python development
py-dev:
    cd fcb_py && uv run maturin develop

# Run Python linter
py-lint:
    cd fcb_py && uv run ruff check --fix .

# Format Python code
py-fmt:
    cd fcb_py && uv run ruff format .

# Run Python tests
py-test:
    cd fcb_py && uv run pytest tests/

# Clean Python build artifacts
py-clean:
    cd fcb_py && cargo clean

# Install Python package in development mode
py-develop:
    cd fcb_py && maturin develop

# ============================================================================
# WASM Commands
# ============================================================================

# Build WASM package (web target, debug)
build-wasm:
    cd src/rust/wasm && wasm-pack build --dev

# Build WASM for production
build-wasm-release:
    cd src/rust/wasm && wasm-pack build --release

# ============================================================================
# CLI Commands
# ============================================================================

# Run FCB info command on test data
fcb_info:
    cargo run -p fcb_cli info -i fcb_core/tests/data/delft.fcb

# Generate file statistics (CSV output)
file-stats:
    cargo run -p fcb_core --bin stats -- -d fcb_core/benchmark_data/ -f csv

# Run benchmarks
bench:
    cargo bench -p fcb_core --bench read -- --release

# Build fcb_core release binary
build-fcb_core:
    cargo build --release -p fcb_core
