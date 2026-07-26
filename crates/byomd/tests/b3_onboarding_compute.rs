//! B3 slice 3 — the §7.4 onboarding one-shot compute path.
//!
//! ```text
//! onboarding_offer                  the Society FUNDS the invitation
//! onboarding_compute_permit_consume ONE hosted model call, ever
//! onboarding_episode_claim          the ONE onboarding Episode
//! onboarding_episode_complete       EVIDENCE ONLY — never acceptance
//! ```
//!
//! Three refusals this suite pins: a SECOND compute use, completion turning
//! into acceptance, and authority surviving `membership_refuse`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::runtime::Fixture;
use common::{far_future, kind_of, read_candidate_token, test_digest};
use serde_json::{json, Value};

const CANDIDATE: &str = "part-cand-onb";

struct Onboarding {
    offer_id: String,
    onboarding_id: String,
    compute_intent_ref: String,
    subject_digest: Value,
    revision: u64,
}

/// A funded OnboardingActivationOffer with its one-shot compute intent.
fn funded(f: &Fixture) -> Onboarding {
    let subject_digest = test_digest(0xb7);
    let offered = f.governance(&json!({
        "version": "0.2", "op": "membership_offer",
        "meta": f.meta("onb-offer", None),
        "participant_ref": CANDIDATE,
        "proposed_standing_ref": "standing-proposal-onb",
        "subject_digest": subject_digest,
        "offered_by_decision_ref": format!("dec-society-{}", f.society_id),
        "expires_at": far_future(),
    }));
    assert_eq!(offered["outcome"], "ok", "membership_offer: {offered}");
    let offer_id = offered["result"]["offer_id"].as_str().unwrap().to_owned();
    let onboarding_id = format!("onb-{offer_id}");
    let compute_intent_ref = format!("oci-{onboarding_id}");
    let reply = f.governance(&json!({
        "version": "0.2", "op": "onboarding_offer",
        "meta": f.meta("onb-fund", None),
        "membership_offer_ref": offer_id,
        "candidate_participant_ref": CANDIDATE,
        "proposed_manifestation_ref": "manif-cand-onb",
        "proposed_manifestation_digest": test_digest(0xb8),
        "exact_context_ref": "ctx-onb-minimal",
        "exact_context_digest": test_digest(0xb9),
        "resource_reservation_ref": "resv-onb-1",
        "onboarding_compute_intent_ref": compute_intent_ref,
        "expires_at": far_future(),
        "adopted_by_decision_ref": format!("dec-offer-{offer_id}"),
    }));
    assert_eq!(reply["outcome"], "ok", "onboarding_offer: {reply}");
    let r = &reply["result"];
    assert_eq!(r["state"], "offered");
    assert_eq!(r["max_episodes"], 1);
    assert_eq!(r["general_effect_and_child_authority"], "none");
    assert_eq!(r["membership_offer_state"], "onboarding");
    assert_eq!(
        r["allowed_operations"],
        json!([
            "membership_refuse",
            "membership_accept",
            "candidate_self_policy_propose"
        ]),
        "§7.4 verbatim: the candidate channel may only refuse, accept, or \
         return proposed policies"
    );
    Onboarding {
        onboarding_id,
        compute_intent_ref,
        subject_digest,
        revision: r["revision"].as_u64().unwrap(),
        offer_id,
    }
}

#[allow(clippy::too_many_arguments)]
fn consume_compute(f: &Fixture, o: &Onboarding, key: &str, revision: u64, seed: u8) -> Value {
    let token = f.broker_token(&o.compute_intent_ref);
    let digest = f
        .row(
            "SELECT digest FROM onboarding_compute_intents WHERE compute_intent_id = ?1",
            &o.compute_intent_ref,
        )
        .expect("the compute intent digest");
    f.runtime(
        &token,
        &json!({
            "version": "0.2", "op": "onboarding_compute_permit_consume",
            "meta": f.meta(&format!("occ-{key}"), Some(revision)),
            "compute_intent_ref": o.compute_intent_ref,
            "compute_intent_digest": serde_json::from_str::<Value>(&digest).unwrap(),
            "stable_compute_key": format!("occ-{}", o.compute_intent_ref),
            "onboarding_fence_epoch": 1,
            // The broker's invocation ref is stable across retries: only a
            // CHANGED canonical binding conflicts.
            "kovee_invocation_ref": "kovee-inv-onb-1",
            "provider_context_manifest_ref": "kovee-pcm-onb-1",
            "provider_context_manifest_digest": test_digest(seed),
            "disclosure_manifest_ref": "kovee-disclosure-onb-1",
            "disclosure_manifest_digest": test_digest(0xc4),
            "model_profile_ref": "kovee-model-profile-1",
            "model_profile_digest": test_digest(0xc5),
        }),
    )
}

fn claim(f: &Fixture, o: &Onboarding, receipt: Option<&str>, key: &str) -> Value {
    let token = f.onboarding_token(&o.onboarding_id);
    let mut body = json!({
        "version": "0.2", "op": "onboarding_episode_claim",
        "meta": f.meta(&format!("onbclm-{key}"), None),
        "onboarding_ref": o.onboarding_id,
        "candidate_participant_ref": CANDIDATE,
        "proposed_manifestation_ref": "manif-cand-onb",
        "proposed_manifestation_digest": test_digest(0xb8),
        "onboarding_fence_epoch": 1,
        "holder_runtime_binding": "candidate-worker-a",
        "stable_claim_key": format!("onbclaim-{key}"),
    });
    if let Some(receipt) = receipt {
        let digest = f
            .row(
                "SELECT digest FROM onboarding_compute_receipts WHERE receipt_id = ?1",
                receipt,
            )
            .expect("the receipt digest");
        common::runtime::merge(
            &mut body,
            json!({
                "compute_receipt_ref": receipt,
                "compute_receipt_digest": serde_json::from_str::<Value>(&digest).unwrap(),
            }),
        );
    }
    f.runtime(&token, &body)
}

#[test]
fn one_compute_use_only_and_a_second_consume_is_refused() {
    let f = Fixture::start("b3-onb-oneshot", 8);
    let o = funded(&f);
    assert_eq!(
        f.row(
            "SELECT state FROM onboarding_compute_intents WHERE compute_intent_id = ?1",
            &o.compute_intent_ref
        ),
        Some("authorized".to_owned())
    );

    let first = consume_compute(&f, &o, "c1", o.revision, 0xc3);
    assert_eq!(first["outcome"], "ok", "{first}");
    let receipt = &first["result"];
    assert_eq!(
        receipt["max_uses"], 1,
        "§7.4: at most ONE compute use per offer"
    );
    assert_eq!(receipt["grants"]["tools"], "none");
    assert_eq!(receipt["grants"]["network"], "none");
    assert_eq!(receipt["grants"]["workspace"], "none");
    assert_eq!(receipt["grants"]["children"], "none");
    assert_eq!(receipt["grants"]["reusable_participant_authority"], "none");
    let receipt_id = receipt["receipt_id"].as_str().unwrap().to_owned();
    assert_eq!(
        f.count("SELECT COUNT(*) FROM onboarding_compute_receipts"),
        1
    );
    assert_eq!(
        f.row(
            "SELECT state FROM onboarding_compute_intents WHERE compute_intent_id = ?1",
            &o.compute_intent_ref
        ),
        Some("consumed".to_owned()),
        "the one-shot authority is spent"
    );

    // A SECOND compute use with a different final manifest is refused: the
    // one-shot slot is exhausted, not re-negotiable.
    let second = consume_compute(&f, &o, "c2", o.revision + 1, 0x0c);
    assert_eq!(kind_of(&second), "idempotency_mismatch", "{second}");
    assert!(second["problem"]["detail"]
        .as_str()
        .unwrap()
        .contains("ONE compute use per offer"));
    assert_eq!(
        f.count("SELECT COUNT(*) FROM onboarding_compute_receipts"),
        1,
        "a second consume mints no second receipt"
    );

    // The exact retry (identical final manifests) recovers the stored
    // receipt rather than asking for new authority.
    let retry = consume_compute(&f, &o, "c3", o.revision + 1, 0xc3);
    assert_eq!(retry["outcome"], "ok", "{retry}");
    assert_eq!(retry["result"]["replayed"], true);
    assert_eq!(retry["result"]["receipt_id"], json!(receipt_id));
    assert_eq!(
        f.count("SELECT COUNT(*) FROM onboarding_compute_receipts"),
        1
    );
}

#[test]
fn completion_is_evidence_and_never_acceptance() {
    let f = Fixture::start("b3-onb-evidence", 8);
    let o = funded(&f);
    let consumed = consume_compute(&f, &o, "c1", o.revision, 0xc3);
    assert_eq!(consumed["outcome"], "ok", "{consumed}");
    let receipt_id = consumed["result"]["receipt_id"]
        .as_str()
        .unwrap()
        .to_owned();

    let claimed = claim(&f, &o, Some(&receipt_id), "k1");
    assert_eq!(claimed["outcome"], "ok", "{claimed}");
    let episode_id = claimed["result"]["onboarding_episode_id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(claimed["result"]["max_episodes"], 1);
    assert_eq!(claimed["result"]["acceptance_effect"], "none");
    assert_eq!(
        claimed["result"]["allowed_output_operations"],
        json!([
            "refuse",
            "membership_accept",
            "candidate_self_policy_propose"
        ]),
        "§7.4 verbatim: the output may only drive these three"
    );

    // A SECOND Episode under one offer is refused (max_episodes: 1).
    let second = claim(&f, &o, Some(&receipt_id), "k2");
    assert_eq!(kind_of(&second), "stale_binding", "{second}");
    assert!(second["problem"]["detail"]
        .as_str()
        .unwrap()
        .contains("max_episodes"));

    // Completion: EVIDENCE ONLY.
    let token = f.onboarding_token(&o.onboarding_id);
    let completed = f.runtime(
        &token,
        &json!({
            "version": "0.2", "op": "onboarding_episode_complete",
            "meta": f.meta("onbcmp", Some(1)),
            "onboarding_episode_ref": episode_id,
            "onboarding_ref": o.onboarding_id,
            "onboarding_fence_epoch": 1,
            "outcome": "completed",
            "output_refs": ["candidate-output-1"],
            "evidence_refs": ["candidate-evidence-1"],
        }),
    );
    assert_eq!(completed["outcome"], "ok", "{completed}");
    let r = &completed["result"];
    assert_eq!(r["completion_is_evidence_only"], true);
    assert_eq!(r["acceptance"]["membership_accepted"], false);
    assert_eq!(r["acceptance"]["membership_acceptance_ref"], Value::Null);
    assert_eq!(r["acceptance"]["standing_created"], false);
    assert_eq!(r["acceptance"]["participant_authority_granted"], false);

    // THE EVIDENCE, in the store: the MembershipOffer never left
    // `onboarding`, no acceptance exists, no Standing was created, and the
    // candidate holds no active Participant record.
    assert_eq!(
        f.row(
            "SELECT state FROM membership_offers WHERE offer_id = ?1",
            &o.offer_id
        ),
        Some("onboarding".to_owned()),
        "runtime output is never membership assent (§16.6 item 12)"
    );
    assert_eq!(
        f.row(
            "SELECT acceptance_id FROM membership_offers WHERE offer_id = ?1",
            &o.offer_id
        ),
        None,
        "no MembershipAcceptance follows from a completed compute"
    );
    assert_eq!(
        f.number(
            "SELECT COUNT(*) FROM standing_revisions WHERE participant_ref = ?1",
            CANDIDATE
        ),
        Some(0),
        "no Standing follows from a completed compute"
    );
    assert_eq!(
        f.row(
            "SELECT state FROM participants WHERE participant_id = ?1",
            CANDIDATE
        ),
        Some("proposed".to_owned()),
        "the candidate is still only proposed"
    );
    // Acceptance remains a CANDIDATE act on the candidate surface.
    let cand_token = read_candidate_token(&f.daemon, &o.offer_id);
    let accepted = f.daemon.call_candidate(
        &cand_token,
        &json!({
            "version": "0.2", "op": "membership_accept",
            "meta": f.meta("onb-accept", Some(2)),
            "offer_ref": o.offer_id,
            "subject_digest": o.subject_digest,
        }),
    );
    assert_eq!(accepted["outcome"], "ok", "{accepted}");
    assert_eq!(
        f.row(
            "SELECT state FROM membership_offers WHERE offer_id = ?1",
            &o.offer_id
        ),
        Some("accepted".to_owned()),
        "only the candidate's own act accepts"
    );
}

#[test]
fn a_refusal_fences_the_onboarding_workload() {
    let f = Fixture::start("b3-onb-refuse", 8);
    let o = funded(&f);
    // The workload holds its token BEFORE the refusal.
    let token = f.onboarding_token(&o.onboarding_id);
    let broker = f.broker_token(&o.compute_intent_ref);

    let cand_token = read_candidate_token(&f.daemon, &o.offer_id);
    let refused = f.daemon.call_candidate(
        &cand_token,
        &json!({
            "version": "0.2", "op": "membership_refuse",
            "meta": f.meta("onb-refuse", Some(2)),
            "offer_ref": o.offer_id,
            "offer_subject_digest": o.subject_digest,
        }),
    );
    assert_eq!(refused["outcome"], "ok", "{refused}");
    assert_eq!(
        refused["result"]["fence_epoch"], 2,
        "the refusal advances the onboarding fence"
    );

    // The OnboardingActivationOffer went terminal and unused compute
    // authority is revoked, in the SAME CAS transaction.
    assert_eq!(
        f.row(
            "SELECT state FROM onboarding_offers WHERE onboarding_id = ?1",
            &o.onboarding_id
        ),
        Some("refused".to_owned())
    );
    assert_eq!(
        f.row(
            "SELECT state FROM onboarding_compute_intents WHERE compute_intent_id = ?1",
            &o.compute_intent_ref
        ),
        Some("failed".to_owned()),
        "§7.4: refusal revokes unused onboarding compute authority"
    );
    // The workload's own token files are gone.
    assert!(!f.token_path_exists(&format!("runtime-onboarding-{}.token", o.onboarding_id)));
    assert!(!f.token_path_exists(&format!("runtime-broker-{}.token", o.compute_intent_ref)));

    // And the token it still holds no longer works: the fence is part of
    // the subject the credential binds.
    let stale_claim = f.runtime(
        &token,
        &json!({
            "version": "0.2", "op": "onboarding_episode_claim",
            "meta": f.meta("onbclm-after", None),
            "onboarding_ref": o.onboarding_id,
            "candidate_participant_ref": CANDIDATE,
            "proposed_manifestation_ref": "manif-cand-onb",
            "proposed_manifestation_digest": test_digest(0xb8),
            "onboarding_fence_epoch": 1,
            "holder_runtime_binding": "candidate-worker-a",
            "stable_claim_key": "onbclaim-after",
        }),
    );
    // The credential still binds fence 1, so byom's own gate answers: the
    // offer is terminal and no new invitation reuses it.
    assert_eq!(kind_of(&stale_claim), "stale_binding", "{stale_claim}");
    assert!(stale_claim["problem"]["detail"]
        .as_str()
        .unwrap()
        .contains("terminal offer never admits"));
    // Presenting the NEW fence does not help either: the credential binds
    // the fence, so a fence-2 subject never matches the fence-1 token.
    let new_fence = f.runtime(
        &token,
        &json!({
            "version": "0.2", "op": "onboarding_episode_claim",
            "meta": f.meta("onbclm-after2", None),
            "onboarding_ref": o.onboarding_id,
            "candidate_participant_ref": CANDIDATE,
            "proposed_manifestation_ref": "manif-cand-onb",
            "proposed_manifestation_digest": test_digest(0xb8),
            "onboarding_fence_epoch": 2,
            "holder_runtime_binding": "candidate-worker-a",
            "stable_claim_key": "onbclaim-after2",
        }),
    );
    assert_eq!(kind_of(&new_fence), "forbidden", "{new_fence}");

    // The compute permit is equally fenced.
    let digest = f
        .row(
            "SELECT digest FROM onboarding_compute_intents WHERE compute_intent_id = ?1",
            &o.compute_intent_ref,
        )
        .unwrap();
    let stale_compute = f.runtime(
        &broker,
        &json!({
            "version": "0.2", "op": "onboarding_compute_permit_consume",
            "meta": f.meta("occ-after", Some(o.revision)),
            "compute_intent_ref": o.compute_intent_ref,
            "compute_intent_digest": serde_json::from_str::<Value>(&digest).unwrap(),
            "stable_compute_key": format!("occ-{}", o.compute_intent_ref),
            "onboarding_fence_epoch": 1,
            "kovee_invocation_ref": "kovee-inv-onb-after",
            "provider_context_manifest_ref": "kovee-pcm-onb-1",
            "provider_context_manifest_digest": test_digest(0xc3),
            "disclosure_manifest_ref": "kovee-disclosure-onb-1",
            "disclosure_manifest_digest": test_digest(0xc4),
            "model_profile_ref": "kovee-model-profile-1",
            "model_profile_digest": test_digest(0xc5),
        }),
    );
    assert_eq!(kind_of(&stale_compute), "stale_binding", "{stale_compute}");
    assert_eq!(
        f.count("SELECT COUNT(*) FROM onboarding_compute_receipts"),
        0
    );
    assert_eq!(f.count("SELECT COUNT(*) FROM onboarding_episodes"), 0);
}
