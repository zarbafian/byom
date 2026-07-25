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
/// - `payload_secret` is the per-event `local_erasure_safe` object
///   secret (PROFILE §6): kept in the same same-UID store under the
///   developer profile, so digests stay locally verifiable.
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

const MIGRATIONS: [&str; 2] = [V1, V2];

#[derive(Debug, thiserror::Error)]
pub enum SchemaError {
    #[error(transparent)]
    Db(#[from] rusqlite::Error),
}

/// Opens pragmas and applies numbered migrations; returns the effective
/// journal mode (`wal` on disk, `memory` in memory).
pub fn open_and_migrate(conn: &Connection) -> Result<String, SchemaError> {
    let journal_mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))?;
    conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA synchronous = FULL;")?;
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
