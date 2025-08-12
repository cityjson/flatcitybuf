.PHONY: pre-commit
pre-commit:
	make check-common
	make check-wasm
	make check-py

.PHONY: check-common
check-common:
	cargo fmt
	cargo clippy --fix --allow-dirty --workspace --all-targets --all-features --exclude fcb_wasm --exclude fcb_py
	cargo clippy --fix --allow-dirty -p fcb_wasm --target wasm32-unknown-unknown
	# cargo machete --exclude wasm
	cargo nextest run --all-features --workspace --exclude fcb_wasm --exclude fcb_py
	cargo check --all-features --workspace --exclude fcb_wasm --exclude fcb_py
	cargo build --workspace --all-features --exclude fcb_wasm --exclude fcb_py

.PHONY: check-wasm
check-wasm:
	cargo clippy --fix --allow-dirty -p fcb_wasm --target wasm32-unknown-unknown
	cargo check -p fcb_wasm --target wasm32-unknown-unknown
	cargo build -p fcb_wasm --target wasm32-unknown-unknown

.PHONY: check-py
check-py:
	cd fcb_py && uv sync --extra dev
	cd fcb_py && uv run maturin develop
	cd fcb_py && uv run ruff check --fix .
	cd fcb_py && uv run ruff format .
	cd fcb_py && uv run pytest tests/

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
	cd wasm && wasm-pack build --target web --debug --no-pack
# cargo build --target web --release

.PHONY: py-build
py-build:
	cd fcb_py && maturin build --release

.PHONY: py-develop
py-develop:
	cd fcb_py && maturin develop

.PHONY: py-test
py-test:
	cd fcb_py && uv run pytest tests/

.PHONY: py-clean
py-clean:
	cd fcb_py && cargo clean
	cd fcb_py && rm -rf target/
	cd fcb_py && rm -rf *.so
	cd fcb_py && rm -rf build/
	cd fcb_py && rm -rf dist/
	cd fcb_py && rm -rf *.egg-info