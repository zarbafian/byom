//! B3 slice 2 — the Episode lease (DESIGN.md §11.2), machine-checked in
//! `proof/specs/EpisodeLease.tla`.
//!
//! Three properties this suite pins on the real daemon:
//!
//! 1. **DUAL fences.** Every protected runtime command presents the Byom
//!    lease fence AND the Kovee invocation fence; a stale byom fence and a
//!    stale host fence each refuse on their own (family contract L21).
//! 2. **The claim CAS increments.** Each claim mints exactly one fresh
//!    fence and one immutable attempt, and the prior binding row is
//!    fenced, never rewritten (`FencePerAttempt`, `HolderIsCurrent`).
//! 3. **Clocked expiry.** Reclaim is refused before the deadline minted at
//!    claim and permitted after the AUTHORITATIVE clock passes it; a crash
//!    (SIGKILL + restart) enables nothing at all
//!    (`NoPrematureExpiry`, `ReclaimNeedsExpiryOrYield`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::runtime::{merge, Claim, Fixture, Subordinate};
use common::{kind_of, test_digest};
use serde_json::{json, Value};

/// One queued Episode ready to be claimed.
fn queued(tag: &str, ttl_key: &str) -> (Fixture, common::runtime::Episode) {
    let f = Fixture::start(tag, 8);
    let wake = f.wake("w1");
    let ep = f.request_episode(&wake, "e1");
    f.admit_placement(&ep, ttl_key, Subordinate::Confirmed(200));
    (f, ep)
}

#[test]
fn the_claim_cas_increments_the_fence_and_mints_one_attempt() {
    let (f, ep) = queued("b3-ep-claim", "p1");
    let first = f.claim(&ep.episode_id, "worker-a", 300, 7, "c1");
    assert_eq!(first.byom_fence_epoch, 1, "the first claim mints fence 1");
    assert_eq!(first.lease_revision, 1);
    assert_eq!(
        f.number(
            "SELECT attempt_count FROM episode_lease_heads WHERE episode_id = ?1",
            &ep.episode_id
        ),
        Some(1)
    );
    assert_eq!(
        f.number(
            "SELECT COUNT(*) FROM episode_attempts WHERE episode_id = ?1",
            &ep.episode_id
        ),
        Some(1),
        "one immutable EpisodeAttempt per claim (FencePerAttempt)"
    );

    // The committed ByomEpisodeBinding carries the C2 field list, both
    // fence epochs, and the Δ5 context refs.
    let binding = &first.binding;
    for member in [
        "byom_endpoint_ref",
        "endpoint_incarnation",
        "society_ref",
        "recovery_epoch",
        "participant_ref",
        "participant_binding_epoch",
        "manifestation_ref",
        "activity_stream_ref",
        "episode_ref",
        "generation",
        "byom_attempt_ref",
        "byom_fence_epoch",
        "kovee_invocation_ref",
        "kovee_invocation_fence",
        "mandate_use_refs",
        "context_source_digest",
        "byom_budget_reservation_ref",
        "byom_budget_reservation_digest",
        "external_budget_bridge_ref",
        "kovee_subordinate_reservation_ref",
        "kovee_subordinate_reservation_digest",
        "dependency_digest",
        "digest",
        "stable_binding_key",
        "allowed_local_commitments",
        "context_manifest_ref",
        "context_manifest_digest",
    ] {
        assert!(
            binding.get(member).is_some(),
            "ByomEpisodeBinding is field-verbatim: {member} is missing from {binding}"
        );
    }
    assert_eq!(binding["byom_fence_epoch"], 1);
    assert_eq!(binding["kovee_invocation_fence"], 7);
    assert_eq!(
        binding["provider_context_manifest_ref"], "kovee-pcm-1",
        "the Δ5 provider pair is carried as an all-or-none pair"
    );
    assert!(
        binding.get("kovee_context_assembly_ref").is_none(),
        "the unused optional Δ5 pair is absent entirely, never half-present"
    );

    // The exact retry under the same stable_binding_key returns the
    // IDENTICAL binding (family contract L22) — never a second attempt.
    let retry = f.claim_raw(
        &ep.episode_id,
        "worker-a",
        300,
        7,
        &format!("bindkey-{}-c1", ep.episode_id),
        "c1",
    );
    assert_eq!(retry["outcome"], "ok", "{retry}");
    assert_eq!(retry["result"]["byom_attempt_ref"], first.attempt_ref);
    assert_eq!(
        f.number(
            "SELECT COUNT(*) FROM episode_attempts WHERE episode_id = ?1",
            &ep.episode_id
        ),
        Some(1),
        "the idempotent create at the claim CAS mints no second attempt"
    );
}

#[test]
fn each_of_the_dual_fences_refuses_on_its_own() {
    let (f, ep) = queued("b3-ep-fences", "p1");
    let c = f.claim(&ep.episode_id, "worker-a", 300, 7, "c1");
    let token = f.worker_token(&ep.episode_id);

    // A STALE BYOM fence: the host fence is current, the byom one is not.
    let mut stale_byom = json!({
        "version": "0.2", "op": "episode_start",
        "meta": f.meta("srt-stale-byom", Some(c.lease_revision)),
        "episode_ref": ep.episode_id, "generation": 1,
        "byom_attempt_ref": c.attempt_ref,
        "byom_fence_epoch": c.byom_fence_epoch + 1,
        "kovee_invocation_fence": c.kovee_invocation_fence,
    });
    let refused = f.runtime(&token, &stale_byom);
    assert_eq!(kind_of(&refused), "stale_lease", "{refused}");
    assert!(
        refused["problem"]["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("stale byom_fence_epoch"),
        "{refused}"
    );

    // A STALE HOST fence: the byom fence is current, the Kovee one is not.
    merge(
        &mut stale_byom,
        json!({"byom_fence_epoch": c.byom_fence_epoch,
               "kovee_invocation_fence": c.kovee_invocation_fence + 1,
               "meta": f.meta("srt-stale-kovee", Some(c.lease_revision))}),
    );
    let refused = f.runtime(&token, &stale_byom);
    assert_eq!(kind_of(&refused), "stale_lease", "{refused}");
    assert!(
        refused["problem"]["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("stale kovee_invocation_fence"),
        "a mutation carrying only ONE of the DUAL fences is invalid: {refused}"
    );

    // Both current: the transition lands.
    let started = f.start_episode(&ep.episode_id, &c, "ok");
    assert_eq!(started["outcome"], "ok", "{started}");
    assert_eq!(started["result"]["state"], "running");
    assert_eq!(started["result"]["lease_state"], "lease_running");
}

#[test]
fn a_superseded_attempt_cannot_advance_anything() {
    // A voluntary yield makes the head re-claimable without any clock;
    // the reclaim advances the fence and the old attempt goes stale.
    let (f, ep) = queued("b3-ep-stale-worker", "p1");
    let first = f.claim(&ep.episode_id, "worker-a", 300, 7, "c1");
    let started = f.start_episode(&ep.episode_id, &first, "s1");
    assert_eq!(started["outcome"], "ok", "{started}");
    let lease_revision = started["result"]["lease_revision"].as_u64().unwrap();

    let token = f.worker_token(&ep.episode_id);
    let mut yielded = json!({
        "version": "0.2", "op": "episode_yield",
        "meta": f.meta("yld", Some(lease_revision)),
        "target_state": "waiting",
    });
    merge(
        &mut yielded,
        f.fences(
            &ep.episode_id,
            &Claim {
                lease_revision,
                ..first.clone()
            },
        ),
    );
    let reply = f.runtime(&token, &yielded);
    assert_eq!(reply["outcome"], "ok", "{reply}");
    assert_eq!(
        f.number(
            "SELECT yield_count FROM episode_lease_heads WHERE episode_id = ?1",
            &ep.episode_id
        ),
        Some(1),
        "a voluntary yield is one of the only two things that re-open the head"
    );

    let second = f.claim(&ep.episode_id, "worker-b", 300, 9, "c2");
    assert_eq!(
        second.byom_fence_epoch, 2,
        "the reclaim mints a FRESH fence"
    );
    assert_eq!(
        f.number(
            "SELECT COUNT(*) FROM episode_attempts WHERE episode_id = ?1",
            &ep.episode_id
        ),
        Some(2),
        "prior attempts remain historical; a fence is never reused"
    );
    // The first attempt's binding row is FENCED, retained for audit.
    assert_eq!(
        f.row(
            "SELECT state FROM byom_episode_bindings WHERE binding_id = ?1",
            &first.binding_ref
        )
        .as_deref(),
        Some("fenced"),
        "either fence advancing invalidates the binding for every further \
         mutation; the row is retained for orphan-result diagnostics"
    );
    // The superseded worker can advance nothing.
    let stale = f.start_episode(&ep.episode_id, &first, "stale");
    assert_eq!(kind_of(&stale), "stale_lease", "{stale}");
}

#[test]
fn reclaim_is_refused_before_the_deadline_and_permitted_after() {
    let (f, ep) = queued("b3-ep-clock", "p1");
    // A one-second lease: the deadline is minted from the AUTHORITATIVE
    // server clock as `now + lease_ttl_seconds`.
    let first = f.claim(&ep.episode_id, "worker-a", 1, 7, "c1");
    assert_eq!(first.byom_fence_epoch, 1);

    // BEFORE the deadline: a live leased head is not stealable.
    let early = f.claim_raw(&ep.episode_id, "worker-b", 1, 9, "bk-early", "c2");
    assert_eq!(kind_of(&early), "stale_lease", "{early}");
    assert!(
        early["problem"]["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("authoritative clock"),
        "{early}"
    );
    assert_eq!(
        f.row(
            "SELECT state FROM episode_lease_heads WHERE episode_id = ?1",
            &ep.episode_id
        )
        .as_deref(),
        Some("lease_leased"),
        "nothing expired: the clock has not passed the deadline"
    );
    assert_eq!(
        f.number(
            "SELECT expiry_count FROM episode_lease_heads WHERE episode_id = ?1",
            &ep.episode_id
        ),
        Some(0)
    );

    // AFTER the deadline: the server_time sweep expires the head (never
    // deleting it, never reusing the fence) and the reclaim lands.
    std::thread::sleep(std::time::Duration::from_millis(2100));
    let second = f.claim(&ep.episode_id, "worker-b", 60, 9, "c3");
    assert_eq!(second.byom_fence_epoch, 2, "reclaim under a FRESH fence");
    assert_eq!(
        f.number(
            "SELECT expiry_count FROM episode_lease_heads WHERE episode_id = ?1",
            &ep.episode_id
        ),
        Some(1),
        "the reclaim consumed exactly one authoritative-time expiry \
         (ReclaimNeedsExpiryOrYield)"
    );
    assert_eq!(
        f.number(
            "SELECT COUNT(*) FROM episode_attempts WHERE episode_id = ?1",
            &ep.episode_id
        ),
        Some(2),
        "expiry never deletes the head or reuses a fence"
    );
    // attempts <= 1 + expiries + yields
    let attempts = f
        .number(
            "SELECT attempt_count FROM episode_lease_heads WHERE episode_id = ?1",
            &ep.episode_id,
        )
        .unwrap();
    let expiries = f
        .number(
            "SELECT expiry_count FROM episode_lease_heads WHERE episode_id = ?1",
            &ep.episode_id,
        )
        .unwrap();
    let yields = f
        .number(
            "SELECT yield_count FROM episode_lease_heads WHERE episode_id = ?1",
            &ep.episode_id,
        )
        .unwrap();
    assert!(
        attempts <= 1 + expiries + yields,
        "attempts={attempts} expiries={expiries} yields={yields}"
    );
}

#[test]
fn a_crash_alone_never_enables_reclaim() {
    let f = Fixture::start("b3-ep-crash-reclaim", 8);
    let wake = f.wake("w1");
    let ep = f.request_episode(&wake, "e1");
    f.admit_placement(&ep, "p1", Subordinate::Confirmed(200));
    // A LONG lease: the worker holds it and then the whole endpoint dies.
    let first = f.claim(&ep.episode_id, "worker-a", 3600, 7, "c1");
    let started = f.start_episode(&ep.episode_id, &first, "s1");
    assert_eq!(started["outcome"], "ok", "{started}");

    let mut f = f;
    f.daemon.stop();
    f.daemon.restart(&[]);

    // The worker is gone (its process died with the daemon), yet the
    // head is untouched and NOT reclaimable: there is no liveness probe
    // anywhere — only the authoritative clock re-opens a lease.
    assert_eq!(
        f.row(
            "SELECT state FROM episode_lease_heads WHERE episode_id = ?1",
            &ep.episode_id
        )
        .as_deref(),
        Some("lease_running")
    );
    let after_crash = f.claim_raw(&ep.episode_id, "worker-b", 60, 9, "bk-crash", "c2");
    assert_eq!(kind_of(&after_crash), "stale_lease", "{after_crash}");
    assert_eq!(
        f.number(
            "SELECT expiry_count FROM episode_lease_heads WHERE episode_id = ?1",
            &ep.episode_id
        ),
        Some(0),
        "crash/stuttering mints nothing: no expiry, no attempt"
    );
    assert_eq!(
        f.number(
            "SELECT attempt_count FROM episode_lease_heads WHERE episode_id = ?1",
            &ep.episode_id
        ),
        Some(1)
    );
}

#[test]
fn checkpoints_and_terminalization_run_under_both_fences() {
    let (f, ep) = queued("b3-ep-lifecycle", "p1");
    let c = f.claim(&ep.episode_id, "worker-a", 600, 7, "c1");
    let started = f.start_episode(&ep.episode_id, &c, "s1");
    let mut lease_revision = started["result"]["lease_revision"].as_u64().unwrap();
    let token = f.worker_token(&ep.episode_id);

    let mut checkpoint = json!({
        "version": "0.2", "op": "checkpoint_commit",
        "meta": f.meta("ckp", None),
        "expected_lease_revision": lease_revision,
        "checkpoint_ref": "ckpt-1",
        "checkpoint_digest": test_digest(0xe1),
    });
    merge(&mut checkpoint, f.fences(&ep.episode_id, &c));
    let committed = f.runtime(&token, &checkpoint);
    assert_eq!(committed["outcome"], "ok", "{committed}");
    lease_revision = committed["result"]["lease_revision"].as_u64().unwrap();
    assert_eq!(
        f.number(
            "SELECT COUNT(*) FROM episode_attempt_events WHERE episode_id = ?1",
            &ep.episode_id
        ),
        Some(3),
        "claimed + started + checkpoint, all immutable"
    );

    // A worker usage report is evidence only.
    let mut report = json!({
        "version": "0.2", "op": "usage_report",
        "meta": f.meta("urep", None),
        "source": "worker_report",
        "stable_report_key": "urepkey-1",
        "quantities": [{"dimension": "unit", "unit": "unit", "amount": 140}],
    });
    merge(&mut report, f.fences(&ep.episode_id, &c));
    let reported = f.runtime(&token, &report);
    assert_eq!(reported["outcome"], "ok", "{reported}");
    assert_eq!(reported["result"]["settlement"]["settled"], false);

    let mut complete = json!({
        "version": "0.2", "op": "episode_complete",
        "meta": f.meta("cmp", Some(lease_revision)),
        "output_refs": ["out-1"], "evidence_refs": ["ev-1"],
        "usage_report_refs": [reported["result"]["report_id"].clone()],
    });
    merge(&mut complete, f.fences(&ep.episode_id, &c));
    let completed = f.runtime(&token, &complete);
    assert_eq!(completed["outcome"], "ok", "{completed}");
    assert_eq!(completed["result"]["state"], "completed");
    assert_eq!(completed["result"]["lease_state"], "lease_terminal");
    assert_eq!(
        completed["result"]["byom_episode_binding_state"],
        "released"
    );
    // Both effect heads are in the reply's downstream closure even when
    // no effect was admitted (§13.2: every consumer checks both).
    let closure: &Value = &completed["result"]["dependency_closure"];
    assert!(closure.get("effect_outcome_admission_heads").is_some());
    assert!(closure.get("effect_governance_disposition_heads").is_some());
    // Completion is evidence: no Delivery was authored.
    assert_eq!(
        f.count("SELECT COUNT(*) FROM deliveries"),
        0,
        "completion is evidence only; the Delivery stays pledgor-authored"
    );
    // The terminal Episode's runtime channel is withdrawn.
    assert!(!f
        .daemon
        .data_dir
        .join("channels")
        .join(format!("runtime-worker-{}.token", ep.episode_id))
        .exists());
}

#[test]
fn an_expired_lease_over_a_running_episode_is_ambiguous_not_repeated() {
    let (f, ep) = queued("b3-ep-ambiguous", "p1");
    let c = f.claim(&ep.episode_id, "worker-a", 1, 7, "c1");
    let started = f.start_episode(&ep.episode_id, &c, "s1");
    assert_eq!(started["outcome"], "ok", "{started}");
    std::thread::sleep(std::time::Duration::from_millis(2100));
    // The next claim attempt runs the server_time sweep first: a running
    // Episode past its lease deadline has unknown external use, so it
    // becomes ambiguous and is NEVER blindly repeated.
    let refused = f.claim_raw(&ep.episode_id, "worker-b", 60, 9, "bk-amb", "c2");
    assert_eq!(kind_of(&refused), "effect_ambiguous", "{refused}");
    assert_eq!(
        f.row(
            "SELECT state FROM episodes WHERE episode_id = ?1",
            &ep.episode_id
        )
        .as_deref(),
        Some("ambiguous")
    );
}

#[test]
fn a_stale_episode_cannot_advance_the_continuation_head() {
    // §11.3: `continuation_write` locks the exact generation and its ONE
    // ContinuationHead. A stale Episode/Manifestation may retain its bytes
    // as local diagnostic evidence but cannot advance the head.
    let (f, ep) = queued("b3-ep-continuation", "p1");
    let first = f.claim(&ep.episode_id, "worker-a", 300, 7, "c1");
    let started = f.start_episode(&ep.episode_id, &first, "s1");
    let lease_revision = started["result"]["lease_revision"].as_u64().unwrap();

    // The current attempt writes the head at revision zero.
    let written = f.participant(&json!({
        "version": "0.2", "op": "continuation_write",
        "meta": f.meta("cont-1", None),
        "activity_stream_ref": f.stream,
        "generation": 1,
        "summary_ref": "summary-1",
        "unresolved_refs": [],
        "exact_state_refs": ["state-notes-1"],
        "source_event_cursor": "cursor-ref-1",
        "expected_head_revision": 0,
        "classification_ref": "class-participant-private",
        "episode_ref": ep.episode_id,
        "byom_fence_epoch": first.byom_fence_epoch,
    }));
    assert_eq!(written["outcome"], "ok", "{written}");
    assert_eq!(written["result"]["head_revision"], 1);

    // The attempt yields and a successor claims: the first worker is now
    // stale under a superseded fence.
    let token = f.worker_token(&ep.episode_id);
    let mut yielded = json!({
        "version": "0.2", "op": "episode_yield",
        "meta": f.meta("yld-cont", Some(lease_revision)),
        "target_state": "waiting",
    });
    merge(
        &mut yielded,
        f.fences(
            &ep.episode_id,
            &Claim {
                lease_revision,
                ..first.clone()
            },
        ),
    );
    assert_eq!(f.runtime(&token, &yielded)["outcome"], "ok");
    let second = f.claim(&ep.episode_id, "worker-b", 300, 9, "c2");
    assert_eq!(second.byom_fence_epoch, first.byom_fence_epoch + 1);

    let stale = f.participant(&json!({
        "version": "0.2", "op": "continuation_write",
        "meta": f.meta("cont-stale", None),
        "activity_stream_ref": f.stream,
        "generation": 1,
        "summary_ref": "summary-orphan",
        "unresolved_refs": [],
        "exact_state_refs": [],
        "source_event_cursor": "cursor-ref-2",
        "prior_continuation_ref": written["result"]["continuation_id"],
        "prior_continuation_digest": written["result"]["digest"],
        "expected_head_revision": 1,
        "classification_ref": "class-participant-private",
        "episode_ref": ep.episode_id,
        "byom_fence_epoch": first.byom_fence_epoch,
    }));
    assert_eq!(kind_of(&stale), "stale_lease", "{stale}");
    assert_eq!(
        f.number(
            "SELECT continuation_head_revision FROM activity_streams
             WHERE activity_stream_id = ?1",
            &f.stream
        ),
        Some(1),
        "the head did not advance under a superseded fence"
    );

    // The CURRENT attempt can.
    let ok = f.participant(&json!({
        "version": "0.2", "op": "continuation_write",
        "meta": f.meta("cont-2", None),
        "activity_stream_ref": f.stream,
        "generation": 1,
        "summary_ref": "summary-2",
        "unresolved_refs": [],
        "exact_state_refs": [],
        "source_event_cursor": "cursor-ref-3",
        "prior_continuation_ref": written["result"]["continuation_id"],
        "prior_continuation_digest": written["result"]["digest"],
        "expected_head_revision": 1,
        "classification_ref": "class-participant-private",
        "episode_ref": ep.episode_id,
        "byom_fence_epoch": second.byom_fence_epoch,
    }));
    assert_eq!(ok["outcome"], "ok", "{ok}");
    assert_eq!(ok["result"]["head_revision"], 2);
}
