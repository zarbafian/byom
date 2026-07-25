# The akson + kovee + byom family contract

Status: proposed (C0 revision work; ratified only when the plan's §13 re-review
is dispositioned and D8 sign-off is recorded)

Date: 2026-07-25

Canonical here (byom); lock-manifest-vendored into kovee and akson (plan D3).

Pinned sources:

| Artifact | Pin |
|---|---|
| `kovee/DESIGN.md` v0.1 | sha256 `40820c476d59ebdd458955fd5939289b3ef2bff03c3d1266f5e80f3087935860` (repo `7aad4a6`) + `kovee/design/2026-07-25-amendment-governance-owner.md` |
| `byom/DESIGN.md` v0.2 | sha256 `ccea384ff931bcf45d30df680b86835ac682006072a07ef2f34f565eba5fa501` (repo `cc4249c`) + `byom/design/2026-07-25-amendment-family-contract.md` |
| `akson/` | repo `e5e80dc` (pre-release; consumed via the A0 checklist) |
| Implementation plan v2 | sha256 `d5e73952ac90a67a4e4a060052ca66d4be729dec500436b86623b89c54afb2d2` |

## 1. Ownership

| System | Owns | Never |
|---|---|---|
| **Kovee** | Spaces, contributions/relations, lenses, branches, attention, exact ContextAssembly, local commitments, assistant hosting, workers, artifacts, model/tool brokers, effect drivers, placement | Decides governance; acts as genesis governance actor; manufactures authority from intelligence; crosses installations itself |
| **Byom** | Societies, charters, participants, assemblies, endeavors, calls, pledges, mandates, episodes-as-authority, budgets, engrams, decisions | Calls a model; plans; picks workers; executes effects; holds Kovee/Akson credentials |
| **Akson** | Endpoint identity, introduction (ADR-0015), signed contracts, consent, bounded peer execution, evidence, carriage | Understands spaces or societies; grants a peer ambient access |

Three seams, one owner each, consumers vendor via the lock manifest:
`byom_governed_work_v1` (byom-normative text, Kovee-owned host schemas — C2);
the worker/harness binding (C3); `akson_byom_exchange_v1` (akson-side narrow
surface + byom-owned payload schemas — C4). Plan decisions D1–D12 apply as
written in the plan; D9's naming table is carried by the two amendment records.

## 2. The operation × authority matrix

Left side: the kovee requirements ledger (69 rows, extracted from kovee
§6.1/§9/§10/§11.6/§16.1/§17/§18/§26 against the pinned SHA; row ids L1–L69).
Right side: BPP §14.6 catalog operations with their §14.7 registry surface,
actor, and dependency-closure categories. Closure legend (byom §14.7):
**E** endpoint/recovery/Society/Charter · **P** principal/Participant
binding/Standing/self-policy/control domain · **A** assembly/decision snapshot
· **O** exact object+revision · **M** complete Mandate chain · **B**
budgets/meters · **D** visibility/classification/erasure/disclosure · **F**
activity/episode/host fences · **X** Kovee/Akson source bindings.

`[C2]` marks Kovee-owned shapes specified in the governed-work bundle;
`Δn` marks a ratified delta (§4). Zero rows are unmapped.

### A. Enablement, binding, principal channel

| L# | Kovee need | BPP mapping | Notes |
|---|---|---|---|
| L1 | `governance enable` bootstrap | `society_prepare`/`society_bootstrap` — governance surface, human sovereign seat, E,P,D,B, fresh challenge — **then** the greenfield binding saga `[C2]` (D10) | Two steps by design: genesis is native and human; Kovee binds after. Δ1 |
| L2 | `RealmAuthorityBinding` epochs | `KoveeRealmByomBinding` + `KoveeSocietyMapping` `[C2]`; epoch advance invalidates derived channels/permits | isolation_mode → dedicated byomd per realm (byom §16) |
| L3 | Dedicated authority per realm | byom §16: dedicated per realm until a proven realm-scoped multi-tenant profile | restated, both designs agree |
| L4 | Principal mapping; no manufactured membership | identity/binding service: channel supplies actor; `KoveeSocietyMapping` `[C2]`; human seats fillable only by humans (registry rows for governance seats) | |
| L5–L6 | `DelegatedPrincipalCredential` semantics | `[C2]` DPC profile; consumed by `kovee_endeavor_form` (registry: "source-qualified human principal through an exact Kovee delegated-principal channel"); atomic (issuer,nonce) consume; retry → stored result via `IdempotencyDomain` + `idempotency_result` | |
| L7 | Read-only projection identity | projection surface — "authorized principal, Participant, or **narrow projection service**"; recovery reads use the separate recovery-workload binding (`external_command_result_query` row) | |
| L8 | Binding validated before every use | X category in every host-facing closure; binding revision/epoch in `[C2]` shapes | |
| L9 | `ProtocolMatrix` discovery; envelopes never nested | `hello`/`protocol_info`/`feature_info`; BPP keeps its own envelope, `urn:` problems; kovee gateway routes, never wraps | |

### B. Endeavor formation (frontier promotion)

| L# | Kovee need | BPP mapping | Notes |
|---|---|---|---|
| L10 | Immutable `CollaborationContextBundle` | Kovee-owned; ref+digest are fields of `KoveeEndeavorFormCommand` — server recomputes and admits the exact bundle | |
| L11 | Atomic `mission_submit` bootstrap with members[] | **Δ1** — superseded: Society and Participants pre-exist; `kovee_endeavor_form` commits Position + GovernanceDecision + Endeavor + idempotency result + journal transition atomically, sole human formation seat; `formation_requires_participation` → ordinary `endeavor_propose/position/finalize` | |
| L12–L14 | prepare/start/cancel saga | `EndeavorFormationIntent/Slot/Attempt` `[C2]` (states verbatim from byom §16.3); cancel only from `prepared` with no slot | |
| L15 | Read-only reconciliation | `external_command_result_query` (five-fact union) + `external_command_terminalize` (same-source-human only) + `awaiting_principal`; resubmission only by fresh same-human proof over the stable command bytes | |
| L16 | `ExternalLink` CAS | Kovee-owned; `linking/linked` intent states; byom returns the signed `KoveeEndeavorFormResult` envelope (also on `committed` recovery) | |
| L17 | Confirmation screen truths | server-recompute rule: request fields can only match server-derived values; screen renders Society decision rules instead of members[]/approval_rule (Δ1) | |
| L18 | Post-frontier isolation | new exact assembly + bundle required; `ContextManifest` rechecked at materialization (Δ5) | |

### C. Episode lifecycle (turns → episodes)

| L# | Kovee need | BPP mapping | Notes |
|---|---|---|---|
| L19–L20 | `SageTurnBinding` → per-episode binding | `ByomEpisodeBinding` `[C2]`: episode/attempt refs, dual fence epochs, context refs (Δ5), budget reservation refs; shown via kovee KCP + byom projection surface | |
| L21 | Dual fence proofs | runtime registry row: episode ops demand "mTLS/attested workload; **Byom and host fences**" | |
| L22 | turn↔invocation idempotency | `Episode`/`EpisodeAttempt` + `episode_claim` CAS lease (fence increments; stale worker fenced); binding idempotency key `[C2]` | |
| L23 | Provider manifests | Kovee-owned provider/workspace manifests (C3b); byom sees `ManifestationRevision` (host_kind `kovee_deployment`) + enforcement evidence | |
| L24 | Typed yield; next turn | `episode_yield`/`episode_complete`/`episode_fail`; **Δ3**: the next wake is participant-authored, kernel-admitted — no central scheduler | |
| L25 | Attention never wakes | notify-only; `wake_intent_submit` → `activation_admit` → `resource_allocate` (kernel, non-callable) → `placement_admit` (narrow Kovee placement adapter) | Δ3 |
| L26 | Cancellation/deadline honesty | `activity_hold/close`, `episode_fail`; fence advance revokes permits; unknown outcome → ambiguous machinery (L45) | |
| L27 | Result first, then fenced submit | `delivery_submit` (participant surface, "exact episode fence when cited") after the immutable local result commit; fenced episode's result stays orphan diagnostic | |
| L28 | Deadline/budget/profile intersection | restrictive effective-profile intersection (byom §16.4); budgets via L31–L33 | |
| L29–L30 | Continuation; crash recovery | `continuation_write` + `ContinuationHead` CAS (one generation head); claim-fences stale attempts; resume from Continuation on a different compatible Manifestation is a conformance scenario | |

### D. Budgets

| L# | Kovee need | BPP mapping | Notes |
|---|---|---|---|
| L31 | Authoritative reservation before launch | `BudgetAccount` conservation + reservation at `resource_allocate`; refs land in `ByomEpisodeBinding` | |
| L32 | Subordinate set | `byom_subordinate` bridge saga `[C2]`; never above parent dimensions; lower local ceilings allowed | |
| L33 | Idempotent settlement | `usage_report` (runtime) + `UsageSettlement(Head)`; disagreement/stale lease blocks spend → `budget_reconcile` (governance, fresh challenge) | |

### E. Child work inside a governed episode

| L# | Kovee need | BPP mapping | Notes |
|---|---|---|---|
| L34–L37 | `allowed_local_commitments`; intra-turn bounds | `[C2]` field on `ByomEpisodeBinding`; child Kovee commitments bind both fence proofs; anything cross-episode, peer-reaching, or deliverable-producing uses typed byom ops (`call_open`/`pledge_propose`/`act_intent_*`) under the applicable decision | audit refs enter the episode record and `ProviderContextManifest` |

### F. Effects and external authorization

| L# | Kovee need | BPP mapping | Notes |
|---|---|---|---|
| L38 | One semantic owner per effect | byom-bound acts: `act_intent_prepare/position/finalize` own the decision; a local `DecisionUse` cannot consume them; no double human approval | Δ4 |
| L39 | One-shot pre-egress saga | `execution_permit_consume` — runtime surface, "trusted host effect service bound to exact prepared host Effect; workload mTLS, exact one-shot key, dual fences"; retry returns the same `ExecutionConsumptionReceipt` | exact op name match |
| L40–L43 | Receipt binding; consumption record; non-reversal; intersection permit | `ExecutionConsumptionReceipt` (byom-owned); `ExternalAuthorizationConsumption`/`ExecutionPermit` (Kovee-owned, `owner_protocol: byom\|akson`); `act_intent_cancel` "cannot claim effect rollback" | |
| L44 | Post-egress broker ownership | `effect_outcome_admit` — "narrow trusted effect-admission adapter", closure E,O,B,D,X, no judgmental field | |
| L45 | Ambiguity honesty | `EffectOutcomeAdmission`(+Head) then `EffectGovernanceDisposition`(+Head), EOA-head-first lock order, both in closure; `effect_reconcile` (governance seat, fresh challenge for ambiguous release); conservative budget settlement while ambiguous | |
| L46–L47 | Local intent/decision records; worker fence rows | Kovee-owned; interop via exact digests in the consume call; worker mutations demand current Kovee fence **and** byom fence when bound | |

### G. Human decisions (gate inbox)

| L# | Kovee need | BPP mapping | Notes |
|---|---|---|---|
| L48–L50 | Rendered validation; digest-bound decision; no cached subject | server-prepared subjects + field-complete `PreparationTrace`; human positions via `act_intent_position`/`mandate_position`/`charter_position` etc. (governance seats, fresh phishing-resistant challenge, exact subject digest); CAS on exact intent digest re-renders stale subjects; the "gate inbox" renders pending intents/calls/positions | Δ4 |

### H. Projections, timeline, directory, memory

| L# | Kovee need | BPP mapping | Notes |
|---|---|---|---|
| L51 | Cursored events | `snapshot_get`/`events_read`/`events_wait`/`event_payload` (projection surface, opaque audience-bound cursors) | |
| L52 | Epochs, expiry, rebuild | `cursor_recover`, `recovery_checkpoint_show`, endpoint incarnation + recovery epoch; authorized snapshot + boundary cursor for full rebuild | |
| L53 | Visibility intersection | Kovee-side rule; byom D closure on every projected read | |
| L54 | Merged causal timeline | Kovee-owned view over source-ordered streams; never cross-system consensus order | |
| L55 | Directory; trust suspension | **Δ6** — byom §7.5 claims/interests/evidence + B4 claim/evidence directory; Akson-binding-change suspension via `[C4]` epochs + fail-closed capability matrix (B5) | |
| L56 | Engrams | `engram_propose/admit/read/search/attest/hold/retire`, `context_manifest_show`; quarantine-first; Kovee stores bytes by canonical digest | |

### I. Workspace

| L# | Kovee need | BPP mapping | Notes |
|---|---|---|---|
| L57–L59 | Provider manifests; fenced allocation ledger | Kovee-owned (`WorkspaceProviderManifest`, `WorkspaceAllocationBinding`); byom holds the logical `WorkspaceAllocation` only; no workspace-named BPP operation exists — scope reaches workers via the effective-profile intersection | |
| L60 | Digest-bound apply | apply is an act: `act_intent_*` + `execution_permit_consume`; driver refuses moved target or stale fence | |

### J. Akson federation

| L# | Kovee need | BPP mapping | Notes |
|---|---|---|---|
| L61 | Staged outbound flow | byom §17 flow is kovee §18.1's flow — aligned verbatim at the seam; mandate → act → permit → `byom_akson_dispatch_v1` driver `[C2]` → akson consent atomic with dispatch | |
| L62 | Least-privilege akson surface | `[C4]` `akson_byom_exchange_v1` + A0; until then no broad-socket connection, no automated-federation claim | |
| L63 | Narrow `kovee-akson` driver | unchanged; callable only by byom's delegation engine with current delegation act + akson consent ref | |
| L64 | Atomic-with-egress consumption | `ExecutionConsumptionReceipt` + `phase: atomic_with_egress` + `ByomAksonDispatchOutcomeReceipt` closed union (Kovee-owned head) | |
| L65 | Late results, advisory cancel | B5 scenarios; late outcomes verified but cannot satisfy advanced generations; quarantine via EOA/disposition split | |

### K. Milestone acceptance language

| L# | Kovee need | BPP mapping | Notes |
|---|---|---|---|
| L66–L67 | K2: "one plan gate, two fenced aspects" | **Δ2** — reads: one Endeavor formed by `kovee_endeavor_form`, one governed decision (`endeavor_finalize` or `act_intent_finalize`), two fenced Pledge episodes, base-bound deliverable via `delivery_submit`/`review_record`; kill-survival and no-duplicate-formation unchanged | |
| L68 | K5: no second authority | unchanged; byom ids/digests/cursors preserved | |
| L69 | K6 exit | unchanged in substance; maps onto `[C4]` chain identity | |

## 3. The eleven §17.5 prerequisites, discharged

| # | Kovee §17.5 (verbatim intent) | BPP discharge |
|---|---|---|
| 1 | one-shot UDS `hello` framing | per-surface Unix sockets; version per request or persistent framing after `hello` (byom §14.10) |
| 2 | multi-human principals; no silent operator impersonation | six surfaces; human seats bind source-qualified human principals with fresh challenges; admin never crosses into Society authorship (registry postscript) |
| 3 | `{attempt_id, generation, fence_epoch}` lease proofs | `EpisodeLeaseHead` CAS + fence epochs; runtime rows demand both fences |
| 4 | server-prepared gate intents | server-prepared subjects + `PreparationTrace`; positions fill only the authenticated actor's eligible seat |
| 5 | idempotent one-shot `execution_permit_consume` | exact operation, runtime surface, one-shot key, same-receipt retry |
| 6 | typed operation set | endeavors / calls-and-pledges / activities / runtime / knowledge / recovery families, under Δ2/Δ3 ontology |
| 7 | atomic `mission_submit` | Δ1: `kovee_endeavor_form` against a pre-existing Society; genesis is native |
| 8 | read-only command-result reconciliation | `external_command_result_query` five-fact union; cannot submit or grant |
| 9 | durable idempotency, irreversible keys outlive effects | `IdempotencyDomain` + `idempotency_result`; retention per byom §14.9 |
| 10 | cursor/snapshot epochs + recovery | `cursor_recover`, `recovery_checkpoint_show`, incarnation + recovery epochs |
| 11 | normative MCP/harness schemas | C3a MCP candidate+participant profiles; `attached_harness` `ManifestationRevision`; conformance-tested |

## 4. Ratified deltas (Δ1–Δ6)

Kovee's design encodes six Sage-shaped assumptions with no literal BPP
counterpart. Each is **re-scoped by D1's architectural inversion** — recorded
here so no implementer treats the design text as controlling:

- **Δ1 — no member-enrolling bootstrap.** Societies and Participants pre-exist
  (B1 onboarding); `kovee_endeavor_form` fills exactly one human seat;
  multi-party formation falls back to `endeavor_propose/position/finalize` via
  `formation_requires_participation`. `MissionBootstrap` members[]/
  approval_rule are replaced by the Society's standing decision rules.
  → kovee amendment A4; K2 sheet.
- **Δ2 — plan is a lens.** No canonical plan object, no aspect records:
  aspects → Pledges; `aspect_generation` → pledge revision + activity/episode
  generation fences; plan gates → Endeavor/act decisions. K2/K5/K6 acceptance
  language reads per L66–L69. → kovee amendment A4.
- **Δ3 — wake ownership inverts.** No governance scheduler admits events or
  creates turns: participants (or their adopted ActivationPolicies) author
  `WakeIntent`; the kernel admits and allocates; Kovee places. Kovee attention
  only notifies. → already normative in plan §0.1.
- **Δ4 — gate kinds become act classes.** `model_egress/share/outbound/apply/
  budget` are BPA-1-expressible act/effect classes carried in ActIntent
  subjects and bounded by Mandates/StandingMandateRevisions — not a
  per-Endeavor `gate_policy_ref` object. The kovee "gate inbox" renders
  pending prepared intents and eligible seats. → C2 subject taxonomy.
- **Δ5 — `BriefingManifest` dissolves.** Exact per-wake context =
  `ContextAssembly` (Kovee) + `ContextManifest` (byom, rechecked at
  materialization) + byom source fields in `ProviderContextManifest` + context
  refs on `ByomEpisodeBinding`. → C2 fields.
- **Δ6 — directory is evidence, not authority.** Observed-outcome routing =
  byom §7.5 evidence + the B4 claim/evidence directory; ranked routing UI is a
  Kovee projection (K5). Trust suspension on Akson binding change = C4
  epoch/capability machinery (B5). → B0.4/B0.5 bundle freezes carry the detail.

## 5. Ratification checklist (C0 exit)

- [ ] Plan §13 step-2 re-review of plan v2 dispositioned (three lenses)
- [ ] D8 human sign-off recorded (both project owners + akson maintainer)
- [ ] Amendment records merged in kovee and byom
- [ ] This contract + matrix reviewed at R0 (layers L1, L4)
- [ ] `plan/dag.json` green; near-term sheets present (C0, C1, C3a, I0, K0,
      K1, B0.1, B1)
- [ ] Lock manifest seeded (`plan/family-lock.json`) and consumers reference it
