#!/usr/bin/env bash
# Local, on-demand validation for byom — the akson run-checks.sh pattern.
# Extends as the workspace lands (cargo fmt/clippy/test, descriptor parity).
set -euo pipefail
cd "$(dirname "$0")"

echo "== conformance (schemas + envelope/machine vectors)"
python3 conformance/run.py

echo "== descriptor-model parity (proof/specs <-> spec/descriptors)"
python3 proof/check-descriptors.py

echo "== family vectors (independent rederiver)"
python3 family-vectors/xcheck.py

echo "== family vectors (TypeScript independent rederiver)"
node family-vectors/tscheck/check.mjs

echo "run-checks: OK"
