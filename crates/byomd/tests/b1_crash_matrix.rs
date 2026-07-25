//! The kill-and-restart matrix (kovee's KOVEED_ABORT pattern, byomd's
//! BYOMD_ABORT): for EVERY slice-1 mutation and EVERY §15.3 commit
//! boundary — after SQL prepare (before the witness CAS), after the
//! witness CAS, inside SQL finalize before commit, and after commit
//! before the reply — the daemon is killed at that exact point,
//! restarted, and the flow must continue: the exact retry answers
//! (fresh execution when provably unjournaled, byte-stable replay when
//! recovered), every effect lands exactly once, and the onboarding
//! machine completes end to end.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::*;
use serde_json::{json, Value};

/// Runs the whole onboarding flow, killing the daemon at `phase` of
/// `crash_op`, restarting, and retrying the exact request.
fn run_flow(crash_op: &str, phase: &str) {
    let tag = format!("cm-{crash_op}-{phase}");
    let abort = format!("{phase}:{crash_op}");
    let mut daemon = TestDaemon::start_with_env(&tag, &[("BYOMD_ABORT", &abort)]);
    let incarnation = daemon.incarnation();

    // A step sends `request` on `surface` (candidate steps carry a
    // token); when it is the crash step, the daemon dies and the exact
    // request is retried after restart.
    let send = |daemon: &mut TestDaemon,
                op: &str,
                surface: &str,
                token: Option<&str>,
                request: &Value|
     -> Value {
        if op == crash_op {
            let outcome = daemon.call_raw(surface, token, &request.to_string());
            assert!(outcome.is_err(), "{tag}: expected death, got {outcome:?}");
            daemon.wait_exit();
            daemon.restart(&[]);
            let retried = daemon
                .call_raw(surface, token, &request.to_string())
                .unwrap_or_else(|e| panic!("{tag}: retry failed: {e}"));
            assert_eq!(retried["outcome"], "ok", "{tag}: retry: {retried}");
            // A second identical send replays byte-stably.
            let again = daemon
                .call_raw(surface, token, &request.to_string())
                .unwrap();
            assert_eq!(again, retried, "{tag}: replay must be byte-stable");
            retried
        } else {
            let reply = daemon
                .call_raw(surface, token, &request.to_string())
                .unwrap_or_else(|e| panic!("{tag}: {op} failed: {e}"));
            assert_eq!(reply["outcome"], "ok", "{tag}: {op}: {reply}");
            reply
        }
    };

    // 1. society_prepare
    let prepare = json!({
        "version": "0.2", "op": "society_prepare",
        "meta": meta(&incarnation, &format!("{tag}-prep"), None),
        "home_authority_ref": "auth-home-1",
        "proposed_charter_ref": "charter-draft-1",
        "proposed_charter_digest": test_digest(0xa1),
        "classification_binding_ref": "class-bind-1",
        "classification_binding_digest": test_digest(0xa2),
    });
    let prepared = send(&mut daemon, "society_prepare", "governance", None, &prepare);
    let society_id = prepared["result"]["society_id"]
        .as_str()
        .unwrap()
        .to_owned();

    // 2. society_bootstrap (atomic genesis)
    let bootstrap = json!({
        "version": "0.2", "op": "society_bootstrap",
        "meta": meta(&incarnation, &format!("{tag}-boot"), Some(1)),
        "society_id": society_id,
        "preparation_ref": prepared["result"]["preparation_ref"],
        "subject_digest": prepared["result"]["subject_digest"],
    });
    let booted = send(
        &mut daemon,
        "society_bootstrap",
        "governance",
        None,
        &bootstrap,
    );
    let cursor = booted["source_cursor"].as_str().unwrap().to_owned();

    // 3. membership_offer (+ candidate channel)
    let subject = test_digest(0xb1);
    let offer = json!({
        "version": "0.2", "op": "membership_offer",
        "meta": meta(&incarnation, &format!("{tag}-offer"), None),
        "participant_ref": "part-agent-1",
        "proposed_standing_ref": "standing-proposal-1",
        "subject_digest": subject,
        "offered_by_decision_ref": "dec-offer-1",
        "expires_at": far_future(),
    });
    let offered = send(&mut daemon, "membership_offer", "governance", None, &offer);
    let offer_id = offered["result"]["offer_id"].as_str().unwrap().to_owned();
    let token = read_candidate_token(&daemon, &offer_id);

    // 4. membership_accept over the candidate channel
    let accept = json!({
        "version": "0.2", "op": "membership_accept",
        "meta": meta(&incarnation, &format!("{tag}-accept"), Some(1)),
        "offer_ref": offer_id,
        "subject_digest": subject,
    });
    let accepted = send(
        &mut daemon,
        "membership_accept",
        "candidate",
        Some(&token),
        &accept,
    );
    let acceptance_id = accepted["result"]["acceptance_id"]
        .as_str()
        .unwrap()
        .to_owned();

    // 5. participant_admit (Standing activation, channel conversion)
    let admit = json!({
        "version": "0.2", "op": "participant_admit",
        "meta": meta(&incarnation, &format!("{tag}-admit"), Some(2)),
        "offer_ref": offer_id,
        "membership_acceptance_ref": acceptance_id,
        "admitted_by_decision_ref": "dec-admit-1",
        "admission_subject_digest": subject,
    });
    send(&mut daemon, "participant_admit", "governance", None, &admit);

    // 6. manifestation_admit
    let events = daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "events_read",
                "continuation": cursor, "page_size": 512}),
    );
    let manifestation_id = events["result"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["kind"] == "manifestation.proposed")
        .expect("manifestation.proposed")["object_ref"]
        .as_str()
        .unwrap()
        .to_owned();
    let manif = json!({
        "version": "0.2", "op": "manifestation_admit",
        "meta": meta(&incarnation, &format!("{tag}-manif"), Some(1)),
        "manifestation_ref": manifestation_id,
        "admitted_by_decision_ref": "dec-manif-1",
    });
    send(
        &mut daemon,
        "manifestation_admit",
        "governance",
        None,
        &manif,
    );

    // The surviving ledger: every onboarding effect exactly once, the
    // witness generations dense, the participant's Standing active.
    let events = daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "events_read",
                "continuation": cursor, "page_size": 512}),
    );
    let list = events["result"]["events"].as_array().unwrap();
    for kind in [
        "society.genesis",
        "membership.offered",
        "membership.accepted",
        "membership.admitted",
        "standing.activated",
        "manifestation.admitted",
        "channel.converted",
    ] {
        let n = list.iter().filter(|e| e["kind"] == kind).count();
        let expected = if kind == "standing.activated" { 2 } else { 1 };
        assert_eq!(n, expected, "{tag}: {kind} count in {list:?}");
    }
    let entries = daemon.witness_entries();
    for (i, entry) in entries.iter().enumerate() {
        assert_eq!(entry["generation"].as_u64().unwrap(), i as u64 + 1, "{tag}");
    }
    let participant = daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "participant_show",
                "participant_ref": "part-agent-1"}),
    );
    assert_eq!(
        participant["result"]["state"], "active",
        "{tag}: {participant}"
    );
}

/// The refusal arm: crash membership_refuse at `phase`, restart, and the
/// exact retry returns the retained receipt.
fn run_refusal(phase: &str) {
    let tag = format!("cm-refuse-{phase}");
    let abort = format!("{phase}:membership_refuse");
    let mut daemon = TestDaemon::start_with_env(&tag, &[("BYOMD_ABORT", &abort)]);
    let (_sid, cursor, incarnation) = bootstrap_society(&daemon, &tag);
    let (offer_id, token, subject) =
        make_offer(&daemon, &incarnation, &tag, "part-agent-1", &far_future());
    let refuse = json!({
        "version": "0.2", "op": "membership_refuse",
        "meta": meta(&incarnation, &format!("{tag}-refuse"), Some(1)),
        "offer_ref": offer_id,
        "offer_subject_digest": subject,
        "refusal_reason_ref": "reason-1",
    });
    daemon.call_candidate_expect_death(&token, &refuse);
    daemon.restart(&[]);
    let retried = daemon.call_candidate(&token, &refuse);
    assert_eq!(retried["outcome"], "ok", "{tag}: {retried}");
    assert_eq!(retried["result"]["offer_state"], "refused");
    // Byte-stable receipt on every further exact retry, even though the
    // channel is now terminally fenced.
    let again = daemon.call_candidate(&token, &refuse);
    assert_eq!(again, retried, "{tag}: retained receipt");
    // Refused exactly once in the ledger; the terminal offer never
    // admits afterwards.
    let events = daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "events_read",
                "continuation": cursor, "page_size": 512}),
    );
    let refused = events["result"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == "membership.refused")
        .count();
    assert_eq!(refused, 1, "{tag}");
}

/// The slice-2 governed-work mutations: for each, the full attached
/// flow is driven with the daemon killed at the given §15.3 boundary of
/// that op, restarted, and the exact retry must answer (fresh execution
/// when provably unjournaled, byte-stable replay when recovered) with
/// every effect landing exactly once — the flow then completes to a
/// fulfilled pledge.
const SLICE2_CRASH_OPS: [&str; 16] = [
    "candidate_self_policy_propose",
    "mandate_prepare",
    "mandate_position",
    "mandate_issue",
    "activity_open",
    "wake_intent_submit",
    "continuation_write",
    "endeavor_propose",
    "endeavor_position",
    "endeavor_finalize",
    "call_open",
    "pledge_propose",
    "pledge_position",
    "pledge_finalize",
    "delivery_submit",
    "review_record",
];

fn run_slice2_flow(index: usize, op: &str, phase: &str) {
    // Short tag: the tag lands in the socket-directory path, which must
    // stay under the ~108-byte SUN_LEN cap.
    let abbrev: String = phase.split('_').filter_map(|w| w.chars().next()).collect();
    let tag = format!("cm2-{index}{abbrev}");
    let flow = governed_flow(&tag, FlowMode::CrashAt { op, phase });
    // The surviving ledger: dense witness generations, the pledge
    // fulfilled, the crash op's effect landed exactly once.
    let entries = flow.daemon.witness_entries();
    for (i, entry) in entries.iter().enumerate() {
        assert_eq!(
            entry["generation"].as_u64().unwrap(),
            i as u64 + 1,
            "{tag}: dense witness generations"
        );
    }
    let snapshot = flow.daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "snapshot_get",
                "society_id": flow.society_id, "kinds": ["pledges"]}),
    );
    let state = snapshot["result"]["pledges"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["pledge_id"] == json!(flow.pledge_id))
        .expect("pledge")["state"]
        .clone();
    assert_eq!(state, "fulfilled", "{tag}: {snapshot}");
}

#[test]
fn crash_matrix_slice2_before_witness() {
    for (i, op) in SLICE2_CRASH_OPS.iter().enumerate() {
        run_slice2_flow(i, op, "before_witness");
    }
}

#[test]
fn crash_matrix_slice2_after_witness() {
    for (i, op) in SLICE2_CRASH_OPS.iter().enumerate() {
        run_slice2_flow(i, op, "after_witness");
    }
}

#[test]
fn crash_matrix_slice2_before_finalize() {
    for (i, op) in SLICE2_CRASH_OPS.iter().enumerate() {
        run_slice2_flow(i, op, "before_finalize");
    }
}

#[test]
fn crash_matrix_slice2_after_finalize() {
    for (i, op) in SLICE2_CRASH_OPS.iter().enumerate() {
        run_slice2_flow(i, op, "after_finalize");
    }
}

#[test]
fn crash_matrix_before_witness() {
    for op in [
        "society_prepare",
        "society_bootstrap",
        "membership_offer",
        "membership_accept",
        "participant_admit",
        "manifestation_admit",
    ] {
        run_flow(op, "before_witness");
    }
    run_refusal("before_witness");
}

#[test]
fn crash_matrix_after_witness() {
    for op in [
        "society_prepare",
        "society_bootstrap",
        "membership_offer",
        "membership_accept",
        "participant_admit",
        "manifestation_admit",
    ] {
        run_flow(op, "after_witness");
    }
    run_refusal("after_witness");
}

#[test]
fn crash_matrix_before_finalize() {
    for op in [
        "society_prepare",
        "society_bootstrap",
        "membership_offer",
        "membership_accept",
        "participant_admit",
        "manifestation_admit",
    ] {
        run_flow(op, "before_finalize");
    }
    run_refusal("before_finalize");
}

#[test]
fn crash_matrix_after_finalize() {
    for op in [
        "society_prepare",
        "society_bootstrap",
        "membership_offer",
        "membership_accept",
        "participant_admit",
        "manifestation_admit",
    ] {
        run_flow(op, "after_finalize");
    }
    run_refusal("after_finalize");
}
