.PHONY: write
write:
	cargo run --bin write

.PHONY: pre-commit
pre-commit:
	cargo fmt
	cargo clippy --fix --allow-dirty
	cargo machete
	cargo test
	cargo check --all-features
	cargo build --release
