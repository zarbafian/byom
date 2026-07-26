//! Immutable `GovernanceDecision` records and their transactional
//! resolution (BY-A1, DESIGN.md §1087-1113).
//!
//! Governance operations used to accept ANY identifier-shaped string as
//! their decision reference: a literal `dec-1` created a proposed
//! Participant, an active Standing and an active Manifestation. Nothing
//! was resolved, so nothing bound the act to an authority.
//!
//! Now every operation that takes a decision reference RESOLVES it
//! inside the prepare transaction, BEFORE any mutation is prepared:
//!
//! ```text
//! resolve(conn, society, "dec-offer-<offer>", …) must find a row whose
//!   kind / subject_kind / subject_ref are exactly this act's,
//!   subject_digest is the COMPLETE canonical DigestRef of the subject
//!     as it stands NOW,
//!   seat_snapshot seats the acting actor at its CURRENT binding epoch,
//!   dependency_closure still matches the Society's charter head,
//!     classification binding, endpoint incarnation and recovery epoch,
//!   digest re-derives from the retained per-object secret (immutable).
//! ```
//!
//! Anything else — absent, literal, stale, wrong subject, wrong actor —
//! is the typed refusal `decision_incomplete`, never a silent success.
//!
//! Where decisions come from: the B0.1 bundle has no decision-minting
//! operation (the assembly/collective family that mints them is a later
//! bundle), so the endpoint FORMS one at each governance act that
//! carries full seat assent under the sovereign seat, and the dependent
//! act resolves it. Decision references are derived from the subject
//! they decide (`dec-offer-<offer_id>`), so the caller can name the
//! decision it is acting under without a second round trip; the content
//! is server-authored and immutable.

use bpp_core::digest::DigestRef;
use bpp_core::problem::{Problem, ProblemKind};
use bpp_core::time::rfc3339_utc;
use byom_store::effects::Effect;
use byom_store::rows;
use rusqlite::Connection;
use serde_json::{json, Map, Value};

use crate::gov_ops::obj_pairs;
use crate::part_common::conn_record_digest;
use crate::state;

/// Decision kinds this slice forms and resolves.
pub const KIND_SOCIETY_GENESIS: &str = "society_genesis";
pub const KIND_MEMBERSHIP_ADMISSION: &str = "membership_admission";
pub const KIND_MANIFESTATION_ADMISSION: &str = "manifestation_admission";
pub const KIND_MANDATE_AUTHORITY: &str = "mandate_authority";
pub const KIND_CHARTER_ADOPTION: &str = "charter_adoption";

/// The decision reference derived from the subject it decides.
pub fn society_decision_ref(society_id: &str) -> String {
    format!("dec-society-{society_id}")
}
pub fn offer_decision_ref(offer_id: &str) -> String {
    format!("dec-offer-{offer_id}")
}
pub fn manifestation_decision_ref(manifestation_id: &str) -> String {
    format!("dec-manif-{manifestation_id}")
}
pub fn mandate_decision_ref(mandate_id: &str) -> String {
    format!("dec-mandate-{mandate_id}")
}
pub fn charter_decision_ref(charter_revision_id: &str) -> String {
    format!("dec-charter-{charter_revision_id}")
}

/// The typed refusal when no immutable, current decision resolves.
pub fn decision_incomplete(detail: &str) -> Problem {
    Problem::new(
        ProblemKind::DecisionIncomplete,
        "the operation cites no resolvable current GovernanceDecision",
    )
    .with_status(409)
    .with_detail(detail.to_owned())
}

/// One seat of a decision's slot snapshot.
#[derive(Debug, Clone)]
pub struct DecisionSeat {
    pub seat_ref: String,
    pub participant_ref: String,
    pub actor_ref: String,
    pub participant_binding_epoch: u64,
}

fn seats_json(seats: &[DecisionSeat]) -> Value {
    Value::Array(
        seats
            .iter()
            .map(|s| {
                json!({
                    "seat_ref": s.seat_ref,
                    "participant_ref": s.participant_ref,
                    "actor_ref": s.actor_ref,
                    "participant_binding_epoch": s.participant_binding_epoch,
                })
            })
            .collect(),
    )
}

/// The dependency closure a decision is formed under and stays current
/// against: the Society's charter head, classification binding,
/// endpoint incarnation and recovery epoch.
pub fn dependency_closure(conn: &Connection, society_id: &str) -> Result<Value, Problem> {
    let society = rows::get_society(conn, society_id)
        .map_err(crate::gov_ops::db_err)?
        .ok_or_else(state::not_found)?;
    let incarnation = byom_store::schema::meta_get_text(conn, "endpoint_incarnation")
        .map_err(|e| state::internal(&e.to_string()))?
        .unwrap_or_default();
    Ok(json!({
        "charter_head_ref": society.charter_head_ref,
        "charter_head_digest":
            serde_json::from_str::<Value>(&society.charter_head_digest).unwrap_or(Value::Null),
        "classification_binding_ref": society.classification_binding_ref,
        "classification_binding_digest":
            serde_json::from_str::<Value>(&society.classification_binding_digest)
                .unwrap_or(Value::Null),
        "society_recovery_epoch": society.recovery_epoch,
        "endpoint_incarnation": incarnation,
    }))
}

/// Forms ONE immutable GovernanceDecision, returning its effect. The
/// record digest is minted under a random per-object secret (D-R1-2), so
/// any later alteration of the stored row fails resolution.
#[allow(clippy::too_many_arguments)]
pub fn form(
    conn: &Connection,
    decision_id: &str,
    society_id: &str,
    kind: &str,
    subject_kind: &str,
    subject_ref: &str,
    subject_digest: &DigestRef,
    rule_set_ref: &str,
    seats: &[DecisionSeat],
    position_refs: &[String],
    source: &str,
    actor_ref: &str,
    now: i64,
) -> Result<Effect, Problem> {
    if rows::get_row(conn, "governance_decisions", "decision_id", decision_id)
        .map_err(crate::gov_ops::db_err)?
        .is_some()
    {
        // Decisions never change: a second formation under one id is a
        // rebinding attempt, not an update.
        return Err(state::stale_binding(
            "a GovernanceDecision already exists for this subject",
        ));
    }
    let created_at = rfc3339_utc(now);
    let snapshot = seats_json(seats);
    let closure = dependency_closure(conn, society_id)?;
    let record = record_value(
        decision_id,
        society_id,
        kind,
        subject_kind,
        subject_ref,
        subject_digest,
        rule_set_ref,
        &snapshot,
        position_refs,
        source,
        actor_ref,
        &closure,
        &created_at,
    );
    let digest = conn_record_digest(
        conn,
        society_id,
        decision_id,
        "bpp-governance-decision-v0",
        &record,
    )?;
    Ok(Effect::Upsert {
        table: "governance_decisions".into(),
        row: obj_pairs([
            ("decision_id", json!(decision_id)),
            ("society_id", json!(society_id)),
            ("kind", json!(kind)),
            ("subject_kind", json!(subject_kind)),
            ("subject_ref", json!(subject_ref)),
            (
                "subject_digest",
                serde_json::to_value(subject_digest).unwrap_or(Value::Null),
            ),
            ("rule_set_ref", json!(rule_set_ref)),
            ("seat_snapshot", json!(snapshot.to_string())),
            (
                "position_refs",
                json!(json!(position_refs.to_vec()).to_string()),
            ),
            ("source", json!(source)),
            (
                "digest",
                serde_json::to_value(&digest).unwrap_or(Value::Null),
            ),
            ("created_at", json!(created_at)),
            ("actor_ref", json!(actor_ref)),
            ("dependency_closure", json!(closure.to_string())),
        ]),
    })
}

#[allow(clippy::too_many_arguments)]
fn record_value(
    decision_id: &str,
    society_id: &str,
    kind: &str,
    subject_kind: &str,
    subject_ref: &str,
    subject_digest: &DigestRef,
    rule_set_ref: &str,
    snapshot: &Value,
    position_refs: &[String],
    source: &str,
    actor_ref: &str,
    closure: &Value,
    created_at: &str,
) -> Value {
    json!({
        "decision_id": decision_id,
        "society_id": society_id,
        "kind": kind,
        "subject_kind": subject_kind,
        "subject_ref": subject_ref,
        "subject_digest": serde_json::to_value(subject_digest).unwrap_or(Value::Null),
        "rule_set_ref": rule_set_ref,
        "seat_snapshot": snapshot,
        "position_refs": position_refs.to_vec(),
        "source": source,
        "actor_ref": actor_ref,
        "dependency_closure": closure,
        "created_at": created_at,
    })
}

/// What one resolution must match.
pub struct Expect<'a> {
    pub society_id: &'a str,
    pub kind: &'a str,
    pub subject_kind: &'a str,
    pub subject_ref: &'a str,
    /// The subject digest AS IT STANDS NOW (complete canonical
    /// `DigestRef` — BY-D1).
    pub subject_digest: &'a DigestRef,
    /// The channel-derived actor performing the act.
    pub actor: &'a str,
}

/// Resolves the cited decision transactionally. Every failure is the
/// typed `decision_incomplete` refusal — the mutation is never prepared.
pub fn resolve(
    conn: &Connection,
    decision_ref: &str,
    expect: &Expect<'_>,
) -> Result<Map<String, Value>, Problem> {
    let Some(row) = rows::get_row(conn, "governance_decisions", "decision_id", decision_ref)
        .map_err(crate::gov_ops::db_err)?
    else {
        return Err(decision_incomplete(
            "no GovernanceDecision exists for the cited reference",
        ));
    };
    if rows::str_of(&row, "society_id") != expect.society_id {
        return Err(decision_incomplete("decision belongs to another Society"));
    }
    if rows::str_of(&row, "kind") != expect.kind {
        return Err(decision_incomplete(
            "decision does not decide this kind of act",
        ));
    }
    if rows::str_of(&row, "subject_kind") != expect.subject_kind
        || rows::str_of(&row, "subject_ref") != expect.subject_ref
    {
        return Err(decision_incomplete("decision has a different subject"));
    }
    if !expect
        .subject_digest
        .same_ref_json(&rows::json_of(&row, "subject_digest"))
    {
        return Err(decision_incomplete(
            "decision subject digest is not the subject as it stands now",
        ));
    }
    // The acting actor must hold a seat in the decision's snapshot, at
    // the participant's CURRENT binding epoch.
    let snapshot: Value =
        serde_json::from_str(rows::str_of(&row, "seat_snapshot")).unwrap_or(Value::Null);
    let seated = snapshot.as_array().into_iter().flatten().any(|seat| {
        if seat["actor_ref"].as_str() != Some(expect.actor) {
            return false;
        }
        let participant_ref = seat["participant_ref"].as_str().unwrap_or_default();
        match rows::get_participant(conn, participant_ref) {
            Ok(Some(p)) => {
                p.state == "active"
                    && seat["participant_binding_epoch"].as_u64() == Some(p.binding_epoch)
            }
            _ => false,
        }
    });
    if !seated {
        return Err(decision_incomplete(
            "the acting actor holds no current seat in the decision's slot snapshot",
        ));
    }
    // The dependency closure must still be the Society's current one.
    let recorded: Value =
        serde_json::from_str(rows::str_of(&row, "dependency_closure")).unwrap_or(Value::Null);
    let current = dependency_closure(conn, expect.society_id)?;
    if recorded != current {
        return Err(decision_incomplete(
            "the decision's dependency closure is no longer current",
        ));
    }
    // Immutability: the record digest re-derives from the retained
    // per-object secret, so an altered stored row cannot resolve.
    let record = record_value(
        decision_ref,
        rows::str_of(&row, "society_id"),
        rows::str_of(&row, "kind"),
        rows::str_of(&row, "subject_kind"),
        rows::str_of(&row, "subject_ref"),
        &serde_json::from_value(rows::json_of(&row, "subject_digest"))
            .map_err(|_| decision_incomplete("decision subject digest is not canonical"))?,
        rows::str_of(&row, "rule_set_ref"),
        &snapshot,
        &serde_json::from_str::<Vec<String>>(rows::str_of(&row, "position_refs"))
            .unwrap_or_default(),
        rows::str_of(&row, "source"),
        rows::str_of(&row, "actor_ref"),
        &recorded,
        rows::str_of(&row, "created_at"),
    );
    let recomputed = conn_record_digest(
        conn,
        expect.society_id,
        decision_ref,
        "bpp-governance-decision-v0",
        &record,
    )?;
    if !recomputed.same_ref_json(&rows::json_of(&row, "digest")) {
        return Err(decision_incomplete(
            "the stored GovernanceDecision does not re-derive its own digest",
        ));
    }
    Ok(row)
}
