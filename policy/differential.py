#!/usr/bin/env python3
"""BPA-1 differential check: both evaluators over seeded structured cases.

    python3 policy/differential.py --seed 45217 --cases 256

Deterministic (own 64-bit LCG, no environment input): every run with the
same seed generates the same policies, requests, and malformed mutants,
executes every case through policy/eval.py (in process) and policy/eval.mjs
(one `node eval.mjs batch`), and requires byte-identical (JCS) results —
typed rejections included. On top of raw agreement it asserts the ADR-0001
executable algebra laws on the Python results:

  L1 intersect commutativity: intersect(a,b) and intersect(b,a) agree on
     ok-ness and, when ok, on exact canonical bytes;
  L2 is_subset(intersect(a,b), a) is never ok-and-false (it may reject
     incomparable when a intra-mixes pinned structures);
  L3 decide conjunction: decide(intersect(a,b), req) is allow exactly when
     decide(a, req) and decide(b, req) are both allow (deny preservation
     under intersection + deny-wins).

About 1 in 5 generated policies carries one structured malformation (unknown
member, $domain key, inverted interval, duplicate ids, float quantity,
non-normalized CIDR, non-NFC segment, wrong clock, bad effect, unknown
domain), so the fail-closed paths — including first-error pointer equality —
are differentially exercised, not just the happy path. Exit 0 only on full
agreement and law compliance.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import shutil
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent

spec = importlib.util.spec_from_file_location("bpa1_eval", HERE / "eval.py")
ev = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ev)


class Lcg:
    """Deterministic 64-bit LCG (Knuth MMIX constants)."""

    def __init__(self, seed: int):
        self.state = seed & 0xFFFFFFFFFFFFFFFF

    def next(self) -> int:
        self.state = (self.state * 6364136223846793005
                      + 1442695040888963407) & 0xFFFFFFFFFFFFFFFF
        return self.state >> 33

    def below(self, n: int) -> int:
        return self.next() % n

    def pick(self, seq):
        return seq[self.below(len(seq))]

    def coin(self, num: int, den: int) -> bool:
        return self.below(den) < num


def dref(ch: str) -> dict:
    return {"class": "structural_public", "algorithm": "sha-256",
            "value_hex": ch * 64}


PINS = [dref("a"), dref("b")]          # two of each pinned structure so
OP_IDS = ["op_a", "op_b", "op_c", "op_d"]   # incomparables actually occur
SQIDS = ["src:one", "src:two", "src:three"]
SEGMENTS = ["src", "api", "docs"]
PURPOSE_PATHS = [["p:root"], ["p:root", "p:a"], ["p:root", "p:a", "p:x"],
                 ["p:root", "p:b"]]
ELEMENTS = ["public", "internal", "secret"]
PROFILES = ["basic", "attested", "audited"]
EPOCHS = ["ep-1", "ep-2"]
CURRENCIES = ["USD", "EUR"]
HOSTS = [
    {"dns": "a.example"},
    {"dns": "b.example"},
    {"ip4_cidr": {"octets": [10, 0, 0, 0], "prefix_len": 8}},
    {"ip4_cidr": {"octets": [10, 1, 0, 0], "prefix_len": 16}},
    {"ip6_cidr": {"groups": [8193, 3512, 0, 0, 0, 0, 0, 0],
                  "prefix_len": 32}},
]


def gen_subset(rng: Lcg, pool: list, lo: int = 0) -> list:
    k = lo + rng.below(len(pool) + 1 - lo)
    picked = []
    for item in pool:
        if len(picked) < k and rng.coin(1, 2):
            picked.append(item)
    return picked


def gen_atom(rng: Lcg, domain: str) -> dict:
    if domain == "operation":
        return {"ids": gen_subset(rng, OP_IDS)}
    if domain in ("object", "binding"):
        return {"ids": gen_subset(rng, SQIDS)}
    if domain == "path":
        n = rng.below(3)
        return {"root": rng.pick(["ws:r1", "ws:r2"]),
                "segments": SEGMENTS[:n],
                "match": rng.pick(["exact", "subtree"])}
    if domain == "network_destination":
        first = rng.below(1000)
        return {"scheme": rng.pick(["https", "ssh"]),
                "host": rng.pick(HOSTS),
                "ports": {"first": first, "last": first + rng.below(2000)},
                "protocol": "tcp"}
    if domain == "purpose":
        return {"snapshot": rng.pick(PINS), "path": rng.pick(PURPOSE_PATHS)}
    if domain == "classification":
        return {"lattice": rng.pick(PINS),
                "allowed": gen_subset(rng, ELEMENTS)}
    if domain == "time":
        nb = rng.below(500)
        return {"not_before": nb, "not_after": nb + rng.below(500)}
    if domain == "quantity":
        if rng.coin(1, 2):
            return {"dimension": "money", "canonical_unit": "unit",
                    "scale": 2, "max": rng.below(1000),
                    "currency": rng.pick(CURRENCIES),
                    "pricing_revision": "pr-1"}
        return {"dimension": "compute", "canonical_unit": "token",
                "scale": 0, "max": rng.below(1000)}
    if domain == "rate":
        return {"dimension": "compute", "canonical_unit": "token",
                "capacity": rng.below(100),
                "refill_amount": rng.below(50),
                "refill_period_milliseconds": rng.pick([500, 1000, 20000]),
                "max_burst": rng.below(50), "epoch": rng.pick(EPOCHS),
                "clock": "authority_server"}
    if domain == "assurance":
        return {"order": rng.pick(PINS),
                "admitted": gen_subset(rng, PROFILES)}
    # schema_evidence
    return {"schema": rng.pick(PINS), "verifier": rng.pick(PINS),
            "attestor": rng.pick(PINS), "assurance_policy": rng.pick(PINS)}


def gen_rule(rng: Lcg) -> dict:
    atoms = {}
    for domain in gen_subset(rng, list(ev.DOMAINS))[:3]:
        atoms[domain] = gen_atom(rng, domain)
    return {"effect": "deny" if rng.coin(1, 4) else "allow", "atoms": atoms}


def mutate(rng: Lcg, policy: dict) -> dict:
    """Inject exactly one structured malformation (fail-closed paths)."""
    policy = json.loads(json.dumps(policy))  # deep copy
    kind = rng.below(11)
    if kind == 0:
        policy["note"] = "x"
    elif kind == 1:
        policy["$domain"] = "spoof"
    elif kind == 2:
        policy["rules"] = {"not": "an array"}
    elif not policy["rules"]:
        policy["rules"] = [{"effect": "audit", "atoms": {}}]
    else:
        rule = policy["rules"][rng.below(len(policy["rules"]))]
        if kind == 3:
            rule["effect"] = "audit"
        elif kind == 4:
            rule["atoms"]["regex"] = {"pattern": ".*"}
        elif kind == 5:
            rule["atoms"]["time"] = {"not_before": 9, "not_after": 1}
        elif kind == 6:
            rule["atoms"]["operation"] = {"ids": ["op_a", "op_a"]}
        elif kind == 7:
            rule["atoms"]["quantity"] = {
                "dimension": "compute", "canonical_unit": "token",
                "scale": 0, "max": 1.5}
        elif kind == 8:
            rule["atoms"]["network_destination"] = {
                "scheme": "https",
                "host": {"ip4_cidr": {"octets": [10, 0, 0, 1],
                                      "prefix_len": 8}},
                "ports": {"first": 1, "last": 2}, "protocol": "tcp"}
        elif kind == 9:
            rule["atoms"]["path"] = {"root": "ws:r1",
                                     "segments": ["étude"],
                                     "match": "exact"}
        else:
            rule["atoms"]["rate"] = {**gen_atom(rng, "rate"),
                                     "clock": "wall"}
    return policy


def gen_policy(rng: Lcg) -> dict:
    policy = {"rules": [gen_rule(rng) for _ in range(rng.below(4))]}
    if rng.coin(1, 5):
        policy = mutate(rng, policy)
    return policy


def gen_point(rng: Lcg, domain: str):
    if domain == "operation":
        return rng.pick(OP_IDS)
    if domain in ("object", "binding"):
        return rng.pick(SQIDS)
    if domain == "path":
        return {"root": rng.pick(["ws:r1", "ws:r2"]),
                "segments": SEGMENTS[:rng.below(4)]}
    if domain == "network_destination":
        host = rng.pick([{"dns": "a.example"}, {"ip4": [10, 1, 2, 3]},
                         {"ip6": [8193, 3512, 0, 0, 0, 0, 0, 1]}])
        return {"scheme": rng.pick(["https", "ssh"]), "host": host,
                "port": rng.below(2000), "protocol": "tcp"}
    if domain == "purpose":
        return {"snapshot": rng.pick(PINS), "path": rng.pick(PURPOSE_PATHS)}
    if domain == "classification":
        return {"lattice": rng.pick(PINS), "element": rng.pick(ELEMENTS)}
    if domain == "time":
        return {"at": rng.below(1000)}
    if domain == "quantity":
        if rng.coin(1, 2):
            return {"dimension": "money", "canonical_unit": "unit",
                    "scale": 2, "amount": rng.below(1000),
                    "currency": rng.pick(CURRENCIES),
                    "pricing_revision": "pr-1"}
        return {"dimension": "compute", "canonical_unit": "token",
                "scale": 0, "amount": rng.below(1000)}
    if domain == "rate":
        return gen_atom(rng, "rate")
    if domain == "assurance":
        return {"order": rng.pick(PINS), "profile": rng.pick(PROFILES)}
    return gen_atom(rng, "schema_evidence")


def gen_request(rng: Lcg) -> dict:
    req = {d: gen_point(rng, d) for d in gen_subset(rng, list(ev.DOMAINS))}
    if rng.coin(1, 10):
        req["operation"] = "Bad-Op!"  # malformed request point
    return req


def main(argv) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--seed", type=int, required=True)
    ap.add_argument("--cases", type=int, default=256)
    args = ap.parse_args(argv)

    node = shutil.which("node")
    if node is None:
        print("differential: FAIL — node not found (both evaluators are "
              "required)")
        return 1

    rng = Lcg(args.seed)
    cases: list[dict] = []
    law_failures = 0

    def law_fail(name, detail):
        nonlocal law_failures
        law_failures += 1
        print(f"LAW FAIL {name}: {detail}")

    for i in range(args.cases):
        a, b = gen_policy(rng), gen_policy(rng)
        req = gen_request(rng)
        batch = [
            {"policy_op": "well_formed", "policy": a},
            {"policy_op": "well_formed", "policy": b},
            {"policy_op": "canonical", "policy": a},
            {"policy_op": "intersect", "a": a, "b": b},
            {"policy_op": "intersect", "a": b, "b": a},
            {"policy_op": "is_subset", "child": a, "parent": b},
            {"policy_op": "decide", "policy": a, "request": req},
            {"policy_op": "decide", "policy": b, "request": req},
        ]
        iab = ev.run_case(batch[3])
        iba = ev.run_case(batch[4])
        # L1: commutativity (ok-ness always; canonical bytes when ok).
        if iab["ok"] != iba["ok"]:
            law_fail("L1", f"case {i}: ok mismatch")
        elif iab["ok"] and iab["canonical"] != iba["canonical"]:
            law_fail("L1", f"case {i}: canonical bytes differ")
        if iab["ok"]:
            batch.append({"policy_op": "is_subset", "child": iab["value"],
                          "parent": a})
            batch.append({"policy_op": "decide", "policy": iab["value"],
                          "request": req})
            # L2: the meet is never ok-and-not-subset of a factor.
            sub = ev.run_case(batch[-2])
            if sub["ok"] and not sub["subset"]:
                law_fail("L2", f"case {i}: intersect not subset of a")
            # L3: decide conjunction.
            da = ev.run_case(batch[6])
            db = ev.run_case(batch[7])
            dm = ev.run_case(batch[-1])
            if da["ok"] and db["ok"] and dm["ok"]:
                want_allow = (da["decision"] == "allow"
                              and db["decision"] == "allow")
                if (dm["decision"] == "allow") != want_allow:
                    law_fail("L3", f"case {i}: decide conjunction broken")
        cases.extend(batch)

    py_results = [ev.run_case(c) for c in cases]
    proc = subprocess.run(
        [node, str(HERE / "eval.mjs"), "batch"],
        input=json.dumps(cases), capture_output=True, text=True)
    if proc.returncode != 0:
        print(f"differential: FAIL — eval.mjs: {proc.stderr.strip()[:400]}")
        return 1
    mjs_results = json.loads(proc.stdout)
    if len(mjs_results) != len(py_results):
        print("differential: FAIL — result count mismatch")
        return 1

    mismatches = 0
    for case, py_r, mjs_r in zip(cases, py_results, mjs_results):
        if ev.jcs(py_r) != ev.jcs(mjs_r):
            mismatches += 1
            if mismatches <= 5:
                print(f"MISMATCH {case['policy_op']}:\n"
                      f"  case:     {ev.jcs(case)[:300]}\n"
                      f"  eval.py:  {ev.jcs(py_r)}\n"
                      f"  eval.mjs: {ev.jcs(mjs_r)}")

    rejected = sum(1 for r in py_results if not r["ok"])
    print(f"differential: seed {args.seed}, {args.cases} structured cases, "
          f"{len(cases)} evaluations ({rejected} typed rejections), "
          f"{len(cases) - mismatches}/{len(cases)} agree, "
          f"{law_failures} law failure(s)")
    if mismatches or law_failures:
        print("differential: FAIL")
        return 1
    print("differential: OK — both evaluators byte-identical (JCS)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
