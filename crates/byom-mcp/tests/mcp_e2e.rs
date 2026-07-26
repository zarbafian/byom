//! End-to-end MCP tests: spawn a real `byomd`, run the `byom-mcp`
//! binary as a child over stdio pipes, and drive the MCP conversation —
//! initialize, tools/list against the document, tools/call against the
//! live daemon (the REAL candidate acceptance flow, then the
//! participant profile with the admitted agent's minted credential),
//! and the refusal paths (deny-by-absence, envelope/channel-derived
//! fields in tool input, the profile split).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{BufRead as _, BufReader, Write as _};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

/// The contract the server must expose, loaded independently here.
const DOCUMENT_JSON: &str = include_str!("../../../mcp/byom-mcp.tools.json");

fn profile_tools(profile: &str) -> Vec<Value> {
    let doc: Value = serde_json::from_str(DOCUMENT_JSON).unwrap();
    doc["profiles"][profile]["tools"]
        .as_array()
        .unwrap()
        .clone()
}

fn tmp(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// `byomd` lives in another crate, so no `CARGO_BIN_EXE_byomd` here;
/// resolve it next to this test binary (built by `cargo test
/// --workspace`, which run-checks.sh uses).
fn byomd_bin() -> PathBuf {
    let mut path = std::env::current_exe().unwrap();
    path.pop(); // deps/
    path.pop(); // debug/
    let bin = path.join("byomd");
    assert!(
        bin.exists(),
        "byomd binary not found at {}; run `cargo test --workspace` \
         (or `cargo build -p byomd`) first",
        bin.display()
    );
    bin
}

// ------------------------------------------------------------- daemon ----

struct DaemonProc {
    child: Child,
    data_dir: PathBuf,
    run_dir: PathBuf,
}

impl DaemonProc {
    fn start(data_dir: &Path, run_dir: &Path) -> DaemonProc {
        let child = Command::new(byomd_bin())
            .env("BYOM_DATA_DIR", data_dir)
            .env("BYOM_RUNTIME_DIR", run_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn byomd");
        let daemon = DaemonProc {
            child,
            data_dir: data_dir.to_path_buf(),
            run_dir: run_dir.to_path_buf(),
        };
        let deadline = Instant::now() + Duration::from_secs(15);
        let surfaces = ["governance", "candidate", "participant", "projection"];
        'wait: loop {
            for surface in surfaces {
                if UnixStream::connect(daemon.socket(surface)).is_err() {
                    assert!(
                        Instant::now() < deadline,
                        "byomd sockets never came up in {}",
                        daemon.run_dir.display()
                    );
                    std::thread::sleep(Duration::from_millis(25));
                    continue 'wait;
                }
            }
            return daemon;
        }
    }

    fn socket(&self, surface: &str) -> PathBuf {
        self.run_dir.join(format!("{surface}.sock"))
    }

    /// One request line, one reply line, over the named surface socket;
    /// `token` is the channel-credential preamble.
    fn call(&self, surface: &str, token: Option<&str>, request: &Value) -> Value {
        let mut stream = UnixStream::connect(self.socket(surface)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(20)))
            .unwrap();
        let mut line = String::new();
        if let Some(token) = token {
            // A channel credential presents a FRESH sender-constrained
            // proof per call (BY-C1), exactly as a real client does.
            let preamble = if token.starts_with("bpk1.") {
                // CLAIM the channel for this process, then MAC the exact
                // call under the peer-bound key byomd issued (BY-C1):
                // the credential file itself carries no key material.
                let key = byomd::channel::claim(&self.run_dir, token).expect("claim channel");
                byomd::channel::mint_proof(
                    token,
                    &key,
                    request["op"].as_str().unwrap_or_default(),
                    byomd::channel::Peer::current(),
                    bpp_core::time::unix_now(),
                )
                .expect("mint channel proof")
            } else {
                token.to_owned()
            };
            line.push_str(&preamble);
            line.push('\n');
        }
        line.push_str(&request.to_string());
        line.push('\n');
        stream.write_all(line.as_bytes()).unwrap();
        let mut reply = String::new();
        BufReader::new(stream).read_line(&mut reply).unwrap();
        serde_json::from_str(reply.trim_end()).unwrap()
    }

    fn expect_ok(&self, surface: &str, token: Option<&str>, request: &Value) -> Value {
        let reply = self.call(surface, token, request);
        assert_eq!(reply["outcome"], "ok", "expected ok, got {reply}");
        reply
    }

    fn incarnation(&self) -> String {
        let reply = self.expect_ok(
            "governance",
            None,
            &json!({"version": "0.2", "op": "hello"}),
        );
        reply["result"]["endpoint_incarnation"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    fn channel_token(&self, file: &str) -> String {
        let path = self.data_dir.join("channels").join(file);
        std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read token {}: {e}", path.display()))
            .trim()
            .to_owned()
    }
}

impl Drop for DaemonProc {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// -------------------------------------------------- request builders ----

fn meta(incarnation: &str, key: &str, expected_revision: Option<u64>) -> Value {
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

fn test_digest(seed: u8) -> Value {
    json!({
        "class": "local_erasure_safe",
        "algorithm": "hmac-sha-256",
        "key_ref": format!("test-key-{seed}"),
        "value_hex": format!("{seed:02x}").repeat(32),
    })
}

/// Prepares and bootstraps one Society; returns
/// (society_id, genesis_cursor, incarnation).
fn bootstrap_society(daemon: &DaemonProc) -> (String, String, String) {
    let incarnation = daemon.incarnation();
    let prepared = daemon.expect_ok(
        "governance",
        None,
        &json!({
            "version": "0.2", "op": "society_prepare",
            "meta": meta(&incarnation, "e2e-prep", None),
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
    let booted = daemon.expect_ok(
        "governance",
        None,
        &json!({
            "version": "0.2", "op": "society_bootstrap",
            "meta": meta(&incarnation, "e2e-boot", Some(1)),
            "society_id": society_id,
            "preparation_ref": prepared["result"]["preparation_ref"],
            "subject_digest": prepared["result"]["subject_digest"],
        }),
    );
    let cursor = booted["source_cursor"].as_str().unwrap().to_owned();
    (society_id, cursor, incarnation)
}

/// Creates one membership offer for `participant`; returns
/// (offer_id, subject_digest) — the candidate channel token file appears
/// under `<data-dir>/channels/candidate-<offer_id>.token`.
fn make_offer(
    daemon: &DaemonProc,
    incarnation: &str,
    society_id: &str,
    participant: &str,
) -> (String, Value) {
    let subject_digest = test_digest(0xb1);
    let reply = daemon.expect_ok(
        "governance",
        None,
        &json!({
            "version": "0.2", "op": "membership_offer",
            "meta": meta(incarnation, "e2e-offer", None),
            "participant_ref": participant,
            "proposed_standing_ref": "standing-proposal-1",
            "subject_digest": subject_digest,
            "offered_by_decision_ref": format!("dec-society-{society_id}"),
            "expires_at": "2030-01-01T00:00:00Z",
        }),
    );
    let offer_id = reply["result"]["offer_id"].as_str().unwrap().to_owned();
    (offer_id, subject_digest)
}

/// Governance side of onboarding after the candidate accepted: admit
/// the participant (offer revision 2), admit its proposed
/// manifestation, and hand back the minted participant-channel token.
fn admit_participant(
    daemon: &DaemonProc,
    incarnation: &str,
    genesis_cursor: &str,
    offer_id: &str,
    acceptance_id: &str,
    subject_digest: &Value,
    participant: &str,
) -> String {
    daemon.expect_ok(
        "governance",
        None,
        &json!({
            "version": "0.2", "op": "participant_admit",
            "meta": meta(incarnation, "e2e-admit", Some(2)),
            "offer_ref": offer_id,
            "membership_acceptance_ref": acceptance_id,
            "admitted_by_decision_ref": format!("dec-offer-{offer_id}"),
            "admission_subject_digest": subject_digest,
        }),
    );
    let events = daemon.expect_ok(
        "projection",
        None,
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
    daemon.expect_ok(
        "governance",
        None,
        &json!({
            "version": "0.2", "op": "manifestation_admit",
            "meta": meta(incarnation, "e2e-manif", Some(1)),
            "manifestation_ref": manifestation_id,
            "admitted_by_decision_ref": format!("dec-manif-{manifestation_id}"),
        }),
    );
    daemon.channel_token(&format!("participant-{participant}.token"))
}

/// The §11.1 mandate chain before any non-pledged ActivityStream:
/// prepare (the AGENT, over its own MCP session), assent (governance
/// seat), issue (governance).
///
/// The prepare runs through the agent's MCP server rather than from this
/// test process, because a byom channel is held by exactly ONE LIVE
/// process (BY-C1): the agent's session is the holder, so the harness
/// must not claim the same participant channel behind its back.
fn issue_mandate(
    daemon: &DaemonProc,
    incarnation: &str,
    agent: &mut McpServer,
    participant: &str,
    purpose_ref: &str,
) -> String {
    let prepared = agent.call_ok(
        "byom_mandate_prepare",
        json!({
            "grantee_participant_ref": participant,
            "purpose_ref": purpose_ref,
            "allowed_operations": ["activity_open", "continuation_write",
                                   "wake_intent_submit"],
            "resource_selectors": ["res-repo-1"],
            "data_class_selectors": ["class-public"],
            "destination_selectors": [],
            "budget_ceiling_set_ref": "budget-mandate-1",
            "concurrency_ceiling": 2,
            "delegation": {"allowed": false, "max_depth": 0, "max_children": 0,
                           "grantee_selectors": []},
            "expires_at": "2030-01-01T00:00:00Z",
        }),
    );
    let mandate_id = prepared["result"]["mandate_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let seat = prepared["result"]["required_seat_refs"][0]
        .as_str()
        .unwrap()
        .to_owned();
    daemon.expect_ok(
        "governance",
        None,
        &json!({
            "version": "0.2", "op": "mandate_position",
            "meta": meta(incarnation, "e2e-mpos", None),
            "proposal_ref": mandate_id,
            "proposal_revision": 1,
            "subject_digest": prepared["result"]["subject_digest"],
            "seat_ref": seat,
            "value": "assent",
        }),
    );
    daemon.expect_ok(
        "governance",
        None,
        &json!({
            "version": "0.2", "op": "mandate_issue",
            "meta": meta(incarnation, "e2e-missue", Some(1)),
            "mandate_id": mandate_id,
            "subject_digest": prepared["result"]["subject_digest"],
        }),
    );
    mandate_id
}

// --------------------------------------------------------- MCP server ----

struct McpServer {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl McpServer {
    /// Spawns `byom-mcp --profile <profile>` with a scrubbed channel
    /// environment plus `env`.
    fn start(runtime_dir: &Path, profile: &str, env: &[(&str, &str)]) -> McpServer {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_byom-mcp"));
        cmd.args(["--profile", profile])
            .env("BYOM_RUNTIME_DIR", runtime_dir)
            .env_remove("BYOM_MCP_PROFILE")
            .env_remove("BYOM_CANDIDATE_TOKEN")
            .env_remove("BYOM_CANDIDATE_TOKEN_FILE")
            .env_remove("BYOM_PARTICIPANT_TOKEN")
            .env_remove("BYOM_PARTICIPANT_TOKEN_FILE")
            .env_remove("BYOM_SOCIETY")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        for (key, value) in env {
            cmd.env(key, value);
        }
        let mut child = cmd.spawn().expect("spawn byom-mcp");
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        McpServer {
            child,
            stdin,
            stdout,
            next_id: 0,
        }
    }

    /// One JSON-RPC request, one response; asserts the ids match (so a
    /// stray reply to a notification would be caught).
    fn rpc(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        let mut line =
            json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string();
        line.push('\n');
        self.stdin.write_all(line.as_bytes()).unwrap();
        self.stdin.flush().unwrap();
        let mut reply = String::new();
        self.stdout.read_line(&mut reply).unwrap();
        assert!(!reply.is_empty(), "server closed the stream on {method}");
        let parsed: Value = serde_json::from_str(reply.trim_end()).unwrap();
        assert_eq!(parsed["id"].as_u64(), Some(id), "got {parsed}");
        parsed
    }

    fn notify(&mut self, method: &str) {
        let mut line = json!({"jsonrpc": "2.0", "method": method}).to_string();
        line.push('\n');
        self.stdin.write_all(line.as_bytes()).unwrap();
        self.stdin.flush().unwrap();
    }

    fn initialize(&mut self) {
        let reply = self.rpc(
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "mcp_e2e", "version": "0"},
            }),
        );
        assert_eq!(
            reply["result"]["protocolVersion"].as_str(),
            Some("2025-06-18")
        );
        assert_eq!(
            reply["result"]["serverInfo"]["name"].as_str(),
            Some("byom-mcp")
        );
        self.notify("notifications/initialized");
    }

    fn tools(&mut self) -> Vec<Value> {
        let reply = self.rpc("tools/list", json!({}));
        reply["result"]["tools"].as_array().unwrap().clone()
    }

    /// A tools/call, returning `(text, is_error)`.
    fn call(&mut self, name: &str, arguments: Value) -> (String, bool) {
        let reply = self.rpc("tools/call", json!({"name": name, "arguments": arguments}));
        let result = &reply["result"];
        let text = result["content"][0]["text"].as_str().unwrap().to_owned();
        (text, result["isError"].as_bool().unwrap())
    }

    /// A tools/call that must succeed; returns the parsed reply
    /// (`result` plus `revision`/`source_cursor` when minted).
    fn call_ok(&mut self, name: &str, arguments: Value) -> Value {
        let (text, is_error) = self.call(name, arguments);
        assert!(!is_error, "{name}: {text}");
        serde_json::from_str(&text).unwrap()
    }
}

impl Drop for McpServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// tools/list must be the named profile section of the document,
/// verbatim and in order, with the access flag surfaced as readOnlyHint.
fn assert_tools_match_document(listed: &[Value], profile: &str, expected_count: usize) {
    let expected = profile_tools(profile);
    assert_eq!(listed.len(), expected_count, "{profile} tool count");
    assert_eq!(listed.len(), expected.len());
    for (tool, row) in listed.iter().zip(&expected) {
        let name = row["name"].as_str().unwrap();
        assert_eq!(tool["name"].as_str(), Some(name), "name/order drift");
        // Description verbatim — it carries the gated marking.
        assert_eq!(
            tool["description"].as_str(),
            row["description"].as_str(),
            "{name} description drift"
        );
        // Input schema verbatim from the document.
        assert_eq!(
            tool["inputSchema"], row["input_schema"],
            "{name} schema drift"
        );
        // Gated marking: access flag ⇔ readOnlyHint ⇔ description text.
        let gated = row["access"].as_str().unwrap() == "gated";
        assert_eq!(
            tool["annotations"]["readOnlyHint"].as_bool(),
            Some(!gated),
            "{name} readOnlyHint drift"
        );
        assert_eq!(
            tool["description"].as_str().unwrap().contains("gated"),
            gated,
            "{name} description gating marking drift"
        );
    }
}

// -------------------------------------------------------------- tests ----

/// tools/list is the document, per profile — and the refusal paths need
/// no daemon: they must trigger before any socket dial.
#[test]
fn tools_list_matches_the_document_and_refusals_precede_dispatch() {
    // No byomd runs behind this runtime dir.
    let runtime = tmp("mcp-list-runtime");

    // Candidate profile: exactly the 3 sender-constrained tools.
    let mut candidate = McpServer::start(
        &runtime,
        "candidate",
        &[("BYOM_CANDIDATE_TOKEN", "tok-e2e")],
    );
    candidate.initialize();
    assert_tools_match_document(&candidate.tools(), "candidate", 3);

    // Deny-by-absence: a governance op is not a tool, and a PARTICIPANT
    // tool does not exist under the candidate binding (A4: never both).
    for absent in [
        "byom_participant_admit",
        "byom_activity_open",
        "byom_society_show",
    ] {
        let (text, is_error) = candidate.call(absent, json!({}));
        assert!(is_error);
        assert!(text.contains("unknown tool"), "{text}");
        assert!(text.contains("deny-by-absence"), "{text}");
    }

    // Channel-derived fields in tool input are refused before dispatch
    // (no daemon is even reachable — the refusal is local).
    let (text, is_error) = candidate.call(
        "byom_membership_accept",
        json!({"offer_ref": "offer-1", "subject_digest": test_digest(0xb1),
               "actor_ref": "candidate:chan-1"}),
    );
    assert!(is_error);
    assert!(text.contains("actor_ref"), "{text}");
    assert!(text.contains("closed shape"), "{text}");
    // The bridge-derived envelope cannot ride in either.
    let (text, is_error) = candidate.call(
        "byom_membership_accept",
        json!({"offer_ref": "offer-1", "subject_digest": test_digest(0xb1),
               "meta": {"idempotency_key": "mine"}}),
    );
    assert!(is_error);
    assert!(text.contains("meta"), "{text}");
    drop(candidate);

    // Candidate mode without the offer token refuses to serve at all.
    let refused = Command::new(env!("CARGO_BIN_EXE_byom-mcp"))
        .args(["--profile", "candidate"])
        .env("BYOM_RUNTIME_DIR", &runtime)
        .env_remove("BYOM_CANDIDATE_TOKEN")
        .env_remove("BYOM_CANDIDATE_TOKEN_FILE")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(
        !refused.success(),
        "candidate mode must refuse to serve without the offer token"
    );

    // Participant profile: exactly the 34 tools; the candidate tools do
    // not exist here.
    let mut participant = McpServer::start(&runtime, "participant", &[]);
    participant.initialize();
    assert_tools_match_document(&participant.tools(), "participant", 34);
    let (text, is_error) = participant.call("byom_membership_accept", json!({}));
    assert!(is_error);
    assert!(text.contains("deny-by-absence"), "{text}");
    let (text, is_error) = participant.call(
        "byom_activity_show",
        json!({"activity_stream_ref": "act-1", "actor_ref": "participant:p-1"}),
    );
    assert!(is_error);
    assert!(text.contains("actor_ref"), "{text}");
    assert!(text.contains("closed shape"), "{text}");
}

/// The real C3a flow against a live daemon: the candidate accepts its
/// one MembershipOffer over MCP (envelope, token preamble, and the
/// update-class expected_revision all bridge-derived), governance
/// admits, and the admitted agent works the participant profile —
/// activity_open and wake_intent_submit under a real mandate chain.
#[test]
fn candidate_accept_then_participant_work_over_mcp() {
    let data = tmp("mcp-e2e-data");
    let runtime = tmp("mcp-e2e-runtime");
    let daemon = DaemonProc::start(&data, &runtime);
    let (society_id, genesis_cursor, incarnation) = bootstrap_society(&daemon);
    let participant = "part-agent-1";
    let (offer_id, subject_digest) = make_offer(&daemon, &incarnation, &society_id, participant);
    let token_file = data
        .join("channels")
        .join(format!("candidate-{offer_id}.token"));

    // -- candidate profile: the offer-scoped binding, token from the
    //    minted file --
    let mut candidate = McpServer::start(
        &runtime,
        "candidate",
        &[("BYOM_CANDIDATE_TOKEN_FILE", &token_file.to_string_lossy())],
    );
    candidate.initialize();
    assert_eq!(candidate.tools().len(), 3, "exactly the 3 candidate tools");

    // ACCEPT the exact offer: version/op/meta are bridge-derived (fresh
    // idempotency key, hello incarnation, the open-channel revision
    // pin); the tool input is exactly the op's caller-supplied args.
    let accepted = candidate.call_ok(
        "byom_membership_accept",
        json!({"offer_ref": offer_id, "subject_digest": subject_digest}),
    );
    assert_eq!(accepted["result"]["offer_state"], "accepted");
    let acceptance_id = accepted["result"]["acceptance_id"]
        .as_str()
        .unwrap()
        .to_owned();
    // The CAS passed against the derived expected_revision 1: the offer
    // moved to revision 2.
    assert_eq!(accepted["revision"].as_u64(), Some(2));

    // MCP-1 / D-R1-3: the idempotency key is the LOGICAL-CALL key
    // derived from (tool, canonical input, session), so an AMBIGUOUS
    // TRANSPORT RETRY of the identical call reuses it and replays the
    // retained receipt — it does NOT mint a second acceptance. (With
    // the withdrawn fresh-random-key-per-invocation behaviour this
    // answered stale_revision, i.e. a second command had been formed.)
    let retried = candidate.call_ok(
        "byom_membership_accept",
        json!({"offer_ref": offer_id, "subject_digest": subject_digest}),
    );
    assert_eq!(
        retried, accepted,
        "an ambiguous retry replays the SAME receipt, byte-identically"
    );
    // Exactly ONE acceptance exists for the offer.
    let accepted_events = daemon.expect_ok(
        "projection",
        None,
        &json!({"version": "0.2", "op": "events_read",
                "continuation": genesis_cursor, "page_size": 512}),
    );
    let acceptances = accepted_events["result"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == "membership.accepted")
        .count();
    assert_eq!(acceptances, 1, "one logical call, one committed effect");

    // -- governance admits; the participant channel credential is
    //    minted at participant_admit --
    let agent_token = admit_participant(
        &daemon,
        &incarnation,
        &genesis_cursor,
        &offer_id,
        &acceptance_id,
        &subject_digest,
        participant,
    );

    // Admission closed the candidate channel server-side (A4: it never
    // converts in place); anything further over it — here a §7.4
    // retraction citing the acceptance — is non-enumerating forbidden.
    let (text, is_error) = candidate.call(
        "byom_membership_refuse",
        json!({"offer_ref": offer_id, "offer_subject_digest": subject_digest,
               "superseded_acceptance_ref": acceptance_id}),
    );
    assert!(is_error);
    assert!(
        text.contains("https://byom.dev/problems/forbidden"),
        "{text}"
    );
    drop(candidate);

    // -- participant profile: the admitted agent's sender-constrained
    //    channel --
    let mut agent = McpServer::start(
        &runtime,
        "participant",
        &[("BYOM_PARTICIPANT_TOKEN", agent_token.as_str())],
    );
    agent.initialize();
    assert_eq!(agent.tools().len(), 34, "exactly the 34 participant tools");
    let mandate_id = issue_mandate(
        &daemon,
        &incarnation,
        &mut agent,
        participant,
        "purpose-explore-1",
    );

    // Mutation: activity_open under the mandate (create-class meta,
    // participant socket, token preamble).
    let opened = agent.call_ok(
        "byom_activity_open",
        json!({
            "kind": "exploration",
            "purpose_ref": "purpose-explore-1",
            "purpose_digest": test_digest(0xc0),
            "mandate_refs": [mandate_id],
            "budget_account_set_ref": "budget-mandate-1",
        }),
    );
    let stream_id = opened["result"]["activity_stream_id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(opened["result"]["state"], "ready");
    assert!(
        opened["source_cursor"].as_str().is_some(),
        "the events cursor reaches the agent: {opened}"
    );

    // Mutation: wake_intent_submit — accepted and left pending (§11.1,
    // no activation machinery in this slice).
    let wake = agent.call_ok(
        "byom_wake_intent_submit",
        json!({
            "activity_stream_ref": stream_id,
            "generation": 1,
            "origin": "direct_participant",
            "exact_cause_ref": "cause-followup-1",
            "exact_cause_digest": test_digest(0xc2),
            "purpose_ref": "purpose-explore-1",
            "stable_wake_key": "wake-mcp-e2e-1",
            "expires_at": "2030-01-01T00:00:00Z",
        }),
    );
    assert_eq!(wake["result"]["state"], "submitted");

    // Read: the append landed — activity_show routes to the projection
    // socket (no credential preamble there).
    let shown = agent.call_ok(
        "byom_activity_show",
        json!({"activity_stream_ref": stream_id}),
    );
    assert_eq!(shown["result"]["kind"], "exploration");
    assert_eq!(shown["result"]["participant_ref"], participant);
    assert_eq!(shown["result"]["revision"].as_u64(), Some(1));

    // MCP-1, THE CONFIRMATION'S OWN PROBE: a LOST COMMITTED REPLY on an
    // update whose `expected_revision` the bridge DERIVES from live
    // state. `activity_hold` reads the stream's revision from
    // `activity_show`; the hold commits and moves it, so the retry
    // derives a DIFFERENT expected_revision for the same logical call.
    // The R1 fix hashed that derived CAS token into the covered request,
    // so the retry answered `idempotency_mismatch` — the one situation
    // the retained receipt exists for. (The earlier test used
    // `membership_accept`, whose derivation is the constant 1, and so
    // never exercised this at all.)
    let hold_args = json!({"activity_stream_ref": stream_id, "generation": 1,
                           "hold_reason_ref": "reason-pause-1"});
    let held = agent.call_ok("byom_activity_hold", hold_args.clone());
    assert_eq!(held["result"]["state"], "held", "{held}");
    // The commit moved the revision the bridge derives from.
    let moved = agent.call_ok(
        "byom_activity_show",
        json!({"activity_stream_ref": stream_id}),
    );
    assert_eq!(
        moved["result"]["revision"].as_u64(),
        Some(2),
        "the derived CAS token is now different: {moved}"
    );
    // The reply above stands in for the one that was lost: the caller
    // retries the IDENTICAL tool call.
    let retried_hold = agent.call_ok("byom_activity_hold", hold_args);
    assert_eq!(
        retried_hold, held,
        "the retry of a lost committed reply must replay the SAME receipt"
    );
    // Exactly one hold committed.
    let after = agent.call_ok(
        "byom_activity_show",
        json!({"activity_stream_ref": stream_id}),
    );
    assert_eq!(
        after["result"]["revision"].as_u64(),
        Some(2),
        "one logical call, one committed effect: {after}"
    );

    // Read: society_show through the same profile.
    let society = agent.call_ok("byom_society_show", json!({"society_id": society_id}));
    assert_eq!(society["result"]["state"], "active");

    // A daemon problem surfaces as a tool error carrying the closed
    // problem type (non-enumerating not_found).
    let (text, is_error) = agent.call(
        "byom_activity_show",
        json!({"activity_stream_ref": "act-none"}),
    );
    assert!(is_error);
    assert!(
        text.contains("https://byom.dev/problems/not_found"),
        "{text}"
    );

    // Deny-by-absence holds against the live daemon too: governance
    // never becomes callable.
    let (text, is_error) = agent.call("byom_mandate_issue", json!({}));
    assert!(is_error);
    assert!(text.contains("deny-by-absence"), "{text}");

    // Channel-derived identity in input refuses before any dispatch.
    let (text, is_error) = agent.call(
        "byom_wake_intent_submit",
        json!({
            "activity_stream_ref": stream_id,
            "generation": 1,
            "origin": "direct_participant",
            "exact_cause_ref": "cause-2",
            "exact_cause_digest": test_digest(0xc3),
            "purpose_ref": "purpose-explore-1",
            "stable_wake_key": "wake-mcp-e2e-2",
            "expires_at": "2030-01-01T00:00:00Z",
            "actor_ref": "participant:someone-else",
        }),
    );
    assert!(is_error);
    assert!(text.contains("actor_ref"), "{text}");
    assert!(text.contains("closed shape"), "{text}");
    drop(agent);

    // -- the same-UID sovereign runs the participant profile with NO
    //    token (byomd's developer-profile channel rule): the caller
    //    resolves to the sovereign, so an unknown target answers
    //    not_found — not the forbidden an unknown credential would get.
    let mut sovereign = McpServer::start(&runtime, "participant", &[]);
    sovereign.initialize();
    let (text, is_error) = sovereign.call(
        "byom_wake_intent_withdraw",
        json!({"wake_intent_ref": "wake-none"}),
    );
    assert!(is_error);
    assert!(
        text.contains("https://byom.dev/problems/not_found"),
        "sovereign channel must resolve (not forbidden): {text}"
    );
}
