# Properties: design ↔ descriptors ↔ model

Traceability for every machine-checked property of the B0.1 model suite, plus
what remains. "§" refers to `../DESIGN.md`; "G*" gap notes to
`../spec/schemas/ops/README.md`; descriptors to `../spec/descriptors/*.json`;
the ground rules to `../spec/adr/0003-model-checking.md` (B-ADR-4) and plan
§3 ("What we take from akson — deliberately, and honestly").

## Machine-checked status

All seven B0.1 sheet models were checked with TLC (exhaustive breadth-first
over the configured constants, `-deadlock`, `tla2tools.jar` via
`make -C proof` / `./check.sh <Spec>`) on 2026-07-26, zero errors. None is
authored-only.

| Model | Distinct states | States generated | Graph depth |
|---|---:|---:|---:|
| `specs/MembershipOfferStanding.tla` | 93 | 189 | 8 |
| `specs/Pledge.tla` | 40 | 101 | 14 |
| `specs/EpisodeLease.tla` | 33,488 | 176,434 | 20 |
| `specs/ActIntentPermit.tla` | 61 | 116 | 10 |
| `specs/MandateChain.tla` | 7,988 | 18,011 | 11 |
| `specs/BudgetConservation.tla` | 600 | 2,937 | 12 |
| `specs/AuthorityJournal.tla` | 2,902 | 6,933 | 17 |

C2 slice 1 adds one model beyond the B0.1 sheet, checked the same way
(TLC exhaustive, `-deadlock`, zero errors, 2026-07-26):

| Model | Distinct states | States generated | Graph depth |
|---|---:|---:|---:|
| `specs/GreenfieldEnablement.tla` | 65 | 121 | 11 |

**Liveness: none is claimed, anywhere.** Every checked property is safety
(state invariants plus `[][...]_vars` action invariants, which are safety);
no fairness is assumed by any spec. Honest reason: every model treats daemon
state as durable and a byomd crash as stuttering, and a specification whose
crashes are stutter steps can promise no progress without fairness
assumptions on a daemon that does not exist yet. Liveness claims (e.g.
"every queued Episode eventually terminates or expires") land with byomd's
sweeper/deadline behavior in B1, with their fairness assumptions stated then,
akson's `TaskLiveness.tla` being the template for stating them honestly.

**Code conformance lands with byomd (applies to every model below).** There
is no implementation in this repo yet, so there is no code-side conformance
oracle: the refinement boundary of every model currently stops at the
committed transition descriptors, checked mechanically by
`check-descriptors.py` (below). When byomd lands (B1), the ADR-0003
requirement binds: its pure transition functions must be tied to these
models in the default workspace test invocation (`cargo test -p bpp-spec`
per the B0.1 sheet), generated from the descriptor registry where feasible,
and any hand-transcribed remainder must be flagged here per machine.

## Checked

### specs/MembershipOfferStanding.tla — offer, acceptance, admission, Standing

One MembershipOffer, its coupled OnboardingActivationOffer, one Standing
(§7.4, §14.8). The admit/refuse/revoke/expiry race on the same offer
revision (§7.4's CAS) is faithful because all four actions guard on the same
state variable; refusal/revocation fence the onboarding offer in the same
transaction (the descriptor cascade rows).

| Model invariant | Design | Descriptor rows |
|---|---|---|
| `StandingRequiresAdmission` | §7.4 Standing exists only through admission | `participant_admit` (both rows, one transaction) |
| `AdmissionRequiresAcceptance` | §7.4 only the candidate authors acceptance | `membership_accept` |
| `SilenceNeverAccepts` | §7.4/§14.8 silence never becomes acceptance | `server_time` rows |
| `AtMostOneStanding` | §7.4 one StandingRevision through the CAS | admit vs refuse/revoke/expiry fan-out |
| `RefusalFencesOnboarding`, `RevocationFencesOnboarding` | §7.4 refusal is real; minimum revocation set | `membership_refuse`/`membership_offer_revoke` cascades |
| `OneComputeUse` | §7.4 one zero-general-effect compute permit | `onboarding_compute_permit_consume` |
| `CompletionIsNotAcceptance` | §14.3 completion is evidence only | `onboarding_episode_complete` |

**Projection**: one offer, one candidate, one onboarding offer, one
Standing; offer revisions, subject digests, decisions, channels, and fences
are abstracted to the state enums; replays are free (every guard makes the
exact retry a no-op — the descriptor-level "exact retry returns the same
receipt" claim). **Refinement boundary**: `membership-offer-standing.json`
and `onboarding-activation-offer.json` (exact, machine-checked); no code is
claimed to refine this model yet (see the byomd note above). **Fairness**:
none; safety only. **Coverage**: no constants (fixed small state space); 93
distinct states, exhaustive.

### specs/Pledge.tla — the 13-state Pledge plus folded proposal stage

The §9.3–9.5/§14.8 lifecycle exactly as committed in `pledge.json`,
including `disputed`, with the folded proposal stage (G20) and the
one-successor amendment CAS (G22). Finalization determinism ("authors no
missing seat", R9) is a guard over an explicit seat-receipt set, not a
promise.

| Model invariant | Design | Descriptor rows |
|---|---|---|
| `FinalizedHasAllSeats` | §9.3/§14.6 every required slot separately attributable; R9 authors no seat | `pledge_position`, `pledge_finalize` |
| `AtMostOneSuccessor` | §14.8 one current successor only (G22) | `pledge_amend` |
| `SupersededIffSuccessor` | §14.8 acceptance atomically supersedes | `pledge_amend` |
| `TerminalIsFinal` | §14.8 closed-machine rule: terminal states are final | all terminal fan-out rows |
| `ResumesBounded` | §14.8 each resume is a new Activity generation (finite check) | `pledge_resume` |

**Projection**: one Pledge, its required seat set, one amendment CAS slot;
seat Position payloads, terms digests, budgets, Activity generations, and
Delivery/Review bodies abstracted; every operation replayable at any time
(guards make exact retries no-ops); daemon state durable, crash =
stuttering, so crash honesty here is exactly the closed-transition property.
**Refinement boundary**: `pledge.json` (exact, machine-checked); the
cascade rows (`activity_open`, `delivery_submit`, `review_record`) are
modeled as this machine's view of those transactions — the owning machines
are EpisodeLease's ActivityStream and the unmodeled delivery-review
descriptor; no code yet. **Fairness**: none; safety only. **Coverage**:
`Seats = {pledgor, terms}`, `MaxResumes = 2`; 40 distinct states,
exhaustive.

### specs/EpisodeLease.tla — activation pipeline and the lease CAS

Folds three committed descriptors — `activity-stream.json`,
`wake-intent.json` (WakeIntent/ActivationAdmission/ResourceAllocation with
the named kernel transitions `activation_admit` and `resource_allocate`),
and `episode.json` (Episode + folded EpisodeLeaseHead, G29) — because their
safety story is joint: nothing runs without the whole §11.1 pipeline, and
nothing completes except under the current lease fence (§11.2).

| Model invariant | Design | Descriptor rows |
|---|---|---|
| `PipelineNoSkip` | §11.1 no skippable stage: intent → admission → allocation → bridged before queue/run | `wake_intent_submit`, `activation_admit`, `resource_allocate` |
| `FencePerAttempt` | §11.2 every claim mints one fresh fence and one immutable attempt | `episode_claim` rows |
| `FenceUnique` | §11.2 UNIQUE(episode_id, generation) | `episode_claim` rows |
| `HolderIsCurrent` | §11.2 a live lease head is held under the current fence | lease rows |
| `CompletionUnderCurrentFence` | §11.2/§14.8 stale-claim rejection: a superseded worker's completion never lands | `episode_complete` |
| `AmbiguousNeverCompleted` | §14.8 unknown external use is ambiguous, never reported complete | `server_time` row |
| `RunningHasRunningLease` | §11.2 the Episode runs only under a running head | `episode_start` |

**Projection**: one ActivityStream, one WakeIntent revision, one admission,
one allocation, one Episode, one lease head, `Workers` competing for it;
worker crash/silence needs no explicit action because an expired head is
re-claimable at any time (`ReClaim` — the crash story for workers); daemon
state durable, byomd crash = stuttering. **Refinement boundary**: the three
descriptors above (exact, machine-checked); Kovee's placement/attention side
is out of scope (only its committed outcomes — bridge, deny, unknown —
appear); no code yet. **Fairness**: none; safety only. **Coverage**:
`Workers = {w1, w2}`, `MaxFence = 3`; 33,488 distinct states, exhaustive.

### specs/ActIntentPermit.tla — one-shot execution permit

The §13.1/§14.8 ActIntent machine exactly as committed in `act-intent.json`:
server-prepared intent, seat positions, deterministic finalization, one-shot
`execution_permit_consume`, host attempt, source-qualified outcome admission
with ambiguous reconciliation. Replay (`ReplayConsume`) and conflicting-key
consumption (`ConflictConsume`) are explicit no-effect actions, so the
one-shot invariants are checked against live replay/conflict traffic, not by
its absence.

| Model invariant | Design | Descriptor rows |
|---|---|---|
| `OneShotConsumption` | §13.1 steps 4–6: consume at most once, exactly one immutable receipt | `execution_permit_consume` |
| `ConsumeRequiresAuthorization` | §13.1 no consumption without the finalized full seat snapshot | `act_intent_finalize`, `execution_permit_consume` |
| `DecisionFencesConsumption` | §14.8 denied/expired/canceled never consumed and never will | deny/expiry/cancel fan-out rows |
| `FinalizeAuthorsNoSeat` | R22/R23 deterministic finalization | `act_intent_position`/`act_intent_finalize` |
| `SpentBindsKey` | §13.1 the consumption key binds the one spent decision | `execution_permit_consume` |

**Projection**: one ActIntent, its required seat set, a small set of
idempotency keys competing for the one permit; PreparationTrace, subject
digests, GovernanceDecision payloads, fences, and receipts abstracted (a
receipt is a counter, so replay provably mints nothing); daemon state
durable, crash = stuttering ("none or recoverable intent", §14.8).
**Refinement boundary**: `act-intent.json` (exact, machine-checked); the
EffectOutcomeAdmission/EffectGovernanceDisposition head machinery of §14.8
is folded into the `succeeded/failed/ambiguous` outcome states — its
separate-head CAS story is not modeled in B0.1; no code yet. **Fairness**:
none; safety only. **Coverage**: `Seats = {participant, human}`,
`Keys = {k1, k2}`; 61 distinct states, exhaustive.

### specs/MandateChain.tla — never-widening derivation

The §10.1–10.2/§14.8 Mandate machine exactly as committed in
`mandate.json`, over the bounded chain root → c1 → c2. A widening
derivation attempt is modeled explicitly (`AttemptWiden`) and must fail
`authority_widening` (G33) as a rejected no-op, so never-widening is checked
against live widening attempts, not by their absence.

| Model invariant / property | Design | Descriptor rows |
|---|---|---|
| `NeverWiden` | §10.2 a child is a mechanical subset of every parent | `mandate_derive` |
| `RootClosure` | §10.2 transitive: the grandchild never exceeds the root | `mandate_derive` |
| `NonDelegableNeverDerived` | §10.1 human non-delegable powers never appear in a child | `mandate_derive` |
| `UseCap`, `ExhaustedIsSpent` | §14.8 use ordinal slots; last-slot consumption is the exhaustion row | `execution_permit_consume` cascade |
| `UsesMonotonic` (action) | §14.8 revocation cannot un-send prior effects | `mandate_revoke` |
| `NoUseUnderInactiveChain` (action) | §14.8 hold/revocation/exhaustion/expiry of any ancestor fences every descendant's new uses | `mandate_hold`/`mandate_revoke`/`server_time` |

**Projection**: authority as a finite capability set (BPA-1 subjects,
resources, data classes, destinations folded into abstract capabilities);
budgets live in BudgetConservation; `MaxUses = 1` so the last-slot
consumption IS the exhaustion row; replays disabled by guards; daemon state
durable, crash = stuttering. **Refinement boundary**: `mandate.json`
(exact, machine-checked); standing mandates are B0.2 (amendment A7) and are
not modeled; no code yet. **Fairness**: none; safety only. **Coverage**:
`Caps = {read, act, human_power}`, `NonDelegable = {human_power}`, chain
depth 3, `MaxUses = 1`; 7,988 distinct states, exhaustive. The subset
lattice is complete at this size; the invariants quantify over all chains of
depth ≤ 3 — deeper chains are future Apalache induction work (see
Remaining).

### specs/BudgetConservation.tla — the conservation identity

§11.4: `ceiling = remaining + reserved + committed + uncertain +
delegated_to_children`, held under reserve, commit, measured settlement,
release, ambiguous (uncertain) marking, conservative resolution, child
delegation, and unused-delegation return.

| Model invariant / property | Design |
|---|---|
| `Conservation` | §11.4 the identity, per account, in every reachable state |
| `DelegatedMatchesChild` | §11.4 delegated quantity exists exactly once |
| `ChildDelegatesNothing` | model depth bound (declared, checked) |
| `GrandConservation` | §11.4 settlement cannot spend in two places; Kovee bridging cannot double-charge |
| `ReleasedMonotonic` (action) | §11.4 released_lifetime is a monotonic audit counter, never a bucket |
| `CommittedMonotonic` (action) | settled spend is never silently un-spent |

**Projection**: one dimension, one parent account, one delegated child;
small-natural quantities; BudgetReservationSet, ExternalBudgetBridge, and
UsageSettlement records folded into the bucket moves they cause (the Kovee
bridge's deny/unknown outcomes are the Release/MarkUncertain moves); daemon
state durable, crash = stuttering. **Refinement boundary**: none at the
descriptor layer — BudgetAccount is the §11.4 ledger, not a §14.8 transition
machine, and B0.1 commits no budget descriptor (`@parity none`, declared to
the parity checker; the BudgetReservationSet/ExternalBudgetBridge state
enums land with the runtime slice); no code yet. **Fairness**: none; safety
only. **Coverage**: `Cap = 3`, `MaxReleased = 4`, amounts 1..2; 600 distinct
states, exhaustive. The identity is bound-sensitive by nature; lifting it
beyond fixed `Cap` is the flagged Apalache induction candidate (ADR-0003).

### specs/AuthorityJournal.tla — the §15.3 mutation protocol vs rollback

The three-step authority mutation protocol (SQL prepare → external witness
CAS → SQL finalize) with two competing transactions, crashes at every commit
boundary, lost witness receipts, and a database snapshot/restore adversary
(the §15.3 rollback threat). The external journal is append-only: the
adversary restores the database, never the witness.

| Model invariant / property | Design | Descriptor rows |
|---|---|---|
| `NoVisibleWithoutEntry` | §15.3 step 3: no visible mutation without its witnessed entry | `journal_sql_finalize` |
| `VisibleIsFinalized` | §15.3 visibility is exactly finalization | `journal_sql_finalize` |
| `MirrorNeverAhead` | §15.3 the local mirror never outruns the external journal | `journal_sql_finalize` |
| `AbandonedHasNoEntry` | §15.3 abandonment only after proving no entry | `journal_abandon` |
| `WitnessedHasEntry` | §15.3 witnessed/finalized state has its exact entry | `journal_witness_cas` |
| `EntryUnique` | §15.3 the witness dedups by transaction id | `journal_witness_cas` |
| `ActiveHasNoMismatch` | §15.3 startup comparison: a rolled-back database reopens only sealed_diagnostic | startup/restore semantics |
| `SealedNoNewAuthority` (action) | §15.3 a sealed endpoint mints no entry and widens no visibility | endpoint incarnation machine |
| `ExternalMonotonic` (action) | §15.3 the external journal is non-rollbackable | witness facility |

**Projection**: two transactions, one endpoint incarnation, the external
journal as a generation counter plus entry log, one snapshot + one restore;
digests folded into generation numbers plus transaction identity; the daemon
crashes at every boundary (`Crash` interleaves anywhere; startup runs the
§15.3 comparison). **Refinement boundary**: `authority-journal.json` — the
§14.8 "Authority mutation journal" row, committed as a descriptor with this
suite (the named internal transitions `journal_sql_prepare`,
`journal_witness_cas`, `journal_abandon`, `journal_sql_finalize`; exact,
machine-checked); the endpoint-incarnation machine itself
(active/sealed_diagnostic/retired, restore lineage) is folded to the sealed
flag; no code yet. **Fairness**: none; safety only (in particular, no claim
that a witness_unknown transaction is eventually resolved). **Coverage**:
`T = {t1, t2}`, `MaxGen = 2`; 2,902 distinct states, exhaustive.

### specs/GreenfieldEnablement.tla — the D10 greenfield enablement saga (C2 slice 1)

Per governed scope: create `KoveeRealmByomBinding` + `KoveeSocietyMapping`
durably (inert), then CAS `KoveeGovernanceOwnerBinding` `none → byom` at the
expected revision (byom §16.6 item 1; kovee amendment A2; the frozen
`governance_enable` authority row, family contract §2.A), exactly as
committed in `greenfield-enablement.json` and specified in
`../spec/governed-work/greenfield-saga.md`.

| Model invariant | Design | Descriptor rows |
|---|---|---|
| `NoOverlappingActiveOwners`, `NoOverlappingEnablementSlots` | §16.6 no overlapping active owner selectors — no scope under two owners | `governance_enable` / `owner_cas_none_to_byom` guards (rejection is the absence of a row) |
| `RetryIdempotent` | frozen row: retry returns the identical binding; exact-CAS at expected revision | the two `governance_enable` self-rows |
| `NoActivationAfterRollback`, `ActiveEpochNeverRolledBack` | D10: rollback-before-activation; re-enable only under a new binding epoch | `governance_enable_rollback`, `rolled_back → bindings_created` |
| `SageNeverExercised` | amendment A1: the sage arm is spec fidelity, never exercised | (no row sets it) |
| `OwnerMatchesPhase` | the owner arm flips exactly at the CAS and survives the freeze | `owner_cas_none_to_byom`, `governance_disable` |

**Projection**: saga phase, binding epoch, and owner arm per scope; two
scopes with one overlapping pair; record bytes, subject digests, principal
identity, and step-up assurance abstracted (the frozen row fixes them — a
CAS proves concurrency, not authority). Restore honesty: durable state plus
stuttering crashes; the rewind of durable state by a store restore is out
of model scope — the saga doc §5 requires query-first resolution there, and
that behavior is not machine-checked yet. **Refinement boundary**:
`greenfield-enablement.json` (exact, machine-checked); the paired
`endeavor-formation.json` machine is descriptor + executable-walk covered
only (no C2 slice 1 model — its recovery table is exercised by the six
`../spec/vectors/governed-work/` walks). **Fairness**: none; safety only.
**Non-vacuity probes** (run ad hoc, not yet in a harness): allowing
`Activate` from `rolled_back` violates `NoActivationAfterRollback`;
dropping `EnableCreate`'s overlap guard violates
`NoOverlappingEnablementSlots`. **Coverage**: 65 distinct states,
exhaustive at `{s1, s2}`, one overlap pair, `MaxEpoch = 2`.

### check-descriptors.py — descriptor ↔ model parity, as CI

Every module carries a machine-readable `@parity` block after its module
terminator: the descriptor file(s) it models, its states, and its
`from -> to via op` transitions. `python3 proof/check-descriptors.py` (wired
into `../run-checks.sh`) compares those blocks against
`../spec/descriptors/*.json` with exact set equality in both directions — a
descriptor row with no model transition, or vice versa, fails (ADR-0003).
Current binding: 7 modules, 9 descriptors, 92 states, 177 transitions in
exact agreement; BudgetConservation declares `@parity none` (no committed
budget descriptor); 12 descriptors have no B0.1 model (their machines are
listed in §14.8 but are not on the B0.1 sheet's model list).

> **What parity does and does not catch (be honest about it).** The
> `@parity` block is a transcription of the model's transition relation,
> exactly akson's hand-transcribed-oracle situation (plan §3): the checker
> catches descriptor drift and annotation drift, and its quoted-literal
> check catches a state renamed only in the model, but editing a TLA+
> *action* while leaving its annotation and the descriptor untouched is
> model-only drift it cannot see. TLC still checks the invariants of the
> edited model, and the transitions themselves are exercised as executable
> walks by the machine vectors below, which shrinks — but does not close —
> that window. Closing it means generating the annotation (or the TLA+
> relation) from the descriptor registry; that lands with the byomd
> conformance oracle (ADR-0003 requires at least one generated table there).

### spec/vectors/machines/ — crash/replay state walks, as executable vectors

Fourteen state-walk vectors (4 Pledge, 3 Episode/lease, 4 ActIntent, 3
AuthorityJournal) under `../spec/vectors/machines/`, validated by
`python3 conformance/run.py` through a small interpreter over the committed
descriptor JSON — the §14.8 closed-machine rule as an executable oracle,
independent of both TLC and the parity checker. The conventions: walks start
at `absent`; `accepted` steps must be exact descriptor rows; `rejected`
steps must be absent rows (an unlisted transition is invalid); `replay`
steps retry the immediately preceding accepted mutation and must be
state-idempotent (the "exact retry returns the retained receipt" claim at
descriptor level); `{"crash": true}` markers restart the daemon between
steps — every descriptor-level variable is durable, so the walk resumes
unchanged, which is each machine's committed §14.8 crash outcome. The walks
cover, among others: one-shot permit consumption under replay, the
expired-lease re-claim (a real transition minting a fresh fence — not a
replay), ambiguous-never-completed, terminal-is-final for every modeled
terminal, and the witness-unknown query/abandon-after-proof recovery paths.

## Remaining

1. **Negative checks.** Akson's `negative-checks.sh` (mutations that must
   produce counterexamples, probes that must be refuted) has no byom
   counterpart yet; until it lands, harness non-vacuity rests on the
   documented TLC state counts and the parity mutation behavior. ADR-0003
   requires it before B0.1 moves to `accepted`.
2. **Induction.** All seven models are TLC-bounded (exhaustive at their
   configured constants). BudgetConservation (arbitrary `Cap`) and
   MandateChain (arbitrary chain depth) are the flagged Apalache candidates.
3. **The conformance oracle.** Lands with byomd (B1): pure transition
   functions tied to these models in `cargo test`, generated from the
   descriptor registry where feasible, hand-transcribed remainders flagged
   here per machine (ADR-0003, plan §3).
4. **Unfolded machinery.** The EffectOutcomeAdmission/
   EffectGovernanceDisposition dual-head CAS, the endpoint-incarnation
   restore lineage, standing mandates (B0.2), and the Assembly machines are
   listed in §14.8 but not modeled in B0.1.

## What model checking does not cover

Crypto soundness (BPA-1 signatures, digests, and witnesses are modeled as
perfect), byte-level envelope acceptance (covered by `../family-vectors/`
and the conformance runner's acceptance vectors), BDPL parsing, Kovee's
attention/placement internals (only their committed outcomes appear), and
timing/side channels.
