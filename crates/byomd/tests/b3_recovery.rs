//! B3 slice 1 — the recovery surface (DESIGN.md §16.3; registry R42 and
//! R40) against the real daemon.
//!
//! Each of the five facts is produced under its EXACT precondition, and
//! every status-specific field set is checked closed both ways — the
//! required fields present, the forbidden fields absent. Then
//! terminalization's closed three-way result, its four blocking states,
//! and the §16.3 race: a delayed command and a terminalization cannot
//! both win.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::kovee::{self, Bend, Seam};
use common::*;
use serde_json::{json, Value};

/// The status-specific field sets of the five-fact union (§16.3, closed).
const RESULT_FIELDS: [&str; 8] = [
    "committed_result_envelope",
    "committed_result_digest",
    "committed_result_signature",
    "tombstone_ref",
    "tombstone_digest",
    "tombstone_reason",
    "historical_fence_receipt_ref",
    "historical_fence_receipt_digest",
];
const LINEAGE_FIELDS: [&str; 2] = [
    "restore_lineage_evidence_ref",
    "restore_lineage_evidence_digest",
];

fn required_for(status: &str) -> Vec<&'static str> {
    match status {
        "committed" => vec![
            "committed_result_envelope",
            "committed_result_digest",
            "committed_result_signature",
        ],
        "non_reexecuting_tombstone" => {
            vec!["tombstone_ref", "tombstone_digest", "tombstone_reason"]
        }
        "historically_fenced_absent" => vec![
            "restore_lineage_evidence_ref",
            "restore_lineage_evidence_digest",
            "historical_fence_receipt_ref",
            "historical_fence_receipt_digest",
        ],
        _ => vec![],
    }
}

/// Asserts the closed field discipline of one five-fact answer.
fn assert_closed(result: &Value, status: &str) {
    assert_eq!(result["status"], json!(status), "{result}");
    for base in [
        "query_digest",
        "current_endpoint_incarnation",
        "target_endpoint_incarnation",
        "idempotency_domain_digest",
        "observed_at",
        "server_signature",
        "digest",
    ] {
        assert!(!result[base].is_null(), "{base} missing: {result}");
    }
    let required = required_for(status);
    for field in required.iter() {
        assert!(
            !result[*field].is_null(),
            "{field} required for {status}: {result}"
        );
    }
    let mut forbidden: Vec<&str> = RESULT_FIELDS
        .iter()
        .copied()
        .filter(|f| !required.contains(f))
        .collect();
    if status != "historically_fenced_absent" {
        // Live `absent` forbids RestoreLineage fields outright; `unknown`
        // MAY carry diagnostic evidence but this endpoint never does.
        forbidden.extend(LINEAGE_FIELDS.iter().copied());
    }
    for field in forbidden {
        assert!(
            result.get(field).is_none(),
            "{field} is forbidden for {status}: {result}"
        );
    }
    // The signed body: `digest` covers the record minus itself.
    let mut without = result.clone();
    without.as_object_mut().unwrap().remove("digest");
    assert_eq!(
        result["digest"],
        kovee::sealed(bpp_core::hostint::QUERY_RESULT_TAG, &without),
        "result digest covers the signed body"
    );
    assert!(result["server_signature"]
        .as_str()
        .unwrap_or_default()
        .starts_with("sig1."));
}

struct Fixture {
    daemon: TestDaemon,
    society: String,
    incarnation: String,
    sovereign: String,
    seam: Seam,
}

fn fixture(tag: &str) -> Fixture {
    let mut daemon = TestDaemon::start(tag);
    let (society, _cursor, incarnation) = bootstrap_society(&daemon, tag);
    let sovereign = sovereign_id(&daemon, &society);
    let seam = kovee::install_seam(&mut daemon, &society, &incarnation, 0);
    Fixture {
        daemon,
        society,
        incarnation,
        sovereign,
        seam,
    }
}

impl Fixture {
    fn attempt(&self, tag: &str, key: &str, nonce: &str, sponsors: &[&str]) -> kovee::Attempt {
        self.seam.form(
            0,
            &self.sovereign,
            "kovee-principal-1",
            key,
            nonce,
            kovee::proposal(sponsors, tag),
            kovee::position(&self.sovereign, "assent"),
            &Bend::default(),
        )
    }

    fn form(&self, attempt: &kovee::Attempt) -> Value {
        self.daemon
            .call_raw(
                "governance",
                Some(&attempt.credential),
                &attempt.request.to_string(),
            )
            .unwrap()
    }

    fn query(&self, request: &Value) -> Value {
        self.daemon
            .call_raw(
                "projection",
                Some(&self.seam.recovery_workload_token),
                &request.to_string(),
            )
            .unwrap()
    }

    fn terminalize(&self, request: &Value, credential: &str) -> Value {
        self.daemon
            .call_raw("governance", Some(credential), &request.to_string())
            .unwrap()
    }
}

#[test]
fn the_five_facts_each_answer_their_exact_precondition() {
    let fx = fixture("b3r1");
    let inc = fx.incarnation.clone();

    // ---- absent: a complete query of the LIVE target domain finds
    // neither a result nor a terminal tombstone.
    let never = fx.attempt("b3r1-absent", "k-absent", "n-absent", &[&fx.sovereign]);
    let absent = fx.query(&fx.seam.query(
        0,
        "kovee-principal-1",
        "k-absent",
        &never.canonical_command_digest,
        &inc,
        0,
        None,
    ));
    assert_eq!(absent["outcome"], "ok", "{absent}");
    assert_closed(&absent["result"], "absent");

    // ---- committed: the retained signed KoveeEndeavorFormResult, so no
    // second actor-only fetch is required.
    let formed = fx.attempt("b3r1", "k-committed", "n-committed", &[&fx.sovereign]);
    let reply = fx.form(&formed);
    assert_eq!(reply["outcome"], "ok", "{reply}");
    let committed = fx.query(&fx.seam.query(
        0,
        "kovee-principal-1",
        "k-committed",
        &formed.canonical_command_digest,
        &inc,
        0,
        None,
    ));
    let result = &committed["result"];
    assert_closed(result, "committed");
    assert_eq!(
        result["committed_result_envelope"], reply["result"],
        "the query re-serves the exact committed envelope"
    );
    assert!(result["committed_result_signature"]
        .as_str()
        .unwrap()
        .starts_with("sig1."));

    // ---- non_reexecuting_tombstone: the durable Byom-owned terminal
    // claim a formation_requires_participation rejection installed.
    let refused = fx.attempt(
        "b3r1-t",
        "k-tomb",
        "n-tomb",
        &[&fx.sovereign, "part-agent-1"],
    );
    let problem = fx.form(&refused);
    assert_eq!(kind_of(&problem), "formation_requires_participation");
    let tombstoned = fx.query(&fx.seam.query(
        0,
        "kovee-principal-1",
        "k-tomb",
        &refused.canonical_command_digest,
        &inc,
        0,
        None,
    ));
    let result = &tombstoned["result"];
    assert_closed(result, "non_reexecuting_tombstone");
    assert_eq!(
        result["tombstone_ref"], problem["problem"]["dev.byom.tombstone_ref"],
        "the same tombstone the formation refusal named"
    );

    // ---- unknown: a HISTORICAL target with no lineage cited. Missing
    // lineage is unknown, NEVER live absent.
    let unknown = fx.query(&fx.seam.query(
        0,
        "kovee-principal-1",
        "k-absent",
        &never.canonical_command_digest,
        "inc-predecessor-1",
        0,
        None,
    ));
    assert_closed(&unknown["result"], "unknown");

    // ---- historically_fenced_absent: a complete, externally witnessed
    // RestoreLineageProof over a permanently fenced predecessor.
    let hop = kovee::lineage(
        "lin-1",
        &fx.seam.endpoint_root_id,
        &fx.society,
        ("inc-predecessor-1", 0),
        (&inc, 0),
        "complete",
        "witness-1",
    );
    let proof = kovee::lineage_proof(
        "rlp-1",
        &fx.seam.endpoint_root_id,
        &fx.society,
        ("inc-predecessor-1", 0),
        (&inc, 0),
        &kovee::kovee_domain_digest(&fx.seam.realm_ref, "k-absent"),
        std::slice::from_ref(&hop),
    );
    let mut fx = fx;
    reinstall(&mut fx, |cfg| {
        cfg["restore_lineages"] = json!([hop]);
        cfg["restore_lineage_proofs"] = json!([proof]);
        cfg["external_witness_receipts"] = json!([kovee::witness_receipt("witness-1")]);
    });
    let proof_digest = fx.proof_digest("rlp-1");
    let fenced = fx.query(&fx.seam.query(
        0,
        "kovee-principal-1",
        "k-absent",
        &never.canonical_command_digest,
        "inc-predecessor-1",
        0,
        Some(("rlp-1", &proof_digest)),
    ));
    let result = &fenced["result"];
    assert_closed(result, "historically_fenced_absent");
    assert_eq!(result["restore_lineage_evidence_ref"], json!("rlp-1"));
    assert!(result["historical_fence_receipt_ref"]
        .as_str()
        .unwrap()
        .starts_with("hfr-"));
}

#[test]
fn an_incomplete_or_unwitnessed_hop_is_unknown_never_fenced_absent() {
    let mut fx = fixture("b3r2");
    let inc = fx.incarnation.clone();
    let never = fx.attempt("b3r2", "k-hist", "n-hist", &[&fx.sovereign]);
    let domain = kovee::kovee_domain_digest(&fx.seam.realm_ref, "k-hist");

    // A hop whose retention is incomplete: a later complete hop cannot
    // launder it, and the answer is unknown.
    let incomplete = kovee::lineage(
        "lin-incomplete",
        &fx.seam.endpoint_root_id,
        &fx.society,
        ("inc-predecessor-1", 0),
        (&inc, 0),
        "incomplete",
        "witness-1",
    );
    let proof = kovee::lineage_proof(
        "rlp-incomplete",
        &fx.seam.endpoint_root_id,
        &fx.society,
        ("inc-predecessor-1", 0),
        (&inc, 0),
        &domain,
        std::slice::from_ref(&incomplete),
    );
    reinstall(&mut fx, |cfg| {
        cfg["restore_lineages"] = json!([incomplete]);
        cfg["restore_lineage_proofs"] = json!([proof]);
        cfg["external_witness_receipts"] = json!([kovee::witness_receipt("witness-1")]);
    });
    let digest = fx.proof_digest("rlp-incomplete");
    let answer = fx.query(&fx.seam.query(
        0,
        "kovee-principal-1",
        "k-hist",
        &never.canonical_command_digest,
        "inc-predecessor-1",
        0,
        Some(("rlp-incomplete", &digest)),
    ));
    assert_closed(&answer["result"], "unknown");

    // A complete hop the endpoint cannot witness is equally unknown.
    let unwitnessed = kovee::lineage(
        "lin-unwitnessed",
        &fx.seam.endpoint_root_id,
        &fx.society,
        ("inc-predecessor-1", 0),
        (&inc, 0),
        "complete",
        "witness-offline",
    );
    let proof2 = kovee::lineage_proof(
        "rlp-unwitnessed",
        &fx.seam.endpoint_root_id,
        &fx.society,
        ("inc-predecessor-1", 0),
        (&inc, 0),
        &domain,
        std::slice::from_ref(&unwitnessed),
    );
    reinstall(&mut fx, |cfg| {
        cfg["restore_lineages"] = json!([unwitnessed]);
        cfg["restore_lineage_proofs"] = json!([proof2]);
        cfg["external_witness_receipts"] = json!([kovee::witness_receipt("witness-1")]);
    });
    let digest = fx.proof_digest("rlp-unwitnessed");
    let answer = fx.query(&fx.seam.query(
        0,
        "kovee-principal-1",
        "k-hist",
        &never.canonical_command_digest,
        "inc-predecessor-1",
        0,
        Some(("rlp-unwitnessed", &digest)),
    ));
    assert_closed(&answer["result"], "unknown");
}

#[test]
fn the_query_is_read_only_and_narrowly_bound() {
    let fx = fixture("b3r3");
    let never = fx.attempt("b3r3", "k-q", "n-q", &[&fx.sovereign]);
    let request = fx.seam.query(
        0,
        "kovee-principal-1",
        "k-q",
        &never.canonical_command_digest,
        &fx.incarnation,
        0,
        None,
    );
    // Without the narrow recovery workload it is forbidden — an ordinary
    // projection reader cannot reconcile external commands.
    let bare = fx
        .daemon
        .call_raw("projection", None, &request.to_string())
        .unwrap();
    assert_eq!(kind_of(&bare), "forbidden", "{bare}");

    // R42 is a projection row: it never answers on governance.
    let governance = fx
        .daemon
        .call_raw("governance", None, &request.to_string())
        .unwrap();
    assert_eq!(kind_of(&governance), "forbidden_surface", "{governance}");

    // A superseded recovery binding cannot authenticate the query.
    let mut stale = request.clone();
    stale["current_recovery_binding_revision"] = json!(99);
    assert_eq!(kind_of(&fx.query(&stale)), "stale_binding");
}

#[test]
fn terminalization_is_a_closed_three_way_result() {
    let fx = fixture("b3r4");
    let inc = fx.incarnation.clone();

    // ---- committed: a formation that already committed is a Byom
    // no-op; terminalization returns the committed envelope.
    let formed = fx.attempt("b3r4", "k-done", "n-done", &[&fx.sovereign]);
    let form_reply = fx.form(&formed);
    assert_eq!(form_reply["outcome"], "ok", "{form_reply}");
    let (request, credential) = fx.seam.terminalize(
        0,
        &fx.sovereign,
        "kovee-principal-1",
        "k-done",
        "n-term-1",
        &formed.canonical_command_digest,
        &inc,
        0,
        "the user gave up waiting",
        None,
    );
    let reply = fx.terminalize(&request, &credential);
    assert_eq!(reply["outcome"], "ok", "{reply}");
    let result = &reply["result"];
    assert_eq!(result["status"], "committed");
    assert_eq!(result["committed_result_envelope"], form_reply["result"]);
    for forbidden in [
        "tombstone_ref",
        "tombstone_digest",
        "tombstone_reason",
        "authority_journal_receipt_ref",
        "authority_journal_receipt_digest",
        "blocking_state",
        "blocking_evidence_digest",
    ] {
        assert!(result.get(forbidden).is_none(), "{forbidden}: {result}");
    }

    // ---- terminalized: an unresolved domain gets the restore-safe
    // non-reexecuting tombstone plus a synchronous journal receipt.
    let pending = fx.attempt("b3r4b", "k-pending", "n-pending", &[&fx.sovereign]);
    let (request, credential) = fx.seam.terminalize(
        0,
        &fx.sovereign,
        "kovee-principal-1",
        "k-pending",
        "n-term-2",
        &pending.canonical_command_digest,
        &inc,
        0,
        "the principal abandoned the formation",
        None,
    );
    let reply = fx.terminalize(&request, &credential);
    assert_eq!(reply["outcome"], "ok", "{reply}");
    let result = &reply["result"];
    assert_eq!(result["status"], "terminalized", "{result}");
    for required in [
        "tombstone_ref",
        "tombstone_digest",
        "tombstone_reason",
        "authority_journal_receipt_ref",
        "authority_journal_receipt_digest",
    ] {
        assert!(!result[required].is_null(), "{required}: {result}");
    }
    for forbidden in [
        "committed_result_envelope",
        "committed_result_digest",
        "committed_result_signature",
        "blocking_state",
        "blocking_evidence_digest",
    ] {
        assert!(result.get(forbidden).is_none(), "{forbidden}: {result}");
    }
    let tombstone_ref = result["tombstone_ref"].as_str().unwrap().to_owned();

    // ---- not_terminalizable: a HISTORICAL target with no lineage.
    let hist = fx.attempt("b3r4c", "k-hist", "n-hist", &[&fx.sovereign]);
    let (request, credential) = fx.seam.terminalize(
        0,
        &fx.sovereign,
        "kovee-principal-1",
        "k-hist",
        "n-term-3",
        &hist.canonical_command_digest,
        "inc-predecessor-1",
        0,
        "reconciling a restored endpoint",
        None,
    );
    let reply = fx.terminalize(&request, &credential);
    let result = &reply["result"];
    assert_eq!(result["status"], "not_terminalizable", "{result}");
    assert_eq!(result["blocking_state"], "lineage_incomplete");
    assert!(!result["blocking_evidence_digest"].is_null());
    for forbidden in [
        "committed_result_envelope",
        "tombstone_ref",
        "authority_journal_receipt_ref",
    ] {
        assert!(result.get(forbidden).is_none(), "{forbidden}: {result}");
    }

    // ---- not_terminalizable: witness_unavailable, from a complete hop
    // this endpoint cannot witness.
    let mut fx = fx;
    let hop = kovee::lineage(
        "lin-1",
        &fx.seam.endpoint_root_id,
        &fx.society,
        ("inc-predecessor-1", 0),
        (&inc, 0),
        "complete",
        "witness-offline",
    );
    let proof = kovee::lineage_proof(
        "rlp-1",
        &fx.seam.endpoint_root_id,
        &fx.society,
        ("inc-predecessor-1", 0),
        (&inc, 0),
        &kovee::kovee_domain_digest(&fx.seam.realm_ref, "k-hist"),
        std::slice::from_ref(&hop),
    );
    reinstall(&mut fx, |cfg| {
        cfg["restore_lineages"] = json!([hop]);
        cfg["restore_lineage_proofs"] = json!([proof]);
        cfg["external_witness_receipts"] = json!([kovee::witness_receipt("witness-1")]);
    });
    let digest = fx.proof_digest("rlp-1");
    let (request, credential) = fx.seam.terminalize(
        0,
        &fx.sovereign,
        "kovee-principal-1",
        "k-hist",
        "n-term-4",
        &hist.canonical_command_digest,
        "inc-predecessor-1",
        0,
        "reconciling a restored endpoint",
        Some(("rlp-1", &digest)),
    );
    let reply = fx.terminalize(&request, &credential);
    assert_eq!(
        reply["result"]["blocking_state"], "witness_unavailable",
        "{reply}"
    );

    // ---- not_terminalizable: domain_conflict, when the domain is
    // claimed by other command bytes.
    let other_command = kovee::portable(
        bpp_core::hostint::COMMAND_TAG,
        &json!({"a different command over the same Kovee key": true}),
    );
    let (request, credential) = fx.seam.terminalize(
        0,
        &fx.sovereign,
        "kovee-principal-1",
        "k-pending",
        "n-term-5",
        &other_command,
        &inc,
        0,
        "second thoughts",
        None,
    );
    let reply = fx.terminalize(&request, &credential);
    assert_eq!(reply["result"]["status"], "not_terminalizable", "{reply}");
    assert_eq!(reply["result"]["blocking_state"], "domain_conflict");

    // Re-terminalizing an already tombstoned domain re-serves the same
    // tombstone — never a second claim.
    let (request, credential) = fx.seam.terminalize(
        0,
        &fx.sovereign,
        "kovee-principal-1",
        "k-pending",
        "n-term-6",
        &pending.canonical_command_digest,
        &inc,
        0,
        "second thoughts",
        None,
    );
    let reply = fx.terminalize(&request, &credential);
    assert_eq!(reply["result"]["status"], "terminalized", "{reply}");
    assert_eq!(reply["result"]["tombstone_ref"], json!(tombstone_ref));
}

#[test]
fn a_prepared_or_in_flight_execution_blocks_terminalization() {
    let mut fx = fixture("b3r8");
    let inc = fx.incarnation.clone();
    let attempt = fx.attempt("b3r8", "k-flight", "n-flight", &[&fx.sovereign]);

    // Kill the daemon after the prepare transaction commits, before the
    // witness CAS: the §15.3 pending row is exactly "execution prepared".
    fx.daemon
        .restart(&[("BYOMD_ABORT", "before_witness:kovee_endeavor_form")]);
    let died = fx.daemon.call_raw(
        "governance",
        Some(&attempt.credential),
        &attempt.request.to_string(),
    );
    assert!(died.is_err(), "expected the daemon to die: {died:?}");
    fx.daemon.wait_exit();
    fx.daemon.restart(&[]);
    // Startup recovery abandoned it after proving no witness entry; park
    // it back in `prepared` to hold the domain in flight for this test.
    let conn = rusqlite::Connection::open(fx.daemon.data_dir.join("byom.db")).unwrap();
    let moved = conn
        .execute(
            "UPDATE authority_pending SET state = 'prepared'
             WHERE state = 'abandoned' AND operation = 'kovee_endeavor_form'",
            [],
        )
        .unwrap();
    assert_eq!(moved, 1, "exactly one prepared transition to hold");
    drop(conn);

    let (request, credential) = fx.seam.terminalize(
        0,
        &fx.sovereign,
        "kovee-principal-1",
        "k-flight",
        "n-term",
        &attempt.canonical_command_digest,
        &inc,
        0,
        "the user pressed cancel",
        None,
    );
    let reply = fx.terminalize(&request, &credential);
    assert_eq!(reply["result"]["status"], "not_terminalizable", "{reply}");
    assert_eq!(reply["result"]["blocking_state"], "prepared_or_in_flight");

    // The read-only query says `unknown` for the same reason — in-flight
    // state is never reported as live `absent`.
    let answer = fx.query(&fx.seam.query(
        0,
        "kovee-principal-1",
        "k-flight",
        &attempt.canonical_command_digest,
        &inc,
        0,
        None,
    ));
    assert_closed(&answer["result"], "unknown");
}

#[test]
fn terminalization_is_same_source_human_only() {
    let fx = fixture("b3r5");
    let pending = fx.attempt("b3r5", "k-x", "n-x", &[&fx.sovereign]);
    // Another principal on its own valid channel cannot terminalize the
    // first human's command.
    let (mut request, _) = fx.seam.terminalize(
        0,
        &fx.sovereign,
        "kovee-principal-2",
        "k-x",
        "n-y",
        &pending.canonical_command_digest,
        &fx.incarnation,
        0,
        "not mine to cancel",
        None,
    );
    // The request names the FIRST human's durable binding while the
    // channel belongs to the second: the same-source-human check fails.
    request["source_principal_ref"] = json!("kovee-principal-1");
    let credential = fx.seam.credential(
        &fx.sovereign,
        "kovee-principal-2",
        "n-y",
        &pending.canonical_command_digest,
        &["external_command_terminalize"],
        0,
        &Bend::default(),
    );
    let reply = fx.terminalize(&request, &Seam::preamble(&credential));
    assert_eq!(kind_of(&reply), "forbidden", "{reply}");

    // And R40 answers only on the delegated-principal channel.
    let (request, _) = fx.seam.terminalize(
        0,
        &fx.sovereign,
        "kovee-principal-1",
        "k-x",
        "n-z",
        &pending.canonical_command_digest,
        &fx.incarnation,
        0,
        "abandon",
        None,
    );
    let bare = fx
        .daemon
        .call_raw("governance", None, &request.to_string())
        .unwrap();
    assert_eq!(kind_of(&bare), "forbidden", "{bare}");
}

#[test]
fn a_delayed_command_racing_terminalization_cannot_both_win() {
    // Terminalization first: the delayed command observes the tombstone.
    let fx = fixture("b3r6");
    let delayed = fx.attempt("b3r6", "k-race", "n-race", &[&fx.sovereign]);
    let (request, credential) = fx.seam.terminalize(
        0,
        &fx.sovereign,
        "kovee-principal-1",
        "k-race",
        "n-term",
        &delayed.canonical_command_digest,
        &fx.incarnation,
        0,
        "assume it never arrived",
        None,
    );
    let terminalized = fx.terminalize(&request, &credential);
    assert_eq!(terminalized["result"]["status"], "terminalized");

    let arrived = fx.form(&delayed);
    assert_eq!(
        kind_of(&arrived),
        "forbidden",
        "the delayed command observes the tombstone: {arrived}"
    );
    assert_eq!(
        arrived["problem"]["dev.byom.tombstone_ref"],
        terminalized["result"]["tombstone_ref"]
    );
    let snapshot = fx.daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "snapshot_get",
                "society_id": fx.society, "kinds": ["endeavors"]}),
    );
    assert!(
        snapshot["result"]["endeavors"]
            .as_array()
            .unwrap()
            .is_empty(),
        "the tombstoned domain never executed"
    );

    // The other order: the command commits first, and terminalization is
    // a no-op that returns the committed envelope.
    let fx2 = fixture("b3r7");
    let winner = fx2.attempt("b3r7", "k-race", "n-race", &[&fx2.sovereign]);
    let committed = fx2.form(&winner);
    assert_eq!(committed["outcome"], "ok", "{committed}");
    let (request, credential) = fx2.seam.terminalize(
        0,
        &fx2.sovereign,
        "kovee-principal-1",
        "k-race",
        "n-term",
        &winner.canonical_command_digest,
        &fx2.incarnation,
        0,
        "too late",
        None,
    );
    let late = fx2.terminalize(&request, &credential);
    assert_eq!(late["result"]["status"], "committed", "{late}");
    assert!(late["result"].get("tombstone_ref").is_none());
    assert_eq!(
        fx2.query_status("k-race", &winner.canonical_command_digest),
        "committed"
    );
}

// ------------------------------------------------------------ helpers ----

impl Fixture {
    /// The retained proof digest for a cited RestoreLineageProof.
    fn proof_digest(&self, proof_id: &str) -> Value {
        let text =
            std::fs::read_to_string(self.daemon.data_dir.join("kovee").join("host-binding.json"))
                .unwrap();
        let cfg: Value = serde_json::from_str(&text).unwrap();
        cfg["restore_lineage_proofs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["proof_id"] == json!(proof_id))
            .unwrap_or_else(|| panic!("proof {proof_id}"))["digest"]
            .clone()
    }

    fn query_status(&self, key: &str, canonical: &Value) -> String {
        let reply = self.query(&self.seam.query(
            0,
            "kovee-principal-1",
            key,
            canonical,
            &self.incarnation,
            0,
            None,
        ));
        reply["result"]["status"].as_str().unwrap().to_owned()
    }
}

/// Rewrites the installed host binding (the sealed restore protocol's
/// output arriving as endpoint configuration) and restarts the endpoint.
fn reinstall(fx: &mut Fixture, edit: impl FnOnce(&mut Value)) {
    let path = fx.daemon.data_dir.join("kovee").join("host-binding.json");
    let mut cfg: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    edit(&mut cfg);
    kovee::write_config(&fx.daemon, &cfg);
    fx.daemon.restart(&[]);
}
