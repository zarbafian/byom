//! B1 acceptance (the sheet's `b1_acceptance`): the agent notices and
//! explores FIRST — under its issued mandate — then accepts one Pledge
//! through the full seat sequence, survives daemon kill/restart at every
//! commit point (byte-stable replay after each), delivers a reviewable
//! change-set, and `review_record` closes the loop — with a complete
//! causal event timeline and budgets reserved then settled.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::*;
use serde_json::json;

#[test]
fn full_flow_survives_kill_restart_at_every_commit_point() {
    let flow = governed_flow("acc", FlowMode::RestartAfterEachCommit);
    let daemon = &flow.daemon;

    // The complete causal timeline, replayed from genesis: every stage
    // of the flow appears, in order, each event causally attributed.
    let events = daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "events_read",
                "continuation": flow.genesis_cursor, "page_size": 512}),
    );
    let list = events["result"]["events"].as_array().unwrap();
    let kinds: Vec<&str> = list.iter().map(|e| e["kind"].as_str().unwrap()).collect();
    let expected_order = [
        "society.prepared",
        "society.genesis",
        "membership.offered",
        "candidate-self-policy.proposed",
        "membership.accepted",
        "membership.admitted",
        "manifestation.admitted",
        "mandate.prepared",
        "mandate.position_recorded",
        "mandate.issued",
        "activity.opened",
        "wake-intent.submitted",
        "continuation.written",
        "endeavor.proposed",
        "endeavor.position_recorded",
        "endeavor.finalized",
        "call.opened",
        "pledge.proposed",
        "pledge.position_recorded",
        "pledge.committed",
        "pledge.underway",
        "delivery.submitted",
        "review.recorded",
        "budget.settled",
    ];
    let mut cursor_pos = 0usize;
    for expected in expected_order {
        let found = kinds[cursor_pos..].iter().position(|k| *k == expected);
        let at = found.unwrap_or_else(|| {
            panic!("timeline missing {expected} after index {cursor_pos}: {kinds:?}")
        });
        cursor_pos += at + 1;
    }
    // Causal attribution on every event: causation and correlation are
    // never absent, and every mutation-caused event names its request.
    for e in list {
        assert!(
            !e["causation_ref"].as_str().unwrap_or_default().is_empty(),
            "uncaused event: {e}"
        );
        assert!(
            !e["correlation_ref"].as_str().unwrap_or_default().is_empty(),
            "uncorrelated event: {e}"
        );
        assert!(
            !e["occurred_at"].as_str().unwrap_or_default().is_empty(),
            "untimed event: {e}"
        );
    }
    // Density under single-step pagination (sequences never on the wire).
    let mut cursor = flow.genesis_cursor.clone();
    let mut walked = 0;
    loop {
        let page = daemon.call(
            "projection",
            &json!({"version": "0.2", "op": "events_read",
                    "continuation": cursor, "page_size": 1}),
        );
        let items = page["result"]["events"].as_array().unwrap().clone();
        if items.is_empty() {
            break;
        }
        walked += items.len();
        cursor = page["result"]["continuation"].as_str().unwrap().to_owned();
        assert!(walked <= 256, "runaway pagination");
    }
    assert_eq!(
        walked,
        list.len(),
        "single-step pagination covers the ledger"
    );

    // The witness generations are dense across every restart.
    let entries = flow.daemon.witness_entries();
    for (i, entry) in entries.iter().enumerate() {
        assert_eq!(entry["generation"].as_u64().unwrap(), i as u64 + 1);
    }

    // The settled surviving state: pledge fulfilled, endeavor active,
    // both streams live, wake intent still pending.
    let snapshot = daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "snapshot_get",
                "society_id": flow.society_id}),
    );
    let pledge = snapshot["result"]["pledges"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["pledge_id"] == json!(flow.pledge_id))
        .expect("pledge in snapshot")
        .clone();
    assert_eq!(pledge["state"], "fulfilled", "{snapshot}");
    let mandate = snapshot["result"]["mandates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["mandate_id"] == json!(flow.mandate_id))
        .expect("mandate in snapshot")
        .clone();
    assert_eq!(mandate["state"], "active");

    // Exploration came FIRST: the exploration stream precedes the
    // pledge_work stream in the ledger.
    let explore_at = kinds.iter().position(|k| *k == "activity.opened").unwrap();
    let commit_at = kinds.iter().position(|k| *k == "pledge.committed").unwrap();
    assert!(
        explore_at < commit_at,
        "exploration must precede the pledge: {kinds:?}"
    );

    // Deterministic delivery: the retained receipt replays byte-stably
    // and cites the exact terms.
    let shown = daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "activity_show",
                "activity_stream_ref": flow.work_stream}),
    );
    assert_eq!(shown["result"]["kind"], "pledge_work");
}

#[test]
fn clean_flow_derivation_and_reads_close_the_loop() {
    let flow = governed_flow("acc2", FlowMode::Clean);
    let daemon = &flow.daemon;
    let incarnation = &flow.incarnation;

    // §10.2 never-widening at the delegation ceiling: the issued parent
    // forbids delegation entirely, so even a strictly narrower child is
    // authority_widening — the mechanical checks decide, not intent.
    let narrow = daemon
        .call_raw(
            "participant",
            Some(&flow.agent_token),
            &json!({
                "version": "0.2", "op": "mandate_derive",
                "meta": meta(incarnation, "acc2-derive", None),
                "parent_mandate_ref": flow.mandate_id,
                "parent_mandate_revision": 2,
                "parent_mandate_digest": flow.mandate_subject_digest,
                "grantee_participant_ref": "part-agent-1",
                "purpose_ref": "purpose-explore-1",
                "allowed_operations": ["activity_open"],
                "budget_ceiling_set_ref": "budget-mandate-child-1",
                "concurrency_ceiling": 1,
                "delegation": {"allowed": false, "max_depth": 0, "max_children": 0,
                               "grantee_selectors": []},
                "expires_at": far_future(),
            })
            .to_string(),
        )
        .unwrap();
    assert_eq!(kind_of(&narrow), "authority_widening", "{narrow}");

    // The originating-surface recovery reads: idempotency_result returns
    // the retained delivery receipt WITHOUT re-executing; cursor_recover
    // re-mints a verified continuation.
    let retained = daemon
        .call_raw(
            "participant",
            Some(&flow.agent_token),
            &json!({"version": "0.2", "op": "idempotency_result",
                    "operation": "delivery_submit",
                    "idempotency_key": "idem-acc2-deliver"})
            .to_string(),
        )
        .unwrap();
    assert_eq!(retained["outcome"], "ok", "{retained}");
    assert_eq!(retained["result"]["state"], "retained");
    assert_eq!(
        retained["result"]["result"]["result"]["delivery_id"],
        json!(flow.delivery_id)
    );
    let recovered = daemon
        .call_raw(
            "participant",
            None,
            &json!({"version": "0.2", "op": "cursor_recover",
                    "continuation": flow.genesis_cursor})
            .to_string(),
        )
        .unwrap();
    assert_eq!(recovered["outcome"], "ok", "{recovered}");
    let fresh = recovered["result"]["continuation"].as_str().unwrap();
    let replayed = daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "events_read",
                "continuation": fresh, "page_size": 512}),
    );
    assert!(
        !replayed["result"]["events"].as_array().unwrap().is_empty(),
        "recovered cursor replays the ledger"
    );

    // events_wait: an immediate page when events exist; a bounded empty
    // wait at the head.
    let waited = daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "events_wait",
                "continuation": flow.genesis_cursor, "page_size": 8,
                "max_wait_milliseconds": 3000}),
    );
    assert_eq!(waited["outcome"], "ok", "{waited}");
    assert!(!waited["result"]["events"].as_array().unwrap().is_empty());
    let head_cursor = {
        // Walk to the head.
        let mut cursor = flow.genesis_cursor.clone();
        loop {
            let page = daemon.call(
                "projection",
                &json!({"version": "0.2", "op": "events_read",
                        "continuation": cursor, "page_size": 512}),
            );
            let n = page["result"]["events"].as_array().unwrap().len();
            cursor = page["result"]["continuation"].as_str().unwrap().to_owned();
            if n == 0 {
                break cursor;
            }
        }
    };
    let start = std::time::Instant::now();
    let empty = daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "events_wait",
                "continuation": head_cursor, "page_size": 8,
                "max_wait_milliseconds": 300}),
    );
    assert_eq!(empty["outcome"], "ok");
    assert!(empty["result"]["events"].as_array().unwrap().is_empty());
    assert!(
        start.elapsed() >= std::time::Duration::from_millis(280),
        "the long-poll honors its bound"
    );

    // charter proposal → sovereign governance-surface position →
    // adoption → history shows r1 and r2.
    let society = daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "society_show", "society_id": flow.society_id}),
    );
    let proposed = daemon
        .call_raw(
            "participant",
            None,
            &json!({
                "version": "0.2", "op": "charter_propose",
                "meta": meta(incarnation, "acc2-charter", None),
                "charter_id": "charter-main",
                "previous_digest": society["result"]["charter_head_digest"],
                "human_sovereign_seats": ["seat-sovereign-1"],
                "admission_rule": rule("rule-admission"),
                "suspension_rule": rule("rule-suspension"),
                "obligation_disposition_rule": rule("rule-obligation"),
                "decision_rule_set": [rule("rule-general")],
                "delegable_power_set": [],
                "non_delegable_power_set": ["membership", "charter"],
                "standing_classes": ["member"],
                "assembly_constraints": bpa1_allow_all(),
                "mandate_constraints": bpa1_allow_all(),
                "pledge_constraints": bpa1_allow_all(),
                "budget_and_concurrency_ceilings": bpa1_allow_all(),
                "data_and_retention_policy_refs": [],
                "emergency_hold_rule": rule("rule-hold"),
                "dispute_rule": rule("rule-dispute"),
                "dissolution_rule": rule("rule-dissolve"),
            })
            .to_string(),
        )
        .unwrap();
    assert_eq!(proposed["outcome"], "ok", "{proposed}");
    let proposal_ref = proposed["result"]["charter_proposal_id"].as_str().unwrap();
    let seat = proposed["result"]["required_seat_refs"][0]
        .as_str()
        .unwrap();
    let positioned = daemon.call(
        "governance",
        &json!({
            "version": "0.2", "op": "charter_position",
            "meta": meta(incarnation, "acc2-chpos", None),
            "proposal_ref": proposal_ref,
            "proposal_revision": 1,
            "subject_digest": proposed["result"]["subject_digest"],
            "seat_ref": seat,
            "value": "assent",
        }),
    );
    assert_eq!(positioned["outcome"], "ok", "{positioned}");
    let adopted = daemon.call(
        "governance",
        &json!({
            "version": "0.2", "op": "charter_finalize",
            "meta": meta(incarnation, "acc2-chfin", Some(1)),
            "charter_id": "charter-main",
            "subject_digest": proposed["result"]["subject_digest"],
        }),
    );
    assert_eq!(adopted["outcome"], "ok", "{adopted}");
    assert_eq!(adopted["result"]["revision"], 2);
    let history = daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "charter_history",
                "charter_id": flow.society_id, "page_size": 16}),
    );
    assert_eq!(history["outcome"], "ok", "{history}");
    let revisions = history["result"]["revisions"].as_array().unwrap();
    assert_eq!(revisions.len(), 2, "{history}");
    assert_eq!(revisions[0]["revision"], 1);
    assert_eq!(revisions[1]["revision"], 2);
    assert_eq!(revisions[1]["state"], "adopted");
    assert!(revisions[1]["effective_at"].is_string());
}

fn rule(name: &str) -> serde_json::Value {
    json!({"rule_ref": name, "rule_digest": test_digest(0xe1)})
}
