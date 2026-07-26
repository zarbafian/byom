//! B3 slice 2 — the FOUR-STAGE activation (DESIGN.md §11.1).
//!
//! Activation has four distinct records and four owners, and this suite
//! proves that structurally rather than by prose:
//!
//! ```text
//! 1 WakeIntent          participant channel only
//! 2 ActivationAdmission kernel (activation_admit)   may deny, cannot invent
//! 3 ResourceAllocation  kernel (resource_allocate)  only an ADMITTED intent
//! 4 PlacementAdmission  narrow Kovee adapter        only a RESERVED allocation
//! ```
//!
//! "Arrival, Kovee Attention, ranking, a host cron, or a model score
//! cannot skip a stage" is a registry and guard fact: no surface but the
//! participant one authors a WakeIntent, the stage ids are kernel-derived
//! so a caller cannot name a stage it skipped, and every later stage
//! refuses an absent or wrong-state predecessor.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::runtime::{Fixture, Subordinate};
use common::{far_future, kind_of, test_digest};
use serde_json::json;

#[test]
fn all_four_activation_stages_commit_in_order() {
    let f = Fixture::start("b3-act-order", 8);
    let wake = f.wake("w1");

    // Stage 1 is committed and pending; nothing else exists yet.
    assert_eq!(
        f.row(
            "SELECT state FROM wake_intents WHERE wake_intent_id = ?1",
            &wake
        )
        .as_deref(),
        Some("submitted")
    );
    assert!(
        f.row(
            "SELECT state FROM activation_admissions WHERE admission_id = ?1",
            &Fixture::admission_ref(&wake)
        )
        .is_none(),
        "no admission exists before the kernel evaluates the committed WakeIntent"
    );

    let ep = f.request_episode(&wake, "e1");

    // Stage 2: one admission per WakeIntent revision, admitted, citing
    // the exact intent it evaluated.
    assert_eq!(
        f.row(
            "SELECT state FROM activation_admissions WHERE admission_id = ?1",
            &ep.admission_ref
        )
        .as_deref(),
        Some("admitted")
    );
    assert_eq!(
        f.row(
            "SELECT wake_intent_ref FROM activation_admissions WHERE admission_id = ?1",
            &ep.admission_ref
        )
        .as_deref(),
        Some(wake.as_str()),
        "the admission cites the exact committed WakeIntent (stage 1 -> 2)"
    );

    // Stage 3: reserved, citing the admission, with the bridge persisted
    // under its stable key BEFORE queueing.
    assert_eq!(
        f.row(
            "SELECT state FROM resource_allocations WHERE allocation_id = ?1",
            &ep.allocation_ref
        )
        .as_deref(),
        Some("reserved")
    );
    assert_eq!(
        f.row(
            "SELECT activation_admission_ref FROM resource_allocations WHERE allocation_id = ?1",
            &ep.allocation_ref
        )
        .as_deref(),
        Some(ep.admission_ref.as_str()),
        "the allocation cites the exact admission (stage 2 -> 3)"
    );
    assert_eq!(
        f.row(
            "SELECT state FROM external_budget_bridges WHERE bridge_id = ?1",
            &ep.bridge_ref
        )
        .as_deref(),
        Some("requested"),
        "the §11.4 bridge is persisted under its stable key before queueing"
    );
    // The Episode exists but does NOT queue: queueing requires both
    // exact reservation sets.
    assert_eq!(
        f.row(
            "SELECT state FROM episodes WHERE episode_id = ?1",
            &ep.episode_id
        )
        .as_deref(),
        Some("eligible")
    );

    // Stage 4: the narrow Kovee adapter, with the subordinate confirmed.
    let admitted = f.admit_placement(&ep, "p1", Subordinate::Confirmed(200));
    assert_eq!(admitted["result"]["placement_admitted"], true);
    assert_eq!(admitted["result"]["bridge_state"], "confirmed");
    assert_eq!(admitted["result"]["episode_queued"], true);
    assert_eq!(
        f.row(
            "SELECT state FROM resource_allocations WHERE allocation_id = ?1",
            &ep.allocation_ref
        )
        .as_deref(),
        Some("bridged"),
        "stage 3 completes only with BOTH reservation sets (reserved -> bridged)"
    );
    assert_eq!(
        f.row(
            "SELECT state FROM episodes WHERE episode_id = ?1",
            &ep.episode_id
        )
        .as_deref(),
        Some("queued"),
        "eligible -> queued via resource_allocate, behind both reservation sets"
    );
    assert_eq!(
        f.row(
            "SELECT placement_admission_ref FROM episodes WHERE episode_id = ?1",
            &ep.episode_id
        )
        .as_deref(),
        Some("plc-kovee-placement-p1"),
        "the queued Episode cites the exact PlacementAdmission (stage 4)"
    );
}

#[test]
fn no_stage_can_be_skipped_and_no_other_origin_substitutes() {
    let f = Fixture::start("b3-act-skip", 8);

    // Stage 1 absent: the kernel evaluates only a COMMITTED WakeIntent.
    let no_intent = f.participant(&json!({
        "version": "0.2", "op": "episode_request",
        "meta": f.meta("ereq-none", None),
        "activity_stream_ref": f.stream,
        "generation": 1,
        "wake_intent_ref": "wake-does-not-exist",
        "activation_admission_ref": "adm-wake-does-not-exist-r1",
    }));
    assert_eq!(kind_of(&no_intent), "admission_required", "{no_intent}");

    // A caller cannot NAME a stage it did not obtain: the admission ref
    // is kernel-derived from the exact WakeIntent revision.
    let wake = f.wake("w1");
    let forged = f.participant(&json!({
        "version": "0.2", "op": "episode_request",
        "meta": f.meta("ereq-forged", None),
        "activity_stream_ref": f.stream,
        "generation": 1,
        "wake_intent_ref": wake,
        "activation_admission_ref": "adm-i-made-this-up",
    }));
    assert_eq!(kind_of(&forged), "admission_required", "{forged}");

    // Stage 4 absent: an eligible (unqueued) Episode cannot be claimed.
    let ep = f.request_episode(&wake, "e1");
    let early = f.claim_raw(&ep.episode_id, "worker-a", 300, 7, "bk-early", "c0");
    assert_eq!(kind_of(&early), "admission_required", "{early}");
    assert!(
        early["problem"]["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("not queued"),
        "{early}"
    );

    // Stage 3 needs a RESERVED allocation: a placement against an
    // already-released one is refused, and the released allocation's
    // narrow adapter channel is withdrawn with it.
    let placement_token = f.placement_token(&ep.allocation_ref);
    f.admit_placement(&ep, "p1", Subordinate::Denied);
    let after_denial =
        f.admit_placement_with(&ep, "p2", Subordinate::Confirmed(10), &placement_token);
    assert_eq!(kind_of(&after_denial), "stale_binding", "{after_denial}");
    assert!(
        !f.daemon
            .data_dir
            .join("channels")
            .join(format!("runtime-placement-{}.token", ep.allocation_ref))
            .exists(),
        "a released allocation's placement channel is withdrawn"
    );

    // No other origin substitutes for the participant channel: the
    // registry binds wake_intent_submit and episode_request to the
    // participant surface ONLY, so an arrival/attention/cron/model-score
    // driver on the runtime surface reaches no row at all.
    for op in ["wake_intent_submit", "episode_request"] {
        let body = json!({
            "version": "0.2", "op": op,
            "meta": f.meta("cron", None),
            "activity_stream_ref": f.stream, "generation": 1,
            "origin": "direct_participant",
            "exact_cause_ref": "cause-arrival-1",
            "exact_cause_digest": test_digest(0xc2),
            "purpose_ref": "purpose-explore-1",
            "stable_wake_key": "wake-from-arrival",
            "expires_at": far_future(),
            "wake_intent_ref": "wake-x", "activation_admission_ref": "adm-x",
        });
        let refused = f.runtime("rwk1.deadbeef", &body);
        assert_eq!(
            kind_of(&refused),
            "forbidden_surface",
            "{op} must not be reachable from the runtime surface: {refused}"
        );
    }
    // And the closed WakeIntent shape admits only the two §11.1 origins:
    // a host cron or a model score cannot even be spelled.
    let bad_origin = f.participant(&json!({
        "version": "0.2", "op": "wake_intent_submit",
        "meta": f.meta("wake-cron", None),
        "activity_stream_ref": f.stream, "generation": 1,
        "origin": "host_cron",
        "exact_cause_ref": "cause-cron-1",
        "exact_cause_digest": test_digest(0xc2),
        "purpose_ref": "purpose-explore-1",
        "stable_wake_key": "wake-cron-1",
        "expires_at": far_future(),
    }));
    assert_eq!(kind_of(&bad_origin), "invalid", "{bad_origin}");
}

#[test]
fn a_withdrawn_wake_intent_is_denied_and_the_denial_is_committed_evidence() {
    let f = Fixture::start("b3-act-withdrawn", 8);
    let wake = f.wake("w1");
    let withdrawn = f.participant(&json!({
        "version": "0.2", "op": "wake_intent_withdraw",
        "meta": f.meta("wdraw", Some(1)),
        "wake_intent_ref": wake,
    }));
    assert_eq!(withdrawn["outcome"], "ok", "{withdrawn}");
    // The withdrawal advances the WakeIntent revision, so the derived
    // admission id follows the exact revision the kernel evaluated.
    let reply = f.participant(&json!({
        "version": "0.2", "op": "episode_request",
        "meta": f.meta("ereq-withdrawn", None),
        "activity_stream_ref": f.stream,
        "generation": 1,
        "wake_intent_ref": wake,
        "activation_admission_ref": format!("adm-{wake}-r2"),
    }));
    assert_eq!(kind_of(&reply), "stale_binding", "{reply}");
    assert_eq!(
        f.row(
            "SELECT state FROM activation_admissions WHERE admission_id = ?1",
            &format!("adm-{wake}-r2")
        )
        .as_deref(),
        Some("denied"),
        "the denial is a COMMITTED record, not a dropped request (§14.8: a retry \
         returns the same admission)"
    );
    // No allocation and no Episode followed the denial.
    assert!(f
        .row(
            "SELECT state FROM resource_allocations WHERE allocation_id = ?1",
            &Fixture::allocation_ref(&wake)
        )
        .is_none());
}

#[test]
fn a_held_mandate_denies_activation() {
    let f = Fixture::start("b3-act-held", 8);
    let held = f.governance(&json!({
        "version": "0.2", "op": "mandate_hold",
        "meta": f.meta("mhold", Some(2)),
        "mandate_id": f.mandate_id,
        "held_by_decision_ref": common::mandate_decision(&f.mandate_id),
    }));
    assert_eq!(held["outcome"], "ok", "{held}");
    let wake = f.wake("w1");
    let reply = f.request_episode_raw(&wake, "e1");
    assert_eq!(kind_of(&reply), "mandate_held", "{reply}");
    assert_eq!(
        f.row(
            "SELECT eligibility_reason_codes FROM activation_admissions
             WHERE admission_id = ?1",
            &Fixture::admission_ref(&wake)
        )
        .as_deref(),
        Some("[\"mandate_held\"]")
    );
}

#[test]
fn a_revoked_mandate_denies_activation() {
    let f = Fixture::start("b3-act-revoked", 8);
    let revoked = f.governance(&json!({
        "version": "0.2", "op": "mandate_revoke",
        "meta": f.meta("mrev", Some(2)),
        "mandate_id": f.mandate_id,
        "revoked_by_decision_ref": common::mandate_decision(&f.mandate_id),
    }));
    assert_eq!(revoked["outcome"], "ok", "{revoked}");
    let wake = f.wake("w1");
    let reply = f.request_episode_raw(&wake, "e1");
    assert_eq!(kind_of(&reply), "forbidden", "{reply}");
    assert_eq!(
        f.row(
            "SELECT eligibility_reason_codes FROM activation_admissions
             WHERE admission_id = ?1",
            &Fixture::admission_ref(&wake)
        )
        .as_deref(),
        Some("[\"mandate_unusable\"]"),
        "a non-pledged ActivityStream never activates without a usable mandate \
         (§11.1)"
    );
}

#[test]
fn the_rate_ceiling_denies_activation() {
    // The mandate's concurrency ceiling is the RATE ceiling on Episodes
    // in flight: semantic scores may order an eligible set but cannot
    // create eligibility or a starvation exemption (§11.4).
    let f = Fixture::start("b3-act-rate", 2);
    for i in 0..2 {
        let wake = f.wake(&format!("w{i}"));
        f.request_episode(&wake, &format!("e{i}"));
    }
    let wake = f.wake("w-over");
    let reply = f.request_episode_raw(&wake, "e-over");
    assert_eq!(kind_of(&reply), "budget_exceeded", "{reply}");
    assert_eq!(
        f.row(
            "SELECT eligibility_reason_codes FROM activation_admissions
             WHERE admission_id = ?1",
            &Fixture::admission_ref(&wake)
        )
        .as_deref(),
        Some("[\"rate_ceiling\"]")
    );
}

#[test]
fn an_exhausted_budget_denies_activation() {
    // The mandate's §11.4 allowance is 1024 units and one Episode
    // reserves 256 worst case, so the fifth in-flight Episode is denied.
    let f = Fixture::start("b3-act-budget", 16);
    for i in 0..4 {
        let wake = f.wake(&format!("w{i}"));
        f.request_episode(&wake, &format!("e{i}"));
    }
    let ledger = f.ledger();
    assert_eq!(
        ledger.reserved,
        1024 + 1024,
        "four episodes plus the mandate hold"
    );
    assert!(ledger.conserves(), "{ledger:?}");
    let wake = f.wake("w-over");
    let reply = f.request_episode_raw(&wake, "e-over");
    assert_eq!(kind_of(&reply), "budget_exceeded", "{reply}");
    assert_eq!(
        f.row(
            "SELECT eligibility_reason_codes FROM activation_admissions
             WHERE admission_id = ?1",
            &Fixture::admission_ref(&wake)
        )
        .as_deref(),
        Some("[\"budget_exhausted\"]")
    );
    assert!(
        f.ledger().conserves(),
        "conservation holds through the refusal"
    );
}
