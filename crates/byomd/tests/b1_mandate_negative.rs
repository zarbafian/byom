//! The B1 mandate-gate negatives (`b1_mandate_negative`): exploration is
//! refused without a mandate, and with a held, revoked, expired,
//! exhausted, or insufficient one — six cases, each with its typed
//! problem, decided by the §11.1 mandate-binding gate at activity_open.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::*;
use serde_json::{json, Value};

struct Rig {
    daemon: TestDaemon,
    incarnation: String,
    agent_token: String,
}

impl Rig {
    fn new(tag: &str) -> Rig {
        let daemon = TestDaemon::start(tag);
        let (_society, _cursor, incarnation) = bootstrap_society(&daemon, tag);
        let (offer_id, cand_token, subject) =
            make_offer(&daemon, &incarnation, tag, "part-agent-1", &far_future());
        let accepted = accept_offer(
            &daemon,
            &incarnation,
            &cand_token,
            tag,
            &offer_id,
            &subject,
            1,
        );
        assert_eq!(accepted["outcome"], "ok", "{accepted}");
        let admitted = daemon.call(
            "governance",
            &json!({
                "version": "0.2", "op": "participant_admit",
                "meta": meta(&incarnation, &format!("{tag}-admit"), Some(2)),
                "offer_ref": offer_id,
                "membership_acceptance_ref": accepted["result"]["acceptance_id"],
                "admitted_by_decision_ref": offer_decision(&offer_id),
                "admission_subject_digest": subject,
            }),
        );
        assert_eq!(admitted["outcome"], "ok", "{admitted}");
        let agent_token = read_participant_token(&daemon, "part-agent-1");
        Rig {
            daemon,
            incarnation,
            agent_token,
        }
    }

    /// Prepares + assents + issues one mandate for the agent; returns
    /// (mandate_id, subject_digest).
    fn issued_mandate(
        &self,
        tag: &str,
        purpose: &str,
        allowed_operations: Value,
        concurrency: u64,
        expires_at: &str,
    ) -> (String, Value) {
        let prepared = self
            .daemon
            .call_raw(
                "participant",
                Some(&self.agent_token),
                &json!({
                    "version": "0.2", "op": "mandate_prepare",
                    "meta": meta(&self.incarnation, &format!("{tag}-mprep"), None),
                    "grantee_participant_ref": "part-agent-1",
                    "purpose_ref": purpose,
                    "allowed_operations": allowed_operations,
                    "resource_selectors": [],
                    "data_class_selectors": [],
                    "destination_selectors": [],
                    "budget_ceiling_set_ref": format!("budget-{tag}"),
                    "concurrency_ceiling": concurrency,
                    "delegation": {"allowed": false, "max_depth": 0,
                                   "max_children": 0, "grantee_selectors": []},
                    "expires_at": expires_at,
                })
                .to_string(),
            )
            .unwrap();
        assert_eq!(prepared["outcome"], "ok", "{tag}: {prepared}");
        let mandate_id = prepared["result"]["mandate_id"]
            .as_str()
            .unwrap()
            .to_owned();
        let seat = prepared["result"]["required_seat_refs"][0]
            .as_str()
            .unwrap()
            .to_owned();
        let digest = prepared["result"]["subject_digest"].clone();
        let positioned = self.daemon.call(
            "governance",
            &json!({
                "version": "0.2", "op": "mandate_position",
                "meta": meta(&self.incarnation, &format!("{tag}-mpos"), None),
                "proposal_ref": mandate_id,
                "proposal_revision": 1,
                "subject_digest": digest,
                "seat_ref": seat,
                "value": "assent",
            }),
        );
        assert_eq!(positioned["outcome"], "ok", "{tag}: {positioned}");
        let issued = self.daemon.call(
            "governance",
            &json!({
                "version": "0.2", "op": "mandate_issue",
                "meta": meta(&self.incarnation, &format!("{tag}-missue"), Some(1)),
                "mandate_id": mandate_id,
                "subject_digest": digest,
            }),
        );
        assert_eq!(issued["outcome"], "ok", "{tag}: {issued}");
        (mandate_id, digest)
    }

    fn explore(&self, tag: &str, purpose: &str, mandate_refs: Value) -> Value {
        self.daemon
            .call_raw(
                "participant",
                Some(&self.agent_token),
                &json!({
                    "version": "0.2", "op": "activity_open",
                    "meta": meta(&self.incarnation, &format!("{tag}-open"), None),
                    "kind": "exploration",
                    "purpose_ref": purpose,
                    "purpose_digest": test_digest(0xc0),
                    "mandate_refs": mandate_refs,
                    "budget_account_set_ref": "budget-any",
                })
                .to_string(),
            )
            .unwrap()
    }
}

/// Case 1 — ABSENT: no mandate at all. §11.1/B1: the mandate chain comes
/// before any non-pledged ActivityStream.
#[test]
fn exploration_without_a_mandate_is_refused() {
    let rig = Rig::new("mn-absent");
    let refused = rig.explore("mn-absent", "purpose-explore-1", json!([]));
    assert_eq!(kind_of(&refused), "forbidden", "{refused}");
    // Citing a mandate that does not exist is non-enumerating not_found.
    let ghost = rig.explore("mn-ghost", "purpose-explore-1", json!(["mnd-none"]));
    assert_eq!(kind_of(&ghost), "not_found", "{ghost}");
}

/// Case 2 — HELD: a held mandate fences new uses with mandate_held.
#[test]
fn exploration_with_a_held_mandate_is_refused() {
    let rig = Rig::new("mn-held");
    let (mandate_id, _) = rig.issued_mandate(
        "mn-held",
        "purpose-explore-1",
        json!(["activity_open"]),
        4,
        &far_future(),
    );
    let held = rig.daemon.call(
        "governance",
        &json!({
            "version": "0.2", "op": "mandate_hold",
            "meta": meta(&rig.incarnation, "mn-held-hold", Some(2)),
            "mandate_id": mandate_id,
            "held_by_decision_ref": mandate_decision(&mandate_id),
        }),
    );
    assert_eq!(held["outcome"], "ok", "{held}");
    let refused = rig.explore("mn-held", "purpose-explore-1", json!([mandate_id]));
    assert_eq!(kind_of(&refused), "mandate_held", "{refused}");
}

/// Case 3 — REVOKED: a revoked mandate is a stale binding, and its
/// reserved budget released.
#[test]
fn exploration_with_a_revoked_mandate_is_refused() {
    let rig = Rig::new("mn-revoked");
    let (mandate_id, _) = rig.issued_mandate(
        "mn-revoked",
        "purpose-explore-1",
        json!(["activity_open"]),
        4,
        &far_future(),
    );
    let revoked = rig.daemon.call(
        "governance",
        &json!({
            "version": "0.2", "op": "mandate_revoke",
            "meta": meta(&rig.incarnation, "mn-revoked-rev", Some(2)),
            "mandate_id": mandate_id,
            "revoked_by_decision_ref": mandate_decision(&mandate_id),
        }),
    );
    assert_eq!(revoked["outcome"], "ok", "{revoked}");
    let refused = rig.explore("mn-revoked", "purpose-explore-1", json!([mandate_id]));
    assert_eq!(kind_of(&refused), "stale_binding", "{refused}");
}

/// Case 4 — EXPIRED: server-time expiry races use through the same head
/// CAS; the expired mandate refuses as a stale binding.
#[test]
fn exploration_with_an_expired_mandate_is_refused() {
    let rig = Rig::new("mn-expired");
    // Expires one second from now; wait past it.
    let soon = bpp_core::time::rfc3339_utc(bpp_core::time::unix_now() + 1);
    let (mandate_id, _) = rig.issued_mandate(
        "mn-expired",
        "purpose-explore-1",
        json!(["activity_open"]),
        4,
        &soon,
    );
    std::thread::sleep(std::time::Duration::from_millis(1500));
    let refused = rig.explore("mn-expired", "purpose-explore-1", json!([mandate_id]));
    assert_eq!(kind_of(&refused), "stale_binding", "{refused}");
    assert!(
        refused["problem"]["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("expired"),
        "{refused}"
    );
}

/// Case 5 — EXHAUSTED: the concurrency ceiling is a use ceiling; the
/// open-streams count at the ceiling refuses budget_exceeded.
#[test]
fn exploration_with_an_exhausted_mandate_is_refused() {
    let rig = Rig::new("mn-exhausted");
    let (mandate_id, _) = rig.issued_mandate(
        "mn-exhausted",
        "purpose-explore-1",
        json!(["activity_open"]),
        1,
        &far_future(),
    );
    let first = rig.explore("mn-exhausted-a", "purpose-explore-1", json!([mandate_id]));
    assert_eq!(first["outcome"], "ok", "{first}");
    let refused = rig.explore("mn-exhausted-b", "purpose-explore-1", json!([mandate_id]));
    assert_eq!(kind_of(&refused), "budget_exceeded", "{refused}");
}

/// Case 6 — INSUFFICIENT: a live mandate whose scope does not cover the
/// use — outside its purpose, or outside its allowed operations.
#[test]
fn exploration_with_an_insufficient_mandate_is_refused() {
    let rig = Rig::new("mn-insufficient");
    let (mandate_id, _) = rig.issued_mandate(
        "mn-insufficient",
        "purpose-explore-1",
        json!(["activity_open"]),
        4,
        &far_future(),
    );
    // Outside the mandate's purpose.
    let wrong_purpose = rig.explore(
        "mn-insufficient-p",
        "purpose-something-else",
        json!([mandate_id]),
    );
    assert_eq!(kind_of(&wrong_purpose), "forbidden", "{wrong_purpose}");
    assert!(
        wrong_purpose["problem"]["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("purpose"),
        "{wrong_purpose}"
    );
    // A mandate whose allowed_operations do not cover activity_open.
    let (narrow_id, _) = rig.issued_mandate(
        "mn-insufficient2",
        "purpose-explore-1",
        json!(["continuation_write"]),
        4,
        &far_future(),
    );
    let wrong_ops = rig.explore("mn-insufficient-o", "purpose-explore-1", json!([narrow_id]));
    assert_eq!(kind_of(&wrong_ops), "forbidden", "{wrong_ops}");
    assert!(
        wrong_ops["problem"]["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("allowed_operations"),
        "{wrong_ops}"
    );
}
