//! B3 slice 2 — the subordinate budget bridge (DESIGN.md §11.4; family
//! contract L31–L33), machine-checked in
//! `proof/specs/SubordinateReservation.tla`.
//!
//! The invariants this suite pins on the real daemon, named after the
//! model's:
//!
//! ```text
//! NeverAboveParent               amount <= parent_worst_case_amount, same
//!                                dimension and unit — narrow or deny, never
//!                                above parent
//! CreateOnce                     the exact retry under the same stable key
//!                                returns the identical row
//! ChargeWithinReservation        the settled charge never exceeds the reserve
//! SettleOnce                     applied once on both sides
//! HeldIffOpen                    an unknown result never unblocks spend
//! UncertainReleaseNeedsGovernance the ONLY release out of uncertain is R38
//! ```
//!
//! and, over every step, the §11.4 conservation identity
//! `ceiling = remaining + reserved + committed + uncertain + delegated`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::runtime::{merge, Fixture, Subordinate, PARENT_ACCOUNT, WORST_CASE};
use common::{kind_of, test_digest};
use serde_json::{json, Value};

fn portable(seed: u8) -> Value {
    common::runtime::portable_digest(seed)
}

#[test]
fn a_subordinate_reservation_is_never_above_its_parent() {
    let f = Fixture::start("b3-bud-above", 8);
    let wake = f.wake("w1");
    let ep = f.request_episode(&wake, "e1");
    let token = f.placement_token(&ep.allocation_ref);
    let allocation_digest = f.allocation_digest(&ep.allocation_ref);

    // ABOVE PARENT: schema-shape-valid, refused as a cross-member check.
    let above = f.runtime(
        &token,
        &json!({
            "version": "0.2", "op": "placement_admit",
            "meta": f.meta("plc-above", None),
            "resource_allocation_ref": ep.allocation_ref,
            "resource_allocation_digest": allocation_digest,
            "kovee_placement_ref": "kovee-placement-above",
            "kovee_placement_revision": 1,
            "kovee_placement_digest": portable(0x5d),
            "source_binding_epoch": 1,
            "selected_manifestation_ref": "manif-selected-1",
            "kovee_invocation_ref": "kovee-inv-above",
            "kovee_fence_epoch": 7,
            "subordinate_reservation": {
                "stable_external_reservation_key": ep.stable_external_key,
                "outcome": "confirmed",
                "subordinate_reservation_ref": "kovee-sub-above",
                "revision": 1, "digest": portable(0x5c),
                "items": [{
                    "kovee_account_ref": "kovee-acct-1",
                    "dimension": "unit", "unit": "unit",
                    "amount": WORST_CASE + 1,
                    "parent_account_ref": PARENT_ACCOUNT,
                    "parent_account_revision": 1,
                    "parent_dimension": "unit", "parent_unit": "unit",
                    "parent_worst_case_amount": WORST_CASE,
                }],
            },
        }),
    );
    assert_eq!(kind_of(&above), "budget_exceeded", "{above}");

    // A RESHAPED dimension is refused too: a subordinate reservation may
    // narrow or deny but never reshape or parallel-charge.
    let reshaped = f.runtime(
        &token,
        &json!({
            "version": "0.2", "op": "placement_admit",
            "meta": f.meta("plc-reshape", None),
            "resource_allocation_ref": ep.allocation_ref,
            "resource_allocation_digest": f.allocation_digest(&ep.allocation_ref),
            "kovee_placement_ref": "kovee-placement-reshape",
            "kovee_placement_revision": 1,
            "kovee_placement_digest": portable(0x5d),
            "source_binding_epoch": 1,
            "selected_manifestation_ref": "manif-selected-1",
            "kovee_invocation_ref": "kovee-inv-reshape",
            "kovee_fence_epoch": 7,
            "subordinate_reservation": {
                "stable_external_reservation_key": ep.stable_external_key,
                "outcome": "confirmed",
                "subordinate_reservation_ref": "kovee-sub-reshape",
                "revision": 1, "digest": portable(0x5c),
                "items": [{
                    "kovee_account_ref": "kovee-acct-1",
                    "dimension": "tokens", "unit": "unit", "amount": 10,
                    "parent_account_ref": PARENT_ACCOUNT,
                    "parent_account_revision": 1,
                    "parent_dimension": "unit", "parent_unit": "unit",
                    "parent_worst_case_amount": WORST_CASE,
                }],
            },
        }),
    );
    assert_eq!(kind_of(&reshaped), "invalid", "{reshaped}");

    // NARROWED is fine, and the bridge records exactly what Kovee said.
    f.admit_placement(&ep, "p1", Subordinate::Confirmed(200));
    assert_eq!(
        f.row(
            "SELECT state FROM subordinate_reservations
             WHERE subordinate_reservation_ref = ?1",
            "kovee-sub-p1"
        )
        .as_deref(),
        Some("confirmed")
    );
    assert_eq!(
        f.row(
            "SELECT reservation_class FROM subordinate_reservations
             WHERE subordinate_reservation_ref = ?1",
            "kovee-sub-p1"
        )
        .as_deref(),
        Some("byom_subordinate")
    );
    assert!(f.ledger().conserves(), "{:?}", f.ledger());
}

#[test]
fn the_create_is_idempotent_over_the_stable_key() {
    let f = Fixture::start("b3-bud-once", 8);
    let wake = f.wake("w1");
    let ep = f.request_episode(&wake, "e1");
    let first = f.admit_placement(&ep, "p1", Subordinate::Confirmed(200));
    let again = f.admit_placement(&ep, "p1", Subordinate::Confirmed(200));
    assert_eq!(
        first["result"], again["result"],
        "the exact retry under the same stable key returns the identical row \
         (CreateOnce)"
    );
    assert_eq!(
        f.count("SELECT COUNT(*) FROM subordinate_reservations"),
        1,
        "never a second reservation row"
    );
    // A DIFFERENT placement against a bridge that is no longer
    // `requested` conflicts: a released bridge never revives.
    let other = f.admit_placement_raw(&ep, "p2", Subordinate::Confirmed(10));
    assert_eq!(kind_of(&other), "stale_binding", "{other}");
}

#[test]
fn conservation_holds_across_reserve_commit_settle_and_release() {
    let f = Fixture::start("b3-bud-conserve", 8);
    let base = f.ledger();
    assert!(base.conserves(), "{base:?}");
    // `mandate_issue` already reserved the mandate's allowance.
    let mandate_hold = base.reserved;

    let wake = f.wake("w1");
    let ep = f.request_episode(&wake, "e1");
    // RESERVE: the worst case moves remaining -> reserved in ONE byom
    // transaction (§11.4).
    let reserved = f.ledger();
    assert!(reserved.conserves(), "{reserved:?}");
    assert_eq!(reserved.reserved, mandate_hold + WORST_CASE as i64);
    assert_eq!(reserved.remaining, base.remaining - WORST_CASE as i64);

    f.admit_placement(&ep, "p1", Subordinate::Confirmed(200));
    assert!(f.ledger().conserves());
    assert_eq!(
        f.ledger().reserved,
        reserved.reserved,
        "confirming the subordinate reservation moves no byom quantity: the \
         parent stays reserved (no parallel charge, no early release)"
    );

    let c = f.claim(&ep.episode_id, "worker-a", 600, 7, "c1");
    let started = f.start_episode(&ep.episode_id, &c, "s1");
    let lease_revision = started["result"]["lease_revision"].as_u64().unwrap();

    // SETTLE from a TRUSTED METER: 140 of the 256 reserved units are
    // charged, the remainder stays held until the saga releases it.
    let meter_token = f.meter_token(&ep.episode_id);
    let mut settle = json!({
        "version": "0.2", "op": "usage_report",
        "meta": f.meta("meter", None),
        "source": "trusted_meter",
        "stable_report_key": "urepkey-meter",
        "quantities": [{"dimension": "unit", "unit": "unit", "amount": 140}],
        "meter_ref": "meter-kovee-1",
        "meter_attestation_ref": "attest-1",
        "stable_settlement_key": "setkey-1",
        "charged_quantities": [{"dimension": "unit", "unit": "unit", "amount": 140}],
    });
    merge(&mut settle, f.fences(&ep.episode_id, &c));
    let settled = f.runtime(&meter_token, &settle);
    assert_eq!(settled["outcome"], "ok", "{settled}");
    assert_eq!(
        settled["result"]["settlement"]["settled"], true,
        "{settled}"
    );
    assert_eq!(settled["result"]["settlement"]["charged"], 140);
    let after_settle = f.ledger();
    assert!(after_settle.conserves(), "{after_settle:?}");
    assert_eq!(after_settle.committed, 140);
    assert_eq!(
        after_settle.reserved,
        reserved.reserved - 140,
        "the charge left `reserved` for `committed`; the remainder is still held"
    );
    assert_eq!(
        f.row(
            "SELECT state FROM external_budget_bridges WHERE bridge_id = ?1",
            &ep.bridge_ref
        )
        .as_deref(),
        Some("settled")
    );

    // SettleOnce, two ways. The exact retry replays the stored transition
    // byte-identically (the §15.3 idempotency record)...
    let replay = f.runtime(&meter_token, &settle);
    assert_eq!(replay, settled, "the exact retry is byte-identical");
    // ...and a FRESH request under the same stable settlement key serves
    // the stored settlement head instead of settling twice.
    let mut resettle = settle.clone();
    merge(&mut resettle, json!({"meta": f.meta("meter-fresh", None)}));
    let head_replay = f.runtime(&meter_token, &resettle);
    assert_eq!(head_replay["outcome"], "ok", "{head_replay}");
    assert_eq!(head_replay["result"]["settlement"]["replayed"], true);
    assert_eq!(
        f.count("SELECT COUNT(*) FROM usage_settlements"),
        1,
        "applied once on both sides (SettleOnce)"
    );
    assert_eq!(f.ledger(), after_settle, "a replayed settle moves nothing");

    // A worker report can never settle, whatever it claims: the closed
    // shape refuses the settlement group on the worker arm, and the
    // worker channel is not the meter channel.
    let worker_token = f.worker_token(&ep.episode_id);
    let forged = f.runtime(&worker_token, &settle);
    assert_eq!(
        kind_of(&forged),
        "forbidden",
        "the worker's Episode token is not the trusted-meter channel: {forged}"
    );

    // RELEASE at terminalization: exactly the reserved remainder returns.
    let mut complete = json!({
        "version": "0.2", "op": "episode_complete",
        "meta": f.meta("cmp", Some(lease_revision)),
        "output_refs": [], "evidence_refs": [], "usage_report_refs": [],
    });
    merge(&mut complete, f.fences(&ep.episode_id, &c));
    let completed = f.runtime(&worker_token, &complete);
    assert_eq!(completed["outcome"], "ok", "{completed}");
    let after_release = f.ledger();
    assert!(after_release.conserves(), "{after_release:?}");
    assert_eq!(after_release.committed, 140);
    assert_eq!(
        after_release.remaining,
        base.remaining - 140,
        "only the measured charge left `remaining`; the rest came back"
    );
    assert_eq!(after_release.reserved, mandate_hold);
    assert_eq!(
        f.row(
            "SELECT state FROM external_budget_bridges WHERE bridge_id = ?1",
            &ep.bridge_ref
        )
        .as_deref(),
        Some("released")
    );
}

#[test]
fn nothing_settles_on_a_bridge_that_was_never_confirmed() {
    let f = Fixture::start("b3-bud-nocommit", 8);
    let wake = f.wake("w1");
    let ep = f.request_episode(&wake, "e1");
    f.admit_placement(&ep, "p1", Subordinate::Confirmed(200));
    let c = f.claim(&ep.episode_id, "worker-a", 600, 7, "c1");
    f.start_episode(&ep.episode_id, &c, "s1");
    // Settle once...
    let meter_token = f.meter_token(&ep.episode_id);
    let mut settle = json!({
        "version": "0.2", "op": "usage_report",
        "meta": f.meta("meter-1", None),
        "source": "trusted_meter",
        "stable_report_key": "urepkey-1",
        "quantities": [{"dimension": "unit", "unit": "unit", "amount": 10}],
        "meter_ref": "meter-kovee-1", "meter_attestation_ref": "attest-1",
        "stable_settlement_key": "setkey-1",
        "charged_quantities": [{"dimension": "unit", "unit": "unit", "amount": 10}],
    });
    merge(&mut settle, f.fences(&ep.episode_id, &c));
    assert_eq!(f.runtime(&meter_token, &settle)["outcome"], "ok");
    // ...and a DIFFERENT settlement key cannot settle the same use again
    // (the unique reservation/key head).
    merge(
        &mut settle,
        json!({"meta": f.meta("meter-2", None),
               "stable_report_key": "urepkey-2",
               "stable_settlement_key": "setkey-2"}),
    );
    let twice = f.runtime(&meter_token, &settle);
    assert_eq!(kind_of(&twice), "stale_binding", "{twice}");
    // And an over-charge beyond the reserved amount is refused
    // (ChargeWithinReservation).
    let f2 = Fixture::start("b3-bud-overcharge", 8);
    let wake2 = f2.wake("w1");
    let ep2 = f2.request_episode(&wake2, "e1");
    f2.admit_placement(&ep2, "p1", Subordinate::Confirmed(200));
    let c2 = f2.claim(&ep2.episode_id, "worker-a", 600, 7, "c1");
    f2.start_episode(&ep2.episode_id, &c2, "s1");
    let mut over = json!({
        "version": "0.2", "op": "usage_report",
        "meta": f2.meta("meter-over", None),
        "source": "trusted_meter",
        "stable_report_key": "urepkey-over",
        "quantities": [{"dimension": "unit", "unit": "unit", "amount": 1}],
        "meter_ref": "meter-kovee-1", "meter_attestation_ref": "attest-1",
        "stable_settlement_key": "setkey-over",
        "charged_quantities": [{"dimension": "unit", "unit": "unit",
                                "amount": WORST_CASE + 1}],
    });
    merge(&mut over, f2.fences(&ep2.episode_id, &c2));
    let refused = f2.runtime(&f2.meter_token(&ep2.episode_id), &over);
    assert_eq!(kind_of(&refused), "budget_exceeded", "{refused}");
}

#[test]
fn an_unknown_outcome_stays_uncertain_and_only_r38_releases_it() {
    let f = Fixture::start("b3-bud-uncertain", 8);
    let base = f.ledger();
    let wake = f.wake("w1");
    let ep = f.request_episode(&wake, "e1");
    let placement_token = f.placement_token(&ep.allocation_ref);

    // Transport loss: a recorded FACT, not a decision.
    let unknown = f.admit_placement(&ep, "p1", Subordinate::Uncertain);
    assert_eq!(unknown["result"]["placement_admitted"], false);
    assert_eq!(unknown["result"]["bridge_state"], "uncertain");
    assert_eq!(unknown["result"]["episode_queued"], false);
    // HeldIffOpen: the byom reservation is NOT released; the hold moves
    // into the §11.4 `uncertain` bucket so spend stays blocked and
    // conservation still holds.
    let held = f.ledger();
    assert!(held.conserves(), "{held:?}");
    assert_eq!(held.uncertain, WORST_CASE as i64);
    assert_eq!(held.remaining, base.remaining - WORST_CASE as i64);
    assert_eq!(
        f.row(
            "SELECT state FROM episodes WHERE episode_id = ?1",
            &ep.episode_id
        )
        .as_deref(),
        Some("eligible"),
        "an uncertain bridge stays UNQUEUED"
    );

    // The stable query still cannot prove the outcome: a conservative
    // hold, nothing releases.
    let still = f.admit_placement_with(&ep, "p1", Subordinate::Uncertain, &placement_token);
    assert_eq!(still["result"]["bridge_state"], "uncertain", "{still}");
    assert_eq!(f.ledger(), held, "nothing releases on a repeated unknown");

    // Guessing is not a transition: no timeout releases the hold. The
    // ONLY release is the R38 seat.
    let released = f.governance(&json!({
        "version": "0.2", "op": "budget_reconcile",
        "meta": f.meta("brc", None),
        "external_budget_bridge_ref": ep.bridge_ref,
        "stable_external_reservation_key": ep.stable_external_key,
        "fresh_challenge_ref": "challenge-1",
        "reason_ref": "reason-transport-loss",
    }));
    assert_eq!(released["outcome"], "ok", "{released}");
    assert_eq!(released["result"]["state"], "released");
    assert_eq!(released["result"]["released_from_bucket"], "uncertain");
    assert_eq!(released["result"]["released_amount"], WORST_CASE);
    let decision = released["result"]["governance_decision_ref"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        f.row(
            "SELECT kind FROM governance_decisions WHERE decision_id = ?1",
            &decision
        )
        .as_deref(),
        Some("budget_reconciliation"),
        "the release is recorded under an immutable GovernanceDecision, never a \
         timeout (family contract L33)"
    );
    let after = f.ledger();
    assert!(after.conserves(), "{after:?}");
    assert_eq!(after.uncertain, 0);
    assert_eq!(after.remaining, base.remaining);

    // A released bridge never revives.
    let twice = f.governance(&json!({
        "version": "0.2", "op": "budget_reconcile",
        "meta": f.meta("brc-2", None),
        "external_budget_bridge_ref": ep.bridge_ref,
        "stable_external_reservation_key": ep.stable_external_key,
        "fresh_challenge_ref": "challenge-2",
        "reason_ref": "reason-transport-loss",
    }));
    assert_eq!(kind_of(&twice), "stale_binding", "{twice}");
}

#[test]
fn a_denial_releases_only_the_demonstrably_unspent_quantity() {
    let f = Fixture::start("b3-bud-denied", 8);
    let base = f.ledger();
    let wake = f.wake("w1");
    let ep = f.request_episode(&wake, "e1");
    let denied = f.admit_placement(&ep, "p1", Subordinate::Denied);
    assert_eq!(denied["result"]["bridge_state"], "released");
    assert_eq!(denied["result"]["episode_queued"], false);
    let after = f.ledger();
    assert!(after.conserves(), "{after:?}");
    assert_eq!(after.remaining, base.remaining, "nothing was ever charged");
    assert_eq!(after.committed, base.committed);
}

#[test]
fn an_unmeasured_use_settles_to_the_conservative_maximum() {
    let f = Fixture::start("b3-bud-conservative", 8);
    let base = f.ledger();
    let wake = f.wake("w1");
    let ep = f.request_episode(&wake, "e1");
    f.admit_placement(&ep, "p1", Subordinate::Confirmed(200));
    let c = f.claim(&ep.episode_id, "worker-a", 600, 7, "c1");
    let started = f.start_episode(&ep.episode_id, &c, "s1");
    let lease_revision = started["result"]["lease_revision"].as_u64().unwrap();
    let token = f.worker_token(&ep.episode_id);
    // No trusted meter ever settled: §11.4 keeps the reservation or
    // settles to the conservative maximum.
    let mut fail = json!({
        "version": "0.2", "op": "episode_fail",
        "meta": f.meta("fal", Some(lease_revision)),
        "failure_reason_ref": "reason-tool-error",
        "evidence_refs": ["ev-1"],
    });
    merge(&mut fail, f.fences(&ep.episode_id, &c));
    let failed = f.runtime(&token, &fail);
    assert_eq!(failed["outcome"], "ok", "{failed}");
    assert_eq!(failed["result"]["state"], "failed");
    assert_eq!(
        failed["result"]["settlement"]["status"],
        "conservatively_maxed"
    );
    let after = f.ledger();
    assert!(after.conserves(), "{after:?}");
    assert_eq!(
        after.committed, WORST_CASE as i64,
        "the whole worst case is charged, not silently released"
    );
    assert_eq!(after.remaining, base.remaining - WORST_CASE as i64);
    assert_eq!(
        f.row(
            "SELECT status FROM usage_settlements WHERE reservation_set_ref = ?1",
            &format!("rset-{}", ep.allocation_ref)
        )
        .as_deref(),
        Some("conservatively_maxed")
    );
    let _ = test_digest(0);
}

#[test]
fn the_stable_query_surfaces_koveees_durable_truth_from_uncertain() {
    // `ResolutionIsReal`: the recovery query can only surface what Kovee
    // actually did — byom records the resolution, it never invents one.
    let f = Fixture::start("b3-bud-resolve", 8);
    let base = f.ledger();
    let wake = f.wake("w1");
    let ep = f.request_episode(&wake, "e1");
    let placement_token = f.placement_token(&ep.allocation_ref);
    f.admit_placement(&ep, "p1", Subordinate::Uncertain);
    assert_eq!(f.ledger().uncertain, WORST_CASE as i64);

    // uncertain -> confirmed: the parked hold comes back into `reserved`
    // (the byom parent was held the whole time), and the Episode queues.
    let resolved = f.admit_placement_with(&ep, "q1", Subordinate::Confirmed(200), &placement_token);
    assert_eq!(resolved["outcome"], "ok", "{resolved}");
    assert_eq!(resolved["result"]["bridge_state"], "confirmed");
    assert_eq!(resolved["result"]["episode_queued"], true);
    let after = f.ledger();
    assert!(after.conserves(), "{after:?}");
    assert_eq!(after.uncertain, 0);
    assert_eq!(after.reserved, base.reserved + WORST_CASE as i64);
    assert_eq!(
        f.row(
            "SELECT state FROM episodes WHERE episode_id = ?1",
            &ep.episode_id
        )
        .as_deref(),
        Some("queued")
    );

    // The other resolution: a verified ABSENCE releases exactly the
    // demonstrably unspent quantity, from whichever bucket held it.
    let f2 = Fixture::start("b3-bud-resolve-denied", 8);
    let base2 = f2.ledger();
    let wake2 = f2.wake("w1");
    let ep2 = f2.request_episode(&wake2, "e1");
    let token2 = f2.placement_token(&ep2.allocation_ref);
    f2.admit_placement(&ep2, "p1", Subordinate::Uncertain);
    let denied = f2.admit_placement_with(&ep2, "q1", Subordinate::Denied, &token2);
    assert_eq!(denied["result"]["bridge_state"], "released", "{denied}");
    let after2 = f2.ledger();
    assert!(after2.conserves(), "{after2:?}");
    assert_eq!(after2.uncertain, 0);
    assert_eq!(after2.remaining, base2.remaining);
}
