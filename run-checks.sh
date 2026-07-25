#!/usr/bin/env bash
# Local, on-demand validation for byom — the akson run-checks.sh pattern.
# Extends as the workspace lands (cargo fmt/clippy/test, descriptor parity).
set -euo pipefail
cd "$(dirname "$0")"

echo "== conformance (schemas + envelope vectors)"
python3 conformance/run.py

echo "== family vectors (independent rederiver)"
python3 family-vectors/xcheck.py

echo "run-checks: OK"
