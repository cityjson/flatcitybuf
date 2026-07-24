#!/usr/bin/env bash
# Regenerate the committed TypeScript FlatBuffers bindings from src/fbs/*.fbs.
#
# The flag set is load-bearing; both flags have been verified to fail:
#   --ts-omit-entrypoint  without it, header.ts becomes a circular
#                         self-re-export; importing fails with SyntaxError
#                         (Node ESM) or TS2303 (tsc with strict: true).
#   --gen-all             without it, header.ts imports ./extension.js,
#                         which is never generated, and compilation fails.
# Neither failure is reported by flatc. See test/generated.test.ts.
#
# Generated with: flatc 25.9.23 / flatbuffers npm ^25.9.23
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${REPO_ROOT}/src/ts/src/generated"
rm -rf "${OUT}" && mkdir -p "${OUT}"
flatc --ts --ts-omit-entrypoint --gen-all -o "${OUT}" \
  "${REPO_ROOT}/src/fbs/header.fbs" \
  "${REPO_ROOT}/src/fbs/feature.fbs"
echo "TypeScript bindings written to ${OUT}"
