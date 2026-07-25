//! Typed row readers for the B1 domain tables. Every reader takes a
//! `&Connection` so mutation prepare closures (inside the open §15.3
//! prepare transaction) and daemon reads share one query surface.

use rusqlite::{params, Connection, OptionalExtension as _, Row};
use serde_json::{Map, Value};

// ------------------------------------------------- generic row access ----
//
// The slice-2 domain tables are read as JSON maps keyed by their exact
// column names (the same closed name set the effects whitelist pins), so
// one reader serves every table and a read-modify-write round-trips
// bit-stably through `Effect::Upsert`. JSON-carrying TEXT columns stay
// serialized; `json_of` parses them on demand.

fn value_of(r: &Row, i: usize) -> rusqlite::Result<Value> {
    Ok(match r.get_ref(i)? {
        rusqlite::types::ValueRef::Null => Value::Null,
        rusqlite::types::ValueRef::Integer(n) => Value::from(n),
        rusqlite::types::ValueRef::Real(f) => serde_json::Number::from_f64(f)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        rusqlite::types::ValueRef::Text(t) => {
            Value::String(String::from_utf8_lossy(t).into_owned())
        }
        rusqlite::types::ValueRef::Blob(_) => Value::Null,
    })
}

fn row_to_map(names: &[String], r: &Row) -> rusqlite::Result<Map<String, Value>> {
    let mut m = Map::new();
    for (i, name) in names.iter().enumerate() {
        m.insert(name.clone(), value_of(r, i)?);
    }
    Ok(m)
}

/// One whitelisted-table row as a column-keyed JSON map. `table` and
/// `key_col` are handler-supplied constants, never caller input.
pub fn get_row(
    conn: &Connection,
    table: &str,
    key_col: &str,
    key: &str,
) -> rusqlite::Result<Option<Map<String, Value>>> {
    debug_assert!(crate::effects::columns_of(table).is_some());
    let mut stmt = conn.prepare(&format!("SELECT * FROM {table} WHERE {key_col} = ?1"))?;
    let names: Vec<String> = stmt
        .column_names()
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    let mut rows = stmt.query([key])?;
    match rows.next()? {
        Some(r) => Ok(Some(row_to_map(&names, r)?)),
        None => Ok(None),
    }
}

/// Every row of a whitelisted table matching `where_col = key`, ordered
/// by `order_col`.
pub fn rows_where(
    conn: &Connection,
    table: &str,
    where_col: &str,
    key: &str,
    order_col: &str,
) -> rusqlite::Result<Vec<Map<String, Value>>> {
    debug_assert!(crate::effects::columns_of(table).is_some());
    let mut stmt = conn.prepare(&format!(
        "SELECT * FROM {table} WHERE {where_col} = ?1 ORDER BY {order_col} ASC"
    ))?;
    let names: Vec<String> = stmt
        .column_names()
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    let mut rows = stmt.query([key])?;
    let mut out = Vec::new();
    while let Some(r) = rows.next()? {
        out.push(row_to_map(&names, r)?);
    }
    Ok(out)
}

/// String accessor (empty when absent/NULL).
pub fn str_of<'m>(m: &'m Map<String, Value>, key: &str) -> &'m str {
    m.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// Integer accessor (0 when absent/NULL).
pub fn u64_of(m: &Map<String, Value>, key: &str) -> u64 {
    m.get(key).and_then(Value::as_u64).unwrap_or_default()
}

/// Parses a JSON-carrying TEXT column (Null when absent or unparseable).
pub fn json_of(m: &Map<String, Value>, key: &str) -> Value {
    m.get(key)
        .and_then(Value::as_str)
        .and_then(|t| serde_json::from_str(t).ok())
        .unwrap_or(Value::Null)
}

/// The current seat head: the single active position on
/// `(proposal_kind, proposal_ref, seat_ref)`.
pub fn active_position(
    conn: &Connection,
    proposal_kind: &str,
    proposal_ref: &str,
    seat_ref: &str,
) -> rusqlite::Result<Option<Map<String, Value>>> {
    let mut stmt = conn.prepare(
        "SELECT * FROM positions
         WHERE proposal_kind = ?1 AND proposal_ref = ?2 AND seat_ref = ?3
           AND status = 'active'
         ORDER BY revision DESC LIMIT 1",
    )?;
    let names: Vec<String> = stmt
        .column_names()
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    let mut rows = stmt.query(params![proposal_kind, proposal_ref, seat_ref])?;
    match rows.next()? {
        Some(r) => Ok(Some(row_to_map(&names, r)?)),
        None => Ok(None),
    }
}

/// Open (non-terminal) activity streams citing a mandate ref in their
/// serialized `mandate_refs` JSON array.
pub fn open_activities_citing_mandate(
    conn: &Connection,
    mandate_id: &str,
) -> rusqlite::Result<u64> {
    let needle = format!("\"{mandate_id}\"");
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM activity_streams
         WHERE state IN ('ready', 'active', 'waiting', 'reviewing', 'held')
           AND instr(mandate_refs, ?1) > 0",
        [&needle],
        |r| r.get(0),
    )?;
    Ok(n as u64)
}

/// A live (still `proposed`) pledge successor occupying the one CAS
/// successor slot of `pledge_id` (D-RT-3).
pub fn live_successor_proposal(
    conn: &Connection,
    pledge_id: &str,
) -> rusqlite::Result<Option<Map<String, Value>>> {
    let mut stmt = conn.prepare(
        "SELECT * FROM pledge_proposals
         WHERE amendment_predecessor_ref = ?1 AND state = 'proposed' LIMIT 1",
    )?;
    let names: Vec<String> = stmt
        .column_names()
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    let mut rows = stmt.query([pledge_id])?;
    match rows.next()? {
        Some(r) => Ok(Some(row_to_map(&names, r)?)),
        None => Ok(None),
    }
}

/// The one active self-policy of `(participant, kind)`, if any.
pub fn active_self_policy(
    conn: &Connection,
    participant_ref: &str,
    kind: &str,
) -> rusqlite::Result<Option<Map<String, Value>>> {
    let mut stmt = conn.prepare(
        "SELECT * FROM self_policies
         WHERE participant_ref = ?1 AND kind = ?2 AND status = 'active'
         ORDER BY revision DESC LIMIT 1",
    )?;
    let names: Vec<String> = stmt
        .column_names()
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    let mut rows = stmt.query(params![participant_ref, kind])?;
    match rows.next()? {
        Some(r) => Ok(Some(row_to_map(&names, r)?)),
        None => Ok(None),
    }
}

/// A wake intent of `participant` under `stable_wake_key` (§11.1
/// uniqueness, enforced in the prepare closure — see schema note).
pub fn wake_intent_by_key(
    conn: &Connection,
    participant_ref: &str,
    stable_wake_key: &str,
) -> rusqlite::Result<Option<Map<String, Value>>> {
    let mut stmt = conn.prepare(
        "SELECT * FROM wake_intents
         WHERE participant_ref = ?1 AND stable_wake_key = ?2 LIMIT 1",
    )?;
    let names: Vec<String> = stmt
        .column_names()
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    let mut rows = stmt.query(params![participant_ref, stable_wake_key])?;
    match rows.next()? {
        Some(r) => Ok(Some(row_to_map(&names, r)?)),
        None => Ok(None),
    }
}

/// The budget-account ledger row of `(account_ref, dimension)`.
pub fn budget_account(
    conn: &Connection,
    account_ref: &str,
    dimension: &str,
) -> rusqlite::Result<Option<Map<String, Value>>> {
    let mut stmt =
        conn.prepare("SELECT * FROM budget_accounts WHERE account_ref = ?1 AND dimension = ?2")?;
    let names: Vec<String> = stmt
        .column_names()
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    let mut rows = stmt.query(params![account_ref, dimension])?;
    match rows.next()? {
        Some(r) => Ok(Some(row_to_map(&names, r)?)),
        None => Ok(None),
    }
}

/// The sole active human participant of the personal profile (the
/// sovereign seat holder).
pub fn sovereign_participant(
    conn: &Connection,
    society_id: &str,
) -> rusqlite::Result<Option<ParticipantRow>> {
    conn.query_row(
        "SELECT participant_id, society_id, kind, revision, binding_epoch,
                display_profile_ref, standing_ref, state, created_at
         FROM participants WHERE society_id = ?1 AND kind = 'human' AND state = 'active'
         ORDER BY created_at ASC LIMIT 1",
        [society_id],
        |r| {
            Ok(ParticipantRow {
                participant_id: r.get(0)?,
                society_id: r.get(1)?,
                kind: r.get(2)?,
                revision: r.get::<_, i64>(3)? as u64,
                binding_epoch: r.get::<_, i64>(4)? as u64,
                display_profile_ref: r.get(5)?,
                standing_ref: r.get(6)?,
                state: r.get(7)?,
                created_at: r.get(8)?,
            })
        },
    )
    .optional()
}

/// One event row (id, kind, payload text, payload digest JSON, society).
pub fn event_payload_row(
    conn: &Connection,
    event_id: &str,
) -> rusqlite::Result<Option<(String, String, String, String)>> {
    conn.query_row(
        "SELECT society_id, kind, payload, payload_digest FROM events WHERE event_id = ?1",
        [event_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )
    .optional()
}

#[derive(Debug, Clone)]
pub struct SocietyRow {
    pub society_id: String,
    pub revision: u64,
    pub state: String,
    pub home_authority_ref: String,
    pub kovee_realm_binding: Option<String>,
    pub kovee_project_binding: Option<String>,
    pub charter_head_ref: String,
    pub charter_head_digest: String,
    pub classification_binding_ref: String,
    pub classification_binding_digest: String,
    pub root_budget_account_set_ref: String,
    pub recovery_epoch: u64,
    pub created_at: String,
    pub preparation: String,
    pub genesis_event_ref: Option<String>,
    pub next_event_sequence: u64,
}

fn society_from(r: &Row) -> rusqlite::Result<SocietyRow> {
    Ok(SocietyRow {
        society_id: r.get(0)?,
        revision: r.get::<_, i64>(1)? as u64,
        state: r.get(2)?,
        home_authority_ref: r.get(3)?,
        kovee_realm_binding: r.get(4)?,
        kovee_project_binding: r.get(5)?,
        charter_head_ref: r.get(6)?,
        charter_head_digest: r.get(7)?,
        classification_binding_ref: r.get(8)?,
        classification_binding_digest: r.get(9)?,
        root_budget_account_set_ref: r.get(10)?,
        recovery_epoch: r.get::<_, i64>(11)? as u64,
        created_at: r.get(12)?,
        preparation: r.get(13)?,
        genesis_event_ref: r.get(14)?,
        next_event_sequence: r.get::<_, i64>(15)? as u64,
    })
}

pub fn get_society(conn: &Connection, id: &str) -> rusqlite::Result<Option<SocietyRow>> {
    conn.query_row(
        "SELECT society_id, revision, state, home_authority_ref, kovee_realm_binding,
                kovee_project_binding, charter_head_ref, charter_head_digest,
                classification_binding_ref, classification_binding_digest,
                root_budget_account_set_ref, recovery_epoch, created_at, preparation,
                genesis_event_ref, next_event_sequence
         FROM societies WHERE society_id = ?1",
        [id],
        society_from,
    )
    .optional()
}

/// The one society of the personal profile, when exactly one exists.
pub fn sole_society(conn: &Connection) -> rusqlite::Result<Option<SocietyRow>> {
    let mut stmt = conn.prepare(
        "SELECT society_id, revision, state, home_authority_ref, kovee_realm_binding,
                kovee_project_binding, charter_head_ref, charter_head_digest,
                classification_binding_ref, classification_binding_digest,
                root_budget_account_set_ref, recovery_epoch, created_at, preparation,
                genesis_event_ref, next_event_sequence
         FROM societies LIMIT 2",
    )?;
    let rows: Vec<SocietyRow> = stmt
        .query_map([], society_from)?
        .collect::<Result<_, _>>()?;
    Ok(if rows.len() == 1 {
        rows.into_iter().next()
    } else {
        None
    })
}

#[derive(Debug, Clone)]
pub struct OfferRow {
    pub offer_id: String,
    pub society_id: String,
    pub participant_ref: String,
    pub proposed_standing_ref: String,
    pub subject_digest: String,
    pub offered_by_decision_ref: String,
    pub expires_at: String,
    pub state: String,
    pub revision: u64,
    pub fence_epoch: u64,
    pub acceptance_id: Option<String>,
    pub accepted_at: Option<String>,
    pub refusal_id: Option<String>,
    pub refused_at: Option<String>,
    pub superseded_acceptance_ref: Option<String>,
    pub refusal_reason_ref: Option<String>,
    pub created_at: String,
}

impl OfferRow {
    /// The full-row upsert map for an effect (every column named).
    pub fn to_effect_row(&self) -> serde_json::Map<String, serde_json::Value> {
        let mut m = serde_json::Map::new();
        let s = |v: &str| serde_json::Value::String(v.to_owned());
        let o = |v: &Option<String>| match v {
            Some(v) => serde_json::Value::String(v.clone()),
            None => serde_json::Value::Null,
        };
        m.insert("offer_id".into(), s(&self.offer_id));
        m.insert("society_id".into(), s(&self.society_id));
        m.insert("participant_ref".into(), s(&self.participant_ref));
        m.insert(
            "proposed_standing_ref".into(),
            s(&self.proposed_standing_ref),
        );
        m.insert("subject_digest".into(), s(&self.subject_digest));
        m.insert(
            "offered_by_decision_ref".into(),
            s(&self.offered_by_decision_ref),
        );
        m.insert("expires_at".into(), s(&self.expires_at));
        m.insert("state".into(), s(&self.state));
        m.insert("revision".into(), serde_json::Value::from(self.revision));
        m.insert(
            "fence_epoch".into(),
            serde_json::Value::from(self.fence_epoch),
        );
        m.insert("acceptance_id".into(), o(&self.acceptance_id));
        m.insert("accepted_at".into(), o(&self.accepted_at));
        m.insert("refusal_id".into(), o(&self.refusal_id));
        m.insert("refused_at".into(), o(&self.refused_at));
        m.insert(
            "superseded_acceptance_ref".into(),
            o(&self.superseded_acceptance_ref),
        );
        m.insert("refusal_reason_ref".into(), o(&self.refusal_reason_ref));
        m.insert("created_at".into(), s(&self.created_at));
        m
    }
}

pub fn get_offer(conn: &Connection, id: &str) -> rusqlite::Result<Option<OfferRow>> {
    conn.query_row(
        "SELECT offer_id, society_id, participant_ref, proposed_standing_ref,
                subject_digest, offered_by_decision_ref, expires_at, state, revision,
                fence_epoch, acceptance_id, accepted_at, refusal_id, refused_at,
                superseded_acceptance_ref, refusal_reason_ref, created_at
         FROM membership_offers WHERE offer_id = ?1",
        [id],
        |r| {
            Ok(OfferRow {
                offer_id: r.get(0)?,
                society_id: r.get(1)?,
                participant_ref: r.get(2)?,
                proposed_standing_ref: r.get(3)?,
                subject_digest: r.get(4)?,
                offered_by_decision_ref: r.get(5)?,
                expires_at: r.get(6)?,
                state: r.get(7)?,
                revision: r.get::<_, i64>(8)? as u64,
                fence_epoch: r.get::<_, i64>(9)? as u64,
                acceptance_id: r.get(10)?,
                accepted_at: r.get(11)?,
                refusal_id: r.get(12)?,
                refused_at: r.get(13)?,
                superseded_acceptance_ref: r.get(14)?,
                refusal_reason_ref: r.get(15)?,
                created_at: r.get(16)?,
            })
        },
    )
    .optional()
}

#[derive(Debug, Clone)]
pub struct ParticipantRow {
    pub participant_id: String,
    pub society_id: String,
    pub kind: String,
    pub revision: u64,
    pub binding_epoch: u64,
    pub display_profile_ref: String,
    pub standing_ref: Option<String>,
    pub state: String,
    pub created_at: String,
}

pub fn get_participant(conn: &Connection, id: &str) -> rusqlite::Result<Option<ParticipantRow>> {
    conn.query_row(
        "SELECT participant_id, society_id, kind, revision, binding_epoch,
                display_profile_ref, standing_ref, state, created_at
         FROM participants WHERE participant_id = ?1",
        [id],
        |r| {
            Ok(ParticipantRow {
                participant_id: r.get(0)?,
                society_id: r.get(1)?,
                kind: r.get(2)?,
                revision: r.get::<_, i64>(3)? as u64,
                binding_epoch: r.get::<_, i64>(4)? as u64,
                display_profile_ref: r.get(5)?,
                standing_ref: r.get(6)?,
                state: r.get(7)?,
                created_at: r.get(8)?,
            })
        },
    )
    .optional()
}

#[derive(Debug, Clone)]
pub struct ManifestationRow {
    pub manifestation_id: String,
    pub society_id: String,
    pub participant_ref: String,
    pub revision: u64,
    pub kind: String,
    pub body_digest: String,
    pub status: String,
    pub admitted_by_decision_ref: Option<String>,
    pub created_at: String,
}

pub fn get_manifestation(
    conn: &Connection,
    id: &str,
) -> rusqlite::Result<Option<ManifestationRow>> {
    conn.query_row(
        "SELECT manifestation_id, society_id, participant_ref, revision, kind,
                body_digest, status, admitted_by_decision_ref, created_at
         FROM manifestation_revisions WHERE manifestation_id = ?1",
        [id],
        |r| {
            Ok(ManifestationRow {
                manifestation_id: r.get(0)?,
                society_id: r.get(1)?,
                participant_ref: r.get(2)?,
                revision: r.get::<_, i64>(3)? as u64,
                kind: r.get(4)?,
                body_digest: r.get(5)?,
                status: r.get(6)?,
                admitted_by_decision_ref: r.get(7)?,
                created_at: r.get(8)?,
            })
        },
    )
    .optional()
}

#[derive(Debug, Clone)]
pub struct ChannelRow {
    pub channel_id: String,
    pub society_id: String,
    pub scope_ref: String,
    pub token: String,
    pub token_path: String,
    pub state: String,
}

/// The candidate channel for one offer.
pub fn candidate_channel_for_offer(
    conn: &Connection,
    offer_ref: &str,
) -> rusqlite::Result<Option<ChannelRow>> {
    conn.query_row(
        "SELECT channel_id, society_id, offer_ref, token, token_path, state
         FROM candidate_channels WHERE offer_ref = ?1",
        [offer_ref],
        |r| {
            Ok(ChannelRow {
                channel_id: r.get(0)?,
                society_id: r.get(1)?,
                scope_ref: r.get(2)?,
                token: r.get(3)?,
                token_path: r.get(4)?,
                state: r.get(5)?,
            })
        },
    )
    .optional()
}

/// Resolves a presented candidate token to its channel (open or closed).
pub fn candidate_channel_by_token(
    conn: &Connection,
    token: &str,
) -> rusqlite::Result<Option<ChannelRow>> {
    conn.query_row(
        "SELECT channel_id, society_id, offer_ref, token, token_path, state
         FROM candidate_channels WHERE token = ?1",
        [token],
        |r| {
            Ok(ChannelRow {
                channel_id: r.get(0)?,
                society_id: r.get(1)?,
                scope_ref: r.get(2)?,
                token: r.get(3)?,
                token_path: r.get(4)?,
                state: r.get(5)?,
            })
        },
    )
    .optional()
}

/// Resolves a presented participant token to its channel (open or
/// closed).
pub fn participant_channel_by_token(
    conn: &Connection,
    token: &str,
) -> rusqlite::Result<Option<ChannelRow>> {
    conn.query_row(
        "SELECT channel_id, society_id, participant_ref, token, token_path, state
         FROM participant_channels WHERE token = ?1",
        [token],
        |r| {
            Ok(ChannelRow {
                channel_id: r.get(0)?,
                society_id: r.get(1)?,
                scope_ref: r.get(2)?,
                token: r.get(3)?,
                token_path: r.get(4)?,
                state: r.get(5)?,
            })
        },
    )
    .optional()
}

pub fn participant_channel_for(
    conn: &Connection,
    participant_ref: &str,
) -> rusqlite::Result<Option<ChannelRow>> {
    conn.query_row(
        "SELECT channel_id, society_id, participant_ref, token, token_path, state
         FROM participant_channels WHERE participant_ref = ?1",
        [participant_ref],
        |r| {
            Ok(ChannelRow {
                channel_id: r.get(0)?,
                society_id: r.get(1)?,
                scope_ref: r.get(2)?,
                token: r.get(3)?,
                token_path: r.get(4)?,
                state: r.get(5)?,
            })
        },
    )
    .optional()
}

#[derive(Debug, Clone)]
pub struct EventRow {
    pub event_id: String,
    pub society_id: String,
    pub sequence: u64,
    pub kind: String,
    pub object_ref: String,
    pub object_revision: u64,
    pub participant_ref: Option<String>,
    pub actor_ref: String,
    pub causation_ref: String,
    pub correlation_ref: String,
    pub payload_digest: String,
    pub visibility_scope_ref: String,
    pub occurred_at: String,
}

/// Ordered events strictly after `after_seq`, at most `limit`.
pub fn events_after(
    conn: &Connection,
    society_id: &str,
    after_seq: u64,
    limit: u64,
) -> rusqlite::Result<Vec<EventRow>> {
    let mut stmt = conn.prepare(
        "SELECT event_id, society_id, sequence, kind, object_ref, object_revision,
                participant_ref, actor_ref, causation_ref, correlation_ref,
                payload_digest, visibility_scope_ref, occurred_at
         FROM events WHERE society_id = ?1 AND sequence > ?2
         ORDER BY sequence ASC LIMIT ?3",
    )?;
    let rows = stmt.query_map(params![society_id, after_seq as i64, limit as i64], |r| {
        Ok(EventRow {
            event_id: r.get(0)?,
            society_id: r.get(1)?,
            sequence: r.get::<_, i64>(2)? as u64,
            kind: r.get(3)?,
            object_ref: r.get(4)?,
            object_revision: r.get::<_, i64>(5)? as u64,
            participant_ref: r.get(6)?,
            actor_ref: r.get(7)?,
            causation_ref: r.get(8)?,
            correlation_ref: r.get(9)?,
            payload_digest: r.get(10)?,
            visibility_scope_ref: r.get(11)?,
            occurred_at: r.get(12)?,
        })
    })?;
    rows.collect()
}
