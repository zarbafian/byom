//! Schema DDL, numbered `user_version` migrations, and `meta` key/value
//! helpers — kovee's store pattern (DDL and version bump in one
//! transaction), plain SQLite WAL for the byom personal profile (B1:
//! same-UID data, developer assurance profile, honestly labeled).

use rusqlite::Connection;

/// Version 1: the B1 slice-1 core tables (DESIGN.md §6–§7 records plus
/// the §15.3 authority journal projection).
///
/// Recorded shape notes:
/// - `events` are dense per Society: `UNIQUE(society_id, sequence)` with
///   the allocation head on the `societies` row; sequences are allocated
///   at journal finalize so aborted/abandoned transactions consume none.
/// - `payload_secret` is VESTIGIAL from V6 on (BY-D2): it used to hold a
///   RAW copy of the per-event `local_erasure_safe` secret, which meant
///   destroying that secret erased nothing. The one retained copy is now
///   the wrapped `object_secrets` row; this column is written empty and
///   swept on destruction.
/// - `idempotency_records` are keyed by the ratified idempotency-domain
///   digest (PROFILE §5): the domain already covers actor binding,
///   operation, incarnation, Society, epoch, and key.
/// - `authority_pending` is the §15.3 AuthorityMutationPending row; its
///   `payload` carries the full invisible transition (result, effects,
///   events) that journal finalize materializes exactly.
/// - `participant_channels` joins the sheet's `candidate_channels`: the
///   participant credential minted when admission converts the candidate
///   channel (§7.4).
const V1: &str = r#"
CREATE TABLE meta (
    key   TEXT PRIMARY KEY,
    value BLOB NOT NULL
) STRICT;

CREATE TABLE audit (
    seq       INTEGER PRIMARY KEY AUTOINCREMENT,
    ts        INTEGER NOT NULL,
    event     TEXT NOT NULL,
    detail    TEXT NOT NULL,
    prev_hash BLOB NOT NULL,
    hash      BLOB NOT NULL
) STRICT;

CREATE TABLE societies (
    society_id                     TEXT PRIMARY KEY,
    revision                       INTEGER NOT NULL,
    state                          TEXT NOT NULL,
    home_authority_ref             TEXT NOT NULL,
    kovee_realm_binding            TEXT,
    kovee_project_binding          TEXT,
    charter_head_ref               TEXT NOT NULL,
    charter_head_digest            TEXT NOT NULL,
    classification_binding_ref     TEXT NOT NULL,
    classification_binding_digest  TEXT NOT NULL,
    root_budget_account_set_ref    TEXT NOT NULL,
    recovery_epoch                 INTEGER NOT NULL,
    created_at                     TEXT NOT NULL,
    -- The society_prepare bootstrap subject: prepared subject JSON,
    -- PreparationTrace, per-object digest secrets, seat set, expiry.
    preparation                    TEXT NOT NULL,
    genesis_event_ref              TEXT,
    next_event_sequence            INTEGER NOT NULL
) STRICT;

CREATE TABLE charter_revisions (
    charter_revision_id     TEXT PRIMARY KEY,
    society_id              TEXT NOT NULL,
    revision                INTEGER NOT NULL,
    body_ref                TEXT NOT NULL,
    body_digest             TEXT NOT NULL,
    state                   TEXT NOT NULL,
    adopted_by_decision_ref TEXT,
    created_at              TEXT NOT NULL
) STRICT;

CREATE TABLE participants (
    participant_id      TEXT PRIMARY KEY,
    society_id          TEXT NOT NULL,
    kind                TEXT NOT NULL,
    revision            INTEGER NOT NULL,
    binding_epoch       INTEGER NOT NULL,
    display_profile_ref TEXT NOT NULL,
    standing_ref        TEXT,
    state               TEXT NOT NULL,
    created_at          TEXT NOT NULL
) STRICT;

CREATE TABLE manifestation_revisions (
    manifestation_id        TEXT PRIMARY KEY,
    society_id              TEXT NOT NULL,
    participant_ref         TEXT NOT NULL,
    revision                INTEGER NOT NULL,
    kind                    TEXT NOT NULL,
    body_digest             TEXT NOT NULL,
    status                  TEXT NOT NULL,
    admitted_by_decision_ref TEXT,
    created_at              TEXT NOT NULL
) STRICT;

CREATE TABLE standing_revisions (
    standing_id     TEXT PRIMARY KEY,
    society_id      TEXT NOT NULL,
    participant_ref TEXT NOT NULL,
    revision        INTEGER NOT NULL,
    status          TEXT NOT NULL,
    offer_ref       TEXT,
    acceptance_ref  TEXT,
    decision_ref    TEXT NOT NULL,
    created_at      TEXT NOT NULL
) STRICT;

CREATE TABLE membership_offers (
    offer_id                  TEXT PRIMARY KEY,
    society_id                TEXT NOT NULL,
    participant_ref           TEXT NOT NULL,
    proposed_standing_ref     TEXT NOT NULL,
    subject_digest            TEXT NOT NULL,
    offered_by_decision_ref   TEXT NOT NULL,
    expires_at                TEXT NOT NULL,
    state                     TEXT NOT NULL,
    revision                  INTEGER NOT NULL,
    fence_epoch               INTEGER NOT NULL,
    acceptance_id             TEXT,
    accepted_at               TEXT,
    refusal_id                TEXT,
    refused_at                TEXT,
    superseded_acceptance_ref TEXT,
    refusal_reason_ref        TEXT,
    created_at                TEXT NOT NULL
) STRICT;

CREATE TABLE candidate_channels (
    channel_id TEXT PRIMARY KEY,
    society_id TEXT NOT NULL,
    offer_ref  TEXT NOT NULL UNIQUE,
    token      TEXT NOT NULL,
    token_path TEXT NOT NULL,
    state      TEXT NOT NULL,
    created_at TEXT NOT NULL,
    closed_at  TEXT
) STRICT;

CREATE TABLE participant_channels (
    channel_id      TEXT PRIMARY KEY,
    society_id      TEXT NOT NULL,
    participant_ref TEXT NOT NULL UNIQUE,
    token           TEXT NOT NULL,
    token_path      TEXT NOT NULL,
    state           TEXT NOT NULL,
    created_at      TEXT NOT NULL,
    closed_at       TEXT
) STRICT;

CREATE TABLE events (
    event_id             TEXT PRIMARY KEY,
    society_id           TEXT NOT NULL,
    sequence             INTEGER NOT NULL,
    kind                 TEXT NOT NULL,
    object_ref           TEXT NOT NULL,
    object_revision      INTEGER NOT NULL,
    participant_ref      TEXT,
    actor_ref            TEXT NOT NULL,
    causation_ref        TEXT NOT NULL,
    correlation_ref      TEXT NOT NULL,
    payload              TEXT NOT NULL,
    payload_digest       TEXT NOT NULL,
    payload_secret       TEXT NOT NULL,
    visibility_scope_ref TEXT NOT NULL,
    occurred_at          TEXT NOT NULL,
    UNIQUE(society_id, sequence)
) STRICT;

CREATE TABLE idempotency_records (
    domain_digest  TEXT PRIMARY KEY,
    society_id     TEXT NOT NULL,
    operation      TEXT NOT NULL,
    request_digest TEXT NOT NULL,
    result         BLOB NOT NULL,
    created_at     TEXT NOT NULL
) STRICT;

CREATE TABLE outbox (
    delivery_id TEXT PRIMARY KEY,
    kind        TEXT NOT NULL,
    payload     TEXT NOT NULL,
    created_at  TEXT NOT NULL
) STRICT;

CREATE TABLE authority_pending (
    transaction_id           TEXT PRIMARY KEY,
    endpoint_incarnation     TEXT NOT NULL,
    society_id               TEXT NOT NULL,
    operation                TEXT NOT NULL,
    actor_binding_digest     TEXT NOT NULL,
    idempotency_domain_digest TEXT NOT NULL,
    prior_journal_generation INTEGER NOT NULL,
    proposed_generation      INTEGER NOT NULL,
    transition_digest        TEXT NOT NULL,
    state                    TEXT NOT NULL,
    payload                  TEXT NOT NULL,
    created_at               TEXT NOT NULL
) STRICT;
"#;

/// Version 2: the B1 slice-2 domain tables — self-policies, the mandate
/// chain, endeavors/calls/pledges (RT-03 slot records; D-RT-3 successor
/// slot), deliveries/reviews (with the §20.1 classification-honesty
/// column), activity streams / wake intents / continuations, the §11.4
/// budget ledger, and the §15.4 privacy-access chain.
///
/// Recorded shape notes:
/// - `positions` keep one row per authored position; the current seat
///   head is the single `status = 'active'` row per
///   `(proposal_kind, proposal_ref, seat_ref)` (RT-03 seat-head CAS —
///   superseding writes the old head to `superseded` in the same
///   transition).
/// - `wake_intents` deliberately carry NO SQL UNIQUE on
///   `(participant_ref, stable_wake_key)`: journal effects apply as
///   full-row upserts (`INSERT OR REPLACE`), under which a UNIQUE
///   constraint would silently DELETE the conflicting row instead of
///   refusing. The §11.1 uniqueness rule is enforced in the prepare
///   closure, where the refusal is typed.
/// - `budget_accounts` hold the §11.4 conservation ledger per
///   `(account_ref, dimension)`:
///   `ceiling = remaining + reserved + committed + uncertain + delegated_to_children`.
/// - `privacy_access_records` chain per Society (PROFILE §7): the whole
///   record JSON (including its `record_digest`) lives in `record`;
///   `record_digest_hex` is denormalized for chain verification.
/// - `charter_revisions` gains `effective_at` (nullable for the V1 rows;
///   the genesis adoption backfills at read time from `created_at`).
const V2: &str = r#"
ALTER TABLE charter_revisions ADD COLUMN effective_at TEXT;

CREATE TABLE self_policies (
    policy_id           TEXT PRIMARY KEY,
    society_id          TEXT NOT NULL,
    participant_ref     TEXT NOT NULL,
    kind                TEXT NOT NULL,
    revision            INTEGER NOT NULL,
    status              TEXT NOT NULL,
    body                TEXT NOT NULL,
    body_digest         TEXT NOT NULL,
    adoption_mode       TEXT NOT NULL,
    provenance          TEXT NOT NULL,
    previous_policy_ref TEXT,
    effective_at        TEXT NOT NULL,
    expires_at          TEXT NOT NULL,
    created_at          TEXT NOT NULL
) STRICT;

CREATE TABLE candidate_policy_proposals (
    proposal_id                 TEXT PRIMARY KEY,
    society_id                  TEXT NOT NULL,
    offer_ref                   TEXT NOT NULL,
    participant_ref             TEXT NOT NULL,
    kind                        TEXT NOT NULL,
    state                       TEXT NOT NULL,
    body                        TEXT NOT NULL,
    body_digest                 TEXT NOT NULL,
    adoption_mode               TEXT NOT NULL,
    adoption_control_domain_ref TEXT NOT NULL,
    activated_policy_ref        TEXT,
    created_at                  TEXT NOT NULL
) STRICT;

CREATE TABLE continuity_roots (
    root_id         TEXT PRIMARY KEY,
    society_id      TEXT NOT NULL,
    participant_ref TEXT NOT NULL,
    revision        INTEGER NOT NULL,
    status          TEXT NOT NULL,
    body            TEXT NOT NULL,
    created_at      TEXT NOT NULL
) STRICT;

CREATE TABLE mandates (
    mandate_id             TEXT PRIMARY KEY,
    society_id             TEXT NOT NULL,
    revision               INTEGER NOT NULL,
    state                  TEXT NOT NULL,
    grantee_participant_ref TEXT NOT NULL,
    issuer_ref             TEXT NOT NULL,
    purpose_ref            TEXT NOT NULL,
    allowed_operations     TEXT NOT NULL,
    resource_selectors     TEXT NOT NULL,
    data_class_selectors   TEXT NOT NULL,
    destination_selectors  TEXT NOT NULL,
    context_ceiling_ref    TEXT,
    budget_ceiling_set_ref TEXT NOT NULL,
    concurrency_ceiling    INTEGER NOT NULL,
    manifestation_selector TEXT,
    delegation             TEXT NOT NULL,
    pledge_ref             TEXT,
    parent_mandate_ref     TEXT,
    subject_digest         TEXT NOT NULL,
    required_seat_refs     TEXT NOT NULL,
    preparation_trace      TEXT NOT NULL,
    dependency_set_ref     TEXT NOT NULL,
    decision_refs          TEXT,
    issued_at              TEXT,
    held_by_decision_ref   TEXT,
    revoked_by_decision_ref TEXT,
    expires_at             TEXT NOT NULL,
    created_at             TEXT NOT NULL
) STRICT;

CREATE TABLE positions (
    position_id               TEXT PRIMARY KEY,
    society_id                TEXT NOT NULL,
    proposal_kind             TEXT NOT NULL,
    proposal_ref              TEXT NOT NULL,
    proposal_revision         INTEGER NOT NULL,
    seat_ref                  TEXT NOT NULL,
    participant_ref           TEXT NOT NULL,
    actor_ref                 TEXT NOT NULL,
    value                     TEXT NOT NULL,
    status                    TEXT NOT NULL,
    revision                  INTEGER NOT NULL,
    assent_mode               TEXT,
    derived_assent_receipt_ref TEXT,
    reason_ref                TEXT,
    subject_digest            TEXT NOT NULL,
    digest                    TEXT NOT NULL,
    created_at                TEXT NOT NULL
) STRICT;

CREATE TABLE endeavors (
    endeavor_id             TEXT PRIMARY KEY,
    society_id              TEXT NOT NULL,
    revision                INTEGER NOT NULL,
    state                   TEXT NOT NULL,
    purpose_ref             TEXT NOT NULL,
    purpose_digest          TEXT NOT NULL,
    sponsor_participant_refs TEXT NOT NULL,
    governance_rule_set_ref TEXT NOT NULL,
    outcome_schema_refs     TEXT NOT NULL,
    acceptance_rule_ref     TEXT NOT NULL,
    classification_join_ref TEXT NOT NULL,
    budget_account_set_ref  TEXT NOT NULL,
    deadline                TEXT,
    subject_digest          TEXT NOT NULL,
    required_seats          TEXT NOT NULL,
    preparation_trace       TEXT NOT NULL,
    formation_decision_ref  TEXT,
    created_at              TEXT NOT NULL
) STRICT;

CREATE TABLE calls (
    call_id      TEXT PRIMARY KEY,
    society_id   TEXT NOT NULL,
    endeavor_ref TEXT NOT NULL,
    revision     INTEGER NOT NULL,
    state        TEXT NOT NULL,
    opened_by    TEXT NOT NULL,
    body         TEXT NOT NULL,
    digest       TEXT NOT NULL,
    created_at   TEXT NOT NULL
) STRICT;

CREATE TABLE pledge_proposals (
    proposal_id                    TEXT PRIMARY KEY,
    society_id                     TEXT NOT NULL,
    endeavor_ref                   TEXT NOT NULL,
    call_ref                       TEXT,
    revision                       INTEGER NOT NULL,
    state                          TEXT NOT NULL,
    pledgor_ref                    TEXT NOT NULL,
    beneficiary_ref                TEXT NOT NULL,
    terms                          TEXT NOT NULL,
    terms_digest                   TEXT NOT NULL,
    required_slots                 TEXT NOT NULL,
    preparation_trace              TEXT NOT NULL,
    amendment_predecessor_ref      TEXT,
    amendment_predecessor_revision INTEGER,
    created_at                     TEXT NOT NULL
) STRICT;

CREATE TABLE pledges (
    pledge_id               TEXT PRIMARY KEY,
    society_id              TEXT NOT NULL,
    endeavor_ref            TEXT NOT NULL,
    call_ref                TEXT,
    revision                INTEGER NOT NULL,
    state                   TEXT NOT NULL,
    pledgor_ref             TEXT NOT NULL,
    beneficiary_ref         TEXT NOT NULL,
    terms                   TEXT NOT NULL,
    terms_digest            TEXT NOT NULL,
    source_proposal_ref     TEXT NOT NULL,
    successor_proposal_ref  TEXT,
    superseded_by           TEXT,
    formation_decision_ref  TEXT NOT NULL,
    workstream_ref          TEXT,
    workstream_generation   INTEGER NOT NULL,
    reservation_refs        TEXT NOT NULL,
    created_at              TEXT NOT NULL
) STRICT;

CREATE TABLE deliveries (
    delivery_id         TEXT PRIMARY KEY,
    society_id          TEXT NOT NULL,
    pledge_ref          TEXT NOT NULL,
    pledge_revision     INTEGER NOT NULL,
    state               TEXT NOT NULL,
    terms_digest        TEXT NOT NULL,
    output_refs         TEXT NOT NULL,
    evidence_refs       TEXT NOT NULL,
    activity_stream_ref TEXT NOT NULL,
    subject_digest      TEXT NOT NULL,
    classification      TEXT NOT NULL,
    submitted_by        TEXT NOT NULL,
    submitted_at        TEXT NOT NULL
) STRICT;

CREATE TABLE reviews (
    review_id                 TEXT PRIMARY KEY,
    society_id                TEXT NOT NULL,
    pledge_ref                TEXT NOT NULL,
    pledge_revision           INTEGER NOT NULL,
    delivery_ref              TEXT NOT NULL,
    outcome                   TEXT NOT NULL,
    reviewed_subject_digest   TEXT NOT NULL,
    decision_or_mandate_use_ref TEXT NOT NULL,
    rubric_ref                TEXT,
    rationale_ref             TEXT,
    reviewer_ref              TEXT NOT NULL,
    created_at                TEXT NOT NULL
) STRICT;

CREATE TABLE activity_streams (
    activity_stream_id         TEXT PRIMARY KEY,
    society_id                 TEXT NOT NULL,
    participant_ref            TEXT NOT NULL,
    generation                 INTEGER NOT NULL,
    revision                   INTEGER NOT NULL,
    kind                       TEXT NOT NULL,
    state                      TEXT NOT NULL,
    purpose_ref                TEXT NOT NULL,
    purpose_digest             TEXT NOT NULL,
    pledge_binding             TEXT,
    activation_policy_ref      TEXT,
    mandate_refs               TEXT NOT NULL,
    budget_account_set_ref     TEXT NOT NULL,
    continuation_head_ref      TEXT,
    continuation_head_revision INTEGER NOT NULL,
    created_at                 TEXT NOT NULL
) STRICT;

CREATE TABLE wake_intents (
    wake_intent_id        TEXT PRIMARY KEY,
    society_id            TEXT NOT NULL,
    participant_ref       TEXT NOT NULL,
    activity_stream_ref   TEXT NOT NULL,
    generation            INTEGER NOT NULL,
    revision              INTEGER NOT NULL,
    origin                TEXT NOT NULL,
    activation_policy_ref TEXT,
    exact_cause_ref       TEXT NOT NULL,
    exact_cause_digest    TEXT NOT NULL,
    purpose_ref           TEXT NOT NULL,
    stable_wake_key       TEXT NOT NULL,
    state                 TEXT NOT NULL,
    expires_at            TEXT NOT NULL,
    created_at            TEXT NOT NULL
) STRICT;

CREATE TABLE continuations (
    continuation_id        TEXT PRIMARY KEY,
    society_id             TEXT NOT NULL,
    activity_stream_ref    TEXT NOT NULL,
    generation             INTEGER NOT NULL,
    sequence               INTEGER NOT NULL,
    head_revision          INTEGER NOT NULL,
    summary_ref            TEXT NOT NULL,
    body                   TEXT NOT NULL,
    digest                 TEXT NOT NULL,
    prior_continuation_ref TEXT,
    created_at             TEXT NOT NULL
) STRICT;

CREATE TABLE budget_accounts (
    account_ref           TEXT NOT NULL,
    dimension             TEXT NOT NULL,
    society_id            TEXT NOT NULL,
    ceiling               INTEGER NOT NULL,
    remaining             INTEGER NOT NULL,
    reserved              INTEGER NOT NULL,
    committed             INTEGER NOT NULL,
    uncertain             INTEGER NOT NULL,
    delegated_to_children INTEGER NOT NULL,
    parent_account_ref    TEXT,
    revision              INTEGER NOT NULL,
    created_at            TEXT NOT NULL,
    PRIMARY KEY (account_ref, dimension)
) STRICT;

CREATE TABLE budget_reservations (
    reservation_id TEXT PRIMARY KEY,
    society_id     TEXT NOT NULL,
    account_ref    TEXT NOT NULL,
    dimension      TEXT NOT NULL,
    holder_kind    TEXT NOT NULL,
    holder_ref     TEXT NOT NULL,
    amount         INTEGER NOT NULL,
    state          TEXT NOT NULL,
    created_at     TEXT NOT NULL
) STRICT;

CREATE TABLE charter_proposals (
    charter_proposal_id TEXT PRIMARY KEY,
    society_id          TEXT NOT NULL,
    charter_id          TEXT NOT NULL,
    revision            INTEGER NOT NULL,
    state               TEXT NOT NULL,
    body                TEXT NOT NULL,
    subject_digest      TEXT NOT NULL,
    required_seats      TEXT NOT NULL,
    preparation_trace   TEXT NOT NULL,
    proposed_by         TEXT NOT NULL,
    created_at          TEXT NOT NULL
) STRICT;

CREATE TABLE privacy_access_records (
    society_id               TEXT NOT NULL,
    internal_access_sequence INTEGER NOT NULL,
    record                   TEXT NOT NULL,
    record_digest_hex        TEXT NOT NULL,
    created_at               TEXT NOT NULL,
    PRIMARY KEY (society_id, internal_access_sequence)
) STRICT;
"#;

/// Version 3: the B3 slice-1 host-integration tables (C2
/// `byom_governed_work_v1`, DESIGN.md §16.3; family contract R39/R40/R42).
///
/// Recorded shape notes:
/// - `delegated_credential_consumptions` is the atomic (issuer, nonce)
///   consume of family contract L5–L6: the PRIMARY KEY is the fence, and
///   the row records which internal idempotency domain the consumed
///   attempt executed, so a replayed nonce re-serves the stored result
///   instead of executing a second time.
/// - `external_command_domains` is the ONE terminal row per external
///   IdempotencyDomain and the whole recovery projection: `state` is
///   `committed` (retained signed KoveeEndeavorFormResult) or
///   `tombstoned` (durable non-reexecuting claim). A domain never leaves
///   its terminal state, so `committed` and `tombstoned` cannot both
///   exist — the §16.3 "both cannot win" invariant is a table constraint,
///   not a code path.
/// - `internal_domain_digest` binds the external Kovee-supplied domain
///   digest 1:1 to byom's own server-recomputed IdempotencyDomain
///   (§14.2); a second external digest over the same internal domain (or
///   the reverse) is `domain_conflict`, never a silent rebind.
/// - `governance_decisions` are immutable: formation writes one row and
///   nothing updates it.
/// - `authority_journal_receipts` are the synchronous receipts the
///   `terminalized` result must carry (§16.3).
const V3: &str = r#"
CREATE TABLE delegated_credential_consumptions (
    issuer_ref               TEXT NOT NULL,
    nonce                    TEXT NOT NULL,
    credential_id            TEXT NOT NULL,
    society_id               TEXT NOT NULL,
    operation                TEXT NOT NULL,
    source_principal_ref     TEXT NOT NULL,
    external_domain_digest   TEXT NOT NULL,
    canonical_command_digest TEXT NOT NULL,
    internal_domain_digest   TEXT NOT NULL,
    outcome                  TEXT NOT NULL,
    consumed_at              TEXT NOT NULL,
    PRIMARY KEY (issuer_ref, nonce)
) STRICT;

CREATE TABLE external_command_domains (
    external_domain_digest       TEXT PRIMARY KEY,
    society_id                   TEXT NOT NULL,
    operation                    TEXT NOT NULL,
    endpoint_incarnation         TEXT NOT NULL,
    society_recovery_epoch       INTEGER NOT NULL,
    byom_command_idempotency_key TEXT NOT NULL,
    canonical_command_digest     TEXT NOT NULL,
    kovee_formation_intent_ref   TEXT NOT NULL,
    source_principal_ref         TEXT NOT NULL,
    source_actor_binding_digest  TEXT NOT NULL,
    internal_domain_digest       TEXT NOT NULL,
    state                        TEXT NOT NULL,
    result_envelope              TEXT,
    result_digest                TEXT,
    result_signature             TEXT,
    tombstone_ref                TEXT,
    tombstone_digest             TEXT,
    tombstone_reason             TEXT,
    tombstone_reason_kind        TEXT,
    created_at                   TEXT NOT NULL
) STRICT;

CREATE TABLE governance_decisions (
    decision_id    TEXT PRIMARY KEY,
    society_id     TEXT NOT NULL,
    kind           TEXT NOT NULL,
    subject_kind   TEXT NOT NULL,
    subject_ref    TEXT NOT NULL,
    subject_digest TEXT NOT NULL,
    rule_set_ref   TEXT NOT NULL,
    seat_snapshot  TEXT NOT NULL,
    position_refs  TEXT NOT NULL,
    source         TEXT NOT NULL,
    digest         TEXT NOT NULL,
    created_at     TEXT NOT NULL
) STRICT;

CREATE TABLE authority_journal_receipts (
    receipt_id             TEXT PRIMARY KEY,
    society_id             TEXT NOT NULL,
    operation              TEXT NOT NULL,
    external_domain_digest TEXT NOT NULL,
    prior_generation       INTEGER NOT NULL,
    proposed_generation    INTEGER NOT NULL,
    subject_ref            TEXT NOT NULL,
    digest                 TEXT NOT NULL,
    created_at             TEXT NOT NULL
) STRICT;
"#;

/// Version 4: the R1 review corrections (reviews/2026-07-26-r1-tracer.md).
///
/// Recorded shape notes:
/// - `authority_pending` gains the FULL §15.3 shapes (BY-J2): the prior
///   journal entry digest the CAS compares, the digest of the exact
///   reply bytes, and the verified witness receipt. Every final state,
///   event, outbox and result byte is hashed BEFORE witnessing, so
///   recovery reproduces byte-identical values.
/// - `object_secrets` holds the RANDOM per-object `local_erasure_safe`
///   secret (D-R1-2), wrapped under the Society key. Destroying ONE row
///   destroys exactly that object's verifiability; every other object
///   stays verifiable. Root-derived deterministic per-object keys were
///   the forbidden scope substitution and are gone.
/// - `erasure_journal` is the append-only hash-chained record of secret
///   destruction; its head is checkpointed beside the witness (BY-J3)
///   together with the audit head, so a rolled-back or altered chain
///   seals the endpoint at startup.
/// - `position_revisions` are IMMUTABLE (BY-P1): superseding appends a
///   new revision and never rewrites the prior row. The current seat
///   head is the separate `position_seat_heads` CAS row.
/// - `channel_credentials` replaces the plaintext reusable bearer token
///   (BY-C1): the store keeps a VERIFIER reference (`proof_key_id`) plus
///   the binding a presented proof must commit to — audience, exact
///   scope, allowed operations, Manifestation/control binding, fence
///   epoch, expiry — and `channel_proof_nonces` fences replay.
const V4: &str = r#"
ALTER TABLE authority_pending ADD COLUMN prior_journal_digest TEXT NOT NULL DEFAULT '';
ALTER TABLE authority_pending ADD COLUMN result_digest TEXT NOT NULL DEFAULT '';
ALTER TABLE authority_pending ADD COLUMN receipt TEXT;

ALTER TABLE governance_decisions ADD COLUMN actor_ref TEXT NOT NULL DEFAULT '';
ALTER TABLE governance_decisions ADD COLUMN dependency_closure TEXT NOT NULL DEFAULT '';

CREATE TABLE object_secrets (
    key_ref      TEXT PRIMARY KEY,
    society_id   TEXT NOT NULL,
    tag          TEXT NOT NULL,
    wrapped      TEXT NOT NULL,
    state        TEXT NOT NULL,
    created_at   TEXT NOT NULL,
    destroyed_at TEXT
) STRICT;

CREATE TABLE erasure_journal (
    seq       INTEGER PRIMARY KEY AUTOINCREMENT,
    ts        INTEGER NOT NULL,
    event     TEXT NOT NULL,
    detail    TEXT NOT NULL,
    prev_hash BLOB NOT NULL,
    hash      BLOB NOT NULL
) STRICT;

CREATE TABLE position_revisions (
    position_id                TEXT PRIMARY KEY,
    society_id                 TEXT NOT NULL,
    proposal_kind              TEXT NOT NULL,
    proposal_ref               TEXT NOT NULL,
    proposal_revision          INTEGER NOT NULL,
    seat_ref                   TEXT NOT NULL,
    participant_ref            TEXT NOT NULL,
    participant_binding_epoch  INTEGER NOT NULL,
    actor_ref                  TEXT NOT NULL,
    authentication_observation TEXT NOT NULL,
    endpoint_incarnation       TEXT NOT NULL,
    recovery_epoch             INTEGER NOT NULL,
    value                      TEXT NOT NULL,
    status                     TEXT NOT NULL,
    revision                   INTEGER NOT NULL,
    assent_mode                TEXT,
    derived_assent_receipt_ref TEXT,
    reason_ref                 TEXT,
    subject_digest             TEXT NOT NULL,
    prior_position_digest      TEXT,
    digest                     TEXT NOT NULL,
    created_at                 TEXT NOT NULL
) STRICT;

CREATE TABLE position_seat_heads (
    proposal_kind TEXT NOT NULL,
    proposal_ref  TEXT NOT NULL,
    seat_ref      TEXT NOT NULL,
    society_id    TEXT NOT NULL,
    position_ref  TEXT NOT NULL,
    revision      INTEGER NOT NULL,
    value         TEXT NOT NULL,
    status        TEXT NOT NULL,
    digest        TEXT NOT NULL,
    updated_at    TEXT NOT NULL,
    PRIMARY KEY (proposal_kind, proposal_ref, seat_ref)
) STRICT;

CREATE TABLE channel_credentials (
    channel_id      TEXT PRIMARY KEY,
    society_id      TEXT NOT NULL,
    audience        TEXT NOT NULL,
    scope_ref       TEXT NOT NULL,
    proof_key_id    TEXT NOT NULL,
    key_path        TEXT NOT NULL,
    operations      TEXT NOT NULL,
    binding_ref     TEXT NOT NULL,
    fence_epoch     INTEGER NOT NULL,
    expires_at      TEXT NOT NULL,
    state           TEXT NOT NULL,
    created_at      TEXT NOT NULL,
    closed_at       TEXT
) STRICT;

CREATE TABLE channel_proof_nonces (
    channel_id TEXT NOT NULL,
    nonce      TEXT NOT NULL,
    seen_at    INTEGER NOT NULL,
    PRIMARY KEY (channel_id, nonce)
) STRICT;
"#;

/// Version 5: the terminal closure record of a fenced channel (BY-C2).
///
/// A closed channel replays EXACTLY the refusal that closed it and
/// nothing else: the closing operation and its retained idempotency
/// domain digest are recorded on the channel, so a post-terminal
/// acceptance or self-policy call cannot borrow the replay path.
const V5: &str = r#"
ALTER TABLE candidate_channels ADD COLUMN closed_by_operation TEXT;
ALTER TABLE candidate_channels ADD COLUMN closed_by_domain_digest TEXT;
ALTER TABLE participant_channels ADD COLUMN closed_by_operation TEXT;
ALTER TABLE participant_channels ADD COLUMN closed_by_domain_digest TEXT;
"#;

/// Version 6: the R1 confirmation wave (reviews/2026-07-26-r1-confirmation.md).
///
/// Recorded shape notes:
/// - `channel_bindings` is the one LIVE HOLDER of a channel (BY-C1). The
///   credential file carries no key material any more; a client claims
///   its channel over the surface socket and byomd issues a proof key
///   bound to the connection's kernel-observed `(pid, start time)`. A
///   claim from another peer is refused while the holder is alive, so a
///   COPIED credential file mints nothing.
/// - `events.payload_secret` stops carrying a RAW copy of the per-event
///   `local_erasure_safe` secret (BY-D2): the only retained copy is the
///   wrapped `object_secrets` row, so destroying that row destroys the
///   secret everywhere. Existing rows are blanked by the migration.
const V6: &str = r#"
CREATE TABLE channel_bindings (
    channel_id TEXT PRIMARY KEY,
    peer_pid   INTEGER NOT NULL,
    peer_start INTEGER NOT NULL,
    bound_at   INTEGER NOT NULL
) STRICT;

UPDATE events SET payload_secret = '';
"#;

/// Version 7: the B3 slice-2 runtime tables — the four-stage activation
/// records (§11.1), the Episode/EpisodeAttempt/EpisodeLeaseHead machine
/// (§11.2), the §11.4 external budget bridge with its `byom_subordinate`
/// reservation and settlement heads, the C2 `ByomEpisodeBinding` row,
/// and the §13.2 effect outcome / governance disposition heads.
///
/// Recorded shape notes:
/// - `activation_admissions.admission_id` IS the §11.1
///   `UNIQUE(wake_intent_ref, wake_intent_revision)`: the kernel derives
///   the id from the exact WakeIntent revision it evaluated, so a retry
///   after a crash finds the one committed admission instead of minting
///   a second decision.
/// - `resource_allocations.allocation_id` likewise carries
///   `UNIQUE(activation_admission_ref, stable_allocation_key)`.
/// - `episode_lease_heads` is the ONE head per `(episode_id,
///   generation)` — `episode_id` is the primary key and `generation` is
///   fixed per Episode, so the §11.2 uniqueness is a table constraint.
///   `expires_at_unix` is the CLOCKED deadline minted at claim
///   (`now + lease_ttl_seconds`): reclaim is enabled only after the
///   authoritative server clock passes it (D-RT-6/RT-10,
///   proof/specs/EpisodeLease.tla `NoPrematureExpiry`). `attempt_count`,
///   `expiry_count` and `yield_count` are the audit counters the
///   `ReclaimNeedsExpiryOrYield` invariant is stated over — a crash
///   alone advances none of them.
/// - `byom_episode_bindings.record` holds the FROZEN C2
///   `byom-episode-binding.schema.json` object verbatim; the indexed
///   columns are denormalized copies for the fence and idempotency
///   lookups (`stable_binding_key` is the L22 idempotent-create key).
/// - `external_budget_bridges`/`subordinate_reservations` carry NO SQL
///   UNIQUE on the stable key: journal effects apply as full-row upserts
///   (`INSERT OR REPLACE`), under which UNIQUE would silently DELETE the
///   conflicting row. The ids are DERIVED from the stable key instead,
///   so idempotent create is a primary key.
/// - the four head tables (`usage_settlement_heads`,
///   `effect_outcome_admission_heads`,
///   `effect_governance_disposition_heads`) are compare-and-swap heads
///   keyed exactly as §11.4/§13.1 specify; their revision rows are
///   immutable and append-only.
const V7: &str = r#"
CREATE TABLE activation_admissions (
    admission_id             TEXT PRIMARY KEY,
    society_id               TEXT NOT NULL,
    wake_intent_ref          TEXT NOT NULL,
    wake_intent_revision     INTEGER NOT NULL,
    wake_intent_digest       TEXT NOT NULL,
    activity_stream_ref      TEXT NOT NULL,
    generation               INTEGER NOT NULL,
    participant_ref          TEXT NOT NULL,
    kernel_policy_version    TEXT NOT NULL,
    dependency_set_ref       TEXT NOT NULL,
    dependency_digest        TEXT NOT NULL,
    eligibility_reason_codes TEXT NOT NULL,
    state                    TEXT NOT NULL,
    decided_at               TEXT NOT NULL,
    digest                   TEXT NOT NULL
) STRICT;

CREATE TABLE resource_allocations (
    allocation_id                  TEXT PRIMARY KEY,
    society_id                     TEXT NOT NULL,
    revision                       INTEGER NOT NULL,
    activation_admission_ref       TEXT NOT NULL,
    activity_stream_ref            TEXT NOT NULL,
    generation                     INTEGER NOT NULL,
    participant_ref                TEXT NOT NULL,
    mandate_ref                    TEXT NOT NULL,
    mandate_use_refs               TEXT NOT NULL,
    byom_budget_reservation_set_ref TEXT NOT NULL,
    reservation_items              TEXT NOT NULL,
    external_budget_bridge_ref     TEXT,
    rate_counter_use_refs          TEXT NOT NULL,
    stable_allocation_key          TEXT NOT NULL,
    expires_at                     TEXT NOT NULL,
    state                          TEXT NOT NULL,
    dependency_digest              TEXT NOT NULL,
    digest                         TEXT NOT NULL,
    created_at                     TEXT NOT NULL
) STRICT;

CREATE TABLE external_budget_bridges (
    bridge_id                        TEXT PRIMARY KEY,
    society_id                       TEXT NOT NULL,
    revision                         INTEGER NOT NULL,
    byom_reservation_set_ref         TEXT NOT NULL,
    byom_reservation_set_revision    INTEGER NOT NULL,
    byom_reservation_set_digest      TEXT NOT NULL,
    external_owner                   TEXT NOT NULL,
    external_endpoint_ref            TEXT NOT NULL,
    external_binding_epoch           INTEGER NOT NULL,
    stable_external_reservation_key  TEXT NOT NULL,
    subordinate_reservation_ref      TEXT,
    subordinate_reservation_revision INTEGER,
    subordinate_reservation_digest   TEXT,
    state                            TEXT NOT NULL,
    reconcile_decision_ref           TEXT,
    settled_charge                   INTEGER NOT NULL,
    created_at                       TEXT NOT NULL,
    digest                           TEXT NOT NULL
) STRICT;

CREATE TABLE subordinate_reservations (
    subordinate_reservation_ref     TEXT PRIMARY KEY,
    society_id                      TEXT NOT NULL,
    external_budget_bridge_ref      TEXT NOT NULL,
    stable_external_reservation_key TEXT NOT NULL,
    revision                        INTEGER NOT NULL,
    reservation_class               TEXT NOT NULL,
    record                          TEXT NOT NULL,
    state                           TEXT NOT NULL,
    created_at                      TEXT NOT NULL,
    digest                          TEXT NOT NULL
) STRICT;

CREATE TABLE placement_admissions (
    admission_id               TEXT PRIMARY KEY,
    society_id                 TEXT NOT NULL,
    resource_allocation_ref    TEXT NOT NULL,
    resource_allocation_digest TEXT NOT NULL,
    kovee_placement_ref        TEXT NOT NULL,
    kovee_placement_revision   INTEGER NOT NULL,
    kovee_placement_digest     TEXT NOT NULL,
    source_binding_epoch       INTEGER NOT NULL,
    selected_manifestation_ref TEXT NOT NULL,
    kovee_invocation_ref       TEXT NOT NULL,
    kovee_fence_epoch          INTEGER NOT NULL,
    verification_status        TEXT NOT NULL,
    admitted_at                TEXT NOT NULL,
    digest                     TEXT NOT NULL
) STRICT;

CREATE TABLE episodes (
    episode_id               TEXT PRIMARY KEY,
    society_id               TEXT NOT NULL,
    activity_stream_ref      TEXT NOT NULL,
    generation               INTEGER NOT NULL,
    revision                 INTEGER NOT NULL,
    endpoint_incarnation     TEXT NOT NULL,
    recovery_epoch           INTEGER NOT NULL,
    participant_ref          TEXT NOT NULL,
    manifestation_ref        TEXT,
    mandate_ref              TEXT NOT NULL,
    wake_intent_ref          TEXT NOT NULL,
    activation_admission_ref TEXT NOT NULL,
    resource_allocation_ref  TEXT NOT NULL,
    placement_admission_ref  TEXT,
    wake_cause_ref           TEXT NOT NULL,
    admission_cursor         TEXT NOT NULL,
    context_manifest_ref     TEXT,
    context_manifest_digest  TEXT,
    mandate_use_refs         TEXT NOT NULL,
    budget_reservation_set_ref TEXT NOT NULL,
    deadline                 TEXT,
    deadline_unix            INTEGER,
    state                    TEXT NOT NULL,
    created_at               TEXT NOT NULL,
    terminal_at              TEXT
) STRICT;

CREATE TABLE episode_lease_heads (
    episode_id             TEXT PRIMARY KEY,
    society_id             TEXT NOT NULL,
    generation             INTEGER NOT NULL,
    revision               INTEGER NOT NULL,
    current_attempt_ref    TEXT NOT NULL,
    holder_runtime_binding TEXT NOT NULL,
    byom_fence_epoch       INTEGER NOT NULL,
    renewed_at             TEXT NOT NULL,
    expires_at             TEXT NOT NULL,
    expires_at_unix        INTEGER NOT NULL,
    state                  TEXT NOT NULL,
    last_attempt_event_ref TEXT,
    attempt_count          INTEGER NOT NULL,
    expiry_count           INTEGER NOT NULL,
    yield_count            INTEGER NOT NULL
) STRICT;

CREATE TABLE episode_attempts (
    attempt_id           TEXT PRIMARY KEY,
    society_id           TEXT NOT NULL,
    episode_id           TEXT NOT NULL,
    generation           INTEGER NOT NULL,
    claim_ordinal        INTEGER NOT NULL,
    holder_runtime_binding TEXT NOT NULL,
    manifestation_ref    TEXT,
    byom_fence_epoch     INTEGER NOT NULL,
    acquired_at          TEXT NOT NULL,
    initial_expires_at   TEXT NOT NULL,
    kovee_invocation_ref TEXT,
    kovee_attempt_ref    TEXT,
    kovee_fence_digest   TEXT,
    claim_subject_digest TEXT NOT NULL,
    created_at           TEXT NOT NULL,
    digest               TEXT NOT NULL
) STRICT;

CREATE TABLE episode_attempt_events (
    event_id                TEXT PRIMARY KEY,
    society_id              TEXT NOT NULL,
    episode_id              TEXT NOT NULL,
    attempt_ref             TEXT NOT NULL,
    expected_lease_revision INTEGER NOT NULL,
    byom_fence_epoch        INTEGER NOT NULL,
    kind                    TEXT NOT NULL,
    payload_digest          TEXT NOT NULL,
    occurred_at             TEXT NOT NULL,
    digest                  TEXT NOT NULL
) STRICT;

CREATE TABLE episode_completions (
    completion_id       TEXT PRIMARY KEY,
    society_id          TEXT NOT NULL,
    episode_ref         TEXT NOT NULL,
    attempt_ref         TEXT NOT NULL,
    byom_fence_epoch    INTEGER NOT NULL,
    runtime_binding_ref TEXT NOT NULL,
    output_refs         TEXT NOT NULL,
    evidence_refs       TEXT NOT NULL,
    usage_report_refs   TEXT NOT NULL,
    outcome             TEXT NOT NULL,
    created_at          TEXT NOT NULL,
    digest              TEXT NOT NULL
) STRICT;

CREATE TABLE byom_episode_bindings (
    binding_id             TEXT PRIMARY KEY,
    society_id             TEXT NOT NULL,
    stable_binding_key     TEXT NOT NULL,
    episode_ref            TEXT NOT NULL,
    byom_attempt_ref       TEXT NOT NULL,
    kovee_invocation_ref   TEXT NOT NULL,
    byom_fence_epoch       INTEGER NOT NULL,
    kovee_invocation_fence INTEGER NOT NULL,
    record                 TEXT NOT NULL,
    state                  TEXT NOT NULL,
    created_at             TEXT NOT NULL,
    digest                 TEXT NOT NULL
) STRICT;

CREATE TABLE usage_reports (
    report_id         TEXT PRIMARY KEY,
    society_id        TEXT NOT NULL,
    episode_ref       TEXT NOT NULL,
    attempt_ref       TEXT NOT NULL,
    byom_fence_epoch  INTEGER NOT NULL,
    source            TEXT NOT NULL,
    stable_report_key TEXT NOT NULL,
    quantities        TEXT NOT NULL,
    settlement_ref    TEXT,
    created_at        TEXT NOT NULL,
    digest            TEXT NOT NULL
) STRICT;

CREATE TABLE usage_settlements (
    settlement_id              TEXT PRIMARY KEY,
    society_id                 TEXT NOT NULL,
    revision                   INTEGER NOT NULL,
    previous_settlement_digest TEXT,
    stable_settlement_key      TEXT NOT NULL,
    reservation_set_ref        TEXT NOT NULL,
    meter_ref                  TEXT NOT NULL,
    meter_attestation_ref      TEXT NOT NULL,
    pricing_revision_ref       TEXT,
    measured_quantities        TEXT NOT NULL,
    charged_quantities         TEXT NOT NULL,
    status                     TEXT NOT NULL,
    created_at                 TEXT NOT NULL,
    digest                     TEXT NOT NULL
) STRICT;

CREATE TABLE usage_settlement_heads (
    reservation_set_ref        TEXT NOT NULL,
    stable_settlement_key      TEXT NOT NULL,
    society_id                 TEXT NOT NULL,
    current_settlement_ref     TEXT NOT NULL,
    current_settlement_revision INTEGER NOT NULL,
    current_settlement_digest  TEXT NOT NULL,
    revision                   INTEGER NOT NULL,
    updated_at                 TEXT NOT NULL,
    PRIMARY KEY (reservation_set_ref, stable_settlement_key)
) STRICT;

CREATE TABLE effect_outcome_admissions (
    admission_id                 TEXT PRIMARY KEY,
    society_id                   TEXT NOT NULL,
    revision                     INTEGER NOT NULL,
    previous_admission_digest    TEXT,
    intent_ref                   TEXT NOT NULL,
    intent_digest                TEXT NOT NULL,
    stable_execution_key         TEXT NOT NULL,
    episode_ref                  TEXT NOT NULL,
    host_protocol                TEXT NOT NULL,
    host_endpoint_ref            TEXT NOT NULL,
    host_effect_ref              TEXT NOT NULL,
    host_effect_digest           TEXT NOT NULL,
    host_receipt_ref             TEXT NOT NULL,
    host_receipt_digest          TEXT NOT NULL,
    host_cursor_or_signature_ref TEXT NOT NULL,
    verification_status          TEXT NOT NULL,
    outcome                      TEXT NOT NULL,
    result_ref                   TEXT,
    result_digest                TEXT,
    usage_settlement_ref         TEXT,
    reconciles_admission_ref     TEXT,
    reconciles_admission_digest  TEXT,
    admitted_by_service          TEXT NOT NULL,
    admitted_at                  TEXT NOT NULL,
    digest                       TEXT NOT NULL
) STRICT;

CREATE TABLE effect_outcome_admission_heads (
    intent_ref                 TEXT NOT NULL,
    stable_execution_key       TEXT NOT NULL,
    society_id                 TEXT NOT NULL,
    current_admission_ref      TEXT NOT NULL,
    current_admission_revision INTEGER NOT NULL,
    current_admission_digest   TEXT NOT NULL,
    current_outcome            TEXT NOT NULL,
    revision                   INTEGER NOT NULL,
    updated_at                 TEXT NOT NULL,
    PRIMARY KEY (intent_ref, stable_execution_key)
) STRICT;

CREATE TABLE effect_governance_dispositions (
    disposition_id                 TEXT PRIMARY KEY,
    society_id                     TEXT NOT NULL,
    revision                       INTEGER NOT NULL,
    previous_disposition_ref       TEXT,
    previous_disposition_revision  INTEGER,
    previous_disposition_digest    TEXT,
    intent_ref                     TEXT NOT NULL,
    intent_digest                  TEXT NOT NULL,
    stable_execution_key           TEXT NOT NULL,
    phase                          TEXT NOT NULL,
    basis_source_admission_ref     TEXT NOT NULL,
    basis_source_admission_revision INTEGER NOT NULL,
    basis_source_admission_digest  TEXT NOT NULL,
    basis_source_outcome           TEXT NOT NULL,
    governance_decision_ref        TEXT NOT NULL,
    governance_decision_digest     TEXT NOT NULL,
    local_outcome                  TEXT NOT NULL,
    result_use                     TEXT NOT NULL,
    classification_admission_ref   TEXT,
    classification_admission_digest TEXT,
    late_source_policy             TEXT,
    created_at                     TEXT NOT NULL,
    digest                         TEXT NOT NULL
) STRICT;

CREATE TABLE effect_governance_disposition_heads (
    intent_ref                    TEXT NOT NULL,
    stable_execution_key          TEXT NOT NULL,
    society_id                    TEXT NOT NULL,
    current_disposition_ref       TEXT NOT NULL,
    current_disposition_revision  INTEGER NOT NULL,
    current_disposition_digest    TEXT NOT NULL,
    state                         TEXT NOT NULL,
    revision                      INTEGER NOT NULL,
    updated_at                    TEXT NOT NULL,
    PRIMARY KEY (intent_ref, stable_execution_key)
) STRICT;
"#;

/// V8 — the B3 slice-3 tables (DESIGN.md §7.4, §11.1, §12.1, §13.1):
///
/// - `attention_notices` — the Kovee attention intake. A notice is
///   EVIDENCE, never authority (§11.1/§16.4, family contract L25): the
///   only field it may carry is the server-computed `eligibility_effect`,
///   and no wake, admission, allocation, or Episode is ever written by it.
/// - `onboarding_offers`, `onboarding_compute_intents`,
///   `onboarding_compute_receipts`, `onboarding_episodes` — the §7.4
///   bounded onboarding path and its ONE-SHOT hosted compute
///   (`max_uses = 1` by construction). Completion is evidence only; no
///   MembershipAcceptance and no Standing come from it.
/// - `act_intents`, `mandate_uses`, `execution_consumption_receipts` —
///   the §13.1 intent-before-effect chain. The MandateUse uniqueness pair
///   `(mandate_ref, use_key)` / `(mandate_ref, use_ordinal)` is expressed
///   as real constraints; the receipt's `max_uses` is 1 by construction.
const V8: &str = r#"
CREATE TABLE attention_notices (
    notice_id                TEXT PRIMARY KEY,
    society_id               TEXT NOT NULL,
    source_protocol          TEXT NOT NULL,
    source_endpoint_ref      TEXT NOT NULL,
    source_event_ref         TEXT NOT NULL,
    source_event_digest      TEXT NOT NULL,
    activity_stream_ref      TEXT NOT NULL,
    generation               INTEGER NOT NULL,
    participant_ref          TEXT NOT NULL,
    stable_notice_key        TEXT NOT NULL,
    eligibility_effect       TEXT NOT NULL,
    eligible_wake_intent_ref TEXT,
    activation_policy_ref    TEXT,
    received_at              TEXT NOT NULL,
    digest                   TEXT NOT NULL
) STRICT;

CREATE TABLE onboarding_offers (
    onboarding_id                      TEXT PRIMARY KEY,
    society_id                         TEXT NOT NULL,
    membership_offer_ref               TEXT NOT NULL UNIQUE,
    candidate_participant_ref          TEXT NOT NULL,
    proposed_manifestation_ref         TEXT NOT NULL,
    proposed_manifestation_digest      TEXT NOT NULL,
    exact_context_ref                  TEXT NOT NULL,
    exact_context_digest               TEXT NOT NULL,
    resource_reservation_ref           TEXT NOT NULL,
    max_episodes                       INTEGER NOT NULL,
    allowed_operations                 TEXT NOT NULL,
    onboarding_compute_intent_ref      TEXT,
    general_effect_and_child_authority TEXT NOT NULL,
    fence_epoch                        INTEGER NOT NULL,
    expires_at                         TEXT NOT NULL,
    adopted_by_decision_ref            TEXT NOT NULL,
    state                              TEXT NOT NULL,
    revision                           INTEGER NOT NULL,
    created_at                         TEXT NOT NULL,
    digest                             TEXT NOT NULL
) STRICT;

CREATE TABLE onboarding_compute_intents (
    compute_intent_id     TEXT PRIMARY KEY,
    society_id            TEXT NOT NULL,
    onboarding_ref        TEXT NOT NULL,
    record                TEXT NOT NULL,
    candidate_fence_epoch INTEGER NOT NULL,
    stable_compute_key    TEXT NOT NULL UNIQUE,
    state                 TEXT NOT NULL,
    receipt_ref           TEXT,
    expires_at            TEXT NOT NULL,
    created_at            TEXT NOT NULL,
    digest                TEXT NOT NULL
) STRICT;

CREATE TABLE onboarding_compute_receipts (
    receipt_id            TEXT PRIMARY KEY,
    society_id            TEXT NOT NULL,
    compute_intent_ref    TEXT NOT NULL UNIQUE,
    stable_compute_key    TEXT NOT NULL,
    record                TEXT NOT NULL,
    max_uses              INTEGER NOT NULL,
    candidate_fence_epoch INTEGER NOT NULL,
    kovee_invocation_ref  TEXT NOT NULL,
    issued_at             TEXT NOT NULL,
    expires_at            TEXT NOT NULL,
    digest                TEXT NOT NULL
) STRICT;

CREATE TABLE onboarding_episodes (
    onboarding_episode_id      TEXT PRIMARY KEY,
    society_id                 TEXT NOT NULL,
    onboarding_ref             TEXT NOT NULL,
    candidate_participant_ref  TEXT NOT NULL,
    proposed_manifestation_ref TEXT NOT NULL,
    compute_receipt_ref        TEXT,
    onboarding_fence_epoch     INTEGER NOT NULL,
    holder_runtime_binding     TEXT NOT NULL,
    stable_claim_key           TEXT NOT NULL UNIQUE,
    revision                   INTEGER NOT NULL,
    state                      TEXT NOT NULL,
    outcome                    TEXT,
    output_refs                TEXT NOT NULL,
    evidence_refs              TEXT NOT NULL,
    acceptance_effect          TEXT NOT NULL,
    claimed_at                 TEXT NOT NULL,
    completed_at               TEXT,
    digest                     TEXT NOT NULL
) STRICT;

CREATE TABLE act_intents (
    intent_id                          TEXT PRIMARY KEY,
    society_id                         TEXT NOT NULL,
    revision                           INTEGER NOT NULL,
    endpoint_incarnation               TEXT NOT NULL,
    recovery_epoch                     INTEGER NOT NULL,
    requested_by_participant           TEXT NOT NULL,
    actor_ref                          TEXT NOT NULL,
    endeavor_ref                       TEXT,
    pledge_ref                         TEXT,
    preparation_trace_ref              TEXT NOT NULL,
    preparation_trace_digest           TEXT NOT NULL,
    preparation_trace                  TEXT NOT NULL,
    kind                               TEXT NOT NULL,
    act_class                          TEXT,
    act_class_subject                  TEXT,
    execution_kind                     TEXT NOT NULL,
    subject_ref                        TEXT NOT NULL,
    subject_revision                   INTEGER NOT NULL,
    subject_digest                     TEXT NOT NULL,
    intent_digest                      TEXT NOT NULL,
    preconditions                      TEXT NOT NULL,
    context_manifest_ref               TEXT,
    context_manifest_digest            TEXT,
    disclosure_manifest_ref            TEXT,
    disclosure_manifest_digest         TEXT,
    driver_audience                    TEXT,
    budget_reservation_set_ref         TEXT NOT NULL,
    mandate_ref                        TEXT NOT NULL,
    mandate_revision                   INTEGER NOT NULL,
    mandate_digest                     TEXT NOT NULL,
    authorization_dependency_set_ref   TEXT NOT NULL,
    dependency_digest                  TEXT NOT NULL,
    authorization_decision_ref         TEXT,
    authorization_slot_snapshot_digest TEXT,
    required_seat_refs                 TEXT NOT NULL,
    stable_execution_key               TEXT NOT NULL UNIQUE,
    expires_at                         TEXT NOT NULL,
    state                              TEXT NOT NULL,
    created_at                         TEXT NOT NULL
) STRICT;

CREATE TABLE mandate_uses (
    mandate_use_id           TEXT PRIMARY KEY,
    society_id               TEXT NOT NULL,
    mandate_ref              TEXT NOT NULL,
    mandate_digest           TEXT NOT NULL,
    intent_ref               TEXT NOT NULL,
    intent_digest            TEXT NOT NULL,
    use_key                  TEXT NOT NULL,
    use_ordinal              INTEGER NOT NULL,
    ceiling_reservation_refs TEXT NOT NULL,
    decision_refs            TEXT NOT NULL,
    consumed_at              TEXT NOT NULL,
    digest                   TEXT NOT NULL,
    UNIQUE(mandate_ref, use_key),
    UNIQUE(mandate_ref, use_ordinal)
) STRICT;

CREATE TABLE execution_consumption_receipts (
    receipt_id                 TEXT PRIMARY KEY,
    society_id                 TEXT NOT NULL,
    byom_endpoint_ref          TEXT NOT NULL,
    endpoint_incarnation       TEXT NOT NULL,
    recovery_epoch             INTEGER NOT NULL,
    intent_ref                 TEXT NOT NULL,
    intent_digest              TEXT NOT NULL,
    mandate_use_ref            TEXT NOT NULL,
    mandate_use_digest         TEXT NOT NULL,
    stable_execution_key       TEXT NOT NULL UNIQUE,
    subject_digest             TEXT NOT NULL,
    disclosure_digest          TEXT,
    driver_audience            TEXT NOT NULL,
    participant_ref            TEXT NOT NULL,
    episode_ref                TEXT,
    episode_fence_digest       TEXT,
    budget_reservation_set_ref TEXT NOT NULL,
    host_effect_ref            TEXT NOT NULL,
    host_effect_digest         TEXT NOT NULL,
    byom_fence_epoch           INTEGER NOT NULL,
    host_fence_epoch           INTEGER NOT NULL,
    issued_at                  TEXT NOT NULL,
    expires_at                 TEXT NOT NULL,
    max_uses                   INTEGER NOT NULL,
    digest                     TEXT NOT NULL
) STRICT;
"#;

/// V9 (seam fix S-1/S-2): the ResourceAllocation's CROSS-BOUNDARY binding
/// digest. `resource_allocations.digest` is byom's own `local_erasure_safe`
/// record commitment under a per-object secret — nobody outside byom can
/// re-derive it, so it can never be the value a counterparty pins. The new
/// column holds the `portable_public` digest over the exact
/// `bpp-resource-allocation-binding-v0` fragment (the members Kovee also
/// holds), which `episode_request` publishes and `placement_admit` compares.
const V9: &str = r#"
ALTER TABLE resource_allocations
    ADD COLUMN binding_digest TEXT NOT NULL DEFAULT '';
"#;

const MIGRATIONS: [&str; 9] = [V1, V2, V3, V4, V5, V6, V7, V8, V9];

#[derive(Debug, thiserror::Error)]
pub enum SchemaError {
    #[error(transparent)]
    Db(#[from] rusqlite::Error),
}

/// Opens pragmas and applies numbered migrations; returns the effective
/// journal mode (`wal` on disk, `memory` in memory).
pub fn open_and_migrate(conn: &Connection) -> Result<String, SchemaError> {
    let journal_mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))?;
    // `secure_delete` zeroes freed content instead of leaving it in the
    // file's free pages: destroying a secret (BY-D2) must not leave its
    // bytes recoverable from the database file.
    conn.execute_batch(
        "PRAGMA foreign_keys = ON; PRAGMA synchronous = FULL; PRAGMA secure_delete = ON;",
    )?;
    let mut version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    while (version as usize) < MIGRATIONS.len() {
        let step = MIGRATIONS[version as usize];
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(step)?;
        tx.execute_batch(&format!("PRAGMA user_version = {}", version + 1))?;
        tx.commit()?;
        version += 1;
    }
    Ok(journal_mode)
}

pub fn meta_get(conn: &Connection, key: &str) -> Result<Option<Vec<u8>>, SchemaError> {
    use rusqlite::OptionalExtension as _;
    Ok(conn
        .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
        .optional()?)
}

pub fn meta_get_text(conn: &Connection, key: &str) -> Result<Option<String>, SchemaError> {
    Ok(meta_get(conn, key)?.map(|b| String::from_utf8_lossy(&b).into_owned()))
}

pub fn meta_set(conn: &Connection, key: &str, value: &[u8]) -> Result<(), SchemaError> {
    conn.execute(
        "INSERT INTO meta (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![key, value],
    )?;
    Ok(())
}
