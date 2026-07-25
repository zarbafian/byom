# The greenfield enablement saga (D10)

Status: **normative** for `byom_governed_work_v1` (C2 slice 1). Sources, all
pinned: byom DESIGN.md §16.6, kovee amendment A2
(`kovee/design/2026-07-25-amendment-governance-owner.md`), and the **frozen
`governance_enable` authority row** of the family contract §2.A
(`design/2026-07-25-family-contract.md`) — that row is reproduced by
reference here, not restated, so it cannot fork. Kovee owns the host
schemas; the record shapes in this directory are the byom-normative side.

What one successful enablement looks like, end to end:

~~~text
$ kovee governance enable --byom local --society soc-1     # amendment A5 verb

step 1  create KoveeRealmByomBinding{krbb-1, epoch 1} +          durable,
        KoveeSocietyMapping{soc-1, revision 2}                   NOT authoritative
step 2  CAS KoveeGovernanceOwnerBinding(realm-1, scope-digest):  atomic with
        governance_owner none -> byom AT expected revision 2     activation
result  binding krbb-1 epoch 1 active; retry of the same
        command returns THIS binding, byte-identical
~~~

The machine, exactly as committed in
`../descriptors/greenfield-enablement.json` and model-checked in
`../../proof/specs/GreenfieldEnablement.tla`:

~~~text
absent ──governance_enable──▶ bindings_created ──owner_cas_none_to_byom──▶ active
             ▲                     │        │                                │
             │(new epoch)          │(retry: │(governance_enable_rollback)    │(retry: same
             │                     │ same   ▼                                │ binding)
             └────────────── rolled_back  (pre-CAS only)      governance_disable
                                   pending bindings)                         ▼
                                                                         disabled
~~~

## 1. Preconditions — Kovee is never the genesis governance actor

A Society already exists, established through native
`society_prepare`/`society_bootstrap` under the bootstrap human's direct
governance channel (registry row R2). Kovee may start/configure/bind `byomd`
and supply inert context only (amendment A2). The saga requires, per the
frozen row's authorization dependency set: the realm revision, the target
`society_ref` + Society recovery epoch, the byomd endpoint
identity/incarnation, an expected absent-or-identical `KoveeRealmByomBinding`,
and the `KoveeSocietyMapping` revision. The allowed actor is a **human
realm-owner principal only** — never a service identity, session, assistant,
or connector; personal-mode bootstrap is owner-only over the UID-checked
local socket, team mode requires the realm `owner` role with fresh step-up.

## 2. Step 1 — create the bindings, durably and inertly

`governance_enable` durably creates the `KoveeRealmByomBinding` and
`KoveeSocietyMapping` rows (shapes in this directory). They are **not yet
authoritative**: the `KoveeGovernanceOwnerBinding` row for the exact scope
still carries `governance_owner: none`, so no derived channel, credential,
or permit may be issued from them. Creation is idempotent over
(realm, exact scope digest, binding epoch): an exact retry returns the
identical pending bindings and never creates a second row.

**Overlap rejection.** If any overlapping scope selector currently holds an
active owner binding — or a pending enablement past step 1 — the enable is
rejected before creating anything (§16.6 item 1: no overlapping active owner
selectors). Rejection is the *absence* of a transition: the machine is
closed, and an unlisted transition is invalid.

## 3. Step 2 — the CAS, `none → byom`, exact and atomic

The same invocation then CASes the scope's `KoveeGovernanceOwnerBinding`
from `none` to `byom` **at the expected revision** under the expected
binding epoch, atomically with marking binding and mapping active and
setting `owner_endpoint_ref`/`owner_binding_ref`. Exact-CAS means: a changed
revision, epoch, or subject digest conflicts and commits nothing — **a CAS
proves concurrency, not authority** (the frozen row's actor and assurance
requirements are checked independently of it). The CAS re-checks the overlap
rule; two overlapping sagas cannot both win (model invariants
`NoOverlappingActiveOwners`, `NoOverlappingEnablementSlots`).

**Retry after activation** returns the stored identical binding — never a
second creation, CAS, or epoch advance (`RetryIdempotent`: per scope and
epoch, step 1 executes at most once and the CAS wins at most once).

## 4. Rollback — strictly before activation

`governance_enable_rollback` exists only from `bindings_created`: it voids
the pending bindings, leaves the owner at `none`, and **spends the binding
epoch**. A rolled-back epoch can never activate; re-enablement is a fresh
`governance_enable` under a **new** binding epoch
(`NoActivationAfterRollback`, `ActiveEpochNeverRolledBack`). After the CAS
there is no rollback — only `governance_disable` (always step-up), which
freezes the owner row (`status: active → frozen`, owner arm retained for
audit) and invalidates derived channels; re-enablement after a disable is a
fresh saga row under a new epoch, not a transition of this machine.

## 5. Restore behavior

- **Kovee store restore.** The saga variables are durable; a restarted or
  restored Kovee re-enters at the recorded state. If the recorded state is
  `bindings_created` and the CAS outcome is unknown (crash between step 2's
  send and its durable record), the operator resolves by **query first** —
  the recovery-only service authority of the frozen row
  (`external_command_result_query` pattern): a service may *query* saga
  state, never create or activate a binding. Only a verified answer drives
  retry or rollback; guessing is not a transition.
- **byomd restore.** A byomd restore advances the Society recovery epoch and
  endpoint incarnation; the binding's `dependency_digest` no longer matches,
  which invalidates every derived channel and permit (family contract L2)
  and blocks new issuance until the human realm-owner re-verifies via
  `governance_show`. The owner CAS outcome itself is never silently redone
  or undone by a restore: an active binding stays active at its recorded
  epoch, a rolled-back epoch stays spent.

This saga is a **different machine** from byom §25's `GovernanceCutover`
(`sage → none → byom`): the `sage` arm of the owner enum exists for spec
fidelity and is never exercised in this stack (amendment A1; model invariant
`SageNeverExercised`).

## 6. Conformance and proof pointers

| Artifact | Where |
|---|---|
| Record schemas (this slice, closed, §16 verbatim) | `*.schema.json` in this directory |
| Saga descriptor (Kovee-owned, `owner: "kovee (C2)"`) | `../descriptors/greenfield-enablement.json` |
| Formation descriptor (paired intent/slot machine) | `../descriptors/endeavor-formation.json` |
| State-walk vectors (retry-identical, rollback/new-epoch, formation walks) | `../vectors/governed-work/` |
| TLA+ model + TLC invariants | `../../proof/specs/GreenfieldEnablement.tla` (parity-bound to the descriptor) |
| Runner checks (schemas, verbatim enums, descriptor ownership, hop-count cross-check) | `../../conformance/run.py` (governed-work family) |

## 7. Recorded gaps in DESIGN.md §16 (C2 slice 1)

Found while transcribing §16 field lists verbatim; each is a byom design
obligation, tracked here until §16 is amended:

1. **`DelegatedPrincipalCredential` has no field list in §16.** §16.3 names
   "a short-lived delegated-principal credential" and §16.6 item 8 requires
   sender-constrained credential profiles, but the record's fields appear
   nowhere in §16. The committed profile
   (`delegated-principal-credential.schema.json`) transcribes §14.4's two
   normative sentences (sender-constrained credential contents; Kovee
   gateway delegation contents) plus the family contract L5–L6 atomic
   (issuer, nonce) rule.
2. **`status` on `KoveeRealmByomBinding` and `KoveeSocietyMapping` is
   untyped in §16.6** (the owner binding's `status: active | frozen` is the
   only typed one). The schemas pin presence and the saga semantics; the
   value sets remain Kovee-owned to close in the C2 host schemas.
3. **`governance_disable` semantics are named but not specified** in the
   frozen authority row ("with step-up") or §16. This document fixes them
   byom-side: freeze (`active → frozen`), owner arm retained, derived
   channels invalidated, re-enablement only as a fresh saga row.
4. **The embedded `endeavor_proposal` and `source_principal_position`
   bodies of `KoveeEndeavorFormCommand` have no §16 shape**; they are
   carried opaque, digest-pinned, with their shapes normatively owned by the
   B0.1 `endeavor_propose`/`endeavor_position` subjects.
5. **The negotiated RestoreLineageProof hop-count limit has no number in
   §16.3**; this bundle pins 64. The hop-count/array-length equality is not
   expressible in JSON Schema and is enforced by the conformance runner.
