# Architecture decision records

An ADR records one decision with lasting consequences: wire formats, encoding
choices, verification posture. Byom wire formats additionally must satisfy the
namespace gate in `../README.md` before shipping in a stable release.

Process:

1. Copy the template below into `NNNN-short-title.md` (next free number).
2. Open a PR. The ADR is `proposed` until merged with maintainer approval,
   then `accepted`. Superseding requires a new ADR that links both ways.
3. Security-relevant ADRs list the affected threat cases and test vectors.
4. A `proposed` ADR states **Criteria**: the observable conditions under
   which it moves to `accepted`. Resolving a plan-level B-ADR id means the
   file listed against it here is accepted.

Template:

~~~markdown
# ADR-NNNN: title

Status: proposed | accepted | superseded by ADR-MMMM
Date: YYYY-MM-DD
Plan id: B-ADR-K (if the B0 plan names this decision)

## Context
What requirement forces a decision, and what was evaluated.

## Decision (proposed)
The choice, stated normatively.

## Criteria
Observable conditions for proposed → accepted.

## Consequences
What becomes easier, harder, or irreversible; affected tests/vectors.
~~~

Index (file number ↔ B0 plan id — the plan skips B-ADR-3 in this bundle, so
file numbers and plan ids diverge after 0002):

| # | Plan id | Title | Status |
|---|---|---|---|
| [0001](0001-bpa1-encoding.md) | B-ADR-1 | BPA-1 policy algebra encoding | accepted |
| [0002](0002-bdpl-serialization.md) | B-ADR-2 | BDPL serialization | proposed |
| [0003](0003-model-checking.md) | B-ADR-4 | Model checking and conformance oracle | accepted |
