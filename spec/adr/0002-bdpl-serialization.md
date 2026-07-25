# ADR-0002: BDPL serialization

Status: proposed
Date: 2026-07-25
Plan id: B-ADR-2

## Context

DESIGN.md §10.4 defines the Byom Deterministic Procedure Language (BDPL):
digest-pinned `ProcedureDefinition` records carry `seat_expression`,
`eligibility_expression`, `outcome_expression`, and
`tie_and_expiry_expression`, and BDPL is total, deterministic,
side-effect-free, non-recursive, and bounded by `maximum_inputs` and
`maximum_steps`. It has no ambient network, clock, model, mutation,
credential, context, or randomness source. §10.5 requires every
digest-pinned BDPL reference to be exact. What the design does not fix is the
serialization of a BDPL procedure body — B0.1 must freeze it only **as far as
B1 uses it** (the four expression slots of `ProcedureDefinition` and their
digests); richer procedure libraries are later bundles.

Evaluated: (a) a human-oriented text syntax frozen now — rejected: it front-
loads grammar, parsing, and formatting compatibility before any Society
authors a procedure, and a parser bug would sit on the authority path;
(b) compiled bytecode — rejected: opaque in the journal, hostile to review
and vectors; (c) the same I-JSON AST approach as BPA-1 (ADR-0001) — chosen.

## Decision (proposed)

- A BDPL procedure body is an **I-JSON AST**: typed expression nodes as JSON
  objects with a required node-kind tag from a closed enum, canonicalized via
  RFC 8785 JCS under §14.2's strict I-JSON acceptance, digest-pinned with the
  shared type-tag construction and its own digest domain (fixed with the
  first schema that embeds a body).
- The AST admits only constructs that keep evaluation **total,
  deterministic, side-effect-free, and bounded**:
  - no unbounded loops and no recursion — no node kind can reference a
    procedure or expression by name; iteration exists only as bounded
    fold/map over already-frozen input collections whose size is capped by
    `maximum_inputs`;
  - every evaluation charges steps against the pinned `maximum_steps`, and
    §14.9's caps on BDPL policy nodes and evaluation steps bound the AST
    itself; exceeding any bound is a typed failure (`policy_overflow`), never
    a partial result;
  - node kinds cover only what §10.4 grants BDPL: selecting already eligible
    seats, counting separately authored Position values, rotation, and
    computing a typed outcome — there is no node for I/O, time, randomness,
    text interpretation, or state mutation, so purity is a property of the
    closed grammar, not of evaluator discipline;
  - typed parameters enter only through the declared
    `typed_parameters_schema_ref`; seeds enter only as the exact admitted
    seed of a `ProcedureSeedAdmission` (§10.4) — the AST cannot name any
    other entropy source.
- **Explicit resource bounds per evaluation**: an evaluation's inputs are the
  frozen eligibility/position snapshot, the typed parameters, and (for lot
  selection) the admitted seed; its budget is the pinned
  (`maximum_inputs`, `maximum_steps`) pair plus the protocol-level node and
  step caps. Two conforming evaluators given the same pinned body and inputs
  must produce byte-identical typed outcomes or the same typed failure.
- **Text surface syntax is deferred.** No human-readable BDPL grammar is
  frozen in B0; if one is added later it compiles to this AST, the AST digest
  remains the only authority-bearing identity, and the surface syntax gets
  its own ADR.

## Criteria

Moves to `accepted` when, within B0.1 (scoped to B1's use):

- the AST JSON Schema for the four `ProcedureDefinition` expression slots is
  published with valid vectors and rejecting vectors (recursion attempt via
  self-reference shape, over-`maximum_steps` body, unknown node kind, float
  literal);
- the digest domain for BDPL bodies is frozen and its derivation vectors
  re-derive in `conformance/run.py`;
- two independent evaluators agree on every evaluation vector, including
  step-exhaustion and input-cap boundary cases;
- a static bound-checker (AST → max step cost) exists so `maximum_steps`
  violations are rejectable at `procedure_propose` time, not only at
  evaluation time.

## Consequences

- BDPL reuses the exact canonicalization, digesting, fail-closed and
  fuzzing machinery of ADR-0001; the two algebras stay structurally alike.
- Journals and reviews see procedures as inspectable JSON; a
  `ProcedureDefinition`'s BDPL body never mutates in place (§14.8) and its
  digest is stable across languages.
- Deferring text syntax means early procedures are authored as raw AST
  (tooling may pretty-print); this is deliberate friction while the set of
  real procedures is small.
- The closed node-kind enum is wire compatibility once frozen: new node
  kinds are a new BDPL version negotiated as a feature bundle, and an
  evaluator must reject unknown kinds rather than skip them.
- Boundedness by construction makes worst-case evaluation cost computable
  before adoption, which the §14.9 per-request database/evaluation budgets
  and the `procedure_propose` rate ceilings depend on.
