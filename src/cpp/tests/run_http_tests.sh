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

# 60s wall-clock budget, not a fixed iteration count: a cold python3 on a
# macOS CI runner can take >15s just to launch (first-run code-signing
# verification), which is longer than the old 100 x 0.05s loop allowed.
DEADLINE=$((SECONDS + 60))
PORT=""
while [[ -z "${PORT}" ]] && (( SECONDS < DEADLINE )); do
  kill -0 "${SERVER_PID}" 2>/dev/null || break   # server died: fail fast
  PORT="$(cat "${TMP}" 2>/dev/null || true)"
  [[ -n "${PORT}" ]] || sleep 0.2
done
[[ -n "${PORT:-}" ]] || { echo "range_server.py did not report a port" >&2; exit 1; }

export FCB_TEST_HTTP_URL="http://127.0.0.1:${PORT}/delft.fcb"
echo "serving fixture at ${FCB_TEST_HTTP_URL}"
"${TEST_EXE}"
