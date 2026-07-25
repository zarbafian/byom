# The akson + kovee + byom family contract

Status: proposed v2 — v1 was reviewed by the C0 three-lens codex review
(`reviews/2026-07-25-c0-lens{1,2,3}-*.md` in the program repo); this revision
applies the dispositions (`reviews/2026-07-25-c0-dispositions.md`). Ratified
only when the blocker-only confirmation pass returns no P0/P1 and D8 sign-off
is recorded. **The C0 three-lens review is itself C0's ratification evidence;
R0 is a later gate (post-C1) for C2/C3a starts — there is no review cycle.**

Date: 2026-07-25 (v2)

Canonical here (byom); lock-manifest-vendored into kovee and akson (plan D3).

Pinned sources:

| Artifact | Pin |
|---|---|
| `kovee/DESIGN.md` v0.1 | sha256 `40820c476d59ebdd458955fd5939289b3ef2bff03c3d1266f5e80f3087935860` (repo `7aad4a6`) + `kovee/design/2026-07-25-amendment-governance-owner.md` |
| `byom/DESIGN.md` v0.2 | sha256 `ccea384ff931bcf45d30df680b86835ac682006072a07ef2f34f565eba5fa501` (repo `cc4249c`) + `byom/design/2026-07-25-amendment-family-contract.md` |
| `akson/` | repo `e5e80dc` (pre-release; ADR-0015 itself still `proposed`; consumed via the A0 checklist) |
| Implementation plan v3 | sha256 `ee5742e936bc23255ca34cc283718b350146faecc1ac48e05338e32c903e36ca` |

## 1. Ownership

| System | Owns | Never |
|---|---|---|
| **Kovee** | Spaces, contributions/relations, lenses, branches, attention, exact ContextAssembly, local commitments, assistant hosting, workers, artifacts, model/tool brokers, effect drivers, placement, **the `byom_akson_dispatch_v1` driver and the `ByomAksonDispatchOutcomeReceipt` head** | Decides governance; acts as genesis governance actor; manufactures authority from intelligence; crosses installations itself |
| **Byom** | Societies, charters, participants, assemblies, endeavors, calls, pledges, mandates, episodes-as-authority, budgets, engrams, decisions | Calls a model; plans; picks workers; executes effects; **calls Akson or holds Akson credentials** (its delegation engine *authorizes*; Kovee's driver *calls*) |
| **Akson** | Endpoint identity, introduction (ADR-0015), signed contracts, consent, bounded peer execution, evidence, carriage | Understands spaces or societies; grants a peer ambient access |

Three seams, dual/triple-owned as marked, consumers vendor via the lock
manifest: `byom_governed_work_v1` (byom-normative text, Kovee-owned host
schemas — C2); the worker/harness binding (C3); `akson_byom_exchange_v1`
(akson surface + byom payload shapes + **kovee driver/outcome head** — C4).
Plan decisions D1–D12 apply as written in plan v3.

## 2. The operation × authority matrix

Left side: the kovee requirements ledger (69 rows, ids L1–L69, extracted from
kovee §6.1/§9/§10/§11.6/§16.1/§17/§18/§26 at the pinned SHA). Right side: BPP
operations bound through §2.0's registry-binding table (exact §14.7 fields per
`(operation, surface)`), plus Kovee-owned contracts and named kernel
transitions.

**Row classes** (every row carries exactly one):
- **[op R#]** — callable BPP operation(s); R# cites the §2.0 registry binding
  carrying surface/actor/closure/fence/offline.
- **[kovee C#]** — a Kovee-owned record/transition; the cited contract (C2,
  C3a/b, C4, or a KCP registry row frozen in that bundle) is its normative
  home. Kovee-owned is a *mapping*, not a gap — the row must name its
  contract or it fails C0.
- **[kernel]** — a named non-callable byom kernel transition
  (`activation_admit`, `resource_allocate`, `procedure_seed_*`, journal
  protocol).
- Prose-only rows are defects and fail C0.

### 2.0 Registry bindings (byom §14.7, reproduced faithfully)

Closure legend: **E** endpoint/recovery/Society/Charter · **P**
principal/Participant binding/Standing/self-policy/control domain · **A**
assembly/decision snapshot · **O** exact object+revision · **M** complete
Mandate chain · **B** budgets/meters · **D**
visibility/classification/erasure/disclosure · **F** activity/episode/host
fences · **X** Kovee/Akson source bindings. Registry key is
`(operation, surface)`; unlisted pairings are forbidden (deny-by-absence).

| R# | Operations | Surface | Actor | Closure | Fence/assurance | Offline |
|---|---|---|---|---|---|---|
| R1 | hello, protocol_info, feature_info | each surface | bounded pre-auth client/actor | endpoint/version | parser/rate limits | no |
| R2 | society_prepare, society_bootstrap | governance | source-qualified human filling bootstrap sovereign seat | E,P,D,B | fresh phishing-resistant challenge; endpoint incarnation | no |
| R3 | society_hold/release/dissolve | governance | human filling exact current human-authority seat | E,P,O,A | fresh challenge; current revision | no |
| R4 | society_show, charter_history, participant_show, activity_show, budget_show, budget_reservation_show, usage_settlement_show, context_manifest_show, snapshot_get, events_read, events_wait, event_payload, recovery_checkpoint_show | projection | authorized principal/Participant/narrow projection service | E,P,O,D (+X when projected) | proof-of-possession; opaque cursors | no |
| R5 | charter_propose, participant_propose, control_domain_propose, procedure_propose, formation_start/revise, assembly_propose, endeavor_propose, call_open, pledge_propose, pledge_amend, dispute_raise, appeal_raise, engram_propose, classification_*_propose, erasure_request, collective_policy_propose | participant | current Participant as itself only | E,P,O,D (+A/M/B per subject) | binding epoch | stage |
| R6 | charter/control_domain/procedure/classification/erasure `_position` | governance | actor for exact prepared human/governance seat | E,P,A,O,D | exact subject; fresh challenge (human/high-risk) | stage |
| R7 | assembly/endeavor/pledge `_position` | participant | Participant for its exact eligible seat | E,P,A,O,M,D | exact subject; binding + assembly epoch | stage |
| R8 | charter_finalize, participant_admit, manifestation_admit, control_domain_finalize/merge, procedure_finalize/hold/release, classification_finalize/revoke, erasure_finalize | governance | governance caller requesting deterministic finalization; authors no missing seat | E,P,A,O,D (+B/M) | current snapshot; fresh challenge for reserved action | no |
| R9 | assembly/collective_decision/endeavor/pledge `_finalize` | participant | any current Participant requesting deterministic finalization; authors no seat | E,P,A,O,M,B,D | snapshot/epoch CAS | no |
| R10 | membership_offer, onboarding_offer, membership_offer_revoke, participant_suspend | governance | exact decided governance actor | E,P,A,O,B,D | current decision; revocation advances candidate fence | no |
| R11 | membership_refuse, membership_accept, candidate_self_policy_propose | candidate | candidate for the exact MembershipOffer only | endpoint, offer, candidate/Manifestation/control-domain binding, onboarding fence, B,D,X | sender-constrained candidate proof; exact subject | no |
| R12 | participation_cease, participant_retire, assembly_withdraw | participant | affected Participant only; not delegable | E,P,O,A | binding epoch | no |
| R13 | manifestation_propose/disable, assent/activation_policy_adopt/revoke, continuity_root_update | participant | owning Participant only | E,P,O,D | sender-constrained channel + binding epoch | stage (not disable/revoke) |
| R14 | assembly_hold/reform/dissolve, endeavor_hold/release/close, call_withdraw, pledge_resume/relinquish, delivery_withdraw, review_record | participant | exact Participant/collective authorized by governing subject | E,P,A,O,M,B,D | subject revision/epoch; fresh challenge per policy | no |
| R15 | mandate_prepare, mandate_derive, standing_mandate_prepare | participant | proposed grantee or authorized issuer in its scope | E,P,A,O,M,B,D | preparation trace; parent revisions | stage (prepare) |
| R16/17 | mandate_position, standing_mandate_position | participant / governance | exact prepared seat (participant/resource-owner; human-authority) | E,P,A,O,M,B,D | exact subject + seat; fresh challenge as required | stage |
| R18 | mandate_issue/hold/revoke, standing_mandate_issue/hold/revoke | governance | exact issuer/human-authority actor under decided rule | E,P,A,O,M,B,D | complete current chain; fresh challenge for root/standing issue | no |
| R19–21 | act_intent_prepare; act_intent_position (participant seat; human seat) | participant / governance | requester/collective in executive policy; exact prepared seat | E,P,A,O,M,B,D,F,X | field-complete PreparationTrace; exact intent digest; fresh challenge | stage |
| R22/23 | act_intent_finalize | participant / governance | deterministic finalizer; authors no seat | E,P,A,O,M,B,D,F,X | exact slot snapshot + revision CAS; fresh challenge (reserved) | no |
| R24/25 | act_intent_cancel | participant / governance | original requester (unconsumed) or exact cancellation authority | E,P,A,O,M,B,D,F,X | current revision; cannot claim effect rollback | no |
| R29 | activity_open/hold/close, wake_intent_submit/withdraw, episode_request, continuation_write, delivery_submit | participant | owning Participant/Manifestation or collective channel in executive policy | E,P,A,O,M,B,D,F | participant + generation fence; exact episode fence when cited | stage (activity proposal only) |
| R30 | episode_claim/start, checkpoint_commit, episode_yield/complete/fail, usage_report | runtime | workload identity bound to exact Episode/Manifestation | E,P,O,M,B,D,F,X | mTLS/attested workload; **Byom and host fences** | no |
| R31 | onboarding_episode_claim/complete | runtime | candidate workload bound to exact offer + proposed Manifestation | endpoint, offer, candidate binding, OnboardingComputeReceipt, B,D,F,X | mTLS/attested; one offer fence | no |
| R32 | onboarding_compute_permit_consume | runtime | Kovee model broker bound to exact OnboardingComputeIntent | endpoint, decision, candidate/Manifestation, final manifests, B,D,F,X | workload mTLS; one-shot key + onboarding fence | no |
| R33 | placement_admit | runtime | narrow Kovee placement adapter bound to exact ResourceAllocation | E,P,O,M,B,D,F,X | source binding; exact placement revision + fences | no |
| R34 | execution_permit_consume | runtime | trusted host effect service bound to exact prepared host Effect | E,P,A,O,M,B,D,F,X | workload mTLS; one-shot key; dual fences | no |
| R35 | effect_outcome_admit | runtime | narrow trusted effect-admission adapter (no judgmental field) | E,O,B,D,X | source receipt verification; stable key | no |
| R36/37 | engram_admit/attest/hold/retire; engram_read/search | participant / projection | locally authorized Participant/reviewer; authorized reader | E,P,A,O,D / E,P,O,D | current subject / scope-bound query | no |
| R38 | budget_reconcile, effect_reconcile | governance | exact reconciliation seat (services prepare evidence only) | E,P,A,O,M,B,D,X | fresh challenge for ambiguous release | no |
| R39 | kovee_endeavor_form | governance | source-qualified human via exact Kovee delegated-principal channel, acting for its admitted bound human Participant, personally filling the sole computed seat | E,P,A,O,B,D,X + active Society, pinned bindings, ContextBundle | fresh attempt proof over stable command/idempotency domain | no |
| R40 | external_command_terminalize | governance | same source human via current lineage-authorized channel | E,P,O,X + historical domain, RestoreLineage | fresh proof; locks idempotency + journal heads; never executes | no |
| R41 | idempotency_result, cursor_recover | originating | same actor/channel class + idempotency/cursor audience | original closure + E,P,D | same sender binding; never re-executes | no |
| R42 | external_command_result_query | projection | narrow Kovee recovery workload with current recovery binding | current E,O,X + original refs/digests; RestoreLineage for historical | workload mTLS; read-only; never submits to old incarnation | no |
| R43 | operational_hold/release, diagnose, backup, restore, key_configure, service_configure | admin | infrastructure administrator, separate identity; no Society authorship | endpoint/host policy only | mTLS + operator quorum | no |
| R44 | erasure_execute/verify | admin | narrow retention executor bound to exact ErasureRequest; cannot change target | endpoint witness, E,A,O,D, retention/key/external-copy state | workload mTLS; journal receipts | no |

### 2.A Enablement, binding, principal channel

| L# | Kovee need | Mapping | Class |
|---|---|---|---|
| L1 | governance enablement | genesis: `society_prepare`/`society_bootstrap` → then the greenfield saga (Δ1 two-step; frozen KCP authority-registry row in C2: exact owner/admin actor, subject digest, assurance, recovery-only service authority) | [op R2] + [kovee C2] |
| L2 | `RealmAuthorityBinding` epochs | `KoveeRealmByomBinding` + `KoveeSocietyMapping`; epoch advance invalidates derived channels/permits; dedicated byomd per realm (byom §16) | [kovee C2] |
| L3 | dedicated authority per realm | byom §16 rule restated; no shared multi-tenant service until proven profile | agreement |
| L4 | principal mapping; no manufactured membership | channel supplies actor; human seats only per R2/R6/R8; `KoveeSocietyMapping` | [op R2/R8] + [kovee C2] |
| L5–L6 | `DelegatedPrincipalCredential` | C2 DPC profile consumed by `kovee_endeavor_form`; atomic (issuer,nonce); retry → stored result | [kovee C2] + [op R39, R41] |
| L7 | projection identity | narrow projection service (R4); recovery workload (R42) | [op R4, R42] |
| L8 | binding validated per use | X closure category on every host-facing row; binding revision/epoch in C2 shapes | [kovee C2] |
| L9 | protocol discovery; no nesting | `hello`/`protocol_info`/`feature_info`; BPP envelopes never nested in KCP | [op R1] |

### 2.B Endeavor formation

| L# | Kovee need | Mapping | Class |
|---|---|---|---|
| L10 | immutable `CollaborationContextBundle` | Kovee-owned; ref+digest fields of `KoveeEndeavorFormCommand`, server-recomputed | [kovee C2] + [op R39] |
| L11 | atomic bootstrap with members[] | **Δ1**: superseded — Society/Participants pre-exist; `kovee_endeavor_form` commits Position+Decision+Endeavor atomically, sole human seat; `formation_requires_participation` → `endeavor_propose/position/finalize` | [op R39, R5, R7, R9] |
| L12–L14 | prepare/start/cancel saga | `EndeavorFormationIntent/Slot/Attempt` (states verbatim, byom §16.3); cancel only from `prepared`, no slot | [kovee C2] |
| L15 | reconciliation | five-fact `external_command_result_query`; `external_command_terminalize` (same-source-human); `awaiting_principal` | [op R42, R40] |
| L16 | `ExternalLink` CAS | Kovee-owned; `linking/linked`; byom returns signed `KoveeEndeavorFormResult` | [kovee C2] |
| L17 | confirmation-screen truths | R39 server-recompute rule; Society decision rules rendered (Δ1) | [op R39] + [kovee UI] |
| L18 | post-frontier isolation | new exact assembly+bundle per formation/act; `ContextManifest` recheck at materialization | [kovee C2] + [op R4 context_manifest_show] |

### 2.C Episode lifecycle

| L# | Kovee need | Mapping | Class |
|---|---|---|---|
| L19–L20 | per-episode binding + show | `ByomEpisodeBinding` (episode/attempt refs, dual fence epochs, context refs, budget refs, `allowed_local_commitments`); shown via KCP + R4 | [kovee C2] + [op R4] |
| L21 | dual fence proofs | R30: Byom **and** host fences on every runtime mutation | [op R30] |
| L22 | idempotent turn↔invocation | `Episode/EpisodeAttempt` + `episode_claim` CAS (fence increments; stale worker fenced) | [op R30] |
| L23 | provider manifests | Kovee-owned (C3b); byom sees `ManifestationRevision` + enforcement evidence | [kovee C3b] |
| L24 | typed yield; next turn | `episode_yield/complete/fail`; **Δ3**: next wake participant-authored, kernel-admitted | [op R30] |
| L25 | attention never wakes | notify-only → `wake_intent_submit` (R29) → `activation_admit`/`resource_allocate` [kernel] → `PlacementBinding` [kovee] → `placement_admit` (R33) → `episode_request` (R29) → claim/start (R30) | [op R29, R33, R30] + [kernel] |
| L26 | cancellation honesty | `activity_hold/close` (R29), `episode_fail` (R30); fence advance revokes permits; unknown → ambiguous (L45) | [op R29, R30] |
| L27 | result-first fenced submit | immutable local result commit → `delivery_submit` (R29, exact episode fence); fenced result stays orphan diagnostic | [op R29] |
| L28 | intersections | restrictive effective-profile intersection (byom §16.4); budgets L31–33 | [kovee C2/C3b] |
| L29–L30 | continuation; crash recovery | `continuation_write` (R29) + `ContinuationHead` CAS; claim fences stale attempts | [op R29, R30] |

### 2.D Budgets

| L# | Kovee need | Mapping | Class |
|---|---|---|---|
| L31 | reserve before launch | reservation at `resource_allocate` [kernel]; refs in `ByomEpisodeBinding` | [kernel] + [kovee C2] |
| L32 | subordinate set | `byom_subordinate` bridge saga; never above parent dimensions | [kovee C2] |
| L33 | settlement | `usage_report` (R30) is **evidence only** (byom §11.4); settlement commits from a trusted broker meter or independently verified provider receipt via an internal transition; disagreement/stale lease blocks spend → `budget_reconcile` (R38, governance seat, fresh challenge) | [op R30, R38] + [kernel settlement] |

### 2.E Child work

| L# | Kovee need | Mapping | Class |
|---|---|---|---|
| L34–L37 | `allowed_local_commitments`; intra-turn bounds | C2 field on `ByomEpisodeBinding`; child Kovee commitments bind both fences; cross-episode/peer/deliverable work → `call_open`/`pledge_propose` (R5)/`act_intent_*` (R19–23) | [kovee C2] + [op R5, R19–23] |

### 2.F Effects

| L# | Kovee need | Mapping | Class |
|---|---|---|---|
| L38 | one semantic owner | byom-bound acts: `act_intent_prepare/position/finalize` (R19–23) own the decision; no double human approval | [op R19–23] |
| L39 | one-shot saga | `execution_permit_consume` (R34): one-shot key, dual fences, same-receipt retry | [op R34] |
| L40–L43 | receipt/consumption/permit records | byom `ExecutionConsumptionReceipt` (kernel-issued); Kovee-owned `ExternalAuthorizationConsumption` — **`phase: pre_egress\|atomic_with_egress` is a Kovee field, not a byom receipt field** — and `ExecutionPermit` intersection; `act_intent_cancel` (R24/25) cannot claim rollback | [op R34, R24/25] + [kovee C2] |
| L44 | post-egress ownership | `effect_outcome_admit` (R35): narrow adapter, source facts only | [op R35] |
| L45 | ambiguity | `EffectOutcomeAdmission` head (R35) → only on ambiguous/late-source judgment: `effect_reconcile` (R38) produces `EffectGovernanceDisposition`; EOA head locks first; both in closure; conservative budget settlement while ambiguous | [op R35, R38] |
| L46–L47 | local records; worker fences | Kovee-owned `ActionIntent`/`DecisionReceipt`/`DecisionUse`; interop via exact digests in the consume call; worker mutations demand both fences (R29/R30/R34) | [kovee KCP] + [op R29/30/34] |

### 2.G Human decisions

| L# | Kovee need | Mapping | Class |
|---|---|---|---|
| L48–L50 | rendered validation; digest-bound decision | server-prepared subjects + `PreparationTrace`; human positions R6/R17/R21 with fresh challenge + exact subject digest; CAS re-renders stale subjects; gate inbox renders pending intents/calls/eligible seats (Δ4) | [op R6, R16/17, R19–23] |

### 2.H Projections, directory, memory

| L# | Kovee need | Mapping | Class |
|---|---|---|---|
| L51 | cursored events | `snapshot_get`/`events_read`/`events_wait`/`event_payload` (R4) | [op R4] |
| L52 | epochs + recovery | `cursor_recover` (R41), `recovery_checkpoint_show` (R4); incarnation + recovery epoch | [op R41, R4] |
| L53 | visibility intersection | Kovee-side rule + byom D closure on every projected read | [kovee KCP] + [op R4] |
| L54 | merged timeline | Kovee-owned K5 view over source-ordered streams (never consensus order); **not an I0 deliverable** | [kovee K5] |
| L55 | directory; trust suspension | **Δ6**: routing over `participant_show` (R4) + `engram_search` (R37) evidence; ProfileClaim publish/read ops are a **tracked byom obligation at B0.4** (amendment A5); Akson-binding-change suspension = C4 epochs + capability matrix | [op R4, R37] + tracked B0.4 + [kovee C4] |
| L56 | engrams | `engram_propose/admit/read/search/attest/hold/retire` (R5, R36, R37); quarantine-first | [op R5, R36, R37] |

### 2.I Workspace

| L# | Kovee need | Mapping | Class |
|---|---|---|---|
| L57–L59 | allocation authority + ledger | logical `WorkspaceAllocation` authored at `resource_allocate` [kernel]; placement admitted via `placement_admit` (R33); release bounded by episode lifecycle (R30); physical materialization ledger (`WorkspaceProviderManifest`, `WorkspaceAllocationBinding`) is Kovee-owned with typed KCP transitions frozen in C2/C3b — there is deliberately no workspace-named BPP operation | [kernel] + [op R33, R30] + [kovee C2/C3b] |
| L60 | digest-bound apply | apply is an act: R19–23 + `execution_permit_consume` (R34); driver refuses moved target/stale fence | [op R19–23, R34] |

### 2.J Akson federation

| L# | Kovee need | Mapping | Class |
|---|---|---|---|
| L61 | staged outbound flow | byom §17 flow at the seam, with the I2 profile being the **manual developer profile added by byom amendment A6** (byom v0.2's default remains the akson-confined worker); mandate → act (R19–23) → permit (R34) → **Kovee's** `byom_akson_dispatch_v1` driver stages via the C4 surface → akson consent atomic with dispatch | [op R19–23, R34] + [kovee C4] |
| L62 | least-privilege akson surface | C4 `akson_byom_exchange_v1` + A0; until then no broad-socket connection, no automated-federation claim | [kovee/akson C4] |
| L63 | narrow driver | **Kovee is the sole caller**; byom's delegation engine authorizes (issues the act/permit) and never calls; driver requires current delegation act + akson consent ref | [kovee C4] |
| L64 | atomic-with-egress consumption | byom `ExecutionConsumptionReceipt` + Kovee `ExternalAuthorizationConsumption{phase: atomic_with_egress}` + Kovee `ByomAksonDispatchOutcomeReceipt` head (closed union) | [kovee C2/C4] + [op R34] |
| L65 | late results, advisory cancel | B5 scenarios; late outcomes verified but cannot satisfy advanced generations; EOA/disposition split (R35/R38) | [op R35, R38] |

### 2.K Milestone acceptance language

| L# | Kovee need | Mapping | Class |
|---|---|---|---|
| L66–L67 | K2 "one plan gate, two fenced aspects" | **Δ2**: one Endeavor via `kovee_endeavor_form` (R39); the gate = server-prepared act subject → eligible human `act_intent_position` (R21, current digest, fresh challenge) → deterministic `act_intent_finalize` (R22/23) — **`endeavor_finalize` is formation, never a gate**; two fenced Pledge episodes; delivery via R29 + `review_record` (R14); kill-survival and no-duplicate-formation unchanged | [op R39, R19–23, R29, R14] |
| L68 | K5 no second authority | byom ids/digests/cursors preserved | agreement |
| L69 | K6 exit | maps onto C4 chain identity | [kovee/akson C4] |

## 3. The eleven §17.5 prerequisites, discharged

| # | Kovee §17.5 (verbatim intent) | BPP discharge |
|---|---|---|
| 1 | one-shot UDS `hello` framing | per-surface sockets; version per request or persistent framing after `hello` (byom §14.10) |
| 2 | multi-human principals; no silent impersonation | six surfaces; human seats bind source-qualified humans with fresh challenges (R2/R6/R8); admin never crosses into Society authorship (R43) |
| 3 | `{attempt_id, generation, fence_epoch}` lease proofs | `EpisodeLeaseHead` CAS + fence epochs; R30 demands both fences |
| 4 | server-prepared gate intents | server-prepared subjects + `PreparationTrace`; positions fill only the authenticated actor's seat (R6/R7/R16–21) |
| 5 | idempotent one-shot `execution_permit_consume` | exact operation (R34) |
| 6 | typed operation set | endeavors/calls-pledges/activities/runtime/knowledge/recovery families under Δ2/Δ3 |
| 7 | atomic `mission_submit` | Δ1: `kovee_endeavor_form` (R39) against a pre-existing Society; genesis native (R2) |
| 8 | read-only command-result reconciliation | `external_command_result_query` (R42), five facts, cannot submit or grant |
| 9 | durable idempotency; irreversible keys outlive effects | `IdempotencyDomain` + `idempotency_result` (R41); retention per byom §14.9 |
| 10 | cursor/snapshot epochs + recovery | `cursor_recover` (R41), `recovery_checkpoint_show` (R4), incarnation + recovery epoch |
| 11 | normative MCP/harness schemas | C3a candidate+participant MCP profiles (candidate binding made normative by byom amendment A4) + `attached_harness` `ManifestationRevision` |

## 4. Proposed deltas (Δ1–Δ6)

Kovee's design encodes six Sage-shaped assumptions with no literal BPP
counterpart. Each is **re-scoped by D1's architectural inversion** — proposed
here, binding once C0 ratifies:

- **Δ1 — no member-enrolling bootstrap.** Societies and Participants pre-exist
  (B1 onboarding); `kovee_endeavor_form` fills exactly one human seat;
  multi-party formation falls back to `endeavor_propose/position/finalize`.
  `MissionBootstrap` members[]/approval_rule → the Society's standing decision
  rules. → kovee amendment A4; C2; K2 sheet.
- **Δ2 — plan is a lens.** No canonical plan object, no aspect records:
  aspects → Pledges; `aspect_generation` → pledge revision + generation
  fences; plan gates → server-prepared act subjects decided by eligible human
  `act_intent_position` + deterministic finalize (never `endeavor_finalize`).
  → kovee amendment A4; L66–L69.
- **Δ3 — wake ownership inverts.** Participants (or adopted ActivationPolicies,
  as provenance ordinals) author `WakeIntent`; the kernel admits and
  allocates; Kovee places (`PlacementBinding` → `placement_admit`); episode
  request/claim/start keep their separate authorities. Kovee attention only
  notifies. → plan §0.1.
- **Δ4 — gate kinds become act classes.** `model_egress/share/outbound/apply/
  budget` are a **closed BPA-1-expressible subject taxonomy delivered in C2**
  (schemas + vectors + negatives), carried in ActIntent subjects, bounded by
  Mandates/StandingMandateRevisions; the gate inbox renders pending prepared
  intents and eligible seats. → C2 inventory.
- **Δ5 — `BriefingManifest` dissolves** into ContextAssembly (Kovee) +
  ContextManifest (byom, rechecked at materialization) + byom source fields
  in `ProviderContextManifest` + context refs on `ByomEpisodeBinding`. → C2.
- **Δ6 — directory is evidence, not authority.** Routing = byom §7.5 evidence
  via `participant_show`/`engram_search` today; **ProfileClaim/evidence
  publish-read-search operations are a tracked byom design obligation at
  B0.4** (byom amendment A5); ranked routing UI is Kovee K5; trust suspension
  on Akson binding change is C4/B5 machinery.

## 5. Ratification checklist (C0 exit)

- [x] C0 three-lens codex review executed (2026-07-25, lenses 1–3) — this
      review is C0's ratification evidence (no separate pre-C0 R0)
- [ ] All findings dispositioned (`reviews/2026-07-25-c0-dispositions.md`)
      and the blocker-only confirmation pass returns no P0/P1
- [ ] D8 human sign-off recorded (both project owners + akson maintainer)
- [ ] Amendment records (kovee A1–A5, byom A1–A7) merged
- [ ] `plan/dag.json` v2 green (lifecycle states, grants, sheet schema)
- [ ] Lock manifest v2 seeded and vendored into kovee and akson
