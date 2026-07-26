//! B3 slice 3 — NOTIFICATION IS NEVER A WAKE (DESIGN.md §11.1/§11.2/§16.4;
//! family contract L25).
//!
//! §16.4: "Kovee Attention may notify the Byom adapter of an admitted exact
//! event. Byom alone decides whether a Participant's WakeIntent and
//! ActivityStream permit a new Episode." This suite pins the whole of that
//! sentence:
//!
//! - a notice commits as EVIDENCE and creates nothing;
//! - its at-most effect is that the participant's OWN already-submitted
//!   WakeIntent, citing this exact cause, becomes ELIGIBLE under its
//!   ALREADY ADOPTED ActivationPolicy;
//! - no notice creates a WakeIntent, an ActivationAdmission, a
//!   ResourceAllocation, or an Episode;
//! - the intake never crosses to a wake-authoring surface.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::runtime::{portable_digest, Fixture};
use common::{bpa1_allow_all, far_future, kind_of, test_digest};
use serde_json::{json, Value};

fn notify(f: &Fixture, key: &str, cause: &str) -> Value {
    let token = f.attention_token(&f.stream);
    f.runtime(
        &token,
        &json!({
            "version": "0.2", "op": "attention_notice_record",
            "meta": f.meta(&format!("atn-{key}"), None),
            "source_protocol": "kovee",
            "source_endpoint_ref": "kovee-endpoint-1",
            "source_event_ref": cause,
            "source_event_digest": portable_digest(0x7a),
            "activity_stream_ref": f.stream,
            "generation": 1,
            "stable_notice_key": format!("notice-{key}"),
        }),
    )
}

/// The four records only the four-stage activation may create.
fn activation_rows(f: &Fixture) -> (i64, i64, i64, i64) {
    (
        f.count("SELECT COUNT(*) FROM wake_intents"),
        f.count("SELECT COUNT(*) FROM activation_admissions"),
        f.count("SELECT COUNT(*) FROM resource_allocations"),
        f.count("SELECT COUNT(*) FROM episodes"),
    )
}

#[test]
fn a_notification_alone_creates_no_admission_no_allocation_and_no_episode() {
    let f = Fixture::start("b3-attn-none", 8);
    assert_eq!(activation_rows(&f), (0, 0, 0, 0), "nothing has woken yet");

    let recorded = notify(&f, "n1", "kovee-event-9");
    assert_eq!(recorded["outcome"], "ok", "{recorded}");
    let r = &recorded["result"];
    // The notice is committed evidence, and it names what it did NOT do.
    assert_eq!(r["eligibility_effect"], "no_effect");
    assert_eq!(r["created"]["wake_intent"], false);
    assert_eq!(r["created"]["activation_admission"], false);
    assert_eq!(r["created"]["resource_allocation"], false);
    assert_eq!(r["created"]["episode"], false);
    assert_eq!(r["eligible_wake_intent_ref"], Value::Null);
    assert_eq!(
        r["required_stages"].as_array().unwrap().len(),
        6,
        "the four-stage activation plus request and claim still has to happen"
    );
    assert_eq!(f.count("SELECT COUNT(*) FROM attention_notices"), 1);

    // THE EVIDENCE: after a notification, the four activation records are
    // still empty. Nothing about attention advanced any of them.
    assert_eq!(
        activation_rows(&f),
        (0, 0, 0, 0),
        "a notification alone creates no wake intent, no admission, no \
         allocation and no episode (§11.1, family contract L25)"
    );
    assert_eq!(f.count("SELECT COUNT(*) FROM episode_lease_heads"), 0);
    assert_eq!(f.count("SELECT COUNT(*) FROM byom_episode_bindings"), 0);
    assert!(f.ledger().conserves(), "{:?}", f.ledger());

    // Ten more notices change nothing: attention cannot accumulate into
    // activation either.
    for i in 0..10 {
        let again = notify(&f, &format!("n-burst-{i}"), &format!("kovee-event-{i}"));
        assert_eq!(again["outcome"], "ok", "{again}");
        assert_eq!(again["result"]["eligibility_effect"], "no_effect");
    }
    assert_eq!(
        activation_rows(&f),
        (0, 0, 0, 0),
        "ranking is not activation"
    );
    assert_eq!(f.count("SELECT COUNT(*) FROM attention_notices"), 11);

    // The exact retry returns the identical notice, never a second record.
    let replayed = notify(&f, "n1", "kovee-event-9");
    assert_eq!(
        replayed, recorded,
        "the exact retry replays byte-identically"
    );
    assert_eq!(f.count("SELECT COUNT(*) FROM attention_notices"), 11);
}

#[test]
fn a_notification_at_most_makes_the_participants_own_wake_intent_eligible() {
    let f = Fixture::start("b3-attn-eligible", 8);
    // The participant adopts its OWN ActivationPolicy first.
    let adopted = f.participant(&json!({
        "version": "0.2", "op": "activation_policy_adopt",
        "meta": f.meta("apol", None),
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
    }));
    assert_eq!(adopted["outcome"], "ok", "{adopted}");

    // ... and authors its OWN WakeIntent against the exact cause. Only the
    // Participant channel can do this (R29).
    let wake = f.participant(&json!({
        "version": "0.2", "op": "wake_intent_submit",
        "meta": f.meta("wake-atn", None),
        "activity_stream_ref": f.stream,
        "generation": 1,
        "origin": "direct_participant",
        "exact_cause_ref": "kovee-event-42",
        "exact_cause_digest": test_digest(0xc2),
        "purpose_ref": "purpose-explore-1",
        "stable_wake_key": "wake-atn-1",
        "expires_at": far_future(),
    }));
    assert_eq!(wake["outcome"], "ok", "{wake}");
    let wake_id = wake["result"]["wake_intent_id"]
        .as_str()
        .unwrap()
        .to_owned();

    // A notice for a DIFFERENT cause has no effect at all.
    let other = notify(&f, "n-other", "kovee-event-99");
    assert_eq!(
        other["result"]["eligibility_effect"], "no_effect",
        "{other}"
    );

    // The notice for the EXACT cause: at most eligibility, nothing more.
    let recorded = notify(&f, "n-exact", "kovee-event-42");
    assert_eq!(recorded["outcome"], "ok", "{recorded}");
    let r = &recorded["result"];
    assert_eq!(r["eligibility_effect"], "wake_intent_eligible");
    assert_eq!(r["eligible_wake_intent_ref"], json!(wake_id));
    assert!(
        !r["activation_policy_ref"].is_null(),
        "under an ADOPTED policy"
    );
    // Still: no admission, no allocation, no episode. The kernel stages are
    // the only path, and the participant's own `episode_request` drives them.
    assert_eq!(
        activation_rows(&f),
        (1, 0, 0, 0),
        "the ONE wake intent is the participant's own; eligibility is not \
         admission (§11.1)"
    );
    assert_eq!(
        f.row(
            "SELECT state FROM wake_intents WHERE wake_intent_id = ?1",
            &wake_id
        ),
        Some("submitted".to_owned()),
        "eligibility does not consume or advance the participant's intent"
    );

    // Only now, driven by the PARTICIPANT, do the stages run.
    let requested = f.request_episode_raw(&wake_id, "e-atn");
    assert_eq!(requested["outcome"], "ok", "{requested}");
    assert_eq!(activation_rows(&f), (1, 1, 1, 1));
}

#[test]
fn the_attention_channel_cannot_wake_and_never_crosses_surfaces() {
    let f = Fixture::start("b3-attn-surface", 8);
    let token = f.attention_token(&f.stream);

    // The attention token is bound to one exact ActivityStream generation:
    // a notice for another generation never matches.
    let wrong_generation = f.runtime(
        &token,
        &json!({
            "version": "0.2", "op": "attention_notice_record",
            "meta": f.meta("atn-gen", None),
            "source_protocol": "kovee",
            "source_endpoint_ref": "kovee-endpoint-1",
            "source_event_ref": "kovee-event-1",
            "source_event_digest": portable_digest(0x7a),
            "activity_stream_ref": f.stream,
            "generation": 2,
            "stable_notice_key": "notice-gen-2",
        }),
    );
    assert_eq!(
        kind_of(&wrong_generation),
        "forbidden",
        "{wrong_generation}"
    );

    // The attention channel is not a worker channel: presenting the
    // attention token to a lease command is refused.
    let stolen = f.runtime(
        &token,
        &json!({
            "version": "0.2", "op": "episode_claim",
            "meta": f.meta("atn-claim", None),
            "episode_ref": "ep-nope",
            "generation": 1,
            "holder_runtime_binding": "attention-adapter",
            "claim_subject_digest": test_digest(0xd1),
            "lease_ttl_seconds": 60,
            "kovee_invocation_ref": "kovee-inv-x",
            "kovee_invocation_fence": 7,
            "stable_binding_key": "bindkey-atn",
            "context_manifest_ref": "ctxman-1",
            "context_manifest_digest": test_digest(0xd2),
            "context_source_digest": portable_digest(0xd3),
            "mandate_use_refs": [],
            "allowed_local_commitments": [],
        }),
    );
    let stolen_kind = kind_of(&stolen);
    assert!(
        stolen_kind == "forbidden" || stolen_kind == "not_found",
        "{stolen}"
    );

    // And the intake is a RUNTIME row only: it answers on no other surface.
    for surface in ["participant", "governance"] {
        let body = json!({
            "version": "0.2", "op": "attention_notice_record",
            "meta": f.meta(&format!("atn-x-{surface}"), None),
            "source_protocol": "kovee",
            "source_endpoint_ref": "kovee-endpoint-1",
            "source_event_ref": "kovee-event-1",
            "source_event_digest": portable_digest(0x7a),
            "activity_stream_ref": f.stream,
            "generation": 1,
            "stable_notice_key": format!("notice-x-{surface}"),
        });
        let reply = if surface == "participant" {
            f.participant(&body)
        } else {
            f.governance(&body)
        };
        assert_eq!(kind_of(&reply), "forbidden_surface", "{surface}: {reply}");
    }
    assert_eq!(activation_rows(&f), (0, 0, 0, 0));
}
