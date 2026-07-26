//! B3 slice 2 — the effect outcome heads (DESIGN.md §13.1/§13.2).
//!
//! Source fact and local consequence are INDEPENDENT axes with separate
//! records and separate heads, and this suite pins that:
//!
//! ```text
//! effect_outcome_admit (runtime)   verified SOURCE facts only, no decision
//! effect_reconcile     (governance) exact GovernanceDecision + fresh challenge
//! ```
//!
//! Both operations lock the EOA head BEFORE the disposition head, so a
//! reconcile that arrives after the source went final is FENCED out of the
//! ambiguous branch and must use the late-source one. Both heads then
//! enter the downstream dependency closure every materializer checks.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::runtime::{merge, portable_digest, Claim, Fixture, Subordinate};
use common::{kind_of, test_digest};
use serde_json::{json, Value};

const INTENT: &str = "intent-outbound-1";
const EXEC_KEY: &str = "execkey-outbound-1";

/// A running Episode holding its lease.
fn running(tag: &str) -> (Fixture, common::runtime::Episode, Claim) {
    let f = Fixture::start(tag, 8);
    let wake = f.wake("w1");
    let ep = f.request_episode(&wake, "e1");
    f.admit_placement(&ep, "p1", Subordinate::Confirmed(200));
    let c = f.claim(&ep.episode_id, "worker-a", 600, 7, "c1");
    let started = f.start_episode(&ep.episode_id, &c, "s1");
    assert_eq!(started["outcome"], "ok", "{started}");
    let lease_revision = started["result"]["lease_revision"].as_u64().unwrap();
    (
        f,
        ep,
        Claim {
            lease_revision,
            ..c
        },
    )
}

fn admit(
    f: &Fixture,
    ep: &common::runtime::Episode,
    c: &Claim,
    key: &str,
    outcome: &str,
    extra: Value,
) -> Value {
    let token = f.worker_token(&ep.episode_id);
    let mut body = json!({
        "version": "0.2", "op": "effect_outcome_admit",
        "meta": f.meta(&format!("eoa-{key}"), None),
        "intent_ref": INTENT,
        "intent_digest": test_digest(0xf1),
        "stable_execution_key": EXEC_KEY,
        "host_protocol": "kovee",
        "host_endpoint_ref": "kovee-endpoint-1",
        "host_effect_ref": format!("kovee-effect-{key}"),
        "host_effect_digest": portable_digest(0xf2),
        "host_receipt_ref": format!("kovee-receipt-{key}"),
        "host_receipt_digest": portable_digest(if outcome == "ambiguous" { 0xf3 } else { 0xf4 }),
        "host_cursor_or_signature_ref": "kovee-sig-1",
        "verification_status": "verified",
        "outcome": outcome,
    });
    merge(&mut body, f.fences(&ep.episode_id, c));
    merge(&mut body, extra);
    f.runtime(&token, &body)
}

fn reconcile(f: &Fixture, key: &str, phase: &str, basis: &Value, extra: Value) -> Value {
    let mut body = json!({
        "version": "0.2", "op": "effect_reconcile",
        "meta": f.meta(&format!("erc-{key}"), None),
        "intent_ref": INTENT,
        "intent_digest": test_digest(0xf1),
        "stable_execution_key": EXEC_KEY,
        "phase": phase,
        "basis_source_admission_ref": basis["admission_id"].clone(),
        "basis_source_admission_revision": basis["revision"].clone(),
        "basis_source_admission_digest": basis["digest"].clone(),
        "local_outcome": "failed",
        "result_use": "unavailable",
        "fresh_challenge_ref": format!("challenge-{key}"),
    });
    merge(&mut body, extra);
    f.governance(&body)
}

#[test]
fn the_eoa_lands_before_the_disposition_and_both_heads_enter_the_closure() {
    let (f, ep, c) = running("b3-eff-order");

    // 1. The narrow runtime adapter admits the AMBIGUOUS source fact.
    let admitted = admit(&f, &ep, &c, "amb", "ambiguous", json!({}));
    assert_eq!(admitted["outcome"], "ok", "{admitted}");
    let basis = admitted["result"].clone();
    assert_eq!(basis["revision"], 1);
    assert_eq!(basis["outcome"], "ambiguous");
    assert_eq!(
        basis["lock_order"],
        json!([
            "effect_outcome_admission_head",
            "effect_governance_disposition_head"
        ]),
        "both operations lock the EOA head BEFORE the disposition head"
    );
    // The source path has NO GovernanceDecision at all.
    assert_eq!(
        f.count("SELECT COUNT(*) FROM governance_decisions WHERE kind = 'effect_reconciliation'"),
        0,
        "effect_outcome_admit accepts source evidence only (§13.2 path 1)"
    );
    let closure = &basis["dependency_closure"];
    assert_eq!(
        closure["effect_outcome_admission_heads"][0]["current_outcome"],
        "ambiguous"
    );
    assert_eq!(
        closure["effect_governance_disposition_heads"],
        json!([]),
        "no disposition head yet — but the member is always in the closure"
    );

    // 2. The governance seat records the LOCAL consequence, and moves
    //    only its OWN head.
    let disposed = reconcile(
        &f,
        "amb",
        "ambiguous_source",
        &basis,
        json!({"late_source_policy": "quarantine_and_redecide"}),
    );
    assert_eq!(disposed["outcome"], "ok", "{disposed}");
    assert_eq!(disposed["result"]["phase"], "ambiguous_source");
    assert_eq!(disposed["result"]["result_use"], "unavailable");
    assert_eq!(
        disposed["result"]["disposition_head_state"],
        "active_ambiguous"
    );
    assert_eq!(
        disposed["result"]["source_head_unchanged"]["current_outcome"], "ambiguous",
        "the disposition never advances the EOA head or the source state"
    );
    assert_eq!(
        disposed["result"]["source_head_unchanged"]["current_admission_ref"],
        basis["admission_id"]
    );
    // Both heads are now in the downstream closure.
    let closure = &disposed["result"]["dependency_closure"];
    assert_eq!(
        closure["effect_outcome_admission_heads"][0]["current_outcome"],
        "ambiguous"
    );
    assert_eq!(
        closure["effect_governance_disposition_heads"][0]["state"],
        "active_ambiguous"
    );
    assert_eq!(
        closure["lock_order"],
        json!([
            "effect_outcome_admission_head",
            "effect_governance_disposition_head"
        ])
    );
    // The decision is immutable and exactly one per disposition.
    let decision = disposed["result"]["governance_decision_ref"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        f.row(
            "SELECT kind FROM governance_decisions WHERE decision_id = ?1",
            &decision
        )
        .as_deref(),
        Some("effect_reconciliation")
    );
}

#[test]
fn a_source_advance_fences_the_active_disposition_and_forces_late_source() {
    let (f, ep, c) = running("b3-eff-late");
    let ambiguous = admit(&f, &ep, &c, "amb", "ambiguous", json!({}));
    let basis = ambiguous["result"].clone();
    let disposed = reconcile(
        &f,
        "amb",
        "ambiguous_source",
        &basis,
        json!({"late_source_policy": "quarantine_and_redecide"}),
    );
    assert_eq!(disposed["outcome"], "ok", "{disposed}");

    // Kovee CAS-commits and signs its own final successor; byom admits
    // it, cites the exact ambiguous admission, and in the SAME
    // transaction marks the active disposition head source_advanced.
    let final_source = admit(
        &f,
        &ep,
        &c,
        "fin",
        "succeeded",
        json!({
            "result_ref": "kovee-result-1",
            "result_digest": test_digest(0xf5),
            "reconciles_admission_ref": basis["admission_id"].clone(),
            "reconciles_admission_digest": basis["digest"].clone(),
        }),
    );
    assert_eq!(final_source["outcome"], "ok", "{final_source}");
    assert_eq!(final_source["result"]["revision"], 2);
    assert_eq!(final_source["result"]["outcome"], "succeeded");
    let closure = &final_source["result"]["dependency_closure"];
    assert_eq!(
        closure["effect_governance_disposition_heads"][0]["state"], "source_advanced",
        "the verified result's use is QUARANTINED while the head is \
         source_advanced (§13.2)"
    );

    // A second ambiguous_source disposition is now FENCED: the current
    // source outcome is final, so only the late-source branch remains.
    let fenced = reconcile(
        &f,
        "amb2",
        "ambiguous_source",
        &final_source["result"],
        json!({"late_source_policy": "quarantine_and_redecide"}),
    );
    assert_eq!(kind_of(&fenced), "stale_revision", "{fenced}");
    assert!(
        fenced["problem"]["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("late_source"),
        "{fenced}"
    );

    // The late-source branch needs a FRESH decision and the exact final
    // source admission; it may release the verified, locally classified
    // result and moves only the disposition head to resolved_late.
    let late = reconcile(
        &f,
        "late",
        "late_source",
        &final_source["result"],
        json!({
            "local_outcome": "succeeded",
            "result_use": "released",
            "classification_admission_ref": "class-adm-1",
            "classification_admission_digest": test_digest(0xf6),
        }),
    );
    assert_eq!(late["outcome"], "ok", "{late}");
    assert_eq!(late["result"]["phase"], "late_source");
    assert_eq!(late["result"]["revision"], 2);
    assert_eq!(late["result"]["disposition_head_state"], "resolved_late");
    assert_eq!(late["result"]["result_use"], "released");
    assert_eq!(
        late["result"]["source_head_unchanged"]["current_admission_revision"], 2,
        "it may revise the local outcome but never the source outcome"
    );
    assert_eq!(
        f.count("SELECT COUNT(*) FROM effect_outcome_admissions"),
        2,
        "the source axis moved exactly twice, by effect_outcome_admit alone"
    );
    assert_eq!(
        f.count("SELECT COUNT(*) FROM effect_governance_dispositions"),
        2
    );
}

#[test]
fn the_source_axis_never_returns_to_ambiguous_and_needs_the_exact_predecessor() {
    let (f, ep, c) = running("b3-eff-source");
    let ambiguous = admit(&f, &ep, &c, "amb", "ambiguous", json!({}));
    let basis = ambiguous["result"].clone();

    // An ambiguous-to-final path that cites nothing is refused.
    let uncited = admit(&f, &ep, &c, "nocite", "failed", json!({}));
    assert_eq!(kind_of(&uncited), "stale_revision", "{uncited}");
    assert!(
        uncited["problem"]["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("reconciles_admission"),
        "{uncited}"
    );

    // The final successor lands...
    let final_source = admit(
        &f,
        &ep,
        &c,
        "fin",
        "failed",
        json!({
            "reconciles_admission_ref": basis["admission_id"].clone(),
            "reconciles_admission_digest": basis["digest"].clone(),
        }),
    );
    assert_eq!(final_source["outcome"], "ok", "{final_source}");
    // ...and nothing returns the source axis to ambiguous.
    let back = admit(&f, &ep, &c, "back", "ambiguous", json!({}));
    assert_eq!(kind_of(&back), "stale_revision", "{back}");
    assert!(
        back["problem"]["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("never returns to ambiguous"),
        "{back}"
    );
}

#[test]
fn the_effect_admission_presents_both_fences_like_every_runtime_mutation() {
    let (f, ep, c) = running("b3-eff-fences");
    let token = f.worker_token(&ep.episode_id);
    let mut body = json!({
        "version": "0.2", "op": "effect_outcome_admit",
        "meta": f.meta("eoa-stale", None),
        "intent_ref": INTENT, "intent_digest": test_digest(0xf1),
        "stable_execution_key": EXEC_KEY,
        "host_protocol": "kovee", "host_endpoint_ref": "kovee-endpoint-1",
        "host_effect_ref": "kovee-effect-1",
        "host_effect_digest": portable_digest(0xf2),
        "host_receipt_ref": "kovee-receipt-1",
        "host_receipt_digest": portable_digest(0xf3),
        "host_cursor_or_signature_ref": "kovee-sig-1",
        "verification_status": "verified", "outcome": "ambiguous",
        "episode_ref": ep.episode_id, "generation": 1,
        "byom_attempt_ref": c.attempt_ref,
        "byom_fence_epoch": c.byom_fence_epoch,
        "kovee_invocation_fence": c.kovee_invocation_fence + 1,
    });
    let refused = f.runtime(&token, &body);
    assert_eq!(kind_of(&refused), "stale_lease", "{refused}");
    merge(
        &mut body,
        json!({"kovee_invocation_fence": c.kovee_invocation_fence,
               "byom_fence_epoch": c.byom_fence_epoch + 1,
               "meta": f.meta("eoa-stale-2", None)}),
    );
    let refused = f.runtime(&token, &body);
    assert_eq!(kind_of(&refused), "stale_lease", "{refused}");
}

#[test]
fn a_different_host_receipt_cannot_reuse_the_source_uniqueness_key() {
    let (f, ep, c) = running("b3-eff-unique");
    let first = admit(&f, &ep, &c, "amb", "ambiguous", json!({}));
    assert_eq!(first["outcome"], "ok", "{first}");
    // The exact replay returns the same record.
    let replay = admit(&f, &ep, &c, "amb", "ambiguous", json!({}));
    assert_eq!(replay, first, "exact replay returns the same record");
    // The same host effect/receipt digest under ANOTHER execution key is
    // refused (§13.2 UNIQUE(host_endpoint_ref, host_effect_ref,
    // host_receipt_digest)).
    let token = f.worker_token(&ep.episode_id);
    let mut body = json!({
        "version": "0.2", "op": "effect_outcome_admit",
        "meta": f.meta("eoa-clash", None),
        "intent_ref": INTENT, "intent_digest": test_digest(0xf1),
        "stable_execution_key": "execkey-other",
        "host_protocol": "kovee", "host_endpoint_ref": "kovee-endpoint-1",
        "host_effect_ref": "kovee-effect-amb",
        "host_effect_digest": portable_digest(0xf2),
        "host_receipt_ref": "kovee-receipt-amb",
        "host_receipt_digest": portable_digest(0xf3),
        "host_cursor_or_signature_ref": "kovee-sig-1",
        "verification_status": "verified", "outcome": "ambiguous",
    });
    merge(&mut body, f.fences(&ep.episode_id, &c));
    let clash = f.runtime(&token, &body);
    assert_eq!(kind_of(&clash), "stale_binding", "{clash}");
}

#[test]
fn a_second_disposition_needs_a_fresh_challenge() {
    let (f, ep, c) = running("b3-eff-challenge");
    let basis = admit(&f, &ep, &c, "amb", "ambiguous", json!({}))["result"].clone();
    let first = reconcile(
        &f,
        "same",
        "ambiguous_source",
        &basis,
        json!({"late_source_policy": "quarantine_and_redecide"}),
    );
    assert_eq!(first["outcome"], "ok", "{first}");
    // Same challenge, fresh request id: the derived GovernanceDecision
    // already decided a disposition.
    let again = f.governance(&json!({
        "version": "0.2", "op": "effect_reconcile",
        "meta": f.meta("erc-same-2", None),
        "intent_ref": INTENT, "intent_digest": test_digest(0xf1),
        "stable_execution_key": EXEC_KEY,
        "phase": "ambiguous_source",
        "basis_source_admission_ref": basis["admission_id"],
        "basis_source_admission_revision": basis["revision"],
        "basis_source_admission_digest": basis["digest"],
        "local_outcome": "failed", "result_use": "unavailable",
        "fresh_challenge_ref": "challenge-same",
        "late_source_policy": "quarantine_and_redecide",
    }));
    assert_eq!(kind_of(&again), "decision_incomplete", "{again}");
}

#[test]
fn the_reconciliation_seat_is_governance_only() {
    let (f, ep, c) = running("b3-eff-surface");
    let basis = admit(&f, &ep, &c, "amb", "ambiguous", json!({}))["result"].clone();
    // The runtime workload cannot reach the reconciliation seat, and the
    // participant cannot either: the registry rows decide.
    let body = json!({
        "version": "0.2", "op": "effect_reconcile",
        "meta": f.meta("erc-runtime", None),
        "intent_ref": INTENT, "intent_digest": test_digest(0xf1),
        "stable_execution_key": EXEC_KEY,
        "phase": "ambiguous_source",
        "basis_source_admission_ref": basis["admission_id"],
        "basis_source_admission_revision": basis["revision"],
        "basis_source_admission_digest": basis["digest"],
        "local_outcome": "failed", "result_use": "unavailable",
        "fresh_challenge_ref": "challenge-x",
        "late_source_policy": "quarantine_and_redecide",
    });
    let from_runtime = f.runtime(&f.worker_token(&ep.episode_id), &body);
    assert_eq!(
        kind_of(&from_runtime),
        "forbidden_surface",
        "{from_runtime}"
    );
    let from_participant = f.participant(&body);
    assert_eq!(
        kind_of(&from_participant),
        "forbidden_surface",
        "{from_participant}"
    );
    // And the runtime source path is not reachable from governance.
    let source_from_gov = f.governance(&json!({
        "version": "0.2", "op": "effect_outcome_admit",
        "meta": f.meta("eoa-gov", None),
        "intent_ref": INTENT, "intent_digest": test_digest(0xf1),
        "stable_execution_key": EXEC_KEY,
        "host_protocol": "kovee", "host_endpoint_ref": "kovee-endpoint-1",
        "host_effect_ref": "kovee-effect-g",
        "host_effect_digest": portable_digest(0xf2),
        "host_receipt_ref": "kovee-receipt-g",
        "host_receipt_digest": portable_digest(0xf7),
        "host_cursor_or_signature_ref": "kovee-sig-1",
        "verification_status": "verified", "outcome": "succeeded",
        "episode_ref": ep.episode_id, "generation": 1,
        "byom_attempt_ref": c.attempt_ref,
        "byom_fence_epoch": c.byom_fence_epoch,
        "kovee_invocation_fence": c.kovee_invocation_fence,
    }));
    assert_eq!(
        kind_of(&source_from_gov),
        "forbidden_surface",
        "{source_from_gov}"
    );
}
