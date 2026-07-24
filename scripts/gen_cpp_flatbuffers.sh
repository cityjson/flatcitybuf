#!/usr/bin/env bash
# Regenerate the committed C++ FlatBuffers headers from src/fbs/*.fbs.
#
# flatc must match the C++ flatbuffers RUNTIME the generated code compiles
# against -- generated code calls runtime APIs that change across versions.
# It does NOT need to match the `flatbuffers` crate pin in src/rust/Cargo.toml
# (24.3.25): the FlatBuffers wire format is stable across versions, so files
# written by the Rust implementation read fine here.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${REPO_ROOT}/src/cpp/include/fcb/generated"

EXPECTED_FLATC_VERSION="${FCB_FLATC_VERSION:-25.9.23}"
ACTUAL="$(flatc --version | grep -oE '[0-9]+\.[0-9]+\.[0-9]+')"
if [[ "${ACTUAL}" != "${EXPECTED_FLATC_VERSION}" ]]; then
  echo "ERROR: flatc ${ACTUAL} found, but ${EXPECTED_FLATC_VERSION} is required." >&2
  echo "       It must match the flatbuffers C++ runtime you build against." >&2
  echo "       Override with FCB_FLATC_VERSION=<v> if you have bumped both." >&2
  exit 1
fi

mkdir -p "${OUT_DIR}"
flatc --cpp --scoped-enums -o "${OUT_DIR}" -I "${REPO_ROOT}/src/fbs" \
  "${REPO_ROOT}/src/fbs/header.fbs" \
  "${REPO_ROOT}/src/fbs/feature.fbs" \
  "${REPO_ROOT}/src/fbs/geometry.fbs" \
  "${REPO_ROOT}/src/fbs/extension.fbs"

echo "Generated C++ headers in ${OUT_DIR}"
