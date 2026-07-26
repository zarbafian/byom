//! B3 slice 1 — `kovee_endeavor_form` across every §15.3 commit and
//! journal boundary (DESIGN.md §15.3, §16.3).
//!
//! At each boundary the daemon is killed mid-request, restarted, and the
//! EXACT attempt retried. Three properties must hold every time:
//!
//! 1. **exactly-once formation** — one Endeavor, one Position, one
//!    GovernanceDecision, whatever the boundary;
//! 2. **dense events** — the per-Society sequence has no gap and no
//!    duplicate, so an abandoned transition consumed no sequence;
//! 3. **byte-identical replay** — the retry and every later replay
//!    return the same bytes, and the recovery query re-serves the same
//!    committed envelope.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::kovee::{self, Bend, Seam};
use common::*;
use serde_json::{json, Value};

/// Every §15.3 boundary of one authority mutation, plus the two witness
/// faults the journal must survive.
const BOUNDARIES: [&str; 6] = [
    "before_witness",
    "after_witness",
    "before_finalize",
    "after_finalize",
    "witness_lose_reply",
    "witness_lose_request",
];

struct Case {
    daemon: TestDaemon,
    society: String,
    cursor: String,
    /// The ledger position the genesis cursor sits at, so density can be
    /// checked as "count == head - base" over the whole page.
    base: u64,
    seam: Seam,
    sovereign: String,
}

fn prepare(tag: &str, phase: &str) -> Case {
    let mut daemon = TestDaemon::start(tag);
    let (society, cursor, incarnation) = bootstrap_society(&daemon, tag);
    let sovereign = sovereign_id(&daemon, &society);
    let seam = kovee::install_seam(&mut daemon, &society, &incarnation, 0);
    let mut case = Case {
        daemon,
        society,
        cursor,
        base: 0,
        seam,
        sovereign,
    };
    case.base = case.head() - case.events().len() as u64;
    // Arm the fault only now: the Society and the seam must exist first.
    case.daemon
        .restart(&[("BYOMD_ABORT", &format!("{phase}:kovee_endeavor_form"))]);
    case
}

impl Case {
    fn form(&self, attempt: &kovee::Attempt) -> Result<Value, String> {
        self.daemon.call_raw(
            "governance",
            Some(&attempt.credential),
            &attempt.request.to_string(),
        )
    }

    fn events(&self) -> Vec<Value> {
        let page = self.daemon.call(
            "projection",
            &json!({"version": "0.2", "op": "events_read",
                    "continuation": self.cursor, "page_size": 512}),
        );
        page["result"]["events"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    }

    fn endeavors(&self) -> Vec<Value> {
        let snapshot = self.daemon.call(
            "projection",
            &json!({"version": "0.2", "op": "snapshot_get",
                    "society_id": self.society, "kinds": ["endeavors"]}),
        );
        snapshot["result"]["endeavors"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    }

    fn head(&self) -> u64 {
        let snapshot = self.daemon.call(
            "projection",
            &json!({"version": "0.2", "op": "snapshot_get",
                    "society_id": self.society, "kinds": ["endeavors"]}),
        );
        snapshot["result"]["as_of_event_sequence"].as_u64().unwrap()
    }

    /// The per-Society event sequence must be DENSE: reading the whole
    /// page from the genesis cursor yields exactly `head - base` events,
    /// each with a distinct id. A gap (an abandoned transition that
    /// consumed a sequence) or a duplicate (a transition materialized
    /// twice) breaks the equality.
    fn assert_dense_events(&self, phase: &str) {
        let events = self.events();
        let ids: std::collections::BTreeSet<&str> = events
            .iter()
            .filter_map(|e| e["event_id"].as_str())
            .collect();
        assert_eq!(ids.len(), events.len(), "{phase}: duplicate event ids");
        assert_eq!(
            events.len() as u64,
            self.head() - self.base,
            "{phase}: the per-Society sequence has a gap or a duplicate"
        );
        // The page's own continuation is at the head: nothing follows.
        let page = self.daemon.call(
            "projection",
            &json!({"version": "0.2", "op": "events_read",
                    "continuation": self.cursor, "page_size": 512}),
        );
        let next = self.daemon.call(
            "projection",
            &json!({"version": "0.2", "op": "events_read",
                    "continuation": page["result"]["continuation"], "page_size": 512}),
        );
        assert_eq!(
            next["result"]["events"].as_array().map(Vec::len),
            Some(0),
            "{phase}: the page did not reach the ledger head"
        );
    }
}

#[test]
fn every_commit_and_journal_boundary_forms_exactly_once() {
    for phase in BOUNDARIES {
        let case = prepare(&format!("b3c-{phase}"), phase);
        let attempt = case.seam.form(
            0,
            &case.sovereign,
            "kovee-principal-1",
            "k-crash",
            "n-crash",
            kovee::proposal(&[&case.sovereign], "b3c"),
            kovee::position(&case.sovereign, "assent"),
            &Bend::default(),
        );
        let mut case = case;
        let outcome = case.form(&attempt);
        match phase {
            // The two witness faults are handled INSIDE the live call:
            // a lost reply recovers by query, a lost request abandons
            // after proof. Neither kills the process.
            "witness_lose_reply" => {
                let reply = outcome.expect("lost reply must still answer");
                assert_eq!(reply["outcome"], "ok", "{phase}: {reply}");
            }
            "witness_lose_request" => {
                let reply = outcome.expect("lost request must still answer");
                assert_eq!(kind_of(&reply), "unavailable", "{phase}: {reply}");
                assert!(
                    case.endeavors().is_empty(),
                    "{phase}: an abandoned transition committed nothing"
                );
                // The endpoint recovers the moment the fault is removed.
                case.daemon.restart(&[]);
            }
            _ => {
                assert!(outcome.is_err(), "{phase}: expected the daemon to die");
                case.daemon.wait_exit();
                case.daemon.restart(&[]);
            }
        }

        // The EXACT attempt is retried and must answer.
        let retried = case
            .form(&attempt)
            .unwrap_or_else(|e| panic!("{phase}: {e}"));
        assert_eq!(retried["outcome"], "ok", "{phase}: retry: {retried}");

        // Byte-identical replay, twice, and across another restart.
        let again = case.form(&attempt).unwrap();
        assert_eq!(again, retried, "{phase}: replay must be byte-identical");
        case.daemon.restart(&[]);
        let after_restart = case.form(&attempt).unwrap();
        assert_eq!(
            after_restart, retried,
            "{phase}: replay after restart must be byte-identical"
        );

        // Exactly-once formation.
        let endeavors = case.endeavors();
        assert_eq!(endeavors.len(), 1, "{phase}: {endeavors:?}");
        assert_eq!(endeavors[0]["state"], "active", "{phase}");
        assert_eq!(
            endeavors[0]["endeavor_id"], retried["result"]["endeavor_ref"],
            "{phase}"
        );
        let formed: Vec<String> = case
            .events()
            .iter()
            .filter_map(|e| e["kind"].as_str().map(str::to_owned))
            .filter(|k| k == "kovee.endeavor_formed" || k == "endeavor.finalized")
            .collect();
        assert_eq!(
            formed.len(),
            2,
            "{phase}: exactly one formation event set: {formed:?}"
        );
        case.assert_dense_events(phase);

        // The recovery surface re-serves the SAME committed envelope.
        let query = case.seam.query(
            0,
            "kovee-principal-1",
            "k-crash",
            &attempt.canonical_command_digest,
            &case.seam.incarnation,
            0,
            None,
        );
        let answer = case
            .daemon
            .call_raw(
                "projection",
                Some(&case.seam.recovery_workload_token),
                &query.to_string(),
            )
            .unwrap();
        assert_eq!(answer["result"]["status"], "committed", "{phase}: {answer}");
        assert_eq!(
            answer["result"]["committed_result_envelope"], retried["result"],
            "{phase}: the retained envelope is the replayed one"
        );
    }
}

#[test]
fn a_crash_before_the_tombstone_commits_nothing_and_replays_the_same_refusal() {
    // The `formation_requires_participation` path is a real authority
    // transition too: killed at each boundary it must still claim the
    // domain exactly once and refuse identically.
    for phase in ["before_witness", "after_witness", "before_finalize"] {
        let mut case = prepare(&format!("b3ct-{phase}"), phase);
        let attempt = case.seam.form(
            0,
            &case.sovereign,
            "kovee-principal-1",
            "k-crash-t",
            "n-crash-t",
            kovee::proposal(&[&case.sovereign, "part-agent-1"], "b3ct"),
            kovee::position(&case.sovereign, "assent"),
            &Bend::default(),
        );
        assert!(case.form(&attempt).is_err(), "{phase}: expected death");
        case.daemon.wait_exit();
        case.daemon.restart(&[]);

        let retried = case
            .form(&attempt)
            .unwrap_or_else(|e| panic!("{phase}: {e}"));
        assert_eq!(
            kind_of(&retried),
            "formation_requires_participation",
            "{phase}: {retried}"
        );
        let again = case.form(&attempt).unwrap();
        assert_eq!(again, retried, "{phase}: the refusal replays identically");
        assert!(
            case.endeavors().is_empty(),
            "{phase}: no Society or Endeavor domain record"
        );
        // A fresh attempt over the same command meets the SAME tombstone.
        let fresh = case.seam.form(
            0,
            &case.sovereign,
            "kovee-principal-1",
            "k-crash-t",
            "n-crash-t2",
            kovee::proposal(&[&case.sovereign, "part-agent-1"], "b3ct"),
            kovee::position(&case.sovereign, "assent"),
            &Bend::default(),
        );
        let fresh_reply = case.form(&fresh).unwrap();
        assert_eq!(
            fresh_reply["problem"]["dev.byom.tombstone_ref"],
            retried["problem"]["dev.byom.tombstone_ref"],
            "{phase}: the domain is claimed exactly once"
        );
        case.assert_dense_events(phase);
    }
}

#[test]
fn a_crash_during_terminalization_installs_the_tombstone_exactly_once() {
    for phase in ["before_witness", "after_witness", "before_finalize"] {
        let mut case = prepare(&format!("b3cx-{phase}"), phase);
        // Re-arm for the terminalize operation instead.
        case.daemon.restart(&[(
            "BYOMD_ABORT",
            &format!("{phase}:external_command_terminalize"),
        )]);
        let attempt = case.seam.form(
            0,
            &case.sovereign,
            "kovee-principal-1",
            "k-crash-x",
            "n-crash-x",
            kovee::proposal(&[&case.sovereign], "b3cx"),
            kovee::position(&case.sovereign, "assent"),
            &Bend::default(),
        );
        let (request, credential) = case.seam.terminalize(
            0,
            &case.sovereign,
            "kovee-principal-1",
            "k-crash-x",
            "n-term-x",
            &attempt.canonical_command_digest,
            &case.seam.incarnation,
            0,
            "abandoned before it ever arrived",
            None,
        );
        let died = case
            .daemon
            .call_raw("governance", Some(&credential), &request.to_string());
        assert!(died.is_err(), "{phase}: expected death");
        case.daemon.wait_exit();
        case.daemon.restart(&[]);

        let retried = case
            .daemon
            .call_raw("governance", Some(&credential), &request.to_string())
            .unwrap_or_else(|e| panic!("{phase}: {e}"));
        assert_eq!(retried["outcome"], "ok", "{phase}: {retried}");
        assert_eq!(retried["result"]["status"], "terminalized", "{phase}");
        let again = case
            .daemon
            .call_raw("governance", Some(&credential), &request.to_string())
            .unwrap();
        assert_eq!(again, retried, "{phase}: replay is byte-identical");

        // The delayed command now loses the race, once and for all.
        let arrived = case.form(&attempt).unwrap();
        assert_eq!(kind_of(&arrived), "forbidden", "{phase}: {arrived}");
        assert_eq!(
            arrived["problem"]["dev.byom.tombstone_ref"], retried["result"]["tombstone_ref"],
            "{phase}"
        );
        assert!(case.endeavors().is_empty(), "{phase}");
        case.assert_dense_events(phase);
    }
}
