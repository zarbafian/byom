#!/usr/bin/env python3
"""ADR-0003 negative mutation suite (RT-11): prove the harness is not
vacuous. Every deliberate mutation below MUST be caught — a mutant that
passes its checker is a failure of the harness, mirroring akson's
negative-checks.sh.

    python3 proof/negative-checks.py            # all mutations
    python3 proof/negative-checks.py --no-tlc   # skip the TLC mutations

Mutation classes:
  parity      descriptor/model drift must fail proof/check-descriptors.py
              (a dropped or invented row, a renamed state, a lost v2
              structured column, a de-versioned format tag, a model-only
              literal rename);
  conformance a widened schema or descriptor must fail conformance/run.py
              (update meta no longer requiring expected_revision; a
              descriptor row losing its crash_result);
  tlc         a weakened model guard must yield a TLC counterexample
              (finalize without the full seat set violates
              FinalizedHasAllSeats; reclaiming a LIVE leased head violates
              ReclaimNeedsExpiryOrYield — D-RT-6).

The MCP widening mutations (RT-16) run inside conformance/run.py itself
(check_mcp_mutations) on every run and are not duplicated here.
"""

from __future__ import annotations

import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PY = sys.executable or "python3"

failures = []
caught = 0


def report(label: str, was_caught: bool):
    global caught
    if was_caught:
        caught += 1
        print(f"caught    {label}")
    else:
        failures.append(label)
        print(f"MISSED    {label}")


def run(cmd, cwd=None) -> int:
    return subprocess.run(cmd, cwd=cwd, capture_output=True,
                          text=True).returncode


def copy_tree(*names: str) -> Path:
    tmp = Path(tempfile.mkdtemp(prefix="byom-neg-"))
    for name in names:
        src = ROOT / name
        dst = tmp / name
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copytree(src, dst)
    return tmp


# ------------------------------------------------------------- parity ----

def parity_fails(tmp: Path) -> bool:
    return run([PY, str(ROOT / "proof" / "check-descriptors.py"),
                "--specs", str(tmp / "proof" / "specs"),
                "--descriptors", str(tmp / "spec" / "descriptors")]) != 0


def mutate_descriptor(tmp: Path, name: str, fn):
    path = tmp / "spec" / "descriptors" / name
    body = json.loads(path.read_text())
    fn(body)
    path.write_text(json.dumps(body, indent=2, ensure_ascii=False) + "\n")


def parity_mutations():
    # 1. drop a modeled descriptor row
    tmp = copy_tree("proof/specs", "spec/descriptors")
    mutate_descriptor(tmp, "pledge.json",
                      lambda b: b["transitions"].pop(0))
    report("parity: descriptor row dropped (pledge absent->proposed)",
           parity_fails(tmp))
    shutil.rmtree(tmp)

    # 2. invent a descriptor row the model does not have
    tmp = copy_tree("proof/specs", "spec/descriptors")

    def invent(b):
        row = dict(b["transitions"][0])
        row.update({"from": "fulfilled", "to": "active",
                    "via": "pledge_resume"})
        b["transitions"].append(row)
    mutate_descriptor(tmp, "pledge.json", invent)
    report("parity: unmodeled row invented (fulfilled->active)",
           parity_fails(tmp))
    shutil.rmtree(tmp)

    # 3. rename a state only in the descriptor
    tmp = copy_tree("proof/specs", "spec/descriptors")

    def rename(b):
        b["states"] = ["activated" if s == "active" else s
                       for s in b["states"]]
    mutate_descriptor(tmp, "pledge.json", rename)
    report("parity: descriptor state renamed (active->activated)",
           parity_fails(tmp))
    shutil.rmtree(tmp)

    # 4. lose a v2 structured column
    tmp = copy_tree("proof/specs", "spec/descriptors")
    mutate_descriptor(tmp, "episode.json",
                      lambda b: b["transitions"][0].pop("crash_result"))
    report("parity: v2 crash_result column dropped (episode)",
           parity_fails(tmp))
    shutil.rmtree(tmp)

    # 5. de-version the format tag
    tmp = copy_tree("proof/specs", "spec/descriptors")
    mutate_descriptor(tmp, "episode.json",
                      lambda b: b.update(format="byom-descriptor/v1"))
    report("parity: descriptor format de-versioned (episode v1)",
           parity_fails(tmp))
    shutil.rmtree(tmp)

    # 6. drop a @parity transition annotation from the model
    tmp = copy_tree("proof/specs", "spec/descriptors")
    spec = tmp / "proof" / "specs" / "Pledge.tla"
    text = spec.read_text()
    needle = ("\\* @parity transition: active -> superseded "
              "via pledge_finalize\n")
    assert needle in text, "annotation needle missing"
    spec.write_text(text.replace(needle, ""))
    report("parity: model annotation dropped (active->superseded)",
           parity_fails(tmp))
    shutil.rmtree(tmp)

    # 7. model-only literal rename (annotation and descriptor unchanged)
    tmp = copy_tree("proof/specs", "spec/descriptors")
    spec = tmp / "proof" / "specs" / "EpisodeLease.tla"
    text = spec.read_text()
    marker = text.index("====")
    body = text[:marker].replace('"lease_expired"', '"lease_stale"')
    spec.write_text(body + text[marker:])
    report("parity: model-only state literal renamed (lease_expired)",
           parity_fails(tmp))
    shutil.rmtree(tmp)


# -------------------------------------------------------- conformance ----

def conformance_mutations():
    # 8. widen an update meta back to optional expected_revision
    tmp = Path(tempfile.mkdtemp(prefix="byom-neg-"))
    shutil.copytree(ROOT / "spec", tmp / "spec")
    path = (tmp / "spec" / "schemas" / "ops"
            / "membership-accept-request.schema.json")
    body = json.loads(path.read_text())
    mm = body["$defs"]["mutationMeta"]
    mm["required"] = [r for r in mm["required"] if r != "expected_revision"]
    path.write_text(json.dumps(body, indent=2, ensure_ascii=False) + "\n")
    rc = run([PY, str(ROOT / "conformance" / "run.py"),
              str(tmp / "spec")])
    report("conformance: update meta widened "
           "(membership_accept expected_revision optional)", rc != 0)
    shutil.rmtree(tmp)

    # 9. descriptor row loses its guards
    tmp = Path(tempfile.mkdtemp(prefix="byom-neg-"))
    shutil.copytree(ROOT / "spec", tmp / "spec")
    path = tmp / "spec" / "descriptors" / "society.json"
    body = json.loads(path.read_text())
    body["transitions"][0]["guards"] = []
    path.write_text(json.dumps(body, indent=2, ensure_ascii=False) + "\n")
    rc = run([PY, str(ROOT / "conformance" / "run.py"),
              str(tmp / "spec")])
    report("conformance: descriptor guards emptied (society)", rc != 0)
    shutil.rmtree(tmp)


# ---------------------------------------------------------------- tlc ----

def tlc_fails(tmp: Path, spec: str) -> bool:
    return run(["java", "-XX:+UseParallelGC", "-cp",
                str(ROOT / "proof" / "tools" / "tla2tools.jar"),
                "tlc2.TLC", "-metadir", str(tmp / "states"),
                "-workers", "auto", "-deadlock", "-cleanup",
                "-config", str(tmp / f"{spec}.cfg"),
                str(tmp / f"{spec}.tla")]) != 0


def tlc_mutations():
    # 10. finalize without the complete seat set (R9 determinism)
    tmp = Path(tempfile.mkdtemp(prefix="byom-neg-"))
    for suffix in (".tla", ".cfg"):
        shutil.copy(ROOT / "proof" / "specs" / f"Pledge{suffix}",
                    tmp / f"Pledge{suffix}")
    text = (tmp / "Pledge.tla").read_text()
    needle = '  /\\ positions = Seats\n'
    assert needle in text
    (tmp / "Pledge.tla").write_text(text.replace(needle, "", 1))
    report("tlc: Pledge finalize seat guard dropped "
           "(FinalizedHasAllSeats counterexample)", tlc_fails(tmp, "Pledge"))
    shutil.rmtree(tmp)

    # 11. reclaim a LIVE leased head (D-RT-6 must catch it)
    tmp = Path(tempfile.mkdtemp(prefix="byom-neg-"))
    for suffix in (".tla", ".cfg"):
        shutil.copy(ROOT / "proof" / "specs" / f"EpisodeLease{suffix}",
                    tmp / f"EpisodeLease{suffix}")
    text = (tmp / "EpisodeLease.tla").read_text()
    needle = 'ReClaim(w) ==\n  /\\ lease = "lease_expired"'
    assert needle in text
    (tmp / "EpisodeLease.tla").write_text(text.replace(
        needle, 'ReClaim(w) ==\n  /\\ lease = "lease_leased"', 1))
    report("tlc: EpisodeLease reclaim guard weakened to the live head "
           "(ReclaimNeedsExpiryOrYield counterexample)",
           tlc_fails(tmp, "EpisodeLease"))
    shutil.rmtree(tmp)


def main(argv) -> int:
    parity_mutations()
    conformance_mutations()
    if "--no-tlc" in argv:
        print("tlc mutations skipped (--no-tlc)")
    else:
        tlc_mutations()
    total = caught + len(failures)
    print(f"negative-checks: {caught}/{total} mutations caught")
    if failures:
        print("result:   FAIL (harness is vacuous for: "
              + "; ".join(failures) + ")")
        return 1
    print("result:   PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
