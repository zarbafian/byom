//! B1 slice-1 end-to-end over the real per-surface sockets: negotiation
//! on every surface, atomic genesis, the onboarding happy path
//! (offer → candidate acceptance over the channel token → admit ×2 →
//! Standing active), and the dense, complete causal event timeline.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::*;
use serde_json::json;

#[test]
fn negotiation_answers_on_every_surface() {
    let daemon = TestDaemon::start("e2e-nego");
    for surface in ["governance", "candidate", "participant", "projection"] {
        // The candidate socket still takes its token preamble; an empty
        // token line is fine for pre-auth negotiation.
        let hello = json!({"version": "0.2", "op": "hello"});
        let reply = if surface == "candidate" {
            daemon.call_candidate("", &hello)
        } else {
            daemon.call(surface, &hello)
        };
        assert_eq!(reply["outcome"], "ok", "{surface}: {reply}");
        assert_eq!(reply["result"]["surface"], surface);
        assert_eq!(reply["result"]["versions"][0], "0.2");
    }
    let info = daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "protocol_info"}),
    );
    assert_eq!(info["result"]["limits"]["request_bytes_max"], 262_144);
    assert_eq!(info["result"]["limits"]["events_page_items_max"], 512);
    let features = daemon.call(
        "governance",
        &json!({"version": "0.2", "op": "feature_info"}),
    );
    let names: Vec<&str> = features["result"]["features"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["feature"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"b0.1-slice1"), "{names:?}");
    // The journal recovery profile is honestly advertised as developer
    // recovery, never production rollback resistance (§15.3).
    assert!(
        names.contains(&"authority-journal:developer-recovery"),
        "{names:?}"
    );
    // An unknown version is refused before op dispatch.
    let wrong = daemon.call("governance", &json!({"version": "9.9", "op": "hello"}));
    assert_eq!(kind_of(&wrong), "unsupported_version");
}

#[test]
fn genesis_onboarding_admission_and_dense_events() {
    let daemon = TestDaemon::start("e2e-flow");
    let (society_id, genesis_cursor, incarnation) = bootstrap_society(&daemon, "flow");

    // The Society is active with Charter r1 and the sovereign seat.
    let society = daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "society_show", "society_id": society_id}),
    );
    assert_eq!(society["outcome"], "ok", "{society}");
    assert_eq!(society["result"]["state"], "active");
    assert_eq!(society["result"]["recovery_epoch"], 0);
    assert_eq!(
        society["result"]["charter_head_digest"]["class"],
        "local_erasure_safe"
    );

    // Offer → the candidate channel token file is minted, offer-scoped.
    let (offer_id, token, subject_digest) =
        make_offer(&daemon, &incarnation, "flow", "part-agent-1", &far_future());

    // The candidate accepts over its channel (§7.4: only the candidate
    // authors acceptance; acceptance is not Standing).
    let accepted = accept_offer(
        &daemon,
        &incarnation,
        &token,
        "flow",
        &offer_id,
        &subject_digest,
        1,
    );
    assert_eq!(accepted["outcome"], "ok", "{accepted}");
    assert_eq!(accepted["result"]["offer_state"], "accepted");
    let acceptance_id = accepted["result"]["acceptance_id"].as_str().unwrap();

    // Acceptance is not Standing: the participant is still not projected.
    let shown = daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "participant_show", "participant_ref": "part-agent-1"}),
    );
    assert_eq!(kind_of(&shown), "not_found", "no Standing before admission");

    // Governance admits: offer CAS + acceptance binding + Standing
    // activation + channel conversion in one authority transaction.
    let admitted = daemon.call(
        "governance",
        &json!({
            "version": "0.2", "op": "participant_admit",
            "meta": meta(&incarnation, "flow-admit", Some(2)),
            "offer_ref": offer_id,
            "membership_acceptance_ref": acceptance_id,
            "admitted_by_decision_ref": offer_decision(&offer_id),
            "admission_subject_digest": subject_digest,
        }),
    );
    assert_eq!(admitted["outcome"], "ok", "{admitted}");
    assert_eq!(admitted["result"]["participant_state"], "active");
    assert_eq!(admitted["result"]["offer_state"], "admitted");
    assert_eq!(admitted["result"]["standing_status"], "active");
    assert_eq!(admitted["result"]["binding_epoch"], 1);
    assert_eq!(
        admitted["result"]["activated_self_policy_refs"],
        json!([]),
        "no self-policies existed to activate"
    );

    // The candidate channel closed; its token file is gone; a
    // participant channel token was minted.
    let cand_token = daemon
        .data_dir
        .join("channels")
        .join(format!("candidate-{offer_id}.token"));
    assert!(!cand_token.exists(), "candidate token fenced at admission");
    let part_token = daemon
        .data_dir
        .join("channels")
        .join("participant-part-agent-1.token");
    assert!(part_token.exists(), "participant channel minted");

    // The proposed attached_harness manifestation is admitted second.
    let manifestation_id = {
        // Find it via the event ledger below; for the call we read from
        // the manifestation.proposed event's object_ref.
        let events = daemon.call(
            "projection",
            &json!({"version": "0.2", "op": "events_read",
                    "continuation": genesis_cursor, "page_size": 512}),
        );
        let list = events["result"]["events"].as_array().unwrap().clone();
        list.iter()
            .find(|e| e["kind"] == "manifestation.proposed")
            .expect("manifestation.proposed event")["object_ref"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let manifested = daemon.call(
        "governance",
        &json!({
            "version": "0.2", "op": "manifestation_admit",
            "meta": meta(&incarnation, "flow-manif", Some(1)),
            "manifestation_ref": manifestation_id,
            "admitted_by_decision_ref": manifestation_decision(&manifestation_id),
        }),
    );
    assert_eq!(manifested["outcome"], "ok", "{manifested}");
    assert_eq!(manifested["result"]["status"], "active");

    // Standing active, visible on the projection surface.
    let participant = daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "participant_show", "participant_ref": "part-agent-1"}),
    );
    assert_eq!(participant["outcome"], "ok", "{participant}");
    assert_eq!(participant["result"]["state"], "active");
    assert_eq!(participant["result"]["kind"], "agent");
    assert_eq!(participant["result"]["binding_epoch"], 1);

    // The event ledger is dense (per-Society sequences 1..=N, no gaps)
    // and carries the complete causal timeline.
    let events = daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "events_read",
                "continuation": genesis_cursor, "page_size": 512}),
    );
    assert_eq!(events["outcome"], "ok", "{events}");
    let list = events["result"]["events"].as_array().unwrap();
    let kinds: Vec<&str> = list.iter().map(|e| e["kind"].as_str().unwrap()).collect();
    for expected in [
        "society.prepared",
        "society.genesis",
        "charter.adopted",
        "participant.admitted",
        "standing.activated",
        "budget.roots_established",
        "membership.offered",
        "participant.proposed",
        "manifestation.proposed",
        "channel.candidate_minted",
        "membership.accepted",
        "membership.admitted",
        "channel.converted",
        "manifestation.admitted",
    ] {
        assert!(kinds.contains(&expected), "missing {expected}: {kinds:?}");
    }
    // Dense: reading from genesis returns every sequence exactly once.
    // (Sequences are not on the wire; density is proven by pagination:
    // page one event at a time and the chain never skips or stalls.)
    let mut cursor = genesis_cursor.clone();
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
        assert!(walked <= 64, "runaway pagination");
    }
    assert_eq!(
        walked,
        list.len(),
        "single-step pagination covers the ledger"
    );

    // Every mutation's source_cursor continues the same ledger.
    assert!(admitted["source_cursor"].is_string());

    // A byte-identical mutation replay returns the retained result.
    let replay = daemon.call(
        "governance",
        &json!({
            "version": "0.2", "op": "manifestation_admit",
            "meta": meta(&incarnation, "flow-manif", Some(1)),
            "manifestation_ref": manifestation_id,
            "admitted_by_decision_ref": manifestation_decision(&manifestation_id),
        }),
    );
    assert_eq!(replay, manifested, "idempotent replay is byte-identical");
}

#[test]
fn wrong_society_and_unknown_records_are_not_found() {
    let daemon = TestDaemon::start("e2e-nf");
    bootstrap_society(&daemon, "nf");
    let reply = daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "society_show", "society_id": "soc-does-not-exist"}),
    );
    assert_eq!(kind_of(&reply), "not_found");
    // A structured cursor claiming an audience fails closed at the
    // schema (the token is authenticated and opaque).
    let bad = daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "events_read",
                "continuation": "ct-not-minted-by-the-server", "page_size": 8}),
    );
    assert_eq!(kind_of(&bad), "invalid");
}
