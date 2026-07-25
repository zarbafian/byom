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
- **G3 — timestamp encoding.** DESIGN.md fixes no wire encoding for
  `created_at`/`expires_at`/`effective_at`; these schemas use RFC 3339 UTC
  with a `Z` offset.
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
  referencing selector definitions. The encoding has since landed —
  `../bpa1-policy.schema.json`, ADR-0001 accepted — but published schemas
  are immutable, so these bodies stay open objects until their next schema
  version binds them to `bpa1-policy` at the registry freeze (ADR-0001
  open item).
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
  registry freeze (G12 precedent). The same row lists "amend" against
  → superseded: the descriptor records `pledge_amend` as the driving
  operation, with supersession committing at the amendment's acceptance
  (one CAS successor slot).
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
  definitions; `manifestation_selector` is an opaque BPA-1 body pending
  ADR-0001 — now concretely `../bpa1-policy.schema.json` (ADR-0001
  accepted), bound at the next schema version per G10's landing note. The
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
  the field-complete PreparationTrace verbatim (§10.5, R19 fence) plus the
  derived `required_seat_refs`; `kind` and `preconditions` items stay open
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
  intent/host-effect/subject/disclosure/budget/driver-audience bindings, and
  both current fences — `byom_fence_epoch` + `host_fence_epoch` are the
  derived dual-fence names (R34; G24 precedent). The result is the verbatim
  ExecutionConsumptionReceipt with `max_uses` pinned const 1. Same canonical
  request and key → same receipt; a changed request →
  `idempotency_mismatch`; a different key cannot consume the spent one-shot
  decision (→ `stale_revision`) — vectors.
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
