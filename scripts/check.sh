#!/usr/bin/env bash
# Full quality gate. Every step must pass; nothing is skipped or downgraded on failure.
#
#   ./scripts/check.sh
#
# Runs on Linux / WSL (cross-compiling with cargo-xwin, Windows executables started through WSL
# interop) and on Windows under Git Bash. Set DENY_OFFLINE=1 to use the local advisory database
# when the machine has no network access.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
# shellcheck source=scripts/build-env.sh
source "$ROOT/scripts/build-env.sh"

PYTHON_BIN="python3"
if ! python3 -c "import sys" >/dev/null 2>&1; then
    PYTHON_BIN="python"
fi

echo "==> [1/7] Formatting (cargo fmt)"
cargo fmt --all -- --check

echo "==> [2/7] Third-party notices are in sync with Cargo.lock"
"$PYTHON_BIN" scripts/gen-notices.py --check

echo "==> [3/7] Dependency licences, bans and advisories (cargo deny)"
if [ "${DENY_OFFLINE:-0}" = "1" ]; then
    cargo deny --offline check
else
    cargo deny check
fi

echo "==> [4/7] Strict clippy gate ($TARGET_TRIPLE)"
cargo_win clippy --workspace --all-targets -- -D warnings

echo "==> [5/7] Unit and integration tests ($TARGET_TRIPLE)"
if [ "$USE_XWIN" -eq 1 ]; then
    # Build the test binaries, then start each Windows .exe directly: letting cargo act as the
    # runner hangs under WSL2 /init.
    TEST_EXES="$(
        cargo_win test --workspace --no-run --message-format=json \
        | "$PYTHON_BIN" -c '
import json, sys
seen = set()
for line in sys.stdin:
    line = line.strip()
    if not line.startswith("{"):
        continue
    obj = json.loads(line)
    if obj.get("reason") == "compiler-artifact" and obj.get("profile", {}).get("test"):
        exe = obj.get("executable")
        if exe and exe not in seen:
            seen.add(exe)
            print(exe)
'
    )"
    if [ -z "$TEST_EXES" ]; then
        echo "ERROR: no test executables discovered" >&2
        exit 1
    fi
    while IFS= read -r exe; do
        echo "  -> $exe"
        run_exe 600 "$exe"
    done <<< "$TEST_EXES"
else
    cargo_win test --workspace
fi

echo "==> [6/7] Release build, packaging and isolated installer self-test"
"$ROOT/scripts/package.sh"

echo "==> [7/7] sccache statistics"
if command -v sccache >/dev/null 2>&1; then
    sccache --show-stats </dev/null
fi

echo "==> All gates passed."
