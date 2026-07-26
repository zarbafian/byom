//! Participant-surface governed work (§8–§9; registry R18–R28): the
//! endeavor lifecycle, calls, pledges under the RT-03 slot/seat
//! discipline with the D-RT-3 amendment successor split, deliveries
//! (pledgor-only, §20.1 classification honesty), reviews (exact reviewer
//! seat, budget settlement per §11.4 conservation), and the complete
//! charter restatement proposal (§6.2). Every handler drives ONE §15.3
//! authority transaction; dependency revalidation happens inside the
//! prepare closure against the open transaction.

use bpp_core::ops;
use bpp_core::problem::{Problem, ProblemKind};
use bpp_core::time::{parse_rfc3339_utc, rfc3339_utc};
use byom_store::effects::Effect;
use byom_store::rows;
use byom_store::{CrashHooks, CursorMint, MutationScope, Prepared, Store};
use serde_json::{json, Map, Value};

use crate::gov_ops::{check_meta_binding, db_err, digest_json, mint, obj_pairs, run};
use crate::part_common::{
    self, all_seats_assent, digest_of, mint_position, prepare_trace, record_position, reserve,
    seats_from_json, seats_json, settle_holder, source_row, Caller, Seat,
};
use crate::part_ops::event;
use crate::state;

fn opt_json(v: &Option<String>) -> Value {
    v.as_ref().map(|s| json!(s)).unwrap_or(Value::Null)
}

fn require_active_participant(
    conn: &rusqlite::Connection,
    participant_ref: &str,
) -> Result<rows::ParticipantRow, Problem> {
    let p = rows::get_participant(conn, participant_ref)
        .map_err(db_err)?
        .ok_or_else(state::not_found)?;
    if p.state != "active" {
        return Err(state::stale_binding("participant holds no active Standing"));
    }
    Ok(p)
}

// ------------------------------------------------------------ endeavor ----

pub fn endeavor_propose(
    store: &mut Store,
    caller: &Caller,
    req: &ops::EndeavorProposeRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    check_meta_binding(store, &req.meta, &caller.society_id)?;
    let endeavor_id = mint(store, "end")?;
    let dependency_set_ref = mint(store, "deps")?;
    let propose_event = mint(store, "evt")?;
    let mut seats = Vec::new();
    for sponsor in &req.sponsor_participant_refs {
        seats.push(Seat {
            seat_ref: mint(store, "seat-sponsor")?,
            kind: "sponsor".into(),
            participant_ref: sponsor.clone(),
            surface: "participant".into(),
        });
    }
    let created_at = rfc3339_utc(now);

    let subject = json!({
        "endeavor_id": endeavor_id,
        "purpose_ref": req.purpose_ref,
        "purpose_digest": digest_json(&req.purpose_digest),
        "sponsor_participant_refs": req.sponsor_participant_refs,
        "governance_rule_set_ref": req.governance_rule_set_ref,
        "outcome_schema_refs": req.outcome_schema_refs,
        "acceptance_rule_ref": req.acceptance_rule_ref,
        "classification_join_ref": req.classification_join_ref,
        "budget_account_set_ref": req.budget_account_set_ref,
        "deadline": opt_json(&req.deadline),
    });
    let (subject_digest, _secret) = store
        .mint_object_digest(
            &format!("society-key:{}/object:{endeavor_id}", caller.society_id),
            "bpp-endeavor-subject-v0",
            &subject,
        )
        .map_err(|e| state::internal(&e.to_string()))?;
    let rid = &req.meta.request_id;
    let field_sources = vec![
        source_row("/endeavor_id", rid, "/meta/request_id", "t-mint-id"),
        source_row("/purpose_ref", rid, "/purpose_ref", "t-copy"),
        source_row("/purpose_digest", rid, "/purpose_digest", "t-copy"),
        source_row(
            "/sponsor_participant_refs",
            rid,
            "/sponsor_participant_refs",
            "t-copy",
        ),
        source_row(
            "/governance_rule_set_ref",
            rid,
            "/governance_rule_set_ref",
            "t-copy",
        ),
        source_row(
            "/outcome_schema_refs",
            rid,
            "/outcome_schema_refs",
            "t-copy",
        ),
        source_row(
            "/acceptance_rule_ref",
            rid,
            "/acceptance_rule_ref",
            "t-copy",
        ),
        source_row(
            "/classification_join_ref",
            rid,
            "/classification_join_ref",
            "t-copy",
        ),
        source_row(
            "/budget_account_set_ref",
            rid,
            "/budget_account_set_ref",
            "t-copy",
        ),
        source_row("/deadline", rid, "/deadline", "t-copy"),
    ];
    let trace = prepare_trace(
        store,
        &caller.society_id,
        "endeavor_propose",
        &caller.actor,
        rid,
        body,
        &subject_digest,
        &dependency_set_ref,
        field_sources,
        now,
    )?;

    let scope = MutationScope {
        society_id: caller.society_id.clone(),
        operation: "endeavor_propose".into(),
        actor: caller.actor.clone(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let caller = caller.clone();
    let req = req.clone();
    run(store, scope, now, hooks, move |conn, _| {
        // Every named sponsor must hold Standing, and the proposer must
        // be one of them (an endeavor is sponsored, never planted).
        for sponsor in &req.sponsor_participant_refs {
            require_active_participant(conn, sponsor)?;
        }
        if !req
            .sponsor_participant_refs
            .contains(&caller.participant.participant_id)
        {
            return Err(state::forbidden_detail(
                "the proposer must be a named sponsor",
            ));
        }
        if let Some(deadline) = &req.deadline {
            if parse_rfc3339_utc(deadline).is_some_and(|t| t <= now) {
                return Err(state::invalid("deadline is already past"));
            }
        }
        let effects = vec![Effect::Upsert {
            table: "endeavors".into(),
            row: obj_pairs([
                ("endeavor_id", json!(endeavor_id)),
                ("society_id", json!(caller.society_id)),
                ("revision", json!(1)),
                ("state", json!("proposed")),
                ("purpose_ref", json!(req.purpose_ref)),
                ("purpose_digest", digest_json(&req.purpose_digest)),
                (
                    "sponsor_participant_refs",
                    json!(json!(req.sponsor_participant_refs).to_string()),
                ),
                (
                    "governance_rule_set_ref",
                    json!(req.governance_rule_set_ref),
                ),
                (
                    "outcome_schema_refs",
                    json!(json!(req.outcome_schema_refs).to_string()),
                ),
                ("acceptance_rule_ref", json!(req.acceptance_rule_ref)),
                (
                    "classification_join_ref",
                    json!(req.classification_join_ref),
                ),
                ("budget_account_set_ref", json!(req.budget_account_set_ref)),
                ("deadline", opt_json(&req.deadline)),
                ("subject_digest", digest_json(&subject_digest)),
                ("required_seats", json!(seats_json(&seats).to_string())),
                ("preparation_trace", json!(trace.to_string())),
                ("formation_decision_ref", Value::Null),
                ("created_at", json!(created_at)),
            ]),
        }];
        let events = vec![event(
            &caller.society_id,
            &propose_event,
            "endeavor.proposed",
            &endeavor_id,
            1,
            &caller.participant.participant_id,
            &caller.actor,
            &req.meta,
            json!({"state": "proposed", "purpose_ref": req.purpose_ref}),
        )];
        Ok(Prepared {
            result: json!({
                "endeavor_id": endeavor_id,
                "revision": 1,
                "state": "proposed",
                "subject_digest": digest_json(&subject_digest),
                "required_seat_refs":
                    seats.iter().map(|s| s.seat_ref.clone()).collect::<Vec<_>>(),
                "dependency_set_ref": dependency_set_ref,
                "created_at": created_at,
                "preparation_trace": trace,
            }),
            revision: Some(1),
            cursor: CursorMint::AfterEvents {
                society_id: caller.society_id.clone(),
            },
            effects,
            events,
        })
    })
}

/// endeavor_position (participant, create): the RT-03 sponsor-seat
/// discipline over the prepared endeavor subject.
pub fn endeavor_position(
    store: &mut Store,
    caller: &Caller,
    req: &ops::PositionRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    check_meta_binding(store, &req.meta, &caller.society_id)?;
    let minted = mint_position(store)?;
    let scope = MutationScope {
        society_id: caller.society_id.clone(),
        operation: "endeavor_position".into(),
        actor: caller.actor.clone(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let caller = caller.clone();
    let req = req.clone();
    run(store, scope, now, hooks, move |conn, _| {
        let endeavor = rows::get_row(conn, "endeavors", "endeavor_id", &req.proposal_ref)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        if rows::str_of(&endeavor, "state") != "proposed" {
            return Err(state::stale_binding("endeavor is not in state proposed"));
        }
        let seats = seats_from_json(&rows::json_of(&endeavor, "required_seats"));
        let subject = rows::json_of(&endeavor, "subject_digest");
        let (effects, result) = record_position(
            conn,
            &minted,
            "endeavor",
            &caller.society_id,
            rows::u64_of(&endeavor, "revision"),
            &digest_of(&subject)?,
            &seats,
            &req,
            &caller.participant.participant_id,
            &caller.actor,
            "participant",
            now,
        )?;
        let events = vec![event(
            &caller.society_id,
            &minted.event_id,
            "endeavor.position_recorded",
            &req.proposal_ref,
            rows::u64_of(&endeavor, "revision"),
            &caller.participant.participant_id,
            &caller.actor,
            &req.meta,
            json!({"seat_ref": req.seat_ref, "value": req.value}),
        )];
        Ok(Prepared {
            result,
            revision: None,
            cursor: CursorMint::AfterEvents {
                society_id: caller.society_id.clone(),
            },
            effects,
            events,
        })
    })
}

pub fn endeavor_finalize(
    store: &mut Store,
    caller: &Caller,
    req: &ops::EndeavorFinalizeRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    check_meta_binding(store, &req.meta, &caller.society_id)?;
    let decision_ref = mint(store, "dec-endeavor")?;
    let finalize_event = mint(store, "evt")?;
    let budget_event = mint(store, "evt")?;
    let scope = MutationScope {
        society_id: caller.society_id.clone(),
        operation: "endeavor_finalize".into(),
        actor: caller.actor.clone(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let caller = caller.clone();
    let req = req.clone();
    run(store, scope, now, hooks, move |conn, _| {
        let endeavor = rows::get_row(conn, "endeavors", "endeavor_id", &req.endeavor_id)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        if req.meta.expected_revision != Some(rows::u64_of(&endeavor, "revision")) {
            return Err(state::stale_revision());
        }
        if rows::str_of(&endeavor, "state") != "proposed" {
            return Err(state::stale_binding("endeavor is not in state proposed"));
        }
        let subject = rows::json_of(&endeavor, "subject_digest");
        if !req.subject_digest.same_ref_json(&subject) {
            return Err(state::invalid(
                "subject_digest does not commit to the exact prepared subject",
            ));
        }
        let seats = seats_from_json(&rows::json_of(&endeavor, "required_seats"));
        all_seats_assent(conn, "endeavor", &req.endeavor_id, &seats)?;
        // Formation delegates the endeavor's budget ceiling from the
        // Society root in the same transition (§11.4 conservation).
        let society = rows::get_society(conn, &caller.society_id)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        let budget_ref = rows::str_of(&endeavor, "budget_account_set_ref").to_owned();
        let mut effects = Vec::new();
        part_common::delegate_child(
            conn,
            &mut effects,
            &caller.society_id,
            &society.root_budget_account_set_ref,
            &budget_ref,
            part_common::UNIT_DIMENSION,
            part_common::ENDEAVOR_CEILING,
            now,
        )?;
        let revision = rows::u64_of(&endeavor, "revision") + 1;
        let mut active = endeavor.clone();
        active.insert("state".into(), json!("active"));
        active.insert("revision".into(), json!(revision));
        active.insert("formation_decision_ref".into(), json!(decision_ref));
        effects.push(Effect::Upsert {
            table: "endeavors".into(),
            row: active,
        });
        let events = vec![
            event(
                &caller.society_id,
                &finalize_event,
                "endeavor.finalized",
                &req.endeavor_id,
                revision,
                &caller.participant.participant_id,
                &caller.actor,
                &req.meta,
                json!({"state": "active", "decision_ref": decision_ref}),
            ),
            event(
                &caller.society_id,
                &budget_event,
                "budget.delegated",
                &budget_ref,
                1,
                &caller.participant.participant_id,
                &caller.actor,
                &req.meta,
                json!({"parent": society.root_budget_account_set_ref,
                       "ceiling": part_common::ENDEAVOR_CEILING,
                       "dimension": part_common::UNIT_DIMENSION}),
            ),
        ];
        Ok(Prepared {
            result: json!({
                "endeavor_id": req.endeavor_id,
                "revision": revision,
                "state": "active",
                "formation_decision_ref": decision_ref,
                "budget_account_set_ref": budget_ref,
            }),
            revision: Some(revision),
            cursor: CursorMint::AfterEvents {
                society_id: caller.society_id.clone(),
            },
            effects,
            events,
        })
    })
}

/// endeavor_hold / endeavor_release (participant, update).
pub fn endeavor_hold_release(
    store: &mut Store,
    caller: &Caller,
    op: &str,
    req: &ops::EndeavorHoldRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    check_meta_binding(store, &req.meta, &caller.society_id)?;
    let hold_event = mint(store, "evt")?;
    let scope = MutationScope {
        society_id: caller.society_id.clone(),
        operation: op.to_owned(),
        actor: caller.actor.clone(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let caller = caller.clone();
    let op = op.to_owned();
    let req = req.clone();
    run(store, scope, now, hooks, move |conn, _| {
        let endeavor = rows::get_row(conn, "endeavors", "endeavor_id", &req.endeavor_id)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        if req.meta.expected_revision != Some(rows::u64_of(&endeavor, "revision")) {
            return Err(state::stale_revision());
        }
        sponsor_only(&endeavor, &caller)?;
        let (from, to) = if op == "endeavor_hold" {
            ("active", "held")
        } else {
            ("held", "active")
        };
        if rows::str_of(&endeavor, "state") != from {
            return Err(state::stale_binding("closed endeavor transition"));
        }
        let revision = rows::u64_of(&endeavor, "revision") + 1;
        let mut moved = endeavor.clone();
        moved.insert("state".into(), json!(to));
        moved.insert("revision".into(), json!(revision));
        Ok(Prepared {
            result: json!({
                "endeavor_id": req.endeavor_id,
                "revision": revision,
                "state": to,
            }),
            revision: Some(revision),
            cursor: CursorMint::AfterEvents {
                society_id: caller.society_id.clone(),
            },
            effects: vec![Effect::Upsert {
                table: "endeavors".into(),
                row: moved,
            }],
            events: vec![event(
                &caller.society_id,
                &hold_event,
                if op == "endeavor_hold" {
                    "endeavor.held"
                } else {
                    "endeavor.released"
                },
                &req.endeavor_id,
                revision,
                &caller.participant.participant_id,
                &caller.actor,
                &req.meta,
                json!({"state": to,
                       "reason_ref": opt_json(&req.hold_reason_ref)
                           .as_str().map(str::to_owned)
                           .or(req.release_reason_ref.clone())}),
            )],
        })
    })
}

fn sponsor_only(endeavor: &Map<String, Value>, caller: &Caller) -> Result<(), Problem> {
    let sponsors = rows::json_of(endeavor, "sponsor_participant_refs");
    let is_sponsor = sponsors.as_array().is_some_and(|a| {
        a.iter()
            .any(|v| v.as_str() == Some(caller.participant.participant_id.as_str()))
    });
    if is_sponsor {
        Ok(())
    } else {
        Err(state::forbidden_detail(
            "only a named sponsor steers this endeavor",
        ))
    }
}

pub fn endeavor_close(
    store: &mut Store,
    caller: &Caller,
    req: &ops::EndeavorCloseRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    check_meta_binding(store, &req.meta, &caller.society_id)?;
    let close_event = mint(store, "evt")?;
    let scope = MutationScope {
        society_id: caller.society_id.clone(),
        operation: "endeavor_close".into(),
        actor: caller.actor.clone(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let caller = caller.clone();
    let req = req.clone();
    run(store, scope, now, hooks, move |conn, _| {
        let endeavor = rows::get_row(conn, "endeavors", "endeavor_id", &req.endeavor_id)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        if req.meta.expected_revision != Some(rows::u64_of(&endeavor, "revision")) {
            return Err(state::stale_revision());
        }
        sponsor_only(&endeavor, &caller)?;
        let current = rows::str_of(&endeavor, "state");
        let allowed = match req.target_state.as_str() {
            "reviewing" => matches!(current, "active" | "held"),
            _ => matches!(current, "active" | "held" | "reviewing"),
        };
        if !allowed {
            return Err(state::stale_binding("closed endeavor transition"));
        }
        let revision = rows::u64_of(&endeavor, "revision") + 1;
        let mut closed = endeavor.clone();
        closed.insert("state".into(), json!(req.target_state));
        closed.insert("revision".into(), json!(revision));
        Ok(Prepared {
            result: json!({
                "endeavor_id": req.endeavor_id,
                "revision": revision,
                "state": req.target_state,
            }),
            revision: Some(revision),
            cursor: CursorMint::AfterEvents {
                society_id: caller.society_id.clone(),
            },
            effects: vec![Effect::Upsert {
                table: "endeavors".into(),
                row: closed,
            }],
            events: vec![event(
                &caller.society_id,
                &close_event,
                "endeavor.closed",
                &req.endeavor_id,
                revision,
                &caller.participant.participant_id,
                &caller.actor,
                &req.meta,
                json!({"state": req.target_state,
                       "closure_decision_ref": opt_json(&req.closure_decision_ref)}),
            )],
        })
    })
}

// ---------------------------------------------------------------- call ----

pub fn call_open(
    store: &mut Store,
    caller: &Caller,
    req: &ops::CallOpenRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    check_meta_binding(store, &req.meta, &caller.society_id)?;
    let call_id = mint(store, "call")?;
    let open_event = mint(store, "evt")?;
    let created_at = rfc3339_utc(now);
    let call_body = json!({
        "requested_outcome_schema_refs": req.requested_outcome_schema_refs,
        "acceptance_criteria_refs": req.acceptance_criteria_refs,
        "evidence_requirements": req.evidence_requirements,
        "context_ceiling_ref": opt_json(&req.context_ceiling_ref),
        "budget_ceiling_ref": opt_json(&req.budget_ceiling_ref),
        "eligible_participant_selector":
            req.eligible_participant_selector.clone().unwrap_or(Value::Null),
        "deadline": opt_json(&req.deadline),
        "disclosure_ceiling_ref": opt_json(&req.disclosure_ceiling_ref),
    });
    let call_digest = store
        .record_digest(&caller.society_id, &call_id, "bpp-call-v0", &call_body)
        .map_err(|e| state::internal(&e.to_string()))?;
    let scope = MutationScope {
        society_id: caller.society_id.clone(),
        operation: "call_open".into(),
        actor: caller.actor.clone(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let caller = caller.clone();
    let req = req.clone();
    run(store, scope, now, hooks, move |conn, _| {
        let endeavor = rows::get_row(conn, "endeavors", "endeavor_id", &req.endeavor_id)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        if rows::str_of(&endeavor, "state") != "active" {
            return Err(state::stale_binding("endeavor is not active"));
        }
        sponsor_only(&endeavor, &caller)?;
        if let Some(deadline) = &req.deadline {
            if parse_rfc3339_utc(deadline).is_some_and(|t| t <= now) {
                return Err(state::invalid("deadline is already past"));
            }
        }
        let effects = vec![Effect::Upsert {
            table: "calls".into(),
            row: obj_pairs([
                ("call_id", json!(call_id)),
                ("society_id", json!(caller.society_id)),
                ("endeavor_ref", json!(req.endeavor_id)),
                ("revision", json!(1)),
                ("state", json!("open")),
                ("opened_by", json!(caller.participant.participant_id)),
                ("body", json!(call_body.to_string())),
                ("digest", digest_json(&call_digest)),
                ("created_at", json!(created_at)),
            ]),
        }];
        let events = vec![event(
            &caller.society_id,
            &open_event,
            "call.opened",
            &call_id,
            1,
            &caller.participant.participant_id,
            &caller.actor,
            &req.meta,
            json!({"endeavor_ref": req.endeavor_id, "state": "open"}),
        )];
        Ok(Prepared {
            result: json!({
                "call_id": call_id,
                "endeavor_id": req.endeavor_id,
                "revision": 1,
                "state": "open",
                "digest": digest_json(&call_digest),
                "created_at": created_at,
            }),
            revision: Some(1),
            cursor: CursorMint::AfterEvents {
                society_id: caller.society_id.clone(),
            },
            effects,
            events,
        })
    })
}

pub fn call_withdraw(
    store: &mut Store,
    caller: &Caller,
    req: &ops::CallWithdrawRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    check_meta_binding(store, &req.meta, &caller.society_id)?;
    let withdraw_event = mint(store, "evt")?;
    let scope = MutationScope {
        society_id: caller.society_id.clone(),
        operation: "call_withdraw".into(),
        actor: caller.actor.clone(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let caller = caller.clone();
    let req = req.clone();
    run(store, scope, now, hooks, move |conn, _| {
        let call = rows::get_row(conn, "calls", "call_id", &req.call_id)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        if req.meta.expected_revision != Some(rows::u64_of(&call, "revision")) {
            return Err(state::stale_revision());
        }
        // Opener only (§9.2): the call is not another participant's to
        // withdraw.
        if rows::str_of(&call, "opened_by") != caller.participant.participant_id {
            return Err(state::forbidden());
        }
        if rows::str_of(&call, "state") != "open" {
            return Err(state::stale_binding("call is not open"));
        }
        let revision = rows::u64_of(&call, "revision") + 1;
        let mut withdrawn = call.clone();
        withdrawn.insert("state".into(), json!("withdrawn"));
        withdrawn.insert("revision".into(), json!(revision));
        Ok(Prepared {
            result: json!({
                "call_id": req.call_id,
                "revision": revision,
                "state": "withdrawn",
            }),
            revision: Some(revision),
            cursor: CursorMint::AfterEvents {
                society_id: caller.society_id.clone(),
            },
            effects: vec![Effect::Upsert {
                table: "calls".into(),
                row: withdrawn,
            }],
            events: vec![event(
                &caller.society_id,
                &withdraw_event,
                "call.withdrawn",
                &req.call_id,
                revision,
                &caller.participant.participant_id,
                &caller.actor,
                &req.meta,
                json!({"state": "withdrawn"}),
            )],
        })
    })
}

// -------------------------------------------------------------- pledge ----

/// The serialized pledge terms (propose and amend share this shape); the
/// terms digest commits to exactly these members.
#[allow(clippy::too_many_arguments)]
fn pledge_terms(
    pledgor_ref: &str,
    beneficiary_ref: &str,
    exact_outcome_schema_refs: &[String],
    acceptance_criteria_refs: &[String],
    evidence_requirements: &[String],
    reviewer_rule_ref: &str,
    input_context_ref: &str,
    input_context_digest: Value,
    budget_request_set: Value,
    disclosure_manifest_ref: &Option<String>,
    allowed_manifestation_selector: &Value,
    delegation_ceiling: Value,
    deadline: &str,
    cancellation_terms: Value,
    dependency_refs: &[String],
) -> Value {
    json!({
        "pledgor_ref": pledgor_ref,
        "beneficiary_ref": beneficiary_ref,
        "exact_outcome_schema_refs": exact_outcome_schema_refs,
        "acceptance_criteria_refs": acceptance_criteria_refs,
        "evidence_requirements": evidence_requirements,
        "reviewer_rule_ref": reviewer_rule_ref,
        "input_context_ref": input_context_ref,
        "input_context_digest": input_context_digest,
        "budget_request_set": budget_request_set,
        "disclosure_manifest_ref": opt_json(disclosure_manifest_ref),
        "allowed_manifestation_selector": allowed_manifestation_selector,
        "delegation_ceiling": delegation_ceiling,
        "deadline": deadline,
        "cancellation_terms": cancellation_terms,
        "dependency_refs": dependency_refs,
    })
}

/// The RT-03 slot records plus the concrete seats of one pledge
/// proposal: one `pledgor_assent` slot (the proposed pledgor, filled on
/// the participant surface) and one `beneficiary_assent` slot. Every
/// slot's subject digest is the terms digest — a seat can never collect
/// assent on another subject.
struct PledgeSlots {
    slots: Value,
    seats: Vec<Seat>,
}

fn pledge_slots(
    store: &Store,
    pledgor_ref: &str,
    beneficiary_ref: &str,
    terms_digest: &Value,
) -> Result<PledgeSlots, Problem> {
    let pledgor_seat = mint(store, "seat-pledgor")?;
    let beneficiary_seat = mint(store, "seat-beneficiary")?;
    let seats = vec![
        Seat {
            seat_ref: pledgor_seat.clone(),
            kind: "pledgor_assent".into(),
            participant_ref: pledgor_ref.to_owned(),
            surface: "participant".into(),
        },
        Seat {
            seat_ref: beneficiary_seat.clone(),
            kind: "beneficiary_assent".into(),
            participant_ref: beneficiary_ref.to_owned(),
            surface: "participant".into(),
        },
    ];
    let slots = json!([
        {
            "kind": "pledgor_assent",
            "multiplicity": 1,
            "seat_refs": [pledgor_seat],
            "subject_digest": terms_digest,
        },
        {
            "kind": "beneficiary_assent",
            "multiplicity": 1,
            "seat_refs": [beneficiary_seat],
            "subject_digest": terms_digest,
        },
    ]);
    Ok(PledgeSlots { slots, seats })
}

fn slots_store_json(slots: &Value, seats: &[Seat]) -> String {
    json!({"slots": slots, "seats": seats_json(seats)}).to_string()
}

pub fn pledge_propose(
    store: &mut Store,
    caller: &Caller,
    req: &ops::PledgeProposeRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    check_meta_binding(store, &req.meta, &caller.society_id)?;
    let proposal_id = mint(store, "plg-prop")?;
    let dependency_set_ref = mint(store, "deps")?;
    let propose_event = mint(store, "evt")?;
    let created_at = rfc3339_utc(now);
    let terms = pledge_terms(
        &req.proposed_pledgor_ref,
        &req.beneficiary_ref,
        &req.exact_outcome_schema_refs,
        &req.acceptance_criteria_refs,
        &req.evidence_requirements,
        &req.reviewer_rule_ref,
        &req.input_context_ref,
        digest_json(&req.input_context_digest),
        serde_json::to_value(&req.budget_request_set).unwrap_or(Value::Null),
        &req.disclosure_manifest_ref,
        &req.allowed_manifestation_selector,
        serde_json::to_value(&req.delegation_ceiling).unwrap_or(Value::Null),
        &req.deadline,
        serde_json::to_value(&req.cancellation_terms).unwrap_or(Value::Null),
        &req.dependency_refs,
    );
    let (terms_digest, _secret) = store
        .mint_object_digest(
            &format!("society-key:{}/object:{proposal_id}", caller.society_id),
            "bpp-pledge-terms-v0",
            &terms,
        )
        .map_err(|e| state::internal(&e.to_string()))?;
    let terms_digest_json = digest_json(&terms_digest);
    let slots = pledge_slots(
        store,
        &req.proposed_pledgor_ref,
        &req.beneficiary_ref,
        &terms_digest_json,
    )?;
    let rid = &req.meta.request_id;
    let trace = prepare_trace(
        store,
        &caller.society_id,
        "pledge_propose",
        &caller.actor,
        rid,
        body,
        &terms_digest,
        &dependency_set_ref,
        vec![
            source_row("/proposal_id", rid, "/meta/request_id", "t-mint-id"),
            source_row("/terms", rid, "", "t-copy-typed-terms"),
        ],
        now,
    )?;
    let scope = MutationScope {
        society_id: caller.society_id.clone(),
        operation: "pledge_propose".into(),
        actor: caller.actor.clone(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let caller = caller.clone();
    let req = req.clone();
    run(store, scope, now, hooks, move |conn, _| {
        let endeavor = rows::get_row(conn, "endeavors", "endeavor_id", &req.endeavor_id)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        if rows::str_of(&endeavor, "state") != "active" {
            return Err(state::stale_binding("endeavor is not active"));
        }
        if let Some(call_ref) = &req.call_ref {
            let call = rows::get_row(conn, "calls", "call_id", call_ref)
                .map_err(db_err)?
                .ok_or_else(state::not_found)?;
            if rows::str_of(&call, "endeavor_ref") != req.endeavor_id {
                return Err(state::stale_binding("call belongs to another endeavor"));
            }
            if rows::str_of(&call, "state") != "open" {
                return Err(state::stale_binding("call is not open"));
            }
        }
        require_active_participant(conn, &req.proposed_pledgor_ref)?;
        require_active_participant(conn, &req.beneficiary_ref)?;
        if parse_rfc3339_utc(&req.deadline).is_some_and(|t| t <= now) {
            return Err(state::invalid("deadline is already past"));
        }
        let effects = vec![Effect::Upsert {
            table: "pledge_proposals".into(),
            row: obj_pairs([
                ("proposal_id", json!(proposal_id)),
                ("society_id", json!(caller.society_id)),
                ("endeavor_ref", json!(req.endeavor_id)),
                ("call_ref", opt_json(&req.call_ref)),
                ("revision", json!(1)),
                ("state", json!("proposed")),
                ("pledgor_ref", json!(req.proposed_pledgor_ref)),
                ("beneficiary_ref", json!(req.beneficiary_ref)),
                ("terms", json!(terms.to_string())),
                ("terms_digest", terms_digest_json.clone()),
                (
                    "required_slots",
                    json!(slots_store_json(&slots.slots, &slots.seats)),
                ),
                ("preparation_trace", json!(trace.to_string())),
                ("amendment_predecessor_ref", Value::Null),
                ("amendment_predecessor_revision", Value::Null),
                ("created_at", json!(created_at)),
            ]),
        }];
        let events = vec![event(
            &caller.society_id,
            &propose_event,
            "pledge.proposed",
            &proposal_id,
            1,
            &caller.participant.participant_id,
            &caller.actor,
            &req.meta,
            json!({"endeavor_ref": req.endeavor_id, "state": "proposed",
                   "pledgor_ref": req.proposed_pledgor_ref}),
        )];
        Ok(Prepared {
            result: json!({
                "proposal_id": proposal_id,
                "endeavor_id": req.endeavor_id,
                "revision": 1,
                "state": "proposed",
                "terms_digest": terms_digest_json,
                "required_slots": slots.slots,
                "dependency_set_ref": dependency_set_ref,
                "created_at": created_at,
                "preparation_trace": trace,
            }),
            revision: Some(1),
            cursor: CursorMint::AfterEvents {
                society_id: caller.society_id.clone(),
            },
            effects,
            events,
        })
    })
}

/// pledge_amend (participant, create): a SEPARATE proposed successor
/// occupying the ONE CAS successor slot of its predecessor (D-RT-3) —
/// never an in-place edit of committed terms.
pub fn pledge_amend(
    store: &mut Store,
    caller: &Caller,
    req: &ops::PledgeAmendRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    check_meta_binding(store, &req.meta, &caller.society_id)?;
    let proposal_id = mint(store, "plg-prop")?;
    let dependency_set_ref = mint(store, "deps")?;
    let amend_event = mint(store, "evt")?;
    let created_at = rfc3339_utc(now);
    let predecessor = rows::get_row(
        store.conn(),
        "pledges",
        "pledge_id",
        &req.amendment_of.pledge_ref,
    )
    .map_err(db_err)?
    .ok_or_else(state::not_found)?;
    let pledgor_ref = req
        .proposed_pledgor_ref
        .clone()
        .unwrap_or_else(|| rows::str_of(&predecessor, "pledgor_ref").to_owned());
    let beneficiary_ref = req
        .beneficiary_ref
        .clone()
        .unwrap_or_else(|| rows::str_of(&predecessor, "beneficiary_ref").to_owned());
    let terms = pledge_terms(
        &pledgor_ref,
        &beneficiary_ref,
        &req.exact_outcome_schema_refs,
        &req.acceptance_criteria_refs,
        &req.evidence_requirements,
        &req.reviewer_rule_ref,
        &req.input_context_ref,
        digest_json(&req.input_context_digest),
        serde_json::to_value(&req.budget_request_set).unwrap_or(Value::Null),
        &req.disclosure_manifest_ref,
        &req.allowed_manifestation_selector,
        serde_json::to_value(&req.delegation_ceiling).unwrap_or(Value::Null),
        &req.deadline,
        serde_json::to_value(&req.cancellation_terms).unwrap_or(Value::Null),
        &req.dependency_refs,
    );
    let (terms_digest, _secret) = store
        .mint_object_digest(
            &format!("society-key:{}/object:{proposal_id}", caller.society_id),
            "bpp-pledge-terms-v0",
            &terms,
        )
        .map_err(|e| state::internal(&e.to_string()))?;
    let terms_digest_json = digest_json(&terms_digest);
    let slots = pledge_slots(store, &pledgor_ref, &beneficiary_ref, &terms_digest_json)?;
    let trace = prepare_trace(
        store,
        &caller.society_id,
        "pledge_amend",
        &caller.actor,
        &req.meta.request_id,
        body,
        &terms_digest,
        &dependency_set_ref,
        vec![
            source_row(
                "/proposal_id",
                &req.meta.request_id,
                "/meta/request_id",
                "t-mint-id",
            ),
            source_row("/terms", &req.meta.request_id, "", "t-copy-typed-terms"),
        ],
        now,
    )?;
    let scope = MutationScope {
        society_id: caller.society_id.clone(),
        operation: "pledge_amend".into(),
        actor: caller.actor.clone(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let caller = caller.clone();
    let req = req.clone();
    run(store, scope, now, hooks, move |conn, _| {
        let pledge = rows::get_row(conn, "pledges", "pledge_id", &req.amendment_of.pledge_ref)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        // The amendment pins its predecessor EXACTLY (D-RT-3).
        if req.amendment_of.pledge_revision != rows::u64_of(&pledge, "revision") {
            return Err(state::stale_revision());
        }
        let prior = rows::json_of(&pledge, "terms_digest");
        if !req.amendment_of.prior_terms_digest.same_ref_json(&prior) {
            return Err(state::stale_binding(
                "prior_terms_digest does not pin the committed terms",
            ));
        }
        if !matches!(
            rows::str_of(&pledge, "state"),
            "active" | "underway" | "waiting"
        ) {
            return Err(state::stale_binding("pledge is not amendable"));
        }
        // Only a party to the obligation proposes its successor.
        let party = caller.participant.participant_id.as_str();
        if rows::str_of(&pledge, "pledgor_ref") != party
            && rows::str_of(&pledge, "beneficiary_ref") != party
        {
            return Err(state::forbidden());
        }
        // ONE successor slot: a live proposed successor blocks a second.
        if rows::live_successor_proposal(conn, &req.amendment_of.pledge_ref)
            .map_err(db_err)?
            .is_some()
        {
            return Err(state::stale_binding(
                "the one amendment successor slot is already occupied (D-RT-3)",
            ));
        }
        if parse_rfc3339_utc(&req.deadline).is_some_and(|t| t <= now) {
            return Err(state::invalid("deadline is already past"));
        }
        let effects = vec![Effect::Upsert {
            table: "pledge_proposals".into(),
            row: obj_pairs([
                ("proposal_id", json!(proposal_id)),
                ("society_id", json!(caller.society_id)),
                ("endeavor_ref", json!(rows::str_of(&pledge, "endeavor_ref"))),
                ("call_ref", rows::json_of(&pledge, "call_ref")),
                ("revision", json!(1)),
                ("state", json!("proposed")),
                ("pledgor_ref", json!(pledgor_ref)),
                ("beneficiary_ref", json!(beneficiary_ref)),
                ("terms", json!(terms.to_string())),
                ("terms_digest", terms_digest_json.clone()),
                (
                    "required_slots",
                    json!(slots_store_json(&slots.slots, &slots.seats)),
                ),
                ("preparation_trace", json!(trace.to_string())),
                (
                    "amendment_predecessor_ref",
                    json!(req.amendment_of.pledge_ref),
                ),
                (
                    "amendment_predecessor_revision",
                    json!(req.amendment_of.pledge_revision),
                ),
                ("created_at", json!(created_at)),
            ]),
        }];
        let events = vec![event(
            &caller.society_id,
            &amend_event,
            "pledge.amendment_proposed",
            &proposal_id,
            1,
            &caller.participant.participant_id,
            &caller.actor,
            &req.meta,
            json!({"amendment_of": req.amendment_of.pledge_ref, "state": "proposed"}),
        )];
        Ok(Prepared {
            result: json!({
                "proposal_id": proposal_id,
                "amendment_of": {
                    "pledge_ref": req.amendment_of.pledge_ref,
                    "pledge_revision": req.amendment_of.pledge_revision,
                },
                "revision": 1,
                "state": "proposed",
                "terms_digest": terms_digest_json,
                "required_slots": slots.slots,
                "dependency_set_ref": dependency_set_ref,
                "created_at": created_at,
                "preparation_trace": trace,
            }),
            revision: Some(1),
            cursor: CursorMint::AfterEvents {
                society_id: caller.society_id.clone(),
            },
            effects,
            events,
        })
    })
}

/// pledge_position (participant, create): the RT-03 seat discipline over
/// the exact terms digest; the pledge assent-mode oneOf was enforced at
/// parse.
pub fn pledge_position(
    store: &mut Store,
    caller: &Caller,
    req: &ops::PositionRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    check_meta_binding(store, &req.meta, &caller.society_id)?;
    let minted = mint_position(store)?;
    let scope = MutationScope {
        society_id: caller.society_id.clone(),
        operation: "pledge_position".into(),
        actor: caller.actor.clone(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let caller = caller.clone();
    let req = req.clone();
    run(store, scope, now, hooks, move |conn, _| {
        let proposal = rows::get_row(conn, "pledge_proposals", "proposal_id", &req.proposal_ref)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        if rows::str_of(&proposal, "state") != "proposed" {
            return Err(state::stale_binding("pledge proposal is not proposed"));
        }
        let stored = rows::json_of(&proposal, "required_slots");
        let seats = seats_from_json(&stored["seats"]);
        let terms = rows::json_of(&proposal, "terms_digest");
        let (effects, result) = record_position(
            conn,
            &minted,
            "pledge",
            &caller.society_id,
            rows::u64_of(&proposal, "revision"),
            &digest_of(&terms)?,
            &seats,
            &req,
            &caller.participant.participant_id,
            &caller.actor,
            "participant",
            now,
        )?;
        let events = vec![event(
            &caller.society_id,
            &minted.event_id,
            "pledge.position_recorded",
            &req.proposal_ref,
            rows::u64_of(&proposal, "revision"),
            &caller.participant.participant_id,
            &caller.actor,
            &req.meta,
            json!({"seat_ref": req.seat_ref, "value": req.value,
                   "assent_mode": opt_json(&req.assent_mode)}),
        )];
        Ok(Prepared {
            result,
            revision: None,
            cursor: CursorMint::AfterEvents {
                society_id: caller.society_id.clone(),
            },
            effects,
            events,
        })
    })
}

pub fn pledge_finalize(
    store: &mut Store,
    caller: &Caller,
    req: &ops::PledgeFinalizeRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    check_meta_binding(store, &req.meta, &caller.society_id)?;
    let pledge_id = mint(store, "plg")?;
    let decision_ref = mint(store, "dec-pledge")?;
    let commit_event = mint(store, "evt")?;
    let budget_event = mint(store, "evt")?;
    let supersede_event = mint(store, "evt")?;
    // One reservation id per possible budget dimension (minted outside
    // the closure, stable across CAS revalidation).
    let mut reservation_mints = Vec::new();
    for _ in 0..64 {
        reservation_mints.push(mint(store, "rsv")?);
    }
    let created_at = rfc3339_utc(now);
    let scope = MutationScope {
        society_id: caller.society_id.clone(),
        operation: "pledge_finalize".into(),
        actor: caller.actor.clone(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let caller = caller.clone();
    let req = req.clone();
    run(store, scope, now, hooks, move |conn, _| {
        let proposal = rows::get_row(conn, "pledge_proposals", "proposal_id", &req.proposal_ref)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        let proposal_revision = rows::u64_of(&proposal, "revision");
        if req.meta.expected_revision != Some(proposal_revision)
            || req.proposal_revision != proposal_revision
        {
            return Err(state::stale_revision());
        }
        if rows::str_of(&proposal, "state") != "proposed" {
            return Err(state::stale_binding("pledge proposal is not proposed"));
        }
        let terms_digest = rows::json_of(&proposal, "terms_digest");
        if !req.subject_digest.same_ref_json(&terms_digest) {
            return Err(state::invalid(
                "subject_digest does not commit to the exact proposed terms",
            ));
        }
        // Only a party finalizes.
        let party = caller.participant.participant_id.as_str();
        if rows::str_of(&proposal, "pledgor_ref") != party
            && rows::str_of(&proposal, "beneficiary_ref") != party
        {
            return Err(state::forbidden());
        }
        let stored = rows::json_of(&proposal, "required_slots");
        let seats = seats_from_json(&stored["seats"]);
        all_seats_assent(conn, "pledge", &req.proposal_ref, &seats)?;

        // D-RT-3: the successor CAS pair is both-or-neither and must pin
        // the proposal's recorded predecessor exactly.
        let predecessor_ref = rows::str_of(&proposal, "amendment_predecessor_ref").to_owned();
        let mut effects = Vec::new();
        let mut events = Vec::new();
        if predecessor_ref.is_empty() {
            if req.supersedes_pledge_ref.is_some() {
                return Err(state::stale_binding(
                    "this proposal amends nothing; the successor pair must be absent",
                ));
            }
        } else {
            let (Some(cited_ref), Some(cited_revision)) =
                (&req.supersedes_pledge_ref, req.supersedes_pledge_revision)
            else {
                return Err(state::stale_binding(
                    "finalizing an amendment requires the exact successor CAS pair (D-RT-3)",
                ));
            };
            if *cited_ref != predecessor_ref {
                return Err(state::stale_binding(
                    "supersedes_pledge_ref does not cite the recorded predecessor",
                ));
            }
            let predecessor = rows::get_row(conn, "pledges", "pledge_id", &predecessor_ref)
                .map_err(db_err)?
                .ok_or_else(state::not_found)?;
            if cited_revision != rows::u64_of(&predecessor, "revision") {
                return Err(state::stale_revision());
            }
            if !matches!(
                rows::str_of(&predecessor, "state"),
                "active" | "underway" | "waiting"
            ) {
                return Err(state::stale_binding("predecessor is not supersedable"));
            }
            // The predecessor's reservations release; the successor
            // reserves afresh (conservation holds across the split).
            settle_holder(conn, &mut effects, "pledge", &predecessor_ref, false)?;
            let pre_revision = rows::u64_of(&predecessor, "revision") + 1;
            let mut superseded = predecessor.clone();
            superseded.insert("state".into(), json!("superseded"));
            superseded.insert("revision".into(), json!(pre_revision));
            superseded.insert("superseded_by".into(), json!(pledge_id));
            superseded.insert("successor_proposal_ref".into(), json!(req.proposal_ref));
            effects.push(Effect::Upsert {
                table: "pledges".into(),
                row: superseded,
            });
            events.push(event(
                &caller.society_id,
                &supersede_event,
                "pledge.superseded",
                &predecessor_ref,
                pre_revision,
                &caller.participant.participant_id,
                &caller.actor,
                &req.meta,
                json!({"superseded_by": pledge_id}),
            ));
        }

        // §11.4: reserve every requested dimension against the
        // endeavor's account set in this one transaction.
        let endeavor_ref = rows::str_of(&proposal, "endeavor_ref").to_owned();
        let endeavor = rows::get_row(conn, "endeavors", "endeavor_id", &endeavor_ref)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        let account_ref = rows::str_of(&endeavor, "budget_account_set_ref").to_owned();
        let terms = rows::json_of(&proposal, "terms");
        let mut amounts: Vec<(String, u64)> = Vec::new();
        if let Some(items) = terms["budget_request_set"]["items"].as_array() {
            for item in items {
                let dimension = item["dimension"].as_str().unwrap_or_default().to_owned();
                let amount = item["max"].as_u64().unwrap_or(0);
                match amounts.iter_mut().find(|(d, _)| *d == dimension) {
                    Some((_, a)) => *a += amount,
                    None => amounts.push((dimension, amount)),
                }
            }
        }
        let mut reservation_refs = Vec::new();
        for (i, (dimension, amount)) in amounts.iter().enumerate() {
            let reservation_id = reservation_mints
                .get(i)
                .ok_or_else(|| state::internal("reservation mint pool exhausted"))?;
            reserve(
                conn,
                &mut effects,
                &caller.society_id,
                &account_ref,
                dimension,
                *amount,
                reservation_id,
                "pledge",
                &pledge_id,
                now,
            )?;
            reservation_refs.push(reservation_id.clone());
        }

        let mut finalized = proposal.clone();
        finalized.insert("state".into(), json!("finalized"));
        finalized.insert("revision".into(), json!(proposal_revision + 1));
        effects.push(Effect::Upsert {
            table: "pledge_proposals".into(),
            row: finalized,
        });
        effects.push(Effect::Upsert {
            table: "pledges".into(),
            row: obj_pairs([
                ("pledge_id", json!(pledge_id)),
                ("society_id", json!(caller.society_id)),
                ("endeavor_ref", json!(endeavor_ref)),
                ("call_ref", rows::json_of(&proposal, "call_ref")),
                ("revision", json!(1)),
                ("state", json!("active")),
                ("pledgor_ref", json!(rows::str_of(&proposal, "pledgor_ref"))),
                (
                    "beneficiary_ref",
                    json!(rows::str_of(&proposal, "beneficiary_ref")),
                ),
                ("terms", json!(rows::str_of(&proposal, "terms"))),
                ("terms_digest", terms_digest.clone()),
                ("source_proposal_ref", json!(req.proposal_ref)),
                ("successor_proposal_ref", Value::Null),
                ("superseded_by", Value::Null),
                ("formation_decision_ref", json!(decision_ref)),
                ("workstream_ref", Value::Null),
                ("workstream_generation", json!(0)),
                (
                    "reservation_refs",
                    json!(json!(reservation_refs).to_string()),
                ),
                ("created_at", json!(created_at)),
            ]),
        });
        events.push(event(
            &caller.society_id,
            &commit_event,
            "pledge.committed",
            &pledge_id,
            1,
            &caller.participant.participant_id,
            &caller.actor,
            &req.meta,
            json!({"endeavor_ref": endeavor_ref, "state": "active",
                   "source_proposal_ref": req.proposal_ref,
                   "decision_ref": decision_ref}),
        ));
        events.push(event(
            &caller.society_id,
            &budget_event,
            "budget.reserved",
            &account_ref,
            1,
            &caller.participant.participant_id,
            &caller.actor,
            &req.meta,
            json!({"holder": pledge_id,
                   "reservations": reservation_refs.clone()}),
        ));
        Ok(Prepared {
            result: json!({
                "pledge_id": pledge_id,
                "revision": 1,
                "state": "active",
                "endeavor_id": endeavor_ref,
                "terms_digest": terms_digest,
                "source_proposal_ref": req.proposal_ref,
                "formation_decision_ref": decision_ref,
                "reservation_refs": reservation_refs,
                "created_at": created_at,
            }),
            revision: Some(1),
            cursor: CursorMint::AfterEvents {
                society_id: caller.society_id.clone(),
            },
            effects,
            events,
        })
    })
}

/// pledge_resume / pledge_relinquish (participant, update; pledgor-only).
pub fn pledge_resume_relinquish(
    store: &mut Store,
    caller: &Caller,
    op: &str,
    req: &ops::PledgeIdRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    check_meta_binding(store, &req.meta, &caller.society_id)?;
    let move_event = mint(store, "evt")?;
    let scope = MutationScope {
        society_id: caller.society_id.clone(),
        operation: op.to_owned(),
        actor: caller.actor.clone(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let caller = caller.clone();
    let op = op.to_owned();
    let req = req.clone();
    run(store, scope, now, hooks, move |conn, _| {
        let pledge = rows::get_row(conn, "pledges", "pledge_id", &req.pledge_id)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        if req.meta.expected_revision != Some(rows::u64_of(&pledge, "revision")) {
            return Err(state::stale_revision());
        }
        // The obligation is the pledgor's alone to resume or lay down.
        if rows::str_of(&pledge, "pledgor_ref") != caller.participant.participant_id {
            return Err(state::forbidden());
        }
        let mut effects = Vec::new();
        let (to, payload) = if op == "pledge_resume" {
            if rows::str_of(&pledge, "state") != "waiting" {
                return Err(state::stale_binding("pledge is not waiting"));
            }
            ("active", json!({"state": "active"}))
        } else {
            if !matches!(
                rows::str_of(&pledge, "state"),
                "active" | "underway" | "waiting"
            ) {
                return Err(state::stale_binding("pledge is not relinquishable"));
            }
            // Reserved budget releases; the obligation dispositions
            // independently under its OWN cancellation terms (§9.4).
            settle_holder(conn, &mut effects, "pledge", &req.pledge_id, false)?;
            (
                "relinquished",
                json!({"state": "relinquished",
                       "statement_ref": opt_json(&req.statement_ref),
                       "disposition":
                           "obligation dispositioned under its own cancellation terms"}),
            )
        };
        let revision = rows::u64_of(&pledge, "revision") + 1;
        let mut moved = pledge.clone();
        moved.insert("state".into(), json!(to));
        moved.insert("revision".into(), json!(revision));
        effects.push(Effect::Upsert {
            table: "pledges".into(),
            row: moved,
        });
        Ok(Prepared {
            result: json!({
                "pledge_id": req.pledge_id,
                "revision": revision,
                "state": to,
            }),
            revision: Some(revision),
            cursor: CursorMint::AfterEvents {
                society_id: caller.society_id.clone(),
            },
            effects,
            events: vec![event(
                &caller.society_id,
                &move_event,
                if op == "pledge_resume" {
                    "pledge.resumed"
                } else {
                    "pledge.relinquished"
                },
                &req.pledge_id,
                revision,
                &caller.participant.participant_id,
                &caller.actor,
                &req.meta,
                payload,
            )],
        })
    })
}

// -------------------------------------------------- delivery + review ----

/// The §20.1 classification-honesty rule for one delivery: content
/// attributed to an `attached_harness` manifestation carries the Society
/// top label (quarantined from finer flows) unless a
/// complete-readable-source attestation is cited among the evidence.
pub const SOURCE_ATTESTATION_PREFIX: &str = "attest-complete-readable-source";

fn classify_delivery(
    conn: &rusqlite::Connection,
    pledgor_ref: &str,
    evidence_refs: &[String],
) -> Result<(String, String), Problem> {
    let manifestations = rows::rows_where(
        conn,
        "manifestation_revisions",
        "participant_ref",
        pledgor_ref,
        "revision",
    )
    .map_err(db_err)?;
    let attached = manifestations.iter().rev().any(|m| {
        rows::str_of(m, "status") == "active" && rows::str_of(m, "kind") == "attached_harness"
    });
    if !attached {
        return Ok((
            "declared".to_owned(),
            "producer manifestation is not an attached harness".to_owned(),
        ));
    }
    let attested = evidence_refs
        .iter()
        .any(|e| e.starts_with(SOURCE_ATTESTATION_PREFIX));
    if attested {
        Ok((
            "attested".to_owned(),
            "complete-readable-source attestation cited; declared classification admitted"
                .to_owned(),
        ))
    } else {
        Ok((
            "society_top".to_owned(),
            "attached-harness output without a complete-readable-source attestation is \
             labeled at the Society top classification (quarantined from finer flows)"
                .to_owned(),
        ))
    }
}

pub fn delivery_submit(
    store: &mut Store,
    caller: &Caller,
    req: &ops::DeliverySubmitRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    check_meta_binding(store, &req.meta, &caller.society_id)?;
    let delivery_id = mint(store, "dlv")?;
    let submit_event = mint(store, "evt")?;
    let submitted_at = rfc3339_utc(now);
    let scope = MutationScope {
        society_id: caller.society_id.clone(),
        operation: "delivery_submit".into(),
        actor: caller.actor.clone(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let caller = caller.clone();
    let req = req.clone();
    run(store, scope, now, hooks, move |conn, _| {
        let pledge = rows::get_row(conn, "pledges", "pledge_id", &req.pledge_id)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        if req.pledge_revision != rows::u64_of(&pledge, "revision") {
            return Err(state::stale_revision());
        }
        let terms = rows::json_of(&pledge, "terms_digest");
        if !req.terms_digest.same_ref_json(&terms) {
            return Err(state::invalid(
                "terms_digest does not pin the committed terms",
            ));
        }
        // The pledgor is CHANNEL-DERIVED: only the committed pledgor's
        // authenticated channel delivers (§9.5).
        if rows::str_of(&pledge, "pledgor_ref") != caller.participant.participant_id {
            return Err(state::forbidden());
        }
        if rows::str_of(&pledge, "state") != "underway" {
            return Err(state::stale_binding("pledge work is not underway"));
        }
        if rows::str_of(&pledge, "workstream_ref") != req.activity_stream_ref {
            return Err(state::stale_binding(
                "activity_stream_ref is not the pledge's bound workstream",
            ));
        }
        let (classification, classification_reason) =
            classify_delivery(conn, &caller.participant.participant_id, &req.evidence_refs)?;
        let subject = json!({
            "delivery_id": delivery_id,
            "pledge_ref": req.pledge_id,
            "pledge_revision": req.pledge_revision,
            "terms_digest": digest_json(&req.terms_digest),
            "output_refs": req.output_refs,
            "evidence_refs": req.evidence_refs,
            "activity_stream_ref": req.activity_stream_ref,
            "classification": classification,
        });
        let subject_digest = part_common::conn_record_digest(
            conn,
            &caller.society_id,
            &delivery_id,
            "bpp-delivery-v0",
            &subject,
        )?;
        let pledge_revision = rows::u64_of(&pledge, "revision") + 1;
        let mut delivered = pledge.clone();
        delivered.insert("state".into(), json!("delivered"));
        delivered.insert("revision".into(), json!(pledge_revision));
        let effects = vec![
            Effect::Upsert {
                table: "deliveries".into(),
                row: obj_pairs([
                    ("delivery_id", json!(delivery_id)),
                    ("society_id", json!(caller.society_id)),
                    ("pledge_ref", json!(req.pledge_id)),
                    ("pledge_revision", json!(req.pledge_revision)),
                    ("state", json!("submitted")),
                    ("terms_digest", json!(digest_json(&req.terms_digest))),
                    ("output_refs", json!(json!(req.output_refs).to_string())),
                    ("evidence_refs", json!(json!(req.evidence_refs).to_string())),
                    ("activity_stream_ref", json!(req.activity_stream_ref)),
                    ("subject_digest", json!(digest_json(&subject_digest))),
                    ("classification", json!(classification)),
                    ("submitted_by", json!(caller.participant.participant_id)),
                    ("submitted_at", json!(submitted_at)),
                ]),
            },
            Effect::Upsert {
                table: "pledges".into(),
                row: delivered,
            },
        ];
        let events = vec![event(
            &caller.society_id,
            &submit_event,
            "delivery.submitted",
            &delivery_id,
            1,
            &caller.participant.participant_id,
            &caller.actor,
            &req.meta,
            json!({"pledge_ref": req.pledge_id, "state": "submitted",
                   "classification": classification,
                   "classification_reason": classification_reason}),
        )];
        Ok(Prepared {
            result: json!({
                "delivery_id": delivery_id,
                "pledge_id": req.pledge_id,
                "pledge_revision": pledge_revision,
                "pledge_state": "delivered",
                "state": "submitted",
                "classification": classification,
                "classification_reason": classification_reason,
                "subject_digest": digest_json(&subject_digest),
                "submitted_at": submitted_at,
            }),
            revision: Some(1),
            cursor: CursorMint::AfterEvents {
                society_id: caller.society_id.clone(),
            },
            effects,
            events,
        })
    })
}

pub fn review_record(
    store: &mut Store,
    caller: &Caller,
    req: &ops::ReviewRecordRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    check_meta_binding(store, &req.meta, &caller.society_id)?;
    let review_id = mint(store, "rvw")?;
    let review_event = mint(store, "evt")?;
    let settle_event = mint(store, "evt")?;
    let created_at = rfc3339_utc(now);
    let scope = MutationScope {
        society_id: caller.society_id.clone(),
        operation: "review_record".into(),
        actor: caller.actor.clone(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let caller = caller.clone();
    let req = req.clone();
    run(store, scope, now, hooks, move |conn, _| {
        let pledge = rows::get_row(conn, "pledges", "pledge_id", &req.pledge_id)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        if req.pledge_revision != rows::u64_of(&pledge, "revision") {
            return Err(state::stale_revision());
        }
        if rows::str_of(&pledge, "state") != "delivered" {
            return Err(state::stale_binding("no delivery awaits review"));
        }
        let delivery = rows::get_row(conn, "deliveries", "delivery_id", &req.delivery_id)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        if rows::str_of(&delivery, "pledge_ref") != req.pledge_id
            || rows::str_of(&delivery, "state") != "submitted"
        {
            return Err(state::stale_binding("delivery is not awaiting review"));
        }
        let delivery_subject = rows::json_of(&delivery, "subject_digest");
        if !req.reviewed_subject_digest.same_ref_json(&delivery_subject) {
            return Err(state::invalid(
                "reviewed_subject_digest does not pin the exact delivery",
            ));
        }
        // The exact reviewer seat: the attached slice's reviewer rule
        // names the beneficiary, and the pledgor never reviews its own
        // delivery (reviewer independence).
        let reviewer = caller.participant.participant_id.as_str();
        if rows::str_of(&pledge, "beneficiary_ref") != reviewer {
            return Err(part_common::position_ineligible(
                "only the exact reviewer seat (the beneficiary) records the review",
            ));
        }
        if rows::str_of(&pledge, "pledgor_ref") == reviewer {
            return Err(Problem::new(
                ProblemKind::IndependenceConflict,
                "the pledgor cannot review its own delivery",
            )
            .with_status(409));
        }
        let (pledge_state, delivery_state, settlement) = match req.outcome.as_str() {
            "fulfilled" => ("fulfilled", "accepted", Some(true)),
            "revision_requested" => ("active", "revision_requested", None),
            "rejected" => ("rejected", "rejected", Some(false)),
            _ => ("disputed", "disputed", None),
        };
        let mut effects = Vec::new();
        let mut events = Vec::new();
        if let Some(commit) = settlement {
            settle_holder(conn, &mut effects, "pledge", &req.pledge_id, commit)?;
        }
        let pledge_revision = rows::u64_of(&pledge, "revision") + 1;
        let mut moved = pledge.clone();
        moved.insert("state".into(), json!(pledge_state));
        moved.insert("revision".into(), json!(pledge_revision));
        effects.push(Effect::Upsert {
            table: "pledges".into(),
            row: moved,
        });
        let mut delivery_moved = delivery.clone();
        delivery_moved.insert("state".into(), json!(delivery_state));
        effects.push(Effect::Upsert {
            table: "deliveries".into(),
            row: delivery_moved,
        });
        effects.push(Effect::Upsert {
            table: "reviews".into(),
            row: obj_pairs([
                ("review_id", json!(review_id)),
                ("society_id", json!(caller.society_id)),
                ("pledge_ref", json!(req.pledge_id)),
                ("pledge_revision", json!(req.pledge_revision)),
                ("delivery_ref", json!(req.delivery_id)),
                ("outcome", json!(req.outcome)),
                (
                    "reviewed_subject_digest",
                    json!(digest_json(&req.reviewed_subject_digest)),
                ),
                (
                    "decision_or_mandate_use_ref",
                    json!(req.decision_or_mandate_use_ref),
                ),
                ("rubric_ref", opt_json(&req.rubric_ref)),
                ("rationale_ref", opt_json(&req.rationale_ref)),
                ("reviewer_ref", json!(reviewer)),
                ("created_at", json!(created_at)),
            ]),
        });
        events.push(event(
            &caller.society_id,
            &review_event,
            "review.recorded",
            &review_id,
            1,
            &caller.participant.participant_id,
            &caller.actor,
            &req.meta,
            json!({"pledge_ref": req.pledge_id, "delivery_ref": req.delivery_id,
                   "outcome": req.outcome, "pledge_state": pledge_state,
                   "decision_or_mandate_use_ref": req.decision_or_mandate_use_ref}),
        ));
        if let Some(commit) = settlement {
            events.push(event(
                &caller.society_id,
                &settle_event,
                "budget.settled",
                &req.pledge_id,
                1,
                &caller.participant.participant_id,
                &caller.actor,
                &req.meta,
                json!({"holder": req.pledge_id,
                       "disposition": if commit { "committed" } else { "released" }}),
            ));
        }
        Ok(Prepared {
            result: json!({
                "review_id": review_id,
                "pledge_id": req.pledge_id,
                "delivery_id": req.delivery_id,
                "outcome": req.outcome,
                "pledge_state": pledge_state,
                "pledge_revision": pledge_revision,
                "created_at": created_at,
            }),
            revision: Some(1),
            cursor: CursorMint::AfterEvents {
                society_id: caller.society_id.clone(),
            },
            effects,
            events,
        })
    })
}

// ------------------------------------------------------------- charter ----

/// charter_propose (participant, create; §6.2): a COMPLETE restatement
/// pinned against the exact current charter head — never a diff.
pub fn charter_propose(
    store: &mut Store,
    caller: &Caller,
    req: &ops::CharterProposeRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    check_meta_binding(store, &req.meta, &caller.society_id)?;
    let proposal_id = mint(store, "charter-prop")?;
    let dependency_set_ref = mint(store, "deps")?;
    let seat_ref = mint(store, "seat-sovereign")?;
    let propose_event = mint(store, "evt")?;
    let created_at = rfc3339_utc(now);
    let sovereign = rows::sovereign_participant(store.conn(), &caller.society_id)
        .map_err(db_err)?
        .ok_or_else(state::not_found)?;
    // §6.2/G41: charter positions are filled by human sovereign seats on
    // the GOVERNANCE surface only.
    let seats = vec![Seat {
        seat_ref: seat_ref.clone(),
        kind: "human_sovereign".into(),
        participant_ref: sovereign.participant_id.clone(),
        surface: "governance".into(),
    }];
    let restatement = {
        let mut m = body.as_object().cloned().unwrap_or_default();
        m.remove("version");
        m.remove("op");
        m.remove("meta");
        Value::Object(m)
    };
    let subject = json!({
        "charter_proposal_id": proposal_id,
        "restatement": restatement,
    });
    let (subject_digest, _secret) = store
        .mint_object_digest(
            &format!("society-key:{}/object:{proposal_id}", caller.society_id),
            "bpp-charter-subject-v0",
            &subject,
        )
        .map_err(|e| state::internal(&e.to_string()))?;
    let trace = prepare_trace(
        store,
        &caller.society_id,
        "charter_propose",
        &caller.actor,
        &req.meta.request_id,
        body,
        &subject_digest,
        &dependency_set_ref,
        vec![
            source_row(
                "/charter_proposal_id",
                &req.meta.request_id,
                "/meta/request_id",
                "t-mint-id",
            ),
            source_row(
                "/restatement",
                &req.meta.request_id,
                "",
                "t-copy-restatement",
            ),
        ],
        now,
    )?;
    let scope = MutationScope {
        society_id: caller.society_id.clone(),
        operation: "charter_propose".into(),
        actor: caller.actor.clone(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let caller = caller.clone();
    let req = req.clone();
    run(store, scope, now, hooks, move |conn, _| {
        let society = rows::get_society(conn, &caller.society_id)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        // The restatement pins the EXACT current head.
        let head: Value = serde_json::from_str(&society.charter_head_digest).unwrap_or(Value::Null);
        if !req.previous_digest.same_ref_json(&head) {
            return Err(state::stale_binding(
                "previous_digest does not pin the current charter head",
            ));
        }
        // One live proposal per charter id at a time.
        let live = rows::rows_where(
            conn,
            "charter_proposals",
            "charter_id",
            &req.charter_id,
            "created_at",
        )
        .map_err(db_err)?
        .into_iter()
        .any(|p| rows::str_of(&p, "state") == "proposed");
        if live {
            return Err(state::stale_binding(
                "a live charter proposal already occupies this charter id",
            ));
        }
        let effects = vec![Effect::Upsert {
            table: "charter_proposals".into(),
            row: obj_pairs([
                ("charter_proposal_id", json!(proposal_id)),
                ("society_id", json!(caller.society_id)),
                ("charter_id", json!(req.charter_id)),
                ("revision", json!(1)),
                ("state", json!("proposed")),
                ("body", json!(restatement.to_string())),
                ("subject_digest", digest_json(&subject_digest)),
                ("required_seats", json!(seats_json(&seats).to_string())),
                ("preparation_trace", json!(trace.to_string())),
                ("proposed_by", json!(caller.participant.participant_id)),
                ("created_at", json!(created_at)),
            ]),
        }];
        let events = vec![event(
            &caller.society_id,
            &propose_event,
            "charter.proposed",
            &proposal_id,
            1,
            &caller.participant.participant_id,
            &caller.actor,
            &req.meta,
            json!({"charter_id": req.charter_id, "state": "proposed"}),
        )];
        Ok(Prepared {
            result: json!({
                "charter_proposal_id": proposal_id,
                "charter_id": req.charter_id,
                "revision": 1,
                "state": "proposed",
                "subject_digest": digest_json(&subject_digest),
                "required_seat_refs": [seat_ref],
                "dependency_set_ref": dependency_set_ref,
                "created_at": created_at,
                "preparation_trace": trace,
            }),
            revision: Some(1),
            cursor: CursorMint::AfterEvents {
                society_id: caller.society_id.clone(),
            },
            effects,
            events,
        })
    })
}
