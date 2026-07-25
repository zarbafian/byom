//! The serialized "exact pending set" (§15.3): the full transition an
//! authority mutation prepares invisibly and journal finalize
//! materializes exactly — row upserts against a closed table/column
//! whitelist plus dense-sequenced events. Both the live finalize path
//! and crash recovery interpret the SAME serialized effects, so a
//! recovered transaction finalizes byte-identically.

use rusqlite::{Connection, ToSql};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One state effect: a full-row upsert into a whitelisted table. The
/// row must name every column of the table (fail closed), so the effect
/// is self-contained and replays deterministically.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Effect {
    Upsert {
        table: String,
        row: serde_json::Map<String, Value>,
    },
}

/// One event to append at finalize. The event id is minted at prepare
/// (results may cite it, e.g. `genesis_event_ref`); the Society sequence
/// is allocated at finalize time (dense; abandoned transactions consume
/// none).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewEvent {
    pub event_id: String,
    pub society_id: String,
    pub kind: String,
    pub object_ref: String,
    pub object_revision: u64,
    #[serde(default)]
    pub participant_ref: Option<String>,
    pub actor_ref: String,
    pub causation_ref: String,
    pub correlation_ref: String,
    pub payload: Value,
    pub visibility_scope_ref: String,
}

#[derive(Debug, thiserror::Error)]
pub enum EffectError {
    #[error(transparent)]
    Db(#[from] rusqlite::Error),
    #[error("effect rejected: {0}")]
    Rejected(String),
}

/// The closed table/column whitelist (schema V1 domain tables).
const TABLES: &[(&str, &[&str])] = &[
    (
        "societies",
        &[
            "society_id",
            "revision",
            "state",
            "home_authority_ref",
            "kovee_realm_binding",
            "kovee_project_binding",
            "charter_head_ref",
            "charter_head_digest",
            "classification_binding_ref",
            "classification_binding_digest",
            "root_budget_account_set_ref",
            "recovery_epoch",
            "created_at",
            "preparation",
            "genesis_event_ref",
            "next_event_sequence",
        ],
    ),
    (
        "charter_revisions",
        &[
            "charter_revision_id",
            "society_id",
            "revision",
            "body_ref",
            "body_digest",
            "state",
            "adopted_by_decision_ref",
            "created_at",
        ],
    ),
    (
        "participants",
        &[
            "participant_id",
            "society_id",
            "kind",
            "revision",
            "binding_epoch",
            "display_profile_ref",
            "standing_ref",
            "state",
            "created_at",
        ],
    ),
    (
        "manifestation_revisions",
        &[
            "manifestation_id",
            "society_id",
            "participant_ref",
            "revision",
            "kind",
            "body_digest",
            "status",
            "admitted_by_decision_ref",
            "created_at",
        ],
    ),
    (
        "standing_revisions",
        &[
            "standing_id",
            "society_id",
            "participant_ref",
            "revision",
            "status",
            "offer_ref",
            "acceptance_ref",
            "decision_ref",
            "created_at",
        ],
    ),
    (
        "membership_offers",
        &[
            "offer_id",
            "society_id",
            "participant_ref",
            "proposed_standing_ref",
            "subject_digest",
            "offered_by_decision_ref",
            "expires_at",
            "state",
            "revision",
            "fence_epoch",
            "acceptance_id",
            "accepted_at",
            "refusal_id",
            "refused_at",
            "superseded_acceptance_ref",
            "refusal_reason_ref",
            "created_at",
        ],
    ),
    (
        "candidate_channels",
        &[
            "channel_id",
            "society_id",
            "offer_ref",
            "token",
            "token_path",
            "state",
            "created_at",
            "closed_at",
        ],
    ),
    (
        "participant_channels",
        &[
            "channel_id",
            "society_id",
            "participant_ref",
            "token",
            "token_path",
            "state",
            "created_at",
            "closed_at",
        ],
    ),
];

fn columns_of(table: &str) -> Option<&'static [&'static str]> {
    TABLES
        .iter()
        .find(|(name, _)| *name == table)
        .map(|(_, cols)| *cols)
}

enum Bound {
    Null,
    Integer(i64),
    Text(String),
}

impl ToSql for Bound {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        match self {
            Bound::Null => rusqlite::types::Null.to_sql(),
            Bound::Integer(i) => i.to_sql(),
            Bound::Text(t) => t.to_sql(),
        }
    }
}

fn bind(value: &Value) -> Result<Bound, EffectError> {
    Ok(match value {
        Value::Null => Bound::Null,
        Value::Number(n) => Bound::Integer(
            n.as_i64()
                .ok_or_else(|| EffectError::Rejected("non-integer number".to_owned()))?,
        ),
        Value::String(s) => Bound::Text(s.clone()),
        Value::Bool(_) => return Err(EffectError::Rejected("bool column".to_owned())),
        // Structured values (typed digests, JSON bodies) persist as JSON
        // text.
        other => Bound::Text(other.to_string()),
    })
}

/// Applies one effect inside the open finalize transaction.
pub fn apply(conn: &Connection, effect: &Effect) -> Result<(), EffectError> {
    match effect {
        Effect::Upsert { table, row } => {
            let Some(columns) = columns_of(table) else {
                return Err(EffectError::Rejected(format!(
                    "table {table} not whitelisted"
                )));
            };
            if row.len() != columns.len() || !columns.iter().all(|c| row.contains_key(*c)) {
                return Err(EffectError::Rejected(format!(
                    "row for {table} must name exactly its columns"
                )));
            }
            let placeholders: Vec<String> = (1..=columns.len()).map(|i| format!("?{i}")).collect();
            let sql = format!(
                "INSERT OR REPLACE INTO {table} ({}) VALUES ({})",
                columns.join(", "),
                placeholders.join(", ")
            );
            let mut bounds = Vec::with_capacity(columns.len());
            for c in columns {
                bounds.push(bind(&row[*c])?);
            }
            let params: Vec<&dyn ToSql> = bounds.iter().map(|b| b as &dyn ToSql).collect();
            conn.execute(&sql, params.as_slice())?;
            Ok(())
        }
    }
}
