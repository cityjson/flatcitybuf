

.PHONY: pre-commit
pre-commit:
	cargo fmt
	cargo clippy --fix --allow-dirty
	cargo machete
	cargo test
	cargo check --all-features
	cargo build --release

.PHONY: encode
encode:
	cargo run --bin flatcitybuf_cli serialize -i tests/data/delft.city.jsonl -o temp/delft.fcb

.PHONY: decode
decode:
	cargo run --bin flatcitybuf_cli deserialize -i temp/small.fcb -o temp/small.city.jsonl

.PHONY: bench
bench:
	cargo bench --bench read -- --html-reports
