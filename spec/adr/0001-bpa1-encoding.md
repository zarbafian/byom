# ADR-0001: BPA-1 policy algebra encoding

Status: proposed
Date: 2026-07-25
Plan id: B-ADR-1

## Context

DESIGN.md §10.5 fixes Byom Policy Algebra v1 (BPA-1) semantically: every
authority-bearing selector is a bounded union of positive atoms plus explicit
deny atoms, deny wins, the atom types form a closed table of twelve domains,
and `intersect(a,b)` / `is_subset(child,parent)` are total functions over
canonical values — unknown types, unresolved aliases, mutable collections,
floating-point quantities, and incomparable values reject. What §10.5 does not
fix is the wire and storage encoding: how a selector is serialized, how its
canonical bytes and digest are formed, and how implementations exchange and
fuzz policy values. B0.1 must freeze that encoding because Mandate,
self-policy, and registry schemas embed policy values, and cross-language
vectors (two independent policy evaluators, per the B0.1 verification gate)
need exact bytes.

Evaluated: (a) a custom binary encoding — rejected: a second canonicalization
stack, no incremental inspectability, harder fuzzing; (b) a text surface
language — rejected for authority-bearing values: §10.5 explicitly excludes
free text and regexes from policy values, and parsing would precede
authorization; (c) I-JSON AST objects canonicalized with the existing RFC 8785
JCS rule from §14.2 — chosen.

## Decision (proposed)

- A BPA-1 policy value is an **I-JSON AST object**: a `selector` node holding
  a bounded array of positive atom nodes and a bounded array of deny atom
  nodes. Each atom node is an object with a required type tag drawn from a
  **closed enum** mirroring §10.5's twelve domains:
  `operation`, `object`, `path`, `network_destination`, `binding`, `purpose`,
  `classification`, `time`, `quantity`, `rate`, `assurance`,
  `schema_evidence` (exact tag spellings freeze with the bundle registry).
  An unknown type tag is not extensible data — it fails closed.
- Every atom node's fields are the canonical fields of its §10.5 row (e.g.
  `rate` carries the exact integer `RateCeiling`; `quantity` carries a
  non-negative fixed-scale integer plus dimension and canonical unit; free
  text and display names never appear in the AST — display fields live
  outside policy values and are ignored by the evaluator).
- Canonical bytes are **RFC 8785 JCS** over the AST under the strict I-JSON
  acceptance rules of §14.2 (no floats: all quantities are fixed-scale
  integers). The policy digest uses the shared type-tag construction
  (`spec/README.md`): `SHA-256(JCS(["bpa1-policy-v1", ast]))` carried as a
  typed `DigestRef` whose digest **domain is `bpa1-policy-v1`**; the class is
  chosen per §14.2's digest-class table by the embedding record.
- `intersect(a, b)` and `is_subset(child, parent)` are **total pure
  functions over the AST**: no I/O, no clock, no name resolution, no
  implementation callbacks. A child is a subset only when each positive atom
  is covered by a parent atom and every applicable deny is preserved; any
  incomparable or unknown input returns a typed rejection value (never an
  exception path that differs between implementations).
- The encoding is **fuzzable by construction**: the AST is closed, bounded
  (§14.9 caps policy nodes and evaluation steps), and schema-described, so
  structure-aware fuzzers can generate valid and near-valid values, and the
  algebra laws (idempotence, commutativity and associativity of `intersect`,
  `is_subset` as a partial order, deny preservation under intersection) are
  executable properties.

## Criteria

Moves to `accepted` when, within B0.1:

- the AST JSON Schema is published in `spec/schemas/` and every atom type of
  §10.5 has at least one valid and one rejecting vector;
- algebra-law vectors (including deny-wins and incomparable-reject cases) are
  frozen and the two independent policy evaluators required by the B0.1
  verification gate agree on every vector;
- `bpa1-policy-v1` digest vectors re-derive in `conformance/run.py`;
- a fuzz harness runs `intersect`/`is_subset` over generated ASTs with no
  crash and no panic-as-rejection ambiguity.

## Consequences

- One canonicalization stack (JCS) serves envelopes, idempotency domains, and
  policy digests; no second byte format to prove.
- Policy values are inspectable JSON everywhere they are stored or journaled,
  which the deny-by-absence registry and PreparationTrace rely on.
- Irreversible once frozen: atom tag spellings and field names become wire
  compatibility; adding an atom type is a new algebra version (BPA-2), never
  an in-place extension, because the enum is closed.
- Evaluators must implement JCS exactly; the akson-published JCS vectors plus
  BPP digest vectors are the guard.
- Text surface syntax for humans, if ever wanted, is a display concern layered
  on top and needs its own ADR; it can never be the authority-bearing value.
