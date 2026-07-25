# Amendment: family-contract alignment

Status: proposed (C0 revision work; becomes normative on C0 ratification)

Date: 2026-07-25

Amends: `byom/DESIGN.md` at sha256
`ccea384ff931bcf45d30df680b86835ac682006072a07ef2f34f565eba5fa501`.
Authority: the family contract (`byom/design/2026-07-25-family-contract.md`)
and plan decisions D1, D9, D10 (`2026-07-25-kovee-byom-implementation-plan.md`,
sha256 `d5e73952ac90a67a4e4a060052ca66d4be729dec500436b86623b89c54afb2d2`).

## A1 — Naming normalization (plan D9)

README/DESIGN references to the gateway via the historical `../axon` path read
`../akson`; the Rust gateway is **Akson** (`aksond`, `akson-*`). The external
`akson-ai` Python project remains authoring inspiration only.

## A2 — Kovee integration is greenfield-first (plan D1, D10)

§16's Kovee prerequisites are being implemented against a kovee that has
**byom as its governance owner from day one** (never Sage). Consequences:

- §25's Sage migration (`GovernanceCutover`, `sage → none → byom`) remains
  specified but **unbuilt**; no milestone in the current program exercises the
  `sage` arm of `KoveeGovernanceOwnerBinding`.
- A **greenfield enablement saga** (`none → byom` without a Sage
  predecessor) is added to the `byom_governed_work_v1` bundle (C2): create
  `KoveeRealmByomBinding` + `KoveeSocietyMapping`, then CAS the owner binding
  `none → byom`, with exact-CAS, retry, overlapping-scope rejection,
  rollback-before-activation, and restore behavior specified and
  model-checked. Kovee is never the genesis governance actor (§16.3's
  `society_prepare`/`society_bootstrap` rule is unchanged and controlling).
- §26's open dependency "Kovee must adopt the Byom adapter and BPP
  prerequisites" is tracked as C-track milestones C2/C3 with kovee K2/B3
  consuming them; the Akson least-privilege surface dependency is tracked as
  A0/C4.

## A3 — Review-cadence completion

The program adds **RC4**, a contract review of the frozen
`akson_byom_exchange_v1` surface that makes C4 implementation-ready for its
consumers (K6/B5) — the plan's §8 table named only R0–R5; `plan/dag.json`
carries the node.

## Follow-through

Folded into DESIGN.md at the next design revision (v0.3); until then this
record rides alongside the pinned v0.2 text. R0 reviews it with the family
contract.
