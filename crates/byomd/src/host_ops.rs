//! B3 slice 1 — byom's side of the Kovee seam (C2 `byom_governed_work_v1`;
//! DESIGN.md §16.3; family contract R39/R40/R42, §2.B L10–L15).
//!
//! This module holds `kovee_endeavor_form` (governance, create; R39):
//! the delegated-principal channel. The DelegatedPrincipalCredential is
//! channel material read off the transport preamble; the request body
//! carries only the stable command plus a fresh attempt envelope. On
//! success ONE atomic commit: Position + immutable GovernanceDecision +
//! active Endeavor + idempotency result + events + the §15.3
//! authority-journal transition.
//!
//! Three things the code makes structural rather than trusted:
//!
//! 1. **The stable/fresh split.** The idempotency-covered body is
//!    `{version, op, meta, command}` — attempt id, nonce, recovery
//!    binding, observation and proof are projected OUT. A retry with a
//!    fresh envelope therefore replays the stored result; a changed
//!    command byte is `idempotency_mismatch`.
//! 2. **Server recomputation.** Every digest §16.3 says the server
//!    recomputes is recomputed here from the bytes; the request field
//!    can only match.
//! 3. **One terminal row per external domain.**
//!    `external_command_domains` has ONE row keyed by the
//!    IdempotencyDomain digest whose `state` is `committed` or
//!    `tombstoned`, so "a delayed command racing terminalization — one
//!    wins, both cannot" is a primary key, not a code path.

use bpp_core::digest::DigestRef;
use bpp_core::hostint::{self, DelegatedPrincipalCredential};
use bpp_core::ops;
use bpp_core::problem::{Problem, ProblemKind};
use bpp_core::time::rfc3339_utc;
use byom_store::effects::Effect;
use byom_store::rows;
use byom_store::{CrashHooks, CursorMint, MutationScope, Prepared, Store};
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::gov_ops::{check_meta_binding, db_err, digest_json, mint, obj_pairs, run};
use crate::host_config::HostConfig;
use crate::part_common::{self, Seat};
use crate::part_ops::event;
use crate::state;

/// The channel-derived actor of a delegated principal. It is NEVER a
/// request field: it is built from the authenticated credential, so the
/// server-recomputed IdempotencyDomain and actor binding belong to the
/// principal, not to the gateway.
pub fn principal_actor(source_principal_ref: &str) -> String {
    format!("kovee-principal:{source_principal_ref}")
}

/// The two terminal states of one external IdempotencyDomain.
pub const STATE_COMMITTED: &str = "committed";
pub const STATE_TOMBSTONED: &str = "tombstoned";

pub fn conflict(detail: &str) -> Problem {
    Problem::new(
        ProblemKind::IdempotencyMismatch,
        "the external idempotency domain does not match this command",
    )
    .with_status(409)
    .with_detail(detail.to_owned())
}

// ------------------------------------------------ credential checking ----

/// The verified delegated principal: the credential plus the
/// channel-derived actor string every downstream derivation uses.
pub struct Principal {
    pub credential: DelegatedPrincipalCredential,
    pub actor: String,
}

/// The four-member binding pin of a `KoveeRealmByomBinding` (§16.6 item
/// 1: every formation intent and slot pins ref, revision, epoch, digest).
pub fn binding_pinned(
    binding: &bpp_core::hostint::KoveeRealmByomBinding,
    r: &str,
    revision: u64,
    epoch: u64,
    digest: &DigestRef,
) -> bool {
    binding.binding_ref == r
        && binding.binding_revision == revision
        && binding.binding_epoch == epoch
        && &binding.digest == digest
}

/// The complete §14.4/§16.3 credential check for one operation:
/// issuer, audience, operation family, short expiry, endpoint
/// incarnation, realm-binding quadruple, Society and recovery epoch.
/// Every failure is a plain `forbidden`/`stale_binding` — a refusal
/// discloses nothing about Society contents.
pub fn verify_credential(
    store: &Store,
    cfg: &HostConfig,
    credential: &DelegatedPrincipalCredential,
    operation: &str,
    society_recovery_epoch: u64,
    now: i64,
) -> Result<Principal, Problem> {
    let binding = &cfg.realm_byom_binding;
    if !cfg
        .delegated_principal_issuers
        .contains(&credential.issuer_ref)
    {
        return Err(state::forbidden_detail(
            "credential issuer is not a delegated-principal gateway of this binding",
        ));
    }
    if credential.audience != binding.delegated_principal_audience {
        return Err(state::forbidden_detail(
            "credential audience is not this binding's delegated_principal_audience",
        ));
    }
    if !credential
        .allowed_operations
        .iter()
        .any(|op| op == operation)
    {
        return Err(state::forbidden_detail(
            "credential workload does not allow this operation",
        ));
    }
    if !credential.live_at(now) {
        return Err(state::forbidden_detail("credential is outside its window"));
    }
    let incarnation = store
        .incarnation()
        .map_err(|e| state::internal(&e.to_string()))?;
    if credential.endpoint_incarnation != incarnation {
        return Err(state::stale_binding(
            "credential names another endpoint incarnation",
        ));
    }
    if !binding_pinned(
        binding,
        &credential.realm_byom_binding_ref,
        credential.realm_byom_binding_revision,
        credential.realm_byom_binding_epoch,
        &credential.realm_byom_binding_digest,
    ) {
        return Err(state::stale_binding(
            "credential does not pin the installed KoveeRealmByomBinding",
        ));
    }
    if credential.society_ref != cfg.society_mapping.society_ref {
        return Err(state::stale_binding(
            "credential names a Society outside this KoveeSocietyMapping",
        ));
    }
    if credential.society_recovery_epoch != society_recovery_epoch {
        return Err(state::stale_binding(
            "credential names a superseded Society recovery epoch",
        ));
    }
    Ok(Principal {
        actor: principal_actor(&credential.source_principal_ref),
        credential: credential.clone(),
    })
}

/// The same check, for a channel that has not yet named an operation
/// (the R41 originating-surface recovery reads): the credential must
/// still belong to the installed binding, name this endpoint's
/// incarnation, sit inside its window, and cover the Society at its
/// current recovery epoch — so a preamble can never invent a principal
/// outside the configured seam.
pub fn verify_channel(
    store: &Store,
    credential: &DelegatedPrincipalCredential,
    now: i64,
) -> Result<Principal, Problem> {
    let cfg = HostConfig::load(store)?;
    let society = rows::get_society(store.conn(), &cfg.society_mapping.society_ref)
        .map_err(db_err)?
        .ok_or_else(state::not_found)?;
    let operation = credential
        .allowed_operations
        .first()
        .cloned()
        .unwrap_or_default();
    verify_credential(
        store,
        &cfg,
        credential,
        &operation,
        society.recovery_epoch,
        now,
    )
}

// ------------------------------------------------------- domain rows ----

/// The one terminal row of an external IdempotencyDomain, if it exists.
pub fn domain_row(
    conn: &Connection,
    external_digest_hex: &str,
) -> Result<Option<Map<String, Value>>, Problem> {
    rows::get_row(
        conn,
        "external_command_domains",
        "external_domain_digest",
        external_digest_hex,
    )
    .map_err(db_err)
}

/// The stored (internal domain, outcome) of a consumed (issuer, nonce).
pub fn consumption(
    conn: &Connection,
    issuer: &str,
    nonce: &str,
) -> Result<Option<(String, String)>, Problem> {
    let mut stmt = conn
        .prepare(
            "SELECT internal_domain_digest, outcome FROM delegated_credential_consumptions
             WHERE issuer_ref = ?1 AND nonce = ?2",
        )
        .map_err(db_err)?;
    let mut found = stmt.query([issuer, nonce]).map_err(db_err)?;
    match found.next().map_err(db_err)? {
        Some(r) => Ok(Some((r.get(0).map_err(db_err)?, r.get(1).map_err(db_err)?))),
        None => Ok(None),
    }
}

/// Is an authority transition over this internal domain prepared or in
/// flight (§16.3 `prepared_or_in_flight`)?
pub fn in_flight(conn: &Connection, internal_digest_hex: &str) -> Result<bool, Problem> {
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM authority_pending
             WHERE idempotency_domain_digest = ?1 AND state IN ('prepared','witness_unknown')",
            [internal_digest_hex],
            |r| r.get(0),
        )
        .map_err(db_err)?;
    Ok(n > 0)
}

// ------------------------------------------------------ shared bodies ----

/// The §16.3 embedded EndeavorProposal body: the exact argument members
/// of the B0.1 `endeavor_propose` subject (which normatively owns this
/// shape), carried opaque on the wire and parsed here.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct FormationProposal {
    purpose_ref: String,
    purpose_digest: DigestRef,
    sponsor_participant_refs: Vec<String>,
    governance_rule_set_ref: String,
    outcome_schema_refs: Vec<String>,
    acceptance_rule_ref: String,
    classification_join_ref: String,
    budget_account_set_ref: String,
    #[serde(default)]
    deadline: Option<String>,
}

/// The §16.3 embedded source-principal Position body. Closed on purpose:
/// the operation "cannot import an offline Position, fill another
/// Participant's seat, or invoke an automatic assent policy", so the
/// only admissible mode is the principal's own direct assent.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourcePosition {
    participant_ref: String,
    value: String,
    assent_mode: String,
    #[serde(default)]
    reason_ref: Option<String>,
}

/// The computed formation slot snapshot (§16.3: the server recomputes
/// it; the request can only match). It names no server-minted id, so
/// Kovee derives the identical bytes from the proposal it authored.
fn slot_snapshot(command: &ops::KoveeEndeavorFormCommand, proposal: &FormationProposal) -> Value {
    let mut seats: Vec<Value> = proposal
        .sponsor_participant_refs
        .iter()
        .map(|p| json!({"kind": "sponsor", "participant_ref": p, "surface": "participant"}))
        .collect();
    seats.sort_by_key(|s| s["participant_ref"].as_str().unwrap_or_default().to_owned());
    json!({
        "society_ref": command.society_ref,
        "society_recovery_epoch": command.society_recovery_epoch,
        "governance_rule_set_ref": proposal.governance_rule_set_ref,
        "endeavor_proposal_digest": digest_json(&command.endeavor_proposal_digest),
        "required_seats": seats,
    })
}

/// The idempotency-covered projection of a form request: version, op,
/// meta and the STABLE command — never the per-attempt envelope. This
/// projection IS the stable-command/fresh-attempt split (§16.3).
fn stable_body(body: &Value) -> Value {
    let mut m = Map::new();
    for key in ["version", "op", "meta", "command"] {
        if let Some(v) = body.get(key) {
            m.insert(key.to_owned(), v.clone());
        }
    }
    Value::Object(m)
}

/// The problem a terminally claimed domain answers with — a
/// deterministic function of the stored tombstone, so a replay answers
/// identically without re-executing.
pub fn tombstone_problem(
    reason_kind: &str,
    reason: &str,
    tombstone_ref: &str,
    digest: &Value,
) -> Problem {
    let kind = if reason_kind == "formation_requires_participation" {
        ProblemKind::FormationRequiresParticipation
    } else {
        ProblemKind::Forbidden
    };
    let title = if kind == ProblemKind::FormationRequiresParticipation {
        "the computed formation needs another Participant's Position"
    } else {
        "this IdempotencyDomain is terminally claimed by a non-reexecuting tombstone"
    };
    let mut problem = Problem::new(kind, title)
        .with_status(409)
        .with_detail(reason.to_owned());
    problem
        .extensions
        .push(("dev.byom.tombstone_ref".to_owned(), json!(tombstone_ref)));
    problem
        .extensions
        .push(("dev.byom.tombstone_digest".to_owned(), digest.clone()));
    problem.extensions.push((
        "dev.byom.tombstone_reason_kind".to_owned(),
        json!(reason_kind),
    ));
    problem
}

/// Maps one retained result envelope back to the caller's answer: a
/// committed formation replays its bytes, a tombstone replays its
/// problem. Both are pure functions of the stored bytes.
fn replay(bytes: Vec<u8>) -> Result<Vec<u8>, Problem> {
    let parsed: Value =
        serde_json::from_slice(&bytes).map_err(|e| state::internal(&e.to_string()))?;
    let result = &parsed["result"];
    if result["kovee_external_command_outcome"] == json!("non_reexecuting_tombstone") {
        return Err(tombstone_problem(
            result["tombstone_reason_kind"].as_str().unwrap_or_default(),
            result["tombstone_reason"].as_str().unwrap_or_default(),
            result["tombstone_ref"].as_str().unwrap_or_default(),
            &result["tombstone_digest"],
        ));
    }
    Ok(bytes)
}

/// One `external_command_domains` row — the single terminal claim over
/// an external IdempotencyDomain.
#[allow(clippy::too_many_arguments)]
pub fn domain_effect_row(
    external_hex: &str,
    society_id: &str,
    operation: &str,
    incarnation: &str,
    recovery_epoch: u64,
    idempotency_key: &str,
    canonical_command_hex: &str,
    intent_ref: &str,
    source_principal_ref: &str,
    source_actor_binding_hex: &str,
    internal_hex: &str,
    state: &str,
    committed: Option<(&Value, &DigestRef, &str)>,
    tombstone: Option<(&str, &DigestRef, &str, &str)>,
    created_at: &str,
) -> Map<String, Value> {
    obj_pairs([
        ("external_domain_digest", json!(external_hex)),
        ("society_id", json!(society_id)),
        ("operation", json!(operation)),
        ("endpoint_incarnation", json!(incarnation)),
        ("society_recovery_epoch", json!(recovery_epoch)),
        ("byom_command_idempotency_key", json!(idempotency_key)),
        ("canonical_command_digest", json!(canonical_command_hex)),
        ("kovee_formation_intent_ref", json!(intent_ref)),
        ("source_principal_ref", json!(source_principal_ref)),
        (
            "source_actor_binding_digest",
            json!(source_actor_binding_hex),
        ),
        ("internal_domain_digest", json!(internal_hex)),
        ("state", json!(state)),
        (
            "result_envelope",
            committed
                .map(|(e, _, _)| json!(e.to_string()))
                .unwrap_or(Value::Null),
        ),
        (
            "result_digest",
            committed
                .map(|(_, d, _)| json!(d.value_hex))
                .unwrap_or(Value::Null),
        ),
        (
            "result_signature",
            committed.map(|(_, _, s)| json!(s)).unwrap_or(Value::Null),
        ),
        (
            "tombstone_ref",
            tombstone
                .map(|(r, _, _, _)| json!(r))
                .unwrap_or(Value::Null),
        ),
        (
            "tombstone_digest",
            tombstone
                .map(|(_, d, _, _)| json!(digest_json(d).to_string()))
                .unwrap_or(Value::Null),
        ),
        (
            "tombstone_reason",
            tombstone
                .map(|(_, _, r, _)| json!(r))
                .unwrap_or(Value::Null),
        ),
        (
            "tombstone_reason_kind",
            tombstone
                .map(|(_, _, _, k)| json!(k))
                .unwrap_or(Value::Null),
        ),
        ("created_at", json!(created_at)),
    ])
}

/// The short authenticated continuation, derivable inside an open
/// prepare transaction (same secret and derivation as
/// `Store::mint_short_cursor`).
pub fn short_cursor(conn: &Connection, society_id: &str, seq: u64) -> Result<String, Problem> {
    use bpp_core::canonical::{hex, hmac_sha256};
    let secret = byom_store::schema::meta_get(conn, "cursor_secret")
        .map_err(|e| state::internal(&e.to_string()))?
        .ok_or_else(|| state::internal("store is not bootstrapped"))?;
    let bound = format!("bs1|projection|events:{society_id}|{seq}");
    let tag = hmac_sha256(&secret, bound.as_bytes());
    Ok(format!("bs1.{society_id}.{seq:x}.{}", hex(&tag[..16])))
}

/// The endpoint signature, derivable inside an open prepare transaction
/// (same derivation as `Store::endpoint_sign`).
pub fn conn_sign(conn: &Connection, payload: &Value) -> Result<String, Problem> {
    use bpp_core::canonical::{hex, hmac_sha256, jcs};
    let root = byom_store::schema::meta_get(conn, "index_root_key")
        .map_err(|e| state::internal(&e.to_string()))?
        .ok_or_else(|| state::internal("store is not bootstrapped"))?;
    let key = hmac_sha256(&root, b"endpoint-result-signature");
    let preimage = jcs(payload).map_err(|e| state::internal(&e.to_string()))?;
    Ok(format!("sig1.{}", hex(&hmac_sha256(&key, &preimage))))
}

// ------------------------------------------------- kovee_endeavor_form ----

#[allow(clippy::too_many_lines)]
pub fn kovee_endeavor_form(
    store: &mut Store,
    credential: &DelegatedPrincipalCredential,
    req: &ops::KoveeEndeavorFormRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    let cfg = HostConfig::load(store)?;
    let command = &req.command;

    // §16.3: an already ACTIVE Society — Kovee's gateway is NEVER the
    // genesis actor. A Society that does not exist is refused before any
    // credential state is touched, and the refusal names the native path.
    let society = rows::get_society(store.conn(), &command.society_ref)
        .map_err(db_err)?
        .ok_or_else(|| {
            state::forbidden_detail(
                "kovee_endeavor_form never bootstraps a Society: establish it natively through \
                 society_prepare/society_bootstrap on the direct governance channel",
            )
        })?;
    if society.state != "active" {
        return Err(state::stale_binding("Society is not active"));
    }
    let principal = verify_credential(
        store,
        &cfg,
        credential,
        "kovee_endeavor_form",
        society.recovery_epoch,
        now,
    )?;

    // The pinned binding/mapping preconditions (§16.6 item 1; L2/L8).
    if !binding_pinned(
        &cfg.realm_byom_binding,
        &command.realm_byom_binding_ref,
        command.realm_byom_binding_revision,
        command.realm_byom_binding_epoch,
        &command.realm_byom_binding_digest,
    ) {
        return Err(state::stale_binding(
            "command does not pin the installed KoveeRealmByomBinding",
        ));
    }
    if command.byom_endpoint_ref != cfg.realm_byom_binding.byom_endpoint_ref {
        return Err(state::stale_binding("command names another byom endpoint"));
    }
    let incarnation = store
        .incarnation()
        .map_err(|e| state::internal(&e.to_string()))?;
    if command.command_endpoint_incarnation != incarnation {
        return Err(state::stale_binding(
            "command names a superseded endpoint incarnation",
        ));
    }
    if cfg.society_mapping.society_ref != command.society_ref
        || cfg.society_mapping.society_recovery_epoch != society.recovery_epoch
    {
        return Err(state::stale_binding(
            "no active KoveeSocietyMapping covers this Society at its current recovery epoch",
        ));
    }
    if command.society_recovery_epoch != society.recovery_epoch {
        return Err(state::stale_binding(
            "command names a superseded Society recovery epoch",
        ));
    }
    if !cfg.recovery_binding.matches(
        &req.attempt_recovery_binding_ref,
        req.attempt_recovery_binding_revision,
        req.attempt_recovery_binding_epoch,
        &req.attempt_recovery_binding_digest,
    ) {
        return Err(state::stale_binding(
            "the attempt does not pin the current recovery binding",
        ));
    }
    check_meta_binding(store, &req.meta, &society.society_id)?;

    // §16.3: the authenticated delegated principal is the ONLY author;
    // every actor-derived request field can only match the channel.
    let cred = &principal.credential;
    if command.source_principal_ref != cred.source_principal_ref
        || command.source_actor_binding_digest != cred.source_actor_binding_digest
    {
        return Err(state::forbidden_detail(
            "the command names another source principal than the authenticated channel",
        ));
    }
    if cred.delegated_principal_subject_digest != req.canonical_command_digest {
        return Err(state::forbidden_detail(
            "the credential is not bound to this exact prepared command",
        ));
    }

    // The admitted human Participant the principal acts for (R39: never
    // manufactured membership).
    let admission = |detail: &str| {
        Problem::new(ProblemKind::AdmissionRequired, detail)
            .with_status(403)
            .with_detail("kovee_endeavor_form acts for an already admitted human Participant")
    };
    let participant = rows::get_participant(store.conn(), &cred.bound_participant_ref)
        .map_err(db_err)?
        .ok_or_else(|| admission("the credential's bound Participant is not admitted"))?;
    if participant.society_id != society.society_id
        || participant.kind != "human"
        || participant.state != "active"
        || participant.standing_ref.is_none()
    {
        return Err(admission(
            "the bound Participant is not an active human Participant with Standing",
        ));
    }
    if participant.binding_epoch != cred.participant_binding_epoch {
        return Err(state::stale_binding(
            "the credential pins a superseded Participant binding epoch",
        ));
    }

    // The server-recomputed internal IdempotencyDomain and the fresh
    // per-attempt authentication binding (§16.3).
    let scope = MutationScope {
        society_id: society.society_id.clone(),
        operation: "kovee_endeavor_form".into(),
        actor: principal.actor.clone(),
        meta: req.meta.clone(),
        body: stable_body(body),
    };
    let internal_domain = store
        .domain_digest(&scope)
        .map_err(|e| state::internal(&e.to_string()))?;
    let expected_proof = hostint::attempt_proof(
        &req.canonical_command_digest,
        &command.idempotency_domain_digest,
        &req.attempt_nonce,
        &req.attempt_recovery_binding_digest,
        &cred.source_actor_binding_digest,
    )
    .map_err(|e| state::internal(&e))?;
    if expected_proof != req.authentication_proof {
        return Err(state::forbidden_detail(
            "the attempt proof does not bind this command, domain, nonce, recovery binding and \
             actor binding",
        ));
    }

    // Atomic (issuer, nonce) consume: a replayed nonce re-serves the
    // stored result and never re-executes (family contract L5–L6).
    if let Some((stored, _)) = consumption(store.conn(), &cred.issuer_ref, &cred.nonce)? {
        let Some((_, bytes)) = store
            .lookup_idempotency(&stored)
            .map_err(|e| state::internal(&e.to_string()))?
        else {
            return Err(state::internal("consumed nonce has no retained result"));
        };
        return replay(bytes);
    }

    // The one terminal row per external domain.
    let external_hex = command.idempotency_domain_digest.value_hex.clone();
    if let Some(row) = domain_row(store.conn(), &external_hex)? {
        if rows::str_of(&row, "canonical_command_digest") != req.canonical_command_digest.value_hex
        {
            return Err(conflict(
                "this IdempotencyDomain is already bound to another canonical command",
            ));
        }
        if rows::str_of(&row, "internal_domain_digest") != internal_domain.value_hex {
            return Err(conflict(
                "this IdempotencyDomain is already bound to another byom command domain",
            ));
        }
        if rows::str_of(&row, "state") == STATE_TOMBSTONED {
            return Err(tombstone_problem(
                rows::str_of(&row, "tombstone_reason_kind"),
                rows::str_of(&row, "tombstone_reason"),
                rows::str_of(&row, "tombstone_ref"),
                &rows::json_of(&row, "tombstone_digest"),
            ));
        }
    }

    // The opaque §16.3 bodies and the digests the server recomputes.
    let proposal: FormationProposal = serde_json::from_value(command.endeavor_proposal.clone())
        .map_err(|e| state::invalid(&format!("endeavor_proposal: {e}")))?;
    let position: SourcePosition =
        serde_json::from_value(command.source_principal_position.clone())
            .map_err(|e| state::invalid(&format!("source_principal_position: {e}")))?;
    if hostint::portable_digest(hostint::PROPOSAL_TAG, &command.endeavor_proposal)
        .map_err(|e| state::internal(&e))?
        != command.endeavor_proposal_digest
    {
        return Err(state::invalid(
            "endeavor_proposal_digest does not cover the proposal bytes",
        ));
    }
    if hostint::portable_digest(hostint::POSITION_TAG, &command.source_principal_position)
        .map_err(|e| state::internal(&e))?
        != command.source_principal_position_digest
    {
        return Err(state::invalid(
            "source_principal_position_digest does not cover the position bytes",
        ));
    }
    if command.expected_governance_rule_set_ref != proposal.governance_rule_set_ref {
        return Err(state::invalid(
            "expected_governance_rule_set_ref is not the proposal's active rule set",
        ));
    }
    let snapshot = slot_snapshot(command, &proposal);
    let snapshot_digest = hostint::portable_digest(hostint::SLOT_SNAPSHOT_TAG, &snapshot)
        .map_err(|e| state::internal(&e))?;
    if snapshot_digest != command.expected_slot_snapshot_digest {
        return Err(state::invalid(
            "expected_slot_snapshot_digest is not the server-computed formation snapshot",
        ));
    }

    // The sole-computed-seat rule (§16.3): exactly ONE required seat, and
    // the source principal's own explicit Position fills it. Anything
    // else is `formation_requires_participation` — the participants use
    // ordinary endeavor_propose/position/finalize instead.
    let requires_participation = if proposal.sponsor_participant_refs.len() != 1 {
        Some("the computed formation snapshot requires more than one seat")
    } else if proposal.sponsor_participant_refs[0] != cred.bound_participant_ref
        || position.participant_ref != cred.bound_participant_ref
    {
        Some("the one required seat belongs to another Participant")
    } else if position.assent_mode != "direct_participant" {
        Some("the carried Position is not the principal's own direct assent")
    } else if position.value != "assent" {
        Some("the carried Position does not assent to the computed subject")
    } else {
        None
    };

    // Pre-minted identifiers and pre-derived records: stable across CAS
    // revalidation, so a re-run of the prepare closure is byte-identical.
    let society_id = society.society_id.clone();
    let endeavor_id = mint(store, "end")?;
    let decision_ref = mint(store, "dec-kovee-formation")?;
    let position_id = mint(store, "pos")?;
    let seat_ref = mint(store, "seat-sponsor")?;
    let tombstone_ref = mint(store, "tomb")?;
    let dependency_set_ref = mint(store, "deps")?;
    let event_ids: Vec<String> = (0..4)
        .map(|_| mint(store, "evt"))
        .collect::<Result<_, _>>()?;
    let created_at = rfc3339_utc(now);

    let subject = json!({
        "endeavor_id": endeavor_id,
        "purpose_ref": proposal.purpose_ref,
        "purpose_digest": digest_json(&proposal.purpose_digest),
        "sponsor_participant_refs": proposal.sponsor_participant_refs,
        "governance_rule_set_ref": proposal.governance_rule_set_ref,
        "outcome_schema_refs": proposal.outcome_schema_refs,
        "acceptance_rule_ref": proposal.acceptance_rule_ref,
        "classification_join_ref": proposal.classification_join_ref,
        "budget_account_set_ref": proposal.budget_account_set_ref,
        "deadline": proposal.deadline.clone().map(Value::from).unwrap_or(Value::Null),
    });
    let subject_digest = store
        .record_digest(
            &society_id,
            &endeavor_id,
            "bpp-endeavor-subject-v0",
            &subject,
        )
        .map_err(|e| state::internal(&e.to_string()))?;
    let rid = &req.meta.request_id;
    let trace = part_common::prepare_trace(
        store,
        &society_id,
        "kovee_endeavor_form",
        &principal.actor,
        rid,
        body,
        &subject_digest,
        &dependency_set_ref,
        vec![
            part_common::source_row("/endeavor_id", rid, "/command", "t-mint-id"),
            part_common::source_row(
                "/purpose_ref",
                rid,
                "/command/endeavor_proposal/purpose_ref",
                "t-copy",
            ),
            part_common::source_row(
                "/sponsor_participant_refs",
                rid,
                "/command/endeavor_proposal/sponsor_participant_refs",
                "t-copy",
            ),
            part_common::source_row(
                "/governance_rule_set_ref",
                rid,
                "/command/endeavor_proposal/governance_rule_set_ref",
                "t-copy",
            ),
        ],
        now,
    )?;
    let position_record = json!({
        "position_id": position_id,
        "proposal_kind": "endeavor",
        "proposal_ref": endeavor_id,
        "proposal_revision": 1,
        "seat_ref": seat_ref,
        "participant_ref": cred.bound_participant_ref,
        "value": position.value,
        "status": "active",
        "created_at": created_at,
    });
    let position_digest = store
        .record_digest(
            &society_id,
            &position_id,
            "bpp-position-v0",
            &position_record,
        )
        .map_err(|e| state::internal(&e.to_string()))?;
    let decision = json!({
        "decision_id": decision_ref,
        "society_ref": society_id,
        "kind": "endeavor_formation",
        "subject_kind": "endeavor",
        "subject_ref": endeavor_id,
        "subject_digest": digest_json(&subject_digest),
        "rule_set_ref": proposal.governance_rule_set_ref,
        "seat_snapshot": snapshot,
        "position_refs": [position_id],
        "source": "kovee_endeavor_form",
        "created_at": created_at,
    });
    let decision_digest = hostint::portable_digest(hostint::DECISION_TAG, &decision)
        .map_err(|e| state::internal(&e))?;
    let endeavor_digest = hostint::portable_digest(
        hostint::ENDEAVOR_TAG,
        &json!({
            "endeavor_ref": endeavor_id,
            "endeavor_revision": 1,
            "society_ref": society_id,
            "state": "active",
            "purpose_ref": proposal.purpose_ref,
            "governance_rule_set_ref": proposal.governance_rule_set_ref,
            "formation_decision_ref": decision_ref,
            "formation_slot_snapshot_digest": digest_json(&snapshot_digest),
        }),
    )
    .map_err(|e| state::internal(&e))?;
    let tombstone = json!({
        "tombstone_ref": tombstone_ref,
        "society_ref": society_id,
        "society_recovery_epoch": society.recovery_epoch,
        "operation": "kovee_endeavor_form",
        "idempotency_domain_digest": digest_json(&command.idempotency_domain_digest),
        "canonical_command_digest": digest_json(&req.canonical_command_digest),
        "reason_kind": "formation_requires_participation",
        "reason": requires_participation.unwrap_or_default(),
        "created_at": created_at,
    });
    let tombstone_digest = hostint::portable_digest(hostint::TOMBSTONE_TAG, &tombstone)
        .map_err(|e| state::internal(&e))?;

    let cred = cred.clone();
    let command = command.clone();
    let canonical_command_digest = req.canonical_command_digest.clone();
    let meta = req.meta.clone();
    let internal_hex = internal_domain.value_hex.clone();
    let intent_ref = command.kovee_formation_intent_ref.clone();
    let actor = principal.actor.clone();

    let bytes = run(store, scope, now, hooks, move |conn, _| {
        // Dependency revalidation inside the open prepare transaction.
        let society = rows::get_society(conn, &society_id)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        if society.state != "active" || society.recovery_epoch != command.society_recovery_epoch {
            return Err(state::stale_binding("Society moved under this command"));
        }
        let participant = rows::get_participant(conn, &cred.bound_participant_ref)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        if participant.state != "active"
            || participant.binding_epoch != cred.participant_binding_epoch
        {
            return Err(state::stale_binding("the bound Participant moved"));
        }
        if consumption(conn, &cred.issuer_ref, &cred.nonce)?.is_some() {
            return Err(state::forbidden_detail("credential nonce already consumed"));
        }
        if let Some(row) = domain_row(conn, &external_hex)? {
            if rows::str_of(&row, "state") == STATE_TOMBSTONED {
                return Err(tombstone_problem(
                    rows::str_of(&row, "tombstone_reason_kind"),
                    rows::str_of(&row, "tombstone_reason"),
                    rows::str_of(&row, "tombstone_ref"),
                    &rows::json_of(&row, "tombstone_digest"),
                ));
            }
        }

        let consumed = |outcome: &str| Effect::Upsert {
            table: "delegated_credential_consumptions".into(),
            row: obj_pairs([
                ("issuer_ref", json!(cred.issuer_ref)),
                ("nonce", json!(cred.nonce)),
                ("credential_id", json!(cred.credential_id)),
                ("society_id", json!(society_id)),
                ("operation", json!("kovee_endeavor_form")),
                ("source_principal_ref", json!(cred.source_principal_ref)),
                ("external_domain_digest", json!(external_hex)),
                (
                    "canonical_command_digest",
                    json!(canonical_command_digest.value_hex),
                ),
                ("internal_domain_digest", json!(internal_hex)),
                ("outcome", json!(outcome)),
                ("consumed_at", json!(created_at)),
            ]),
        };
        let domain = |state: &str,
                      committed: Option<(&Value, &DigestRef, &str)>,
                      tomb: Option<(&str, &DigestRef, &str, &str)>| {
            Effect::Upsert {
                table: "external_command_domains".into(),
                row: domain_effect_row(
                    &external_hex,
                    &society_id,
                    "kovee_endeavor_form",
                    &incarnation,
                    society.recovery_epoch,
                    &command.byom_command_idempotency_key,
                    &canonical_command_digest.value_hex,
                    &intent_ref,
                    &cred.source_principal_ref,
                    &cred.source_actor_binding_digest.value_hex,
                    &internal_hex,
                    state,
                    committed,
                    tomb,
                    &created_at,
                ),
            }
        };

        // ---- formation_requires_participation: a DEFINITE pre-commit
        // rejection that claims the idempotency domain with a
        // non-reexecuting tombstone and commits NO Society or Endeavor
        // domain record at all.
        if let Some(reason) = requires_participation {
            return Ok(Prepared {
                result: json!({
                    "kovee_external_command_outcome": "non_reexecuting_tombstone",
                    "kovee_formation_intent_ref": intent_ref,
                    "idempotency_domain_digest": digest_json(&command.idempotency_domain_digest),
                    "canonical_command_digest": digest_json(&canonical_command_digest),
                    "tombstone_ref": tombstone_ref,
                    "tombstone_digest": digest_json(&tombstone_digest),
                    "tombstone_reason": reason,
                    "tombstone_reason_kind": "formation_requires_participation",
                }),
                revision: None,
                cursor: CursorMint::AfterEvents {
                    society_id: society_id.clone(),
                },
                effects: vec![
                    consumed(STATE_TOMBSTONED),
                    domain(
                        STATE_TOMBSTONED,
                        None,
                        Some((
                            &tombstone_ref,
                            &tombstone_digest,
                            reason,
                            "formation_requires_participation",
                        )),
                    ),
                ],
                events: vec![event(
                    &society_id,
                    &event_ids[0],
                    "kovee.formation_tombstoned",
                    &tombstone_ref,
                    1,
                    &cred.bound_participant_ref,
                    &actor,
                    &meta,
                    json!({"reason_kind": "formation_requires_participation",
                           "reason": reason,
                           "kovee_formation_intent_ref": intent_ref}),
                )],
            });
        }

        // ---- the success path: ONE atomic commit.
        let seats = vec![Seat {
            seat_ref: seat_ref.clone(),
            kind: "sponsor".into(),
            participant_ref: cred.bound_participant_ref.clone(),
            surface: "participant".into(),
        }];
        let mut effects = Vec::new();
        // §11.4 conservation: formation delegates the endeavor ceiling
        // from the Society root, exactly as native finalization does.
        part_common::delegate_child(
            conn,
            &mut effects,
            &society_id,
            &society.root_budget_account_set_ref,
            &proposal.budget_account_set_ref,
            part_common::UNIT_DIMENSION,
            part_common::ENDEAVOR_CEILING,
            now,
        )?;
        effects.push(Effect::Upsert {
            table: "endeavors".into(),
            row: obj_pairs([
                ("endeavor_id", json!(endeavor_id)),
                ("society_id", json!(society_id)),
                ("revision", json!(1)),
                ("state", json!("active")),
                ("purpose_ref", json!(proposal.purpose_ref)),
                ("purpose_digest", digest_json(&proposal.purpose_digest)),
                (
                    "sponsor_participant_refs",
                    json!(json!(proposal.sponsor_participant_refs).to_string()),
                ),
                (
                    "governance_rule_set_ref",
                    json!(proposal.governance_rule_set_ref),
                ),
                (
                    "outcome_schema_refs",
                    json!(json!(proposal.outcome_schema_refs).to_string()),
                ),
                ("acceptance_rule_ref", json!(proposal.acceptance_rule_ref)),
                (
                    "classification_join_ref",
                    json!(proposal.classification_join_ref),
                ),
                (
                    "budget_account_set_ref",
                    json!(proposal.budget_account_set_ref),
                ),
                (
                    "deadline",
                    proposal
                        .deadline
                        .clone()
                        .map(Value::from)
                        .unwrap_or(Value::Null),
                ),
                ("subject_digest", digest_json(&subject_digest)),
                (
                    "required_seats",
                    json!(part_common::seats_json(&seats).to_string()),
                ),
                ("preparation_trace", json!(trace.to_string())),
                ("formation_decision_ref", json!(decision_ref)),
                ("created_at", json!(created_at)),
            ]),
        });
        // The append-only PositionRevision plus its separate seat-head
        // CAS row (BY-P1).
        let bound_participant = rows::get_participant(conn, &cred.bound_participant_ref)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        effects.push(Effect::Upsert {
            table: "position_revisions".into(),
            row: obj_pairs([
                ("position_id", json!(position_id)),
                ("society_id", json!(society_id)),
                ("proposal_kind", json!("endeavor")),
                ("proposal_ref", json!(endeavor_id)),
                ("proposal_revision", json!(1)),
                ("seat_ref", json!(seat_ref)),
                ("participant_ref", json!(cred.bound_participant_ref)),
                (
                    "participant_binding_epoch",
                    json!(bound_participant.binding_epoch),
                ),
                ("actor_ref", json!(actor)),
                (
                    "authentication_observation",
                    json!("kovee-delegated-principal-credential"),
                ),
                ("endpoint_incarnation", json!(incarnation)),
                ("recovery_epoch", json!(society.recovery_epoch)),
                ("value", json!(position.value)),
                ("status", json!("active")),
                ("revision", json!(1)),
                ("assent_mode", json!(position.assent_mode)),
                ("derived_assent_receipt_ref", Value::Null),
                (
                    "reason_ref",
                    position
                        .reason_ref
                        .clone()
                        .map(Value::from)
                        .unwrap_or(Value::Null),
                ),
                ("subject_digest", digest_json(&subject_digest)),
                ("prior_position_digest", Value::Null),
                ("digest", digest_json(&position_digest)),
                ("created_at", json!(created_at)),
            ]),
        });
        effects.push(Effect::Upsert {
            table: "position_seat_heads".into(),
            row: obj_pairs([
                ("proposal_kind", json!("endeavor")),
                ("proposal_ref", json!(endeavor_id)),
                ("seat_ref", json!(seat_ref)),
                ("society_id", json!(society_id)),
                ("position_ref", json!(position_id)),
                ("revision", json!(1)),
                ("value", json!(position.value)),
                ("status", json!("active")),
                ("digest", digest_json(&position_digest)),
                ("updated_at", json!(created_at)),
            ]),
        });
        effects.push(Effect::Upsert {
            table: "governance_decisions".into(),
            row: obj_pairs([
                ("decision_id", json!(decision_ref)),
                ("society_id", json!(society_id)),
                ("kind", json!("endeavor_formation")),
                ("subject_kind", json!("endeavor")),
                ("subject_ref", json!(endeavor_id)),
                ("subject_digest", digest_json(&subject_digest)),
                ("rule_set_ref", json!(proposal.governance_rule_set_ref)),
                ("seat_snapshot", json!(snapshot.to_string())),
                ("position_refs", json!(json!([position_id]).to_string())),
                ("source", json!("kovee_endeavor_form")),
                ("digest", digest_json(&decision_digest)),
                ("created_at", json!(created_at)),
                ("actor_ref", json!(actor)),
                (
                    "dependency_closure",
                    json!(crate::gov_decision::dependency_closure(conn, &society_id)?.to_string()),
                ),
            ]),
        });

        // The signed result envelope Kovee links to, retained for the
        // committed fact of external_command_result_query.
        let head: i64 = conn
            .query_row(
                "SELECT next_event_sequence FROM societies WHERE society_id = ?1",
                [&society_id],
                |r| r.get(0),
            )
            .map_err(db_err)?;
        let source_cursor = short_cursor(conn, &society_id, (head as u64) + 3)?;
        let mut envelope = json!({
            "kovee_formation_intent_ref": intent_ref,
            "canonical_command_digest": digest_json(&canonical_command_digest),
            "society_ref": society_id,
            "society_recovery_epoch": society.recovery_epoch,
            "idempotency_domain_digest": digest_json(&command.idempotency_domain_digest),
            "endeavor_ref": endeavor_id,
            "endeavor_revision": 1,
            "endeavor_digest": digest_json(&endeavor_digest),
            "formation_decision_ref": decision_ref,
            "formation_slot_snapshot_digest": digest_json(&snapshot_digest),
            "source_cursor": source_cursor,
        });
        let envelope_digest = hostint::portable_digest(hostint::RESULT_TAG, &envelope)
            .map_err(|e| state::internal(&e))?;
        envelope["digest"] = digest_json(&envelope_digest);
        let signature = conn_sign(conn, &envelope)?;
        effects.push(consumed(STATE_COMMITTED));
        effects.push(domain(
            STATE_COMMITTED,
            Some((&envelope, &envelope_digest, &signature)),
            None,
        ));

        let who = &cred.bound_participant_ref;
        let events = vec![
            event(
                &society_id,
                &event_ids[0],
                "endeavor.position_recorded",
                &endeavor_id,
                1,
                who,
                &actor,
                &meta,
                json!({"seat_ref": seat_ref, "value": position.value,
                       "assent_mode": position.assent_mode}),
            ),
            event(
                &society_id,
                &event_ids[1],
                "endeavor.finalized",
                &endeavor_id,
                1,
                who,
                &actor,
                &meta,
                json!({"state": "active", "decision_ref": decision_ref}),
            ),
            event(
                &society_id,
                &event_ids[2],
                "budget.delegated",
                &proposal.budget_account_set_ref,
                1,
                who,
                &actor,
                &meta,
                json!({"parent": society.root_budget_account_set_ref,
                       "ceiling": part_common::ENDEAVOR_CEILING,
                       "dimension": part_common::UNIT_DIMENSION}),
            ),
            event(
                &society_id,
                &event_ids[3],
                "kovee.endeavor_formed",
                &endeavor_id,
                1,
                who,
                &actor,
                &meta,
                json!({"kovee_formation_intent_ref": intent_ref,
                       "canonical_command_digest": digest_json(&canonical_command_digest),
                       "idempotency_domain_digest":
                           digest_json(&command.idempotency_domain_digest)}),
            ),
        ];
        Ok(Prepared {
            result: envelope,
            revision: Some(1),
            cursor: CursorMint::AfterEvents {
                society_id: society_id.clone(),
            },
            effects,
            events,
        })
    })?;
    replay(bytes)
}
