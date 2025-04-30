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
# cargo nextest run -p wasm --target wasm32-unknown-unknown
	cargo check -p fcb_wasm --target wasm32-unknown-unknown
	cargo build -p fcb_wasm --target wasm32-unknown-unknown

.PHONY: ser
ser:
	cargo run -p fcb_cli ser -i fcb_core/tests/data/delft.city.jsonl -o fcb_core/tests/data/delft_attr.fcb --attr-index b3_h_dak_50p,identificatie
# cargo run -p fcb_cli ser -i fcb_core/tests/data/delft.city.jsonl -o temp/delft_bbox.fcb
# cargo run --bin flatcitybuf_cli serialize -i tests/data/delft.city.jsonl -o temp/delft.fcb
	# cargo run -p fcb_cli ser -i fcb_core/tests/data/delft.city.jsonl -o fcb_core/benchmark_data/attribute/delft.fcb


.PHONY: deser
deser:
	cargo run -p fcb_cli deser -i temp/delft_attr.fcb -o temp/delft_attr.city.jsonl
# cargo run --bin flatcitybuf_cli deserialize -i temp/small.fcb -o temp/small.city.jsonl

.PHONY: fcb_info
fcb_info:
	cargo run -p fcb_cli info -i fcb_core/tests/data/delft_bbox.fcb

.PHONY: bench
bench:
	cargo bench -p fcb_core --bench read

.PHONY: build-fcb_core
build-fcb_core:
	cargo build --release --bin flatcitybuf_cli

.PHONY: wasm-build
wasm-build:
	cd wasm && wasm-pack build --target web --debug --out-dir ../../ts
# cargo build --target web --release