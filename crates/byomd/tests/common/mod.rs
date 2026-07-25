//! Shared harness for the byomd integration suites: spawns the real
//! daemon binary against throwaway data/runtime dirs, speaks the wire
//! protocol over the real per-surface sockets, and can kill/restart the
//! process (the crash matrix) or restore a database snapshot (the
//! rollback adversary).

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

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
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        for (k, v) in env {
            cmd.env(k, v);
        }
        let child = cmd.spawn().expect("spawn byomd");
        let daemon = TestDaemon {
            child: Some(child),
            data_dir,
            run_dir,
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
    /// `token` is the candidate-channel preamble.
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
            stream
                .write_all(format!("{token}\n").as_bytes())
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
        "offered_by_decision_ref": "dec-offer-1",
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
