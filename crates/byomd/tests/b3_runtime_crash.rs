//! B3 slice 2 — a crash cell at EVERY new §15.3 commit point.
//!
//! One cell per (commit point, boundary): the daemon is started with
//! `BYOMD_ABORT=<phase>:<transition>`, driven to that transition, killed
//! mid-request, restarted, and the EXACT retry must answer `ok` and then
//! replay byte-identically. The four §15.3 boundaries are
//! `before_witness`, `after_witness`, `before_finalize`, `after_finalize`.
//!
//! The two §11.1 kernel transitions and the `server_time` sweep are not
//! callable operations, so they are targeted by their TRANSITION name
//! (`crate::dispatch::internal_hooks`) — each is its own authority
//! transaction, so a crash between two activation stages recovers the
//! committed prefix instead of re-deciding it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::runtime::{merge, Fixture, Subordinate};
use serde_json::{json, Value};

const PHASES: [&str; 4] = [
    "before_witness",
    "after_witness",
    "before_finalize",
    "after_finalize",
];

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

#[test]
fn the_kernel_activation_admit_commit_point_survives_every_boundary() {
    for phase in PHASES {
        let abort = format!("{phase}:activation_admit");
        let mut f = Fixture::start_with_env(
            &format!("b3-crash-adm-{phase}"),
            8,
            &[("BYOMD_ABORT", &abort)],
        );
        let wake = f.wake("w1");
        let request = json!({
            "version": "0.2", "op": "episode_request",
            "meta": f.meta("ereq", None),
            "activity_stream_ref": f.stream,
            "generation": 1,
            "wake_intent_ref": wake,
            "activation_admission_ref": Fixture::admission_ref(&wake),
        });
        let token = f.agent_token.clone();
        let retried = crash_and_replay(&mut f, "participant", Some(&token), &request);
        assert_eq!(retried["result"]["state"], "eligible", "{phase}");
        // Exactly ONE admission decision exists: the crash recovered the
        // committed prefix, it did not re-decide.
        assert_eq!(
            f.count("SELECT COUNT(*) FROM activation_admissions"),
            1,
            "{phase}: retry returns the same admission (§14.8)"
        );
        assert_eq!(f.count("SELECT COUNT(*) FROM resource_allocations"), 1);
        assert_eq!(f.count("SELECT COUNT(*) FROM episodes"), 1);
        assert!(f.ledger().conserves(), "{phase}: {:?}", f.ledger());
    }
}

#[test]
fn the_kernel_resource_allocate_commit_point_survives_every_boundary() {
    for phase in PHASES {
        let abort = format!("{phase}:resource_allocate");
        let mut f = Fixture::start_with_env(
            &format!("b3-crash-alloc-{phase}"),
            8,
            &[("BYOMD_ABORT", &abort)],
        );
        let wake = f.wake("w1");
        let request = json!({
            "version": "0.2", "op": "episode_request",
            "meta": f.meta("ereq", None),
            "activity_stream_ref": f.stream,
            "generation": 1,
            "wake_intent_ref": wake,
            "activation_admission_ref": Fixture::admission_ref(&wake),
        });
        let token = f.agent_token.clone();
        crash_and_replay(&mut f, "participant", Some(&token), &request);
        // Stage 2 was already committed BEFORE the crash and is reused.
        assert_eq!(f.count("SELECT COUNT(*) FROM activation_admissions"), 1);
        assert_eq!(
            f.count("SELECT COUNT(*) FROM resource_allocations"),
            1,
            "{phase}: one allocation, one reservation — never a double reserve"
        );
        assert_eq!(
            f.count(
                "SELECT COUNT(*) FROM budget_reservations
                 WHERE holder_kind = 'episode_allocation'"
            ),
            1
        );
        assert!(f.ledger().conserves(), "{phase}: {:?}", f.ledger());
    }
}

#[test]
fn the_episode_request_commit_point_survives_every_boundary() {
    for phase in PHASES {
        let abort = format!("{phase}:episode_request");
        let mut f = Fixture::start_with_env(
            &format!("b3-crash-ereq-{phase}"),
            8,
            &[("BYOMD_ABORT", &abort)],
        );
        let wake = f.wake("w1");
        let request = json!({
            "version": "0.2", "op": "episode_request",
            "meta": f.meta("ereq", None),
            "activity_stream_ref": f.stream,
            "generation": 1,
            "wake_intent_ref": wake,
            "activation_admission_ref": Fixture::admission_ref(&wake),
        });
        let token = f.agent_token.clone();
        crash_and_replay(&mut f, "participant", Some(&token), &request);
        assert_eq!(f.count("SELECT COUNT(*) FROM episodes"), 1, "{phase}");
        assert!(f.ledger().conserves(), "{phase}: {:?}", f.ledger());
    }
}

#[test]
fn the_placement_admit_commit_point_survives_every_boundary() {
    for phase in PHASES {
        let abort = format!("{phase}:placement_admit");
        let mut f = Fixture::start_with_env(
            &format!("b3-crash-plc-{phase}"),
            8,
            &[("BYOMD_ABORT", &abort)],
        );
        let wake = f.wake("w1");
        let ep = f.request_episode(&wake, "e1");
        let token = f.placement_token(&ep.allocation_ref);
        let request = json!({
            "version": "0.2", "op": "placement_admit",
            "meta": f.meta("plc", None),
            "resource_allocation_ref": ep.allocation_ref,
            "resource_allocation_digest": f.allocation_digest(&ep.allocation_ref),
            "kovee_placement_ref": "kovee-placement-1",
            "kovee_placement_revision": 1,
            "kovee_placement_digest": common::runtime::portable_digest(0x5d),
            "source_binding_epoch": 1,
            "selected_manifestation_ref": "manif-selected-1",
            "kovee_invocation_ref": "kovee-inv-1",
            "kovee_fence_epoch": 7,
            "subordinate_reservation": {
                "stable_external_reservation_key": ep.stable_external_key,
                "outcome": "confirmed",
                "subordinate_reservation_ref": "kovee-sub-1",
                "revision": 1,
                "digest": common::runtime::portable_digest(0x5c),
                "items": [{
                    "kovee_account_ref": "kovee-acct-1",
                    "dimension": "unit", "unit": "unit", "amount": 200,
                    "parent_account_ref": common::runtime::PARENT_ACCOUNT,
                    "parent_account_revision": 1,
                    "parent_dimension": "unit", "parent_unit": "unit",
                    "parent_worst_case_amount": common::runtime::WORST_CASE,
                }],
            },
        });
        crash_and_replay(&mut f, "runtime", Some(&token), &request);
        assert_eq!(
            f.count("SELECT COUNT(*) FROM subordinate_reservations"),
            1,
            "{phase}: CreateOnce holds across the crash"
        );
        assert_eq!(
            f.row(
                "SELECT state FROM episodes WHERE episode_id = ?1",
                &ep.episode_id
            )
            .as_deref(),
            Some("queued"),
            "{phase}"
        );
        assert!(f.ledger().conserves(), "{phase}: {:?}", f.ledger());
    }
}

#[test]
fn the_episode_claim_commit_point_survives_every_boundary() {
    for phase in PHASES {
        let abort = format!("{phase}:episode_claim");
        let mut f = Fixture::start_with_env(
            &format!("b3-crash-clm-{phase}"),
            8,
            &[("BYOMD_ABORT", &abort)],
        );
        let wake = f.wake("w1");
        let ep = f.request_episode(&wake, "e1");
        f.admit_placement(&ep, "p1", Subordinate::Confirmed(200));
        let token = f.worker_token(&ep.episode_id);
        let request = json!({
            "version": "0.2", "op": "episode_claim",
            "meta": f.meta("clm", None),
            "episode_ref": ep.episode_id,
            "generation": 1,
            "holder_runtime_binding": "worker-a",
            "claim_subject_digest": common::test_digest(0xd1),
            "lease_ttl_seconds": 600,
            "kovee_invocation_ref": "kovee-inv-1",
            "kovee_invocation_fence": 7,
            "stable_binding_key": "bindkey-crash-1",
            "context_manifest_ref": "ctxman-1",
            "context_manifest_digest": common::test_digest(0xd2),
            "context_source_digest": common::runtime::portable_digest(0xd3),
            "mandate_use_refs": ["muse-1"],
            "allowed_local_commitments": ["kovee_local_note"],
        });
        crash_and_replay(&mut f, "runtime", Some(&token), &request);
        assert_eq!(
            f.number(
                "SELECT attempt_count FROM episode_lease_heads WHERE episode_id = ?1",
                &ep.episode_id
            ),
            Some(1),
            "{phase}: one fresh fence and one immutable attempt, never two"
        );
        assert_eq!(
            f.count("SELECT COUNT(*) FROM byom_episode_bindings"),
            1,
            "{phase}: the idempotent create at the claim CAS (L22)"
        );
    }
}

/// The protected per-attempt commit points, each driven to its crash
/// boundary from a running Episode.
#[test]
fn the_protected_per_attempt_commit_points_survive_every_boundary() {
    for op in [
        "episode_start",
        "checkpoint_commit",
        "episode_yield",
        "episode_complete",
        "episode_fail",
        "usage_report",
        "effect_outcome_admit",
    ] {
        for phase in PHASES {
            let abort = format!("{phase}:{op}");
            let mut f = Fixture::start_with_env(
                &format!("b3-crash-{op}-{phase}"),
                8,
                &[("BYOMD_ABORT", &abort)],
            );
            let wake = f.wake("w1");
            let ep = f.request_episode(&wake, "e1");
            f.admit_placement(&ep, "p1", Subordinate::Confirmed(200));
            let c = f.claim(&ep.episode_id, "worker-a", 600, 7, "c1");
            let mut lease_revision = c.lease_revision;
            // Everything but episode_start needs the Episode running
            // first, and episode_start is the crash target itself.
            if op != "episode_start" {
                let started = f.start_episode(&ep.episode_id, &c, "s1");
                assert_eq!(started["outcome"], "ok", "{op}/{phase}: {started}");
                lease_revision = started["result"]["lease_revision"].as_u64().unwrap();
            }
            let mut request = match op {
                "episode_start" => json!({
                    "version": "0.2", "op": "episode_start",
                    "meta": f.meta("srt", Some(lease_revision)),
                }),
                "checkpoint_commit" => json!({
                    "version": "0.2", "op": "checkpoint_commit",
                    "meta": f.meta("ckp", None),
                    "expected_lease_revision": lease_revision,
                    "checkpoint_ref": "ckpt-1",
                    "checkpoint_digest": common::test_digest(0xe1),
                }),
                "episode_yield" => json!({
                    "version": "0.2", "op": "episode_yield",
                    "meta": f.meta("yld", Some(lease_revision)),
                    "target_state": "waiting",
                }),
                "episode_complete" => json!({
                    "version": "0.2", "op": "episode_complete",
                    "meta": f.meta("cmp", Some(lease_revision)),
                    "output_refs": ["out-1"], "evidence_refs": ["ev-1"],
                    "usage_report_refs": [],
                }),
                "episode_fail" => json!({
                    "version": "0.2", "op": "episode_fail",
                    "meta": f.meta("fal", Some(lease_revision)),
                    "failure_reason_ref": "reason-tool-error",
                    "evidence_refs": ["ev-1"],
                }),
                "usage_report" => json!({
                    "version": "0.2", "op": "usage_report",
                    "meta": f.meta("urep", None),
                    "source": "worker_report",
                    "stable_report_key": "urepkey-1",
                    "quantities": [{"dimension": "unit", "unit": "unit",
                                    "amount": 140}],
                }),
                _ => json!({
                    "version": "0.2", "op": "effect_outcome_admit",
                    "meta": f.meta("eoa", None),
                    "intent_ref": "intent-1",
                    "intent_digest": common::test_digest(0xf1),
                    "stable_execution_key": "execkey-1",
                    "host_protocol": "kovee",
                    "host_endpoint_ref": "kovee-endpoint-1",
                    "host_effect_ref": "kovee-effect-1",
                    "host_effect_digest": common::runtime::portable_digest(0xf2),
                    "host_receipt_ref": "kovee-receipt-1",
                    "host_receipt_digest": common::runtime::portable_digest(0xf3),
                    "host_cursor_or_signature_ref": "kovee-sig-1",
                    "verification_status": "verified",
                    "outcome": "ambiguous",
                }),
            };
            merge(&mut request, f.fences(&ep.episode_id, &c));
            let token = f.worker_token(&ep.episode_id);
            crash_and_replay(&mut f, "runtime", Some(&token), &request);
            assert!(f.ledger().conserves(), "{op}/{phase}: {:?}", f.ledger());
        }
    }
}

#[test]
fn the_trusted_meter_settlement_commit_point_survives_every_boundary() {
    for phase in PHASES {
        let abort = format!("{phase}:usage_report");
        let mut f = Fixture::start_with_env(
            &format!("b3-crash-meter-{phase}"),
            8,
            &[("BYOMD_ABORT", &abort)],
        );
        let wake = f.wake("w1");
        let ep = f.request_episode(&wake, "e1");
        f.admit_placement(&ep, "p1", Subordinate::Confirmed(200));
        let c = f.claim(&ep.episode_id, "worker-a", 600, 7, "c1");
        f.start_episode(&ep.episode_id, &c, "s1");
        let mut request = json!({
            "version": "0.2", "op": "usage_report",
            "meta": f.meta("meter", None),
            "source": "trusted_meter",
            "stable_report_key": "urepkey-meter",
            "quantities": [{"dimension": "unit", "unit": "unit", "amount": 140}],
            "meter_ref": "meter-kovee-1",
            "meter_attestation_ref": "attest-1",
            "stable_settlement_key": "setkey-1",
            "charged_quantities": [{"dimension": "unit", "unit": "unit",
                                    "amount": 140}],
        });
        merge(&mut request, f.fences(&ep.episode_id, &c));
        let token = f.meter_token(&ep.episode_id);
        crash_and_replay(&mut f, "runtime", Some(&token), &request);
        assert_eq!(
            f.count("SELECT COUNT(*) FROM usage_settlements"),
            1,
            "{phase}: SettleOnce holds across the crash"
        );
        let ledger = f.ledger();
        assert!(ledger.conserves(), "{phase}: {ledger:?}");
        assert_eq!(ledger.committed, 140, "{phase}: charged exactly once");
    }
}

#[test]
fn the_server_time_sweep_commit_point_survives_every_boundary() {
    for phase in PHASES {
        let abort = format!("{phase}:server_time");
        let mut f = Fixture::start_with_env(
            &format!("b3-crash-sweep-{phase}"),
            8,
            &[("BYOMD_ABORT", &abort)],
        );
        let wake = f.wake("w1");
        let ep = f.request_episode(&wake, "e1");
        f.admit_placement(&ep, "p1", Subordinate::Confirmed(200));
        f.claim(&ep.episode_id, "worker-a", 1, 7, "c1");
        std::thread::sleep(std::time::Duration::from_millis(2100));
        // The reclaim runs the authoritative-clock sweep first; the crash
        // lands on the sweep's own commit point.
        let request = json!({
            "version": "0.2", "op": "episode_claim",
            "meta": f.meta("clm-2", None),
            "episode_ref": ep.episode_id,
            "generation": 1,
            "holder_runtime_binding": "worker-b",
            "claim_subject_digest": common::test_digest(0xd1),
            "lease_ttl_seconds": 600,
            "kovee_invocation_ref": "kovee-inv-2",
            "kovee_invocation_fence": 9,
            "stable_binding_key": "bindkey-crash-2",
            "context_manifest_ref": "ctxman-1",
            "context_manifest_digest": common::test_digest(0xd2),
            "context_source_digest": common::runtime::portable_digest(0xd3),
            "mandate_use_refs": ["muse-1"],
            "allowed_local_commitments": [],
        });
        let token = f.worker_token(&ep.episode_id);
        crash_and_replay(&mut f, "runtime", Some(&token), &request);
        // Exactly one expiry was consumed, so `attempts <= 1 + expiries
        // + yields` still holds: the crash minted nothing.
        assert_eq!(
            f.number(
                "SELECT expiry_count FROM episode_lease_heads WHERE episode_id = ?1",
                &ep.episode_id
            ),
            Some(1),
            "{phase}"
        );
        assert_eq!(
            f.number(
                "SELECT attempt_count FROM episode_lease_heads WHERE episode_id = ?1",
                &ep.episode_id
            ),
            Some(2),
            "{phase}"
        );
    }
}

#[test]
fn the_two_reconciliation_seats_survive_every_boundary() {
    // budget_reconcile: the R38 release out of an uncertain bridge.
    for phase in PHASES {
        let abort = format!("{phase}:budget_reconcile");
        let mut f = Fixture::start_with_env(
            &format!("b3-crash-brc-{phase}"),
            8,
            &[("BYOMD_ABORT", &abort)],
        );
        let base = f.ledger();
        let wake = f.wake("w1");
        let ep = f.request_episode(&wake, "e1");
        f.admit_placement(&ep, "p1", Subordinate::Uncertain);
        let request = json!({
            "version": "0.2", "op": "budget_reconcile",
            "meta": f.meta("brc", None),
            "external_budget_bridge_ref": ep.bridge_ref,
            "stable_external_reservation_key": ep.stable_external_key,
            "fresh_challenge_ref": "challenge-1",
            "reason_ref": "reason-transport-loss",
        });
        crash_and_replay(&mut f, "governance", None, &request);
        let ledger = f.ledger();
        assert!(ledger.conserves(), "{phase}: {ledger:?}");
        assert_eq!(ledger.uncertain, 0, "{phase}");
        assert_eq!(ledger.remaining, base.remaining, "{phase}");
        assert_eq!(
            f.count(
                "SELECT COUNT(*) FROM governance_decisions
                 WHERE kind = 'budget_reconciliation'"
            ),
            1,
            "{phase}: one immutable decision, never two"
        );
    }
    // effect_reconcile: the R38 local-consequence disposition.
    for phase in PHASES {
        let abort = format!("{phase}:effect_reconcile");
        let mut f = Fixture::start_with_env(
            &format!("b3-crash-erc-{phase}"),
            8,
            &[("BYOMD_ABORT", &abort)],
        );
        let wake = f.wake("w1");
        let ep = f.request_episode(&wake, "e1");
        f.admit_placement(&ep, "p1", Subordinate::Confirmed(200));
        let c = f.claim(&ep.episode_id, "worker-a", 600, 7, "c1");
        f.start_episode(&ep.episode_id, &c, "s1");
        let worker = f.worker_token(&ep.episode_id);
        let mut admit = json!({
            "version": "0.2", "op": "effect_outcome_admit",
            "meta": f.meta("eoa", None),
            "intent_ref": "intent-1",
            "intent_digest": common::test_digest(0xf1),
            "stable_execution_key": "execkey-1",
            "host_protocol": "kovee",
            "host_endpoint_ref": "kovee-endpoint-1",
            "host_effect_ref": "kovee-effect-1",
            "host_effect_digest": common::runtime::portable_digest(0xf2),
            "host_receipt_ref": "kovee-receipt-1",
            "host_receipt_digest": common::runtime::portable_digest(0xf3),
            "host_cursor_or_signature_ref": "kovee-sig-1",
            "verification_status": "verified",
            "outcome": "ambiguous",
        });
        merge(&mut admit, f.fences(&ep.episode_id, &c));
        let admitted = f.runtime(&worker, &admit);
        assert_eq!(admitted["outcome"], "ok", "{phase}: {admitted}");
        let basis = admitted["result"].clone();
        let request = json!({
            "version": "0.2", "op": "effect_reconcile",
            "meta": f.meta("erc", None),
            "intent_ref": "intent-1",
            "intent_digest": common::test_digest(0xf1),
            "stable_execution_key": "execkey-1",
            "phase": "ambiguous_source",
            "basis_source_admission_ref": basis["admission_id"],
            "basis_source_admission_revision": basis["revision"],
            "basis_source_admission_digest": basis["digest"],
            "local_outcome": "failed",
            "result_use": "unavailable",
            "fresh_challenge_ref": "challenge-1",
            "late_source_policy": "quarantine_and_redecide",
        });
        crash_and_replay(&mut f, "governance", None, &request);
        assert_eq!(
            f.count("SELECT COUNT(*) FROM effect_governance_dispositions"),
            1,
            "{phase}: one disposition revision, never two"
        );
        assert_eq!(
            f.count("SELECT COUNT(*) FROM effect_outcome_admissions"),
            1,
            "{phase}: the source axis never moved"
        );
    }
}
