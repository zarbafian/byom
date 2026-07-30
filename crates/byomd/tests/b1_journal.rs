//! Fault injection at every §15.3 authority-journal boundary, against
//! the TLC-checked model (proof/specs/AuthorityJournal.tla): kill before
//! and after the witness CAS, lost witness reply/request, kill inside
//! finalize, database rollback → `sealed_diagnostic` closes every
//! non-diagnostic surface; no early result, event, or credential;
//! continuous generations; exact recovery or abandonment.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::*;
use serde_json::json;

fn offer_request(daemon: &TestDaemon, incarnation: &str, key: &str) -> serde_json::Value {
    json!({
        "version": "0.2", "op": "membership_offer",
        "meta": meta(incarnation, key, None),
        "participant_ref": "part-agent-j",
        "proposed_standing_ref": "standing-proposal-1",
        "subject_digest": test_digest(0xc1),
        "offered_by_decision_ref": society_decision(daemon),
        "expires_at": far_future(),
    })
}

fn offer_count(daemon: &TestDaemon, cursor: &str) -> usize {
    let events = daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "events_read",
                "continuation": cursor, "page_size": 512}),
    );
    events["result"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == "membership.offered")
        .count()
}

fn generations_continuous(daemon: &TestDaemon) {
    let entries = daemon.witness_entries();
    for (i, entry) in entries.iter().enumerate() {
        assert_eq!(
            entry["generation"].as_u64().unwrap(),
            i as u64 + 1,
            "witness generations must be dense and continuous"
        );
    }
}

#[test]
fn kill_before_witness_cas_leaves_no_authority_and_abandons_after_proof() {
    let mut daemon = TestDaemon::start_with_env(
        "jrnl-beforecas",
        &[("BYOMD_ABORT", "before_witness:membership_offer")],
    );
    let (_sid, cursor, incarnation) = bootstrap_society(&daemon, "jb");
    let head_before = daemon.witness_entries().len();

    // The daemon dies after SQL prepare, before the witness CAS.
    daemon.call_expect_death(
        "governance",
        &offer_request(&daemon, &incarnation, "jb-offer"),
    );
    // The witness never saw the transaction.
    assert_eq!(daemon.witness_entries().len(), head_before);

    // Restart without faults: startup proves no entry and abandons the
    // inert pending state — no visible offer, no event, no channel.
    daemon.restart(&[]);
    assert_eq!(offer_count(&daemon, &cursor), 0, "no early event");
    let channels = std::fs::read_dir(daemon.data_dir.join("channels"))
        .map(|d| d.count())
        .unwrap_or(0);
    assert_eq!(channels, 0, "no early credential");

    // The same request retries cleanly (nothing was journaled, nothing
    // replays: fresh execution).
    let retry = daemon.call(
        "governance",
        &offer_request(&daemon, &incarnation, "jb-offer"),
    );
    assert_eq!(retry["outcome"], "ok", "{retry}");
    assert_eq!(offer_count(&daemon, &cursor), 1);
    generations_continuous(&daemon);
}

#[test]
fn kill_after_witness_cas_recovers_by_the_exact_receipt_once() {
    let mut daemon = TestDaemon::start_with_env(
        "jrnl-aftercas",
        &[("BYOMD_ABORT", "after_witness:membership_offer")],
    );
    let (_sid, cursor, incarnation) = bootstrap_society(&daemon, "ja");
    let head_before = daemon.witness_entries().len();

    // The daemon dies after the witness CAS, before SQL finalize: the
    // entry exists but nothing is visible yet.
    daemon.call_expect_death(
        "governance",
        &offer_request(&daemon, &incarnation, "ja-offer"),
    );
    assert_eq!(daemon.witness_entries().len(), head_before + 1);

    // Restart: recovery finalizes the exact witnessed transaction ONCE.
    daemon.restart(&[]);
    assert_eq!(offer_count(&daemon, &cursor), 1, "recovered exactly once");
    // The retained result answers the byte-identical retry.
    let retry = daemon.call(
        "governance",
        &offer_request(&daemon, &incarnation, "ja-offer"),
    );
    assert_eq!(retry["outcome"], "ok", "{retry}");
    assert_eq!(
        offer_count(&daemon, &cursor),
        1,
        "the retry replays, never re-executes"
    );
    // And the recovered credential is usable.
    let offer_id = retry["result"]["offer_id"].as_str().unwrap();
    let token = read_candidate_token(&daemon, offer_id);
    assert!(!token.is_empty());
    generations_continuous(&daemon);
}

#[test]
fn kill_inside_finalize_before_commit_recovers_identically() {
    let mut daemon = TestDaemon::start_with_env(
        "jrnl-beforefin",
        &[("BYOMD_ABORT", "before_finalize:membership_offer")],
    );
    let (_sid, cursor, incarnation) = bootstrap_society(&daemon, "jf");
    daemon.call_expect_death(
        "governance",
        &offer_request(&daemon, &incarnation, "jf-offer"),
    );
    // THE LOST COMMIT, in the shape startup is allowed to skip (BY-J3):
    // the checkpoint of the transaction whose commit was lost is
    // STRICTLY ahead of the rolled-back database in the audit ledger
    // (the transaction appended its audit record before checkpointing),
    // level in the erasure ledger, and names the next generation. This
    // is the only window `verify_chains_against_checkpoints` admits, so
    // the recovery below is a genuine skip and not an exact match.
    {
        let conn = rusqlite::Connection::open(daemon.data_dir.join("byom.db")).unwrap();
        let witness =
            byom_store::witness::Witness::open(&daemon.data_dir.join("authority-witness.jsonl"))
                .unwrap();
        let checkpoints = byom_store::checkpoint::Checkpoints::open(
            &daemon.data_dir.join("authority-checkpoints.jsonl"),
        )
        .unwrap();
        let (audit_seq, _) = byom_store::audit::head_of(&conn, byom_store::audit::AUDIT).unwrap();
        let (erasure_seq, _) =
            byom_store::audit::head_of(&conn, byom_store::audit::ERASURE).unwrap();
        let mirror: u64 = byom_store::schema::meta_get_text(&conn, "journal_mirror_gen")
            .unwrap()
            .unwrap()
            .parse()
            .unwrap();
        let last = checkpoints.latest(&witness).unwrap().unwrap();
        assert!(
            last.audit.seq > audit_seq,
            "a lost commit leaves its checkpoint strictly ahead in the audit ledger: \
             checkpoint {} vs database {audit_seq}",
            last.audit.seq
        );
        assert_eq!(last.erasure.seq, erasure_seq);
        assert_eq!(last.journal_generation, mirror + 1);
    }
    daemon.restart(&[]);
    assert_eq!(
        offer_count(&daemon, &cursor),
        1,
        "finalized exactly once at recovery"
    );
    let retry = daemon.call(
        "governance",
        &offer_request(&daemon, &incarnation, "jf-offer"),
    );
    assert_eq!(retry["outcome"], "ok");
    assert_eq!(offer_count(&daemon, &cursor), 1);
    generations_continuous(&daemon);
}

#[test]
fn kill_after_finalize_before_reply_replays_the_retained_result() {
    let mut daemon = TestDaemon::start_with_env(
        "jrnl-afterfin",
        &[("BYOMD_ABORT", "after_finalize:membership_offer")],
    );
    let (_sid, cursor, incarnation) = bootstrap_society(&daemon, "jr");
    daemon.call_expect_death(
        "governance",
        &offer_request(&daemon, &incarnation, "jr-offer"),
    );
    daemon.restart(&[]);
    // Committed before the crash: visible exactly once, and the retry
    // returns the retained result.
    assert_eq!(offer_count(&daemon, &cursor), 1);
    let retry = daemon.call(
        "governance",
        &offer_request(&daemon, &incarnation, "jr-offer"),
    );
    assert_eq!(retry["outcome"], "ok");
    assert_eq!(offer_count(&daemon, &cursor), 1);
    generations_continuous(&daemon);
}

#[test]
fn lost_witness_reply_is_queried_never_guessed() {
    let daemon = TestDaemon::start_with_env(
        "jrnl-lostreply",
        &[("BYOMD_ABORT", "witness_lose_reply:membership_offer")],
    );
    let (_sid, cursor, incarnation) = bootstrap_society(&daemon, "jl");
    // The receipt is lost in flight; the daemon queries by transaction
    // id, finds the exact entry, and finalizes once — the caller still
    // gets the result.
    let reply = daemon.call(
        "governance",
        &offer_request(&daemon, &incarnation, "jl-offer"),
    );
    assert_eq!(reply["outcome"], "ok", "{reply}");
    assert_eq!(offer_count(&daemon, &cursor), 1);
    generations_continuous(&daemon);
}

#[test]
fn lost_witness_request_abandons_after_proof_and_stays_retryable() {
    let mut daemon = TestDaemon::start_with_env(
        "jrnl-lostreq",
        &[("BYOMD_ABORT", "witness_lose_request:membership_offer")],
    );
    let (_sid, cursor, incarnation) = bootstrap_society(&daemon, "jq");
    let head_before = daemon.witness_entries().len();
    let reply = daemon.call(
        "governance",
        &offer_request(&daemon, &incarnation, "jq-offer"),
    );
    // Proven not committed: honestly unavailable, never a fake success.
    assert_eq!(kind_of(&reply), "unavailable", "{reply}");
    assert_eq!(daemon.witness_entries().len(), head_before);
    assert_eq!(offer_count(&daemon, &cursor), 0);

    // Without the fault the same request commits.
    daemon.restart(&[]);
    let retry = daemon.call(
        "governance",
        &offer_request(&daemon, &incarnation, "jq-offer"),
    );
    assert_eq!(retry["outcome"], "ok", "{retry}");
    assert_eq!(offer_count(&daemon, &cursor), 1);
    generations_continuous(&daemon);
}

#[test]
fn database_rollback_seals_every_non_diagnostic_surface() {
    let mut daemon = TestDaemon::start("jrnl-rollback");
    let (society_id, _cursor, incarnation) = bootstrap_society(&daemon, "js");

    // The rollback adversary takes its backup... (inside the data dir,
    // so the daemon's fixture cleanup removes it too; snapshot_db only
    // copies byom.db* files, so the subdir never enters a snapshot)
    daemon.stop();
    let snapshot = daemon.data_dir.join("rollback-snapshot");
    daemon.snapshot_db(&snapshot);

    // ...authority advances (a witnessed offer)...
    daemon.restart(&[]);
    let reply = daemon.call(
        "governance",
        &offer_request(&daemon, &incarnation, "js-offer"),
    );
    assert_eq!(reply["outcome"], "ok");
    let head_after = daemon.witness_entries().len();

    // ...and the database is restored in place. The witness does NOT
    // roll back.
    daemon.stop();
    daemon.restore_db(&snapshot);
    daemon.restart(&[]);
    assert_eq!(
        daemon.witness_entries().len(),
        head_after,
        "witness untouched"
    );

    // Startup comparison: a witnessed transaction the database no longer
    // knows cannot be skipped or re-created — sealed_diagnostic, every
    // non-diagnostic surface refuses.
    for (surface, request) in [
        (
            "projection",
            json!({"version": "0.2", "op": "society_show", "society_id": society_id}),
        ),
        (
            "governance",
            offer_request(&daemon, &incarnation, "js-after-seal"),
        ),
        (
            "projection",
            json!({"version": "0.2", "op": "events_read",
                   "continuation": "bc1.00.00", "page_size": 1}),
        ),
        (
            "participant",
            json!({"version": "0.2", "op": "participation_cease",
                   "meta": meta(&incarnation, "js-cease", Some(1))}),
        ),
    ] {
        let reply = daemon.call(surface, &request);
        assert_eq!(kind_of(&reply), "endpoint_sealed", "{surface}: {reply}");
    }
    let via_candidate = daemon.call_candidate(
        "any-token",
        &json!({"version": "0.2", "op": "membership_accept",
                "meta": meta(&incarnation, "js-acc", Some(1)),
                "offer_ref": "offer-x", "subject_digest": test_digest(1)}),
    );
    assert_eq!(kind_of(&via_candidate), "endpoint_sealed");

    // The diagnostic remainder still answers: liveness and negotiation.
    let hello = daemon.call("governance", &json!({"version": "0.2", "op": "hello"}));
    assert_eq!(hello["outcome"], "ok");
    // Sealing survives restart: recovery diagnostics only until a
    // reconciled new incarnation (out of slice-1 scope).
    daemon.restart(&[]);
    let still_sealed = daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "society_show", "society_id": "soc-any"}),
    );
    assert_eq!(kind_of(&still_sealed), "endpoint_sealed");
}

#[test]
fn genesis_is_atomic_under_crash() {
    // Crash the atomic genesis BEFORE the witness CAS: none of the
    // genesis set may exist afterwards (§6.1 crash result: none or
    // complete genesis).
    let mut daemon = TestDaemon::start_with_env(
        "jrnl-genesis",
        &[("BYOMD_ABORT", "before_witness:society_bootstrap")],
    );
    let incarnation = daemon.incarnation();
    let prepare = json!({
        "version": "0.2", "op": "society_prepare",
        "meta": meta(&incarnation, "jg-prep", None),
        "home_authority_ref": "auth-home-1",
        "proposed_charter_ref": "charter-draft-1",
        "proposed_charter_digest": test_digest(0xa1),
        "classification_binding_ref": "class-bind-1",
        "classification_binding_digest": test_digest(0xa2),
    });
    let prepared = daemon.call("governance", &prepare);
    assert_eq!(prepared["outcome"], "ok");
    let society_id = prepared["result"]["society_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let bootstrap = json!({
        "version": "0.2", "op": "society_bootstrap",
        "meta": meta(&incarnation, "jg-boot", Some(1)),
        "society_id": society_id,
        "preparation_ref": prepared["result"]["preparation_ref"],
        "subject_digest": prepared["result"]["subject_digest"],
    });
    daemon.call_expect_death("governance", &bootstrap);

    daemon.restart(&[]);
    let shown = daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "society_show", "society_id": society_id}),
    );
    assert_eq!(
        shown["result"]["state"], "forming",
        "no partial genesis: {shown}"
    );

    // The exact retry completes the genesis.
    let booted = daemon.call("governance", &bootstrap);
    assert_eq!(booted["outcome"], "ok", "{booted}");
    assert_eq!(booted["result"]["state"], "active");
    generations_continuous(&daemon);
}
