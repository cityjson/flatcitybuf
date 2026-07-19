#!/usr/bin/env bash
# Launch the range-capable test server, point the suite at it, tear it down.
set -euo pipefail

PYTHON="${1:?python}"
SERVER="${2:?range_server.py}"
DATA_DIR="${3:?data dir}"
TEST_EXE="${4:?test binary}"

TMP="$(mktemp)"
"${PYTHON}" "${SERVER}" "${DATA_DIR}" > "${TMP}" &
SERVER_PID=$!
trap 'kill "${SERVER_PID}" 2>/dev/null || true; rm -f "${TMP}"' EXIT

for _ in $(seq 1 100); do
  PORT="$(cat "${TMP}" 2>/dev/null || true)"
  [[ -n "${PORT}" ]] && break
  sleep 0.05
done
[[ -n "${PORT:-}" ]] || { echo "range_server.py did not report a port" >&2; exit 1; }

export FCB_TEST_HTTP_URL="http://127.0.0.1:${PORT}/delft.fcb"
echo "serving fixture at ${FCB_TEST_HTTP_URL}"
"${TEST_EXE}"
