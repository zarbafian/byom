//! B3 slice 1 — the recovery surface of the Kovee seam (DESIGN.md
//! §16.3; family contract R42 and R40).
//!
//! - `external_command_result_query` (projection, read; R42): the
//!   **five-fact union**, closed per status. A service reconciler may use
//!   only this: it cannot submit, terminalize, modify, or impersonate the
//!   original human.
//! - `external_command_terminalize` (governance, create; R40): the only
//!   liveness mutation after an ambiguous formation, same source human
//!   through a current lineage-authorized channel, closed three-way
//!   result.
//!
//! The five facts, and the exact precondition each answers:
//!
//! | fact | precondition |
//! |---|---|
//! | `committed` | the domain row is terminal `committed` — its retained signed envelope is re-served |
//! | `non_reexecuting_tombstone` | the domain row is terminal `tombstoned` |
//! | `absent` | LIVE target, complete query, no row and nothing in flight |
//! | `historically_fenced_absent` | HISTORICAL target, a complete verified RestoreLineageProof, no row |
//! | `unknown` | in flight, missing/incomplete/unverifiable lineage, or historical recovery disabled |
//!
//! `absent` never releases the Kovee slot and is never returned for a
//! historical target; missing or incomplete lineage is `unknown`, never
//! live `absent`.

use bpp_core::digest::DigestRef;
use bpp_core::hostint::{self, DelegatedPrincipalCredential, LineageFault, RestoreLineage};
use bpp_core::ops;
use bpp_core::problem::{Problem, ProblemKind};
use bpp_core::time::rfc3339_utc;
use byom_store::effects::Effect;
use byom_store::rows;
use byom_store::{CrashHooks, CursorMint, MutationScope, Prepared, Store};
use serde_json::{json, Value};

use crate::gov_ops::{check_meta_binding, db_err, digest_json, mint, obj_pairs, run};
use crate::host_config::HostConfig;
use crate::host_ops::{
    self, conflict, conn_sign, domain_effect_row, domain_row, in_flight, principal_actor,
    verify_credential, STATE_COMMITTED, STATE_TOMBSTONED,
};
use crate::part_ops::event;
use crate::state;

/// What a historical lookup resolved to before the domain is consulted.
enum Lineage {
    /// The target is the live domain of this endpoint at this epoch.
    Live,
    /// A complete, externally witnessed proof of continuity.
    Fenced {
        proof_ref: String,
        proof_digest: DigestRef,
        hops: Vec<RestoreLineage>,
    },
    /// Missing, incomplete, unavailable, or unverifiable (§16.3): a
    /// query answers `unknown`, a terminalization `not_terminalizable`.
    Unverified(LineageFault),
}

/// The §16.3 historical exception: read historical evidence without
/// accepting an old-incarnation request or reviving old authority.
#[allow(clippy::too_many_arguments)]
fn resolve_lineage(
    cfg: &HostConfig,
    current_incarnation: &str,
    current_epoch: u64,
    target_incarnation: &str,
    target_epoch: u64,
    society_ref: &str,
    domain_digest: &DigestRef,
    proof_ref: Option<&String>,
    proof_digest: Option<&DigestRef>,
) -> Lineage {
    if target_incarnation == current_incarnation && target_epoch == current_epoch {
        return Lineage::Live;
    }
    if cfg.realm_byom_binding.historical_recovery_mode != "exact_formation_intent_only" {
        return Lineage::Unverified(LineageFault::Incomplete(
            "historical recovery is disabled on this binding".to_owned(),
        ));
    }
    let (Some(proof_ref), Some(proof_digest)) = (proof_ref, proof_digest) else {
        return Lineage::Unverified(LineageFault::Incomplete(
            "a historical target requires an externally witnessed RestoreLineageProof".to_owned(),
        ));
    };
    let Some(proof) = cfg.proof(proof_ref) else {
        return Lineage::Unverified(LineageFault::Incomplete(
            "the cited RestoreLineageProof is not retained at this endpoint".to_owned(),
        ));
    };
    if &proof.digest != proof_digest {
        return Lineage::Unverified(LineageFault::Incomplete(
            "the cited proof digest does not pin the retained proof".to_owned(),
        ));
    }
    match proof.verify(
        |r| cfg.lineage(r),
        |l| cfg.witness_verifies(l),
        target_incarnation,
        target_epoch,
        current_incarnation,
        current_epoch,
        society_ref,
        domain_digest,
    ) {
        Ok(hops) => Lineage::Fenced {
            proof_ref: proof.proof_id.clone(),
            proof_digest: proof.digest.clone(),
            hops,
        },
        Err(fault) => Lineage::Unverified(fault),
    }
}

/// The deterministic signed fence receipt a `historically_fenced_absent`
/// answer carries: it names every permanently fenced predecessor domain,
/// so it PROVES the old command can no longer arrive (§16.3) rather than
/// relabelling absence as a tombstone that never existed.
#[allow(clippy::too_many_arguments)]
fn fence_receipt(
    cfg: &HostConfig,
    society_ref: &str,
    target_incarnation: &str,
    target_epoch: u64,
    current_incarnation: &str,
    current_epoch: u64,
    domain_digest: &DigestRef,
    hops: &[RestoreLineage],
) -> Result<(String, DigestRef), Problem> {
    let receipt = json!({
        "endpoint_root_id": cfg.endpoint_root_id,
        "society_ref": society_ref,
        "target_endpoint_incarnation": target_incarnation,
        "target_society_recovery_epoch": target_epoch,
        "current_endpoint_incarnation": current_incarnation,
        "current_society_recovery_epoch": current_epoch,
        "idempotency_domain_digest": digest_json(domain_digest),
        "fenced_predecessor_domains": hops.iter().map(|h| json!({
            "lineage_ref": h.lineage_id,
            "predecessor_endpoint_incarnation": h.predecessor_endpoint_incarnation,
            "predecessor_society_recovery_epoch": h.predecessor_society_recovery_epoch,
            "predecessor_domain_execution": h.predecessor_domain_execution,
            "external_witness_ref": h.external_witness_ref,
        })).collect::<Vec<_>>(),
    });
    let digest = hostint::portable_digest(hostint::FENCE_RECEIPT_TAG, &receipt)
        .map_err(|e| state::internal(&e))?;
    Ok((format!("hfr-{}", &digest.value_hex[..24]), digest))
}

/// Seals one result envelope: sign the body, then digest the signed
/// body (`digest` covers the record minus itself).
fn seal(store: &Store, tag: &str, mut envelope: Value) -> Result<Value, Problem> {
    let signature = store
        .endpoint_sign(&envelope)
        .map_err(|e| state::internal(&e.to_string()))?;
    envelope["server_signature"] = json!(signature);
    let digest = hostint::self_digest(tag, &envelope).map_err(|e| state::internal(&e))?;
    envelope["digest"] = digest_json(&digest);
    Ok(envelope)
}

// ------------------------------ external_command_result_query (R42) ----

#[allow(clippy::too_many_lines)]
pub fn external_command_result_query(
    store: &Store,
    workload_token: Option<&str>,
    req: &ops::ExternalCommandResultQueryRequest,
    now: i64,
) -> Result<Vec<u8>, Problem> {
    let cfg = HostConfig::load(store)?;
    // R42 rides the narrow Kovee recovery workload — never the
    // delegated-principal credential and never an ambient reader.
    let expected = crate::host_config::recovery_workload_token(store, &cfg.realm_byom_binding);
    if expected.is_none() || workload_token.map(str::trim) != expected.as_deref() {
        return Err(state::forbidden_detail(
            "external_command_result_query rides the narrow recovery workload of the installed \
             KoveeRealmByomBinding",
        ));
    }
    let incarnation = store
        .incarnation()
        .map_err(|e| state::internal(&e.to_string()))?;
    if req.current_byom_endpoint_ref != cfg.realm_byom_binding.byom_endpoint_ref
        || req.current_endpoint_incarnation != incarnation
    {
        return Err(state::stale_binding(
            "the query does not name this endpoint's current incarnation",
        ));
    }
    if !cfg.recovery_binding.matches(
        &req.current_recovery_binding_ref,
        req.current_recovery_binding_revision,
        req.current_recovery_binding_epoch,
        &req.current_recovery_binding_digest,
    ) {
        return Err(state::stale_binding(
            "the query does not authenticate through the current recovery binding",
        ));
    }
    if req.target_realm_byom_binding_ref != cfg.realm_byom_binding.binding_ref {
        return Err(state::stale_binding(
            "the query names another Realm binding",
        ));
    }
    if req.target_society_ref != cfg.society_mapping.society_ref {
        return Err(state::stale_binding(
            "the query names a Society outside this KoveeSocietyMapping",
        ));
    }
    if !hostint::DPC_OPERATIONS.contains(&req.operation.as_str()) {
        return Err(state::invalid(
            "operation is not an external command of this bundle",
        ));
    }
    let society = rows::get_society(store.conn(), &req.target_society_ref)
        .map_err(db_err)?
        .ok_or_else(state::not_found)?;

    let query_value = serde_json::to_value(req).map_err(|e| state::internal(&e.to_string()))?;
    let query_digest = hostint::portable_digest(hostint::QUERY_TAG, &query_value)
        .map_err(|e| state::internal(&e))?;
    let external_hex = req.idempotency_domain_digest.value_hex.clone();

    let lineage = resolve_lineage(
        &cfg,
        &incarnation,
        society.recovery_epoch,
        &req.target_endpoint_incarnation,
        req.target_society_recovery_epoch,
        &req.target_society_ref,
        &req.idempotency_domain_digest,
        req.restore_lineage_proof_ref.as_ref(),
        req.restore_lineage_proof_digest.as_ref(),
    );

    let mut envelope = json!({
        "query_digest": digest_json(&query_digest),
        "current_endpoint_incarnation": incarnation,
        "target_endpoint_incarnation": req.target_endpoint_incarnation,
        "idempotency_domain_digest": digest_json(&req.idempotency_domain_digest),
        "observed_at": rfc3339_utc(now),
    });

    let row = domain_row(store.conn(), &external_hex)?;
    let command_matches = row.as_ref().is_some_and(|r| {
        rows::str_of(r, "canonical_command_digest") == req.canonical_command_digest.value_hex
    });

    match (&row, command_matches) {
        // A retained row for another command over this domain is not a
        // fact about THIS command: unverifiable, therefore unknown.
        (Some(_), false) => {
            envelope["status"] = json!("unknown");
        }
        (Some(row), true) if rows::str_of(row, "state") == STATE_COMMITTED => {
            envelope["status"] = json!("committed");
            envelope["committed_result_envelope"] = rows::json_of(row, "result_envelope");
            envelope["committed_result_digest"] = digest_json(&DigestRef::portable_public(
                rows::str_of(row, "result_digest").to_owned(),
            ));
            envelope["committed_result_signature"] = json!(rows::str_of(row, "result_signature"));
        }
        (Some(row), true) if rows::str_of(row, "state") == STATE_TOMBSTONED => {
            envelope["status"] = json!("non_reexecuting_tombstone");
            envelope["tombstone_ref"] = json!(rows::str_of(row, "tombstone_ref"));
            envelope["tombstone_digest"] = rows::json_of(row, "tombstone_digest");
            envelope["tombstone_reason"] = json!(rows::str_of(row, "tombstone_reason"));
        }
        (Some(_), true) => {
            envelope["status"] = json!("unknown");
        }
        (None, _) => match &lineage {
            Lineage::Live => {
                // A complete query of the LIVE target domain. Anything
                // prepared or in flight is unknown, never absent.
                let internal = internal_domain_digest(store, req, &society.society_id)?;
                if in_flight(store.conn(), &internal.value_hex)? {
                    envelope["status"] = json!("unknown");
                } else {
                    envelope["status"] = json!("absent");
                }
            }
            Lineage::Fenced {
                proof_ref,
                proof_digest,
                hops,
            } => {
                let (receipt_ref, receipt_digest) = fence_receipt(
                    &cfg,
                    &req.target_society_ref,
                    &req.target_endpoint_incarnation,
                    req.target_society_recovery_epoch,
                    &incarnation,
                    society.recovery_epoch,
                    &req.idempotency_domain_digest,
                    hops,
                )?;
                envelope["status"] = json!("historically_fenced_absent");
                envelope["restore_lineage_evidence_ref"] = json!(proof_ref);
                envelope["restore_lineage_evidence_digest"] = digest_json(proof_digest);
                envelope["historical_fence_receipt_ref"] = json!(receipt_ref);
                envelope["historical_fence_receipt_digest"] = digest_json(&receipt_digest);
            }
            Lineage::Unverified(_) => {
                envelope["status"] = json!("unknown");
            }
        },
    }
    ok_value(seal(store, hostint::QUERY_RESULT_TAG, envelope)?)
}

/// The server-recomputed internal IdempotencyDomain of a live external
/// command: the actor is derived from the named source principal, never
/// from a request-supplied identity.
fn internal_domain_digest(
    store: &Store,
    req: &ops::ExternalCommandResultQueryRequest,
    society_id: &str,
) -> Result<DigestRef, Problem> {
    let scope = MutationScope {
        society_id: society_id.to_owned(),
        operation: req.operation.clone(),
        actor: principal_actor(&req.source_principal_ref),
        meta: bpp_core::envelope::MutationMeta {
            request_id: "external-command-result-query".to_owned(),
            idempotency_key: req.byom_command_idempotency_key.clone(),
            expected_endpoint_incarnation: String::new(),
            expected_recovery_epoch: 0,
            expected_revision: None,
            causation_event_ref: None,
            correlation_ref: None,
        },
        body: Value::Null,
    };
    store
        .domain_digest(&scope)
        .map_err(|e| state::internal(&e.to_string()))
}

// ------------------------------- external_command_terminalize (R40) ----

fn not_terminalizable(
    store: &Store,
    req: &ops::ExternalCommandTerminalizeRequest,
    blocking_state: &str,
    detail: &str,
    now: i64,
) -> Result<Value, Problem> {
    let evidence = json!({
        "blocking_state": blocking_state,
        "detail": detail,
        "idempotency_domain_digest": digest_json(&req.idempotency_domain_digest),
        "canonical_command_digest": digest_json(&req.canonical_command_digest),
        "target_endpoint_incarnation": req.target_endpoint_incarnation,
        "target_society_recovery_epoch": req.target_society_recovery_epoch,
    });
    let digest = hostint::portable_digest(hostint::BLOCKING_EVIDENCE_TAG, &evidence)
        .map_err(|e| state::internal(&e))?;
    let envelope = json!({
        "status": "not_terminalizable",
        "target_endpoint_incarnation": req.target_endpoint_incarnation,
        "target_society_ref": req.target_society_ref,
        "target_society_recovery_epoch": req.target_society_recovery_epoch,
        "canonical_command_digest": digest_json(&req.canonical_command_digest),
        "idempotency_domain_digest": digest_json(&req.idempotency_domain_digest),
        "blocking_state": blocking_state,
        "blocking_evidence_digest": digest_json(&digest),
        "observed_at": rfc3339_utc(now),
    });
    seal(store, hostint::TERMINALIZE_RESULT_TAG, envelope)
}

#[allow(clippy::too_many_lines)]
pub fn external_command_terminalize(
    store: &mut Store,
    credential: &DelegatedPrincipalCredential,
    req: &ops::ExternalCommandTerminalizeRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    let cfg = HostConfig::load(store)?;
    let society = rows::get_society(store.conn(), &req.target_society_ref)
        .map_err(db_err)?
        .ok_or_else(state::not_found)?;
    let principal = verify_credential(
        store,
        &cfg,
        credential,
        "external_command_terminalize",
        society.recovery_epoch,
        now,
    )?;
    let cred = &principal.credential;
    let incarnation = store
        .incarnation()
        .map_err(|e| state::internal(&e.to_string()))?;
    if req.target_byom_endpoint_ref != cfg.realm_byom_binding.byom_endpoint_ref {
        return Err(state::stale_binding("the request names another endpoint"));
    }
    if !cfg.recovery_binding.matches(
        &req.current_recovery_binding_ref,
        req.current_recovery_binding_revision,
        req.current_recovery_binding_epoch,
        &req.current_recovery_binding_digest,
    ) {
        return Err(state::stale_binding(
            "terminalization authenticates only through the CURRENT recovery binding",
        ));
    }
    if !hostint::DPC_OPERATIONS.contains(&req.operation.as_str()) {
        return Err(state::invalid(
            "operation is not an external command of this bundle",
        ));
    }
    check_meta_binding(store, &req.meta, &society.society_id)?;

    // "Same source human", checked from the DURABLE source-principal
    // binding: the fresh channel supplies the current actor binding; the
    // request retains the target one. A binding-epoch change may fence
    // execution while still letting the same human deny future execution
    // — but another principal, service, controller, or successor
    // Participant cannot terminalize (§16.3).
    if req.source_principal_ref != cred.source_principal_ref
        || req.current_source_actor_binding_digest != cred.source_actor_binding_digest
    {
        return Err(state::forbidden_detail(
            "terminalization requires the same source human on a fresh channel",
        ));
    }
    let expected_proof = hostint::attempt_proof(
        &req.canonical_command_digest,
        &req.idempotency_domain_digest,
        &cred.nonce,
        &req.current_recovery_binding_digest,
        &cred.source_actor_binding_digest,
    )
    .map_err(|e| state::internal(&e))?;
    if expected_proof != req.authentication_proof {
        return Err(state::forbidden_detail(
            "the fresh proof does not bind this domain, command, nonce, recovery binding and \
             actor binding",
        ));
    }

    // The retained result of THIS exact request comes first: a retry
    // after a crash, or a replayed credential nonce, re-serves the same
    // bytes rather than recomputing a fresh observation time (R41: never
    // re-executes).
    let terminalize_scope = MutationScope {
        society_id: society.society_id.clone(),
        operation: "external_command_terminalize".into(),
        actor: principal.actor.clone(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let terminalize_internal = store
        .domain_digest(&terminalize_scope)
        .map_err(|e| state::internal(&e.to_string()))?;
    let request_digest =
        Store::request_digest(body).map_err(|e| state::internal(&e.to_string()))?;
    let retained = |store: &Store, domain_hex: &str| -> Result<Option<Vec<u8>>, Problem> {
        Ok(store
            .lookup_idempotency(domain_hex)
            .map_err(|e| state::internal(&e.to_string()))?
            .filter(|(stored, _)| *stored == request_digest)
            .map(|(_, bytes)| bytes))
    };
    if let Some(bytes) = retained(store, &terminalize_internal.value_hex)? {
        return Ok(bytes);
    }
    if let Some((stored, _)) = host_ops::consumption(store.conn(), &cred.issuer_ref, &cred.nonce)? {
        if let Some(bytes) = retained(store, &stored)? {
            return Ok(bytes);
        }
        return Err(state::forbidden_detail(
            "credential nonce already consumed by another command",
        ));
    }

    let external_hex = req.idempotency_domain_digest.value_hex.clone();
    let existing = domain_row(store.conn(), &external_hex)?;
    if let Some(row) = &existing {
        if rows::str_of(row, "canonical_command_digest") != req.canonical_command_digest.value_hex {
            // The domain is claimed by other bytes: a closed blocking
            // state, never a mutation.
            return ok_value(not_terminalizable(
                store,
                req,
                "domain_conflict",
                "the IdempotencyDomain is claimed by another canonical command",
                now,
            )?);
        }
        if rows::str_of(row, "source_actor_binding_digest")
            != req.target_source_actor_binding_digest.value_hex
        {
            return Err(state::forbidden_detail(
                "the retained target actor binding is another human's",
            ));
        }
        // §16.3: if a result committed, return the committed envelope
        // (a Byom no-op); if a tombstone exists, re-serve it.
        if rows::str_of(row, "state") == STATE_COMMITTED {
            let envelope = json!({
                "status": "committed",
                "target_endpoint_incarnation": req.target_endpoint_incarnation,
                "target_society_ref": req.target_society_ref,
                "target_society_recovery_epoch": req.target_society_recovery_epoch,
                "canonical_command_digest": digest_json(&req.canonical_command_digest),
                "idempotency_domain_digest": digest_json(&req.idempotency_domain_digest),
                "committed_result_envelope": rows::json_of(row, "result_envelope"),
                "committed_result_digest": digest_json(&DigestRef::portable_public(
                    rows::str_of(row, "result_digest").to_owned())),
                "committed_result_signature": rows::str_of(row, "result_signature"),
                "observed_at": rfc3339_utc(now),
            });
            return ok_value(seal(store, hostint::TERMINALIZE_RESULT_TAG, envelope)?);
        }
        if rows::str_of(row, "state") == STATE_TOMBSTONED {
            let receipt = rows::get_row(
                store.conn(),
                "authority_journal_receipts",
                "external_domain_digest",
                &external_hex,
            )
            .map_err(db_err)?;
            let (receipt_ref, receipt_digest) = match &receipt {
                Some(r) => (
                    rows::str_of(r, "receipt_id").to_owned(),
                    rows::json_of(r, "digest"),
                ),
                None => return Err(state::internal("tombstone without a journal receipt")),
            };
            let envelope = json!({
                "status": "terminalized",
                "target_endpoint_incarnation": req.target_endpoint_incarnation,
                "target_society_ref": req.target_society_ref,
                "target_society_recovery_epoch": req.target_society_recovery_epoch,
                "canonical_command_digest": digest_json(&req.canonical_command_digest),
                "idempotency_domain_digest": digest_json(&req.idempotency_domain_digest),
                "tombstone_ref": rows::str_of(row, "tombstone_ref"),
                "tombstone_digest": rows::json_of(row, "tombstone_digest"),
                "tombstone_reason": rows::str_of(row, "tombstone_reason"),
                "authority_journal_receipt_ref": receipt_ref,
                "authority_journal_receipt_digest": receipt_digest,
                "observed_at": rfc3339_utc(now),
            });
            return ok_value(seal(store, hostint::TERMINALIZE_RESULT_TAG, envelope)?);
        }
    }

    // Nothing terminal yet. Lock the domain and its journal state.
    let lineage = resolve_lineage(
        &cfg,
        &incarnation,
        society.recovery_epoch,
        &req.target_endpoint_incarnation,
        req.target_society_recovery_epoch,
        &req.target_society_ref,
        &req.idempotency_domain_digest,
        req.restore_lineage_proof_ref.as_ref(),
        req.restore_lineage_proof_digest.as_ref(),
    );
    if let Lineage::Unverified(fault) = &lineage {
        return ok_value(not_terminalizable(
            store,
            req,
            fault.blocking_state(),
            fault.detail(),
            now,
        )?);
    }
    let target_internal = {
        let scope = MutationScope {
            society_id: society.society_id.clone(),
            operation: req.operation.clone(),
            actor: principal.actor.clone(),
            meta: bpp_core::envelope::MutationMeta {
                request_id: "external-command-terminalize".to_owned(),
                idempotency_key: req.byom_command_idempotency_key.clone(),
                expected_endpoint_incarnation: String::new(),
                expected_recovery_epoch: 0,
                expected_revision: None,
                causation_event_ref: None,
                correlation_ref: None,
            },
            body: Value::Null,
        };
        store
            .domain_digest(&scope)
            .map_err(|e| state::internal(&e.to_string()))?
    };
    if matches!(lineage, Lineage::Live) && in_flight(store.conn(), &target_internal.value_hex)? {
        return ok_value(not_terminalizable(
            store,
            req,
            "prepared_or_in_flight",
            "an authority transition over this domain is prepared or in flight",
            now,
        )?);
    }

    // Install the restore-safe non-reexecuting tombstone atomically.
    let scope = terminalize_scope;
    let tombstone_ref = mint(store, "tomb")?;
    let receipt_ref = mint(store, "ajr")?;
    let event_id = mint(store, "evt")?;
    let created_at = rfc3339_utc(now);
    let society_id = society.society_id.clone();
    let tombstone = json!({
        "tombstone_ref": tombstone_ref,
        "society_ref": society_id,
        "society_recovery_epoch": req.target_society_recovery_epoch,
        "operation": req.operation,
        "idempotency_domain_digest": digest_json(&req.idempotency_domain_digest),
        "canonical_command_digest": digest_json(&req.canonical_command_digest),
        "reason_kind": "terminalized",
        "reason": req.reason,
        "created_at": created_at,
    });
    let tombstone_digest = hostint::portable_digest(hostint::TOMBSTONE_TAG, &tombstone)
        .map_err(|e| state::internal(&e))?;

    let cred = cred.clone();
    let req_owned = req.clone();
    let actor = principal.actor.clone();
    let target_hex = target_internal.value_hex.clone();
    let external = req.idempotency_domain_digest.clone();
    let terminalize_hex = terminalize_internal.value_hex.clone();
    let live = matches!(lineage, Lineage::Live);

    let bytes = run(store, scope, now, hooks, move |conn, _| {
        // Re-lock under the open transaction: a delayed command that
        // committed first wins and this operation mutates nothing.
        if let Some(row) = domain_row(conn, &external.value_hex)? {
            let _ = row;
            return Err(conflict(
                "the domain became terminal while terminalization prepared",
            ));
        }
        if live && in_flight(conn, &target_hex)? {
            return Err(Problem::new(
                ProblemKind::ExternalCommandNotTerminalizable,
                "an authority transition over this domain is prepared or in flight",
            )
            .with_status(409));
        }
        if host_ops::consumption(conn, &cred.issuer_ref, &cred.nonce)?.is_some() {
            return Err(state::forbidden_detail("credential nonce already consumed"));
        }
        let prior: i64 = byom_store::schema::meta_get_text(conn, "journal_mirror_gen")
            .map_err(|e| state::internal(&e.to_string()))?
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let receipt = json!({
            "receipt_id": receipt_ref,
            "society_ref": society_id,
            "operation": "external_command_terminalize",
            "idempotency_domain_digest": digest_json(&external),
            "prior_journal_generation": prior,
            "proposed_journal_generation": prior + 1,
            "subject_ref": tombstone_ref,
            "created_at": created_at,
        });
        let receipt_digest = hostint::portable_digest(hostint::JOURNAL_RECEIPT_TAG, &receipt)
            .map_err(|e| state::internal(&e))?;
        let effects = vec![
            Effect::Upsert {
                table: "delegated_credential_consumptions".into(),
                row: obj_pairs([
                    ("issuer_ref", json!(cred.issuer_ref)),
                    ("nonce", json!(cred.nonce)),
                    ("credential_id", json!(cred.credential_id)),
                    ("society_id", json!(society_id)),
                    ("operation", json!("external_command_terminalize")),
                    ("source_principal_ref", json!(cred.source_principal_ref)),
                    ("external_domain_digest", json!(external.value_hex)),
                    (
                        "canonical_command_digest",
                        json!(req_owned.canonical_command_digest.value_hex),
                    ),
                    ("internal_domain_digest", json!(terminalize_hex)),
                    ("outcome", json!(STATE_TOMBSTONED)),
                    ("consumed_at", json!(created_at)),
                ]),
            },
            Effect::Upsert {
                table: "external_command_domains".into(),
                row: domain_effect_row(
                    &external.value_hex,
                    &society_id,
                    &req_owned.operation,
                    &req_owned.target_endpoint_incarnation,
                    req_owned.target_society_recovery_epoch,
                    &req_owned.byom_command_idempotency_key,
                    &req_owned.canonical_command_digest.value_hex,
                    &req_owned.kovee_formation_intent_ref,
                    &req_owned.source_principal_ref,
                    &req_owned.target_source_actor_binding_digest.value_hex,
                    &target_hex,
                    STATE_TOMBSTONED,
                    None,
                    Some((
                        &tombstone_ref,
                        &tombstone_digest,
                        &req_owned.reason,
                        "terminalized",
                    )),
                    &created_at,
                ),
            },
            Effect::Upsert {
                table: "authority_journal_receipts".into(),
                row: obj_pairs([
                    ("receipt_id", json!(receipt_ref)),
                    ("society_id", json!(society_id)),
                    ("operation", json!("external_command_terminalize")),
                    ("external_domain_digest", json!(external.value_hex)),
                    ("prior_generation", json!(prior)),
                    ("proposed_generation", json!(prior + 1)),
                    ("subject_ref", json!(tombstone_ref)),
                    ("digest", digest_json(&receipt_digest)),
                    ("created_at", json!(created_at)),
                ]),
            },
        ];
        let envelope = json!({
            "status": "terminalized",
            "target_endpoint_incarnation": req_owned.target_endpoint_incarnation,
            "target_society_ref": req_owned.target_society_ref,
            "target_society_recovery_epoch": req_owned.target_society_recovery_epoch,
            "canonical_command_digest": digest_json(&req_owned.canonical_command_digest),
            "idempotency_domain_digest": digest_json(&external),
            "tombstone_ref": tombstone_ref,
            "tombstone_digest": digest_json(&tombstone_digest),
            "tombstone_reason": req_owned.reason,
            "authority_journal_receipt_ref": receipt_ref,
            "authority_journal_receipt_digest": digest_json(&receipt_digest),
            "observed_at": created_at,
        });
        let mut sealed = envelope;
        let signature = conn_sign(conn, &sealed)?;
        sealed["server_signature"] = json!(signature);
        let digest = hostint::self_digest(hostint::TERMINALIZE_RESULT_TAG, &sealed)
            .map_err(|e| state::internal(&e))?;
        sealed["digest"] = digest_json(&digest);
        Ok(Prepared {
            result: sealed,
            revision: None,
            cursor: CursorMint::AfterEvents {
                society_id: society_id.clone(),
            },
            effects,
            events: vec![event(
                &society_id,
                &event_id,
                "kovee.external_command_terminalized",
                &tombstone_ref,
                1,
                &cred.bound_participant_ref,
                &actor,
                &req_owned.meta,
                json!({"kovee_formation_intent_ref": req_owned.kovee_formation_intent_ref,
                       "reason": req_owned.reason}),
            )],
        })
    })?;
    Ok(bytes)
}

/// Wraps a read/no-op envelope in the BPP success shape.
fn ok_value(result: Value) -> Result<Vec<u8>, Problem> {
    serde_json::to_vec(&bpp_core::envelope::Success::new(result))
        .map_err(|e| state::internal(&e.to_string()))
}
