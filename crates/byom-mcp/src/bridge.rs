//! The per-surface socket bridge: assembles the BPP envelope
//! server-side around already-validated tool args and speaks byomd's
//! one-line-request, one-line-reply protocol (the byom-cli client
//! shape), with the per-surface credential preamble.
//!
//! Socket routing follows the tool's registry surface (the same rows
//! byomd dispatches with):
//! - candidate profile: everything on `candidate.sock`, the offer-scoped
//!   channel CREDENTIAL minting a fresh sender-constrained proof as the
//!   transport preamble line of every call (§7.4, BY-C1);
//! - participant profile: participant-surface ops AND the
//!   originating-surface recovery reads on `participant.sock` (§14.4
//!   "originating surface"), with a freshly minted participant channel
//!   proof preamble when a credential is configured — the same-UID sovereign of
//!   the developer profile sends the request directly, exactly byomd's
//!   channel rule; projection reads on `projection.sock`, never with a
//!   preamble.
//!
//! Envelope derivation — the C3a binding envelope: `version` is the
//! document's pinned protocol version; mutations carry `meta` with a
//! **logical-call idempotency key** derived from (tool, canonical JCS
//! input, per-session salt) — MCP-1/D-R1-3: an ambiguous transport
//! retry of the identical call reuses the key and replays the retained
//! receipt instead of minting a second command — the endpoint incarnation
//! learned via `hello` on the dispatch surface, and recovery epoch 0
//! (the byom-cli developer-profile discipline; a recovered Society
//! answers `stale_binding` honestly). Update-class ops additionally
//! require `expected_revision` (RT-01) — never tool input (the frozen
//! document derives the whole envelope bridge-side), so it is derived
//! from observable channel/projection state per op:
//! - `membership_accept`: an OPEN candidate channel only ever accepts
//!   from the offer's minted revision 1 — refusal, expiry, and
//!   admission close the channel in the same commit that bumps the
//!   offer past it, and acceptance itself moves to the accepted state
//!   no second acceptance may leave;
//! - `membership_refuse`: revision 1, or 2 when the call cites a
//!   `superseded_acceptance_ref` (§7.4 retraction: the acceptance
//!   transition was the one bump an open channel can have seen);
//! - `activity_hold`/`activity_close`: `activity_show` (projection);
//! - `endeavor_finalize`, `pledge_resume`, `pledge_relinquish`:
//!   `snapshot_get` (projection; needs `$BYOM_SOCIETY` to name the
//!   Society scope);
//! - `pledge_finalize`: the op's own `proposal_revision` argument (the
//!   daemon requires `meta.expected_revision` to equal it);
//! - `wake_intent_withdraw`/`call_withdraw`: the target's minted
//!   revision 1 (its only other writer is the terminal withdraw
//!   itself);
//! - `delivery_withdraw`/`act_intent_cancel`: revision 1 — B1 answers
//!   `feature_unavailable` before any CAS for these.
//!
//! A wrong derivation always fails CLOSED in the daemon
//! (`stale_revision`, zero effects); the bridge never retries.

use std::io::{BufRead as _, BufReader, Write as _};
use std::os::unix::net::UnixStream;

use bpp_core::canonical::{jcs, sha256_hex};
use bpp_core::registry::{OpClass, Surface};
use byomd::channel::{self, Peer};
use byomd::socket::{self, SocketSurface};
use serde_json::{json, Map, Value};

use crate::document::{Profile, Tool};

/// A failed bridge call.
pub enum BridgeError {
    /// Transport or local failure: byomd unreachable, malformed reply,
    /// an unresolvable revision derivation…
    Io(String),
    /// The daemon answered a problem — carried verbatim so the MCP tool
    /// error keeps the problem type and kind.
    Problem(Value),
}

/// The daemon connection state: profile, credential, Society pin.
pub struct Bridge {
    profile: Profile,
    /// Candidate: the offer-scoped channel CREDENTIAL (required) — the
    /// proof key byomd minted, never a bearer token.
    /// Participant: the participant channel credential, or None for the
    /// same-UID sovereign.
    token: Option<String>,
    /// `$BYOM_SOCIETY` — the Society scope for snapshot-resolved
    /// revision derivations (only those need it).
    society: Option<String>,
    /// The LOGICAL-CALL salt (D-R1-3): stable for this MCP session, so
    /// an ambiguous transport retry of the same tool call reuses the
    /// same idempotency key instead of minting a second command.
    session_salt: String,
}

impl Bridge {
    /// Builds the bridge from the environment. Candidate mode REQUIRES
    /// the offer token (`BYOM_CANDIDATE_TOKEN`, or a token file at
    /// `BYOM_CANDIDATE_TOKEN_FILE` — the file byomd mints under
    /// `<data-dir>/channels/`); participant mode takes
    /// `BYOM_PARTICIPANT_TOKEN`/`BYOM_PARTICIPANT_TOKEN_FILE` when the
    /// channel credential was minted, else runs as the same-UID
    /// sovereign.
    pub fn from_env(profile: Profile) -> Result<Bridge, String> {
        let token = match profile {
            Profile::Candidate => {
                let token = env_token("BYOM_CANDIDATE_TOKEN", "BYOM_CANDIDATE_TOKEN_FILE")?;
                let Some(token) = token else {
                    return Err(
                        "candidate profile requires the offer-scoped channel token: set \
                         BYOM_CANDIDATE_TOKEN, or BYOM_CANDIDATE_TOKEN_FILE to the token \
                         file byomd minted under <data-dir>/channels/"
                            .to_owned(),
                    );
                };
                Some(token)
            }
            Profile::Participant => {
                env_token("BYOM_PARTICIPANT_TOKEN", "BYOM_PARTICIPANT_TOKEN_FILE")?
            }
        };
        let society = std::env::var("BYOM_SOCIETY").ok().filter(|s| !s.is_empty());
        Ok(Bridge::new(profile, token, society))
    }

    /// The explicit constructor (tests). The session salt comes from
    /// `$BYOM_MCP_SESSION` when the harness pins one, else from this
    /// process's identity — stable for the session either way, so the
    /// logical-call key is stable across ambiguous retries.
    pub fn new(profile: Profile, token: Option<String>, society: Option<String>) -> Bridge {
        let session_salt = std::env::var("BYOM_MCP_SESSION")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("pid:{}", std::process::id()));
        Bridge {
            profile,
            token,
            society,
            session_salt,
        }
    }

    /// Runs one tool invocation as its surface op: envelope assembled
    /// here, validated `args` passed through verbatim at the top level
    /// (the §14.2 request shape). Returns the daemon's Success envelope
    /// minus `outcome` (`result`, plus `revision`/`source_cursor` when
    /// the daemon minted them).
    pub fn call(
        &self,
        version: &str,
        tool: &Tool,
        args: &Map<String, Value>,
    ) -> Result<Value, BridgeError> {
        let (surface, preamble) = self.dial_plan(tool)?;
        let mut body = Map::new();
        body.insert("version".into(), json!(version));
        body.insert("op".into(), json!(tool.op));
        if tool.class != OpClass::Read {
            body.insert("meta".into(), self.meta(version, tool, args, surface)?);
        }
        for (key, member) in args {
            // The closed schemas cannot declare envelope members
            // (load-time check), so a collision here is impossible; the
            // guard keeps that invariant fail-closed.
            if body.contains_key(key) {
                return Err(BridgeError::Io(format!(
                    "input member {key:?} collides with the server-side envelope"
                )));
            }
            body.insert(key.clone(), member.clone());
        }
        let reply = self.request(surface, preamble, &Value::Object(body))?;
        let mut open = reply.as_object().cloned().unwrap_or_default();
        open.remove("outcome");
        Ok(Value::Object(open))
    }

    /// Which socket the tool dials, and whether the credential preamble
    /// precedes the request line.
    fn dial_plan(&self, tool: &Tool) -> Result<(SocketSurface, Option<&str>), BridgeError> {
        let token = self.token.as_deref();
        match (self.profile, tool.surface) {
            (Profile::Candidate, Surface::Candidate) => Ok((SocketSurface::Candidate, token)),
            (Profile::Participant, Surface::Participant | Surface::Originating) => {
                Ok((SocketSurface::Participant, token))
            }
            (Profile::Participant, Surface::Projection) => Ok((SocketSurface::Projection, None)),
            // document::load only admits the surfaces above; anything
            // else is a defect, refused rather than mis-dialed.
            _ => Err(BridgeError::Io(format!(
                "tool {} resolves to surface {:?} outside the {} binding",
                tool.name,
                tool.surface.as_str(),
                self.profile.as_str()
            ))),
        }
    }

    /// MutationMeta for one call. The idempotency key is the LOGICAL
    /// CALL key (MCP-1 / D-R1-3): a deterministic derivation from
    /// (tool, canonical JCS input, per-session salt). A fresh random key
    /// per invocation was a double-commit hazard — if the daemon
    /// committed and the reply was lost, the MCP caller could not
    /// recover or reuse the key and the retry minted a SECOND Mandate.
    /// With this derivation the ambiguous retry of an identical call
    /// carries the identical key and replays the retained receipt.
    fn meta(
        &self,
        version: &str,
        tool: &Tool,
        args: &Map<String, Value>,
        surface: SocketSurface,
    ) -> Result<Value, BridgeError> {
        let key = self.logical_call_key(tool, args)?;
        let mut meta = Map::new();
        meta.insert("request_id".into(), json!(format!("req-{key}")));
        meta.insert("idempotency_key".into(), json!(format!("mcp-{key}")));
        meta.insert(
            "expected_endpoint_incarnation".into(),
            json!(self.incarnation(version, surface)?),
        );
        meta.insert("expected_recovery_epoch".into(), json!(0));
        if tool.class == OpClass::Update {
            meta.insert(
                "expected_revision".into(),
                json!(self.expected_revision(version, tool, args)?),
            );
        }
        Ok(Value::Object(meta))
    }

    /// The deterministic logical-call key of one tool invocation.
    pub fn logical_call_key(
        &self,
        tool: &Tool,
        args: &Map<String, Value>,
    ) -> Result<String, BridgeError> {
        let bound = json!({
            "session": self.session_salt,
            "tool": tool.name,
            "op": tool.op,
            "input": Value::Object(args.clone()),
        });
        let bytes = jcs(&bound)
            .map_err(|e| BridgeError::Io(format!("canonicalizing the logical call: {e}")))?;
        Ok(sha256_hex(&bytes)[..32].to_owned())
    }

    /// The daemon's endpoint incarnation, learned via `hello` on the
    /// dispatch surface (pre-auth negotiation; the byom-cli shape).
    fn incarnation(&self, version: &str, surface: SocketSurface) -> Result<String, BridgeError> {
        let preamble = match surface {
            SocketSurface::Candidate => self.token.as_deref(),
            SocketSurface::Participant => self.token.as_deref(),
            _ => None,
        };
        let reply = self.request(
            surface,
            preamble,
            &json!({"version": version, "op": "hello"}),
        )?;
        reply["result"]["endpoint_incarnation"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| BridgeError::Io("hello reply carries no endpoint_incarnation".into()))
    }

    /// The update-class CAS token, derived from observable state (see
    /// the module docs; the frozen document keeps the whole envelope
    /// bridge-derived).
    fn expected_revision(
        &self,
        version: &str,
        tool: &Tool,
        args: &Map<String, Value>,
    ) -> Result<u64, BridgeError> {
        match tool.op.as_str() {
            // Acceptance only ever applies to the offer's minted
            // revision (see the module docs); a wrong pin fails closed
            // as stale_revision with zero effects.
            "membership_accept" => Ok(1),
            // Refusal targets the minted revision, or the
            // post-acceptance revision when it retracts one (§7.4: the
            // citation is the op's own argument).
            "membership_refuse" => Ok(if args.contains_key("superseded_acceptance_ref") {
                2
            } else {
                1
            }),
            // Sole-writer targets sit at their minted revision while
            // updatable (B1).
            "wake_intent_withdraw" | "call_withdraw" => Ok(1),
            // Not implemented in B1: the daemon answers
            // feature_unavailable before any CAS comparison.
            "delivery_withdraw" | "act_intent_cancel" => Ok(1),
            // The daemon requires meta.expected_revision to equal the
            // op's own proposal_revision argument.
            "pledge_finalize" => args
                .get("proposal_revision")
                .and_then(Value::as_u64)
                .ok_or_else(|| {
                    BridgeError::Io("pledge_finalize input carries no proposal_revision".into())
                }),
            "activity_hold" | "activity_close" => {
                let target = required_str(args, "activity_stream_ref")?;
                let reply = self.request(
                    SocketSurface::Projection,
                    None,
                    &json!({"version": version, "op": "activity_show",
                            "activity_stream_ref": target}),
                )?;
                revision_of(&reply["result"], "activity_show")
            }
            "endeavor_finalize" => {
                self.snapshot_revision(version, "endeavors", "endeavor_id", args, "endeavor_id")
            }
            "pledge_resume" | "pledge_relinquish" => {
                self.snapshot_revision(version, "pledges", "pledge_id", args, "pledge_id")
            }
            other => Err(BridgeError::Io(format!(
                "no expected_revision derivation for update op {other:?}"
            ))),
        }
    }

    /// Reads the target's current revision out of the Society snapshot
    /// (projection).
    fn snapshot_revision(
        &self,
        version: &str,
        kind: &str,
        id_member: &str,
        args: &Map<String, Value>,
        arg_name: &str,
    ) -> Result<u64, BridgeError> {
        let target = required_str(args, arg_name)?;
        let Some(society) = &self.society else {
            return Err(BridgeError::Io(format!(
                "deriving expected_revision for this update needs the Society scope; \
                 set BYOM_SOCIETY (required only for {kind} updates)"
            )));
        };
        let reply = self.request(
            SocketSurface::Projection,
            None,
            &json!({"version": version, "op": "snapshot_get",
                    "society_id": society, "kinds": [kind]}),
        )?;
        let row = reply["result"][kind]
            .as_array()
            .into_iter()
            .flatten()
            .find(|row| row[id_member].as_str() == Some(target));
        match row {
            Some(row) => revision_of(row, kind),
            None => Err(BridgeError::Io(format!(
                "{arg_name} {target:?} is not present in the Society snapshot"
            ))),
        }
    }

    /// One request line in, one reply line out (the whole protocol).
    /// The preamble is a FRESH sender-constrained proof minted for this
    /// exact operation and this process's connection (BY-C1) — never a
    /// replayable bearer token.
    fn request(
        &self,
        surface: SocketSurface,
        preamble: Option<&str>,
        body: &Value,
    ) -> Result<Value, BridgeError> {
        let preamble = match preamble {
            Some(credential) => Some(self.proof_for(credential, body)?),
            None => None,
        };
        let preamble = preamble.as_deref();
        let path = socket::socket_path(surface);
        let mut stream = UnixStream::connect(&path).map_err(|e| {
            BridgeError::Io(format!(
                "could not reach byomd at {} ({e}); is the daemon running?",
                path.display()
            ))
        })?;
        let mut line = String::new();
        if let Some(token) = preamble {
            line.push_str(token);
            line.push('\n');
        }
        line.push_str(&body.to_string());
        line.push('\n');
        stream.write_all(line.as_bytes()).map_err(io_error)?;
        let mut reply = String::new();
        BufReader::new(stream)
            .read_line(&mut reply)
            .map_err(io_error)?;
        let parsed: Value = serde_json::from_str(reply.trim_end())
            .map_err(|e| BridgeError::Io(format!("malformed daemon reply: {e}")))?;
        match parsed.get("outcome").and_then(Value::as_str) {
            Some("ok") => Ok(parsed),
            Some("problem") => Err(BridgeError::Problem(
                parsed.get("problem").cloned().unwrap_or(Value::Null),
            )),
            _ => Err(BridgeError::Io(
                "malformed daemon reply: no outcome".to_owned(),
            )),
        }
    }
}

impl Bridge {
    /// Mints the per-call proof for the surface's channel credential.
    /// The daemon rebuilds the same preimage from what it OBSERVES, so a
    /// proof copied to another process does not verify there.
    fn proof_for(&self, credential: &str, body: &Value) -> Result<String, BridgeError> {
        let operation = body["op"].as_str().unwrap_or_default();
        channel::mint_proof(
            credential,
            operation,
            Peer::current(),
            bpp_core::time::unix_now(),
        )
        .ok_or_else(|| BridgeError::Io("channel credential is not a byom proof key".to_owned()))
    }
}

fn required_str<'a>(args: &'a Map<String, Value>, name: &str) -> Result<&'a str, BridgeError> {
    args.get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| BridgeError::Io(format!("validated input carries no {name}")))
}

fn revision_of(row: &Value, what: &str) -> Result<u64, BridgeError> {
    row.get("revision")
        .and_then(Value::as_u64)
        .ok_or_else(|| BridgeError::Io(format!("{what} projection carries no revision")))
}

fn io_error(e: std::io::Error) -> BridgeError {
    BridgeError::Io(format!("daemon socket io: {e}"))
}

/// Loads and checks one credential from the environment: the token
/// value variable wins, else the token file path variable. A token is
/// one visible-ASCII line; byomd frames the participant preamble by
/// "does not open a JSON object", so a `{`-leading token is refused
/// here rather than mis-framed there.
fn env_token(value_var: &str, file_var: &str) -> Result<Option<String>, String> {
    let raw = match std::env::var(value_var) {
        Ok(v) if !v.trim().is_empty() => v,
        _ => match std::env::var(file_var) {
            Ok(path) if !path.trim().is_empty() => {
                std::fs::read_to_string(path.trim()).map_err(|e| format!("{file_var}: {e}"))?
            }
            _ => return Ok(None),
        },
    };
    check_token(raw.trim()).map(|t| Some(t.to_owned()))
}

fn check_token(token: &str) -> Result<&str, String> {
    if token.is_empty() {
        return Err("channel token is empty".to_owned());
    }
    if !token.bytes().all(|b| (0x21..=0x7e).contains(&b)) {
        return Err("channel token is not one visible-ASCII line".to_owned());
    }
    if token.starts_with('{') {
        return Err("channel token cannot open a JSON object".to_owned());
    }
    Ok(token)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::document::{self, Profile};

    fn tool(profile: Profile, name: &str) -> crate::document::Tool {
        document::load(profile)
            .unwrap()
            .tools
            .into_iter()
            .find(|t| t.name == name)
            .unwrap()
    }

    #[test]
    fn tokens_are_one_visible_ascii_line() {
        assert!(check_token("chan-7f00").is_ok());
        assert!(check_token("").is_err());
        assert!(check_token("two\nlines").is_err());
        assert!(check_token("has space").is_err());
        assert!(
            check_token("{\"op\":\"x\"}").is_err(),
            "must not frame as a request"
        );
    }

    #[test]
    fn revision_derivations_that_need_no_daemon() {
        let bridge = Bridge::new(Profile::Candidate, Some("tok".into()), None);
        let accept = tool(Profile::Candidate, "byom_membership_accept");
        let args = Map::new();
        // The open-channel pin: acceptance targets the minted revision.
        match bridge.expected_revision("0.2", &accept, &args) {
            Ok(revision) => assert_eq!(revision, 1),
            Err(_) => panic!("candidate derivation must not dial anywhere"),
        }
        // Refusal follows its own citation: plain refusal targets the
        // minted revision, a §7.4 retraction the post-acceptance one.
        let refuse = tool(Profile::Candidate, "byom_membership_refuse");
        match bridge.expected_revision("0.2", &refuse, &args) {
            Ok(revision) => assert_eq!(revision, 1),
            Err(_) => panic!("refuse derivation must not dial anywhere"),
        }
        let mut retraction = Map::new();
        retraction.insert(
            "superseded_acceptance_ref".into(),
            serde_json::json!("acceptance-1"),
        );
        match bridge.expected_revision("0.2", &refuse, &retraction) {
            Ok(revision) => assert_eq!(revision, 2),
            Err(_) => panic!("retraction derivation must not dial anywhere"),
        }
        // pledge_finalize takes the op's own proposal_revision argument.
        let bridge = Bridge::new(Profile::Participant, None, None);
        let finalize = tool(Profile::Participant, "byom_pledge_finalize");
        let mut args = Map::new();
        args.insert("proposal_revision".into(), serde_json::json!(7));
        match bridge.expected_revision("0.2", &finalize, &args) {
            Ok(revision) => assert_eq!(revision, 7),
            Err(_) => panic!("proposal_revision passes through"),
        }
        // Snapshot-resolved updates refuse without the Society scope,
        // naming the fix — before any socket dial.
        let finalize = tool(Profile::Participant, "byom_endeavor_finalize");
        let mut args = Map::new();
        args.insert("endeavor_id".into(), serde_json::json!("end-1"));
        match bridge.expected_revision("0.2", &finalize, &args) {
            Err(BridgeError::Io(detail)) => assert!(detail.contains("BYOM_SOCIETY"), "{detail}"),
            _ => panic!("must refuse without BYOM_SOCIETY"),
        }
    }

    #[test]
    fn dial_plans_follow_the_registry_surface() {
        let candidate = Bridge::new(Profile::Candidate, Some("tok".into()), None);
        let accept = tool(Profile::Candidate, "byom_membership_accept");
        let (surface, preamble) = candidate.dial_plan(&accept).ok().unwrap();
        assert_eq!(surface, SocketSurface::Candidate);
        assert_eq!(preamble, Some("tok"));

        let sovereign = Bridge::new(Profile::Participant, None, None);
        let open = tool(Profile::Participant, "byom_activity_open");
        let (surface, preamble) = sovereign.dial_plan(&open).ok().unwrap();
        assert_eq!(surface, SocketSurface::Participant);
        assert_eq!(preamble, None, "the same-UID sovereign sends no preamble");

        let show = tool(Profile::Participant, "byom_society_show");
        let (surface, preamble) = sovereign.dial_plan(&show).ok().unwrap();
        assert_eq!(surface, SocketSurface::Projection);
        assert_eq!(preamble, None, "projection never takes a preamble");

        let agent = Bridge::new(Profile::Participant, Some("ptok".into()), None);
        let recover = tool(Profile::Participant, "byom_idempotency_result");
        let (surface, preamble) = agent.dial_plan(&recover).ok().unwrap();
        assert_eq!(
            surface,
            SocketSurface::Participant,
            "originating reads answer on the mutation-capable socket"
        );
        assert_eq!(preamble, Some("ptok"));
    }
}
