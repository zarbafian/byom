//! The C3a negative core, server-side (§7.4; registry G35 rows):
//! nothing but the candidate accepts, nothing but governance admits,
//! the exact refusal retry returns its retained receipt, terminal
//! offers never admit, expiry races admission through the same CAS, and
//! self-policy adoption can never enter through governance or runtime.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::*;
use serde_json::json;

#[test]
fn nothing_but_the_candidate_accepts() {
    let daemon = TestDaemon::start("neg-accept");
    let (_sid, _cursor, incarnation) = bootstrap_society(&daemon, "na");
    let (offer_id, token, subject) =
        make_offer(&daemon, &incarnation, "na", "part-agent-1", &far_future());

    let accept = json!({
        "version": "0.2", "op": "membership_accept",
        "meta": meta(&incarnation, "na-accept", Some(1)),
        "offer_ref": offer_id,
        "subject_digest": subject,
    });
    // The governance surface has NO membership_accept row: the registry
    // decides, deny by absence (B1 sheet: forbidden-surface).
    let via_gov = daemon.call("governance", &accept);
    assert_eq!(kind_of(&via_gov), "forbidden_surface", "{via_gov}");
    // Nor participant or projection.
    let via_part = daemon.call("participant", &accept);
    assert_eq!(kind_of(&via_part), "forbidden_surface");
    let via_proj = daemon.call("projection", &accept);
    assert_eq!(kind_of(&via_proj), "forbidden_surface");

    // The candidate surface without the channel token: forbidden,
    // non-enumerating.
    let no_token = daemon.call_candidate("", &accept);
    assert_eq!(kind_of(&no_token), "forbidden");
    let bad_token = daemon.call_candidate("cand-token-forged", &accept);
    assert_eq!(kind_of(&bad_token), "forbidden");

    // A candidate credential is offer-scoped: a second offer's token
    // cannot act on the first offer.
    let (_offer2, token2, _subject2) =
        make_offer(&daemon, &incarnation, "na2", "part-agent-2", &far_future());
    let cross = daemon.call_candidate(&token2, &accept);
    assert_eq!(kind_of(&cross), "forbidden", "{cross}");

    // The right channel accepts.
    let ok = daemon.call_candidate(&token, &accept);
    assert_eq!(ok["outcome"], "ok", "{ok}");
}

#[test]
fn nothing_but_governance_admits() {
    let daemon = TestDaemon::start("neg-admit");
    let (_sid, _cursor, incarnation) = bootstrap_society(&daemon, "ng");
    let (offer_id, token, subject) =
        make_offer(&daemon, &incarnation, "ng", "part-agent-1", &far_future());
    let accepted = accept_offer(&daemon, &incarnation, &token, "ng", &offer_id, &subject, 1);
    let acceptance_id = accepted["result"]["acceptance_id"].as_str().unwrap();

    let admit = json!({
        "version": "0.2", "op": "participant_admit",
        "meta": meta(&incarnation, "ng-admit", Some(2)),
        "offer_ref": offer_id,
        "membership_acceptance_ref": acceptance_id,
        "admitted_by_decision_ref": offer_decision(&offer_id),
        "admission_subject_digest": subject,
    });
    // participant_admit has ONLY a governance row.
    let via_candidate = daemon.call_candidate(&token, &admit);
    assert_eq!(
        kind_of(&via_candidate),
        "forbidden_surface",
        "{via_candidate}"
    );
    let via_participant = daemon.call("participant", &admit);
    assert_eq!(kind_of(&via_participant), "forbidden_surface");
    let via_projection = daemon.call("projection", &admit);
    assert_eq!(kind_of(&via_projection), "forbidden_surface");

    // Runtime output cannot cross the candidate surface either:
    // manifestation_admit is governance-only too.
    let manif_admit = json!({
        "version": "0.2", "op": "manifestation_admit",
        "meta": meta(&incarnation, "ng-manif", Some(1)),
        "manifestation_ref": "manif-x",
        "admitted_by_decision_ref": "dec-2",
    });
    assert_eq!(
        kind_of(&daemon.call_candidate(&token, &manif_admit)),
        "forbidden_surface"
    );

    // Governance admits.
    let ok = daemon.call("governance", &admit);
    assert_eq!(ok["outcome"], "ok", "{ok}");
}

#[test]
fn neither_governance_nor_runtime_installs_self_policy() {
    // B1 sheet: assent_policy_adopt / activation_policy_adopt on the
    // governance (or any non-participant) surface → forbidden-surface,
    // decided purely by the registry rows.
    let daemon = TestDaemon::start("neg-policy");
    let (_sid, _cursor, incarnation) = bootstrap_society(&daemon, "np");
    for op in ["assent_policy_adopt", "activation_policy_adopt"] {
        let request = json!({
            "version": "0.2", "op": op,
            "meta": meta(&incarnation, &format!("np-{op}"), None),
        });
        for surface in ["governance", "projection"] {
            let reply = daemon.call(surface, &request);
            assert_eq!(
                kind_of(&reply),
                "forbidden_surface",
                "{op} on {surface}: {reply}"
            );
        }
        let via_candidate = daemon.call_candidate("", &request);
        assert_eq!(kind_of(&via_candidate), "forbidden_surface");
    }
    // An operation absent from the bundle entirely is feature_unavailable.
    let unknown = daemon.call(
        "governance",
        &json!({"version": "0.2", "op": "not_an_operation"}),
    );
    assert_eq!(kind_of(&unknown), "feature_unavailable");
}

#[test]
fn refusal_retry_returns_the_retained_receipt() {
    let daemon = TestDaemon::start("neg-refuse");
    let (_sid, _cursor, incarnation) = bootstrap_society(&daemon, "nr");
    let (offer_id, token, subject) =
        make_offer(&daemon, &incarnation, "nr", "part-agent-1", &far_future());
    // Accept first, then retract by refusal citing the acceptance
    // (§7.4 accepted → refused).
    let accepted = accept_offer(&daemon, &incarnation, &token, "nr", &offer_id, &subject, 1);
    let acceptance_id = accepted["result"]["acceptance_id"]
        .as_str()
        .unwrap()
        .to_owned();

    let refuse = json!({
        "version": "0.2", "op": "membership_refuse",
        "meta": meta(&incarnation, "nr-refuse", Some(2)),
        "offer_ref": offer_id,
        "offer_subject_digest": subject,
        "superseded_acceptance_ref": acceptance_id,
        "refusal_reason_ref": "reason-1",
    });
    let refused = daemon.call_candidate(&token, &refuse);
    assert_eq!(refused["outcome"], "ok", "{refused}");
    assert_eq!(refused["result"]["offer_state"], "refused");
    assert_eq!(
        refused["result"]["superseded_acceptance_ref"],
        acceptance_id
    );

    // The channel is terminally fenced...
    let token_file = daemon
        .data_dir
        .join("channels")
        .join(format!("candidate-{offer_id}.token"));
    assert!(!token_file.exists(), "refusal closes the candidate channel");

    // ...yet the EXACT refusal retry returns the same receipt,
    // byte-identically (§14.8 crash column).
    let retried = daemon.call_candidate(&token, &refuse);
    assert_eq!(retried, refused, "exact retry returns the retained receipt");

    // Any OTHER use of the fenced credential is forbidden.
    let late_accept = daemon.call_candidate(
        &token,
        &json!({
            "version": "0.2", "op": "membership_accept",
            "meta": meta(&incarnation, "nr-late", Some(3)),
            "offer_ref": offer_id,
            "subject_digest": subject,
        }),
    );
    assert_eq!(kind_of(&late_accept), "forbidden", "{late_accept}");
}

#[test]
fn terminal_offers_never_admit() {
    let daemon = TestDaemon::start("neg-terminal");
    let (_sid, _cursor, incarnation) = bootstrap_society(&daemon, "nt");
    let (offer_id, token, subject) =
        make_offer(&daemon, &incarnation, "nt", "part-agent-1", &far_future());
    let accepted = accept_offer(&daemon, &incarnation, &token, "nt", &offer_id, &subject, 1);
    let acceptance_id = accepted["result"]["acceptance_id"]
        .as_str()
        .unwrap()
        .to_owned();

    // Refusal retracts the acceptance and races admission on the SAME
    // offer revision — the refusal wins here.
    let refused = daemon.call_candidate(
        &token,
        &json!({
            "version": "0.2", "op": "membership_refuse",
            "meta": meta(&incarnation, "nt-refuse", Some(2)),
            "offer_ref": offer_id,
            "offer_subject_digest": subject,
            "superseded_acceptance_ref": acceptance_id,
        }),
    );
    assert_eq!(refused["outcome"], "ok");

    // Admission citing the pre-refusal revision loses the CAS.
    let stale_admit = daemon.call(
        "governance",
        &json!({
            "version": "0.2", "op": "participant_admit",
            "meta": meta(&incarnation, "nt-admit", Some(2)),
            "offer_ref": offer_id,
            "membership_acceptance_ref": acceptance_id,
            "admitted_by_decision_ref": "dec-1",
            "admission_subject_digest": subject,
        }),
    );
    assert_eq!(kind_of(&stale_admit), "stale_revision", "{stale_admit}");

    // Even citing the CURRENT revision, a terminal offer never admits.
    let current_admit = daemon.call(
        "governance",
        &json!({
            "version": "0.2", "op": "participant_admit",
            "meta": meta(&incarnation, "nt-admit2", Some(3)),
            "offer_ref": offer_id,
            "membership_acceptance_ref": acceptance_id,
            "admitted_by_decision_ref": "dec-1",
            "admission_subject_digest": subject,
        }),
    );
    assert_eq!(kind_of(&current_admit), "stale_binding", "{current_admit}");

    // The refused candidate never appears in the projection.
    let shown = daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "participant_show", "participant_ref": "part-agent-1"}),
    );
    assert_eq!(kind_of(&shown), "not_found");
}

#[test]
fn stale_acceptance_expires_and_expiry_fences_admission() {
    let daemon = TestDaemon::start("neg-expiry");
    let (_sid, _cursor, incarnation) = bootstrap_society(&daemon, "ne");
    // A short-lived offer; once its expires_at lapses, the
    // deterministic server-time transition fires on first touch.
    let soon = bpp_core::time::rfc3339_utc(bpp_core::time::unix_now() + 1);
    let (offer_id, token, subject) = make_offer(&daemon, &incarnation, "ne", "part-agent-1", &soon);
    std::thread::sleep(std::time::Duration::from_millis(2100));
    let accept = accept_offer(&daemon, &incarnation, &token, "ne", &offer_id, &subject, 1);
    // The channel was fenced by expiry before the acceptance could land.
    assert_eq!(kind_of(&accept), "forbidden", "{accept}");

    // Silence expires: admission against the expired offer is terminal.
    let admit = daemon.call(
        "governance",
        &json!({
            "version": "0.2", "op": "participant_admit",
            "meta": meta(&incarnation, "ne-admit", Some(2)),
            "offer_ref": offer_id,
            "membership_acceptance_ref": "acc-never-existed",
            "admitted_by_decision_ref": "dec-1",
            "admission_subject_digest": subject,
        }),
    );
    assert_eq!(kind_of(&admit), "stale_binding", "{admit}");
}

#[test]
fn candidate_subject_and_revision_discipline() {
    let daemon = TestDaemon::start("neg-subject");
    let (_sid, _cursor, incarnation) = bootstrap_society(&daemon, "ns");
    let (offer_id, token, subject) =
        make_offer(&daemon, &incarnation, "ns", "part-agent-1", &far_future());

    // Acceptance must commit to the EXACT offer subject.
    let wrong_subject = daemon.call_candidate(
        &token,
        &json!({
            "version": "0.2", "op": "membership_accept",
            "meta": meta(&incarnation, "ns-a1", Some(1)),
            "offer_ref": offer_id,
            "subject_digest": test_digest(0xee),
        }),
    );
    assert_eq!(kind_of(&wrong_subject), "invalid", "{wrong_subject}");

    // Update CAS: a stale expected_revision is refused.
    let stale = accept_offer(
        &daemon,
        &incarnation,
        &token,
        "ns-a2",
        &offer_id,
        &subject,
        7,
    );
    assert_eq!(kind_of(&stale), "stale_revision");

    // Reads never carry meta; mutations always do (closed schemas).
    let read_with_meta = daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "society_show", "society_id": "soc-1",
                "meta": meta(&incarnation, "ns-m", None)}),
    );
    assert_eq!(kind_of(&read_with_meta), "invalid");
    let mutation_without_meta = daemon.call(
        "governance",
        &json!({"version": "0.2", "op": "membership_offer",
                "participant_ref": "p", "proposed_standing_ref": "s",
                "subject_digest": test_digest(1), "offered_by_decision_ref": "d",
                "expires_at": far_future()}),
    );
    assert_eq!(kind_of(&mutation_without_meta), "invalid");

    // An acceptance-shaped acceptance still works after the negatives:
    // nothing above mutated the offer.
    let ok = accept_offer(
        &daemon,
        &incarnation,
        &token,
        "ns-ok",
        &offer_id,
        &subject,
        1,
    );
    assert_eq!(ok["outcome"], "ok", "{ok}");
}

#[test]
fn accepted_then_refused_retraction_and_same_revision_race() {
    let daemon = TestDaemon::start("neg-race");
    let (_sid, _cursor, incarnation) = bootstrap_society(&daemon, "nx");
    let (offer_id, token, subject) =
        make_offer(&daemon, &incarnation, "nx", "part-agent-1", &far_future());
    let accepted = accept_offer(&daemon, &incarnation, &token, "nx", &offer_id, &subject, 1);
    let acceptance_id = accepted["result"]["acceptance_id"]
        .as_str()
        .unwrap()
        .to_owned();

    // Refusal citing a WRONG superseded acceptance is refused.
    let wrong_cite = daemon.call_candidate(
        &token,
        &json!({
            "version": "0.2", "op": "membership_refuse",
            "meta": meta(&incarnation, "nx-r1", Some(2)),
            "offer_ref": offer_id,
            "offer_subject_digest": subject,
            "superseded_acceptance_ref": "acc-other",
        }),
    );
    assert_eq!(kind_of(&wrong_cite), "stale_binding", "{wrong_cite}");

    // Admission wins the same-revision race; the refusal then loses the
    // CAS (admission and retraction cannot both win).
    let admitted = daemon.call(
        "governance",
        &json!({
            "version": "0.2", "op": "participant_admit",
            "meta": meta(&incarnation, "nx-admit", Some(2)),
            "offer_ref": offer_id,
            "membership_acceptance_ref": acceptance_id,
            "admitted_by_decision_ref": offer_decision(&offer_id),
            "admission_subject_digest": subject,
        }),
    );
    assert_eq!(admitted["outcome"], "ok");
    let late_refuse = daemon.call_candidate(
        &token,
        &json!({
            "version": "0.2", "op": "membership_refuse",
            "meta": meta(&incarnation, "nx-r2", Some(2)),
            "offer_ref": offer_id,
            "offer_subject_digest": subject,
            "superseded_acceptance_ref": acceptance_id,
        }),
    );
    // The channel was converted at admission: the fenced credential
    // cannot author a post-admission refusal.
    assert_eq!(kind_of(&late_refuse), "forbidden", "{late_refuse}");
}

#[test]
fn post_admission_credential_fencing() {
    let daemon = TestDaemon::start("neg-fence");
    let (_sid, _cursor, incarnation) = bootstrap_society(&daemon, "nf");
    let (offer_id, token, subject) =
        make_offer(&daemon, &incarnation, "nf", "part-agent-1", &far_future());
    let accepted = accept_offer(&daemon, &incarnation, &token, "nf", &offer_id, &subject, 1);
    let acceptance_id = accepted["result"]["acceptance_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let admitted = daemon.call(
        "governance",
        &json!({
            "version": "0.2", "op": "participant_admit",
            "meta": meta(&incarnation, "nf-admit", Some(2)),
            "offer_ref": offer_id,
            "membership_acceptance_ref": acceptance_id,
            "admitted_by_decision_ref": offer_decision(&offer_id),
            "admission_subject_digest": subject,
        }),
    );
    assert_eq!(admitted["outcome"], "ok");
    // Every candidate-channel use after conversion is forbidden — accept
    // and fresh refuse alike (only exact terminal-receipt replay would
    // answer, and admission minted none for the candidate).
    let accept_again = accept_offer(
        &daemon,
        &incarnation,
        &token,
        "nf-again",
        &offer_id,
        &subject,
        3,
    );
    assert_eq!(kind_of(&accept_again), "forbidden");
}

#[test]
fn candidate_self_policy_activates_at_admission_exactly_as_authored() {
    let mut daemon = TestDaemon::start("neg-selfpol");
    let (_sid, _cursor, incarnation) = bootstrap_society(&daemon, "sp");
    let (offer_id, token, subject) =
        make_offer(&daemon, &incarnation, "sp", "part-agent-1", &far_future());

    // The candidate authors TWO proposals over its own channel; the
    // bodies are its exact words.
    let assent_body = json!({"rules": [{"effect": "allow",
        "atoms": {"operation": {"ids": ["pledge_position"]}}}]});
    let propose = |key: &str, kind: &str, body: &serde_json::Value| {
        daemon.call_candidate(
            &token,
            &json!({
                "version": "0.2", "op": "candidate_self_policy_propose",
                "meta": meta(&incarnation, key, None),
                "onboarding_ref": offer_id,
                "proposed_policy_kind": kind,
                "proposed_policy_body": body,
                "proposed_policy_digest": test_digest(0xe0),
                "adoption_mode": "direct_candidate",
                "adoption_control_domain_ref": "control-domain-1",
            }),
        )
    };
    let p1 = propose("sp-p1", "assent", &assent_body);
    assert_eq!(p1["outcome"], "ok", "{p1}");
    let p1_id = p1["result"]["proposal_id"].as_str().unwrap().to_owned();
    let p2 = propose("sp-p2", "activation", &bpa1_allow_all());
    assert_eq!(p2["outcome"], "ok", "{p2}");

    // NEVER BEFORE STANDING: no self-policy exists before admission.
    daemon.stop();
    {
        let store = byom_store::Store::open(&daemon.data_dir).unwrap();
        let n: i64 = store
            .conn()
            .query_row("SELECT COUNT(*) FROM self_policies", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0, "no self-policy before Standing");
    }
    daemon.restart(&[]);

    let accepted = accept_offer(&daemon, &incarnation, &token, "sp", &offer_id, &subject, 1);
    assert_eq!(accepted["outcome"], "ok", "{accepted}");
    let acceptance_id = accepted["result"]["acceptance_id"].as_str().unwrap();

    // Citing a proposal that does not exist is citing a record that
    // does not exist (non-enumerating not_found; nothing activates).
    let ghost = daemon.call(
        "governance",
        &json!({
            "version": "0.2", "op": "participant_admit",
            "meta": meta(&incarnation, "sp-admit-ghost", Some(2)),
            "offer_ref": offer_id,
            "membership_acceptance_ref": acceptance_id,
            "admitted_by_decision_ref": offer_decision(&offer_id),
            "admission_subject_digest": subject,
            "included_self_policy_proposal_refs": ["candpol-none"],
        }),
    );
    assert_eq!(kind_of(&ghost), "not_found", "{ghost}");

    // Admission citing ONLY the assent proposal activates exactly that
    // one, exactly as authored.
    let admitted = daemon.call(
        "governance",
        &json!({
            "version": "0.2", "op": "participant_admit",
            "meta": meta(&incarnation, "sp-admit", Some(2)),
            "offer_ref": offer_id,
            "membership_acceptance_ref": acceptance_id,
            "admitted_by_decision_ref": offer_decision(&offer_id),
            "admission_subject_digest": subject,
            "included_self_policy_proposal_refs": [p1_id],
        }),
    );
    assert_eq!(admitted["outcome"], "ok", "{admitted}");
    let activated = admitted["result"]["activated_self_policy_refs"]
        .as_array()
        .unwrap();
    assert_eq!(activated.len(), 1, "{admitted}");

    daemon.stop();
    let store = byom_store::Store::open(&daemon.data_dir).unwrap();
    let rows: Vec<(String, String, String, String, String)> = {
        let mut stmt = store
            .conn()
            .prepare(
                "SELECT kind, status, body, provenance, adoption_mode
                 FROM self_policies WHERE participant_ref = 'part-agent-1'",
            )
            .unwrap();
        let out = stmt
            .query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        out
    };
    assert_eq!(rows.len(), 1, "only the CITED proposal activated: {rows:?}");
    let (kind, status, body, provenance, mode) = &rows[0];
    assert_eq!(kind, "assent");
    assert_eq!(status, "active");
    assert_eq!(provenance, "candidate_authored");
    assert_eq!(mode, "direct_candidate");
    // EXACTLY as authored: the retained body is the candidate's words.
    let stored: serde_json::Value = serde_json::from_str(body).unwrap();
    assert_eq!(stored, assent_body, "policy body activated verbatim");
    // The uncited activation proposal stayed un-activated.
    let unactivated: i64 = store
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM candidate_policy_proposals WHERE state = 'proposed'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(unactivated, 1, "the uncited proposal remains proposed");
}
