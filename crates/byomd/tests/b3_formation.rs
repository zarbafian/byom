//! B3 slice 1 — `kovee_endeavor_form` (registry R39, DESIGN.md §16.3)
//! against the real daemon over the real governance socket.
//!
//! What each test pins:
//! - the happy path commits Position + GovernanceDecision + active
//!   Endeavor as ONE visible atomic result, and the returned envelope's
//!   digests are the ones Kovee independently recomputes;
//! - a replayed nonce re-serves the stored result and never re-executes,
//!   while a FRESH attempt over the same command replays it too (the
//!   stable-command/fresh-attempt split);
//! - `formation_requires_participation` leaves ZERO Society/Endeavor
//!   domain records and a non-reexecuting tombstone claiming the domain;
//! - wrong audience, workload, realm, or command are refused;
//! - a stale binding epoch or Society recovery epoch is refused;
//! - the gateway is never the genesis actor.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::kovee::{self, Bend, Seam};
use common::*;
use serde_json::{json, Value};

struct Fixture {
    daemon: TestDaemon,
    society: String,
    #[allow(dead_code)]
    incarnation: String,
    sovereign: String,
    seam: Seam,
    cursor: String,
}

fn fixture(tag: &str) -> Fixture {
    let mut daemon = TestDaemon::start(tag);
    let (society, cursor, incarnation) = bootstrap_society(&daemon, tag);
    let sovereign = sovereign_id(&daemon, &society);
    let seam = kovee::install_seam(&mut daemon, &society, &incarnation, 0);
    Fixture {
        daemon,
        society,
        incarnation,
        sovereign,
        seam,
        cursor,
    }
}

impl Fixture {
    fn form(&self, attempt: &kovee::Attempt) -> Value {
        self.daemon
            .call_raw(
                "governance",
                Some(&attempt.credential),
                &attempt.request.to_string(),
            )
            .unwrap_or_else(|e| panic!("form: {e}"))
    }

    fn endeavors(&self) -> Vec<Value> {
        let snapshot = self.daemon.call(
            "projection",
            &json!({"version": "0.2", "op": "snapshot_get",
                    "society_id": self.society, "kinds": ["endeavors"]}),
        );
        snapshot["result"]["endeavors"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    }

    fn events(&self) -> Vec<Value> {
        let page = self.daemon.call(
            "projection",
            &json!({"version": "0.2", "op": "events_read",
                    "continuation": self.cursor, "page_size": 512}),
        );
        page["result"]["events"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    }
}

#[test]
fn one_atomic_formation_from_the_delegated_principal_channel() {
    let fx = fixture("b3f1");
    let attempt = fx.seam.form(
        0,
        &fx.sovereign,
        "kovee-principal-1",
        "k-form-1",
        "nonce-1",
        kovee::proposal(&[&fx.sovereign], "b3f1"),
        kovee::position(&fx.sovereign, "assent"),
        &Bend::default(),
    );
    let reply = fx.form(&attempt);
    assert_eq!(reply["outcome"], "ok", "{reply}");
    let envelope = &reply["result"];

    // The envelope is exactly the frozen KoveeEndeavorFormResult record.
    let members: Vec<&str> = envelope
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let mut expected = vec![
        "kovee_formation_intent_ref",
        "canonical_command_digest",
        "society_ref",
        "society_recovery_epoch",
        "idempotency_domain_digest",
        "endeavor_ref",
        "endeavor_revision",
        "endeavor_digest",
        "formation_decision_ref",
        "formation_slot_snapshot_digest",
        "source_cursor",
        "digest",
    ];
    expected.sort_unstable();
    let mut got = members;
    got.sort_unstable();
    assert_eq!(got, expected, "result envelope members: {envelope}");

    // Kovee recomputes what the server pinned: the command digest it
    // sent, its own IdempotencyDomain digest, and the envelope digest.
    assert_eq!(
        envelope["canonical_command_digest"], attempt.canonical_command_digest,
        "the server echoes the exact command digest"
    );
    assert_eq!(
        envelope["idempotency_domain_digest"],
        attempt.idempotency_domain_digest
    );
    assert_eq!(envelope["society_ref"], json!(fx.society));
    assert_eq!(envelope["endeavor_revision"], json!(1));
    let mut without_digest = envelope.clone();
    without_digest.as_object_mut().unwrap().remove("digest");
    assert_eq!(
        envelope["digest"],
        kovee::portable(bpp_core::hostint::RESULT_TAG, &without_digest),
        "the envelope digest covers the envelope minus itself"
    );

    // ONE atomic commit: an ACTIVE Endeavor at revision 1 (never a
    // proposal to finish), its Position, its GovernanceDecision.
    let endeavors = fx.endeavors();
    assert_eq!(endeavors.len(), 1, "{endeavors:?}");
    assert_eq!(endeavors[0]["state"], "active");
    assert_eq!(endeavors[0]["revision"], json!(1));
    assert_eq!(endeavors[0]["endeavor_id"], envelope["endeavor_ref"]);

    let kinds: Vec<String> = fx
        .events()
        .iter()
        .filter_map(|e| e["kind"].as_str().map(str::to_owned))
        .filter(|k| {
            k.starts_with("endeavor.") || k.starts_with("kovee.") || k == "budget.delegated"
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "endeavor.position_recorded",
            "endeavor.finalized",
            "budget.delegated",
            "kovee.endeavor_formed",
        ]
        .map(str::to_owned),
        "one atomic commit emits the whole formation event set"
    );

    // The envelope's short source_cursor is a continuation this endpoint
    // minted, positioned at the head after the formation.
    let after = fx.daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "events_read",
                "continuation": envelope["source_cursor"], "page_size": 512}),
    );
    assert_eq!(after["outcome"], "ok", "{after}");
    assert_eq!(
        after["result"]["events"].as_array().map(Vec::len),
        Some(0),
        "source_cursor points after this formation's events"
    );

    // R41: the delegated principal recovers ITS OWN retained result on
    // its own channel — the sovereign's channel sees nothing of it.
    let recovered = fx
        .daemon
        .call_raw(
            "governance",
            Some(&attempt.credential),
            &json!({"version": "0.2", "op": "idempotency_result",
                    "operation": "kovee_endeavor_form",
                    "idempotency_key": "k-form-1"})
            .to_string(),
        )
        .unwrap();
    assert_eq!(recovered["outcome"], "ok", "{recovered}");
    assert_eq!(recovered["result"]["result"]["result"], *envelope);
    let sovereign_view = fx.daemon.call(
        "governance",
        &json!({"version": "0.2", "op": "idempotency_result",
                "operation": "kovee_endeavor_form",
                "idempotency_key": "k-form-1"}),
    );
    assert_eq!(kind_of(&sovereign_view), "not_found", "{sovereign_view}");
}

#[test]
fn a_replayed_nonce_returns_the_stored_result_and_never_reexecutes() {
    let fx = fixture("b3f2");
    let attempt = fx.seam.form(
        0,
        &fx.sovereign,
        "kovee-principal-1",
        "k-form-2",
        "nonce-a",
        kovee::proposal(&[&fx.sovereign], "b3f2"),
        kovee::position(&fx.sovereign, "assent"),
        &Bend::default(),
    );
    let first = fx.form(&attempt);
    assert_eq!(first["outcome"], "ok", "{first}");

    // Byte-identical replay of the exact attempt.
    let replayed = fx.form(&attempt);
    assert_eq!(
        replayed, first,
        "a replayed nonce re-serves the stored bytes"
    );

    // A FRESH attempt: new nonce, new attempt id, new proof — the same
    // stable command bytes. It replays the stored result, and still only
    // ONE Endeavor exists.
    let retry = fx.seam.form(
        0,
        &fx.sovereign,
        "kovee-principal-1",
        "k-form-2",
        "nonce-b",
        kovee::proposal(&[&fx.sovereign], "b3f2"),
        kovee::position(&fx.sovereign, "assent"),
        &Bend::default(),
    );
    assert_ne!(
        retry.request["authentication_proof"], attempt.request["authentication_proof"],
        "a fresh attempt carries its own proof"
    );
    assert_eq!(
        retry.canonical_command_digest, attempt.canonical_command_digest,
        "the stable command bytes are unchanged"
    );
    let retried = fx.form(&retry);
    assert_eq!(
        retried, first,
        "a fresh attempt over the same command replays the stored result"
    );
    assert_eq!(fx.endeavors().len(), 1, "exactly one formation");

    // Changed semantic bytes under the same key conflict, never reuse.
    let changed = fx.seam.form(
        0,
        &fx.sovereign,
        "kovee-principal-1",
        "k-form-2",
        "nonce-c",
        kovee::proposal(&[&fx.sovereign], "b3f2-different"),
        kovee::position(&fx.sovereign, "assent"),
        &Bend::default(),
    );
    let conflict = fx.form(&changed);
    assert_eq!(kind_of(&conflict), "idempotency_mismatch", "{conflict}");
    assert_eq!(fx.endeavors().len(), 1);
}

#[test]
fn formation_requires_participation_commits_no_domain_record_but_claims_the_domain() {
    let fx = fixture("b3f3");
    // Two required seats: the source principal cannot fill another
    // Participant's seat, so the operation must refuse.
    let attempt = fx.seam.form(
        0,
        &fx.sovereign,
        "kovee-principal-1",
        "k-form-3",
        "nonce-1",
        kovee::proposal(&[&fx.sovereign, "part-agent-1"], "b3f3"),
        kovee::position(&fx.sovereign, "assent"),
        &Bend::default(),
    );
    let refused = fx.form(&attempt);
    assert_eq!(
        kind_of(&refused),
        "formation_requires_participation",
        "{refused}"
    );
    let tombstone_ref = refused["problem"]["dev.byom.tombstone_ref"]
        .as_str()
        .expect("tombstone ref")
        .to_owned();
    assert_eq!(
        refused["problem"]["dev.byom.tombstone_reason_kind"],
        "formation_requires_participation"
    );

    // ZERO Society or Endeavor domain records.
    assert!(fx.endeavors().is_empty(), "no Endeavor was committed");
    let kinds: Vec<String> = fx
        .events()
        .iter()
        .filter_map(|e| e["kind"].as_str().map(str::to_owned))
        .filter(|k| k.starts_with("endeavor.") || k.starts_with("kovee."))
        .collect();
    assert_eq!(
        kinds,
        vec!["kovee.formation_tombstoned".to_owned()],
        "only the tombstone transition happened"
    );

    // The tombstone is non-reexecuting: the exact same attempt, and a
    // fresh attempt over the same command, both refuse identically.
    let again = fx.form(&attempt);
    assert_eq!(
        again, refused,
        "a replayed nonce re-serves the same refusal"
    );
    let fresh = fx.seam.form(
        0,
        &fx.sovereign,
        "kovee-principal-1",
        "k-form-3",
        "nonce-2",
        kovee::proposal(&[&fx.sovereign, "part-agent-1"], "b3f3"),
        kovee::position(&fx.sovereign, "assent"),
        &Bend::default(),
    );
    let fresh_reply = fx.form(&fresh);
    assert_eq!(
        kind_of(&fresh_reply),
        "formation_requires_participation",
        "{fresh_reply}"
    );
    assert_eq!(
        fresh_reply["problem"]["dev.byom.tombstone_ref"],
        json!(tombstone_ref),
        "the SAME tombstone answers; the domain is claimed once"
    );
    assert!(fx.endeavors().is_empty());

    // A policy-derived Position over a sole seat is refused the same
    // way: §16.3 forbids invoking an automatic assent policy here.
    let policy = fx.seam.form(
        0,
        &fx.sovereign,
        "kovee-principal-1",
        "k-form-3b",
        "nonce-3",
        kovee::proposal(&[&fx.sovereign], "b3f3b"),
        kovee::policy_position(&fx.sovereign),
        &Bend::default(),
    );
    let policy_reply = fx.form(&policy);
    assert_eq!(
        kind_of(&policy_reply),
        "formation_requires_participation",
        "{policy_reply}"
    );
    assert!(fx.endeavors().is_empty());
}

#[test]
fn wrong_audience_workload_realm_or_command_is_refused() {
    let fx = fixture("b3f4");
    let make = |key: &str, nonce: &str, bend: &Bend| {
        fx.seam.form(
            0,
            &fx.sovereign,
            "kovee-principal-1",
            key,
            nonce,
            kovee::proposal(&[&fx.sovereign], "b3f4"),
            kovee::position(&fx.sovereign, "assent"),
            bend,
        )
    };

    let audience = make(
        "k-aud",
        "n-aud",
        &Bend {
            audience: Some("aud-someone-else".into()),
            ..Bend::default()
        },
    );
    assert_eq!(kind_of(&fx.form(&audience)), "forbidden");

    let issuer = make(
        "k-iss",
        "n-iss",
        &Bend {
            issuer: Some("kovee-random-service".into()),
            ..Bend::default()
        },
    );
    assert_eq!(
        kind_of(&fx.form(&issuer)),
        "forbidden",
        "a generic Kovee service credential cannot become a principal"
    );

    // Wrong workload: a terminalize-only credential cannot form.
    let mut wrong_workload = make("k-wl", "n-wl", &Bend::default());
    let credential = fx.seam.credential(
        &fx.sovereign,
        "kovee-principal-1",
        "n-wl",
        &wrong_workload.canonical_command_digest,
        &["external_command_terminalize"],
        0,
        &Bend::default(),
    );
    wrong_workload.credential = Seam::preamble(&credential);
    assert_eq!(kind_of(&fx.form(&wrong_workload)), "forbidden");

    // Wrong realm binding.
    let realm = make(
        "k-realm",
        "n-realm",
        &Bend {
            realm_binding_ref: Some("krbb-other".into()),
            ..Bend::default()
        },
    );
    assert_eq!(kind_of(&fx.form(&realm)), "stale_binding");

    // Wrong command: the credential is bound to another prepared subject.
    let other = make("k-other", "n-other", &Bend::default());
    let mut wrong_command = make(
        "k-cmd",
        "n-cmd",
        &Bend {
            credential_subject_digest: Some(other.canonical_command_digest.clone()),
            ..Bend::default()
        },
    );
    let refused = fx.form(&wrong_command);
    assert_eq!(kind_of(&refused), "forbidden", "{refused}");

    // A tampered attempt proof (fresh nonce, stale proof) is refused.
    wrong_command = make("k-proof", "n-proof", &Bend::default());
    wrong_command.request["authentication_proof"] = json!("ap1.".to_owned() + &"0".repeat(64));
    let stale_proof = fx.form(&wrong_command);
    assert_eq!(kind_of(&stale_proof), "forbidden", "{stale_proof}");

    assert!(fx.endeavors().is_empty(), "nothing was committed");
}

#[test]
fn a_stale_binding_or_recovery_epoch_is_refused() {
    let fx = fixture("b3f5");
    let make = |key: &str, nonce: &str, bend: &Bend| {
        fx.seam.form(
            0,
            &fx.sovereign,
            "kovee-principal-1",
            key,
            nonce,
            kovee::proposal(&[&fx.sovereign], "b3f5"),
            kovee::position(&fx.sovereign, "assent"),
            bend,
        )
    };
    let stale_epoch = make(
        "k-epoch",
        "n-epoch",
        &Bend {
            realm_binding_epoch: Some(9),
            ..Bend::default()
        },
    );
    assert_eq!(kind_of(&fx.form(&stale_epoch)), "stale_binding");

    let stale_recovery = make(
        "k-rec",
        "n-rec",
        &Bend {
            society_recovery_epoch: Some(7),
            ..Bend::default()
        },
    );
    assert_eq!(kind_of(&fx.form(&stale_recovery)), "stale_binding");

    let stale_participant = make(
        "k-part",
        "n-part",
        &Bend {
            participant_binding_epoch: Some(99),
            ..Bend::default()
        },
    );
    assert_eq!(kind_of(&fx.form(&stale_participant)), "stale_binding");

    // A superseded endpoint incarnation never reaches the new domain.
    let mut old_incarnation = make("k-inc", "n-inc", &Bend::default());
    old_incarnation.request["meta"]["expected_endpoint_incarnation"] = json!("inc-old");
    assert_eq!(kind_of(&fx.form(&old_incarnation)), "stale_binding");

    // An expired credential is outside its short §14.4 window.
    let expired = make(
        "k-exp",
        "n-exp",
        &Bend {
            expired: true,
            ..Bend::default()
        },
    );
    assert_eq!(kind_of(&fx.form(&expired)), "forbidden");

    assert!(fx.endeavors().is_empty(), "nothing was committed");
}

#[test]
fn the_kovee_gateway_is_never_the_genesis_actor() {
    let mut daemon = TestDaemon::start("b3f6");
    let (society, _cursor, incarnation) = bootstrap_society(&daemon, "b3f6");
    let sovereign = sovereign_id(&daemon, &society);
    let seam = kovee::install_seam(&mut daemon, &society, &incarnation, 0);

    // A formation naming a Society that does not exist must be refused
    // with the native path, never bootstrap one.
    let attempt = seam.form(
        0,
        &sovereign,
        "kovee-principal-1",
        "k-genesis",
        "n-genesis",
        kovee::proposal(&[&sovereign], "b3f6"),
        kovee::position(&sovereign, "assent"),
        &Bend {
            society_ref: Some("soc-does-not-exist".into()),
            ..Bend::default()
        },
    );
    let refused = daemon
        .call_raw(
            "governance",
            Some(&attempt.credential),
            &attempt.request.to_string(),
        )
        .unwrap();
    assert_eq!(kind_of(&refused), "forbidden", "{refused}");
    assert!(
        refused["problem"]["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("society_prepare/society_bootstrap"),
        "the refusal names the native genesis path: {refused}"
    );

    // The delegated-principal channel is the ONLY channel for R39: the
    // same-UID sovereign governance channel cannot stand in for it.
    let no_credential = daemon
        .call_raw("governance", None, &attempt.request.to_string())
        .unwrap();
    assert_eq!(kind_of(&no_credential), "forbidden", "{no_credential}");

    // And a delegated principal cannot reach the sovereign's operations.
    let sovereign_op = daemon
        .call_raw(
            "governance",
            Some(&attempt.credential),
            &json!({"version": "0.2", "op": "membership_offer",
                    "meta": meta(&incarnation, "b3f6-offer", None),
                    "participant_ref": "part-agent-1",
                    "proposed_standing_ref": "standing-proposal-1",
                    "subject_digest": test_digest(0xb1),
                    "offered_by_decision_ref": society_decision(&daemon),
                    "expires_at": far_future()})
            .to_string(),
        )
        .unwrap();
    assert_eq!(kind_of(&sovereign_op), "forbidden", "{sovereign_op}");

    // R39 answers on governance only — never the participant surface.
    let wrong_surface = daemon
        .call_raw("participant", None, &attempt.request.to_string())
        .unwrap();
    assert_eq!(
        kind_of(&wrong_surface),
        "forbidden_surface",
        "{wrong_surface}"
    );
}
