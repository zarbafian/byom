# Operation schemas — complete B0.1 bundle (all sheet families)

One closed request and one closed result schema per operation
(`<op>-request.schema.json`, `<op>-result.schema.json`; the result schema is
the `Success.result` payload). Field names are verbatim from the DESIGN.md
record shapes (§6.1–§6.2, §7.1–§7.4, §9.1–§9.5, §10.1–§10.5, §11.1–§11.3,
§13.1, §14.4, §15.3); surface, actor, and closure come from the family
contract's registry bindings
R2/R3/R4/R5/R6/R7/R8/R9/R10/R11/R12/R13/R14/R15–R25/R29/R34/R41
(`design/2026-07-25-family-contract.md` §2.0, reproducing §14.7). Requests
carry only caller-supplied arguments: the server derives actor, Participant,
Manifestation, Society, and surface from the channel, and request fields
naming those objects can never override the binding (§14.3) — the
"wrong-surface args" negative vectors pin that.

## Gap notes (explicit derivations beyond DESIGN.md's spelled text)

DESIGN.md defines record shapes and state-machine rows, not per-operation
argument or result payloads. Where an op's args are not spelled out they are
derived minimally from the record shape plus the §14.6
prepare/position/finalize catalog rule. Every such derivation is listed here
and freezes with the bundle registry; a conflicting registry freeze wins.

- **G1 — society_prepare/society_bootstrap payloads.** §14.6 gives only the
  prepare rule ("the server prepares canonical subjects and required seats")
  and §6.1 the atomic genesis set. Derived: prepare takes the bootstrap
  inputs (home authority, optional Kovee bindings, staged charter and
  classification refs + digests) and returns the Society in `forming` plus
  `preparation_ref`, `subject_digest`, `sovereign_seat_set`, `expires_at`;
  bootstrap consumes exactly `{society_id, preparation_ref, subject_digest}`
  and returns the active Society record plus `genesis_event_ref`.
- **G2 — society hold/release/dissolve fields.** `hold_reason_ref`,
  `release_reason_ref`, `authority_fence_epoch` ("hold increments authority
  fence"), `dependency_revision_ref` ("release creates new dependency
  revision"), and `dissolution_decision_ref`/`retention_and_key_plan_ref`
  (the §14.8 locked dependencies) are derived names. The dissolving →
  dissolved completion is read as a later `society_dissolve` call once the
  disposition ledger completes (§14.8: "terminal record after disposition
  ledger"; "never auto-completes").
- **G3 — timestamp encoding (amended, RT-17).** DESIGN.md fixes no wire
  encoding for `created_at`/`expires_at`/`effective_at`; these schemas
  use RFC 3339 UTC with a `Z` offset — SEMANTICALLY valid: the shared
  def's pattern pins lexical ranges (months 01-12, days 01-31, hours
  00-23, minutes/seconds 00-59; leap seconds not accepted) and
  implementations MUST additionally reject impossible proleptic-
  Gregorian instants such as `2026-02-30T12:00:00Z`. The conformance
  runner enforces the calendar check on every vector; the
  invalid-calendar vectors pin it.
- **G4 — result payloads generally.** Every result is a minimal projection
  of the §6/§7 record the operation creates or moves; DESIGN.md does not
  enumerate result fields for any of these ops. The onboarding_offer result
  additionally pins the record's invariant constants (`max_episodes: 1`,
  the three-op `allowed_operations` set,
  `general_effect_and_child_authority: "none"`).
- **G5 — participant_propose kind-conditional args.** §7.1 prose (human:
  principal observation; agent: controller domain + proposed
  ManifestationRevision; collective: Assembly epoch) becomes optional
  `principal_observation_ref`, `controller_domain_ref`,
  `proposed_manifestation_ref`, `assembly_ref`/`assembly_epoch`; the
  kind/argument pairing is a registry rule the schema cannot express.
- **G6 — membership_offer_revoke's `revoked_by_decision_ref`.** The record
  shape has only `offered_by_decision_ref`; R10's "exact decided governance
  actor" implies a deciding record for revocation.
- **G7 — participant_admit admission subject.** `admission_subject_digest`,
  `admitted_by_decision_ref`, and `included_self_policy_proposal_refs`
  derive from §14.3 ("activates only candidate-authored self-policy
  proposals included in the admission subject") and the StandingRevision
  shape's `adopted_by_decision_ref`/`membership_acceptance_ref`.
- **G8 — participant_suspend args.** `suspended_by_decision_ref` and
  `suspension_reason_ref` derive from R10; the charter's suspension_rule
  governs the decision itself.
- **G9 — participation_cease / participant_retire.** Both are argument-free
  self-operations (the affected Participant is channel-derived; optional
  `statement_ref` on cease). §14.8's Participant row lists "cease/retire"
  against "suspended or retiring → retired"; read here as cease: active (or
  suspended, since §7.4 grants cease "at any time") → retiring with Standing
  → ceased, and retire: retiring → retired.
- **G10 — opaque policy bodies.** `terms_constraints`,
  `minimum_cancellation_rights`, `context_and_disclosure_ceilings`,
  `budget_and_obligation_ceilings`, `purpose_and_context_ceilings`,
  `budget_rate_and_concurrency_ceilings`, `rate_limit`,
  `schedule_constraints`, `allowed_manifestation_selector`,
  `compatibility_selector`, and `proposed_policy_body` are open objects
  pending the BPA-1 policy-algebra slice (ADR-0001); selector lists
  (`endeavor_selectors`, `mandate_selectors`, …) are identifier arrays
  referencing selector definitions. LANDED (RT-06): the immutable
  successor versions `<op>-…-v2.schema.json` are published now — every
  G10 body is bound to the BPA-1 AST (a byte-identical nested
  `bpa1Policy` def, runner-verified against `../bpa1-policy.schema.json`),
  a closed DecisionRule/terms reference (`{rule_ref, rule_digest}` /
  `{terms_ref, terms_digest}`, exact digest-pinned), the closed §10.1
  delegation shape, or a quantity-atom budget set. The v1 publications
  stay unchanged (immutability); B1 and the C3a tool document freeze to
  the v2 versions (`spec/registry.json` names them per op).
- **G11 — continuity_root_update discriminator.** §14.8 drives the whole
  ContinuityRoot lifecycle through this one op; `target_status`
  (`active | sealed | retired`) plus optional `continuity_root_ref` (absent
  exactly on first adoption) and head CAS via `meta.expected_revision` is
  the derived encoding.
- **G12 — `standing_replacement`.** The §14.8 Standing row names
  "replacement" with no catalog operation; the descriptor records it as the
  named transition `standing_replacement` pending the registry freeze.
- **G13 — descriptor cascade marking.** §14.8 repeats `membership_refuse`,
  `membership_offer_revoke`, `participant_admit`, `onboarding_offer`,
  `participant_suspend`, and `participation_cease` across machine rows
  (one transaction moves several records). In `spec/descriptors/` the
  non-owning occurrences carry `"cascade": true`; the conformance parity
  check counts owning transitions only, so every mutating op is owned by
  exactly one descriptor while the cross-machine atomic effects stay
  machine-readable.
- **G14 — participant_show redaction.** `independence_domain_ref` is
  protected governance metadata (§7.1) and is optional in the result:
  omitted for readers outside the D closure. Reads (`society_show`,
  `participant_show`) never carry `meta` (closed schemas).
- **G15 — descriptor file set.** Beyond the five headline machines
  (society, membership-offer-standing, onboarding-activation-offer,
  candidate-self-policy, manifestation), the exactly-one parity rule
  requires descriptors for the remaining §14.8 rows this slice's ops drive:
  `participant.json`, `participant-assent-policy.json`,
  `participant-activation-policy.json`, `continuity-root.json` — each
  extracted from its own §14.8 row.
- **G16 — channel-derived candidate fields.** `candidate_participant_ref`,
  `candidate_binding_epoch`, `candidate_actor_ref`,
  `onboarding_fence_epoch`, `refused_by_actor_ref`,
  `accepted_by_actor_ref`, and `authentication_observation_ref` from the
  §7.4 record shapes never appear as request fields: the sender-constrained
  candidate credential supplies them (§14.3).
- **G17 — endeavor creation and closure encoding.** §14.8's Endeavor rows
  begin at `proposed`; absent → proposed / `endeavor_propose` is derived
  from the §14.6 catalog plus the §9.1 state list. The propose result
  carries the server-prepared canonical subject and seats (§14.6) as
  `subject_digest` and `required_seat_refs` (derived names).
  `endeavor_close` is one operation discriminated by `target_state`
  (`reviewing | fulfilled | failed | abandoned | dissolved`) per the G11
  precedent; which targets require `closure_decision_ref` /
  `acceptance_evidence_refs` is a registry rule the schema cannot express.
- **G18 — seat position payloads (`endeavor_position`,
  `pledge_position`).** Request fields are verbatim from the §10.3 Position
  shape minus the server-derived actor fields (`position_id`, `revision`,
  `participant_ref`, `actor_ref`, `participant_binding_epoch`,
  `endpoint_incarnation`, `recovery_epoch`,
  `authentication_observation_ref`, `status`, `digest`); a request naming
  another participant's identity fails the closed schema (positions fill
  only the authenticated actor's eligible seat, §14.6). Pre-finalization
  withdraw/supersede is seat-head CAS via `prior_position_digest` plus a
  derived `target_status` (`active | withdrawn`) discriminator.
  Amended (RT-03): a `*_policy_derived` assent_mode REQUIRES its
  `derived_assent_receipt_ref` (closed oneOf; a direct mode forbids it),
  the result's Position `digest` is REQUIRED (it is the seat-head CAS
  token), and `seat_ref` must name one of the prepared slot records'
  concrete `seat_refs` (gap note G44).
- **G19 — Position/Decision row folding.** §14.8's generic Position/Decision
  machine gets no descriptor of its own in this slice: each family's
  position operation is owned by its subject's descriptor as a
  proposal-stage self-transition (`proposed → proposed`), with the Position
  record lifecycle (absent → active → withdrawn or superseded) noted there.
- **G20 — PledgeProposal folded state.** §9.3 leaves `PledgeProposal.state`
  unspecified; the Pledge descriptor and the `pledge_propose`/`pledge_amend`
  results use `proposed` as the folded pre-formation state of the Pledge
  machine. A lapsed proposal creates no Pledge terminal state, and the Call
  machine's `forming → open` on linked-proposal lapse is read as server
  time (§14.8 Call row: "exact linked proposal/outcome/server time").
- **G21 — pledge formation/resume result extras.** `pledge_workstream_ref`
  and `initial_mandate_ref` name §14.8's "initial PledgeWorkstream and
  optional Mandate" atomic effects; `pledge_resume` returns the new
  workstream generation as `workstream_generation` (§14.8: "each resume
  starts a new Activity generation").
- **G22 — pledge disposition 'decision'.** §14.8's Pledge row lists
  "decision" for nonterminal → canceled/failed with no catalog operation
  (§9.5: the remaining obligation becomes canceled, novated, disputed,
  failed, or unresolved under its own procedure); the descriptor records it
  as the named transition `pledge_disposition_decision` pending the
  registry freeze (G12 precedent). AMENDED by D-RT-3 (RT-03): the same
  row's "amend" against → superseded is re-cut — `pledge_amend` only
  creates the separate proposed successor (absent → proposed), and the
  descriptor records `pledge_finalize` as the superseding operation:
  supersession commits exactly when finalization accepts the successor's
  complete fresh seat set under the successor CAS (one CAS successor
  slot; `supersedes_pledge_ref`/`supersedes_pledge_revision` pin the
  predecessor).
- **G23 — pledge_amend payload.** `amendment_of`
  `{pledge_ref, pledge_revision, prior_terms_digest}` is verbatim §9.3; the
  amendment restates the full terms ("needs all currently required seats
  again", §9.5); `proposed_pledgor_ref`/`beneficiary_ref` carry over from
  the prior revision when absent (registry rule); a successor-slot conflict
  fails `stale_revision`. `pledge_propose` never carries `amendment_of`.
- **G24 — delivery pledgor binding and episode fence.** `delivery_id`,
  `delivered_by_participant`, `actor_ref`, `subject_digest`, `state`, and
  `submitted_at` are channel/server-derived (a Delivery is submitted only
  by the authenticated pledgor channel, §9.5). R29's "exact episode fence
  when cited" is encoded as optional `episode_ref` + `byom_fence_epoch` +
  `expected_lease_revision` (names from the §11.2 EpisodeAttempt/
  EpisodeAttemptEvent shapes); `episode_ref` implying the fence fields is a
  registry rule. A later `delivery_submit` for the same Pledge revision
  supersedes the prior Delivery (§9.5 state list).
- **G25 — activity hold/close encoding.** `activity_close` is discriminated
  by `target_state` (`completed | failed | canceled`) per the G11
  precedent; `generation` on `activity_hold`/`activity_close` (and the
  other R29 ops) carries the generation fence explicitly. The catalog has
  no activity un-hold operation and §14.8's ActivityStream row lists no
  held → nonterminal transition; none is derived.
- **G26 — wake intent server-derived provenance.** `wake_intent_id`,
  `revision`, `participant_ref`, `participant_binding_epoch`, `actor_ref`,
  `root_activation_mode`, `root_activation_control_domain_ref`/`_digest`,
  `activation_policy_use_ordinal` (atomically consumed on policy-derived
  intents), `submitted_at`, `state`, and `digest` from the §11.1 record
  shape are server-derived; the request supplies `origin` plus the optional
  exact `activation_policy_ref`/`_digest`.
- **G27 — episode_request payload.** The caller supplies the owner-intent
  chain (`activity_stream_ref`, `generation`, `wake_intent_ref`,
  `activation_admission_ref`, optional `pledge_revision`, `deadline`);
  manifestation, context-manifest, resource-allocation, and placement
  fields are kernel/saga-derived — no stage can be skipped (§11.1). §14.8's
  Episode row begins at `prepared`; the creation transition is derived.
- **G28 — continuation head CAS fields.** `expected_head_revision` is
  verbatim §11.3; the result's `head_revision` projects
  `ContinuationHead.revision`. Predecessor absence exactly at revision zero
  is a registry rule; the losing concurrent writer receives
  `stale_revision` with the current opaque head (vector). Optional
  `episode_ref` + `byom_fence_epoch` pin the current Episode fence when
  episodic.
- **G29 — folded work-lifecycle descriptors.** Delivery and Review share
  one descriptor per their single §14.8 row; WakeIntent, ActivationAdmission,
  and ResourceAllocation fold into `wake-intent.json` with
  `admission_`/`allocation_` state prefixes; Episode and its lease head
  fold into `episode.json` with the `lease_` prefix. Runtime-family vias
  (`episode_claim`/`start`/`yield`/`complete`/`fail`) and cross-machine
  activity/pledge effects are `"cascade": true` pending their owning
  slices; `activation_admit`/`resource_allocate` remain named internal
  kernel transitions (§11.1), never callable operations.
- **G30 — Mandate `prepared` folding and prepare payloads.** §10.1's Mandate
  state enum has no pre-issue state, but §14.8's row begins
  "prepared/positions → active / issue"; `prepared` is the folded pre-issue
  state (G20 precedent). `mandate_prepare`/`mandate_derive` requests carry
  the §10.1 record's caller-suppliable scope fields; `issuer_ref` is
  channel-derived when the authorized issuer prepares and is named as a
  request field only when the proposed grantee prepares (R15's two actor
  readings). Results follow the endeavor_propose pattern plus the R15
  preparation-trace fence (`preparation_trace_ref`/`_digest`).
- **G31 — mandate selector encoding.** Following the G10 split exactly:
  `resource_selectors`, `data_class_selectors`, `destination_selectors`, and
  `delegation.grantee_selectors` are identifier arrays referencing selector
  definitions; `manifestation_selector` is bound to the BPA-1 AST in the
  published v2 successor schemas (RT-06; G10's landing note). The
  `delegation` object is verbatim §10.1
  (`{allowed, max_depth, max_children, grantee_selectors}`).
- **G32 — mandate issue/hold/revoke derivations.** `held_by_decision_ref`,
  `hold_reason_ref`, `revoked_by_decision_ref`, and `revocation_reason_ref`
  derive from R18 plus the G2/G6/G8 precedents. Successor issuance
  atomically supersedes the prior active revision (§14.8 active →
  superseded / matching operation; one owner, mandate_issue). The catalog
  has no mandate un-hold operation and §14.8's row lists no held →
  nonterminal transition; none is derived (G25 precedent). active →
  exhausted is the consumption cascade: MandateUse ordinal slots are created
  on consumption (§14.8) and the use consuming the last slot exhausts the
  Mandate — recorded as a `"cascade": true` occurrence of
  `execution_permit_consume`.
- **G33 — never-widening is kernel-checked.** §10.2's subset rules compare
  child and parent canonical values; a schema cannot express them. The
  derive request pins the exact parent
  (`parent_mandate_ref`/`_revision`/`_digest`, R15 "parent revisions"); an
  absent child selector field carries the parent's set unchanged (never
  wider); a widening attempt fails the typed `authority_widening` problem
  (§14.9) — vector.
- **G34 — act_intent_prepare payload.** The caller supplies typed input
  (kind, execution_kind, exact subject ref+revision, exact Mandate
  ref/revision/digest, optional endeavor/pledge lineage, exact
  context/disclosure manifests, driver_audience); `intent_id`,
  `requested_by_participant`, `actor_ref`, `subject_digest`,
  `preconditions`, `stable_execution_key`, `budget_reservation_set_ref`,
  the authorization dependency set, and `expires_at` are server-derived
  (§10.5: preparation never supplies a semantic default). The result embeds
  the field-complete PreparationTrace verbatim (§10.5, R19 fence) plus
  the derived `required_seat_refs`. Amended (RT-04): the trace is the
  ONE reusable closed shape on EVERY prepared result (society/charter/
  endeavor/pledge/mandate/act-intent), its `operation` is const-bound
  per schema, `field_sources` is non-empty, and the runner mechanically
  verifies subject/dependency digest binding plus COMPLETE
  output-pointer provenance in both directions; `kind` and `preconditions` items stay open
  pending the Δ4 closed act-class taxonomy (C2).
- **G35 — dual-surface operations.** `mandate_position` (R16/R17),
  `act_intent_position` (R20/R21), `act_intent_finalize` (R22/R23), and
  `act_intent_cancel` (R24/R25) exist on both participant and governance
  surfaces. The registry key is `(operation, surface)`, but schema names key
  by operation alone (the bundle/runner rule): one request/result pair per
  operation, with the per-surface actor constraint documented in the schema
  and enforced by the registry — surface is not schema-expressible. Position
  field names stay verbatim §10.3 (`proposal_ref`/`proposal_revision`/
  `subject_digest` name the prepared intent and its exact digest for act
  intents).
- **G36 — `host_effect_attempt`.** §14.8's ActIntent row lists 'host
  attempt' (consumed → executing) with no catalog operation: the execution
  host stores the receipt, intersects it with stricter platform policy,
  mints its local permit, and only then creates a driver attempt (§13.1
  step 7). Recorded as the named transition `host_effect_attempt` pending
  the registry freeze (G12/G22 precedent) — Kovee-owned, never callable on
  BPP.
- **G37 — execution_permit_consume one-shot encoding.** The request carries
  §13.1 step 4 verbatim: the one-shot `stable_execution_key`, exact
  intent/host-effect/subject/context/disclosure/budget/driver-audience
  bindings, and
  both current fences — `byom_fence_epoch` + `host_fence_epoch` are the
  derived dual-fence names (R34; G24 precedent). The result is the verbatim
  ExecutionConsumptionReceipt with `max_uses` pinned const 1. Amended
  (RT-05): `meta.expected_revision` (the exact authorized intent
  revision) is REQUIRED — the one-shot decision consumes against a
  pinned head; the CONTEXT and disclosure bindings are each
  both-or-neither pairs (closed oneOf arms, four arms covering both /
  context-only / disclosure-only / neither), and both are compared —
  ref AND digest — against the pair committed in the act subject, i.e.
  the pair the gate seat assented to, so an act authorized under one
  ContextManifest is not consumable under another and a consumption that
  presents no context for an act that pins one is refused with
  `stale_binding` rather than executed blind (R3-A01). The `episode_ref`
  no longer travels as a pair: its fence digest was byom's own record and
  was removed under A8's converse half (G48). Same canonical request and
  key → same receipt; a changed request → `idempotency_mismatch`; a
  different key cannot consume the spent one-shot decision (→
  `stale_revision`); a stale fence → `stale_revision`; a dangling
  disclosure or context ref fails the closed schema — vectors for all four
  negative classes. Amended again (the receipt half of G48, below): EVERY
  member of the result is rendered — a digest returned as `null` is a
  conformance failure — and the two members the consumer cannot re-derive
  from its own state are `portable_public` over frozen cross-boundary
  fragments.
- **G38 — cursor and page encoding.** Continuation tokens are one opaque
  scope- and audience-bound string (§14.4: authenticated cursor, endpoint
  incarnation, recovery epoch, filter digest, retention semantics), bounded
  at 4096 visible-ASCII bytes (derived; identifiers cap at 128 bytes and
  tokens carry more). `page_size` is required on every list (§14.9 "one
  explicit page size"): 1..512 for events (§14.9 cap), 1..256 for
  charter_history (derived from the mutation list cap).
  `max_wait_milliseconds` 1..60000 is the derived bounded-wait cap.
  `internal_cursor`, `society_sequence`, and `endeavor_sequence` are never
  projected (§14.4: internal sequence values are not exposed; ordering
  travels only inside the token) and tokens never expose hidden counts.
- **G39 — snapshot and payload projection.** `snapshot_get` returns a
  snapshot descriptor by immutable reference plus a fresh continuation
  (§14.9 moves larger content by reference); `kinds`/`endeavor_ref` are
  derived narrowing-only filter fields (§14.4: filters never widen
  visibility). `event_payload` returns the typed payload as an open object
  under the 1 MiB response cap, pinned by the ledger's `payload_digest`;
  per-kind payload schemas land with the event registry (later bundle).
- **G40 — recovery core reads.** `idempotency_result` and `cursor_recover`
  are classed as reads (R41 "never re-executes"; no MutationMeta) on the
  originating surface; the IdempotencyDomain's actor binding, incarnation,
  and society scope are channel-derived and never request fields (§14.3,
  G16 precedent). `idempotency_result`'s
  `status: completed | in_flight | tombstone` and stored-outcome fields
  derive from §14.9's tombstone/retention language and §15.3's pending
  states; `cursor_recover`'s `recovery_mode: resume | snapshot_restart`
  encodes §14.4's "typed problem plus authorized snapshot recovery
  options". `recovery_checkpoint_show` projects the current
  AuthorityEndpointIncarnation checkpoint fields plus the RestoreLineage
  `idempotency_retention` label (§15.3).
- **G41 — charter family encoding.** The genesis CharterRevision lands
  atomically inside `society_bootstrap` (B0.1 sheet; `charter.json` records
  absent → active as a cascade of the Society machine's owning bootstrap).
  `charter_propose` restates the complete §6.2 record against the exact
  current revision (`previous_digest` pin); adoption is only by the current
  charter's exact amendment rule, and the prior active revision moves to
  superseded in the same finalize (§14.8 "active → superseded by new active
  revision"). Rule bodies are G10 opaque objects. `charter_position`
  carries no assent-mode fields: R6 seats are human/governance seats and a
  human-authority requirement is never satisfied by an agent position
  (§10.3). `charter_history` is an R4 projection read with G38 paging.
- **G42 — closed update/create metas (RT-01).** Every mutation is
  registry-classed `create` or `update` (`spec/registry.json`). An
  update-classed op's embedded `mutationMeta` REQUIRES
  `expected_revision` (the current-head CAS is structural — §14.2
  "Updates require the last observed revision"); a create-classed op's
  meta has NO `expected_revision` member at all, so supplying one fails
  the closed schema. The runner derives the per-op class from the
  registry and checks every request schema; the
  `*-missing-expected-revision-invalid` vectors pin one negative per
  update op.
- **G43 — contextual digest classes (RT-02).** Every digest field is
  bound to its contextual class via a pinned wrapper def
  (`localErasureSafeDigest` / `scopeErasureSafeDigest` /
  `ciphertextPublicDigest` refining the closed family `digestRef`):
  authority subjects and per-object erasable content are
  `local_erasure_safe` (PROFILE.md §6.2 — never a public hash, never a
  scope-keyed digest); idempotency-index and checkpoint/journal-chain
  digests (`result_digest`, the recovery-checkpoint chain fields) are
  `scope_erasure_safe`; the sealed ContinuityRoot state blob is
  `ciphertext_public`. A well-constructed digest of the wrong class is
  `digest_class_mismatch`.
- **G44 — concrete slot/seat records (RT-03, D-RT-3).** `pledge_propose`
  and `pledge_amend` results return `required_slots` as closed records
  `{slot_ref, kind, multiplicity, seat_refs[], subject_digest}` — the
  exact repeatable seats the position stage fills — with at most one
  record per §9.3 kind and `multiplicity == len(seat_refs)`
  (runner-enforced). `pledge_amend` creates a SEPARATE proposed successor
  (`amendment_predecessor_ref`/`_revision` echoed on its result);
  `pledge_finalize` supersedes the predecessor under the successor CAS
  (`meta.expected_revision` + the both-or-neither
  `supersedes_pledge_ref`/`supersedes_pledge_revision` pair).
- **G45 — registry and descriptor format v2 (RT-12, RT-09).**
  `spec/registry.json` is the machine-readable `(operation,surface)`
  registry for the whole bundle (one row per pair; the G35 dual-surface
  ops carry exactly two); the runner derives bundle, meta-class, and MCP
  checks from it and fails on any extra/missing surface binding.
  `spec/descriptors/` is format `byom-descriptor/v2`: every transition
  row carries structured `guards`, `locks`, `fences`, `events`, and
  `crash_result` (§14.8's mandated columns), validated by the runner and
  by `proof/check-descriptors.py`, with the negative mutation suite
  proving neither validator is vacuous. RT-09 closes the columns
  SEMANTICALLY: `spec/descriptors/vocabulary.json` is the frozen v2
  vocabulary — the closed per-column value sets (the distinct §14.8
  column values in use across all committed descriptors) plus the event
  grammar `^[a-z0-9-]+\.[a-z0-9_-]+$` — and the runner rejects any
  descriptor column entry outside it (semantic erasure to arbitrary
  nonempty prose fails closed; extending a column is a change to the
  vocabulary file). For modeled machines, `proof/check-descriptors.py`
  additionally compares the `crash_result` and `fences` columns against
  the model's `@parity crash:` / `@parity fences:` transcriptions both
  ways, so a modeled row's crash/fence semantics cannot be swapped even
  for another legal value.
- **G46 — the B0.4 runtime/reconciliation bundle (B3 slice 2).** The
  §14.6 `runtime` family byom implements daemon-side, plus the two R38
  reconciliation seats, ship as bundle `B0.4` in the same
  `spec/registry.json` (`bundle: "B0.4"`): `placement_admit`,
  `episode_claim`, `episode_start`, `checkpoint_commit`, `episode_yield`,
  `episode_complete`, `episode_fail`, `usage_report`,
  `effect_outcome_admit` (runtime) and `effect_reconcile`,
  `budget_reconcile` (governance). Each publishes its closed
  request/result pair here; the runner's `check_runtime_bundle` pins the
  §14.7 surface, the RT-01 meta class, and — on every protected
  per-attempt command — that BOTH fence members are *required*, so a
  shape that could carry one fence alone cannot be committed (family
  contract L21). Derivations DESIGN.md does not spell out, recorded here:
  - **`activation_admit` / `resource_allocate` stage ids are derived from
    the subject they decide** (`adm-<wake_intent>-r<revision>`,
    `alloc-<wake_intent>-r<revision>`), the `gov_decision` idiom: §11.1
    makes them non-callable kernel transitions, and the frozen
    `episode-request-request` schema requires the caller to name the
    ActivationAdmission, so the id must be one the request can only
    match. Each runs as its OWN §15.3 authority transaction (its own
    idempotency domain and journal entry), which is what lets a crash
    between two stages recover the committed prefix.
  - **A denied admission is committed evidence AND a typed refusal.**
    §11.1's ActivationAdmission has a `denied` state and §14.8 says a
    retry returns the same admission, so the denial row commits and
    `episode_request` then refuses with the reason code's typed problem.
  - **The `byom_subordinate` saga outcome is carried on
    `placement_admit`.** §14.6 defines no byom-side catalog operation for
    the Kovee-owned saga verbs (`subordinate_reserved`/`_denied`/
    `_outcome_unknown`/`_query_*`), and byom holds no outbound Kovee
    client in this slice; the narrow stage-4 adapter therefore reports
    the exact subordinate reservation it created, and byom records it
    under the committed descriptor's guards (never above parent, same
    dimension and unit, idempotent over the stable key).
  - **The per-Episode worst case and the lease window are pinned here.**
    §11.4 fixes no per-Episode reservation and §11.2 no lease TTL: one
    Episode reserves 256 units on the mandate's `budget_ceiling_set_ref`
    (dimension `unit`) inside the mandate's 1024-unit allowance, and
    `lease_ttl_seconds` is bounded 1..86400. An unknown outcome moves the
    hold into the §11.4 `uncertain` bucket, so conservation holds and
    nothing returns to `remaining` without the R38 decision.
  - **`usage_report`'s two arms are separated by CHANNEL, not by a
    flag.** The worker's episode-scoped token may file evidence only; the
    narrow trusted-meter token is the only channel whose report settles
    (family contract L33). Both tokens are byomd-minted from the store
    root over the exact `(episode, generation)` subject and published
    `0600` beside the candidate/participant channel files; mTLS and
    attested workload identity are honestly NOT claimed at the developer
    profile (§11.5), and neither is the `fresh_challenge_ref` the two R38
    seats carry.
  - **`effect_outcome_admit` binds the exact Episode/ByomEpisodeBinding,
    not an ActIntent row.** The `act_intent_*` family lands with its own
    bundle, so `intent_ref` + `stable_execution_key` are the opaque
    stable pair the §13.1 heads are unique over, gated by the same dual
    fences as every other runtime mutation.
- **G47 — the B0.5 acts / onboarding-compute / attention bundle (B3
  slice 3).** The §13.1 act chain (`act_intent_prepare/position/finalize`,
  `execution_permit_consume`) is a B0.1 sheet family and keeps its frozen
  schemas; the §7.4 onboarding-compute rows, the §12.1 source-field read,
  and the attention intake ship as bundle `B0.5` in the same
  `spec/registry.json` (`bundle: "B0.5"`):
  `onboarding_compute_permit_consume` (R32), `onboarding_episode_claim` and
  `onboarding_episode_complete` (R31), `attention_notice_record` (derived),
  and `context_manifest_show` (R4). The runner's `check_slice3_bundle` pins
  the §14.7 surface, the RT-01 meta class, and — on every onboarding
  mutation — that the OFFER FENCE member is *required*, so a shape that
  could act after a refusal cannot be committed. Derivations DESIGN.md does
  not spell out, recorded here:
  - **`attention_notice_record` is a DERIVED byom-side operation name.**
    §16.4 states that "Kovee Attention may notify the Byom adapter of an
    admitted exact event", but §14.6 lists NO attention operation and §14.7
    binds none — exactly the situation in which `placement_admit` carries
    the Kovee-owned subordinate saga verbs. The narrow runtime row is
    therefore derived, and the record it writes is EVIDENCE with a
    server-computed `eligibility_effect` of `no_effect` or
    `wake_intent_eligible`: a notification may at most make a participant's
    OWN already-submitted WakeIntent eligible under its ALREADY ADOPTED
    ActivationPolicy. The request shape carries no wake, admission,
    allocation, episode, priority, rank or score member, and the result
    pins `created.{wake_intent,activation_admission,resource_allocation,
    episode}` as constant false (family contract L25; runner-enforced).
    The row owns no descriptor: it drives no §14.8 machine transition,
    because it changes no machine's state.
  - **The §12.1 source fields are byom-composed, and `context_manifest_show`
    projects them.** §16.6 item 5 adds the exact §12.1 field list to
    Kovee's ProviderContextManifest; §12.1 names the members in prose, so
    the frozen `provider-context-manifest-byom-fields` fragment is the
    normative member set, and `episode_claim` now composes it from
    committed state and records it — with byom's own
    `context_source_digest` (class `portable_public` over the
    `$domain`-tagged canonical fragment) — inside the
    `ByomEpisodeBinding` record. Two members §12.1 does not give a record
    for are derived: `disclosure_ceiling_ref` is the Mandate's
    `context_ceiling_ref` when set, else `ceiling-<mandate>`; and
    `ordered_source_items` is byom's SOURCE order over the Episode's
    immutable inputs (the ContextManifest pin and the exact wake cause) —
    Kovee owns the final provider-visible ordering and bytes. The
    Episode's ContextManifest is IMMUTABLE: a later claim naming another
    manifest is refused, and the read refuses refs that do not match.
  - **The OnboardingComputeIntent is authorized before Kovee's final bytes
    exist.** §7.4 requires `provider_context_manifest_*`,
    `disclosure_manifest_*` and `model_profile_*` on the intent, but
    §16.6 item 12 routes the call "through Kovee's final
    ProviderContextManifest and model broker" — which only exists at
    consume time. The intent therefore pins the Society-authorized EXACT
    disclosed context and proposed Manifestation, and the RECEIPT carries
    Kovee's final manifest/disclosure/model digests as presented by the
    broker. `provider_binding_ref`, `region` and
    `retention_and_training_claims` are endpoint labels (§7.4 pins
    presence, not a value shape), and `maximum_output_bytes` is pinned at
    65536.
  - **The one-shot keys are kernel-derived, and each narrow channel binds
    the record whose authority it carries.** `stable_compute_key` is
    `occ-<compute_intent>`, `stable_execution_key` is `exec-<intent>`, and
    the onboarding id is `onb-<membership_offer>` — so a request can only
    echo the server value. The permit channel binds the exact ActIntent (so
    an unauthorized act answers `decision_incomplete` and a spent decision
    `stale_revision`, never an opaque forbidden); the broker channel binds
    the compute key; the candidate channel binds the offer AND its fence,
    so `membership_refuse` — which advances that fence and revokes unused
    compute authority in the same CAS — invalidates the workload's own
    credential. mTLS and attested workload identity remain honestly NOT
    claimed at the developer profile (§11.5).
  - **The Δ4 class subject is compiled, and the per-act ceilings are pinned
    here.** `kind` stays the frozen open identifier; a `kind` that IS one of
    the five act classes compiles `act_class_subject = {act_class,
    subject_atoms}` from the dependency closure (§10.6) — never
    caller-shaped — carrying exactly that class's mandatory domains. The
    purpose atom pins a byom-owned snapshot over the Mandate's purpose
    (§13.1 names no snapshot record); `binding` is `kovee:<driver_audience>`
    and is rechecked against the consuming broker; `classification` pins
    the Society's classification binding and the Mandate's first data
    class. A missing mandatory domain fails preparation closed
    (`policy_conflict`). §13.1 fixes no per-act reservation and §13.4 no
    egress ceiling: one act reserves 64 units on the mandate's
    `budget_ceiling_set_ref` and the `model_egress` quantity atom carries
    262144 output bytes.
  - **`act_intent_finalize` implements the authorizing branch only.** The
    committed descriptor also lists `prepared|awaiting_decision|authorized
    -> denied` via `act_intent_finalize`, plus `act_intent_cancel` and the
    `server_time` expiry; this slice implements the `awaiting_decision ->
    authorized` transition and the `authorized -> consumed` consumption.
    The deny/cancel/expire branches remain unimplemented and honestly
    answer `feature_unavailable` (`act_intent_cancel`) — recorded, not
    silently absent.
- **G48 — the cross-boundary digest classes and the published allocation
  pin (live-seam findings S-1/S-2/S-3, `reviews/2026-07-26-seam-findings.md`
  **in the program repo**, not this one).**
  Wiring Kovee's episode pipeline to byomd's real runtime surface found that
  `placement_admit` required a digest byom published nowhere, and that four
  runtime fields carried a class the counterparty could not derive. The
  ratified rule, applied per field:

  > A digest one protocol DEMANDS from the other MUST be `portable_public`
  > (unkeyed SHA-256 over bytes both sides hold), because the counterparty
  > has to derive the same value. A digest the owner recomputes from its OWN
  > committed state keeps `local_erasure_safe` (per-object secret) — and is
  > therefore never asked for on the wire at all.

  Per-field outcome:
  - **`resource_allocation_digest` (`placement_admit`) → `portable_public`,
    and now PUBLISHED.** It is a genuine cross-boundary pin: Kovee asserts
    which allocation revision it placed, so it must stay a request member.
    But `resource_allocations.digest` is byom's own keyed record commitment
    under a per-object secret — Kovee could only echo an opaque blob (which
    is why the live test read it out of `byom.db`). byom therefore computes
    a SECOND, cross-boundary digest at `resource_allocate`:
    `portable_public` SHA-256 over the `$domain`-tagged canonical
    `bpp-resource-allocation-binding-v0` fragment, whose frozen member set
    is `{allocation_id, activation_admission_ref, activity_stream_ref,
    generation, byom_budget_reservation_set_ref,
    byom_budget_reservation_set_revision, external_budget_bridge_ref,
    stable_allocation_key, stable_external_reservation_key,
    reservation_items}` — every member kernel-derived from names the caller
    already supplied, or fixed by §11.4, so a holder of the activation
    notice derives the same bytes. Mutable members (`revision`, `state`) and
    byom-minted internal ids (`mandate_use_refs`) are deliberately out: the
    pin names the allocation's cross-boundary identity, which never changes
    under it. `episode_request`'s result carries it as
    `resource_allocation_digest` beside `resource_allocation_id`, and
    `placement_admit` compares exactly those bytes. byom's own
    `local_erasure_safe` record digest stays where it was and is now
    demanded from nobody. The construction mirrors `context_source_digest`
    (§12.1, gap note G47) exactly.
  - **`claim_subject_digest` (`episode_claim`) → stays
    `local_erasure_safe`, REMOVED from the request.** The claim subject is
    byom's own authority subject over byom's own staged EpisodeAttempt, and
    PROFILE.md §6.2 requires the per-object class for an authority subject.
    byom recomputes it, so under the rule it must not be an input: byom
    derives it inside the claim transaction, tag
    `bpp-episode-claim-subject-v0` over `{episode_ref, generation,
    claim_ordinal, holder_runtime_binding, byom_attempt_ref,
    byom_fence_epoch, kovee_invocation_ref, kovee_invocation_fence,
    stable_binding_key}`, under the attempt's per-object secret (so
    destroying that attempt destroys exactly its verifiability). The closed
    request shape now REFUSES a `claim_subject_digest` member — the
    committed negative vector. This is the field whose class had forced
    per-object erasure secrets on the counterparty for no benefit.
  - **`context_manifest_digest` (`episode_claim`,
    `context_manifest_show` result) → `portable_public`.** The
    ContextManifest is Kovee's object; byom holds only the ref and cannot
    re-derive a keyed digest over content it does not have. It is also
    preimage material for the already-`portable_public`
    `context_source_digest`, so a keyed value there was the D-R1-2 shape
    (a class both sides must derive containing one only the owner can).
    The projection read republishes the same committed value, so its result
    schema moves with it.
  - **`checkpoint_digest` (`checkpoint_commit`) → `portable_public`.** The
    checkpoint is the workload's content. byom records the commitment and
    holds no bytes to re-derive it from, so the class has to be one the
    worker and every later reader can derive.
  - **The two fields already correct stay correct.**
    `kovee_placement_digest` (`placement_admit`) and
    `context_source_digest` (`episode_claim`) were already
    `portable_public` and are unchanged. Every remaining
    `local_erasure_safe` field in the runtime parsers names one of BYOM's
    own records — `intent_digest`, `result_digest`,
    `reconciles_admission_digest`, `basis_source_admission_digest`,
    `classification_admission_digest` — and is recomputable by byom, so the
    class is correct there.
  - **The `ExecutionConsumptionReceipt` (`execution_permit_consume`
    result) — the RECEIPT half of the same rule, found the same way.**
    Wiring Kovee's model broker to byomd's real act chain found the
    receipt rendering **every digest member `null`**: the daemon composed
    the receipt row in memory and read its structured columns back with a
    text-only accessor, so `intent_digest`, `mandate_use_digest`,
    `subject_digest`, `episode_fence_digest` and `digest` all serialized
    as `null` on the mint path (the replay path, which reads the row back
    from SQLite, was unaffected — which is why only the live seam saw it).
    byom's own B3 suite asserted the non-digest members only, so the gap
    was invisible here. The consumer could not verify the binding it is
    required to hold before egress, and recorded the whole set as
    unverifiable. Fixed at the root — the accessor now parses either form,
    and the receipt's published members and its digest preimage are ONE
    composed fragment, so a rendered member is a digested member and a
    `null` is unrepresentable — and the classes are now the ones a
    consumer can actually check:
    - `intent_digest`, `subject_digest`, `disclosure_digest`,
      `episode_fence_digest` **stay `local_erasure_safe`**. Each is an
      ECHO of a value the consumption request itself pinned, rechecked
      against byom's committed record inside the consuming transaction, so
      the consumer verifies it by exact `DigestRef` identity against the
      value it already holds — no re-derivation is needed and none is
      possible. `subject_digest` additionally MUST keep the per-object
      class: PROFILE.md §6.2 requires authority subjects to be
      `local_erasure_safe` and forbids a public hash there, so the
      cross-boundary rule cannot reach it. The receipt now publishes
      byom's COMMITTED row values for the intent, subject and fence
      digests rather than the caller's echo (proven identical by the
      recheck, and honest about whose record it is).
    - `mandate_use_digest` **→ `portable_public`**, over the frozen
      `bpp-mandate-use-binding-v0` fragment `{mandate_use_id, intent_ref,
      use_key, consumed_at}` — every member published on the receipt
      (`mandate_use_ref`, `intent_ref`, `stable_execution_key`,
      `issued_at`). The consumer never supplied a MandateUse and holds no
      per-object secret, so byom's keyed `mandate_uses.digest` could only
      ever be echoed. That record commitment is unchanged, stays keyed,
      and is now demanded from nobody — the `resource_allocation_digest`
      construction exactly. The MandateUse's byom-internal members
      (`mandate_ref`, `mandate_digest`, `use_ordinal`,
      `ceiling_reservation_refs`, `decision_refs`) are deliberately out:
      a member the consumer cannot hold could never be re-derived.
    - `digest` **→ `portable_public`**, over the frozen
      `bpp-execution-consumption-receipt-binding-v0` fragment: EXACTLY the
      §13.1 receipt members except `digest` itself, absent optionals
      absent (never `null`), the transport-only `replayed` marker
      excluded. The consumer derives it from the receipt it just received.
      This is the one digest that authenticates the binding the broker
      relies on, so a keyed value there was the defect the rule exists to
      prevent. byom keeps no keyed twin: nothing inside byom compares this
      value — it exists to cross the boundary. The stored row's host-side
      and fence columns and `society_id` are not receipt members and stay
      out of the preimage, so the fragment is never byom's whole record.
    - The keyed member digests inside those public preimages are
      **published bytes both sides hold**, not values the consumer must
      derive, so `public_hash_over_erasable_content_forbidden` is
      untouched: destroying an object secret still erases exactly that
      member's own verifiability, while the portable pins only ever proved
      that the receipt's bytes are the bytes byom committed.
    - Receipts minted before this revision keep whatever they committed —
      an immutable record is never rewritten, and a stored receipt replays
      byte-for-byte as it was issued. No store column changed.
    - **The request side, now changed (R3-L01, decision D-R3-3).** The
      converse half of the rule says a digest the owner recomputes from
      its own state is never a request member, and the earlier revision
      recorded `execution_permit_consume`'s request as an exception. R3
      raised that to P1 — it is the seam rule the program had just
      ratified — so the request shape moved:
      - **removed:** `intent_digest`, `subject_digest`,
        `episode_fence_digest`. byom recomputes each from its committed
        ActIntent record, act subject and `ByomEpisodeBinding`, and
        publishes the committed value on the receipt. The echo proved only
        that byom's own value equalled itself, while costing the host
        per-object keyed digests it can never verify.
      - **re-classed to `portable_public`:** `host_effect_digest` and
        `disclosure_digest` on the request, `disclosure_digest` on the
        receipt, and `context_manifest_digest`/`disclosure_manifest_digest`
        on `act_intent_prepare`. Each names a HOST object byom cannot
        re-derive; `effect_outcome_admit` already demanded
        `portable_public` for `host_effect_digest`, so the two ends of one
        Effect's life could not previously name the same value.
      - **added:** `host_effect_credential`, the host-effect registration
        authenticator (R3-A02) — `HMAC-SHA-256(permit channel token,
        $domain-tagged canonical `bpp-host-effect-registration-v0`
        {intent_ref, stable_execution_key, host_effect_ref,
        host_effect_digest})`. The permit is bound to ONE exact prepared
        host Effect; without it the effect ref and digest were merely
        stored because a caller sent them, and a live probe consumed for a
        nonexistent effect.
      - the disclosure pair is compared, ref AND digest, against the pair
        the gate seat assented to, and the receipt renders the COMMITTED
        value (R3-A01); a spent key replays only against a frozen
        semantic-request digest over every substantive member (R3-A04).
      - **`host_effect_digest` is now DERIVED, not asserted (R3-L01,
        D-R3-3).** The registration credential authenticates a tuple
        *containing* the value, which proves who sent it and never what it
        is the digest OF, and byom held no host row to compare against. So
        two members were added — `host_effect_external_idempotency_key`
        and `host_effect_request_byte_digest` (the §11.8 typed byte digest
        of the sealed provider-request bytes) — and byom now rebuilds the
        host's whole frozen `kovee-host-effect-binding-v1` fragment and
        refuses a `host_effect_digest` that does not re-derive from it,
        before any consumption state moves. The fragment has exactly nine
        members in the host's published order: `context_digest`,
        `context_manifest_ref`, `disclosure_digest`,
        `disclosure_manifest_ref`, `external_idempotency_key`,
        `final_provider_request_typed_byte_digest`, `host_effect_ref`,
        `intent_ref`, `stable_execution_key`. Six of the nine come from
        byom's OWN committed `act_intents` row — never from the request's
        echo, A8's converse — `host_effect_ref` from the request, and only
        the two genuinely host-owned members from the two new fields. The
        two are tied to each other and to byom's one-shot key:
        `host_effect_external_idempotency_key` must be exactly
        `kovee-model-{stable_execution_key}-{byte_digest.value_hex[0:16]}`.
        A digest that does not agree, or an untied key, is `forbidden`.
        Byom's expectation is pinned against kovee's OWN recorded vector
        (`crates/byomd/tests/vectors/kovee-host-effect-binding.json`), not
        re-derived from byom's code, so the cross-repo pin goes red on
        whichever side moves its domain tag.
      - **the frozen semantic-request digest is STORED, and its field set
        is machine-checked against the wire (R3-A04).** The replay
        comparison reads a stored column, never a recompute: a
        byte-identical request presented against an edited commitment is
        refused, which a recompute could not see. Every wire member is
        classified exactly once as semantic (15 members, in the replay
        preimage under tag `bpp-execution-permit-consume-request-v0`) or as
        transport (`version`, `op`, `meta`, `host_effect_credential` — the
        last because it is a deterministic function of four members already
        in the semantic set). A test reads the published request schema's
        own property list and fails the suite if any wire member is
        neither, or both — the exact regression that let the two new
        `host_effect_*` members change what the digest was OF while still
        replaying `ok`.
      - Vectors: `-request-valid` on the new member set, plus
        `-request-owner-recomputed-digest-invalid` (an echoed
        `subject_digest` fails the closed shape) and
        `-request-keyed-host-effect-digest-invalid` (the class byom used to
        demand). The obsolete `-request-unpaired-episode-fence-invalid`
        vector is retired with the pair it policed. **kovee's client
        mirrors this shape.** The published receipt keeps its frozen §13.1
        member set — it carries neither the context pair nor the two
        host-owned members, and the committed context ref/digest are
        published on the `act-intent.consumed` event instead. What changed
        is byom's own storage: migration V10 adds the
        `semantic_request_digest` column, V11 retains
        `host_effect_external_idempotency_key` and
        `host_effect_request_byte_digest` on the stored row so a refusal
        can name exactly which member moved.
    - Vectors: `execution-permit-consume-result-valid` (every member
      rendered, both pins the real re-derived values — `b3_act_chain`
      recomputes them from the vector), plus the three negatives
      `-null-digest-invalid` (the exact regression),
      `-keyed-receipt-digest-invalid` and
      `-keyed-mandate-use-digest-invalid` (typing-only rejections with
      arithmetically well-formed offered values).
  - **Rule-eligible, NOT re-classed by this decision.** The two optional
    Kovee-owned pairs on `episode_claim` —
    `kovee_context_assembly_digest` and
    `provider_context_manifest_digest` (and the same pair on the §7.4
    onboarding-compute rows) — name Kovee objects byom cannot recompute, so
    the rule would move them too. They are outside the ratified four-field
    decision and are recorded here for the family-contract owner rather
    than changed silently.
  - **The achievable call order (S-3).** `episode_request` → Kovee authors
    the `PlacementBinding` → `placement_admit` → `episode_claim`/
    `episode_start`. `placement_admit` binds the exact ResourceAllocation,
    including the digest above, so it can only run after the
    `episode_request` that creates it;
    `../../governed-work/episode-budget-dispatch.md` now states this order.
    The family contract's L25 row (a LOCKED, digest-pinned artifact) still
    transcribes `PlacementBinding → placement_admit → episode_request`;
    correcting it is an owner-side amendment plus a new lock row, not a
    byom edit — recorded here so the discrepancy is not silent.
- **G49 — the locked position snapshot (R3-A03).** §13.1 requires
  finalization to authorize against "the finalized full seat snapshot" and
  §14.8 closes the ActIntent machine, but neither names a record for WHICH
  Position revision each seat held. Derived, and committed here:
  - The `GovernanceDecision` carries `position_locks` beside
    `position_refs` — one closed `{position_ref, position_revision,
    position_digest}` per seat, where `position_digest` is byom's own keyed
    record commitment (`local_erasure_safe`; PROFILE.md §6.2 requires the
    per-object class for an authority subject, so the A8 cross-boundary
    rule cannot reach it). The locks are inside the decision's own
    `bpp-governance-decision-v0` record digest, so an edited lock stops the
    decision re-deriving its own digest and the citing operation answers
    `decision_incomplete` — the negative is a test that rewrites the stored
    column, not a claim about the preimage.
  - `act_intent_finalize`'s result projects the same snapshot as
    `authorization_slot_snapshot` plus its
    `authorization_slot_snapshot_digest` (tag `bpp-act-slot-snapshot-v0`),
    on the authorizing branch only.
  - **Supersession** is refused at consumption, not at finalization: a
    PositionRevision is immutable, so a later revision for the same seat is
    an append. `execution_permit_consume` recomputes the snapshot digest
    from the currently active positions and refuses `stale_binding` when it
    no longer equals the one the authorization locked.
  - **A stale binding epoch** invalidates the position itself, at both
    finalization and consumption: the required seats then hold no exact
    current Position revision and the answer is `decision_incomplete`, with
    the detail naming the epoch that moved. A rebound principal therefore
    cannot carry an old seat forward.
  - The same lock shape is written by the endeavor-formation path, so a
    formation decision and an act decision cite seats the same way.
