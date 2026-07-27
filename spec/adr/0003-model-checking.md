# ADR-0003: Model checking and conformance oracle

Status: accepted (2026-07-26, RT-11; proposed 2026-07-25)
Date: 2026-07-25
Plan id: B-ADR-4

> **Erratum (2026-07-28) — a correction of fact, not a supersession.** The
> decision below stands unchanged; this note exists because two of its
> sentences name a command that cannot run.
>
> This ADR cites `cargo test -p bpp-spec` twice (§Criteria, and again as the
> B1 acceptance gate). **There has never been a `bpp-spec` crate.** The
> workspace is `bpp-core`, `byom-store`, `byomd`, `byom-cli`, `byom-mcp`. Read
> those two citations as:
>
> ```
> python3 proof/check-descriptors.py    # descriptor/model parity
> python3 conformance/run.py            # the conformance suite
> ```
>
> The stronger point the same sweep found, and which this ADR should not be
> read as claiming: **no test in `crates/` reads a descriptor or a TLA+ model
> at all.** The Rust tests `include_str!` schemas, vectors, the registry and
> the MCP document — never `spec/descriptors/` or `proof/specs/`. The
> conformance oracle this ADR describes was expected to land with byomd; byomd
> landed and the oracle did not. That gap is recorded as the largest open item
> in `proof/PROPERTIES.md`. Superseding the decision itself would need a new
> ADR linking both ways, per `spec/adr/README.md`; nothing here does that.

## Context

DESIGN.md §14.8 closes every state machine (an unlisted transition is
invalid) and requires the critical Mandate, Pledge, Episode, ActIntent,
Assembly, and budget machines to be model-checked for invariant
preservation, dead transitions, replay, and crash at every commit and
external-call boundary — and makes the machine-readable descriptors and
model checks, not daemon behavior, decide conformance. The B0.1 sheet lists
the concrete models: MembershipOffer/Standing + OnboardingActivationOffer,
Pledge (13 states including `disputed`), ActivityStream/Episode lease,
ActIntent/permit, Mandate chain (never-widening derivation), budget
conservation (`ceiling = remaining + reserved + committed + uncertain +
delegated_to_children`), and the authority journal — each with crash and
replay vectors and a proof README.

The sibling akson project already runs this pattern end to end
(`akson/proof/`): TLC exhaustively checks each spec, Apalache discharges
inductive proofs where TLC's run-length bound bites, `negative-checks.sh`
mutations prove the harness is not vacuous, and a `conformance/` workspace
member ties the implementation's pure transition functions to the models in
plain `cargo test`. Evaluated alternatives: Alloy (weaker
crash/interleaving story for this shape of machine), P (would couple the
spec to one implementation runtime), and hand-written proofs (not
mechanically re-checkable in CI). Akson's pattern is proven in-family and
its tooling costs are known.

## Decision (accepted)

- **TLA+** is the modeling language. Each B0.1 machine gets one spec plus
  TLC config under `spec/models/`, exploring crashes at every commit and
  external-call boundary, message replay, and adversarial interleaving;
  Apalache inductive proofs lift bound-sensitive invariants (budget
  conservation, never-widening Mandate derivation) beyond TLC's run length
  where feasible. Negative checks (deliberate mutations that must yield
  counterexamples) guard against a vacuous harness, mirroring akson's
  `negative-checks.sh`.
- An **in-workspace conformance oracle** ties models to code: a workspace
  member (`cargo test -p bpp-spec` reaches it, per the B0.1 sheet's
  descriptor-parity gate) asserts, for every (state, event) pair, that the
  implementation's pure transition functions equal the TLA+ transition
  relations, and that the machine-readable transition descriptors in
  `spec/descriptors/` agree with both. A change to either side that forgets
  the other fails plain workspace tests — no Java or model-checker install
  needed on that path.
- Akson's **honesty note is adopted as a normative requirement**, not
  prose: a hand-transcribed oracle catches **code drift** (an implementation
  transition contradicting the transcribed relation fails the suite) but not
  **model-only drift** — editing a `.tla` action and nothing else does not
  fail conformance, because the transcription is a third, independent copy.
  Therefore Byom **generates the oracle table from the model where
  feasible** (the descriptor registry is machine-readable precisely so the
  source→target relation can be emitted, compared, or generated rather than
  re-typed); wherever generation is not yet feasible, the proof README must
  say so and name the drift that remains uncaught.
- **Every proof README states**, per model: the **projection** (which
  implementation state is abstracted away and why that is sound), the
  **refinement boundary** (which code artifacts are claimed to refine the
  model, and where the claim stops), the **fairness assumptions** behind any
  liveness claim (safety claims assume none), and the **bounded-state
  coverage** (exact constants/bounds TLC explored, and which invariants are
  additionally inductive and therefore hold for any run length).

## Criteria (met at acceptance — RT-11)

Accepted with all spec-side criteria satisfied within B0.1:

- every machine listed in the B0.1 sheet has a spec, a TLC config that
  completes with zero errors in CI (`.github/workflows/ci.yml`
  `model-checking` job: `make -C proof full`), and crash + replay vectors
  (`spec/vectors/machines/`);
- each model's proof README section contains all four required statements
  (projection, refinement boundary, fairness, coverage —
  `proof/PROPERTIES.md`);
- the negative-check suite (`proof/negative-checks.py`, in CI and
  `run-checks.sh`) produces a caught failure for every deliberate
  parity, conformance, and TLC mutation, plus the RT-16 MCP widening
  mutations inside `conformance/run.py`;
- descriptor parity holds and covers the v2 structured columns:
  descriptors ↔ registry (`spec/registry.json`, RT-12) ↔ modeled
  transitions (`proof/check-descriptors.py`; `conformance/run.py`).

The originally proposed "in-workspace conformance oracle over every
(state, event) pair, generated from the model for at least one machine"
criterion was CIRCULAR at B0.1: it names the B1 workspace's pure
transition functions, which cannot exist before B1 lands. Reworded (RT-11
disposition): what B0.1 proves mechanically NOW is descriptor ↔ model
parity plus the registry derivation; the **code oracle is a B1
acceptance gate** — B1 does not freeze until `cargo test -p bpp-spec`
asserts, for every (state, event) pair of every modeled machine, that the
implementation's pure transition functions equal the committed descriptor
rows (generated, not re-typed, wherever feasible; hand-transcribed
remainders flagged in the proof README). That gate is recorded here so
acceptance of this ADR cannot be read as discharging it.

## Consequences

- Reuses akson's known-cost toolchain (TLC, Apalache, mutation checks);
  reviewers can read both proof trees the same way.
- The spec's closed-transition rule becomes mechanically enforceable: a
  descriptor row with no model transition, or vice versa, is a CI failure,
  not a review catch.
- Honest limits are recorded where they bind: conformance guarantees "code
  cannot silently diverge from the transcribed/generated relation"; it does
  not guarantee the model matches the English design — that remains review,
  which the proof READMEs must not overstate.
- Generating oracle tables from models adds build machinery, accepted to
  shrink the third-copy problem that akson documented (the drift class that
  actually bit them).
- Model checking joins the bundle-freeze gate: a frozen B0.1 cannot ship
  with a red model, and later machine changes require re-checking before any
  new bundle freeze.
