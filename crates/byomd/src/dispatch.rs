//! Per-connection request handling: admission caps, strict I-JSON, the
//! §14.2 envelope shapes, the registry (operation,surface) dispatch
//! truth, the meta-class rule, sealed-endpoint refusal, and dispatch
//! into the handlers. One newline-terminated JSON request per
//! connection: write one line, read one line, the daemon closes. The
//! candidate socket takes one token preamble line before the request;
//! the participant socket takes an OPTIONAL token preamble (an agent's
//! sender-constrained participant token — a line not opening with `{`;
//! the same-UID sovereign sends the request directly).

use std::io::{BufRead as _, BufReader, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bpp_core::envelope::RawRequest;
use bpp_core::limits;
use bpp_core::ops;
use bpp_core::problem::{Failure, Problem, ProblemKind};
use bpp_core::registry;
use bpp_core::time::unix_now;
use byom_store::witness::WitnessFault;
use byom_store::{CrashHooks, Store};
use serde_json::Value;

use crate::part_common::{self, Caller};
use crate::socket::SocketSurface;
use crate::state;
use crate::{cand_ops, gov_authority, gov_ops, part_ops, reads, work_ops};

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
        // as a transport preamble line (the closed per-operation request
        // schemas carry no credential member — §7.4). The participant
        // surface takes the same preamble OPTIONALLY: a first line that
        // does not open a JSON object is the agent's sender-constrained
        // participant token; the same-UID sovereign writes the request
        // directly (tokens never start with `{`).
        let mut line = String::new();
        let token = match surface {
            SocketSurface::Candidate => {
                let mut token_line = String::new();
                let mut limited = (&mut reader).take(4096);
                limited.read_line(&mut token_line)?;
                trim_newline(&mut token_line);
                Some(token_line)
            }
            SocketSurface::Participant => {
                let mut first = String::new();
                let mut limited = (&mut reader).take(limits::REQUEST_MAX_BYTES as u64 + 1);
                limited.read_line(&mut first)?;
                trim_newline(&mut first);
                if first.trim_start().starts_with('{') || first.is_empty() {
                    line = first;
                    Some(String::new())
                } else {
                    Some(first)
                }
            }
            _ => None,
        };
        if line.is_empty() {
            // Read at most one byte past the cap so an oversized request
            // is detected, not buffered.
            let mut limited = (&mut reader).take(limits::REQUEST_MAX_BYTES as u64 + 1);
            limited.read_line(&mut line)?;
            trim_newline(&mut line);
        }
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
        // The diagnostic remainder still answers on a sealed endpoint
        // (§15.3): negotiation plus recovery_checkpoint_show.
        let diagnostic = negotiation
            || (raw.op == "recovery_checkpoint_show" && surface == SocketSurface::Projection);

        // §15.3: a sealed endpoint closes every non-diagnostic surface.
        if !diagnostic && self.lock_store()?.sealed() {
            return Err(state::endpoint_sealed());
        }

        // The registry rows are the dispatch truth (§14.6/§14.7): the
        // negotiation family is bound pre-auth on every advertised
        // surface; the originating-surface recovery reads answer on the
        // mutation-capable sockets (§14.4 "originating surface");
        // everything else needs its exact (operation,surface) row —
        // absent row, deny by absence.
        let originating_read = registry::lookup(&raw.op, registry::Surface::Originating).is_some()
            && matches!(
                surface,
                SocketSurface::Governance | SocketSurface::Candidate | SocketSurface::Participant
            );
        let class = if negotiation || originating_read {
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

        // Negotiation answers on every surface, pre-auth.
        match raw.op.as_str() {
            "hello" => {
                ops::NegotiationRequest::parse(body, "hello").map_err(|e| state::invalid(&e))?;
                return reads::hello(&*self.lock_store()?, surface);
            }
            "protocol_info" => {
                ops::NegotiationRequest::parse(body, "protocol_info")
                    .map_err(|e| state::invalid(&e))?;
                return reads::protocol_info(&*self.lock_store()?);
            }
            "feature_info" => {
                ops::NegotiationRequest::parse(body, "feature_info")
                    .map_err(|e| state::invalid(&e))?;
                return reads::feature_info(&*self.lock_store()?);
            }
            _ => {}
        }

        // A registry-bound operation this slice does not implement is
        // honestly unavailable, never silently absent.
        if !reads::implemented(&raw.op) {
            return Err(feature_unavailable());
        }

        // The events long-poll must not hold the store lock while it
        // waits (writers would starve): poll in bounded steps.
        if raw.op == "events_wait" {
            let req = ops::EventsWaitRequest::parse(body).map_err(|e| state::invalid(&e))?;
            let deadline = Instant::now() + Duration::from_millis(req.max_wait_milliseconds);
            loop {
                let (bytes, count) = {
                    let store = self.lock_store()?;
                    reads::events_page(&store, &req.continuation, req.page_size)?
                };
                if count > 0 || Instant::now() >= deadline {
                    return Ok(bytes);
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }

        // The originating-surface recovery reads (channel-derived actor;
        // the closed shape is checked before any channel resolution).
        if originating_read {
            return match raw.op.as_str() {
                "idempotency_result" => {
                    let req = ops::IdempotencyResultRequest::parse(body)
                        .map_err(|e| state::invalid(&e))?;
                    let store = self.lock_store()?;
                    let actor = self.originating_actor(&store, surface, token)?;
                    reads::idempotency_result(&store, &actor, &req)
                }
                "cursor_recover" => {
                    let req =
                        ops::CursorRecoverRequest::parse(body).map_err(|e| state::invalid(&e))?;
                    let store = self.lock_store()?;
                    reads::cursor_recover(&store, &req.continuation)
                }
                _ => Err(feature_unavailable()),
            };
        }

        match surface {
            SocketSurface::Governance => self.dispatch_governance(&raw, now, hooks),
            SocketSurface::Candidate => {
                self.dispatch_candidate(&raw, token.unwrap_or_default(), now, hooks)
            }
            SocketSurface::Participant => {
                self.dispatch_participant(&raw, token.unwrap_or_default(), now, hooks)
            }
            SocketSurface::Projection => self.dispatch_projection(&raw, now),
        }
    }

    fn originating_actor(
        &self,
        store: &Store,
        surface: SocketSurface,
        token: Option<&str>,
    ) -> Result<String, Problem> {
        match surface {
            SocketSurface::Governance => Ok(gov_ops::ACTOR_GOVERNANCE.to_owned()),
            SocketSurface::Candidate => {
                let channel = cand_ops::resolve_channel(store, token.unwrap_or_default())?;
                if channel.state != "open" {
                    return Err(state::forbidden());
                }
                Ok(format!("candidate:{}", channel.channel_id))
            }
            SocketSurface::Participant => {
                match part_common::resolve_caller(store, token.unwrap_or_default())? {
                    Ok(caller) => Ok(caller.actor),
                    Err(_) => Err(state::forbidden()),
                }
            }
            SocketSurface::Projection => Err(state::forbidden_surface()),
        }
    }

    fn dispatch_governance(
        &self,
        raw: &RawRequest,
        now: i64,
        hooks: CrashHooks,
    ) -> Result<Vec<u8>, Problem> {
        let body = &raw.body;
        let mut store = self.lock_store()?;
        match raw.op.as_str() {
            "society_prepare" => {
                let req =
                    ops::SocietyPrepareRequest::parse(body).map_err(|e| state::invalid(&e))?;
                gov_ops::society_prepare(&mut store, &req, body, now, hooks)
            }
            "society_bootstrap" => {
                let req =
                    ops::SocietyBootstrapRequest::parse(body).map_err(|e| state::invalid(&e))?;
                gov_ops::society_bootstrap(&mut store, &req, body, now, hooks)
            }
            "membership_offer" => {
                let req =
                    ops::MembershipOfferRequest::parse(body).map_err(|e| state::invalid(&e))?;
                gov_ops::membership_offer(&mut store, &req, body, now, hooks)
            }
            "participant_admit" => {
                let req =
                    ops::ParticipantAdmitRequest::parse(body).map_err(|e| state::invalid(&e))?;
                gov_ops::participant_admit(&mut store, &req, body, now, hooks)
            }
            "manifestation_admit" => {
                let req =
                    ops::ManifestationAdmitRequest::parse(body).map_err(|e| state::invalid(&e))?;
                gov_ops::manifestation_admit(&mut store, &req, body, now, hooks)
            }
            "mandate_issue" => {
                let req = ops::MandateIssueRequest::parse(body).map_err(|e| state::invalid(&e))?;
                gov_authority::mandate_issue(&mut store, &req, body, now, hooks)
            }
            "mandate_hold" => {
                let req = ops::MandateHoldRequest::parse(body).map_err(|e| state::invalid(&e))?;
                gov_authority::mandate_hold(&mut store, &req, body, now, hooks)
            }
            "mandate_revoke" => {
                let req = ops::MandateRevokeRequest::parse(body).map_err(|e| state::invalid(&e))?;
                gov_authority::mandate_revoke(&mut store, &req, body, now, hooks)
            }
            "mandate_position" => {
                let req = ops::PositionRequest::parse(
                    body,
                    "mandate_position",
                    ops::PositionAssentRule::Flat,
                )
                .map_err(|e| state::invalid(&e))?;
                gov_authority::governance_position(
                    &mut store,
                    "mandate_position",
                    &req,
                    body,
                    now,
                    hooks,
                )
            }
            "charter_position" => {
                let req = ops::PositionRequest::parse(
                    body,
                    "charter_position",
                    ops::PositionAssentRule::Forbidden,
                )
                .map_err(|e| state::invalid(&e))?;
                gov_authority::governance_position(
                    &mut store,
                    "charter_position",
                    &req,
                    body,
                    now,
                    hooks,
                )
            }
            "charter_finalize" => {
                let req =
                    ops::CharterFinalizeRequest::parse(body).map_err(|e| state::invalid(&e))?;
                gov_authority::charter_finalize(&mut store, &req, body, now, hooks)
            }
            _ => Err(feature_unavailable()),
        }
    }

    fn dispatch_candidate(
        &self,
        raw: &RawRequest,
        token: &str,
        now: i64,
        hooks: CrashHooks,
    ) -> Result<Vec<u8>, Problem> {
        let body = &raw.body;
        let mut store = self.lock_store()?;
        match raw.op.as_str() {
            "membership_accept" => {
                let req =
                    ops::MembershipAcceptRequest::parse(body).map_err(|e| state::invalid(&e))?;
                cand_ops::membership_accept(&mut store, token, &req, body, now, hooks)
            }
            "membership_refuse" => {
                let req =
                    ops::MembershipRefuseRequest::parse(body).map_err(|e| state::invalid(&e))?;
                cand_ops::membership_refuse(&mut store, token, &req, body, now, hooks)
            }
            "candidate_self_policy_propose" => {
                let req = ops::CandidateSelfPolicyProposeRequest::parse(body)
                    .map_err(|e| state::invalid(&e))?;
                cand_ops::candidate_self_policy_propose(&mut store, token, &req, body, now, hooks)
            }
            _ => Err(feature_unavailable()),
        }
    }

    fn dispatch_participant(
        &self,
        raw: &RawRequest,
        token: &str,
        now: i64,
        hooks: CrashHooks,
    ) -> Result<Vec<u8>, Problem> {
        let body = &raw.body;
        // The closed request shape is checked FIRST: a malformed body is
        // `invalid` regardless of channel state (schemas are public;
        // nothing about hidden records is disclosed).
        participant_shape_check(&raw.op, body)?;
        let mut store = self.lock_store()?;
        // Channel-derived caller (§14.3): the actor is never a request
        // field. A CLOSED participant channel serves exactly one thing —
        // the byte-identical replay of a terminal receipt for the exact
        // same request (§7.4 discipline, reused by participation_cease).
        let caller: Caller = match part_common::resolve_caller(&store, token)? {
            Ok(caller) => caller,
            Err(channel) => {
                let Some(meta) = &raw.meta else {
                    return Err(state::forbidden());
                };
                return part_common::closed_channel_replay(&store, &channel, &raw.op, meta, body);
            }
        };
        let invalid = |e: String| state::invalid(&e);
        match raw.op.as_str() {
            "assent_policy_adopt" => {
                let req = ops::AssentPolicyAdoptRequest::parse(body).map_err(invalid)?;
                part_ops::self_policy_adopt(
                    &mut store,
                    &caller,
                    "assent",
                    "assent_policy_adopt",
                    strip_envelope(body),
                    &req.adoption_mode,
                    req.previous_digest.as_ref(),
                    &req.effective_at,
                    &req.expires_at,
                    &req.meta,
                    body,
                    now,
                    hooks,
                )
            }
            "activation_policy_adopt" => {
                let req = ops::ActivationPolicyAdoptRequest::parse(body).map_err(invalid)?;
                part_ops::self_policy_adopt(
                    &mut store,
                    &caller,
                    "activation",
                    "activation_policy_adopt",
                    strip_envelope(body),
                    &req.adoption_mode,
                    req.previous_digest.as_ref(),
                    &req.effective_at,
                    &req.expires_at,
                    &req.meta,
                    body,
                    now,
                    hooks,
                )
            }
            "assent_policy_revoke" => {
                let req = ops::PolicyRevokeRequest::parse(body, "assent_policy_revoke")
                    .map_err(invalid)?;
                part_ops::self_policy_revoke(
                    &mut store,
                    &caller,
                    "assent",
                    "assent_policy_revoke",
                    &req,
                    body,
                    now,
                    hooks,
                )
            }
            "activation_policy_revoke" => {
                let req = ops::PolicyRevokeRequest::parse(body, "activation_policy_revoke")
                    .map_err(invalid)?;
                part_ops::self_policy_revoke(
                    &mut store,
                    &caller,
                    "activation",
                    "activation_policy_revoke",
                    &req,
                    body,
                    now,
                    hooks,
                )
            }
            "continuity_root_update" => {
                let req = ops::ContinuityRootUpdateRequest::parse(body).map_err(invalid)?;
                part_ops::continuity_root_update(&mut store, &caller, &req, body, now, hooks)
            }
            "mandate_prepare" => {
                let req = ops::MandatePrepareRequest::parse(body).map_err(invalid)?;
                part_ops::mandate_prepare(&mut store, &caller, &req, body, now, hooks)
            }
            "mandate_derive" => {
                let req = ops::MandateDeriveRequest::parse(body).map_err(invalid)?;
                part_ops::mandate_derive(&mut store, &caller, &req, body, now, hooks)
            }
            "mandate_position" => {
                let req = ops::PositionRequest::parse(
                    body,
                    "mandate_position",
                    ops::PositionAssentRule::Flat,
                )
                .map_err(invalid)?;
                part_ops::mandate_position(
                    &mut store,
                    &caller,
                    "participant",
                    &req,
                    body,
                    now,
                    hooks,
                )
            }
            "endeavor_propose" => {
                let req = ops::EndeavorProposeRequest::parse(body).map_err(invalid)?;
                work_ops::endeavor_propose(&mut store, &caller, &req, body, now, hooks)
            }
            "endeavor_position" => {
                let req = ops::PositionRequest::parse(
                    body,
                    "endeavor_position",
                    ops::PositionAssentRule::Flat,
                )
                .map_err(invalid)?;
                work_ops::endeavor_position(&mut store, &caller, &req, body, now, hooks)
            }
            "endeavor_finalize" => {
                let req = ops::EndeavorFinalizeRequest::parse(body).map_err(invalid)?;
                work_ops::endeavor_finalize(&mut store, &caller, &req, body, now, hooks)
            }
            "endeavor_hold" | "endeavor_release" => {
                let req = ops::EndeavorHoldRequest::parse(body, &raw.op).map_err(invalid)?;
                work_ops::endeavor_hold_release(
                    &mut store, &caller, &raw.op, &req, body, now, hooks,
                )
            }
            "endeavor_close" => {
                let req = ops::EndeavorCloseRequest::parse(body).map_err(invalid)?;
                work_ops::endeavor_close(&mut store, &caller, &req, body, now, hooks)
            }
            "call_open" => {
                let req = ops::CallOpenRequest::parse(body).map_err(invalid)?;
                work_ops::call_open(&mut store, &caller, &req, body, now, hooks)
            }
            "call_withdraw" => {
                let req = ops::CallWithdrawRequest::parse(body).map_err(invalid)?;
                work_ops::call_withdraw(&mut store, &caller, &req, body, now, hooks)
            }
            "pledge_propose" => {
                let req = ops::PledgeProposeRequest::parse(body).map_err(invalid)?;
                work_ops::pledge_propose(&mut store, &caller, &req, body, now, hooks)
            }
            "pledge_amend" => {
                let req = ops::PledgeAmendRequest::parse(body).map_err(invalid)?;
                work_ops::pledge_amend(&mut store, &caller, &req, body, now, hooks)
            }
            "pledge_position" => {
                let req = ops::PositionRequest::parse(
                    body,
                    "pledge_position",
                    ops::PositionAssentRule::PledgeCoupled,
                )
                .map_err(invalid)?;
                work_ops::pledge_position(&mut store, &caller, &req, body, now, hooks)
            }
            "pledge_finalize" => {
                let req = ops::PledgeFinalizeRequest::parse(body).map_err(invalid)?;
                work_ops::pledge_finalize(&mut store, &caller, &req, body, now, hooks)
            }
            "pledge_resume" | "pledge_relinquish" => {
                let req = ops::PledgeIdRequest::parse(body, &raw.op).map_err(invalid)?;
                work_ops::pledge_resume_relinquish(
                    &mut store, &caller, &raw.op, &req, body, now, hooks,
                )
            }
            "delivery_submit" => {
                let req = ops::DeliverySubmitRequest::parse(body).map_err(invalid)?;
                work_ops::delivery_submit(&mut store, &caller, &req, body, now, hooks)
            }
            "review_record" => {
                let req = ops::ReviewRecordRequest::parse(body).map_err(invalid)?;
                work_ops::review_record(&mut store, &caller, &req, body, now, hooks)
            }
            "activity_open" => {
                let req = ops::ActivityOpenRequest::parse(body).map_err(invalid)?;
                part_ops::activity_open(&mut store, &caller, &req, body, now, hooks)
            }
            "activity_hold" => {
                let req = ops::ActivityHoldRequest::parse(body).map_err(invalid)?;
                part_ops::activity_hold(&mut store, &caller, &req, body, now, hooks)
            }
            "activity_close" => {
                let req = ops::ActivityCloseRequest::parse(body).map_err(invalid)?;
                part_ops::activity_close(&mut store, &caller, &req, body, now, hooks)
            }
            "wake_intent_submit" => {
                let req = ops::WakeIntentSubmitRequest::parse(body).map_err(invalid)?;
                part_ops::wake_intent_submit(&mut store, &caller, &req, body, now, hooks)
            }
            "wake_intent_withdraw" => {
                let req = ops::WakeIntentWithdrawRequest::parse(body).map_err(invalid)?;
                part_ops::wake_intent_withdraw(&mut store, &caller, &req, body, now, hooks)
            }
            "continuation_write" => {
                let req = ops::ContinuationWriteRequest::parse(body).map_err(invalid)?;
                part_ops::continuation_write(&mut store, &caller, &req, body, now, hooks)
            }
            "participation_cease" => {
                let req = ops::ParticipationCeaseRequest::parse(body).map_err(invalid)?;
                part_ops::participation_cease(&mut store, &caller, &req, body, now, hooks)
            }
            "charter_propose" => {
                let req = ops::CharterProposeRequest::parse(body).map_err(invalid)?;
                work_ops::charter_propose(&mut store, &caller, &req, body, now, hooks)
            }
            _ => Err(feature_unavailable()),
        }
    }

    fn dispatch_projection(&self, raw: &RawRequest, now: i64) -> Result<Vec<u8>, Problem> {
        let body = &raw.body;
        let store = self.lock_store()?;
        match raw.op.as_str() {
            "society_show" => {
                let req = ops::SocietyShowRequest::parse(body).map_err(|e| state::invalid(&e))?;
                reads::society_show(&store, &req.society_id)
            }
            "participant_show" => {
                let req =
                    ops::ParticipantShowRequest::parse(body).map_err(|e| state::invalid(&e))?;
                reads::participant_show(&store, &req.participant_ref)
            }
            "activity_show" => {
                let req = ops::ActivityShowRequest::parse(body).map_err(|e| state::invalid(&e))?;
                reads::activity_show(&store, &req.activity_stream_ref)
            }
            "charter_history" => {
                let req =
                    ops::CharterHistoryRequest::parse(body).map_err(|e| state::invalid(&e))?;
                reads::charter_history(&store, &req)
            }
            "snapshot_get" => {
                let req = ops::SnapshotGetRequest::parse(body).map_err(|e| state::invalid(&e))?;
                reads::snapshot_get(&store, &req)
            }
            "events_read" => {
                let req = ops::EventsReadRequest::parse(body).map_err(|e| state::invalid(&e))?;
                reads::events_read(&store, &req.continuation, req.page_size)
            }
            "event_payload" => {
                let req = ops::EventPayloadRequest::parse(body).map_err(|e| state::invalid(&e))?;
                reads::event_payload(&store, "projection:local", &req, now)
            }
            "recovery_checkpoint_show" => {
                let req = ops::RecoveryCheckpointShowRequest::parse(body)
                    .map_err(|e| state::invalid(&e))?;
                reads::recovery_checkpoint_show(&store, &req.society_id)
            }
            _ => Err(feature_unavailable()),
        }
    }

    fn lock_store(&self) -> Result<std::sync::MutexGuard<'_, Store>, Problem> {
        self.store
            .lock()
            .map_err(|_| state::internal("store lock poisoned"))
    }
}

/// Parses (and discards) the closed participant-surface request shape —
/// run before channel resolution so shape errors answer deterministically
/// as `invalid`.
fn participant_shape_check(op: &str, body: &Value) -> Result<(), Problem> {
    let e = |e: String| state::invalid(&e);
    match op {
        "assent_policy_adopt" => ops::AssentPolicyAdoptRequest::parse(body)
            .map(drop)
            .map_err(e),
        "activation_policy_adopt" => ops::ActivationPolicyAdoptRequest::parse(body)
            .map(drop)
            .map_err(e),
        "assent_policy_revoke" | "activation_policy_revoke" => {
            ops::PolicyRevokeRequest::parse(body, op)
                .map(drop)
                .map_err(e)
        }
        "continuity_root_update" => ops::ContinuityRootUpdateRequest::parse(body)
            .map(drop)
            .map_err(e),
        "mandate_prepare" => ops::MandatePrepareRequest::parse(body).map(drop).map_err(e),
        "mandate_derive" => ops::MandateDeriveRequest::parse(body).map(drop).map_err(e),
        "mandate_position" | "endeavor_position" => {
            ops::PositionRequest::parse(body, op, ops::PositionAssentRule::Flat)
                .map(drop)
                .map_err(e)
        }
        "pledge_position" => {
            ops::PositionRequest::parse(body, op, ops::PositionAssentRule::PledgeCoupled)
                .map(drop)
                .map_err(e)
        }
        "endeavor_propose" => ops::EndeavorProposeRequest::parse(body)
            .map(drop)
            .map_err(e),
        "endeavor_finalize" => ops::EndeavorFinalizeRequest::parse(body)
            .map(drop)
            .map_err(e),
        "endeavor_hold" | "endeavor_release" => ops::EndeavorHoldRequest::parse(body, op)
            .map(drop)
            .map_err(e),
        "endeavor_close" => ops::EndeavorCloseRequest::parse(body).map(drop).map_err(e),
        "call_open" => ops::CallOpenRequest::parse(body).map(drop).map_err(e),
        "call_withdraw" => ops::CallWithdrawRequest::parse(body).map(drop).map_err(e),
        "pledge_propose" => ops::PledgeProposeRequest::parse(body).map(drop).map_err(e),
        "pledge_amend" => ops::PledgeAmendRequest::parse(body).map(drop).map_err(e),
        "pledge_finalize" => ops::PledgeFinalizeRequest::parse(body).map(drop).map_err(e),
        "pledge_resume" | "pledge_relinquish" => {
            ops::PledgeIdRequest::parse(body, op).map(drop).map_err(e)
        }
        "delivery_submit" => ops::DeliverySubmitRequest::parse(body).map(drop).map_err(e),
        "review_record" => ops::ReviewRecordRequest::parse(body).map(drop).map_err(e),
        "activity_open" => ops::ActivityOpenRequest::parse(body).map(drop).map_err(e),
        "activity_hold" => ops::ActivityHoldRequest::parse(body).map(drop).map_err(e),
        "activity_close" => ops::ActivityCloseRequest::parse(body).map(drop).map_err(e),
        "wake_intent_submit" => ops::WakeIntentSubmitRequest::parse(body)
            .map(drop)
            .map_err(e),
        "wake_intent_withdraw" => ops::WakeIntentWithdrawRequest::parse(body)
            .map(drop)
            .map_err(e),
        "continuation_write" => ops::ContinuationWriteRequest::parse(body)
            .map(drop)
            .map_err(e),
        "participation_cease" => ops::ParticipationCeaseRequest::parse(body)
            .map(drop)
            .map_err(e),
        "charter_propose" => ops::CharterProposeRequest::parse(body).map(drop).map_err(e),
        _ => Ok(()),
    }
}

/// Registry-bound on this surface but not implemented in this slice:
/// honestly unavailable, never silently absent.
fn feature_unavailable() -> Problem {
    Problem::new(
        ProblemKind::FeatureUnavailable,
        "operation is not implemented in this slice (see feature_info)",
    )
    .with_status(501)
}

/// The operation body minus the envelope members (the self-policy record
/// a policy adoption retains).
fn strip_envelope(body: &Value) -> Value {
    let mut m = body.as_object().cloned().unwrap_or_default();
    m.remove("version");
    m.remove("op");
    m.remove("meta");
    Value::Object(m)
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
