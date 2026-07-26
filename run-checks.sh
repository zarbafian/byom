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

echo "== i1 governed loop (scripted gate: byomd + koveed live, the greenfield"
echo "   binding, notification-is-not-a-wake, the four-stage activation, the"
echo "   formation saga, and the model_egress act chain through kovee's"
echo "   disclosed metered broker with a STUB provider — no network, no model)"
# --all-checks, not --scripted: R3-I01 found the gate's own default was
# narrower than its claim (the crash matrix, the per-source trails and the two
# attached execution paths were reachable only if someone remembered the flag).
# The real-harness cells stay env-gated inside --all-checks and report a SKIP.
python3 conformance/i1-governed-loop/run.py --all-checks

echo "run-checks: OK"
