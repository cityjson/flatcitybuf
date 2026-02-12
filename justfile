# FlatCityBuf Justfile

# Default recipe - list all available commands
default:
    @just --list

# Run all generation scripts in scripts directory
gen-all:
    @echo "Running all shell scripts in scripts..."
    @for script in scripts/*.sh; do \
        echo "Executing $script..."; \
        bash "$script"; \
    done
    @echo "All scripts executed."

# Build the Rust workspace
build:
    cd src/rust && cargo build

# Build with release optimizations
build-release:
    cd src/rust && cargo build --release

# Clean build artifacts
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

# Build C++ bindings
build-cpp:
    cd src/cpp && cmake -B build -S . && cmake --build build

# Clean and rebuild C++ bindings
clean-cpp: build-cpp
    rm -rf src/cpp/build

# Build WASM package
build-wasm:
    cd src/rust/wasm && wasm-pack build --dev

# Build WASM for production
build-wasm-release:
    cd src/rust/wasm && wasm-pack build --release

# Run the API server
run-api:
    cd src/rust && cargo run --bin fcb_api

# Watch for changes and re-run tests
watch:
    cd src/rust && cargo watch -x test

# Watch and run clippy
watch-clippy:
    cd src/rust && cargo watch -x clippy

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

# Run all generation and build
all: gen-all build
