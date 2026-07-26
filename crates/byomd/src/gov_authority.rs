//! Governance-surface authority mutations of the attached slice:
//! `mandate_issue` (with the §11.4 budget reservation), `mandate_hold`,
//! `mandate_revoke`, the governance-side `mandate_position`/
//! `charter_position` seats (the sovereign human filling its prepared
//! seat, §14.5), and `charter_finalize` (§6.2 adoption).

use bpp_core::ops;
use bpp_core::problem::Problem;
use bpp_core::time::rfc3339_utc;
use byom_store::effects::Effect;
use byom_store::rows;
use byom_store::{CrashHooks, CursorMint, MutationScope, Prepared, Store};
use serde_json::{json, Value};

use crate::gov_ops::{check_meta_binding, db_err, mint, obj_pairs, run, ACTOR_GOVERNANCE};
use crate::part_common::{
    self, all_seats_assent, digest_of, mint_position, record_position, seats_from_json,
    settle_holder,
};
use crate::part_ops::{event, expire_mandate_if_due};
use crate::{gov_decision, state};

/// The sovereign human behind the governance surface (developer profile:
/// the same-UID peer is the sovereign; §14.5).
pub fn sovereign(store: &Store) -> Result<(String, rows::ParticipantRow), Problem> {
    let society = rows::sole_society(store.conn())
        .map_err(db_err)?
        .ok_or_else(state::not_found)?;
    let participant = rows::sovereign_participant(store.conn(), &society.society_id)
        .map_err(db_err)?
        .ok_or_else(state::not_found)?;
    Ok((society.society_id, participant))
}

/// mandate_position / charter_position on the GOVERNANCE surface: the
/// sovereign fills its prepared governance-surface seat.
pub fn governance_position(
    store: &mut Store,
    op: &str,
    req: &ops::PositionRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    let (society_id, sovereign_row) = sovereign(store)?;
    check_meta_binding(store, &req.meta, &society_id)?;
    let minted = mint_position(store)?;
    let scope = MutationScope {
        society_id: society_id.clone(),
        operation: op.to_owned(),
        actor: ACTOR_GOVERNANCE.into(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let op = op.to_owned();
    let req = req.clone();
    run(store, scope, now, hooks, move |conn, _| {
        let (kind, table, key, state_col) = if op == "mandate_position" {
            ("mandate", "mandates", "mandate_id", "state")
        } else {
            (
                "charter",
                "charter_proposals",
                "charter_proposal_id",
                "state",
            )
        };
        let proposal = rows::get_row(conn, table, key, &req.proposal_ref)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        let expected_state = if kind == "mandate" {
            "prepared"
        } else {
            "proposed"
        };
        if rows::str_of(&proposal, state_col) != expected_state {
            return Err(state::stale_binding("proposal is not positionable"));
        }
        let seats_col = if kind == "mandate" {
            "required_seat_refs"
        } else {
            "required_seats"
        };
        let seats = seats_from_json(&rows::json_of(&proposal, seats_col));
        let subject = rows::json_of(&proposal, "subject_digest");
        let (effects, result) = record_position(
            conn,
            &minted,
            kind,
            &society_id,
            rows::u64_of(&proposal, "revision"),
            &digest_of(&subject)?,
            &seats,
            &req,
            &sovereign_row.participant_id,
            ACTOR_GOVERNANCE,
            "governance",
            now,
        )?;
        let events = vec![event(
            &society_id,
            &minted.event_id,
            &format!("{kind}.position_recorded"),
            &req.proposal_ref,
            rows::u64_of(&proposal, "revision"),
            &sovereign_row.participant_id,
            ACTOR_GOVERNANCE,
            &req.meta,
            json!({"seat_ref": req.seat_ref, "value": req.value}),
        )];
        Ok(Prepared {
            result,
            revision: None,
            cursor: CursorMint::AfterEvents {
                society_id: society_id.clone(),
            },
            effects,
            events,
        })
    })
}

// -------------------------------------------------------- mandate_issue ----

pub fn mandate_issue(
    store: &mut Store,
    req: &ops::MandateIssueRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    let (society_id, sovereign_row) = sovereign(store)?;
    check_meta_binding(store, &req.meta, &society_id)?;
    expire_mandate_if_due(store, &req.mandate_id, now)?;
    // The mandate authority decision is IMMUTABLE and derived from the
    // mandate it authorizes (BY-A1): hold and revoke resolve it.
    let decision_ref = gov_decision::mandate_decision_ref(&req.mandate_id);
    let reservation_id = mint(store, "rsv")?;
    let issue_event = mint(store, "evt")?;
    let budget_event = mint(store, "evt")?;
    let issued_at = rfc3339_utc(now);
    let scope = MutationScope {
        society_id: society_id.clone(),
        operation: "mandate_issue".into(),
        actor: ACTOR_GOVERNANCE.into(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let req = req.clone();
    run(store, scope, now, hooks, move |conn, _| {
        let mandate = rows::get_row(conn, "mandates", "mandate_id", &req.mandate_id)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        if req.meta.expected_revision != Some(rows::u64_of(&mandate, "revision")) {
            return Err(state::stale_revision());
        }
        if rows::str_of(&mandate, "state") != "prepared" {
            return Err(state::stale_binding("mandate is not in state prepared"));
        }
        let subject = rows::json_of(&mandate, "subject_digest");
        if !req.subject_digest.same_ref_json(&subject) {
            return Err(state::invalid(
                "subject_digest does not commit to the exact prepared subject",
            ));
        }
        let seats = seats_from_json(&rows::json_of(&mandate, "required_seat_refs"));
        all_seats_assent(conn, "mandate", &req.mandate_id, &seats)?;
        let subject_ref: bpp_core::digest::DigestRef = digest_of(&subject)?;
        let decision_seats: Vec<gov_decision::DecisionSeat> = seats
            .iter()
            .map(|s| {
                let epoch = rows::get_participant(conn, &s.participant_ref)
                    .ok()
                    .flatten()
                    .map(|p| p.binding_epoch)
                    .unwrap_or(0);
                gov_decision::DecisionSeat {
                    seat_ref: s.seat_ref.clone(),
                    participant_ref: s.participant_ref.clone(),
                    actor_ref: ACTOR_GOVERNANCE.to_owned(),
                    participant_binding_epoch: epoch,
                }
            })
            .collect();
        let mandate_decision = gov_decision::form(
            conn,
            &decision_ref,
            &society_id,
            gov_decision::KIND_MANDATE_AUTHORITY,
            "mandate",
            &req.mandate_id,
            &subject_ref,
            rows::str_of(&mandate, "dependency_set_ref"),
            &decision_seats,
            &[],
            "mandate_issue",
            ACTOR_GOVERNANCE,
            now,
        )?;
        let grantee =
            rows::get_participant(conn, rows::str_of(&mandate, "grantee_participant_ref"))
                .map_err(db_err)?
                .ok_or_else(state::not_found)?;
        if grantee.state != "active" {
            return Err(state::stale_binding("grantee holds no active Standing"));
        }
        // BudgetConservation: issue reserves the mandate's ceiling in
        // the SAME transition (§10.1/§11.4) — never an unreserved grant.
        let account_ref = rows::str_of(&mandate, "budget_ceiling_set_ref").to_owned();
        let mut effects = vec![mandate_decision];
        part_common::reserve(
            conn,
            &mut effects,
            &society_id,
            &account_ref,
            part_common::UNIT_DIMENSION,
            part_common::MANDATE_CEILING,
            &reservation_id,
            "mandate",
            &req.mandate_id,
            now,
        )?;
        let revision = rows::u64_of(&mandate, "revision") + 1;
        let mut issued = mandate.clone();
        issued.insert("state".into(), json!("active"));
        issued.insert("revision".into(), json!(revision));
        issued.insert("issued_at".into(), json!(issued_at));
        issued.insert(
            "decision_refs".into(),
            json!(json!([decision_ref]).to_string()),
        );
        effects.push(Effect::Upsert {
            table: "mandates".into(),
            row: issued,
        });
        let events = vec![
            event(
                &society_id,
                &issue_event,
                "mandate.issued",
                &req.mandate_id,
                revision,
                &grantee.participant_id,
                ACTOR_GOVERNANCE,
                &req.meta,
                json!({"state": "active", "decision_ref": decision_ref,
                       "issued_at": issued_at}),
            ),
            event(
                &society_id,
                &budget_event,
                "budget.reserved",
                &account_ref,
                1,
                &sovereign_row.participant_id,
                ACTOR_GOVERNANCE,
                &req.meta,
                json!({"holder": req.mandate_id, "reservation": reservation_id,
                       "amount": part_common::MANDATE_CEILING,
                       "dimension": part_common::UNIT_DIMENSION}),
            ),
        ];
        Ok(Prepared {
            result: json!({
                "mandate_id": req.mandate_id,
                "revision": revision,
                "state": "active",
                "issued_at": issued_at,
                "decision_ref": decision_ref,
                "reservation_refs": [reservation_id],
            }),
            revision: Some(revision),
            cursor: CursorMint::AfterEvents {
                society_id: society_id.clone(),
            },
            effects,
            events,
        })
    })
}

// ------------------------------------------------- mandate hold/revoke ----

pub fn mandate_hold(
    store: &mut Store,
    req: &ops::MandateHoldRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    let (society_id, _sovereign_row) = sovereign(store)?;
    check_meta_binding(store, &req.meta, &society_id)?;
    let hold_event = mint(store, "evt")?;
    let scope = MutationScope {
        society_id: society_id.clone(),
        operation: "mandate_hold".into(),
        actor: ACTOR_GOVERNANCE.into(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let req = req.clone();
    run(store, scope, now, hooks, move |conn, _| {
        let mandate = rows::get_row(conn, "mandates", "mandate_id", &req.mandate_id)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        if req.meta.expected_revision != Some(rows::u64_of(&mandate, "revision")) {
            return Err(state::stale_revision());
        }
        if rows::str_of(&mandate, "state") != "active" {
            return Err(state::stale_binding("mandate is not active"));
        }
        // BY-A1: the hold resolves the immutable mandate-authority
        // decision formed at issue.
        gov_decision::resolve(
            conn,
            &req.held_by_decision_ref,
            &gov_decision::Expect {
                society_id: &society_id,
                kind: gov_decision::KIND_MANDATE_AUTHORITY,
                subject_kind: "mandate",
                subject_ref: &req.mandate_id,
                subject_digest: &digest_of(&rows::json_of(&mandate, "subject_digest"))?,
                actor: ACTOR_GOVERNANCE,
            },
        )?;
        let revision = rows::u64_of(&mandate, "revision") + 1;
        let mut held = mandate.clone();
        held.insert("state".into(), json!("held"));
        held.insert("revision".into(), json!(revision));
        held.insert(
            "held_by_decision_ref".into(),
            json!(req.held_by_decision_ref),
        );
        let grantee = rows::str_of(&mandate, "grantee_participant_ref").to_owned();
        Ok(Prepared {
            result: json!({
                "mandate_id": req.mandate_id,
                "revision": revision,
                "state": "held",
            }),
            revision: Some(revision),
            cursor: CursorMint::AfterEvents {
                society_id: society_id.clone(),
            },
            effects: vec![Effect::Upsert {
                table: "mandates".into(),
                row: held,
            }],
            events: vec![event(
                &society_id,
                &hold_event,
                "mandate.held",
                &req.mandate_id,
                revision,
                &grantee,
                ACTOR_GOVERNANCE,
                &req.meta,
                json!({"state": "held", "decision_ref": req.held_by_decision_ref}),
            )],
        })
    })
}

pub fn mandate_revoke(
    store: &mut Store,
    req: &ops::MandateRevokeRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    let (society_id, _sovereign_row) = sovereign(store)?;
    check_meta_binding(store, &req.meta, &society_id)?;
    let revoke_event = mint(store, "evt")?;
    let scope = MutationScope {
        society_id: society_id.clone(),
        operation: "mandate_revoke".into(),
        actor: ACTOR_GOVERNANCE.into(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let req = req.clone();
    run(store, scope, now, hooks, move |conn, _| {
        let mandate = rows::get_row(conn, "mandates", "mandate_id", &req.mandate_id)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        if req.meta.expected_revision != Some(rows::u64_of(&mandate, "revision")) {
            return Err(state::stale_revision());
        }
        if !matches!(
            rows::str_of(&mandate, "state"),
            "prepared" | "active" | "held"
        ) {
            return Err(state::stale_binding("mandate is already terminal"));
        }
        // BY-A1: the revocation resolves the same immutable decision.
        gov_decision::resolve(
            conn,
            &req.revoked_by_decision_ref,
            &gov_decision::Expect {
                society_id: &society_id,
                kind: gov_decision::KIND_MANDATE_AUTHORITY,
                subject_kind: "mandate",
                subject_ref: &req.mandate_id,
                subject_digest: &digest_of(&rows::json_of(&mandate, "subject_digest"))?,
                actor: ACTOR_GOVERNANCE,
            },
        )?;
        // Revocation releases the reserved ceiling in the same
        // transition (conservation holds).
        let mut effects = Vec::new();
        settle_holder(conn, &mut effects, "mandate", &req.mandate_id, false)?;
        let revision = rows::u64_of(&mandate, "revision") + 1;
        let mut revoked = mandate.clone();
        revoked.insert("state".into(), json!("revoked"));
        revoked.insert("revision".into(), json!(revision));
        revoked.insert(
            "revoked_by_decision_ref".into(),
            json!(req.revoked_by_decision_ref),
        );
        let grantee = rows::str_of(&mandate, "grantee_participant_ref").to_owned();
        effects.push(Effect::Upsert {
            table: "mandates".into(),
            row: revoked,
        });
        Ok(Prepared {
            result: json!({
                "mandate_id": req.mandate_id,
                "revision": revision,
                "state": "revoked",
            }),
            revision: Some(revision),
            cursor: CursorMint::AfterEvents {
                society_id: society_id.clone(),
            },
            effects,
            events: vec![event(
                &society_id,
                &revoke_event,
                "mandate.revoked",
                &req.mandate_id,
                revision,
                &grantee,
                ACTOR_GOVERNANCE,
                &req.meta,
                json!({"state": "revoked",
                       "decision_ref": req.revoked_by_decision_ref}),
            )],
        })
    })
}

// ----------------------------------------------------- charter_finalize ----

pub fn charter_finalize(
    store: &mut Store,
    req: &ops::CharterFinalizeRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    let (society_id, sovereign_row) = sovereign(store)?;
    check_meta_binding(store, &req.meta, &society_id)?;
    let revision_id = mint(store, "charter-r")?;
    let decision_ref = mint(store, "dec-charter")?;
    let adopt_event = mint(store, "evt")?;
    let adopted_at = rfc3339_utc(now);
    let scope = MutationScope {
        society_id: society_id.clone(),
        operation: "charter_finalize".into(),
        actor: ACTOR_GOVERNANCE.into(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let req = req.clone();
    run(store, scope, now, hooks, move |conn, _| {
        // The one live proposal under this charter id.
        let proposal = rows::rows_where(
            conn,
            "charter_proposals",
            "charter_id",
            &req.charter_id,
            "created_at",
        )
        .map_err(db_err)?
        .into_iter()
        .find(|p| rows::str_of(p, "state") == "proposed")
        .ok_or_else(state::not_found)?;
        if req.meta.expected_revision != Some(rows::u64_of(&proposal, "revision")) {
            return Err(state::stale_revision());
        }
        let subject = rows::json_of(&proposal, "subject_digest");
        if !req.subject_digest.same_ref_json(&subject) {
            return Err(state::invalid(
                "subject_digest does not commit to the exact proposed restatement",
            ));
        }
        let proposal_id = rows::str_of(&proposal, "charter_proposal_id").to_owned();
        let seats = seats_from_json(&rows::json_of(&proposal, "required_seats"));
        all_seats_assent(conn, "charter", &proposal_id, &seats)?;

        let society = rows::get_society(conn, &society_id)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        // The next charter revision number: one past the highest
        // adopted revision of this Society.
        let head_revision: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(revision), 0) FROM charter_revisions
                 WHERE society_id = ?1 AND state = 'adopted'",
                [&society_id],
                |r| r.get(0),
            )
            .map_err(db_err)?;
        let new_charter_revision = (head_revision as u64) + 1;
        let mut adopted_proposal = proposal.clone();
        adopted_proposal.insert("state".into(), json!("adopted"));
        adopted_proposal.insert(
            "revision".into(),
            json!(rows::u64_of(&proposal, "revision") + 1),
        );
        let mut society_row = crate::gov_ops::society_effect_row(&society);
        society_row.insert("revision".into(), json!(society.revision + 1));
        society_row.insert("charter_head_ref".into(), json!(proposal_id));
        society_row.insert("charter_head_digest".into(), subject.clone());
        let effects = vec![
            Effect::Upsert {
                table: "charter_proposals".into(),
                row: adopted_proposal,
            },
            Effect::Upsert {
                table: "charter_revisions".into(),
                row: obj_pairs([
                    ("charter_revision_id", json!(revision_id)),
                    ("society_id", json!(society_id)),
                    ("revision", json!(new_charter_revision)),
                    ("body_ref", json!(proposal_id)),
                    ("body_digest", subject.clone()),
                    ("state", json!("adopted")),
                    ("adopted_by_decision_ref", json!(decision_ref)),
                    ("created_at", json!(adopted_at)),
                    ("effective_at", json!(adopted_at)),
                ]),
            },
            Effect::Upsert {
                table: "societies".into(),
                row: society_row,
            },
        ];
        let events = vec![event(
            &society_id,
            &adopt_event,
            "charter.adopted",
            &revision_id,
            new_charter_revision,
            &sovereign_row.participant_id,
            ACTOR_GOVERNANCE,
            &req.meta,
            json!({"charter_id": req.charter_id, "body_ref": proposal_id,
                   "decision_ref": decision_ref,
                   "charter_revision": new_charter_revision}),
        )];
        Ok(Prepared {
            result: json!({
                "charter_id": req.charter_id,
                "charter_revision_id": revision_id,
                "revision": new_charter_revision,
                "state": "adopted",
                "adopted_by_decision_ref": decision_ref,
                "effective_at": adopted_at,
            }),
            revision: Some(new_charter_revision),
            cursor: CursorMint::AfterEvents {
                society_id: society_id.clone(),
            },
            effects,
            events,
        })
    })
}
