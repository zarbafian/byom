#!/usr/bin/env bash
# Local, on-demand validation for byom — the akson run-checks.sh pattern.
# Extends as the workspace lands (cargo fmt/clippy/test, descriptor parity).
set -euo pipefail
cd "$(dirname "$0")"

echo "== conformance (schemas + envelope/machine/policy vectors + C3a mcp tool bindings)"
python3 conformance/run.py

echo "== BPA-1 policy evaluator (independent TypeScript-side vector agreement)"
node policy/eval.mjs check spec/vectors/policy

echo "== BPA-1 differential (both evaluators, seeded, deterministic)"
python3 policy/differential.py --seed 45217 --cases 256

echo "== descriptor-model parity (proof/specs <-> spec/descriptors)"
python3 proof/check-descriptors.py

echo "== negative mutation suite (ADR-0003; parity/conformance mutations, TLC when java present)"
if command -v java >/dev/null 2>&1 && [ -f proof/tools/tla2tools.jar ]; then
  python3 proof/negative-checks.py
else
  python3 proof/negative-checks.py --no-tlc
fi

echo "== family vectors (independent rederiver)"
python3 family-vectors/xcheck.py

echo "== family vectors (TypeScript independent rederiver)"
node family-vectors/tscheck/check.mjs

echo "== cargo fmt (workspace formatting)"
cargo fmt --check

echo "== cargo clippy (workspace lints, warnings denied)"
cargo clippy --workspace --all-targets --locked -- -D warnings

echo "== cargo test (workspace: vector round-trips, journal fault injection,"
echo "   onboarding negatives, mandate negatives, cease, privacy chain,"
echo "   classification, acceptance + crash matrix, e2e over real sockets)"
cargo test --workspace --locked

echo "== conformance live replay (slice-op request vectors against a spawned byomd)"
python3 conformance/run.py --live

echo "== i0 society-of-two tracer (scripted flow: byomd + koveed live,"
echo "   byom-mcp/kovee-mcp driven over scripted MCP stdio, per-source trails)"
python3 conformance/i0-society-of-two/run.py --scripted

echo "run-checks: OK"
