#!/usr/bin/env bash
# Build web lalu jalankan server di http://127.0.0.1:8080 (web + API satu port).
# Tes di HP lewat kabel: adb reverse tcp:8080 tcp:8080, buka localhost:8080 di Chrome HP.
set -euo pipefail
cd "$(dirname "$0")/.."
[ -d web/node_modules ] || npm --prefix web install
npm --prefix web run build >/dev/null
command -v adb >/dev/null && adb reverse tcp:8080 tcp:8080 >/dev/null 2>&1 && echo "HP tersambung: buka localhost:8080 di Chrome HP"
cd server && CARGO_BUILD_JOBS=2 exec cargo run -q -- "$@"
