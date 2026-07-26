//! B3 slice 3 — a crash cell at EVERY new §15.3 commit point.
//!
//! One cell per (commit point, boundary): the daemon is started with
//! `BYOMD_ABORT=<phase>:<op>`, driven to that transition, killed
//! mid-request, restarted, and the EXACT retry must answer `ok` and then
//! replay byte-identically. The four §15.3 boundaries are
//! `before_witness`, `after_witness`, `before_finalize`, `after_finalize`.
//!
//! The nine new commit points: `attention_notice_record`,
//! `onboarding_offer`, `onboarding_compute_permit_consume`,
//! `onboarding_episode_claim`, `onboarding_episode_complete`,
//! `act_intent_prepare`, `act_intent_position`, `act_intent_finalize`, and
//! `execution_permit_consume`. Each is a one-shot or once-only record, so a
//! crash must never double-spend it: every cell also asserts the row count.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::runtime::{merge, portable_digest, sign_host_effect, Fixture, Subordinate};
use common::{far_future, test_digest};
use serde_json::{json, Value};

const PHASES: [&str; 4] = [
    "before_witness",
    "after_witness",
    "before_finalize",
    "after_finalize",
];

const BROKER: &str = "kovee-model-broker";
const CANDIDATE: &str = "part-cand-onb";

/// Kills at the abort point, restarts, retries the exact request, and
/// asserts the retry answers `ok` and replays byte-identically.
fn crash_and_replay(f: &mut Fixture, surface: &str, token: Option<&str>, request: &Value) -> Value {
    let line = request.to_string();
    let outcome = f.daemon.call_raw(surface, token, &line);
    assert!(
        outcome.is_err(),
        "expected the daemon to die mid-request, got {outcome:?}"
    );
    f.daemon.wait_exit();
    f.daemon.restart(&[]);
    let retried = f
        .daemon
        .call_raw(surface, token, &line)
        .unwrap_or_else(|e| panic!("retry after crash failed: {e}"));
    assert_eq!(retried["outcome"], "ok", "retry after crash: {retried}");
    let again = f.daemon.call_raw(surface, token, &line).unwrap();
    assert_eq!(again, retried, "the replay must be byte-identical");
    retried
}

fn env(phase: &str, op: &str) -> String {
    format!("{phase}:{op}")
}

// ============================================ the attention intake =======

#[test]
fn the_attention_notice_commit_point_survives_every_boundary() {
    for phase in PHASES {
        let abort = env(phase, "attention_notice_record");
        let mut f = Fixture::start_with_env(
            &format!("b3-crash-atn-{phase}"),
            8,
            &[("BYOMD_ABORT", &abort)],
        );
        let token = f.attention_token(&f.stream);
        let request = json!({
            "version": "0.2", "op": "attention_notice_record",
            "meta": f.meta("atn", None),
            "source_protocol": "kovee",
            "source_endpoint_ref": "kovee-endpoint-1",
            "source_event_ref": "kovee-event-9",
            "source_event_digest": portable_digest(0x7a),
            "activity_stream_ref": f.stream,
            "generation": 1,
            "stable_notice_key": "notice-crash-1",
        });
        let retried = crash_and_replay(&mut f, "runtime", Some(&token), &request);
        assert_eq!(
            retried["result"]["eligibility_effect"], "no_effect",
            "{phase}"
        );
        assert_eq!(
            f.count("SELECT COUNT(*) FROM attention_notices"),
            1,
            "{phase}: one notice, never two"
        );
        // And a crash still creates NOTHING: notification is never a wake.
        for table in [
            "wake_intents",
            "activation_admissions",
            "resource_allocations",
            "episodes",
        ] {
            assert_eq!(
                f.count(&format!("SELECT COUNT(*) FROM {table}")),
                0,
                "{phase}: a crashed notification woke nothing either"
            );
        }
    }
}

// ============================================ the onboarding path ========

struct Onboarding {
    offer_id: String,
    onboarding_id: String,
    compute_intent_ref: String,
    revision: u64,
}

fn membership_offer(f: &Fixture) -> String {
    let offered = f.governance(&json!({
        "version": "0.2", "op": "membership_offer",
        "meta": f.meta("onb-offer", None),
        "participant_ref": CANDIDATE,
        "proposed_standing_ref": "standing-proposal-onb",
        "subject_digest": test_digest(0xb7),
        "offered_by_decision_ref": format!("dec-society-{}", f.society_id),
        "expires_at": far_future(),
    }));
    assert_eq!(offered["outcome"], "ok", "{offered}");
    offered["result"]["offer_id"].as_str().unwrap().to_owned()
}

fn onboarding_offer_request(f: &Fixture, offer_id: &str) -> Value {
    json!({
        "version": "0.2", "op": "onboarding_offer",
        "meta": f.meta("onb-fund", None),
        "membership_offer_ref": offer_id,
        "candidate_participant_ref": CANDIDATE,
        "proposed_manifestation_ref": "manif-cand-onb",
        "proposed_manifestation_digest": test_digest(0xb8),
        "exact_context_ref": "ctx-onb-minimal",
        "exact_context_digest": test_digest(0xb9),
        "resource_reservation_ref": "resv-onb-1",
        "onboarding_compute_intent_ref": format!("oci-onb-{offer_id}"),
        "expires_at": far_future(),
        "adopted_by_decision_ref": format!("dec-offer-{offer_id}"),
    })
}

fn funded(f: &Fixture) -> Onboarding {
    let offer_id = membership_offer(f);
    let reply = f.governance(&onboarding_offer_request(f, &offer_id));
    assert_eq!(reply["outcome"], "ok", "{reply}");
    Onboarding {
        onboarding_id: format!("onb-{offer_id}"),
        compute_intent_ref: format!("oci-onb-{offer_id}"),
        revision: reply["result"]["revision"].as_u64().unwrap(),
        offer_id,
    }
}

fn compute_request(f: &Fixture, o: &Onboarding, revision: u64) -> Value {
    let digest = f
        .row(
            "SELECT digest FROM onboarding_compute_intents WHERE compute_intent_id = ?1",
            &o.compute_intent_ref,
        )
        .expect("the compute intent digest");
    json!({
        "version": "0.2", "op": "onboarding_compute_permit_consume",
        "meta": f.meta("occ", Some(revision)),
        "compute_intent_ref": o.compute_intent_ref,
        "compute_intent_digest": serde_json::from_str::<Value>(&digest).unwrap(),
        "stable_compute_key": format!("occ-{}", o.compute_intent_ref),
        "onboarding_fence_epoch": 1,
        "kovee_invocation_ref": "kovee-inv-onb-1",
        "provider_context_manifest_ref": "kovee-pcm-onb-1",
        "provider_context_manifest_digest": test_digest(0xc3),
        "disclosure_manifest_ref": "kovee-disclosure-onb-1",
        "disclosure_manifest_digest": test_digest(0xc4),
        "model_profile_ref": "kovee-model-profile-1",
        "model_profile_digest": test_digest(0xc5),
    })
}

fn claim_request(f: &Fixture, o: &Onboarding, receipt: Option<&str>) -> Value {
    let mut body = json!({
        "version": "0.2", "op": "onboarding_episode_claim",
        "meta": f.meta("onbclm", None),
        "onboarding_ref": o.onboarding_id,
        "candidate_participant_ref": CANDIDATE,
        "proposed_manifestation_ref": "manif-cand-onb",
        "proposed_manifestation_digest": test_digest(0xb8),
        "onboarding_fence_epoch": 1,
        "holder_runtime_binding": "candidate-worker-a",
        "stable_claim_key": "onbclaim-crash",
    });
    if let Some(receipt) = receipt {
        let digest = f
            .row(
                "SELECT digest FROM onboarding_compute_receipts WHERE receipt_id = ?1",
                receipt,
            )
            .expect("the receipt digest");
        merge(
            &mut body,
            json!({
                "compute_receipt_ref": receipt,
                "compute_receipt_digest": serde_json::from_str::<Value>(&digest).unwrap(),
            }),
        );
    }
    body
}

#[test]
fn the_onboarding_offer_commit_point_survives_every_boundary() {
    for phase in PHASES {
        let abort = env(phase, "onboarding_offer");
        let mut f = Fixture::start_with_env(
            &format!("b3-crash-onbo-{phase}"),
            8,
            &[("BYOMD_ABORT", &abort)],
        );
        let offer_id = membership_offer(&f);
        let request = onboarding_offer_request(&f, &offer_id);
        let retried = crash_and_replay(&mut f, "governance", None, &request);
        assert_eq!(retried["result"]["state"], "offered", "{phase}");
        assert_eq!(
            f.count("SELECT COUNT(*) FROM onboarding_offers"),
            1,
            "{phase}: one OnboardingActivationOffer, never two"
        );
        assert_eq!(
            f.count("SELECT COUNT(*) FROM onboarding_compute_intents"),
            1,
            "{phase}: one one-shot compute intent, never two"
        );
    }
}

#[test]
fn the_onboarding_compute_consume_commit_point_survives_every_boundary() {
    for phase in PHASES {
        let abort = env(phase, "onboarding_compute_permit_consume");
        let mut f = Fixture::start_with_env(
            &format!("b3-crash-occ-{phase}"),
            8,
            &[("BYOMD_ABORT", &abort)],
        );
        let o = funded(&f);
        let token = f.broker_token(&o.compute_intent_ref);
        let request = compute_request(&f, &o, o.revision);
        let retried = crash_and_replay(&mut f, "runtime", Some(&token), &request);
        assert_eq!(retried["result"]["max_uses"], 1, "{phase}");
        assert_eq!(
            f.count("SELECT COUNT(*) FROM onboarding_compute_receipts"),
            1,
            "{phase}: a crash never double-spends the ONE compute use"
        );
        assert_eq!(
            f.row(
                "SELECT state FROM onboarding_compute_intents WHERE compute_intent_id = ?1",
                &o.compute_intent_ref
            ),
            Some("consumed".to_owned()),
            "{phase}"
        );
    }
}

#[test]
fn the_onboarding_episode_claim_commit_point_survives_every_boundary() {
    for phase in PHASES {
        let abort = env(phase, "onboarding_episode_claim");
        let mut f = Fixture::start_with_env(
            &format!("b3-crash-onbclm-{phase}"),
            8,
            &[("BYOMD_ABORT", &abort)],
        );
        let o = funded(&f);
        let token = f.onboarding_token(&o.onboarding_id);
        let request = claim_request(&f, &o, None);
        let retried = crash_and_replay(&mut f, "runtime", Some(&token), &request);
        assert_eq!(retried["result"]["max_episodes"], 1, "{phase}");
        assert_eq!(retried["result"]["acceptance_effect"], "none", "{phase}");
        assert_eq!(
            f.count("SELECT COUNT(*) FROM onboarding_episodes"),
            1,
            "{phase}: at most ONE Episode per offer, crash or not"
        );
    }
}

#[test]
fn the_onboarding_episode_complete_commit_point_survives_every_boundary() {
    for phase in PHASES {
        let abort = env(phase, "onboarding_episode_complete");
        let mut f = Fixture::start_with_env(
            &format!("b3-crash-onbcmp-{phase}"),
            8,
            &[("BYOMD_ABORT", &abort)],
        );
        let o = funded(&f);
        let token = f.onboarding_token(&o.onboarding_id);
        let claimed = f.runtime(&token, &claim_request(&f, &o, None));
        assert_eq!(claimed["outcome"], "ok", "{claimed}");
        let episode_id = claimed["result"]["onboarding_episode_id"]
            .as_str()
            .unwrap()
            .to_owned();
        let request = json!({
            "version": "0.2", "op": "onboarding_episode_complete",
            "meta": f.meta("onbcmp", Some(1)),
            "onboarding_episode_ref": episode_id,
            "onboarding_ref": o.onboarding_id,
            "onboarding_fence_epoch": 1,
            "outcome": "completed",
            "output_refs": ["candidate-output-1"],
            "evidence_refs": ["candidate-evidence-1"],
        });
        let retried = crash_and_replay(&mut f, "runtime", Some(&token), &request);
        assert_eq!(
            retried["result"]["completion_is_evidence_only"], true,
            "{phase}"
        );
        // A crash never turns completion into acceptance.
        assert_eq!(
            f.row(
                "SELECT state FROM membership_offers WHERE offer_id = ?1",
                &o.offer_id
            ),
            Some("onboarding".to_owned()),
            "{phase}: completion is evidence only, even across a crash"
        );
        assert_eq!(
            f.number(
                "SELECT COUNT(*) FROM standing_revisions WHERE participant_ref = ?1",
                CANDIDATE
            ),
            Some(0),
            "{phase}"
        );
    }
}

// ============================================ the §13.1 act chain ========

fn act_prepare_request(f: &Fixture) -> Value {
    json!({
        "version": "0.2", "op": "act_intent_prepare",
        "meta": f.meta("actprep", None),
        "kind": "model_egress",
        "execution_kind": "external_effect",
        "subject_ref": "subject-crash-1",
        "subject_revision": 1,
        "mandate_ref": f.mandate_id,
        "mandate_revision": f.mandate_revision,
        "mandate_digest": f.mandate_subject_digest,
        // Both manifests are the HOST's, portable_public, and each is an
        // all-or-none pair pinned into the assented subject (A8/R3-A01).
        "context_manifest_ref": "ctxman-1",
        "context_manifest_digest": portable_digest(0xe1),
        "disclosure_manifest_ref": "disclosure-crash-1",
        "disclosure_manifest_digest": portable_digest(0xe2),
        "driver_audience": BROKER,
    })
}

#[test]
fn the_act_intent_prepare_commit_point_survives_every_boundary() {
    for phase in PHASES {
        let abort = env(phase, "act_intent_prepare");
        let mut f = Fixture::start_with_env(
            &format!("b3-crash-actprep-{phase}"),
            8,
            &[("BYOMD_ABORT", &abort)],
        );
        let request = act_prepare_request(&f);
        let token = f.agent_token.clone();
        let retried = crash_and_replay(&mut f, "participant", Some(&token), &request);
        assert_eq!(retried["result"]["state"], "prepared", "{phase}");
        assert_eq!(retried["result"]["act_class"], "model_egress", "{phase}");
        assert_eq!(
            f.count("SELECT COUNT(*) FROM act_intents"),
            1,
            "{phase}: one ActIntent, never two"
        );
        assert_eq!(
            f.count(
                "SELECT COUNT(*) FROM budget_reservations
                 WHERE holder_kind = 'act_intent'"
            ),
            1,
            "{phase}: one act reservation — never a double reserve"
        );
        assert!(f.ledger().conserves(), "{phase}: {:?}", f.ledger());
    }
}

#[test]
fn the_act_intent_position_commit_point_survives_every_boundary() {
    for phase in PHASES {
        let abort = env(phase, "act_intent_position");
        let mut f = Fixture::start_with_env(
            &format!("b3-crash-actpos-{phase}"),
            8,
            &[("BYOMD_ABORT", &abort)],
        );
        let prepared = f.prepare_act_raw("a1", "model_egress", Some(BROKER));
        assert_eq!(prepared["outcome"], "ok", "{prepared}");
        let r = &prepared["result"];
        let request = json!({
            "version": "0.2", "op": "act_intent_position",
            "meta": f.meta("actpos", None),
            "proposal_ref": r["intent_id"],
            "proposal_revision": 1,
            "subject_digest": r["subject_digest"],
            "seat_ref": r["required_seat_refs"][0],
            "value": "assent",
        });
        let intent_id = r["intent_id"].as_str().unwrap().to_owned();
        let retried = crash_and_replay(&mut f, "governance", None, &request);
        assert_eq!(retried["result"]["value"], "assent", "{phase}");
        assert_eq!(
            f.count("SELECT COUNT(*) FROM position_revisions WHERE proposal_kind = 'act_intent'"),
            1,
            "{phase}: one immutable PositionRevision, never two"
        );
        assert_eq!(
            f.row(
                "SELECT state FROM act_intents WHERE intent_id = ?1",
                &intent_id
            ),
            Some("awaiting_decision".to_owned()),
            "{phase}"
        );
    }
}

#[test]
fn the_act_intent_finalize_commit_point_survives_every_boundary() {
    for phase in PHASES {
        let abort = env(phase, "act_intent_finalize");
        let mut f = Fixture::start_with_env(
            &format!("b3-crash-actfin-{phase}"),
            8,
            &[("BYOMD_ABORT", &abort)],
        );
        let prepared = f.prepare_act_raw("a1", "model_egress", Some(BROKER));
        assert_eq!(prepared["outcome"], "ok", "{prepared}");
        let r = &prepared["result"];
        let intent_id = r["intent_id"].as_str().unwrap().to_owned();
        let positioned = f.governance(&json!({
            "version": "0.2", "op": "act_intent_position",
            "meta": f.meta("actpos", None),
            "proposal_ref": intent_id,
            "proposal_revision": 1,
            "subject_digest": r["subject_digest"],
            "seat_ref": r["required_seat_refs"][0],
            "value": "assent",
        }));
        assert_eq!(positioned["outcome"], "ok", "{positioned}");
        let request = json!({
            "version": "0.2", "op": "act_intent_finalize",
            "meta": f.meta("actfin", Some(1)),
            "intent_id": intent_id,
            "subject_digest": r["subject_digest"],
        });
        let retried = crash_and_replay(&mut f, "governance", None, &request);
        assert_eq!(retried["result"]["state"], "authorized", "{phase}");
        assert_eq!(
            f.count("SELECT COUNT(*) FROM governance_decisions WHERE kind = 'act_authorization'"),
            1,
            "{phase}: ONE GovernanceDecision, never two"
        );
    }
}

#[test]
fn the_execution_permit_consume_commit_point_survives_every_boundary() {
    for phase in PHASES {
        let abort = env(phase, "execution_permit_consume");
        let mut f = Fixture::start_with_env(
            &format!("b3-crash-perm-{phase}"),
            8,
            &[("BYOMD_ABORT", &abort)],
        );
        let wake = f.wake("w1");
        let ep = f.request_episode(&wake, "e1");
        f.admit_placement(&ep, "p1", Subordinate::Confirmed(200));
        let c = f.claim(&ep.episode_id, "worker-a", 600, 7, "c1");
        let started = f.start_episode(&ep.episode_id, &c, "s1");
        assert_eq!(started["outcome"], "ok", "{started}");
        let act = f.authorized_act("a1", "model_egress", BROKER);
        let token = f.permit_token(&act.intent_id);
        // The consumption the host retries after the crash: the SAME
        // registered Effect and the same semantic request, so the retry
        // recovers the retained receipt instead of asking for new authority.
        let mut request = f.consume_body(
            &act,
            "crash",
            "crash-1",
            &act.stable_execution_key,
            BROKER,
            Some(&ep.episode_id),
            c.byom_fence_epoch,
            c.kovee_invocation_fence,
            act.revision,
            portable_digest(0xf7),
        );
        sign_host_effect(&token, &mut request);
        let retried = crash_and_replay(&mut f, "runtime", Some(&token), &request);
        assert_eq!(retried["result"]["max_uses"], 1, "{phase}");
        // The one-shot never doubles: exactly one MandateUse and one
        // receipt, whatever boundary the crash struck (§14.8: a crash never
        // blindly repeats a non-idempotent effect).
        assert_eq!(
            f.count("SELECT COUNT(*) FROM mandate_uses"),
            1,
            "{phase}: MandateUse inserted exactly once"
        );
        assert_eq!(
            f.count("SELECT COUNT(*) FROM execution_consumption_receipts"),
            1,
            "{phase}: one ExecutionConsumptionReceipt"
        );
        assert_eq!(
            f.row(
                "SELECT state FROM act_intents WHERE intent_id = ?1",
                &act.intent_id
            ),
            Some("consumed".to_owned()),
            "{phase}"
        );
        assert!(f.ledger().conserves(), "{phase}: {:?}", f.ledger());
    }
}
