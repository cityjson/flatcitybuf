.PHONY: test e2e run build clean fmt lint

# Run all tests
test:
	cargo test

# Run E2E tests
e2e:
	cargo test --test e2e -- --test-threads=1

# Run the API server
run:
	FCB_URL=https://storage.googleapis.com/flatcitybuf/3dbag_all_index.fcb cargo run

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

# Run all checks (format, lint, test)
check: fmt lint test e2e