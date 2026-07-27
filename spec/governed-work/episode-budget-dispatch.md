# Governed work: episode, budget-bridge, dispatch, and onboarding contracts (C2 slice 2)

Status: **normative** for `byom_governed_work_v1` (C2 slice 2 — the bundle's
episode/effect/driver contracts; with slice 1 this completes the C2 record
inventory). Sources, all pinned at the v0.2 design SHA: byom DESIGN.md
§7.4, §11.4, §12.1, §13.1/§13.2, §14.3, §16.6 (items 3–5, 8, 11–12), §17.2;
the family contract (`design/2026-07-25-family-contract.md`) rows L19–L37,
L61–L64 and deltas Δ4/Δ5; byom amendment A4. Kovee owns every host schema
and the `byom_akson_dispatch_v1` driver + `ByomAksonDispatchOutcomeReceipt`
head (family contract §1); the shapes in this directory are the
byom-normative side.

What one governed hosted turn looks like, end to end. The call order below is
the ACHIEVABLE one, and it is not negotiable: `placement_admit` binds the
exact `ResourceAllocation` — including the portable allocation digest — so it
can only run AFTER the `episode_request` that creates that allocation. Kovee
authors its `PlacementBinding` in between, from the allocation the
`episode_request` reply published. Placement never comes first (seam findings
S-1/S-3).

~~~text
episode_request (R29)       ONE participant call drives stages 1-3:
                            wake_intent_submit -> activation_admit [kernel]
                            -> resource_allocate [kernel]. It reserves every
                            byom dimension and persists the byom_subordinate
                            bridge in `requested`, and it ANSWERS with the
                            Episode `eligible` (NOT queued) plus
                            `resource_allocation_id` and its portable_public
                            `resource_allocation_digest`           L31-L32
Kovee authors PlacementBinding  among already-eligible Manifestations, pinning
                            the allocation digest the reply carried — the ONE
                            activation record Kovee owns (§11.1 stage 4)
placement_admit (R33)       the narrow Kovee placement adapter presents that
                            digest byte-for-byte and the byom_subordinate saga
                            outcome: reserve -> confirmed (never above the
                            parent worst case) completes both reservation sets
                            and QUEUES the Episode                 L31-L33
episode_claim/start (R30)   Kovee commits ByomEpisodeBinding{ep-88, attempt-2,
                            BOTH fences, context refs, budget/bridge refs,
                            allowed_local_commitments}, idempotent   L19-L22
model call                  through Kovee's ProviderContextManifest carrying
                            the exact byom source fields (Δ5)        §12.1
outbound act                act_intent_* (Δ4 `outbound` subject, BPA-1 atoms)
                            -> execution_permit_consume (R34)
                            -> KOVEE's byom_akson_dispatch_v1 driver stages,
                               consumes akson consent, dispatches    L61-L63
outcome                     one signed ByomAksonDispatchOutcomeReceipt
                            (closed union) + one Kovee head CAS; byom admits
                            the EOA only from that verified receipt  L64
settle                      subordinate settle applied once on both sides;
                            uncertain never releases without R38     L33
~~~

## 1. `ByomEpisodeBinding` — one row per episode/attempt/invocation

`byom-episode-binding.schema.json` carries the §16.6 block **field-verbatim**
(machine-diffed, 23 fields: endpoint/society/participant closure, episode +
attempt refs, the **dual** fence epochs `byom_fence_epoch` +
`kovee_invocation_fence` (L21 — a mutation carrying one fence is the
committed negative vector), `mandate_use_refs[]`, `context_source_digest`,
and the budget reservation/bridge/subordinate refs) plus the four
family-contract groups the §16.6 block omits (gap notes 1–2 below):
`stable_binding_key` (L22 idempotent create at the claim CAS),
`allowed_local_commitments` (L34–L37), and the Δ5 context refs — the byom
`ContextManifest` pair required, the Kovee `ContextAssembly` and
`ProviderContextManifest` pairs optional all-or-none.

Lifecycle (`../descriptors/byom-episode-binding.json`, Kovee-owned):
`bound` → `fenced` (either fence advances; terminal; successor attempts get
a NEW row) or `bound` → `released` (episode terminal, hand-off to
settlement). The Episode/EpisodeLease machine itself stays owned by
`../descriptors/episode.json` — this machine binds, it never claims.

## 2. `ByomSubordinateReservation` — the budget-bridge saga

`byom-subordinate-reservation.schema.json`: the exact subordinate Kovee
reservation of §11.4/§16.6 item 4, `reservation_class: byom_subordinate`,
idempotent over `stable_external_reservation_key`, every item pinned to its
exact parent §11.4 reservation item. **Never above parent**:
`amount <= parent_worst_case_amount` with identical dimension and unit —
JSON Schema cannot compare two members, so the conformance runner enforces
it as a cross-member check (the restore-lineage-proof pattern; the
above-parent and dimension-mismatch vectors are schema-shape-valid and fail
only there). State is the §11.4 `ExternalBudgetBridge.state` list verbatim:
`requested | confirmed | denied | uncertain | settled | released`.

The saga machine (`../descriptors/subordinate-reservation.json`) is
model-checked in `../../proof/specs/SubordinateReservation.tla`
(NeverAboveParent, ChargeWithinReservation, CreateOnce, SettleOnce,
ResolutionIsReal — the recovery query surfaces Kovee's durable truth, never
invents one — HeldIffOpen, UncertainReleaseNeedsGovernance): an unknown
result remains `uncertain`, spend stays blocked, and the only release out of
`uncertain` is the R38 `budget_reconcile` governance seat.

## 3. `byom_akson_dispatch_v1` — Kovee sole caller, one receipt union, one head

- `byom-akson-dispatch-arguments.schema.json` — what the driver requires
  before dispatch (§17.2 prose, derived; gap note 6): the current delegation
  act, the consumed byom permit (`ExecutionConsumptionReceipt`), the inert
  idempotently staged contract, the exact Akson consent, the current Kovee
  effect fence, and the stable execution/delivery keys.
  `caller: kovee` and `driver_profile: byom_akson_dispatch_v1` are consts:
  byom's delegation engine *authorizes* and never calls (L63); no
  pre-existing Akson driver authority becomes authority here by relabeling
  (§16.6 item 11).
- `byom-akson-dispatch-outcome-receipt.schema.json` — the §17.2 field list
  verbatim (machine-diffed, 47 fields) with the closed union
  `pre_result_failed | ambiguous | verification_rejected | verified_result`
  encoded as oneOf arms per §17.2's disposition table, including the
  genesis/successor split (revision 1 has no predecessor; a successor
  carries `previous_receipt_digest` AND the `reconciles_host_receipt_*`
  pair) and the acceptance-triple all-or-none groups. One committed valid
  vector per arm; mixed-arm and successor-without-reconciles negatives.
- `byom-akson-dispatch-outcome-receipt-head.schema.json` +
  `../descriptors/byom-akson-dispatch-outcome-head.json` — the one
  Kovee-owned CAS head per Kovee Effect: ambiguous genesis → `ambiguous`,
  anything else → `final`; the only transition out of `ambiguous` is
  Kovee's own final successor, committed with the head CAS **before** byom
  admits it; a final head is terminal. The
  `dispatch-ambiguous-then-disposition` walk pins that an
  `effect_reconcile` disposition never closes this head.

## 4. Onboarding compute — one shot, never assent

`onboarding-compute-intent.schema.json` and
`onboarding-compute-receipt.schema.json` transcribe §7.4 **field-verbatim**
(machine-diffed): output-only operations,
`tools_network_workspace_children: none`, `max_uses: 1`, candidate fence
pinned. The `onboarding-compute-one-shot` walk runs the committed
OnboardingActivationOffer machine: one consume activates, the exact retry
replays the stored receipt, a **second consume is not a transition**,
completion stays evidence, and the candidate's refusal remains available —
runtime output is never membership assent (§16.6 item 12).

## 5. Sender-constrained credential profiles

`sender-constrained-worker-credential.schema.json` (runtime surface, R30
operation family, bound to the exact Episode/attempt/fence and its
ByomEpisodeBinding) and `sender-constrained-candidate-credential.schema.json`
(candidate surface, exactly one offer, the three candidate operations,
amendment A4 lifecycle). Both are **derived** profiles with the slice-1 DPC
honesty: §16.6 items 8/12 require them but carry no field lists; the
members transcribe §14.3's two normative sentences (see gap note 7 on the
§14.3/§14.4 citation).

## 6. The Δ4 act-class subject taxonomy

`act-class-subject.schema.json`: the closed class list
`model_egress | share | outbound | apply | budget` (family contract Δ4,
verbatim), each subject an atoms object that IS the BPA-1 request-atoms
wire — the schema's `$defs` are byte-identical copies of
`../schemas/bpa1-policy.schema.json`'s (runner-checked) — with per-class
mandatory domains pinned:

| Class | Mandatory domains |
|---|---|
| `model_egress` | operation, purpose, binding, classification, quantity |
| `share` | operation, purpose, object, classification |
| `outbound` | operation, purpose, network_destination, classification |
| `apply` | operation, purpose, object, path, schema_evidence |
| `budget` | operation, purpose, quantity |

Extra domains only narrow further and are always allowed; BPA-1's deny
rules conservatively match absent domains, so omission fails closed. The
runner cross-validates twice: statically (arms equal the transcription
above; copied $defs byte-identical) and dynamically (every committed
subject replays through `policy/eval.py` `decide()` under a universal
allow policy — a schema-valid subject the evaluator rejects is a hard
divergence failure, and the money-without-currency negative shows both
encodings rejecting together).

## 7. Conformance and proof pointers

| Artifact | Where |
|---|---|
| Record schemas (this slice, closed, machine-diffed against §7.4/§16.6/§17.2) | `*.schema.json` in this directory |
| Descriptors (Kovee-owned, `owner: "kovee (C2)"`) | `../descriptors/byom-episode-binding.json`, `../descriptors/subordinate-reservation.json`, `../descriptors/byom-akson-dispatch-outcome-head.json` |
| Schema + walk vectors (per-arm receipt vectors, dual-fence/above-parent negatives, subordinate reserve→commit→settle, dispatch happy + ambiguous-then-disposition, onboarding one-shot) | `../vectors/governed-work/` |
| TLA+ model + TLC invariants (parity-bound to the subordinate descriptor) | `../../proof/specs/SubordinateReservation.tla` |
| Runner checks (schemas, verbatim enums, descriptor states/ownership, never-above-parent cross-member check, taxonomy↔BPA-1 cross-validation via `policy/eval.py`) | `../../conformance/run.py` (governed-work family) |

## 8. Recorded gaps in DESIGN.md (C2 slice 2)

Found while transcribing the slice-2 field lists verbatim; each is a byom
design obligation, tracked here until the design is amended (v0.3):

1. **The §16.6 `ByomEpisodeBinding` block omits fields the family contract
   requires on it.** L19–L20/L34–L37 put `allowed_local_commitments` and
   the Δ5 context refs on the binding, and L22 requires an idempotent
   turn↔invocation create; the §16.6 block has none of them. The committed
   schema carries the block verbatim PLUS these documented additions
   (machine-diffed: 23 verbatim + 8 added members, nothing else).
2. **Which Δ5 context refs the binding carries is not enumerated
   anywhere.** This slice fixes them byom-side: the byom ContextManifest
   pair required; the Kovee ContextAssembly and ProviderContextManifest
   pairs optional, all-or-none.
3. **The Kovee-side `byom_subordinate` reservation record has no field
   list** in §11.4 or §16.6 (only the saga verbs and the bridge's
   `subordinate_reservation_*` back-refs). The committed shape derives from
   §11.4's `ExternalBudgetBridge`/`BudgetReservationSet` members (names
   verbatim where they exist); the per-item never-above-parent rule is not
   expressible in JSON Schema and is enforced by the conformance runner.
4. **§12.1's ProviderContextManifest byom source fields are a prose list,
   not a field block**; member names here are derived (`byom_endpoint` →
   `byom_endpoint_ref` for consistency with the §16.6 binding block).
5. **`OnboardingComputeIntent.retention_and_training_claims` has no value
   shape in §7.4**; carried as an opaque ref, shape Kovee/design-owned.
6. **§7.4's `allowed_output_operations` names `refuse`** where the §14.6
   catalog and `OnboardingActivationOffer.allowed_operations` say
   `membership_refuse`. Transcribed verbatim (the runner pins the verbatim
   list); flagged for a naming amendment.
7. **The driver arguments have no §17.2 field block** (prose preconditions
   only); `byom-akson-dispatch-arguments.schema.json` derives them, and
   L63's sole-caller rule is pinned as consts.
8. **The worker and candidate sender-constrained credentials have no §16
   field lists**; both profiles transcribe §14.3's sentences. Additionally,
   the slice-1 DPC schema cites those sentences as **§14.4**, but in the
   pinned v0.2 text they sit in §14.3 (Authenticated actor; §14.4 is the
   event ledger). The slice-1 bytes are pinned and left untouched; the
   citation defect is recorded here for the v0.3 sweep.
9. **Ambiguous dispatch receipts are genesis-only by derivation, not by a
   literal §17.2 sentence**: every successor must name
   `reconciles_host_receipt_digest`, which the ambiguous arm forbids, so an
   ambiguity-stage change cannot be a successor. The schema encodes the
   derived closure (`receipt_revision: 1` on the ambiguous arm).
10. **The Δ4 per-class mandatory domains have no design-source table.**
    Δ4 says the taxonomy is "delivered in C2", so the §6 table above IS the
    normative definition (a C2 pinning, not a transcription); the runner
    carries the same table and fails on drift.
