.PHONY: test e2e run build clean fmt lint check

# Run all tests
test:
	cargo test

# Run E2E tests (basic functionality + 3DBAG API compatibility)
e2e:
	@echo "Running basic E2E tests..."
	cargo test --test e2e -- --test-threads=1
	@echo "Running 3DBAG API compatibility tests..."
	cargo test --test e2e_comparison -- --test-threads=1

# Run the API server with default 3DBAG data
run:
	FCB_URL=https://flatcitybuf.open3d.city/data/3dbag_all_index.fcb cargo run

# Build release version
build:
	cargo build --release

# Clean build artifacts
clean:
	cargo clean

# Format code
fmt:
	cargo fmt

# Run linter
lint:
	cargo clippy -- -D warnings

# Run all checks (format, lint, test, e2e)
check: fmt lint test e2e