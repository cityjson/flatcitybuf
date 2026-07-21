#!/usr/bin/env bash
# Generate the Class A conformance corpus using the Rust CLI as the oracle.
#
# -A indexes every attribute, so the corpus exercises the B+tree paths.
#
# For each input .city.jsonl we write a .fcb, then read that .fcb back with
# the RUST reader and save its output as the expected result. Comparing C++
# against the Rust reader's view of the same file (rather than against the
# original JSON) cancels out any shared normalisation and isolates C++ bugs.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${REPO_ROOT}/conformance"
RUST="${REPO_ROOT}/src/rust"
INPUTS_DIR="${OUT}/inputs"

mkdir -p "${OUT}"

# Existing fixtures plus any hand-authored edge cases in inputs/.
INPUTS=(
  "${RUST}/fcb_core/tests/data/small.city.jsonl"
  "${RUST}/fcb_core/tests/data/geom_temp.city.jsonl"
  "${RUST}/fcb_core/tests/data/noise_extension.city.jsonl"
)
if [[ -d "${INPUTS_DIR}" ]]; then
  while IFS= read -r f; do INPUTS+=("$f"); done \
    < <(find "${INPUTS_DIR}" -name '*.city.jsonl' | sort)
fi

for src in "${INPUTS[@]}"; do
  [[ -f "${src}" ]] || { echo "skip (missing): ${src}"; continue; }
  name="$(basename "${src}" .city.jsonl)"
  echo "==> ${name}"
  (cd "${RUST}" && cargo run --quiet --release -p fcb_cli -- \
      ser -A -i "${src}" -o "${OUT}/${name}.fcb")
  (cd "${RUST}" && cargo run --quiet --release -p fcb_cli -- \
      deser -i "${OUT}/${name}.fcb" -o "${OUT}/${name}.expected.jsonl")
done

SMALL="${RUST}/fcb_core/tests/data/small.city.jsonl"

# A node size other than the default 16, so a reader that hardcodes 16 fails.
#
# The input MUST have more than 8 features for this fixture to bite. The R-tree
# level bounds are ceil(n / node_size) per level, so for n <= 8 both node sizes
# collapse to a single-level tree of identical size and a reader that hardcodes
# 16 traverses the file correctly by accident. `appearance_depths` has 12
# features: node 16 gives levels [12, 1] = 13 nodes = 520 B, node 8 gives
# [12, 2, 1] = 15 nodes = 600 B. The 80-byte difference lands a hardcoding
# reader inside the feature section, where it fails loudly.
#
# The node size does not affect the feature order, so this file's contents are
# appearance_depths.expected.jsonl and it gets no expected file of its own.
NODE8_SRC="${INPUTS_DIR}/appearance_depths.city.jsonl"
echo "==> appearance_depths_node8 (index_node_size = 8)"
(cd "${RUST}" && cargo run --quiet --release -p fcb_cli -- \
    ser -A --index-node-size 8 -i "${NODE8_SRC}" -o "${OUT}/appearance_depths_node8.fcb")

# features_count = 0, which means "unknown": the reader must scan to EOF.
# The R-tree is omitted because its size is derived from the feature count,
# so a file with both an R-tree and a count of 0 cannot be parsed at all.
# Omitting it also skips the Hilbert sort, so the features come out in input
# order -- a different order from small.expected.jsonl, hence its own file.
echo "==> no_count (features_count = 0)"
(cd "${RUST}" && cargo run --quiet --release -p fcb_cli -- \
    ser -A --no-feature-count --no-spatial-index -i "${SMALL}" -o "${OUT}/no_count.fcb")
(cd "${RUST}" && cargo run --quiet --release -p fcb_cli -- \
    deser -i "${OUT}/no_count.fcb" -o "${OUT}/no_count.expected.jsonl")

echo "Class A corpus written to ${OUT}"
