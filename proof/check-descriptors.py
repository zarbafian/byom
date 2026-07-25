#!/usr/bin/env python3
"""Descriptor-model parity for the B0.1 TLA+ suite.

    python3 proof/check-descriptors.py            # repo layout next to this file
    python3 proof/check-descriptors.py --specs proof/specs --descriptors spec/descriptors

Every module under proof/specs/*.tla carries a machine-readable parity block
after its ``====`` terminator (TLA+ ignores everything past it):

    \\* @parity module: Pledge
    \\* @parity descriptor: pledge.json
    \\* @parity state: proposed
    \\* @parity transition: absent -> proposed via pledge_propose
    \\* @parity crash: absent -> proposed via pledge_propose = <crash_result>
    \\* @parity fences: absent -> proposed via pledge_propose = <f1> | <f2>
    ...

A module that models several committed descriptors repeats the
``@parity descriptor:`` header per descriptor (EpisodeLease folds three;
MembershipOfferStanding folds two). A model with no committed descriptor
machine declares ``@parity none:`` with its reason (BudgetConservation: the
section 11.4 ledger is not a section 14.8 transition machine).

``@parity crash:`` and ``@parity fences:`` transcribe the structured
section 14.8 columns per transition row (RT-09): the value after ``=`` is
the row's exact crash_result, respectively its fences list joined with
`` | `` (an empty fences list is written ``(none)``).

Checked, failing on any divergence (ADR-0003: a descriptor row with no model
transition, or vice versa, is a CI failure, not a review catch):

1. every proof/specs/*.tla module has exactly one parity block, and its
   ``@parity module:`` name equals the file name;
2. every ``@parity descriptor:`` names an existing spec/descriptors/*.json
   file, claimed by exactly one module;
3. per claimed descriptor, the annotated state set equals the descriptor's
   ``states`` exactly (both directions), and the annotated transition set
   equals the descriptor's ``(from, to, via)`` rows exactly (both
   directions);
4. every annotated state name appears as a quoted literal in the module body
   (before the terminator) — a state renamed in the model but not in its
   annotation fails here;
5. every claimed descriptor is a v2 descriptor (RT-09): format
   ``byom-descriptor/v2`` and, per transition row, the structured §14.8
   columns — guards (non-empty), locks, fences, events (non-empty), and a
   non-empty crash_result — so a modeled machine can never bind a
   descriptor that lost its guard/lock/fence/event/crash contract;
6. structured-column parity (RT-09): per claimed descriptor, every
   transition row's crash_result and fences columns equal the module's
   ``@parity crash:`` / ``@parity fences:`` transcriptions exactly (both
   directions) — a modeled machine's crash/fence semantics cannot be
   silently swapped for another legal value, let alone erased.

Honesty (ADR-0003, plan section 3): the parity block is a transcription of
the model's transition relation, compared mechanically against the committed
descriptors; check 4 is the only mechanical model-side guard. Editing a TLA+
action while keeping its annotation and the descriptor in agreement is
model-only drift this checker cannot see — that residue is recorded per
model in proof/PROPERTIES.md.
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

TERMINATOR = re.compile(r"^====+\s*$", re.M)
TAG = re.compile(r"^\\\* @parity (module|descriptor|state|transition|"
                 r"crash|fences|none):\s*(.*)$")
TRANSITION = re.compile(r"^(\S+) -> (\S+) via (\S+)$")
COLUMN = re.compile(r"^(\S+) -> (\S+) via (\S+) = (.+)$")
NO_FENCES = "(none)"  # annotation spelling of an empty fences list


class Checker:
    def __init__(self, specs_dir: Path, desc_dir: Path):
        self.specs_dir = specs_dir
        self.desc_dir = desc_dir
        self.failures: list[str] = []
        self.claimed: dict[str, str] = {}  # descriptor file -> module
        self.counts = {"modules": 0, "descriptors": 0, "states": 0,
                       "transitions": 0, "columns": 0, "column_parity": 0}

    def fail(self, message: str) -> None:
        self.failures.append(message)
        print(f"FAIL  {message}")

    # -- parsing --

    def parse_module(self, path: Path):
        """Return (body, module_name, groups, none_reason) where groups is
        [(descriptor_file, states, transitions, crash, fences)] in
        annotation order; crash and fences map (from, to, via) to the
        annotated column transcription (RT-09)."""
        text = path.read_text(encoding="utf-8")
        m = TERMINATOR.search(text)
        if m is None:
            self.fail(f"{path.name}: no module terminator found")
            return None
        body, tail = text[:m.start()], text[m.end():]
        module = None
        none_reason = None
        groups: list[tuple] = []
        current = None
        for line_no, line in enumerate(tail.splitlines(), 1):
            tag = TAG.match(line.strip())
            if tag is None:
                continue
            kind, value = tag.group(1), tag.group(2).strip()
            if kind == "module":
                if module is not None:
                    self.fail(f"{path.name}: duplicate '@parity module:'")
                module = value
            elif kind == "none":
                none_reason = value
            elif kind == "descriptor":
                current = (value, [], [], {}, {})
                groups.append(current)
            elif current is None:
                self.fail(f"{path.name}: '@parity {kind}:' before any "
                          "'@parity descriptor:'")
            elif kind == "state":
                current[1].append(value)
            elif kind == "transition":
                t = TRANSITION.match(value)
                if t is None:
                    self.fail(f"{path.name}: unparseable transition "
                              f"annotation {value!r}")
                    continue
                current[2].append((t.group(1), t.group(2), t.group(3)))
            else:  # crash / fences column transcription (RT-09)
                t = COLUMN.match(value)
                if t is None:
                    self.fail(f"{path.name}: unparseable '@parity {kind}:' "
                              f"annotation {value!r}")
                    continue
                row = (t.group(1), t.group(2), t.group(3))
                target = current[3] if kind == "crash" else current[4]
                if row in target:
                    self.fail(f"{path.name}: duplicate '@parity {kind}:' "
                              f"for {row}")
                    continue
                target[row] = t.group(4).strip()
        if module is None:
            self.fail(f"{path.name}: missing '@parity module:' annotation")
        elif module != path.stem:
            self.fail(f"{path.name}: '@parity module: {module}' does not "
                      f"match the file name")
        if none_reason is None and not groups:
            self.fail(f"{path.name}: neither '@parity descriptor:' nor "
                      "'@parity none:' declared")
        if none_reason is not None and groups:
            self.fail(f"{path.name}: '@parity none:' cannot coexist with "
                      "'@parity descriptor:'")
        return body, module, groups, none_reason

    # -- checks --

    def check_group(self, name: str, body: str, desc_file: str,
                    states: list[str], transitions, crash: dict,
                    fences: dict):
        if desc_file in self.claimed:
            self.fail(f"{name}: descriptor {desc_file} already claimed by "
                      f"{self.claimed[desc_file]}")
            return
        self.claimed[desc_file] = name
        path = self.desc_dir / desc_file
        if not path.is_file():
            self.fail(f"{name}: annotated descriptor {desc_file} does not "
                      f"exist under {self.desc_dir}")
            return
        descriptor = json.loads(path.read_text(encoding="utf-8"))

        ann_states, desc_states = set(states), set(descriptor["states"])
        if len(ann_states) != len(states):
            self.fail(f"{name}: duplicate '@parity state:' entries for "
                      f"{desc_file}")
        for s in sorted(desc_states - ann_states):
            self.fail(f"{name}: descriptor {desc_file} state {s!r} is not "
                      "modeled (missing '@parity state:')")
        for s in sorted(ann_states - desc_states):
            self.fail(f"{name}: modeled state {s!r} is not a state of "
                      f"descriptor {desc_file}")

        ann_rows = set(transitions)
        desc_rows = {(r["from"], r["to"], r["via"])
                     for r in descriptor["transitions"]}
        if len(ann_rows) != len(transitions):
            self.fail(f"{name}: duplicate '@parity transition:' entries for "
                      f"{desc_file}")
        for row in sorted(desc_rows - ann_rows):
            self.fail(f"{name}: descriptor {desc_file} row "
                      f"{row[0]} -> {row[1]} via {row[2]} is not modeled")
        for row in sorted(ann_rows - desc_rows):
            self.fail(f"{name}: modeled transition "
                      f"{row[0]} -> {row[1]} via {row[2]} is not a row of "
                      f"descriptor {desc_file}")

        # Model-side guard: every annotated state occurs as a quoted literal
        # in the module body, so a state renamed only in the model fails.
        for s in sorted(ann_states):
            if f'"{s}"' not in body:
                self.fail(f"{name}: annotated state {s!r} does not occur as "
                          "a quoted literal in the module body")

        # Descriptor format v2 (RT-09): the structured §14.8 columns must
        # be present on every row of a modeled machine, AND the crash and
        # fence columns must equal the model's transcription exactly.
        if descriptor.get("format") != "byom-descriptor/v2":
            self.fail(f"{name}: descriptor {desc_file} is not format "
                      "byom-descriptor/v2 (RT-09)")
        desc_crash: dict[tuple, str] = {}
        desc_fences: dict[tuple, str] = {}
        for i, r in enumerate(descriptor["transitions"]):
            where = f"{name}: {desc_file} transitions[{i}]"
            for key, min_items in (("guards", 1), ("locks", 0),
                                   ("fences", 0), ("events", 1)):
                val = r.get(key)
                if not (isinstance(val, list) and len(val) >= min_items
                        and all(isinstance(s, str) and s for s in val)):
                    self.fail(f"{where}: {key} must be a list of non-empty "
                              "strings"
                              + (" with at least one entry"
                                 if min_items else "") + " (RT-09)")
                else:
                    self.counts["columns"] += 1
            if not (isinstance(r.get("crash_result"), str)
                    and r["crash_result"]):
                self.fail(f"{where}: crash_result must be a non-empty "
                          "string (RT-09)")
            else:
                self.counts["columns"] += 1
            row = (r["from"], r["to"], r["via"])
            if isinstance(r.get("crash_result"), str):
                desc_crash[row] = r["crash_result"]
            if isinstance(r.get("fences"), list):
                desc_fences[row] = " | ".join(r["fences"]) or NO_FENCES
        # Structured-column parity (RT-09), both directions: every
        # descriptor row carries a crash/fences transcription that
        # matches, and every transcription names a descriptor row.
        for kind, desc_map, ann_map in (("crash", desc_crash, crash),
                                        ("fences", desc_fences, fences)):
            for row in sorted(desc_map):
                got = ann_map.get(row)
                if got is None:
                    self.fail(f"{name}: {desc_file} row {row[0]} -> "
                              f"{row[1]} via {row[2]} has no '@parity "
                              f"{kind}:' transcription (RT-09)")
                elif got != desc_map[row]:
                    self.fail(f"{name}: {desc_file} row {row[0]} -> "
                              f"{row[1]} via {row[2]} {kind} column "
                              f"diverges from the model transcription "
                              f"(RT-09)\n      descriptor: "
                              f"{desc_map[row]!r}\n      model:      "
                              f"{got!r}")
                else:
                    self.counts["column_parity"] += 1
            for row in sorted(set(ann_map) - set(desc_map)):
                self.fail(f"{name}: '@parity {kind}:' transcription for "
                          f"{row[0]} -> {row[1]} via {row[2]} is not a "
                          f"row of descriptor {desc_file}")

        self.counts["descriptors"] += 1
        self.counts["states"] += len(ann_states & desc_states)
        self.counts["transitions"] += len(ann_rows & desc_rows)

    def run(self) -> int:
        specs = sorted(self.specs_dir.glob("*.tla"))
        if not specs:
            self.fail(f"no specs found under {self.specs_dir}")
        modeled_none = []
        for path in specs:
            parsed = self.parse_module(path)
            if parsed is None:
                continue
            body, _module, groups, none_reason = parsed
            self.counts["modules"] += 1
            for desc_file, states, transitions, crash, fences in groups:
                self.check_group(path.name, body, desc_file, states,
                                 transitions, crash, fences)
            if none_reason is not None:
                modeled_none.append(path.stem)
        unmodeled = sorted(p.name for p in self.desc_dir.glob("*.json")
                           if p.name not in self.claimed
                           and p.name != "vocabulary.json")
        print(f"parity:   {self.counts['modules']} modules, "
              f"{self.counts['descriptors']} descriptors bound, "
              f"{self.counts['states']} states and "
              f"{self.counts['transitions']} transitions in exact "
              f"agreement ({self.counts['columns']} v2 structured columns "
              f"checked; {self.counts['column_parity']} crash/fence "
              "column transcriptions in exact agreement)")
        if modeled_none:
            print(f"no-descriptor models (declared '@parity none'): "
                  f"{', '.join(modeled_none)}")
        if unmodeled:
            print(f"descriptors without a B0.1 model ({len(unmodeled)}): "
                  f"{', '.join(unmodeled)}")
        if self.failures:
            print(f"result:   FAIL ({len(self.failures)} failure(s))")
            return 1
        print("result:   PASS")
        return 0


def main(argv: list[str]) -> int:
    root = Path(__file__).resolve().parent
    specs_dir = root / "specs"
    desc_dir = root.parent / "spec" / "descriptors"
    args = argv[1:]
    while args:
        flag = args.pop(0)
        if flag == "--specs" and args:
            specs_dir = Path(args.pop(0))
        elif flag == "--descriptors" and args:
            desc_dir = Path(args.pop(0))
        else:
            print(__doc__)
            return 2
    for d in (specs_dir, desc_dir):
        if not d.is_dir():
            print(f"FAIL  directory not found: {d}")
            return 1
    return Checker(specs_dir, desc_dir).run()


if __name__ == "__main__":
    sys.exit(main(sys.argv))
