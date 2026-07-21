#!/usr/bin/env bash
# Generate the Class A conformance corpus using the Rust CLI as the oracle.
#
# -A indexes every attribute, so the corpus exercises the B+tree paths.
#
# For each input .city.jsonl we write a .fcb, then read that .fcb back with
# the RUST reader and save its output as the expected result. Comparing C++
# against the Rust reader's view of the same file (rather than against the
# original JSON) cancels out any shared normalisation and isolates C++ bugs.
#
# conformance/*.fcb and *.expected.jsonl are tracked in git, and this script
# overwrites both. Regeneration is NOT byte-reproducible -- cjseq2 iterates
# CityObjects from a HashMap with per-process ordering -- so `git diff` after
# running this will show churn even when nothing semantic changed. Compare
# the *parsed* JSON before committing a change; don't commit pure noise.
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

echo "Class A corpus written to ${OUT}"
