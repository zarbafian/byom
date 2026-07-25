# Operation schemas — B0.1 society + participants/candidates slice

One closed request and one closed result schema per operation
(`<op>-request.schema.json`, `<op>-result.schema.json`; the result schema is
the `Success.result` payload). Field names are verbatim from the DESIGN.md
record shapes (§6.1, §7.1–§7.4); surface, actor, and closure come from the
family contract's registry bindings R2/R3/R4/R5/R8/R10/R11/R12/R13
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
  referencing selector definitions.
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
