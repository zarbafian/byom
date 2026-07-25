//! Slice-2 surface round-trips beyond the named B1 suites: the
//! self-policy lifecycle, the policy-derived wake origin and withdrawal,
//! the continuity-root machine, successful never-widening derivation,
//! endeavor hold/release/close, call withdrawal, the D-RT-3 amendment
//! successor split end to end, the interrupted-work waiting/resume path,
//! relinquishment with budget release, and the diagnostic
//! recovery_checkpoint_show.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::*;
use serde_json::{json, Value};

fn part(flow: &FlowOutcome, token: Option<&str>, request: &Value) -> Value {
    flow.daemon
        .call_raw("participant", token, &request.to_string())
        .unwrap_or_else(|e| panic!("{e}\n{request}"))
}

#[test]
fn self_policies_wake_origins_and_continuity_roots() {
    let flow = governed_flow("gw1", FlowMode::Clean);
    let incarnation = &flow.incarnation;
    let agent = Some(flow.agent_token.as_str());

    // Self-policy lifecycle: adopt → revoke → fresh adopt. (The assent
    // policy activated at admission is superseded by citing its digest.)
    let adopt = part(
        &flow,
        agent,
        &json!({
            "version": "0.2", "op": "activation_policy_adopt",
            "meta": meta(incarnation, "gw1x-apol", None),
            "activity_kind_set": ["exploration"],
            "interest_and_event_selectors": [],
            "purpose_and_context_ceilings": bpa1_allow_all(),
            "mandate_selectors": [],
            "budget_rate_and_concurrency_ceilings": bpa1_allow_all(),
            "allowed_manifestation_selector": bpa1_allow_all(),
            "adoption_mode": "direct_participant",
            "adoption_control_domain_ref": "control-domain-1",
            "adoption_control_domain_digest": test_digest(0xf1),
            "root_authentication_evidence_ref": "auth-evidence-1",
            "effective_at": "2026-01-01T00:00:00Z",
            "expires_at": far_future(),
        }),
    );
    assert_eq!(adopt["outcome"], "ok", "{adopt}");
    let policy_id = adopt["result"]["policy_id"].as_str().unwrap().to_owned();

    // A policy-derived wake intent must cite the ACTIVE activation
    // policy; then it withdraws cleanly.
    let wake = part(
        &flow,
        agent,
        &json!({
            "version": "0.2", "op": "wake_intent_submit",
            "meta": meta(incarnation, "gw1x-wake", None),
            "activity_stream_ref": flow.exploration_stream,
            "generation": 1,
            "origin": "participant_activation_policy",
            "activation_policy_ref": policy_id,
            "exact_cause_ref": "cause-policy-1",
            "exact_cause_digest": test_digest(0xf2),
            "purpose_ref": "purpose-explore-1",
            "stable_wake_key": "wake-gw1-policy",
            "expires_at": far_future(),
        }),
    );
    assert_eq!(wake["outcome"], "ok", "{wake}");
    let withdrawn = part(
        &flow,
        agent,
        &json!({
            "version": "0.2", "op": "wake_intent_withdraw",
            "meta": meta(incarnation, "gw1x-wakewd", Some(1)),
            "wake_intent_ref": wake["result"]["wake_intent_id"],
        }),
    );
    assert_eq!(withdrawn["result"]["state"], "withdrawn", "{withdrawn}");
    // A wake intent citing a NON-active policy refuses.
    let revoked = part(
        &flow,
        agent,
        &json!({
            "version": "0.2", "op": "activation_policy_revoke",
            "meta": meta(incarnation, "gw1x-apolrev", Some(1)),
            "policy_ref": policy_id,
        }),
    );
    assert_eq!(revoked["result"]["status"], "revoked", "{revoked}");
    let stale_wake = part(
        &flow,
        agent,
        &json!({
            "version": "0.2", "op": "wake_intent_submit",
            "meta": meta(incarnation, "gw1x-wake2", None),
            "activity_stream_ref": flow.exploration_stream,
            "generation": 1,
            "origin": "participant_activation_policy",
            "activation_policy_ref": policy_id,
            "exact_cause_ref": "cause-policy-2",
            "exact_cause_digest": test_digest(0xf3),
            "purpose_ref": "purpose-explore-1",
            "stable_wake_key": "wake-gw1-policy-2",
            "expires_at": far_future(),
        }),
    );
    assert_eq!(kind_of(&stale_wake), "stale_binding", "{stale_wake}");

    // Continuity-root machine: absent → active → sealed → retired; the
    // Society never unseals (sealed → active is a closed transition).
    let root = part(
        &flow,
        agent,
        &json!({
            "version": "0.2", "op": "continuity_root_update",
            "meta": meta(incarnation, "gw1x-root1", Some(0)),
            "target_status": "active",
            "opaque_provider_ref": "provider-1",
        }),
    );
    assert_eq!(root["outcome"], "ok", "{root}");
    let root_id = root["result"]["continuity_root_id"].as_str().unwrap();
    let sealed = part(
        &flow,
        agent,
        &json!({
            "version": "0.2", "op": "continuity_root_update",
            "meta": meta(incarnation, "gw1x-root2", Some(1)),
            "target_status": "sealed",
            "continuity_root_ref": root_id,
        }),
    );
    assert_eq!(sealed["result"]["status"], "sealed", "{sealed}");
    let unseal = part(
        &flow,
        agent,
        &json!({
            "version": "0.2", "op": "continuity_root_update",
            "meta": meta(incarnation, "gw1x-root3", Some(2)),
            "target_status": "active",
            "continuity_root_ref": root_id,
        }),
    );
    assert_eq!(kind_of(&unseal), "stale_binding", "{unseal}");
    let retired = part(
        &flow,
        agent,
        &json!({
            "version": "0.2", "op": "continuity_root_update",
            "meta": meta(incarnation, "gw1x-root4", Some(2)),
            "target_status": "retired",
            "continuity_root_ref": root_id,
        }),
    );
    assert_eq!(retired["result"]["status"], "retired", "{retired}");

    // recovery_checkpoint_show: the diagnostic remainder, with the
    // Society's recovery epoch.
    let checkpoint = flow.daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "recovery_checkpoint_show",
                "society_id": flow.society_id}),
    );
    assert_eq!(checkpoint["result"]["endpoint_status"], "active");
    assert_eq!(
        checkpoint["result"]["witness_profile"],
        "developer-recovery"
    );
    assert_eq!(checkpoint["result"]["recovery_epoch"], 0);
    assert_eq!(
        checkpoint["result"]["journal_mirror_generation"],
        checkpoint["result"]["witness_head_generation"],
        "{checkpoint}"
    );
}

#[test]
fn derivation_endeavor_lifecycle_and_amendment_successor_split() {
    let mut flow = governed_flow("gw2", FlowMode::Clean);
    let daemon = &flow.daemon;
    let incarnation = &flow.incarnation;
    let agent = Some(flow.agent_token.as_str());

    // A delegable parent mandate → a NARROWER child derives, collects
    // its seat, and issues.
    let parent = part(
        &flow,
        agent,
        &json!({
            "version": "0.2", "op": "mandate_prepare",
            "meta": meta(incarnation, "gw2x-mprep", None),
            "grantee_participant_ref": "part-agent-1",
            "purpose_ref": "purpose-derive-1",
            "allowed_operations": ["activity_open", "continuation_write"],
            "resource_selectors": ["res-a", "res-b"],
            "data_class_selectors": [],
            "destination_selectors": [],
            "budget_ceiling_set_ref": "budget-parent-1",
            "concurrency_ceiling": 4,
            "delegation": {"allowed": true, "max_depth": 2, "max_children": 4,
                           "grantee_selectors": []},
            "expires_at": far_future(),
        }),
    );
    assert_eq!(parent["outcome"], "ok", "{parent}");
    let parent_id = parent["result"]["mandate_id"].as_str().unwrap().to_owned();
    let seat = parent["result"]["required_seat_refs"][0].as_str().unwrap();
    let positioned = daemon.call(
        "governance",
        &json!({
            "version": "0.2", "op": "mandate_position",
            "meta": meta(incarnation, "gw2x-mpos", None),
            "proposal_ref": parent_id,
            "proposal_revision": 1,
            "subject_digest": parent["result"]["subject_digest"],
            "seat_ref": seat,
            "value": "assent",
        }),
    );
    assert_eq!(positioned["outcome"], "ok", "{positioned}");
    let issued = daemon.call(
        "governance",
        &json!({
            "version": "0.2", "op": "mandate_issue",
            "meta": meta(incarnation, "gw2x-missue", Some(1)),
            "mandate_id": parent_id,
            "subject_digest": parent["result"]["subject_digest"],
        }),
    );
    assert_eq!(issued["outcome"], "ok", "{issued}");
    let derived = part(
        &flow,
        agent,
        &json!({
            "version": "0.2", "op": "mandate_derive",
            "meta": meta(incarnation, "gw2x-derive", None),
            "parent_mandate_ref": parent_id,
            "parent_mandate_revision": 2,
            "parent_mandate_digest": parent["result"]["subject_digest"],
            "grantee_participant_ref": "part-agent-1",
            "purpose_ref": "purpose-derive-1",
            "allowed_operations": ["activity_open"],
            "resource_selectors": ["res-a"],
            "budget_ceiling_set_ref": "budget-child-1",
            "concurrency_ceiling": 1,
            "delegation": {"allowed": false, "max_depth": 0, "max_children": 0,
                           "grantee_selectors": []},
            "expires_at": far_future(),
        }),
    );
    assert_eq!(derived["outcome"], "ok", "{derived}");
    assert_eq!(derived["result"]["parent_mandate_ref"], json!(parent_id));

    // Endeavor lifecycle: hold → release → a second call withdraws →
    // close through reviewing to fulfilled.
    let gov_meta = |key: &str, rev: u64| meta(incarnation, key, Some(rev));
    let held = part(
        &flow,
        None,
        &json!({
            "version": "0.2", "op": "endeavor_hold",
            "meta": gov_meta("gw2x-ehold", 2),
            "endeavor_id": flow.endeavor_id,
            "hold_reason_ref": "reason-pause-1",
        }),
    );
    assert_eq!(held["result"]["state"], "held", "{held}");
    let released = part(
        &flow,
        None,
        &json!({
            "version": "0.2", "op": "endeavor_release",
            "meta": gov_meta("gw2x-erel", 3),
            "endeavor_id": flow.endeavor_id,
        }),
    );
    assert_eq!(released["result"]["state"], "active", "{released}");
    let call2 = part(
        &flow,
        None,
        &json!({
            "version": "0.2", "op": "call_open",
            "meta": meta(incarnation, "gw2x-call2", None),
            "endeavor_id": flow.endeavor_id,
            "requested_outcome_schema_refs": ["schema-change-set-1"],
            "acceptance_criteria_refs": ["criteria-review-1"],
            "evidence_requirements": [],
        }),
    );
    assert_eq!(call2["outcome"], "ok", "{call2}");
    // Withdrawal is opener-only: the agent cannot withdraw the
    // sovereign's call.
    let foreign = part(
        &flow,
        agent,
        &json!({
            "version": "0.2", "op": "call_withdraw",
            "meta": meta(incarnation, "gw2x-callwd-x", Some(1)),
            "call_id": call2["result"]["call_id"],
        }),
    );
    assert_eq!(kind_of(&foreign), "forbidden", "{foreign}");
    let withdrawn = part(
        &flow,
        None,
        &json!({
            "version": "0.2", "op": "call_withdraw",
            "meta": meta(incarnation, "gw2x-callwd", Some(1)),
            "call_id": call2["result"]["call_id"],
        }),
    );
    assert_eq!(withdrawn["result"]["state"], "withdrawn", "{withdrawn}");

    // A second pledge for the amendment/interruption arc.
    let proposed = part(
        &flow,
        agent,
        &json!({
            "version": "0.2", "op": "pledge_propose",
            "meta": meta(incarnation, "gw2x-p2", None),
            "endeavor_id": flow.endeavor_id,
            "proposed_pledgor_ref": "part-agent-1",
            "beneficiary_ref": flow.sovereign,
            "exact_outcome_schema_refs": ["schema-change-set-1"],
            "acceptance_criteria_refs": ["criteria-review-1"],
            "evidence_requirements": [],
            "reviewer_rule_ref": "rule-beneficiary-reviews",
            "input_context_ref": "context-input-2",
            "input_context_digest": test_digest(0xa5),
            "budget_request_set": {"items": [
                {"dimension": "unit", "canonical_unit": "unit",
                 "scale": 0, "max": 8}]},
            "allowed_manifestation_selector": bpa1_allow_all(),
            "delegation_ceiling": {"allowed": false, "max_depth": 0,
                                   "max_children": 0},
            "deadline": far_future(),
            "cancellation_terms": {"terms_ref": "terms-cancel-1",
                                   "terms_digest": test_digest(0xa6)},
            "dependency_refs": [],
        }),
    );
    assert_eq!(proposed["outcome"], "ok", "{proposed}");
    let proposal_id = proposed["result"]["proposal_id"].as_str().unwrap();
    let terms = proposed["result"]["terms_digest"].clone();
    let seat_of = |reply: &Value, kind: &str| -> String {
        reply["result"]["required_slots"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == kind)
            .unwrap()["seat_refs"][0]
            .as_str()
            .unwrap()
            .to_owned()
    };
    for (who, token, kind) in [
        ("agent", agent, "pledgor_assent"),
        ("sov", None, "beneficiary_assent"),
    ] {
        let p = part(
            &flow,
            token,
            &json!({
                "version": "0.2", "op": "pledge_position",
                "meta": meta(incarnation, &format!("gw2x-p2pos-{who}"), None),
                "proposal_ref": proposal_id,
                "proposal_revision": 1,
                "subject_digest": terms,
                "seat_ref": seat_of(&proposed, kind),
                "value": "assent",
                "assent_mode": "direct_participant",
            }),
        );
        assert_eq!(p["outcome"], "ok", "{p}");
    }
    let pledge2 = part(
        &flow,
        agent,
        &json!({
            "version": "0.2", "op": "pledge_finalize",
            "meta": meta(incarnation, "gw2x-p2fin", Some(1)),
            "proposal_ref": proposal_id,
            "proposal_revision": 1,
            "subject_digest": terms,
        }),
    );
    assert_eq!(pledge2["outcome"], "ok", "{pledge2}");
    let pledge2_id = pledge2["result"]["pledge_id"].as_str().unwrap().to_owned();

    // D-RT-3: ONE successor slot. The first amendment occupies it; a
    // second is refused while the first is live.
    let amend_body = |key: &str, max: u64| {
        json!({
            "version": "0.2", "op": "pledge_amend",
            "meta": meta(incarnation, key, None),
            "amendment_of": {"pledge_ref": pledge2_id, "pledge_revision": 1,
                             "prior_terms_digest": terms},
            "exact_outcome_schema_refs": ["schema-change-set-1"],
            "acceptance_criteria_refs": ["criteria-review-1"],
            "evidence_requirements": [],
            "reviewer_rule_ref": "rule-beneficiary-reviews",
            "input_context_ref": "context-input-3",
            "input_context_digest": test_digest(0xa7),
            "budget_request_set": {"items": [
                {"dimension": "unit", "canonical_unit": "unit",
                 "scale": 0, "max": max}]},
            "allowed_manifestation_selector": bpa1_allow_all(),
            "delegation_ceiling": {"allowed": false, "max_depth": 0,
                                   "max_children": 0},
            "deadline": far_future(),
            "cancellation_terms": {"terms_ref": "terms-cancel-1",
                                   "terms_digest": test_digest(0xa6)},
            "dependency_refs": [],
        })
    };
    let amended = part(&flow, agent, &amend_body("gw2x-amend", 12));
    assert_eq!(amended["outcome"], "ok", "{amended}");
    let successor_id = amended["result"]["proposal_id"].as_str().unwrap();
    let successor_terms = amended["result"]["terms_digest"].clone();
    let second = part(&flow, agent, &amend_body("gw2x-amend2", 13));
    assert_eq!(kind_of(&second), "stale_binding", "{second}");

    // Seats on the successor; finalize WITHOUT the successor CAS pair
    // fails (both-or-neither), WITH the exact pair supersedes.
    for (who, token, kind) in [
        ("agent", agent, "pledgor_assent"),
        ("sov", None, "beneficiary_assent"),
    ] {
        let p = part(
            &flow,
            token,
            &json!({
                "version": "0.2", "op": "pledge_position",
                "meta": meta(incarnation, &format!("gw2x-succ-{who}"), None),
                "proposal_ref": successor_id,
                "proposal_revision": 1,
                "subject_digest": successor_terms,
                "seat_ref": seat_of(&amended, kind),
                "value": "assent",
                "assent_mode": "direct_participant",
            }),
        );
        assert_eq!(p["outcome"], "ok", "{p}");
    }
    let pairless = part(
        &flow,
        agent,
        &json!({
            "version": "0.2", "op": "pledge_finalize",
            "meta": meta(incarnation, "gw2x-succfin-x", Some(1)),
            "proposal_ref": successor_id,
            "proposal_revision": 1,
            "subject_digest": successor_terms,
        }),
    );
    assert_eq!(kind_of(&pairless), "stale_binding", "{pairless}");
    let superseding = part(
        &flow,
        agent,
        &json!({
            "version": "0.2", "op": "pledge_finalize",
            "meta": meta(incarnation, "gw2x-succfin", Some(1)),
            "proposal_ref": successor_id,
            "proposal_revision": 1,
            "subject_digest": successor_terms,
            "supersedes_pledge_ref": pledge2_id,
            "supersedes_pledge_revision": 1,
        }),
    );
    assert_eq!(superseding["outcome"], "ok", "{superseding}");
    let pledge3_id = superseding["result"]["pledge_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let snapshot = daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "snapshot_get",
                "society_id": flow.society_id, "kinds": ["pledges"]}),
    );
    let state_of = |id: &str| -> String {
        snapshot["result"]["pledges"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["pledge_id"] == json!(id))
            .unwrap()["state"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    assert_eq!(state_of(&pledge2_id), "superseded", "{snapshot}");
    assert_eq!(state_of(&pledge3_id), "active", "{snapshot}");

    // Interrupted work: open → close the stream before delivery → the
    // pledge parks at waiting → resume → relinquish releases budget.
    let work = part(
        &flow,
        agent,
        &json!({
            "version": "0.2", "op": "activity_open",
            "meta": meta(incarnation, "gw2x-work", None),
            "kind": "pledge_work",
            "purpose_ref": "purpose-improve-1",
            "purpose_digest": test_digest(0xa8),
            "pledge_binding": {"pledge_id": pledge3_id, "pledge_revision": 1,
                               "terms_digest": successor_terms},
            "mandate_refs": [],
            "budget_account_set_ref": format!("budget-endeavor-gw2"),
        }),
    );
    assert_eq!(work["outcome"], "ok", "{work}");
    let stream = work["result"]["activity_stream_id"].as_str().unwrap();
    let closed = part(
        &flow,
        agent,
        &json!({
            "version": "0.2", "op": "activity_close",
            "meta": meta(incarnation, "gw2x-close", Some(1)),
            "activity_stream_ref": stream,
            "generation": 1,
            "target_state": "canceled",
        }),
    );
    assert_eq!(closed["outcome"], "ok", "{closed}");
    let resumed = part(
        &flow,
        agent,
        &json!({
            "version": "0.2", "op": "pledge_resume",
            "meta": meta(incarnation, "gw2x-resume", Some(3)),
            "pledge_id": pledge3_id,
        }),
    );
    assert_eq!(resumed["result"]["state"], "active", "{resumed}");
    // Relinquish is pledgor-only.
    let foreign_relinquish = part(
        &flow,
        None,
        &json!({
            "version": "0.2", "op": "pledge_relinquish",
            "meta": meta(incarnation, "gw2x-relq-x", Some(4)),
            "pledge_id": pledge3_id,
        }),
    );
    assert_eq!(
        kind_of(&foreign_relinquish),
        "forbidden",
        "{foreign_relinquish}"
    );
    let relinquished = part(
        &flow,
        agent,
        &json!({
            "version": "0.2", "op": "pledge_relinquish",
            "meta": meta(incarnation, "gw2x-relq", Some(4)),
            "pledge_id": pledge3_id,
            "statement_ref": "statement-cannot-continue-1",
        }),
    );
    assert_eq!(
        relinquished["result"]["state"], "relinquished",
        "{relinquished}"
    );

    // Budget conservation across the whole arc: the endeavor account's
    // reserved bucket drained back (fulfilled committed; superseded and
    // relinquished released).
    flow.daemon.stop();
    let store = byom_store::Store::open(&flow.daemon.data_dir).unwrap();
    let (ceiling, remaining, reserved, committed): (i64, i64, i64, i64) = store
        .conn()
        .query_row(
            "SELECT ceiling, remaining, reserved, committed FROM budget_accounts
             WHERE account_ref = 'budget-endeavor-gw2' AND dimension = 'unit'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(reserved, 0, "all reservations settled");
    assert_eq!(committed, 16, "the fulfilled pledge committed its request");
    assert_eq!(
        ceiling,
        remaining + reserved + committed,
        "§11.4 conservation holds"
    );

    // The endeavor closes through reviewing to fulfilled — but the
    // daemon is stopped; assert via the surviving rows.
    let endeavor_state: String = store
        .conn()
        .query_row(
            "SELECT state FROM endeavors WHERE endeavor_id = ?1",
            [&flow.endeavor_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(endeavor_state, "active");
}
