# ADR-0001: BPA-1 policy algebra encoding

Status: accepted
Date: 2026-07-25 (proposed) / 2026-07-26 (accepted)
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
self-policy, and registry schemas embed policy values (the G10/G31 opaque
bodies in `spec/schemas/ops/`), and cross-language vectors (two independent
policy evaluators, per the B0.1 verification gate) need exact bytes.

Evaluated: (a) a custom binary encoding — rejected: a second canonicalization
stack, no incremental inspectability, harder fuzzing; (b) a text surface
language — rejected for authority-bearing values: §10.5 explicitly excludes
free text and regexes from policy values, and parsing would precede
authorization; (c) I-JSON AST objects canonicalized with the existing RFC 8785
JCS rule from §14.2 — chosen.

## Decision

Frozen concretely in `spec/schemas/bpa1-policy.schema.json` (closed AST),
`policy/eval.py` (reference evaluator), `policy/eval.mjs` (independent
evaluator), and `spec/vectors/policy/` (golden vectors). The normative shape:

- A BPA-1 policy value is an **I-JSON AST object** `{"rules": [...]}`: a
  bounded set of rules, each `{"effect": "allow"|"deny", "atoms": {...}}`.
  `atoms` constrains a subset of the twelve §10.5 domains; an omitted domain
  is unconstrained (top), an empty `atoms` object is the universal rule, and
  a rule is the conjunction of its constrained domains. This realizes
  §10.5's "bounded union of positive atoms plus explicit deny atoms": the
  allow rules are the union's positive terms, the deny rules the explicit
  denies, and deny wins (absence of a matching allow is deny).
- The atom keys are a **closed enum** with these frozen tag spellings for
  the §10.5 twelve-domain table (extracted verbatim below); an unknown key
  is not extensible data — it fails closed. Adding an atom type is a new
  algebra version (BPA-2), never an in-place extension.

  | Tag | §10.5 row (verbatim) | Frozen atom encoding |
  |---|---|---|
  | `operation` | Versioned BPP operation id; subset is set inclusion. | `{ids: [opId...]}` explicit finite set; subset = set inclusion |
  | `object` | Exact source-qualified object id or server-expanded immutable collection snapshot; no mutable alias. | `{ids: [sourceQualifiedId...]}`; a pinned collection snapshot is server-expanded to exact ids at preparation with its transform id in the PreparationTrace (§10.5), so no alias or unexpanded reference reaches the algebra |
  | `path` | Unicode-normalized logical path segments relative to an exact WorkspaceAllocation root plus `exact` or `subtree`; string comparison never authorizes a filesystem open. | `{root, segments[], match: exact\|subtree}`; segments NFC-normalized (evaluator-checked), never `.`/`..`/`/`/controls; different roots are disjoint |
  | `network_destination` | Scheme, A-label hostname or normalized IP/CIDR, port/range, and protocol; DNS resolution is pinned by the broker and private/special ranges require explicit atoms. | `{scheme, host: {dns}\|{ip4_cidr}\|{ip6_cidr}, ports: {first,last}, protocol}`; CIDRs structured (octets/groups + prefix_len, host bits zero); DNS vs IP/CIDR is incomparable (the algebra never resolves), ip4 vs ip6 decidably disjoint |
  | `binding` | Source-qualified immutable binding id; display names have no policy role. | `{ids: [sourceQualifiedId...]}` (covers provider/region/recipient) |
  | `purpose` | Exact purpose ref or descendant in a pinned acyclic purpose snapshot. | `{snapshot: DigestRef, path: [ref...]}` — the root-to-node ancestor chain is materialized from the pinned snapshot at preparation, so descendant testing is pure path-prefix comparison; different snapshots are incomparable |
  | `classification` | Element of a pinned finite lattice; restriction order is the lattice order. | `{lattice: DigestRef, allowed: [element...]}` — the explicit downward-closed set expanded against the pin at preparation (finite because the lattice is), absorbing the lattice order into set inclusion; different lattices are incomparable |
  | `time` | Closed UTC server-time interval; a child interval must be contained. | `{not_before, not_after}` integer server-time milliseconds, both endpoints inclusive |
  | `quantity` | Non-negative fixed-scale integer plus dimension and canonical unit; money additionally names ISO currency and pricing revision. | `{dimension, canonical_unit, scale, max}` ceiling (+ `currency`, `pricing_revision` exactly when dimension is `money`); any mismatch of those keys is incomparable (currency conversion rejects) |
  | `rate` | Exact integer token-bucket `RateCeiling` and authority-server epoch; subset uses capacity/refill/burst containment. | the §10.5 RateCeiling record verbatim: `{dimension, canonical_unit, capacity, refill_amount, refill_period_milliseconds, max_burst, epoch, clock: "authority_server"}`; different dimension/unit/epoch incomparable |
  | `assurance` | Element of a pinned finite refinement order; incomparable profiles reject. | `{order: DigestRef, admitted: [profile...]}` — the explicit upward-closed set expanded against the pin at preparation; different orders are incomparable |
  | `schema_evidence` | Exact immutable schema, verifier, attestor, and assurance-policy digests. | `{schema, verifier, attestor, assurance_policy}`, four typed DigestRefs; subset/meet is exact tuple equality |

  Every pinned structure (lattice, purpose snapshot, refinement order,
  evidence digests) is a typed family `DigestRef` (PROFILE.md §6.1,
  normative — never an unlabelled hash); atom comparability keys on exact
  DigestRef equality. Free text, regexes, display names, floats, and
  callbacks never appear in the AST (§10.5); all quantities are fixed-scale
  I-JSON safe integers.
- **Canonical bytes and digest.** Canonical form sorts set members by
  UTF-16 code units, sorts rules by their JCS bytes, and rejects duplicates;
  canonical bytes are **RFC 8785 JCS** under the strict I-JSON acceptance
  rules of §14.2 (no floats). The policy digest is derived over the
  **`$domain`-tagged** canonical form with digest domain **`bpa1-policy-v1`**
  per the ratified family profile (PROFILE.md §2, D-R0-1) — this supersedes
  this ADR's originally proposed `SHA-256(JCS(["bpa1-policy-v1", ast]))`
  spelling, exactly as R0/BYOM-01 superseded it for the idempotency domain.
  The digest is carried as a typed `DigestRef` whose class is chosen per
  §14.2's digest-class table by the embedding record.
- **Total algebra.** `intersect(a,b)`, `is_subset(child,parent)`, and
  `decide(policy, request_atoms)` are total pure functions over the AST: no
  I/O, no clock, no name resolution, no callbacks, and no exception escapes.
  Every call returns a result or the typed rejection
  `{kind: malformed|overflow|incomparable, where}` (§14.9 mapping:
  `invalid` / `policy_overflow` / `policy_conflict`); malformed input fails
  closed at the first offending location in a fixed validation order, and a
  comparability pre-pass in fixed scan order makes the first incomparable
  conflict identical across implementations. `is_subset` requires every
  child allow rule to be covered by a parent allow rule (a domain the parent
  constrains must be constrained at least as tightly by the child — absence
  is wider, per §10.2/G33) and every applicable parent deny to be preserved;
  `intersect` is the pairwise meet of allow rules plus the union of both
  deny sets (deny preservation); `decide` is deny-wins with default deny,
  where an allow rule requires the constrained request value to be present
  and matching while a deny rule conservatively matches an absent value.
- **Fuzzable by construction.** The AST is closed, bounded (256 rules, 256
  set members, 64 path/purpose segments — §14.9's caps pinned here pending
  registry freeze), and schema-described; the algebra laws (commutativity of
  `intersect` up to canonical bytes, meet-below-factors, deny preservation,
  decide-conjunction) are executable properties checked by the seeded
  differential harness.

## Criteria — met

Each acceptance criterion of the proposed ADR, with its evidence:

- AST JSON Schema published, every atom type with valid + rejecting vectors:
  `spec/schemas/bpa1-policy.schema.json`; `spec/vectors/policy/`
  `schema-valid-*` / `schema-invalid-*` (all twelve domains, plus
  requestAtoms).
- Algebra-law vectors frozen, two independent evaluators agree on every
  vector: `subset-*`, `intersect-*`, `decide-*`, `malformed-*`,
  `overflow-*` vectors (deny-wins, deny preservation, incomparable-reject,
  universal/empty/contradictory adversarial cases); `conformance/run.py`
  re-derives every vector through `policy/eval.py` and cross-checks
  `policy/eval.mjs` batch-wise; `run-checks.sh` additionally runs
  `node policy/eval.mjs check spec/vectors/policy`.
- `bpa1-policy-v1` digest vectors re-derive in `conformance/run.py`: the
  `canonical-*` vectors pin canonical bytes, `$domain`-tagged bytes, and
  SHA-256 hex, re-derived by both evaluators.
- Fuzz harness with no crash and no panic-as-rejection ambiguity:
  `policy/differential.py` (seeded LCG, structured generator with ~20%
  malformed mutants) executes every case through both evaluators and
  requires byte-identical JCS results — typed rejections included — plus
  the algebra laws above; pinned in `run-checks.sh`
  (`--seed 45217 --cases 256`).

## Open items

- **Rate boundary-alignment tightening.** §10.5's "cannot exceed the parent
  under any boundary alignment" and its "active interval, and reserved
  parent share" terms need mandate context outside the atom. BPA-1 freezes
  the algebraic subset as the row's componentwise containment (capacity,
  max_burst, refill rate by exact integer cross-multiplication); worst-case
  per-window dominance and interval/share accounting remain consume-time
  checks under §10.5's atomic ancestor-counter locking, with their own
  golden vectors (boundary double bursts, clock skew, crash) landing with
  that slice. Any algebraic tightening is BPA-2.
- **Numeric caps are interim pins.** 256 rules / 256 set members / 64
  segments concretize §14.9's "bounded" language; the registry may re-pin
  them at bundle freeze — a re-pin is a new schema version, not an edit.
- **G10/G31 rebinding.** Published ops schemas are immutable, so the opaque
  policy bodies (`terms_constraints`, `manifestation_selector`, …) keep
  their open-object spelling until their next schema version, which binds
  them to `bpa1-policy.schema.json` at the registry freeze (B1).

## Consequences

- One canonicalization stack (JCS + `$domain` tagging) serves envelopes,
  idempotency domains, and policy digests; no second byte format to prove.
- Policy values are inspectable JSON everywhere they are stored or
  journaled, which the deny-by-absence registry and PreparationTrace rely
  on; preparation (not the algebra) performs snapshot expansion,
  lattice/order set expansion, and purpose-path materialization, recorded
  with transform ids (§10.5).
- Irreversible now frozen: the twelve atom tag spellings and field names
  above are wire compatibility; adding an atom type is BPA-2, never an
  in-place extension, because the enum is closed.
- Evaluators must implement JCS and the fixed validation/scan orders
  exactly; the policy vectors plus the seeded differential are the guard —
  `python3 conformance/run.py` and `./run-checks.sh` hold both shipped
  evaluators to byte-identical results.
- Text surface syntax for humans, if ever wanted, is a display concern
  layered on top and needs its own ADR; it can never be the
  authority-bearing value.
