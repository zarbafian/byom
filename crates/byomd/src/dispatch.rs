//! Per-connection request handling: admission caps, strict I-JSON, the
//! §14.2 envelope shapes, the registry (operation,surface) dispatch
//! truth, the meta-class rule, sealed-endpoint refusal, and dispatch
//! into the handlers. One newline-terminated JSON request per
//! connection: write one line, read one line, the daemon closes. The
//! candidate socket takes one token preamble line before the request.

use std::io::{BufRead as _, BufReader, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::{Arc, Mutex};

use bpp_core::envelope::RawRequest;
use bpp_core::limits;
use bpp_core::ops;
use bpp_core::problem::{Failure, Problem, ProblemKind};
use bpp_core::registry;
use bpp_core::time::unix_now;
use byom_store::witness::WitnessFault;
use byom_store::{CrashHooks, Store};
use serde_json::Value;

use crate::socket::SocketSurface;
use crate::state;
use crate::{cand_ops, gov_ops, reads};

/// A crash-honesty instruction from the environment
/// (`BYOMD_ABORT=<phase>:<op>`): abort the process, or inject a witness
/// fault, at the named §15.3 boundary of the named operation. Test-only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbortSpec {
    pub phase: String,
    pub op: String,
}

impl AbortSpec {
    pub fn from_env() -> Option<AbortSpec> {
        let raw = std::env::var("BYOMD_ABORT").ok()?;
        let (phase, op) = raw.split_once(':')?;
        Some(AbortSpec {
            phase: phase.to_owned(),
            op: op.to_owned(),
        })
    }

    fn hooks_for(&self, op: &str) -> CrashHooks {
        if self.op != op {
            return CrashHooks::NONE;
        }
        CrashHooks {
            abort_before_witness: self.phase == "before_witness",
            abort_after_witness: self.phase == "after_witness",
            abort_before_finalize: self.phase == "before_finalize",
            abort_after_finalize: self.phase == "after_finalize",
            witness_fault: match self.phase.as_str() {
                "witness_lose_reply" => WitnessFault::LoseReplyAfterWrite,
                "witness_lose_request" => WitnessFault::LoseRequest,
                _ => WitnessFault::None,
            },
        }
    }
}

/// The daemon: one store (journal driver + SQLite) behind a mutex, four
/// dispatch surfaces.
pub struct Daemon {
    store: Mutex<Store>,
    abort: Option<AbortSpec>,
}

impl Daemon {
    pub fn new(store: Store, abort: Option<AbortSpec>) -> Daemon {
        Daemon {
            store: Mutex::new(store),
            abort,
        }
    }

    /// Serves connections until the listener errors, one thread per
    /// connection.
    pub fn serve(self: &Arc<Self>, listener: UnixListener, surface: SocketSurface) {
        let uid = crate::peercred::current_uid();
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            // Channel authentication first: a foreign UID is dropped
            // before a byte is read, and learns nothing.
            if crate::peercred::authenticate_same_uid(&stream, uid).is_err() {
                continue;
            }
            let daemon = Arc::clone(self);
            std::thread::spawn(move || {
                if let Err(e) = daemon.handle_connection(stream, surface) {
                    eprintln!("byomd: connection error: {e}");
                }
            });
        }
    }

    fn handle_connection(&self, stream: UnixStream, surface: SocketSurface) -> std::io::Result<()> {
        // ONE buffer for the whole connection: the token preamble and
        // the request line share it, so no buffered bytes are lost
        // between reads.
        let mut reader = BufReader::new(stream.try_clone()?);
        // The candidate surface presents its offer-scoped channel token
        // as a transport preamble line: the closed per-operation request
        // schemas carry no credential member (§7.4 candidate channel).
        let token = if surface == SocketSurface::Candidate {
            let mut token_line = String::new();
            let mut limited = (&mut reader).take(4096);
            limited.read_line(&mut token_line)?;
            trim_newline(&mut token_line);
            Some(token_line)
        } else {
            None
        };
        let mut line = String::new();
        // Read at most one byte past the cap so an oversized request is
        // detected, not buffered.
        let mut limited = (&mut reader).take(limits::REQUEST_MAX_BYTES as u64 + 1);
        limited.read_line(&mut line)?;
        trim_newline(&mut line);
        let reply = self.dispatch_line(&line, surface, token.as_deref());
        let mut stream = stream;
        stream.write_all(&reply)?;
        stream.write_all(b"\n")?;
        Ok(())
    }

    /// One request line to one reply line (no trailing newline).
    pub fn dispatch_line(
        &self,
        line: &str,
        surface: SocketSurface,
        token: Option<&str>,
    ) -> Vec<u8> {
        match self.dispatch_inner(line, surface, token) {
            Ok(reply) => {
                if reply.len() > limits::RESPONSE_MAX_BYTES {
                    // §14.9 response cap: fail closed rather than stream
                    // an over-cap reply.
                    problem_bytes(state::internal("reply exceeds the 1 MiB response cap"))
                } else {
                    reply
                }
            }
            Err(problem) => problem_bytes(problem),
        }
    }

    fn dispatch_inner(
        &self,
        line: &str,
        surface: SocketSurface,
        token: Option<&str>,
    ) -> Result<Vec<u8>, Problem> {
        // Strict I-JSON acceptance under the request cap (PROFILE §1).
        let value = bpp_core::ijson::parse_request(line.as_bytes())
            .map_err(|e| state::invalid(&format!("not strict I-JSON: {}", e.class.as_str())))?;
        let raw = RawRequest::from_value(&value)?;
        // Version before op: a client speaking another minor learns
        // unsupported-version, not registry noise.
        if raw.version != bpp_core::PROTOCOL_VERSION {
            return Err(Problem::new(
                ProblemKind::UnsupportedVersion,
                "no common protocol version",
            )
            .with_status(400));
        }

        let negotiation = matches!(raw.op.as_str(), "hello" | "protocol_info" | "feature_info");

        // §15.3: a sealed endpoint closes every non-diagnostic surface;
        // only the endpoint/version negotiation closure still answers.
        if !negotiation && self.lock_store()?.sealed() {
            return Err(state::endpoint_sealed());
        }

        // The registry rows are the dispatch truth (§14.6/§14.7): the
        // negotiation family is bound pre-auth on every advertised
        // surface; everything else needs its exact (operation,surface)
        // row — absent row, deny by absence.
        let class = if negotiation {
            registry::OpClass::Read
        } else if let Some(row) = registry::lookup(&raw.op, surface.registry_surface()) {
            row.class
        } else if registry::op_exists(&raw.op) {
            return Err(state::forbidden_surface());
        } else {
            return Err(state::unknown_op());
        };
        // Envelope shape: reads never carry meta; creates carry meta
        // without expected_revision; updates require it (RT-01).
        raw.check_class(class)?;

        let now = unix_now();
        let hooks = self
            .abort
            .as_ref()
            .map(|a| a.hooks_for(&raw.op))
            .unwrap_or(CrashHooks::NONE);
        let body = &raw.body;

        match raw.op.as_str() {
            // ----------------------------------------- negotiation ----
            "hello" => {
                ops::NegotiationRequest::parse(body, "hello").map_err(|e| state::invalid(&e))?;
                reads::hello(&*self.lock_store()?, surface)
            }
            "protocol_info" => {
                ops::NegotiationRequest::parse(body, "protocol_info")
                    .map_err(|e| state::invalid(&e))?;
                reads::protocol_info(&*self.lock_store()?)
            }
            "feature_info" => {
                ops::NegotiationRequest::parse(body, "feature_info")
                    .map_err(|e| state::invalid(&e))?;
                reads::feature_info(&*self.lock_store()?)
            }
            // ------------------------------------------ governance ----
            "society_prepare" => {
                let req =
                    ops::SocietyPrepareRequest::parse(body).map_err(|e| state::invalid(&e))?;
                gov_ops::society_prepare(&mut *self.lock_store()?, &req, body, now, hooks)
            }
            "society_bootstrap" => {
                let req =
                    ops::SocietyBootstrapRequest::parse(body).map_err(|e| state::invalid(&e))?;
                gov_ops::society_bootstrap(&mut *self.lock_store()?, &req, body, now, hooks)
            }
            "membership_offer" => {
                let req =
                    ops::MembershipOfferRequest::parse(body).map_err(|e| state::invalid(&e))?;
                gov_ops::membership_offer(&mut *self.lock_store()?, &req, body, now, hooks)
            }
            "participant_admit" => {
                let req =
                    ops::ParticipantAdmitRequest::parse(body).map_err(|e| state::invalid(&e))?;
                gov_ops::participant_admit(&mut *self.lock_store()?, &req, body, now, hooks)
            }
            "manifestation_admit" => {
                let req =
                    ops::ManifestationAdmitRequest::parse(body).map_err(|e| state::invalid(&e))?;
                gov_ops::manifestation_admit(&mut *self.lock_store()?, &req, body, now, hooks)
            }
            // ------------------------------------------- candidate ----
            "membership_accept" => {
                let req =
                    ops::MembershipAcceptRequest::parse(body).map_err(|e| state::invalid(&e))?;
                cand_ops::membership_accept(
                    &mut *self.lock_store()?,
                    token.unwrap_or_default(),
                    &req,
                    body,
                    now,
                    hooks,
                )
            }
            "membership_refuse" => {
                let req =
                    ops::MembershipRefuseRequest::parse(body).map_err(|e| state::invalid(&e))?;
                cand_ops::membership_refuse(
                    &mut *self.lock_store()?,
                    token.unwrap_or_default(),
                    &req,
                    body,
                    now,
                    hooks,
                )
            }
            // ------------------------------------------ projection ----
            "society_show" => {
                let req = ops::SocietyShowRequest::parse(body).map_err(|e| state::invalid(&e))?;
                reads::society_show(&*self.lock_store()?, &req.society_id)
            }
            "participant_show" => {
                let req =
                    ops::ParticipantShowRequest::parse(body).map_err(|e| state::invalid(&e))?;
                reads::participant_show(&*self.lock_store()?, &req.participant_ref)
            }
            "events_read" => {
                let req = ops::EventsReadRequest::parse(body).map_err(|e| state::invalid(&e))?;
                reads::events_read(&*self.lock_store()?, &req.continuation, req.page_size)
            }
            // Registry-bound on this surface but not yet implemented in
            // this slice: honestly unavailable, never silently absent.
            _ => Err(Problem::new(
                ProblemKind::FeatureUnavailable,
                "operation is not implemented in this slice (see feature_info)",
            )
            .with_status(501)),
        }
    }

    fn lock_store(&self) -> Result<std::sync::MutexGuard<'_, Store>, Problem> {
        self.store
            .lock()
            .map_err(|_| state::internal("store lock poisoned"))
    }
}

fn problem_bytes(problem: Problem) -> Vec<u8> {
    serde_json::to_vec(&Failure { problem }).unwrap_or_else(|_| {
        // Serializing a problem cannot fail; keep a hand-written
        // fallback so the daemon never panics on the reply path.
        br#"{"outcome":"problem","problem":{"type":"https://byom.dev/problems/internal","title":"internal fault","kind":"internal","status":500}}"#
            .to_vec()
    })
}

/// Serializes a `Value` request the way clients do (one line).
pub fn request_line(value: &Value) -> String {
    value.to_string()
}

fn trim_newline(line: &mut String) {
    if line.ends_with('\n') {
        line.pop();
    }
}
