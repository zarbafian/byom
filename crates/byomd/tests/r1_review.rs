//! The R1 tracer-review corrections, each proved by the exact defect the
//! review found (reviews/2026-07-26-r1-tracer.md,
//! reviews/2026-07-26-r1-dispositions.md).
//!
//! Every test here fails against the reviewed behaviour: the assertions
//! are written against what USED to succeed.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::*;
use serde_json::{json, Value};

/// Opens the daemon's database directly — the adversary/inspection
/// channel the crash suites already use.
fn db(daemon: &TestDaemon) -> rusqlite::Connection {
    rusqlite::Connection::open(daemon.data_dir.join("byom.db")).unwrap()
}

// ----------------------------------------------------------- BY-A1 (P0) ----

/// Governance operations resolve an immutable, current
/// `GovernanceDecision` before preparing any mutation. The reviewed
/// build accepted any identifier-shaped string: a literal `dec-1`
/// created a proposed Participant, an active Standing and an active
/// Manifestation.
#[test]
fn governance_ops_resolve_a_real_current_decision_and_nothing_else() {
    let daemon = TestDaemon::start("r1-decision");
    let (society_id, _cursor, incarnation) = bootstrap_society(&daemon, "d1");
    let (offer_id, token, subject) =
        make_offer(&daemon, &incarnation, "d1", "part-agent-1", &far_future());
    let accepted = accept_offer(&daemon, &incarnation, &token, "d1", &offer_id, &subject, 1);
    let acceptance_id = accepted["result"]["acceptance_id"]
        .as_str()
        .unwrap()
        .to_owned();

    let admit_with = |key: &str, decision: &str| -> Value {
        daemon.call(
            "governance",
            &json!({
                "version": "0.2", "op": "participant_admit",
                "meta": meta(&incarnation, key, Some(2)),
                "offer_ref": offer_id,
                "membership_acceptance_ref": acceptance_id,
                "admitted_by_decision_ref": decision,
                "admission_subject_digest": subject,
            }),
        )
    };

    // 1. A LITERAL reference. This is the review's exact finding: it
    //    used to succeed.
    let literal = admit_with("d1-literal", "dec-1");
    assert_eq!(
        kind_of(&literal),
        "decision_incomplete",
        "a literal decision reference must fail closed: {literal}"
    );

    // 2. An ABSENT (well-shaped but unformed) reference.
    let absent = admit_with("d1-absent", "dec-offer-offer-does-not-exist");
    assert_eq!(kind_of(&absent), "decision_incomplete", "{absent}");

    // 3. The WRONG SUBJECT: the Society's genesis decision genuinely
    //    exists and seats this very actor — but it decides the Society,
    //    not this offer.
    let wrong_subject = admit_with("d1-subject", &format!("dec-society-{society_id}"));
    assert_eq!(
        kind_of(&wrong_subject),
        "decision_incomplete",
        "a real decision over another subject must fail closed: {wrong_subject}"
    );

    // 4. STALE: the decision's dependency closure no longer matches the
    //    Society (a changed charter head invalidates it).
    let decision_ref = offer_decision(&offer_id);
    {
        let conn = db(&daemon);
        let closure: String = conn
            .query_row(
                "SELECT dependency_closure FROM governance_decisions WHERE decision_id = ?1",
                [&decision_ref],
                |r| r.get(0),
            )
            .unwrap();
        let mut closure: Value = serde_json::from_str(&closure).unwrap();
        closure["charter_head_ref"] = json!("charter-superseded");
        conn.execute(
            "UPDATE governance_decisions SET dependency_closure = ?2 WHERE decision_id = ?1",
            rusqlite::params![decision_ref, closure.to_string()],
        )
        .unwrap();
    }
    let stale = admit_with("d1-stale", &decision_ref);
    assert_eq!(
        kind_of(&stale),
        "decision_incomplete",
        "a decision whose dependency closure moved is not current: {stale}"
    );

    // 5. WRONG ACTOR: restore the closure but reseat the decision on
    //    somebody else. Nothing the acting actor holds seats it.
    {
        let conn = db(&daemon);
        let closure: String = conn
            .query_row(
                "SELECT dependency_closure FROM governance_decisions WHERE decision_id = ?1",
                [&format!("dec-society-{society_id}")],
                |r| r.get(0),
            )
            .unwrap();
        conn.execute(
            "UPDATE governance_decisions SET dependency_closure = ?2, seat_snapshot = ?3
             WHERE decision_id = ?1",
            rusqlite::params![
                decision_ref,
                closure,
                json!([{ "seat_ref": "seat-someone-else",
                         "participant_ref": "part-agent-1",
                         "actor_ref": "participant:part-agent-1",
                         "participant_binding_epoch": 0 }])
                .to_string()
            ],
        )
        .unwrap();
    }
    let wrong_actor = admit_with("d1-actor", &decision_ref);
    assert_eq!(
        kind_of(&wrong_actor),
        "decision_incomplete",
        "the acting actor must hold a current seat in the snapshot: {wrong_actor}"
    );

    // Nothing above prepared a mutation: the candidate holds no Standing
    // and no admission event exists.
    {
        let conn = db(&daemon);
        let standings: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM standing_revisions WHERE participant_ref = 'part-agent-1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(standings, 0, "no Standing was activated");
        let admissions: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM events WHERE kind = 'membership.admitted'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(admissions, 0, "no admission was journaled");
    }

    // 6. Only the GENUINE current decision admits. (The tampered row is
    //    restored by re-forming it through a fresh offer, so this
    //    exercises the real formation path.)
    let daemon2 = TestDaemon::start("r1-decision-ok");
    let (_sid2, _c2, inc2) = bootstrap_society(&daemon2, "d2");
    let (offer2, token2, subject2) =
        make_offer(&daemon2, &inc2, "d2", "part-agent-1", &far_future());
    let accepted2 = accept_offer(&daemon2, &inc2, &token2, "d2", &offer2, &subject2, 1);
    let ok = daemon2.call(
        "governance",
        &json!({
            "version": "0.2", "op": "participant_admit",
            "meta": meta(&inc2, "d2-admit", Some(2)),
            "offer_ref": offer2,
            "membership_acceptance_ref": accepted2["result"]["acceptance_id"],
            "admitted_by_decision_ref": offer_decision(&offer2),
            "admission_subject_digest": subject2,
        }),
    );
    assert_eq!(ok["outcome"], "ok", "the genuine decision admits: {ok}");
}

/// `membership_offer` itself resolves the Society's genesis decision:
/// the root of the onboarding chain is not exempt.
#[test]
fn membership_offer_resolves_the_society_decision() {
    let daemon = TestDaemon::start("r1-offer-decision");
    let (society_id, _cursor, incarnation) = bootstrap_society(&daemon, "od");
    let offer = |key: &str, decision: &str| -> Value {
        daemon.call(
            "governance",
            &json!({
                "version": "0.2", "op": "membership_offer",
                "meta": meta(&incarnation, key, None),
                "participant_ref": "part-agent-1",
                "proposed_standing_ref": "standing-proposal-1",
                "subject_digest": test_digest(0xb1),
                "offered_by_decision_ref": decision,
                "expires_at": far_future(),
            }),
        )
    };
    let literal = offer("od-literal", "dec-offer-1");
    assert_eq!(kind_of(&literal), "decision_incomplete", "{literal}");
    let genuine = offer("od-ok", &format!("dec-society-{society_id}"));
    assert_eq!(genuine["outcome"], "ok", "{genuine}");
}

// --------------------------------------------------------------- BY-J2 ----

/// Recovery reproduces byte-identical values. The reviewed build
/// generated event sequences, timestamps, payload secrets, digests and
/// outbox bytes AFTER witnessing, so a recovered transaction produced
/// different values; the old test only counted events.
#[test]
fn a_recovered_transaction_reproduces_every_byte() {
    let mut daemon =
        TestDaemon::start_with_env("r1-j2", &[("BYOMD_ABORT", "after_witness:society_prepare")]);
    let incarnation = daemon.incarnation();
    let prepare = json!({
        "version": "0.2", "op": "society_prepare",
        "meta": meta(&incarnation, "j2-prep", None),
        "home_authority_ref": "auth-home-1",
        "proposed_charter_ref": "charter-draft-1",
        "proposed_charter_digest": test_digest(0xa1),
        "classification_binding_ref": "class-bind-1",
        "classification_binding_digest": test_digest(0xa2),
    });
    // Crash between the witness receipt and SQL finalize.
    daemon.call_expect_death("governance", &prepare);
    daemon.restart(&[]);
    // Startup recovery finalized the witnessed transition from the
    // stored pending set. The retry replays the retained bytes.
    let replay = daemon.call("governance", &prepare);
    assert_eq!(replay["outcome"], "ok", "{replay}");

    let conn = db(&daemon);
    // The witness receipt covers the exact result bytes and the exact
    // transition, and finalize persisted the verified receipt.
    let (receipt, result_digest): (String, String) = conn
        .query_row(
            "SELECT receipt, result_digest FROM authority_pending WHERE state = 'finalized'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    let receipt: Value = serde_json::from_str(&receipt).unwrap();
    assert_eq!(receipt["result_digest"], json!(result_digest));
    assert!(receipt["signature"].as_str().is_some_and(|s| !s.is_empty()));
    assert!(receipt["prior_entry_digest"].as_str().is_some());
    assert_eq!(receipt["generation"], json!(1));

    // The retained idempotency result IS the recovered bytes.
    let stored: Vec<u8> = conn
        .query_row("SELECT result FROM idempotency_records", [], |r| r.get(0))
        .unwrap();
    let stored: Value = serde_json::from_slice(&stored).unwrap();
    assert_eq!(stored, replay, "the replay is the finalized bytes");

    // Event sequence, timestamp, payload secret and payload digest were
    // all fixed BEFORE witnessing: the entry's result digest is the
    // digest of exactly these bytes.
    let entries = daemon.witness_entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["result_digest"], json!(result_digest));
    let (seq, occurred_at, secret): (i64, String, String) = conn
        .query_row(
            "SELECT sequence, occurred_at, payload_secret FROM events",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(seq, 1);
    assert!(!occurred_at.is_empty() && secret.len() == 64);
}

// --------------------------------------------------------------- BY-J3 ----

/// An INTERIOR audit alteration with an otherwise consistent journal
/// seals the endpoint. The reviewed build never called the audit
/// verifier and compared no checkpoint, so it reopened active.
#[test]
fn an_interior_audit_alteration_seals_at_startup() {
    let mut daemon = TestDaemon::start("r1-j3");
    let (_sid, _cursor, _inc) = bootstrap_society(&daemon, "j3");
    daemon.stop();
    {
        let conn = db(&daemon);
        // Alter one INTERIOR record's detail — the journal, mirror and
        // pending rows stay perfectly consistent.
        let seq: i64 = conn
            .query_row("SELECT MIN(seq) + 1 FROM audit", [], |r| r.get(0))
            .unwrap();
        let changed = conn
            .execute(
                "UPDATE audit SET detail = detail || ' (altered)' WHERE seq = ?1",
                [seq],
            )
            .unwrap();
        assert_eq!(changed, 1);
    }
    daemon.restart(&[]);
    let reply = daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "society_show", "society_id": "soc-anything"}),
    );
    assert_eq!(
        kind_of(&reply),
        "endpoint_sealed",
        "an altered audit chain must seal every non-diagnostic surface: {reply}"
    );
    // The diagnostic remainder still answers (§15.3).
    let hello = daemon.call("projection", &json!({"version": "0.2", "op": "hello"}));
    assert_eq!(hello["outcome"], "ok", "{hello}");
}

/// A rolled-back audit chain (fewer records than the witnessed
/// checkpoint) seals too.
#[test]
fn an_audit_chain_behind_its_checkpoint_seals() {
    let mut daemon = TestDaemon::start("r1-j3-roll");
    let (_sid, _cursor, _inc) = bootstrap_society(&daemon, "j3r");
    daemon.stop();
    {
        let conn = db(&daemon);
        conn.execute(
            "DELETE FROM audit WHERE seq = (SELECT MAX(seq) FROM audit)",
            [],
        )
        .unwrap();
    }
    daemon.restart(&[]);
    let reply = daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "society_show", "society_id": "soc-anything"}),
    );
    assert_eq!(kind_of(&reply), "endpoint_sealed", "{reply}");
}

// --------------------------------------------------------------- BY-C1 ----

/// The candidate credential is a sender-constrained proof, not a bearer
/// token: a proof copied out of one process fails in another same-UID
/// process, and a proof minted for one operation does not authorize a
/// different one.
#[test]
fn a_copied_proof_fails_from_another_same_uid_process() {
    let daemon = TestDaemon::start("r1-c1");
    let (_sid, _cursor, incarnation) = bootstrap_society(&daemon, "c1");
    let (offer_id, credential, subject) =
        make_offer(&daemon, &incarnation, "c1", "part-agent-1", &far_future());

    let accept = json!({
        "version": "0.2", "op": "membership_accept",
        "meta": meta(&incarnation, "c1-accept", Some(1)),
        "offer_ref": offer_id,
        "subject_digest": subject,
    });
    // A proof minted by ANOTHER process (a different pid/start time):
    // this is exactly "a copied token from another same-UID process".
    let foreign = byomd::channel::mint_proof(
        &credential,
        "membership_accept",
        byomd::channel::Peer {
            pid: std::process::id() as i32 + 1,
            process_start: 1,
        },
        bpp_core::time::unix_now(),
    )
    .unwrap();
    let stolen = daemon
        .call_raw("candidate", Some(&foreign), &accept.to_string())
        .unwrap();
    assert_eq!(
        kind_of(&stolen),
        "forbidden",
        "a proof bound to another process must not verify here: {stolen}"
    );

    // A proof for a DIFFERENT operation does not authorize this one.
    let wrong_op = byomd::channel::mint_proof(
        &credential,
        "membership_refuse",
        byomd::channel::Peer::current(),
        bpp_core::time::unix_now(),
    )
    .unwrap();
    let crossed = daemon
        .call_raw("candidate", Some(&wrong_op), &accept.to_string())
        .unwrap();
    assert_eq!(kind_of(&crossed), "forbidden", "{crossed}");

    // A proof is spent once: replaying the very same line is refused.
    let mine = byomd::channel::mint_proof(
        &credential,
        "membership_accept",
        byomd::channel::Peer::current(),
        bpp_core::time::unix_now(),
    )
    .unwrap();
    let first = daemon
        .call_raw("candidate", Some(&mine), &accept.to_string())
        .unwrap();
    assert_eq!(first["outcome"], "ok", "{first}");
    let replayed = daemon
        .call_raw("candidate", Some(&mine), &accept.to_string())
        .unwrap();
    assert_eq!(
        kind_of(&replayed),
        "forbidden",
        "a spent proof nonce must not verify twice: {replayed}"
    );

    // The store keeps a VERIFIER, never a reusable credential.
    let conn = db(&daemon);
    let (key_id, ops): (String, String) = conn
        .query_row(
            "SELECT proof_key_id, operations FROM channel_credentials WHERE audience = 'candidate'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(key_id.len(), 32, "a key id, not a key");
    let ops: Vec<String> = serde_json::from_str(&ops).unwrap();
    assert!(ops.contains(&"membership_accept".to_owned()));
    assert!(
        !ops.contains(&"mandate_prepare".to_owned()),
        "the credential authorizes an EXACT operation set"
    );
}

// --------------------------------------------------------------- BY-C2 ----

/// Closed-channel replay is ONLY the exact refusal that closed the
/// channel. The reviewed build returned any stored result for any
/// candidate operation, so an exact old `membership_accept` still
/// succeeded after admission — its test used a FRESH idempotency key and
/// therefore missed it.
#[test]
fn only_the_exact_refusal_replays_through_a_closed_channel() {
    let daemon = TestDaemon::start("r1-c2");
    let (_sid, _cursor, incarnation) = bootstrap_society(&daemon, "c2");
    let (offer_id, credential, subject) =
        make_offer(&daemon, &incarnation, "c2", "part-agent-1", &far_future());

    // The EXACT accept the candidate authored while the channel was open.
    let accept = json!({
        "version": "0.2", "op": "membership_accept",
        "meta": meta(&incarnation, "c2-accept", Some(1)),
        "offer_ref": offer_id,
        "subject_digest": subject,
    });
    let accepted = daemon.call_candidate(&credential, &accept);
    assert_eq!(accepted["outcome"], "ok", "{accepted}");
    let acceptance_id = accepted["result"]["acceptance_id"]
        .as_str()
        .unwrap()
        .to_owned();

    // Admission closes the candidate channel.
    let admitted = daemon.call(
        "governance",
        &json!({
            "version": "0.2", "op": "participant_admit",
            "meta": meta(&incarnation, "c2-admit", Some(2)),
            "offer_ref": offer_id,
            "membership_acceptance_ref": acceptance_id,
            "admitted_by_decision_ref": offer_decision(&offer_id),
            "admission_subject_digest": subject,
        }),
    );
    assert_eq!(admitted["outcome"], "ok", "{admitted}");

    // THE EXACT ORIGINAL IDEMPOTENCY KEY — the case the old test missed.
    // A post-terminal acceptance returns the terminal problem, never the
    // retained receipt.
    let replayed = daemon.call_candidate(&credential, &accept);
    assert_eq!(
        kind_of(&replayed),
        "forbidden",
        "an exact post-admission acceptance must be terminal, not replayed: {replayed}"
    );
    assert!(
        replayed.get("result").is_none(),
        "no receipt may leak through a terminally closed channel: {replayed}"
    );

    // A post-terminal self-policy proposal is equally terminal.
    let policy = daemon.call_candidate(
        &credential,
        &json!({
            "version": "0.2", "op": "candidate_self_policy_propose",
            "meta": meta(&incarnation, "c2-policy", None),
            "onboarding_ref": offer_id,
            "proposed_policy_kind": "assent",
            "proposed_policy_body": bpa1_allow_all(),
            "proposed_policy_digest": test_digest(0xb2),
            "adoption_mode": "direct_candidate",
            "adoption_control_domain_ref": "control-domain-1",
        }),
    );
    assert_eq!(kind_of(&policy), "forbidden", "{policy}");
}

/// The one permitted replay still works: the exact refusal that closed
/// the channel, under its own exact key.
#[test]
fn the_exact_refusal_that_closed_the_channel_still_replays() {
    let daemon = TestDaemon::start("r1-c2-refusal");
    let (_sid, _cursor, incarnation) = bootstrap_society(&daemon, "c2r");
    let (offer_id, credential, subject) =
        make_offer(&daemon, &incarnation, "c2r", "part-agent-1", &far_future());
    let refuse = json!({
        "version": "0.2", "op": "membership_refuse",
        "meta": meta(&incarnation, "c2r-refuse", Some(1)),
        "offer_ref": offer_id,
        "offer_subject_digest": subject,
    });
    let refused = daemon.call_candidate(&credential, &refuse);
    assert_eq!(refused["outcome"], "ok", "{refused}");
    let replayed = daemon.call_candidate(&credential, &refuse);
    assert_eq!(
        replayed, refused,
        "the exact refusal replays byte-identically through the closed channel"
    );
    // A DIFFERENT key for the same refusal is a new command on a
    // terminally fenced channel.
    let fresh = json!({
        "version": "0.2", "op": "membership_refuse",
        "meta": meta(&incarnation, "c2r-refuse-2", Some(1)),
        "offer_ref": offer_id,
        "offer_subject_digest": subject,
    });
    assert_eq!(
        kind_of(&daemon.call_candidate(&credential, &fresh)),
        "forbidden"
    );
}

// --------------------------------------------------------------- BY-C3 ----

/// `membership_offer_revoke` is implemented: same-revision CAS, terminal
/// revocation, fence advance, channel closure and events. The reviewed
/// build answered `feature_unavailable` and left the channel live.
#[test]
fn offer_revocation_fences_the_candidate_channel() {
    let daemon = TestDaemon::start("r1-c3");
    let (_sid, cursor, incarnation) = bootstrap_society(&daemon, "c3");
    let (offer_id, credential, subject) =
        make_offer(&daemon, &incarnation, "c3", "part-agent-1", &far_future());

    let revoked = daemon.call(
        "governance",
        &json!({
            "version": "0.2", "op": "membership_offer_revoke",
            "meta": meta(&incarnation, "c3-revoke", Some(1)),
            "offer_ref": offer_id,
            "revoked_by_decision_ref": offer_decision(&offer_id),
        }),
    );
    assert_eq!(revoked["outcome"], "ok", "{revoked}");
    assert_eq!(revoked["result"]["state"], "revoked");
    assert_eq!(revoked["result"]["revision"], json!(2));
    assert_eq!(
        revoked["result"]["fence_epoch"],
        json!(2),
        "revocation advances the onboarding fence"
    );

    // The candidate channel is closed and its credential file is gone.
    let accept = accept_offer(
        &daemon,
        &incarnation,
        &credential,
        "c3",
        &offer_id,
        &subject,
        1,
    );
    assert_eq!(
        kind_of(&accept),
        "forbidden",
        "a revoked offer's channel is dead: {accept}"
    );
    assert!(
        !daemon
            .data_dir
            .join("channels")
            .join(format!("candidate-{offer_id}.token"))
            .exists(),
        "the credential file is removed with the channel"
    );

    // Events record the revocation and the closure, with no refusal
    // attributed to the candidate.
    let events = daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "events_read",
                "continuation": cursor, "page_size": 512}),
    );
    let kinds: Vec<&str> = events["result"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e["kind"].as_str())
        .collect();
    assert!(kinds.contains(&"membership.revoked"), "{kinds:?}");
    assert!(kinds.contains(&"channel.candidate_closed"), "{kinds:?}");
    assert!(
        !kinds.contains(&"membership.refused"),
        "revocation is not a refusal: {kinds:?}"
    );

    // Admission after revocation is terminal.
    let late_admit = daemon.call(
        "governance",
        &json!({
            "version": "0.2", "op": "participant_admit",
            "meta": meta(&incarnation, "c3-admit", Some(2)),
            "offer_ref": offer_id,
            "membership_acceptance_ref": "acc-none",
            "admitted_by_decision_ref": offer_decision(&offer_id),
            "admission_subject_digest": subject,
        }),
    );
    assert_eq!(kind_of(&late_admit), "stale_binding", "{late_admit}");
}

/// Accept and revoke race on the SAME offer revision: exactly one wins.
#[test]
fn accept_and_revoke_race_the_same_offer_revision() {
    let daemon = TestDaemon::start("r1-c3-race");
    let (_sid, _cursor, incarnation) = bootstrap_society(&daemon, "c3x");
    let (offer_id, credential, subject) =
        make_offer(&daemon, &incarnation, "c3x", "part-agent-1", &far_future());
    // The candidate accepts at revision 1.
    let accepted = accept_offer(
        &daemon,
        &incarnation,
        &credential,
        "c3x",
        &offer_id,
        &subject,
        1,
    );
    assert_eq!(accepted["outcome"], "ok", "{accepted}");
    // Revocation citing the PRE-acceptance revision loses the CAS.
    let stale_revoke = daemon.call(
        "governance",
        &json!({
            "version": "0.2", "op": "membership_offer_revoke",
            "meta": meta(&incarnation, "c3x-revoke", Some(1)),
            "offer_ref": offer_id,
            "revoked_by_decision_ref": offer_decision(&offer_id),
        }),
    );
    assert_eq!(kind_of(&stale_revoke), "stale_revision", "{stale_revoke}");
    // Citing the current revision, revocation wins and admission then
    // cannot: revocation and admission never both win.
    let revoked = daemon.call(
        "governance",
        &json!({
            "version": "0.2", "op": "membership_offer_revoke",
            "meta": meta(&incarnation, "c3x-revoke2", Some(2)),
            "offer_ref": offer_id,
            "revoked_by_decision_ref": offer_decision(&offer_id),
        }),
    );
    assert_eq!(revoked["outcome"], "ok", "{revoked}");
    let admit = daemon.call(
        "governance",
        &json!({
            "version": "0.2", "op": "participant_admit",
            "meta": meta(&incarnation, "c3x-admit", Some(3)),
            "offer_ref": offer_id,
            "membership_acceptance_ref": accepted["result"]["acceptance_id"],
            "admitted_by_decision_ref": offer_decision(&offer_id),
            "admission_subject_digest": subject,
        }),
    );
    assert_eq!(kind_of(&admit), "stale_binding", "{admit}");
}

/// A crash inside the revocation journal transition commits none or all,
/// and the retry replays the exact receipt.
#[test]
fn a_crash_during_revocation_commits_none_or_all() {
    let mut daemon = TestDaemon::start_with_env(
        "r1-c3-crash",
        &[("BYOMD_ABORT", "before_finalize:membership_offer_revoke")],
    );
    let (_sid, _cursor, incarnation) = bootstrap_society(&daemon, "c3c");
    let (offer_id, _credential, _subject) =
        make_offer(&daemon, &incarnation, "c3c", "part-agent-1", &far_future());
    let revoke = json!({
        "version": "0.2", "op": "membership_offer_revoke",
        "meta": meta(&incarnation, "c3c-revoke", Some(1)),
        "offer_ref": offer_id,
        "revoked_by_decision_ref": offer_decision(&offer_id),
    });
    daemon.call_expect_death("governance", &revoke);
    daemon.restart(&[]);
    let retried = daemon.call("governance", &revoke);
    assert_eq!(retried["outcome"], "ok", "{retried}");
    assert_eq!(retried["result"]["state"], "revoked");
    let again = daemon.call("governance", &revoke);
    assert_eq!(again, retried, "the retry replays the exact receipt");
    // Exactly one revocation event.
    let conn = db(&daemon);
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM events WHERE kind = 'membership.revoked'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 1, "the crashed transition committed exactly once");
}

// --------------------------------------------------------------- BY-D1 ----

/// Authority-subject comparison uses the COMPLETE canonical `DigestRef`.
/// The reviewed build compared `value_hex` alone, so a request could
/// substitute a different keyed object reference while copying the 32
/// bytes.
#[test]
fn a_copied_value_under_another_reference_is_refused() {
    let daemon = TestDaemon::start("r1-d1");
    let (_sid, _cursor, incarnation) = bootstrap_society(&daemon, "d1d");
    let (offer_id, credential, subject) =
        make_offer(&daemon, &incarnation, "d1d", "part-agent-1", &far_future());

    // Each component varied INDEPENDENTLY, value bytes untouched.
    let mut wrong_key_ref = subject.clone();
    wrong_key_ref["key_ref"] = json!("society-key:soc-other/object:elsewhere");
    let mut wrong_class = subject.clone();
    wrong_class["class"] = json!("scope_erasure_safe");
    let mut wrong_algorithm = subject.clone();
    wrong_algorithm["class"] = json!("portable_public");
    wrong_algorithm["algorithm"] = json!("sha-256");
    wrong_algorithm.as_object_mut().unwrap().remove("key_ref");

    for (name, substitute) in [
        ("key_ref", wrong_key_ref),
        ("class", wrong_class),
        ("algorithm", wrong_algorithm),
    ] {
        let reply = daemon.call_candidate(
            &credential,
            &json!({
                "version": "0.2", "op": "membership_accept",
                "meta": meta(&incarnation, &format!("d1d-{name}"), Some(1)),
                "offer_ref": offer_id,
                "subject_digest": substitute,
            }),
        );
        assert_eq!(
            kind_of(&reply),
            "invalid",
            "a substituted {name} with the same value bytes must be refused: {reply}"
        );
    }
    // The complete, unmodified reference still accepts.
    let ok = accept_offer(
        &daemon,
        &incarnation,
        &credential,
        "d1d",
        &offer_id,
        &subject,
        1,
    );
    assert_eq!(ok["outcome"], "ok", "{ok}");
}

// --------------------------------------------------------------- BY-D2 ----

/// `local_erasure_safe` records carry a RANDOM per-object secret wrapped
/// under the Society key: destroying ONE object's secret leaves every
/// other object verifiable. The reviewed build derived every "per-object"
/// key from one store root — erasing one object could not destroy that
/// object's verification, and destroying the root destroyed all of them.
#[test]
fn destroying_one_object_secret_leaves_the_others_verifiable() {
    let daemon = TestDaemon::start("r1-d2");
    let (society_id, _cursor, _inc) = bootstrap_society(&daemon, "d2s");
    let conn = db(&daemon);

    // Every retained secret is distinct: no derivation from one root.
    let mut stmt = conn
        .prepare("SELECT key_ref, wrapped FROM object_secrets WHERE society_id = ?1")
        .unwrap();
    let secrets: Vec<(String, String)> = stmt
        .query_map([&society_id], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    assert!(secrets.len() >= 2, "several objects exist: {secrets:?}");
    let mut wrapped: Vec<&str> = secrets.iter().map(|(_, w)| w.as_str()).collect();
    wrapped.sort_unstable();
    let before = wrapped.len();
    wrapped.dedup();
    assert_eq!(
        before,
        wrapped.len(),
        "each object has its OWN random secret"
    );

    // Destroy exactly one.
    let victim = secrets[0].0.clone();
    let survivor = secrets[1].0.clone();
    drop(stmt);
    drop(conn);
    daemon.stop_and_take(|data_dir| {
        let store = byom_store::Store::open(data_dir).unwrap();
        assert!(store
            .destroy_object_secret(&victim, bpp_core::time::unix_now())
            .unwrap());
        // The victim's secret is gone; every other object keeps its own.
        assert!(store
            .verify_object_digest(&victim, "bpp-event-payload-v0", &json!({}))
            .unwrap()
            .is_none());
        assert!(store
            .verify_object_digest(&survivor, "bpp-event-payload-v0", &json!({}))
            .unwrap()
            .is_some());
        // The destruction is journaled and re-checkpointed.
        let n: i64 = store
            .conn()
            .query_row("SELECT COUNT(*) FROM erasure_journal", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    });
}

// --------------------------------------------------------------- BY-P1 ----

/// PositionRevisions are append-only with complete bindings, and the
/// current seat head is a SEPARATE CAS row. The reviewed build rewrote
/// the prior row's status through `INSERT OR REPLACE` and omitted the
/// prior position digest, the binding epoch, the incarnation, the
/// recovery epoch and the authentication observation.
#[test]
fn superseding_a_position_appends_and_never_rewrites() {
    let daemon = TestDaemon::start("r1-p1");
    let (society_id, _cursor, incarnation) = bootstrap_society(&daemon, "p1");
    let sovereign = sovereign_id(&daemon, &society_id);

    // A charter proposal gives the sovereign a governance seat to fill.
    let society = daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "society_show", "society_id": society_id}),
    );
    let rule = |name: &str| json!({"rule_ref": name, "rule_digest": test_digest(0xe1)});
    let proposed = daemon.call(
        "participant",
        &json!({
            "version": "0.2", "op": "charter_propose",
            "meta": meta(&incarnation, "p1-propose", None),
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
        }),
    );
    assert_eq!(proposed["outcome"], "ok", "{proposed}");
    let proposal_ref = proposed["result"]["charter_proposal_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let subject = proposed["result"]["subject_digest"].clone();
    let seat = proposed["result"]["required_seat_refs"][0]
        .as_str()
        .unwrap()
        .to_owned();

    let position = |key: &str, value: &str, prior: Option<&Value>| -> Value {
        let mut body = json!({
            "version": "0.2", "op": "charter_position",
            "meta": meta(&incarnation, key, None),
            "proposal_ref": proposal_ref,
            "proposal_revision": 1,
            "seat_ref": seat,
            "value": value,
            "subject_digest": subject,
        });
        if let Some(prior) = prior {
            body["prior_position_digest"] = prior.clone();
        }
        daemon.call("governance", &body)
    };

    let first = position("p1-pos1", "support", None);
    assert_eq!(first["outcome"], "ok", "{first}");
    let first_digest = first["result"]["digest"].clone();
    let first_id = first["result"]["position_id"].as_str().unwrap().to_owned();

    // A second head WITHOUT the prior digest is refused.
    let unconsumed = position("p1-pos-nohead", "assent", None);
    assert_eq!(kind_of(&unconsumed), "stale_revision", "{unconsumed}");

    // A prior digest with the right VALUE but a substituted key_ref is
    // refused too (BY-D1 in the seat-head CAS).
    let mut forged = first_digest.clone();
    forged["key_ref"] = json!("society-key:soc-other/object:elsewhere");
    let substituted = position("p1-pos-forged", "assent", Some(&forged));
    assert_eq!(kind_of(&substituted), "stale_revision", "{substituted}");

    // Superseding with the exact prior digest APPENDS a new revision.
    let second = position("p1-pos2", "assent", Some(&first_digest));
    assert_eq!(second["outcome"], "ok", "{second}");
    assert_eq!(second["result"]["revision"], json!(2));

    let conn = db(&daemon);
    // The prior record is UNTOUCHED — its status is still what its
    // author wrote.
    let (status, prior_digest, epoch, incarnation_col, epoch_col, observation): (
        String,
        Option<String>,
        i64,
        String,
        i64,
        String,
    ) = conn
        .query_row(
            "SELECT status, prior_position_digest, participant_binding_epoch,
                    endpoint_incarnation, recovery_epoch, authentication_observation
             FROM position_revisions WHERE position_id = ?1",
            [&first_id],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(
        status, "active",
        "an immutable PositionRevision is never rewritten"
    );
    assert!(prior_digest.is_none(), "the first revision cites no prior");
    assert!(epoch >= 1, "the participant binding epoch is recorded");
    assert_eq!(incarnation_col, incarnation);
    assert_eq!(epoch_col, 0);
    assert!(!observation.is_empty());
    let _ = sovereign;

    // Two revisions exist; the head is the SEPARATE CAS row.
    let revisions: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM position_revisions WHERE proposal_ref = ?1",
            [&proposal_ref],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(revisions, 2);
    let (head_ref, head_revision): (String, i64) = conn
        .query_row(
            "SELECT position_ref, revision FROM position_seat_heads
             WHERE proposal_kind = 'charter' AND proposal_ref = ?1 AND seat_ref = ?2",
            rusqlite::params![proposal_ref, seat],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(head_revision, 2);
    assert_ne!(head_ref, first_id, "the head moved to the new revision");
    // The superseding revision names the exact prior position digest.
    let cited: String = conn
        .query_row(
            "SELECT prior_position_digest FROM position_revisions WHERE position_id = ?1",
            [&head_ref],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&cited).unwrap(),
        first_digest,
        "the append names its predecessor exactly"
    );
}
