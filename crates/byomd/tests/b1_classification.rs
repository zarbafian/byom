//! §20.1 classification honesty (`b1_classification`): content
//! attributed to an `attached_harness` manifestation carries the Society
//! top label (quarantined from finer flows) UNLESS a
//! complete-readable-source attestation is cited; non-attached (human)
//! output keeps its declared classification.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::*;
use serde_json::{json, Value};

/// Drives one extra pledge → work → delivery on top of a completed flow;
/// returns the delivery reply.
#[allow(clippy::too_many_arguments)]
fn deliver(
    flow: &FlowOutcome,
    tag: &str,
    pledgor: &str,
    pledgor_token: Option<&str>,
    beneficiary: &str,
    beneficiary_token: Option<&str>,
    evidence_refs: Value,
) -> Value {
    let daemon = &flow.daemon;
    let incarnation = &flow.incarnation;
    let call = |token: Option<&str>, request: &Value| -> Value {
        daemon
            .call_raw("participant", token, &request.to_string())
            .unwrap_or_else(|e| panic!("{tag}: {e}\n{request}"))
    };
    let proposed = call(
        pledgor_token,
        &json!({
            "version": "0.2", "op": "pledge_propose",
            "meta": meta(incarnation, &format!("{tag}-prop"), None),
            "endeavor_id": flow.endeavor_id,
            "proposed_pledgor_ref": pledgor,
            "beneficiary_ref": beneficiary,
            "exact_outcome_schema_refs": ["schema-change-set-1"],
            "acceptance_criteria_refs": ["criteria-review-1"],
            "evidence_requirements": [],
            "reviewer_rule_ref": "rule-beneficiary-reviews",
            "input_context_ref": "context-input-1",
            "input_context_digest": test_digest(0xd2),
            "budget_request_set": {"items": [
                {"dimension": "unit", "canonical_unit": "unit",
                 "scale": 0, "max": 4}]},
            "allowed_manifestation_selector": bpa1_allow_all(),
            "delegation_ceiling": {"allowed": false, "max_depth": 0,
                                   "max_children": 0},
            "deadline": far_future(),
            "cancellation_terms": {"terms_ref": "terms-cancel-1",
                                   "terms_digest": test_digest(0xd3)},
            "dependency_refs": [],
        }),
    );
    assert_eq!(proposed["outcome"], "ok", "{tag}: {proposed}");
    let proposal_id = proposed["result"]["proposal_id"].as_str().unwrap();
    let terms_digest = proposed["result"]["terms_digest"].clone();
    let seat = |kind: &str| -> String {
        proposed["result"]["required_slots"]
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
        (pledgor, pledgor_token, "pledgor_assent"),
        (beneficiary, beneficiary_token, "beneficiary_assent"),
    ] {
        let positioned = call(
            token,
            &json!({
                "version": "0.2", "op": "pledge_position",
                "meta": meta(incarnation, &format!("{tag}-pos-{who}"), None),
                "proposal_ref": proposal_id,
                "proposal_revision": 1,
                "subject_digest": terms_digest,
                "seat_ref": seat(kind),
                "value": "assent",
                "assent_mode": "direct_participant",
            }),
        );
        assert_eq!(positioned["outcome"], "ok", "{tag}: {positioned}");
    }
    let finalized = call(
        pledgor_token,
        &json!({
            "version": "0.2", "op": "pledge_finalize",
            "meta": meta(incarnation, &format!("{tag}-fin"), Some(1)),
            "proposal_ref": proposal_id,
            "proposal_revision": 1,
            "subject_digest": terms_digest,
        }),
    );
    assert_eq!(finalized["outcome"], "ok", "{tag}: {finalized}");
    let pledge_id = finalized["result"]["pledge_id"].as_str().unwrap();
    let work = call(
        pledgor_token,
        &json!({
            "version": "0.2", "op": "activity_open",
            "meta": meta(incarnation, &format!("{tag}-work"), None),
            "kind": "pledge_work",
            "purpose_ref": "purpose-improve-1",
            "purpose_digest": test_digest(0xd4),
            "pledge_binding": {"pledge_id": pledge_id, "pledge_revision": 1,
                               "terms_digest": terms_digest},
            "mandate_refs": [],
            "budget_account_set_ref": format!("budget-endeavor-{tag}"),
        }),
    );
    assert_eq!(work["outcome"], "ok", "{tag}: {work}");
    call(
        pledgor_token,
        &json!({
            "version": "0.2", "op": "delivery_submit",
            "meta": meta(incarnation, &format!("{tag}-deliver"), None),
            "pledge_id": pledge_id,
            "pledge_revision": 2,
            "terms_digest": terms_digest,
            "output_refs": ["change-set-x"],
            "evidence_refs": evidence_refs,
            "activity_stream_ref": work["result"]["activity_stream_id"],
        }),
    )
}

#[test]
fn attached_harness_output_is_top_labeled_or_attested() {
    let flow = governed_flow("classify", FlowMode::Clean);

    // The flow's own delivery cited the complete-readable-source
    // attestation: admitted at the declared classification.
    let events = flow.daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "events_read",
                "continuation": flow.genesis_cursor, "page_size": 512}),
    );
    let submitted = events["result"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["kind"] == "delivery.submitted")
        .expect("delivery.submitted")
        .clone();
    let payload = flow.daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "event_payload",
                "event_id": submitted["event_id"]}),
    );
    assert_eq!(
        payload["result"]["payload"]["classification"], "attested",
        "{payload}"
    );

    // The same attached-harness pledgor WITHOUT the attestation: the
    // delivery is labeled at the Society top classification and the
    // reason says quarantine.
    let unattested = deliver(
        &flow,
        "classify-top",
        "part-agent-1",
        Some(&flow.agent_token.clone()),
        &flow.sovereign,
        None,
        json!([]),
    );
    assert_eq!(unattested["outcome"], "ok", "{unattested}");
    assert_eq!(
        unattested["result"]["classification"], "society_top",
        "{unattested}"
    );
    assert!(
        unattested["result"]["classification_reason"]
            .as_str()
            .unwrap()
            .contains("quarantined"),
        "{unattested}"
    );

    // Non-attached (human) output keeps its declared classification —
    // top-labeling is attribution-honesty, not a blanket penalty.
    let human = deliver(
        &flow,
        "classify-human",
        &flow.sovereign,
        None,
        "part-agent-1",
        Some(&flow.agent_token.clone()),
        json!([]),
    );
    assert_eq!(human["outcome"], "ok", "{human}");
    assert_eq!(human["result"]["classification"], "declared", "{human}");
}
