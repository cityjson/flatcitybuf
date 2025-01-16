

.PHONY: pre-commit
pre-commit:
	cargo fmt
	cargo clippy --fix --allow-dirty
	cargo machete
	cargo test
	cargo check --all-features
	cargo build --release

.PHONY: ser
ser:
	cargo run --bin flatcitybuf_cli ser -i tests/data/delft.city.jsonl -o temp/delft_attr.fcb
# cargo run --bin flatcitybuf_cli serialize -i tests/data/delft.city.jsonl -o temp/delft.fcb

.PHONY: deser
deser:
	cargo run --bin flatcitybuf_cli deser -i temp/delft_attr.fcb -o temp/delft_attr.city.jsonl
# cargo run --bin flatcitybuf_cli deserialize -i temp/small.fcb -o temp/small.city.jsonl

.PHONY: bench
bench:
	cargo bench --bench read
