.PHONY: pre-commit
pre-commit:
	make check-common
	make check-wasm

.PHONY: check-common
check-common:
	cargo fmt
	cargo clippy --fix --allow-dirty --workspace --all-targets --all-features --exclude fcb_wasm
	cargo clippy --fix --allow-dirty -p fcb_wasm --target wasm32-unknown-unknown
	# cargo machete --exclude wasm
	cargo nextest run --all-features --workspace --exclude fcb_wasm
	cargo check --all-features --workspace --exclude fcb_wasm
	cargo build --workspace --all-features --exclude fcb_wasm

.PHONY: check-wasm
check-wasm:
	cargo clippy --fix --allow-dirty -p fcb_wasm --target wasm32-unknown-unknown
	cargo check -p fcb_wasm --target wasm32-unknown-unknown
	cargo build -p fcb_wasm --target wasm32-unknown-unknown

.PHONY: fcb_info
fcb_info:
	cargo run -p fcb_cli info -i fcb_core/tests/data/delft_bbox.fcb

.PHONY: file_stats
file_stats:
	cargo run -p fcb_core --bin stats -- -d fcb_core/benchmark_data/ -f csv

.PHONY: bench
bench:
	cargo bench -p fcb_core --bench read -- --release

.PHONY: build-fcb_core
build-fcb_core:
	cargo build --release --bin flatcitybuf_cli

.PHONY: wasm-build
wasm-build:
	cd wasm && wasm-pack build --target web --debug --out-dir ../../ts
# cargo build --target web --release