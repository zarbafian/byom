//! `participation_cease` mid-work (`b1_cease`, §7.4/R12): self-only and
//! unconditional with immediate credential fencing; wrong-actor and
//! conditional-exit attempts fail; the exact retry replays the retained
//! receipt through the closed channel; committed obligations are
//! dispositioned independently — never silently destroyed.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::*;
use serde_json::json;

#[test]
fn cease_mid_work_is_self_only_unconditional_and_replayable() {
    // Mid-work: the full governed flow up to underway pledge work, then
    // the agent ceases while its pledge_work stream is open.
    let flow = governed_flow("cease", FlowMode::Clean);
    let daemon = &flow.daemon;
    let incarnation = &flow.incarnation;

    // Open a SECOND pledge-independent exploration so "mid-work" holds
    // beyond the fulfilled pledge: reuse the still-active mandate.
    let explore = daemon
        .call_raw(
            "participant",
            Some(&flow.agent_token),
            &json!({
                "version": "0.2", "op": "activity_open",
                "meta": meta(incarnation, "cease-open2", None),
                "kind": "exploration",
                "purpose_ref": "purpose-explore-1",
                "purpose_digest": test_digest(0xc5),
                "mandate_refs": [flow.mandate_id],
                "budget_account_set_ref": "budget-mandate-1",
            })
            .to_string(),
        )
        .unwrap();
    assert_eq!(explore["outcome"], "ok", "{explore}");

    // A conditional exit fails the CLOSED schema: participation_cease
    // admits no condition member at all.
    let conditional = daemon
        .call_raw(
            "participant",
            Some(&flow.agent_token),
            &json!({
                "version": "0.2", "op": "participation_cease",
                "meta": meta(incarnation, "cease-cond", Some(2)),
                "conditions": ["only-if-reviewed"],
            })
            .to_string(),
        )
        .unwrap();
    assert_eq!(kind_of(&conditional), "invalid", "{conditional}");

    // Wrong actor: participation_cease is bound to the participant
    // surface only — governance cannot cease anyone (forbidden_surface,
    // decided by the registry rows).
    let via_gov = daemon.call(
        "governance",
        &json!({
            "version": "0.2", "op": "participation_cease",
            "meta": meta(incarnation, "cease-gov", Some(2)),
        }),
    );
    assert_eq!(kind_of(&via_gov), "forbidden_surface", "{via_gov}");
    // And no request member can NAME another participant: the affected
    // Participant is channel-derived, so a "participant_ref" member
    // fails the closed schema — the sovereign cannot cease the agent.
    let named = daemon
        .call_raw(
            "participant",
            None,
            &json!({
                "version": "0.2", "op": "participation_cease",
                "meta": meta(incarnation, "cease-name", Some(2)),
                "participant_ref": "part-agent-1",
            })
            .to_string(),
        )
        .unwrap();
    assert_eq!(kind_of(&named), "invalid", "{named}");

    // The agent ceases: self-only, unconditional, immediate.
    let cease = json!({
        "version": "0.2", "op": "participation_cease",
        "meta": meta(incarnation, "cease-go", Some(2)),
        "statement_ref": "statement-goodbye-1",
    });
    let ceased = daemon
        .call_raw("participant", Some(&flow.agent_token), &cease.to_string())
        .unwrap();
    assert_eq!(ceased["outcome"], "ok", "{ceased}");
    assert_eq!(ceased["result"]["participant_state"], "retiring");
    assert_eq!(ceased["result"]["standing_status"], "ceased");

    // Immediate fencing: any NEW mutation over the closed channel is
    // non-enumerating forbidden...
    let fenced = daemon
        .call_raw(
            "participant",
            Some(&flow.agent_token),
            &json!({
                "version": "0.2", "op": "activity_open",
                "meta": meta(incarnation, "cease-after", None),
                "kind": "exploration",
                "purpose_ref": "purpose-explore-1",
                "purpose_digest": test_digest(0xc6),
                "mandate_refs": [flow.mandate_id],
                "budget_account_set_ref": "budget-mandate-1",
            })
            .to_string(),
        )
        .unwrap();
    assert_eq!(kind_of(&fenced), "forbidden", "{fenced}");
    // ...and the participant token file is gone.
    let token_path = daemon
        .data_dir
        .join("channels")
        .join("participant-part-agent-1.token");
    assert!(!token_path.exists(), "token file fenced at cease");

    // Replay idempotent: the EXACT cease request replays its retained
    // receipt byte-identically through the closed channel.
    let replay = daemon
        .call_raw("participant", Some(&flow.agent_token), &cease.to_string())
        .unwrap();
    assert_eq!(replay, ceased, "exact replay through the closed channel");
    // A malformed body is still a shape error (schemas are public)...
    let tweaked = daemon
        .call_raw(
            "participant",
            Some(&flow.agent_token),
            &json!({
                "version": "0.2", "op": "participation_cease",
                "meta": meta(incarnation, "cease-go", Some(2)),
                "statement_ref": "statement-другое",
            })
            .to_string(),
        )
        .unwrap();
    assert_eq!(kind_of(&tweaked), "invalid", "{tweaked}");
    // ...while any WELL-FORMED non-exact request over the closed channel
    // is non-enumerating forbidden.
    let tweaked2 = daemon
        .call_raw(
            "participant",
            Some(&flow.agent_token),
            &json!({
                "version": "0.2", "op": "participation_cease",
                "meta": meta(incarnation, "cease-go", Some(2)),
                "statement_ref": "statement-else-1",
            })
            .to_string(),
        )
        .unwrap();
    assert_eq!(kind_of(&tweaked2), "forbidden", "{tweaked2}");

    // Obligations are dispositioned INDEPENDENTLY: the fulfilled pledge
    // survives untouched, the open streams are not silently destroyed,
    // and the ledger says so.
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
        .expect("pledge survives cease")
        .clone();
    assert_eq!(pledge["state"], "fulfilled", "{snapshot}");
    let activities = snapshot["result"]["activities"].as_array().unwrap();
    assert!(
        activities.iter().any(|a| a["state"] == "ready"),
        "open streams survive for independent disposition: {snapshot}"
    );
    let events = daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "events_read",
                "continuation": flow.genesis_cursor, "page_size": 512}),
    );
    let cease_event = events["result"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["kind"] == "participation.ceased")
        .expect("participation.ceased event")
        .clone();
    // The cease receipt names the independent-disposition rule via its
    // payload; verify through the privacy-gated payload read.
    let payload = daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "event_payload",
                "event_id": cease_event["event_id"]}),
    );
    assert_eq!(payload["outcome"], "ok", "{payload}");
    assert!(
        payload["result"]["payload"]["obligations"]
            .as_str()
            .unwrap_or_default()
            .contains("independently"),
        "{payload}"
    );

    // The sovereign remains active and the society is unaffected.
    let sovereign = daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "participant_show",
                "participant_ref": flow.sovereign}),
    );
    assert_eq!(sovereign["result"]["state"], "active");
    let agent = daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "participant_show",
                "participant_ref": "part-agent-1"}),
    );
    assert_eq!(agent["result"]["state"], "retiring");
}
