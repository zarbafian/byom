//! Shared harness for the byomd integration suites: spawns the real
//! daemon binary against throwaway data/runtime dirs, speaks the wire
//! protocol over the real per-surface sockets, and can kill/restart the
//! process (the crash matrix) or restore a database snapshot (the
//! rollback adversary).

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod kovee;

use std::io::{BufRead as _, BufReader, Write as _};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

pub struct TestDaemon {
    child: Option<Child>,
    pub data_dir: PathBuf,
    pub run_dir: PathBuf,
    /// The peer-bound proof keys this PROCESS claimed, per credential
    /// (BY-C1). A real client claims once while its channel is open and
    /// keeps the key — which is what lets the exact refusal still replay
    /// through a channel that has since closed (BY-C2).
    claimed: std::sync::Mutex<std::collections::HashMap<String, [u8; 32]>>,
}

fn fresh_dir(tag: &str, which: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "byomd-{tag}-{which}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

impl TestDaemon {
    /// Fresh dirs, no fault injection.
    pub fn start(tag: &str) -> TestDaemon {
        let data_dir = fresh_dir(tag, "data");
        let run_dir = fresh_dir(tag, "run");
        TestDaemon::start_at(data_dir, run_dir, &[])
    }

    /// Fresh dirs with extra environment (e.g. BYOMD_ABORT).
    pub fn start_with_env(tag: &str, env: &[(&str, &str)]) -> TestDaemon {
        let data_dir = fresh_dir(tag, "data");
        let run_dir = fresh_dir(tag, "run");
        TestDaemon::start_at(data_dir, run_dir, env)
    }

    /// (Re)start against existing dirs.
    pub fn start_at(data_dir: PathBuf, run_dir: PathBuf, env: &[(&str, &str)]) -> TestDaemon {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_byomd"));
        cmd.env("BYOM_DATA_DIR", &data_dir)
            .env("BYOM_RUNTIME_DIR", &run_dir)
            .env_remove("BYOMD_ABORT")
            .stdout(Stdio::null());
        if std::env::var_os("BYOMD_TEST_STDERR").is_some() {
            cmd.stderr(Stdio::inherit());
        } else {
            cmd.stderr(Stdio::null());
        }
        for (k, v) in env {
            cmd.env(k, v);
        }
        let child = cmd.spawn().expect("spawn byomd");
        let daemon = TestDaemon {
            child: Some(child),
            data_dir,
            run_dir,
            claimed: std::sync::Mutex::new(std::collections::HashMap::new()),
        };
        daemon.wait_sockets();
        daemon
    }

    fn wait_sockets(&self) {
        let deadline = Instant::now() + Duration::from_secs(15);
        let surfaces = ["governance", "candidate", "participant", "projection"];
        'outer: loop {
            for s in surfaces {
                let path = self.run_dir.join(format!("{s}.sock"));
                if UnixStream::connect(&path).is_err() {
                    if Instant::now() > deadline {
                        panic!("byomd sockets never came up at {}", self.run_dir.display());
                    }
                    std::thread::sleep(Duration::from_millis(25));
                    continue 'outer;
                }
            }
            return;
        }
    }

    /// Kills the daemon (SIGKILL) and reaps it.
    pub fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// Stops the daemon and runs `f` against its data directory with the
    /// daemon's exclusive ownership released (BY-J1) — the in-process
    /// store inspection some R1 tests need.
    pub fn stop_and_take(mut self, f: impl FnOnce(&Path)) {
        self.stop();
        f(&self.data_dir.clone());
    }

    /// Waits for the daemon process to exit on its own (crash hooks).
    pub fn wait_exit(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.wait();
        }
    }

    /// Restarts against the same dirs (no fault env unless given).
    pub fn restart(&mut self, env: &[(&str, &str)]) {
        self.stop();
        let restarted = TestDaemon::start_at(self.data_dir.clone(), self.run_dir.clone(), env);
        self.child = restarted.into_child();
    }

    fn into_child(mut self) -> Option<Child> {
        self.child.take()
    }

    /// One request line, one reply line, over the named surface socket.
    /// `token` is the channel CREDENTIAL (`bpk1.…`): the harness mints a
    /// fresh sender-constrained proof for the exact operation on every
    /// call, exactly as a real client does (BY-C1).
    pub fn call_raw(
        &self,
        surface: &str,
        token: Option<&str>,
        line: &str,
    ) -> Result<Value, String> {
        let path = self.run_dir.join(format!("{surface}.sock"));
        let mut stream = UnixStream::connect(&path).map_err(|e| format!("connect: {e}"))?;
        stream
            .set_read_timeout(Some(Duration::from_secs(20)))
            .unwrap();
        if let Some(token) = token {
            let preamble = self.mint_channel_proof(token, line);
            stream
                .write_all(format!("{preamble}\n").as_bytes())
                .map_err(|e| format!("write token: {e}"))?;
        }
        stream
            .write_all(format!("{line}\n").as_bytes())
            .map_err(|e| format!("write: {e}"))?;
        let mut reader = BufReader::new(stream);
        let mut reply = String::new();
        reader
            .read_line(&mut reply)
            .map_err(|e| format!("read: {e}"))?;
        if reply.is_empty() {
            return Err("connection closed without reply".to_owned());
        }
        serde_json::from_str(reply.trim_end()).map_err(|e| format!("reply parse: {e}"))
    }

    /// Mints the per-call channel proof a real client presents: CLAIM
    /// the channel once for this process (BY-C1 — the credential file
    /// carries no key material), then MAC the exact call under the
    /// peer-bound key byomd issued. A raw (non-credential) preamble is
    /// passed through verbatim so negative tests can present garbage.
    pub fn mint_channel_proof(&self, credential: &str, request_line: &str) -> String {
        if !credential.starts_with("bpk1.") {
            return credential.to_owned();
        }
        let op = serde_json::from_str::<Value>(request_line)
            .ok()
            .and_then(|v| v["op"].as_str().map(str::to_owned))
            .unwrap_or_default();
        let key = self.claim(credential);
        byomd::channel::mint_proof(
            credential,
            &key,
            &op,
            byomd::channel::Peer::current(),
            bpp_core::time::unix_now(),
        )
        .unwrap_or_else(|| panic!("mint proof for {op}"))
    }

    /// This process's peer-bound proof key for one channel credential,
    /// claimed once and kept.
    pub fn claim(&self, credential: &str) -> [u8; 32] {
        let mut held = self.claimed.lock().unwrap();
        if let Some(key) = held.get(credential) {
            return *key;
        }
        // A channel this process may not claim (closed, or held by
        // another live process) leaves the client with NO key: it can
        // only present an unverifiable proof, which byomd refuses. That
        // is the honest client behaviour, not a harness panic.
        let key = byomd::channel::claim(&self.run_dir, credential).unwrap_or([0u8; 32]);
        held.insert(credential.to_owned(), key);
        key
    }

    /// The raw claim outcome (negative tests).
    pub fn try_claim(&self, credential: &str) -> Result<[u8; 32], String> {
        byomd::channel::claim(&self.run_dir, credential)
    }

    pub fn call(&self, surface: &str, request: &Value) -> Value {
        self.call_raw(surface, None, &request.to_string())
            .unwrap_or_else(|e| panic!("call {surface} failed: {e}\nrequest: {request}"))
    }

    pub fn call_candidate(&self, token: &str, request: &Value) -> Value {
        self.call_raw("candidate", Some(token), &request.to_string())
            .unwrap_or_else(|e| panic!("candidate call failed: {e}\nrequest: {request}"))
    }

    /// A call expected to end with the daemon aborting mid-request.
    pub fn call_expect_death(&mut self, surface: &str, request: &Value) {
        let outcome = self.call_raw(surface, None, &request.to_string());
        assert!(
            outcome.is_err(),
            "expected the daemon to die, got {outcome:?}"
        );
        self.wait_exit();
    }

    pub fn call_candidate_expect_death(&mut self, token: &str, request: &Value) {
        let outcome = self.call_raw("candidate", Some(token), &request.to_string());
        assert!(
            outcome.is_err(),
            "expected the daemon to die, got {outcome:?}"
        );
        self.wait_exit();
    }

    pub fn incarnation(&self) -> String {
        let reply = self.call("governance", &json!({"version": "0.2", "op": "hello"}));
        reply["result"]["endpoint_incarnation"]
            .as_str()
            .expect("hello incarnation")
            .to_owned()
    }

    /// The witness journal entries, straight from the file.
    pub fn witness_entries(&self) -> Vec<Value> {
        let text = std::fs::read_to_string(self.data_dir.join("authority-witness.jsonl")).unwrap();
        text.lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    /// Copies the SQLite files (NOT the witness) into a snapshot dir —
    /// the §15.3 rollback adversary's backup.
    pub fn snapshot_db(&self, into: &Path) {
        std::fs::create_dir_all(into).unwrap();
        for entry in std::fs::read_dir(&self.data_dir).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("byom.db") {
                std::fs::copy(entry.path(), into.join(&name)).unwrap();
            }
        }
    }

    /// Restores a database snapshot in place of the live database. The
    /// witness file does NOT roll back.
    pub fn restore_db(&self, from: &Path) {
        for entry in std::fs::read_dir(&self.data_dir).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("byom.db") {
                std::fs::remove_file(entry.path()).unwrap();
            }
        }
        for entry in std::fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name().to_string_lossy().into_owned();
            std::fs::copy(entry.path(), self.data_dir.join(&name)).unwrap();
        }
    }
}

impl Drop for TestDaemon {
    fn drop(&mut self) {
        self.stop();
    }
}

// -------------------------------------------------- request builders ----

pub fn meta(incarnation: &str, key: &str, expected_revision: Option<u64>) -> Value {
    let mut m = json!({
        "request_id": format!("req-{key}"),
        "idempotency_key": format!("idem-{key}"),
        "expected_endpoint_incarnation": incarnation,
        "expected_recovery_epoch": 0,
    });
    if let Some(rev) = expected_revision {
        m["expected_revision"] = json!(rev);
    }
    m
}

pub fn test_digest(seed: u8) -> Value {
    json!({
        "class": "local_erasure_safe",
        "algorithm": "hmac-sha-256",
        "key_ref": format!("test-key-{seed}"),
        "value_hex": format!("{:02x}", seed).repeat(32),
    })
}

/// Prepares and bootstraps one Society; returns
/// (society_id, genesis_cursor, incarnation).
pub fn bootstrap_society(daemon: &TestDaemon, tag: &str) -> (String, String, String) {
    let incarnation = daemon.incarnation();
    let prepare = json!({
        "version": "0.2", "op": "society_prepare",
        "meta": meta(&incarnation, &format!("{tag}-prep"), None),
        "home_authority_ref": "auth-home-1",
        "proposed_charter_ref": "charter-draft-1",
        "proposed_charter_digest": test_digest(0xa1),
        "classification_binding_ref": "class-bind-1",
        "classification_binding_digest": test_digest(0xa2),
    });
    let prepared = daemon.call("governance", &prepare);
    assert_eq!(prepared["outcome"], "ok", "prepare: {prepared}");
    let society_id = prepared["result"]["society_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let bootstrap = json!({
        "version": "0.2", "op": "society_bootstrap",
        "meta": meta(&incarnation, &format!("{tag}-boot"), Some(1)),
        "society_id": society_id,
        "preparation_ref": prepared["result"]["preparation_ref"],
        "subject_digest": prepared["result"]["subject_digest"],
    });
    let booted = daemon.call("governance", &bootstrap);
    assert_eq!(booted["outcome"], "ok", "bootstrap: {booted}");
    let cursor = booted["source_cursor"].as_str().unwrap().to_owned();
    (society_id, cursor, incarnation)
}

/// Creates one membership offer; returns
/// (offer_id, channel_token, subject_digest).
pub fn make_offer(
    daemon: &TestDaemon,
    incarnation: &str,
    tag: &str,
    participant: &str,
    expires_at: &str,
) -> (String, String, Value) {
    let subject_digest = test_digest(0xb1);
    let offer = json!({
        "version": "0.2", "op": "membership_offer",
        "meta": meta(incarnation, &format!("{tag}-offer"), None),
        "participant_ref": participant,
        "proposed_standing_ref": "standing-proposal-1",
        "subject_digest": subject_digest,
        "offered_by_decision_ref": society_decision(daemon),
        "expires_at": expires_at,
    });
    let reply = daemon.call("governance", &offer);
    assert_eq!(reply["outcome"], "ok", "offer: {reply}");
    let offer_id = reply["result"]["offer_id"].as_str().unwrap().to_owned();
    let token = read_candidate_token(daemon, &offer_id);
    (offer_id, token, subject_digest)
}

pub fn read_candidate_token(daemon: &TestDaemon, offer_id: &str) -> String {
    let path = daemon
        .data_dir
        .join("channels")
        .join(format!("candidate-{offer_id}.token"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read token {}: {e}", path.display()))
        .trim()
        .to_owned()
}

/// The sole Society of a test daemon, read from its database (the
/// harness's adversary/inspection channel; the daemon owns the data
/// directory, the SQLite file is readable beside it).
pub fn sole_society_id(daemon: &TestDaemon) -> String {
    let conn = rusqlite::Connection::open(daemon.data_dir.join("byom.db")).unwrap();
    conn.query_row("SELECT society_id FROM societies LIMIT 1", [], |r| r.get(0))
        .unwrap()
}

/// The Society's immutable genesis GovernanceDecision — the authority
/// every membership offer resolves (BY-A1).
pub fn society_decision(daemon: &TestDaemon) -> String {
    format!("dec-society-{}", sole_society_id(daemon))
}

/// The immutable admission decision formed for one offer.
pub fn offer_decision(offer_id: &str) -> String {
    format!("dec-offer-{offer_id}")
}

/// The immutable admission decision formed for one Manifestation.
pub fn manifestation_decision(manifestation_id: &str) -> String {
    format!("dec-manif-{manifestation_id}")
}

/// The immutable authority decision formed for one Mandate at issue.
pub fn mandate_decision(mandate_id: &str) -> String {
    format!("dec-mandate-{mandate_id}")
}

pub fn far_future() -> String {
    "2030-01-01T00:00:00Z".to_owned()
}

pub fn accept_offer(
    daemon: &TestDaemon,
    incarnation: &str,
    token: &str,
    tag: &str,
    offer_id: &str,
    subject_digest: &Value,
    expected_revision: u64,
) -> Value {
    daemon.call_candidate(
        token,
        &json!({
            "version": "0.2", "op": "membership_accept",
            "meta": meta(incarnation, &format!("{tag}-accept"), Some(expected_revision)),
            "offer_ref": offer_id,
            "subject_digest": subject_digest,
        }),
    )
}

pub fn kind_of(reply: &Value) -> &str {
    reply["problem"]["kind"].as_str().unwrap_or("")
}

pub fn read_participant_token(daemon: &TestDaemon, participant: &str) -> String {
    let path = daemon
        .data_dir
        .join("channels")
        .join(format!("participant-{participant}.token"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read token {}: {e}", path.display()))
        .trim()
        .to_owned()
}

/// A trivially satisfiable BPA-1 policy body on the frozen AST.
pub fn bpa1_allow_all() -> Value {
    json!({"rules": [{"effect": "allow", "atoms": {}}]})
}

/// The sovereign human participant id (kind `human`) of the sole
/// Society, via snapshot_get.
pub fn sovereign_id(daemon: &TestDaemon, society_id: &str) -> String {
    let snapshot = daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "snapshot_get", "society_id": society_id,
                "kinds": ["participants"]}),
    );
    snapshot["result"]["participants"]
        .as_array()
        .expect("participants")
        .iter()
        .find(|p| p["kind"] == "human")
        .expect("sovereign human")["participant_id"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// How the governed flow exercises the §15.3 commit points.
pub enum FlowMode<'a> {
    /// Plain happy path.
    Clean,
    /// SIGKILL + restart + byte-stable replay after EVERY mutation
    /// commit point (the b1_acceptance discipline).
    RestartAfterEachCommit,
    /// BYOMD_ABORT kill at the named §15.3 boundary of the named op;
    /// restart; the exact retry must answer and replay byte-stably.
    CrashAt { op: &'a str, phase: &'a str },
}

/// What the completed flow hands back for assertions.
pub struct FlowOutcome {
    pub daemon: TestDaemon,
    pub incarnation: String,
    pub society_id: String,
    pub genesis_cursor: String,
    pub agent_token: String,
    pub sovereign: String,
    pub mandate_id: String,
    pub mandate_subject_digest: Value,
    pub terms_digest: Value,
    pub endeavor_id: String,
    pub pledge_id: String,
    pub delivery_id: String,
    pub review_id: String,
    pub exploration_stream: String,
    pub work_stream: String,
}

/// The complete B1 attached-slice governed flow: onboarding (with a
/// pre-admission candidate self-policy) → mandate chain → exploration
/// (wake intent pending, continuation head) → endeavor → call → pledge
/// seats → pledge_work → deterministic delivery → review_record.
#[allow(clippy::too_many_lines)]
pub fn governed_flow(tag: &str, mode: FlowMode) -> FlowOutcome {
    let mut daemon = match &mode {
        FlowMode::CrashAt { op, phase } => {
            let abort = format!("{phase}:{op}");
            TestDaemon::start_with_env(tag, &[("BYOMD_ABORT", &abort)])
        }
        _ => TestDaemon::start(tag),
    };
    let incarnation = daemon.incarnation();
    let mut crashed = false;

    let mut send = |daemon: &mut TestDaemon,
                    op: &str,
                    surface: &str,
                    token: Option<&str>,
                    request: &Value|
     -> Value {
        let crash_here = matches!(&mode, FlowMode::CrashAt { op: c, .. } if *c == op) && !crashed;
        if crash_here {
            let outcome = daemon.call_raw(surface, token, &request.to_string());
            assert!(
                outcome.is_err(),
                "{tag}: expected death at {op}, got {outcome:?}"
            );
            daemon.wait_exit();
            daemon.restart(&[]);
            crashed = true;
            let retried = daemon
                .call_raw(surface, token, &request.to_string())
                .unwrap_or_else(|e| panic!("{tag}: {op} retry failed: {e}"));
            assert_eq!(retried["outcome"], "ok", "{tag}: {op} retry: {retried}");
            let again = daemon
                .call_raw(surface, token, &request.to_string())
                .unwrap();
            assert_eq!(again, retried, "{tag}: {op} replay must be byte-stable");
            return retried;
        }
        let reply = daemon
            .call_raw(surface, token, &request.to_string())
            .unwrap_or_else(|e| panic!("{tag}: {op} failed: {e}\nrequest: {request}"));
        assert_eq!(reply["outcome"], "ok", "{tag}: {op}: {reply}");
        if matches!(mode, FlowMode::RestartAfterEachCommit) {
            daemon.restart(&[]);
            let again = daemon
                .call_raw(surface, token, &request.to_string())
                .unwrap_or_else(|e| panic!("{tag}: {op} post-restart replay failed: {e}"));
            assert_eq!(
                again, reply,
                "{tag}: {op} replay after kill/restart must be byte-identical"
            );
        }
        reply
    };

    // -- genesis --
    let prepared = send(
        &mut daemon,
        "society_prepare",
        "governance",
        None,
        &json!({
            "version": "0.2", "op": "society_prepare",
            "meta": meta(&incarnation, &format!("{tag}-prep"), None),
            "home_authority_ref": "auth-home-1",
            "proposed_charter_ref": "charter-draft-1",
            "proposed_charter_digest": test_digest(0xa1),
            "classification_binding_ref": "class-bind-1",
            "classification_binding_digest": test_digest(0xa2),
        }),
    );
    let society_id = prepared["result"]["society_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let booted = send(
        &mut daemon,
        "society_bootstrap",
        "governance",
        None,
        &json!({
            "version": "0.2", "op": "society_bootstrap",
            "meta": meta(&incarnation, &format!("{tag}-boot"), Some(1)),
            "society_id": society_id,
            "preparation_ref": prepared["result"]["preparation_ref"],
            "subject_digest": prepared["result"]["subject_digest"],
        }),
    );
    let genesis_cursor = booted["source_cursor"].as_str().unwrap().to_owned();

    // -- onboarding with a pre-admission candidate self-policy --
    let subject = test_digest(0xb1);
    let offered = send(
        &mut daemon,
        "membership_offer",
        "governance",
        None,
        &json!({
            "version": "0.2", "op": "membership_offer",
            "meta": meta(&incarnation, &format!("{tag}-offer"), None),
            "participant_ref": "part-agent-1",
            "proposed_standing_ref": "standing-proposal-1",
            "subject_digest": subject,
            "offered_by_decision_ref": format!("dec-society-{society_id}"),
            "expires_at": far_future(),
        }),
    );
    let offer_id = offered["result"]["offer_id"].as_str().unwrap().to_owned();
    let cand_token = read_candidate_token(&daemon, &offer_id);
    let policy_proposed = send(
        &mut daemon,
        "candidate_self_policy_propose",
        "candidate",
        Some(&cand_token),
        &json!({
            "version": "0.2", "op": "candidate_self_policy_propose",
            "meta": meta(&incarnation, &format!("{tag}-candpol"), None),
            "onboarding_ref": offer_id,
            "proposed_policy_kind": "assent",
            "proposed_policy_body": bpa1_allow_all(),
            "proposed_policy_digest": test_digest(0xb2),
            "adoption_mode": "direct_candidate",
            "adoption_control_domain_ref": "control-domain-1",
        }),
    );
    let policy_proposal_id = policy_proposed["result"]["proposal_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let accepted = send(
        &mut daemon,
        "membership_accept",
        "candidate",
        Some(&cand_token),
        &json!({
            "version": "0.2", "op": "membership_accept",
            "meta": meta(&incarnation, &format!("{tag}-accept"), Some(1)),
            "offer_ref": offer_id,
            "subject_digest": subject,
        }),
    );
    let admitted = send(
        &mut daemon,
        "participant_admit",
        "governance",
        None,
        &json!({
            "version": "0.2", "op": "participant_admit",
            "meta": meta(&incarnation, &format!("{tag}-admit"), Some(2)),
            "offer_ref": offer_id,
            "membership_acceptance_ref": accepted["result"]["acceptance_id"],
            "admitted_by_decision_ref": offer_decision(&offer_id),
            "admission_subject_digest": subject,
            "included_self_policy_proposal_refs": [policy_proposal_id],
        }),
    );
    assert_eq!(
        admitted["result"]["activated_self_policy_refs"]
            .as_array()
            .map(Vec::len),
        Some(1),
        "{tag}: the pre-admission self-policy activates exactly at admission"
    );
    let events = daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "events_read",
                "continuation": genesis_cursor, "page_size": 512}),
    );
    let manifestation_id = events["result"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["kind"] == "manifestation.proposed")
        .expect("manifestation.proposed")["object_ref"]
        .as_str()
        .unwrap()
        .to_owned();
    send(
        &mut daemon,
        "manifestation_admit",
        "governance",
        None,
        &json!({
            "version": "0.2", "op": "manifestation_admit",
            "meta": meta(&incarnation, &format!("{tag}-manif"), Some(1)),
            "manifestation_ref": manifestation_id,
            "admitted_by_decision_ref": manifestation_decision(&manifestation_id),
        }),
    );
    let agent_token = read_participant_token(&daemon, "part-agent-1");
    let sovereign = sovereign_id(&daemon, &society_id);

    // -- the mandate chain before any non-pledged ActivityStream --
    let mandate_prepared = send(
        &mut daemon,
        "mandate_prepare",
        "participant",
        Some(&agent_token),
        &json!({
            "version": "0.2", "op": "mandate_prepare",
            "meta": meta(&incarnation, &format!("{tag}-mprep"), None),
            "grantee_participant_ref": "part-agent-1",
            "purpose_ref": "purpose-explore-1",
            "allowed_operations": ["activity_open", "continuation_write",
                                   "wake_intent_submit"],
            "resource_selectors": ["res-repo-1"],
            "data_class_selectors": ["class-public"],
            "destination_selectors": [],
            "budget_ceiling_set_ref": "budget-mandate-1",
            "concurrency_ceiling": 2,
            "delegation": {"allowed": false, "max_depth": 0, "max_children": 0,
                           "grantee_selectors": []},
            "expires_at": far_future(),
        }),
    );
    let mandate_id = mandate_prepared["result"]["mandate_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let mandate_seat = mandate_prepared["result"]["required_seat_refs"][0]
        .as_str()
        .unwrap()
        .to_owned();
    send(
        &mut daemon,
        "mandate_position",
        "governance",
        None,
        &json!({
            "version": "0.2", "op": "mandate_position",
            "meta": meta(&incarnation, &format!("{tag}-mpos"), None),
            "proposal_ref": mandate_id,
            "proposal_revision": 1,
            "subject_digest": mandate_prepared["result"]["subject_digest"],
            "seat_ref": mandate_seat,
            "value": "assent",
        }),
    );
    send(
        &mut daemon,
        "mandate_issue",
        "governance",
        None,
        &json!({
            "version": "0.2", "op": "mandate_issue",
            "meta": meta(&incarnation, &format!("{tag}-missue"), Some(1)),
            "mandate_id": mandate_id,
            "subject_digest": mandate_prepared["result"]["subject_digest"],
        }),
    );

    // -- exploration under the mandate; wake intent accepted-and-pending;
    //    continuation head CAS --
    let exploration = send(
        &mut daemon,
        "activity_open",
        "participant",
        Some(&agent_token),
        &json!({
            "version": "0.2", "op": "activity_open",
            "meta": meta(&incarnation, &format!("{tag}-explore"), None),
            "kind": "exploration",
            "purpose_ref": "purpose-explore-1",
            "purpose_digest": test_digest(0xc0),
            "mandate_refs": [mandate_id],
            "budget_account_set_ref": "budget-mandate-1",
        }),
    );
    let exploration_stream = exploration["result"]["activity_stream_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let wake = send(
        &mut daemon,
        "wake_intent_submit",
        "participant",
        Some(&agent_token),
        &json!({
            "version": "0.2", "op": "wake_intent_submit",
            "meta": meta(&incarnation, &format!("{tag}-wake"), None),
            "activity_stream_ref": exploration_stream,
            "generation": 1,
            "origin": "direct_participant",
            "exact_cause_ref": "cause-followup-1",
            "exact_cause_digest": test_digest(0xc2),
            "purpose_ref": "purpose-explore-1",
            "stable_wake_key": format!("wake-{tag}"),
            "expires_at": far_future(),
        }),
    );
    assert_eq!(
        wake["result"]["state"], "submitted",
        "{tag}: wake intent accepted and left pending (no activation machinery)"
    );
    send(
        &mut daemon,
        "continuation_write",
        "participant",
        Some(&agent_token),
        &json!({
            "version": "0.2", "op": "continuation_write",
            "meta": meta(&incarnation, &format!("{tag}-cont"), None),
            "activity_stream_ref": exploration_stream,
            "generation": 1,
            "summary_ref": "summary-explore-1",
            "unresolved_refs": [],
            "exact_state_refs": ["state-notes-1"],
            "source_event_cursor": "cursor-ref-1",
            "expected_head_revision": 0,
            "classification_ref": "class-participant-private",
        }),
    );

    // -- endeavor → call --
    let endeavor_proposed = send(
        &mut daemon,
        "endeavor_propose",
        "participant",
        None,
        &json!({
            "version": "0.2", "op": "endeavor_propose",
            "meta": meta(&incarnation, &format!("{tag}-eprop"), None),
            "purpose_ref": "purpose-improve-1",
            "purpose_digest": test_digest(0xd0),
            "sponsor_participant_refs": [sovereign],
            "governance_rule_set_ref": "rules-endeavor-1",
            "outcome_schema_refs": ["schema-change-set-1"],
            "acceptance_rule_ref": "rule-accept-1",
            "classification_join_ref": "class-join-1",
            "budget_account_set_ref": format!("budget-endeavor-{tag}"),
        }),
    );
    let endeavor_id = endeavor_proposed["result"]["endeavor_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let sponsor_seat = endeavor_proposed["result"]["required_seat_refs"][0]
        .as_str()
        .unwrap()
        .to_owned();
    send(
        &mut daemon,
        "endeavor_position",
        "participant",
        None,
        &json!({
            "version": "0.2", "op": "endeavor_position",
            "meta": meta(&incarnation, &format!("{tag}-epos"), None),
            "proposal_ref": endeavor_id,
            "proposal_revision": 1,
            "subject_digest": endeavor_proposed["result"]["subject_digest"],
            "seat_ref": sponsor_seat,
            "value": "assent",
        }),
    );
    send(
        &mut daemon,
        "endeavor_finalize",
        "participant",
        None,
        &json!({
            "version": "0.2", "op": "endeavor_finalize",
            "meta": meta(&incarnation, &format!("{tag}-efin"), Some(1)),
            "endeavor_id": endeavor_id,
            "subject_digest": endeavor_proposed["result"]["subject_digest"],
        }),
    );
    let call = send(
        &mut daemon,
        "call_open",
        "participant",
        None,
        &json!({
            "version": "0.2", "op": "call_open",
            "meta": meta(&incarnation, &format!("{tag}-call"), None),
            "endeavor_id": endeavor_id,
            "requested_outcome_schema_refs": ["schema-change-set-1"],
            "acceptance_criteria_refs": ["criteria-review-1"],
            "evidence_requirements": [],
        }),
    );
    let call_id = call["result"]["call_id"].as_str().unwrap().to_owned();

    // -- pledge: propose → both seats assent → finalize --
    let pledge_proposed = send(
        &mut daemon,
        "pledge_propose",
        "participant",
        Some(&agent_token),
        &json!({
            "version": "0.2", "op": "pledge_propose",
            "meta": meta(&incarnation, &format!("{tag}-pprop"), None),
            "endeavor_id": endeavor_id,
            "call_ref": call_id,
            "proposed_pledgor_ref": "part-agent-1",
            "beneficiary_ref": sovereign,
            "exact_outcome_schema_refs": ["schema-change-set-1"],
            "acceptance_criteria_refs": ["criteria-review-1"],
            "evidence_requirements": [],
            "reviewer_rule_ref": "rule-beneficiary-reviews",
            "input_context_ref": "context-input-1",
            "input_context_digest": test_digest(0xd2),
            "budget_request_set": {"items": [
                {"dimension": "unit", "canonical_unit": "unit",
                 "scale": 0, "max": 16}]},
            "allowed_manifestation_selector": bpa1_allow_all(),
            "delegation_ceiling": {"allowed": false, "max_depth": 0,
                                   "max_children": 0},
            "deadline": far_future(),
            "cancellation_terms": {"terms_ref": "terms-cancel-1",
                                   "terms_digest": test_digest(0xd3)},
            "dependency_refs": [],
        }),
    );
    let proposal_id = pledge_proposed["result"]["proposal_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let terms_digest = pledge_proposed["result"]["terms_digest"].clone();
    let slot_seat = |kind: &str| -> String {
        pledge_proposed["result"]["required_slots"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == kind)
            .unwrap_or_else(|| panic!("slot {kind}"))["seat_refs"][0]
            .as_str()
            .unwrap()
            .to_owned()
    };
    send(
        &mut daemon,
        "pledge_position",
        "participant",
        Some(&agent_token),
        &json!({
            "version": "0.2", "op": "pledge_position",
            "meta": meta(&incarnation, &format!("{tag}-ppos-agent"), None),
            "proposal_ref": proposal_id,
            "proposal_revision": 1,
            "subject_digest": terms_digest,
            "seat_ref": slot_seat("pledgor_assent"),
            "value": "assent",
            "assent_mode": "direct_participant",
        }),
    );
    send(
        &mut daemon,
        "pledge_position",
        "participant",
        None,
        &json!({
            "version": "0.2", "op": "pledge_position",
            "meta": meta(&incarnation, &format!("{tag}-ppos-sov"), None),
            "proposal_ref": proposal_id,
            "proposal_revision": 1,
            "subject_digest": terms_digest,
            "seat_ref": slot_seat("beneficiary_assent"),
            "value": "assent",
            "assent_mode": "direct_participant",
        }),
    );
    let pledge_finalized = send(
        &mut daemon,
        "pledge_finalize",
        "participant",
        Some(&agent_token),
        &json!({
            "version": "0.2", "op": "pledge_finalize",
            "meta": meta(&incarnation, &format!("{tag}-pfin"), Some(1)),
            "proposal_ref": proposal_id,
            "proposal_revision": 1,
            "subject_digest": terms_digest,
        }),
    );
    let pledge_id = pledge_finalized["result"]["pledge_id"]
        .as_str()
        .unwrap()
        .to_owned();

    // -- pledged work → deterministic delivery → review --
    let work = send(
        &mut daemon,
        "activity_open",
        "participant",
        Some(&agent_token),
        &json!({
            "version": "0.2", "op": "activity_open",
            "meta": meta(&incarnation, &format!("{tag}-work"), None),
            "kind": "pledge_work",
            "purpose_ref": "purpose-improve-1",
            "purpose_digest": test_digest(0xd4),
            "pledge_binding": {"pledge_id": pledge_id, "pledge_revision": 1,
                               "terms_digest": terms_digest},
            "mandate_refs": [],
            "budget_account_set_ref": format!("budget-endeavor-{tag}"),
        }),
    );
    let work_stream = work["result"]["activity_stream_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let delivered = send(
        &mut daemon,
        "delivery_submit",
        "participant",
        Some(&agent_token),
        &json!({
            "version": "0.2", "op": "delivery_submit",
            "meta": meta(&incarnation, &format!("{tag}-deliver"), None),
            "pledge_id": pledge_id,
            "pledge_revision": 2,
            "terms_digest": terms_digest,
            "output_refs": ["change-set-1"],
            "evidence_refs": ["attest-complete-readable-source-1"],
            "activity_stream_ref": work_stream,
        }),
    );
    let delivery_id = delivered["result"]["delivery_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let reviewed = send(
        &mut daemon,
        "review_record",
        "participant",
        None,
        &json!({
            "version": "0.2", "op": "review_record",
            "meta": meta(&incarnation, &format!("{tag}-review"), None),
            "pledge_id": pledge_id,
            "pledge_revision": delivered["result"]["pledge_revision"],
            "delivery_id": delivery_id,
            "reviewed_subject_digest": delivered["result"]["subject_digest"],
            "outcome": "fulfilled",
            "decision_or_mandate_use_ref": "dec-review-1",
        }),
    );
    let review_id = reviewed["result"]["review_id"].as_str().unwrap().to_owned();
    assert_eq!(reviewed["result"]["pledge_state"], "fulfilled", "{tag}");

    #[allow(clippy::drop_non_drop)]
    drop(send); // ends the closure's &mut borrow of `crashed`
    if let FlowMode::CrashAt { op, .. } = &mode {
        assert!(crashed, "{tag}: the flow never reached crash op {op}");
    }

    FlowOutcome {
        daemon,
        incarnation,
        society_id,
        genesis_cursor,
        agent_token,
        sovereign,
        mandate_id,
        mandate_subject_digest: mandate_prepared["result"]["subject_digest"].clone(),
        terms_digest,
        endeavor_id,
        pledge_id,
        delivery_id,
        review_id,
        exploration_stream,
        work_stream,
    }
}
