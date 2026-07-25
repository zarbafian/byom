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

const MIGRATIONS: [&str; 1] = [V1];

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
