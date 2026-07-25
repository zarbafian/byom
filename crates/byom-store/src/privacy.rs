//! The §15.4 privacy-access chain (family PROFILE §7, normative;
//! D-R0-1): every allowed AND denied sensitive read appends one chained
//! `PrivacyAccessRecord` — actor, purpose, canonical query/scope digest,
//! result cardinality and byte counts, dependencies, outcome — never
//! result plaintext. Records chain under a per-Society scope key
//! (`scope_erasure_safe`): destroying the chain key erases verifiability
//! of the whole chain, never one record.
//!
//! Release rule (PROFILE §7): sensitive bytes are released only after
//! the covering `allowed` record has committed; a failed record write
//! blocks the read (`privacy_access_record_commit_failed` — unlogged
//! bytes are never served). A denied read still chains a record.
//!
//! Developer-profile honesty: this chain lives in the same same-UID
//! SQLite store — internal access logging only. Operator-resistant
//! witnessing (the separate non-rollbackable access journal of the
//! managed profile) is explicitly UNCLAIMED at B1.

use bpp_core::canonical::{hex, hmac_sha256, tagged_canonical};
use bpp_core::digest::DigestRef;
use bpp_core::time::rfc3339_utc;
use rusqlite::{params, OptionalExtension as _};
use serde_json::{json, Map, Value};

use crate::{Store, StoreError};

/// The `$domain` tag of the record preimage (PROFILE §7).
pub const PRIVACY_RECORD_TAG: &str = "bpp-privacy-access-record-v1";

/// The 14 required preimage members, in PROFILE §7 order (plus the
/// chain link `previous_access_digest`, wholly absent at genesis; the
/// self-referential `record_digest` is EXCLUDED from the preimage).
pub const PRIVACY_PREIMAGE_MEMBERS: [&str; 14] = [
    "society_id",
    "internal_access_sequence",
    "access_event_id",
    "endpoint_incarnation",
    "recovery_epoch",
    "actor_binding_digest",
    "operation",
    "purpose_ref",
    "query_or_scope_digest",
    "result_object_count",
    "result_bytes",
    "outcome",
    "dependency_digest",
    "occurred_at",
];

/// The chain scope key reference for one Society.
pub fn chain_key_ref(society_id: &str) -> String {
    format!("byom-privacy-chain:{society_id}")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Allowed,
    Denied,
    Error,
}

impl Outcome {
    fn as_str(self) -> &'static str {
        match self {
            Outcome::Allowed => "allowed",
            Outcome::Denied => "denied",
            Outcome::Error => "error",
        }
    }
}

/// One sensitive access to record before any release decision.
pub struct Access {
    pub society_id: String,
    pub operation: String,
    pub purpose_ref: String,
    /// The channel-derived actor; only its binding digest enters the
    /// record.
    pub actor: String,
    /// The exact query arguments; only their canonical digest enters the
    /// record (never payload bytes).
    pub query: Value,
    pub result_object_count: u64,
    pub result_bytes: u64,
    pub outcome: Outcome,
}

#[derive(Debug, thiserror::Error)]
pub enum PrivacyError {
    #[error("privacy_record_missing_{0}")]
    MissingMember(&'static str),
    #[error("privacy_record_preimage_carries_record_digest")]
    CarriesRecordDigest,
    #[error(transparent)]
    Store(#[from] StoreError),
}

impl From<rusqlite::Error> for PrivacyError {
    fn from(e: rusqlite::Error) -> PrivacyError {
        PrivacyError::Store(StoreError::Db(e))
    }
}

/// The keyed record digest over the exact PROFILE §7 preimage: rejects a
/// preimage carrying its own `record_digest`, then any missing required
/// member, then HMACs the `$domain`-tagged canonical bytes under the
/// chain scope key.
pub fn record_digest(
    chain_key: &[u8],
    key_ref: &str,
    record: &Value,
) -> Result<DigestRef, PrivacyError> {
    let map = record
        .as_object()
        .ok_or(PrivacyError::MissingMember("society_id"))?;
    if map.contains_key("record_digest") {
        return Err(PrivacyError::CarriesRecordDigest);
    }
    for member in PRIVACY_PREIMAGE_MEMBERS {
        if !map.contains_key(member) {
            return Err(PrivacyError::MissingMember(member));
        }
    }
    let preimage = tagged_canonical(PRIVACY_RECORD_TAG, record)
        .map_err(|e| PrivacyError::Store(StoreError::Canonical(e)))?;
    let mac = hmac_sha256(chain_key, &preimage);
    Ok(DigestRef::scope_erasure_safe(key_ref, hex(&mac)))
}

/// The chain link carrying the previous record's digest forward under
/// the same chain key (PROFILE §7).
pub fn chain_link(previous: &DigestRef) -> DigestRef {
    DigestRef::scope_erasure_safe(
        previous.key_ref.as_deref().unwrap_or_default(),
        previous.value_hex.clone(),
    )
}

impl Store {
    /// The per-Society privacy chain scope key (PROFILE §7).
    pub fn privacy_chain_key(&self, society_id: &str) -> Result<[u8; 32], StoreError> {
        self.scope_key(&format!("privacy-chain:{society_id}"))
    }
}

/// Appends one chained record IN ITS OWN COMMITTED TRANSACTION and
/// returns its dense `internal_access_sequence`. The caller must commit
/// this BEFORE releasing sensitive bytes (PROFILE §7 release rule) and
/// must NOT release on failure.
pub fn append_record(store: &Store, access: &Access, now: i64) -> Result<u64, PrivacyError> {
    let key = store.privacy_chain_key(&access.society_id)?;
    let key_ref = chain_key_ref(&access.society_id);
    let tx = store.conn().unchecked_transaction()?;

    let head: Option<(i64, String)> = tx
        .query_row(
            "SELECT internal_access_sequence, record FROM privacy_access_records
             WHERE society_id = ?1 ORDER BY internal_access_sequence DESC LIMIT 1",
            [&access.society_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let (sequence, previous) = match head {
        Some((seq, prev_text)) => {
            let prev: Value = serde_json::from_str(&prev_text).map_err(StoreError::from)?;
            let prev_digest: DigestRef =
                serde_json::from_value(prev["record_digest"].clone()).map_err(StoreError::from)?;
            ((seq + 1) as u64, Some(prev_digest))
        }
        None => (1, None),
    };

    let access_event_id = store.new_id("pacc")?;
    let actor_binding = store.actor_binding_digest(&access.society_id, &access.actor)?;
    let query_digest = store.record_digest(
        &access.society_id,
        &access_event_id,
        "bpp-privacy-query-v0",
        &access.query,
    )?;
    let dependency_digest = store.record_digest(
        &access.society_id,
        &access_event_id,
        "bpp-privacy-dependency-set-v0",
        &json!({
            "surface_actor": access.actor,
            "society_id": access.society_id,
            "assurance": "developer",
            "witnessing": "internal-logging-only (operator-resistant witnessing unclaimed)",
        }),
    )?;

    // The record map in PROFILE §7 member order; the genesis link is
    // wholly absent — never a null-valued pseudo-DigestRef.
    let mut record = Map::new();
    record.insert("society_id".into(), json!(access.society_id));
    record.insert("internal_access_sequence".into(), json!(sequence));
    record.insert("access_event_id".into(), json!(access_event_id));
    if let Some(previous) = &previous {
        record.insert(
            "previous_access_digest".into(),
            serde_json::to_value(chain_link(previous)).map_err(StoreError::from)?,
        );
    }
    record.insert("endpoint_incarnation".into(), json!(store.incarnation()?));
    record.insert(
        "recovery_epoch".into(),
        json!(store.recovery_epoch(&access.society_id)?),
    );
    record.insert(
        "actor_binding_digest".into(),
        serde_json::to_value(&actor_binding).map_err(StoreError::from)?,
    );
    record.insert("operation".into(), json!(access.operation));
    record.insert("purpose_ref".into(), json!(access.purpose_ref));
    record.insert(
        "query_or_scope_digest".into(),
        serde_json::to_value(&query_digest).map_err(StoreError::from)?,
    );
    record.insert(
        "result_object_count".into(),
        json!(access.result_object_count),
    );
    record.insert("result_bytes".into(), json!(access.result_bytes));
    record.insert("outcome".into(), json!(access.outcome.as_str()));
    record.insert(
        "dependency_digest".into(),
        serde_json::to_value(&dependency_digest).map_err(StoreError::from)?,
    );
    record.insert("occurred_at".into(), json!(rfc3339_utc(now)));

    let record_value = Value::Object(record);
    let digest = record_digest(&key, &key_ref, &record_value)?;
    let mut record = match record_value {
        Value::Object(m) => m,
        _ => Map::new(),
    };
    record.insert(
        "record_digest".into(),
        serde_json::to_value(&digest).map_err(StoreError::from)?,
    );

    tx.execute(
        "INSERT INTO privacy_access_records
            (society_id, internal_access_sequence, record, record_digest_hex, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            access.society_id,
            sequence as i64,
            Value::Object(record).to_string(),
            digest.value_hex,
            rfc3339_utc(now),
        ],
    )?;
    tx.commit()?;
    Ok(sequence)
}

/// Walks one Society's chain genesis → head, re-deriving every record
/// digest and matching every link against the prior record; returns the
/// number of verified records.
pub fn verify_chain(store: &Store, society_id: &str) -> Result<u64, PrivacyError> {
    let key = store.privacy_chain_key(society_id)?;
    let key_ref = chain_key_ref(society_id);
    let mut stmt = store.conn().prepare(
        "SELECT internal_access_sequence, record, record_digest_hex
         FROM privacy_access_records WHERE society_id = ?1
         ORDER BY internal_access_sequence ASC",
    )?;
    let rows: Vec<(i64, String, String)> = stmt
        .query_map([society_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<_, _>>()?;
    let mut previous_hex: Option<String> = None;
    let mut count = 0u64;
    for (i, (seq, record_text, column_hex)) in rows.iter().enumerate() {
        if *seq != (i as i64) + 1 {
            return Err(PrivacyError::Store(StoreError::Corrupt(format!(
                "privacy chain sequence gap at {seq}"
            ))));
        }
        let mut record: Value = serde_json::from_str(record_text).map_err(StoreError::from)?;
        let stored: DigestRef =
            serde_json::from_value(record["record_digest"].clone()).map_err(StoreError::from)?;
        if stored.value_hex != *column_hex {
            return Err(PrivacyError::Store(StoreError::Corrupt(
                "privacy record digest column mismatch".to_owned(),
            )));
        }
        match (&previous_hex, record.get("previous_access_digest")) {
            (None, None) => {}
            (Some(prev), Some(link)) => {
                if link["value_hex"].as_str() != Some(prev.as_str()) {
                    return Err(PrivacyError::Store(StoreError::Corrupt(format!(
                        "privacy chain link mismatch at {seq}"
                    ))));
                }
            }
            _ => {
                return Err(PrivacyError::Store(StoreError::Corrupt(format!(
                    "privacy chain genesis/link shape at {seq}"
                ))));
            }
        }
        if let Some(m) = record.as_object_mut() {
            m.remove("record_digest");
        }
        let derived = record_digest(&key, &key_ref, &record)?;
        if derived.value_hex != stored.value_hex {
            return Err(PrivacyError::Store(StoreError::Corrupt(format!(
                "privacy record digest mismatch at {seq}"
            ))));
        }
        previous_hex = Some(stored.value_hex);
        count += 1;
    }
    Ok(count)
}
