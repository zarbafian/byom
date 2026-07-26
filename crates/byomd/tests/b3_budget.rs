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

use common::runtime::{merge, subordinate_item, Fixture, Subordinate, PARENT_ACCOUNT, WORST_CASE};
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
    let allocation_digest = ep.allocation_digest.clone();

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
            "resource_allocation_digest": ep.allocation_digest,
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
    // **R3-U02, byom's half of the saga.** The measured path reaches the same
    // terminal record: the counterparty's row carries byom's committed charge
    // and is released, so both sides end this Episode naming one number.
    assert_eq!(
        f.row(
            "SELECT state FROM subordinate_reservations
             WHERE subordinate_reservation_ref = ?1",
            "kovee-sub-p1"
        )
        .as_deref(),
        Some("released")
    );
    assert_eq!(
        f.number(
            "SELECT json_extract(record, '$.byom_terminal.charged')
             FROM subordinate_reservations WHERE subordinate_reservation_ref = ?1",
            "kovee-sub-p1",
        ),
        Some(140),
        "byom's own committed charge, on byom's own row"
    );
    assert_eq!(
        completed["result"]["settlement"]["subordinate_reservation_ref"], "kovee-sub-p1",
        "{completed}"
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
    // **R3-U02, byom's half of the saga.** byom just charged the whole bridge
    // on its OWN authority. Its durable record must name what the counterparty
    // now owes, in the same transaction, or a counterparty that crashed
    // between the two sides comes back to a `confirmed` row and no committed
    // byom fact to converge on — which is exactly how the two ledgers split.
    assert_eq!(
        f.row(
            "SELECT state FROM subordinate_reservations
             WHERE subordinate_reservation_ref = ?1",
            "kovee-sub-p1"
        )
        .as_deref(),
        Some("settled"),
        "the counterparty's row used to stay `confirmed` for ever while byom's \
         parent was committed"
    );
    assert_eq!(
        f.number(
            "SELECT json_extract(record, '$.byom_terminal.charged')
             FROM subordinate_reservations WHERE subordinate_reservation_ref = ?1",
            "kovee-sub-p1",
        ),
        Some(WORST_CASE as i64),
        "byom's own committed number, recorded against the counterparty"
    );
    assert_eq!(
        failed["result"]["settlement"]["subordinate_reservation_ref"], "kovee-sub-p1",
        "and the reply names the exact row, so the counterparty can converge \
         on it: {failed}"
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

// ============================== R3 probes: the two-sided settlement saga ==

/// **R3-U01 (byom half), reproduced.** The probe that produced the finding:
/// the parent worst case is 256, kovee narrows the subordinate to 128, and a
/// charge of 200 arrives. Capping against the parent alone accepted it —
/// byom committed 200 while the other side, checking its own 128, answered
/// `budget_exceeded` and stayed `confirmed` with charge 0. The two ledgers
/// split.
///
/// byom now caps INDEPENDENTLY against the EXACT confirmed subordinate items
/// of this bridge (disposition D-R3-2). The boundary the finding asked for is
/// `subordinate + 1`: still far below the parent, and still refused.
#[test]
fn a_settlement_is_capped_against_the_exact_confirmed_subordinate_items() {
    let f = Fixture::start("b3-bud-independent-cap", 8);
    let base = f.ledger();
    let wake = f.wake("w1");
    let ep = f.request_episode(&wake, "e1");
    // Narrowed to half the parent, exactly as the probe had it.
    const SUBORDINATE: u64 = WORST_CASE / 2;
    f.admit_placement(&ep, "p1", Subordinate::Confirmed(SUBORDINATE));
    let c = f.claim(&ep.episode_id, "worker-a", 600, 7, "c1");
    let started = f.start_episode(&ep.episode_id, &c, "s1");
    let lease_revision = started["result"]["lease_revision"].as_u64().unwrap();
    let meter = f.meter_token(&ep.episode_id);
    let held = f.ledger();

    // `request_id` is separate from the stable settlement key on purpose: the
    // exact retry is the §15.3 idempotency replay, while a FRESH request under
    // the same settlement key is the saga's recovery query.
    let charge = |amount: u64, key: &str, request: &str| -> Value {
        let mut body = json!({
            "version": "0.2", "op": "usage_report",
            "meta": f.meta(&format!("meter-{request}"), None),
            "source": "trusted_meter",
            "stable_report_key": format!("urepkey-{request}"),
            "quantities": [{"dimension": "unit", "unit": "unit", "amount": amount}],
            "meter_ref": "meter-kovee-1",
            "meter_attestation_ref": "attest-1",
            "stable_settlement_key": format!("setkey-{key}"),
            "charged_quantities": [{"dimension": "unit", "unit": "unit", "amount": amount}],
        });
        merge(&mut body, f.fences(&ep.episode_id, &c));
        f.runtime(&meter, &body)
    };

    // THE PROBE: 200 units. Below the 256 parent, above the 128 confirmed
    // subordinate.
    let over = charge(200, "probe", "probe");
    assert_eq!(
        kind_of(&over),
        "budget_exceeded",
        "a charge above the exact confirmed subordinate items must be refused \
         HERE, not committed and then contradicted by the other side: {over}"
    );
    assert!(
        over["problem"]["detail"]
            .as_str()
            .unwrap_or_default()
            .contains(&format!("requested 200, remaining {SUBORDINATE}")),
        "the refusal names the SUBORDINATE ceiling, not the parent's: {over}"
    );
    // `subordinate + 1 <= parent` — the exact boundary the finding named.
    let boundary = charge(SUBORDINATE + 1, "boundary", "boundary");
    assert_eq!(
        kind_of(&boundary),
        "budget_exceeded",
        "subordinate + 1 is still <= parent and still refused: {boundary}"
    );

    // Nothing moved and nothing was recorded: the refusals precede every
    // write, so a split ledger is not reachable through this path.
    assert_eq!(f.ledger(), held, "a refused settlement moves no quantity");
    assert!(f.ledger().conserves(), "{:?}", f.ledger());
    assert_eq!(f.count("SELECT COUNT(*) FROM usage_settlements"), 0);
    assert_eq!(
        f.row(
            "SELECT state FROM external_budget_bridges WHERE bridge_id = ?1",
            &ep.bridge_ref
        )
        .as_deref(),
        Some("confirmed"),
        "a refused settlement never advances the bridge"
    );

    // Exactly at the subordinate ceiling settles, and the reply reports the
    // ceiling it capped against so the counterparty's saga can agree.
    let exact = charge(SUBORDINATE, "exact", "exact");
    assert_eq!(exact["outcome"], "ok", "{exact}");
    assert_eq!(exact["result"]["settlement"]["charged"], SUBORDINATE);
    assert_eq!(
        exact["result"]["settlement"]["subordinate_reserved"],
        SUBORDINATE
    );
    let after = f.ledger();
    assert!(after.conserves(), "{after:?}");
    assert_eq!(after.committed, SUBORDINATE as i64);
    assert_eq!(after.remaining, base.remaining - WORST_CASE as i64);

    // The recovery query of the two-sided saga: a fresh request under the
    // SAME stable settlement key surfaces the stored charge, so the other
    // side can apply byom's truth after a crash instead of inventing one.
    let replay = charge(SUBORDINATE, "exact", "exact-recovery");
    assert_eq!(replay["result"]["settlement"]["replayed"], true, "{replay}");
    assert_eq!(replay["result"]["settlement"]["charged"], SUBORDINATE);
    assert_eq!(f.count("SELECT COUNT(*) FROM usage_settlements"), 1);
    assert_eq!(f.ledger(), after, "the recovery query moves nothing");
    let _ = lease_revision;
}

/// **R3-U04, reproduced.** The probe reported two children against ONE
/// parent item and was accepted: the membership test was `.any()`, so every
/// duplicate reused the same parent item and the recorded subordinate total
/// came to twice the parent.
///
/// Identity is now one-to-one — each reported item claims one distinct
/// parent item — and the aggregate per `(account, revision, dimension, unit)`
/// is capped by the parent items that really exist.
#[test]
fn duplicate_parent_item_pins_cannot_amplify_the_parent() {
    let f = Fixture::start("b3-bud-duplicate", 8);
    let wake = f.wake("w1");
    let ep = f.request_episode(&wake, "e1");
    let parent = ep.parent_budget["items"][0].clone();
    assert_eq!(
        ep.parent_budget["items"].as_array().map(Vec::len),
        Some(1),
        "the published parent has exactly ONE item, which is the point"
    );
    let held = f.ledger();

    // THE PROBE: two items, each individually below the parent worst case,
    // both pinning that one parent item.
    let doubled = f.admit_placement_raw(
        &ep,
        "dup",
        Subordinate::ConfirmedItems(vec![
            subordinate_item(&parent, 200),
            subordinate_item(&parent, 200),
        ]),
    );
    assert_eq!(
        kind_of(&doubled),
        "stale_binding",
        "the second item pins a parent item the first already claimed: {doubled}"
    );
    assert!(
        doubled["problem"]["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("UNCLAIMED"),
        "{doubled}"
    );

    // And the aggregate rule holds independently: two items whose sum
    // exceeds the parent are refused even before uniqueness would bite.
    let summed = f.admit_placement_raw(
        &ep,
        "sum",
        Subordinate::ConfirmedItems(vec![
            subordinate_item(&parent, WORST_CASE),
            subordinate_item(&parent, 1),
        ]),
    );
    let summed_kind = kind_of(&summed);
    assert!(
        summed_kind == "stale_binding" || summed_kind == "budget_exceeded",
        "a subordinate total above the actual parent is refused: {summed}"
    );

    // Nothing was recorded for either attempt: the bridge is still
    // `requested` and no subordinate row exists.
    assert_eq!(f.count("SELECT COUNT(*) FROM subordinate_reservations"), 0);
    assert_eq!(
        f.row(
            "SELECT state FROM external_budget_bridges WHERE bridge_id = ?1",
            &ep.bridge_ref
        )
        .as_deref(),
        Some("requested")
    );
    assert_eq!(f.ledger(), held, "a refused admission moves no quantity");

    // One item against one parent item is exactly what the rule permits.
    let ok = f.admit_placement_raw(
        &ep,
        "one",
        Subordinate::ConfirmedItems(vec![subordinate_item(&parent, 200)]),
    );
    assert_eq!(ok["outcome"], "ok", "{ok}");
    assert!(f.ledger().conserves());
}

/// **R3-L02, reproduced (byom half).** Before the fragment, byom published
/// only the allocation id and digest; every other parent fact — the
/// reservation-set reference, the bridge reference, the stable key, the
/// account and the worst-case amount — was reconstructed by the counterparty
/// from a naming convention and its own caller's arguments.
///
/// `episode_request` now publishes the FROZEN `portable_public` fragment, its
/// digest re-derives from exactly those bytes, and a subordinate item whose
/// parent facts were invented instead of taken from it is refused.
#[test]
fn episode_request_publishes_a_verifiable_parent_budget_fragment() {
    use bpp_core::canonical::{sha256_hex, tagged_canonical};

    let f = Fixture::start("b3-bud-fragment", 8);
    let wake = f.wake("w1");
    let ep = f.request_episode(&wake, "e1");
    let fragment = ep.parent_budget.clone();

    // The frozen member set, exactly — nothing more, nothing less.
    let mut members: Vec<&str> = fragment
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .filter(|k| *k != "digest")
        .collect();
    members.sort_unstable();
    assert_eq!(
        members,
        vec![
            "byom_budget_reservation_set_ref",
            "byom_budget_reservation_set_revision",
            "byom_budget_reservation_set_digest",
            "external_budget_bridge_ref",
            "external_budget_bridge_revision",
            "stable_external_reservation_key",
            "items",
        ]
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>(),
    );

    // INDEPENDENT re-derivation, the way a consumer must do it: strip the
    // digest, tag the remainder, hash. A fragment whose digest a consumer
    // cannot re-derive is an out-of-band step wearing a digest.
    let mut without = fragment.clone();
    without.as_object_mut().unwrap().remove("digest");
    let bytes = tagged_canonical("bpp-parent-budget-fragment-v0", &without).unwrap();
    assert_eq!(
        fragment["digest"]["value_hex"].as_str().unwrap(),
        sha256_hex(&bytes),
        "the published parent-budget digest must re-derive from exactly the \
         published members: {fragment}"
    );
    // The nested set digest is portable too, for the same reason.
    let set_bytes = tagged_canonical(
        "bpp-budget-reservation-set-binding-v0",
        &json!({
            "reservation_set_id": fragment["byom_budget_reservation_set_ref"],
            "revision": fragment["byom_budget_reservation_set_revision"],
            "items": fragment["items"],
        }),
    )
    .unwrap();
    assert_eq!(
        fragment["byom_budget_reservation_set_digest"]["value_hex"]
            .as_str()
            .unwrap(),
        sha256_hex(&set_bytes)
    );

    // The published facts are byom's real ones: the bridge and the stable
    // key the kernel derived, and the exact parent items.
    assert_eq!(
        f.row(
            "SELECT stable_external_reservation_key FROM external_budget_bridges
             WHERE bridge_id = ?1",
            fragment["external_budget_bridge_ref"].as_str().unwrap()
        )
        .as_deref(),
        fragment["stable_external_reservation_key"].as_str()
    );
    assert_eq!(fragment["items"][0]["worst_case_amount"], WORST_CASE);
    assert_eq!(fragment["items"][0]["account_ref"], PARENT_ACCOUNT);
    // And it is the SAME construction the pinned family vector records, so
    // the consumer's copy and this producer cannot drift apart in silence.
    assert_eq!(
        fragment["digest"]["class"], "portable_public",
        "the cross-boundary class (A8)"
    );

    // A consumer that INVENTS a parent fact instead of taking it from the
    // fragment is refused: the revision here is one the allocation never
    // reserved.
    let mut invented = subordinate_item(&fragment["items"][0], 100);
    invented["parent_account_revision"] = json!(7);
    let refused =
        f.admit_placement_raw(&ep, "invented", Subordinate::ConfirmedItems(vec![invented]));
    assert_eq!(
        kind_of(&refused),
        "stale_binding",
        "a parent revision the allocation never reserved is not an exact parent \
         item: {refused}"
    );
    assert_eq!(f.count("SELECT COUNT(*) FROM subordinate_reservations"), 0);
}

/// **The producer half of the pinned family vector (R3-L02, D-R3-3).**
///
/// `crates/byomd/tests/vectors/parent-budget-fragment.json` is a RECORDING
/// of this producer's output, and kovee's shipped test verifies that exact
/// recording. Before it existed, kovee's cross-contract test constructed AND
/// verified both sides, so changing only kovee's domain tag left it green —
/// producer and consumer were the same code.
///
/// This test is the other end of that pin: byom's producer must still emit the
/// recorded bytes. Change `PARENT_BUDGET_TAG`, the member set, the nested
/// set-binding construction or the canonicalization here, and this fails.
#[test]
fn the_published_fragment_reproduces_the_pinned_family_vector() {
    const VECTOR: &str = include_str!("vectors/parent-budget-fragment.json");
    /// The one constant both repositories name literally. kovee's shipped test
    /// pins the identical value, so a vendored copy that drifts is caught on
    /// whichever side drifted.
    const PINNED_DIGEST: &str = "9ecda50f25f5a1f4da5e264f175c2bfcfade42fc3e9ca3ebdacfc52bcf819398";

    let vector: Value = serde_json::from_str(VECTOR).expect("the pinned family vector parses");
    let inputs = &vector["inputs"];
    let produced = byomd::episode_ops::parent_budget_fragment(
        inputs["byom_budget_reservation_set_ref"].as_str().unwrap(),
        inputs["byom_budget_reservation_set_revision"]
            .as_u64()
            .unwrap(),
        inputs["external_budget_bridge_ref"].as_str().unwrap(),
        inputs["external_budget_bridge_revision"].as_u64().unwrap(),
        inputs["stable_external_reservation_key"].as_str().unwrap(),
        &inputs["items"],
    )
    .expect("byom composes the fragment");
    assert_eq!(
        produced, vector["fragment"],
        "byom's producer no longer emits the recorded fragment: the consumer's \
         pinned copy is now wrong, and re-recording it is a wire change, not a \
         test fix"
    );
    assert_eq!(produced["digest"]["value_hex"], PINNED_DIGEST);
    assert_eq!(vector["domain"], "bpp-parent-budget-fragment-v0");
}
