//! B3 slice 2 — the effect half: the two INDEPENDENT axes of §13.2.
//!
//! Source fact and local consequence are separate records with separate
//! heads, and the code keeps them separate:
//!
//! ```text
//! effect_outcome_admit   runtime   SOURCE facts only, no decision field
//!                                  -> EffectOutcomeAdmission + EOA head CAS
//!                                  -> marks any active disposition head
//!                                     source_advanced
//! effect_reconcile       governance exact GovernanceDecision + fresh challenge
//!                                  -> EffectGovernanceDisposition + its own
//!                                     head; NEVER advances the EOA head and
//!                                     never claims the host Effect became
//!                                     factually succeeded or failed
//! ```
//!
//! **Lock ordering.** Both operations lock the EOA head BEFORE the
//! disposition head. `effect_outcome_admit` reads and fences whichever
//! disposition is then current and accepts no caller-supplied expected
//! disposition revision, so a concurrent `effect_reconcile` either
//! commits first and is fenced, or observes the final source and must use
//! the late-source branch. Both heads then enter the downstream
//! dependency closure (`episode_ops::effect_head_closure`), which every
//! materializer and local-consequence consumer checks.

use bpp_core::ops;
use bpp_core::problem::{Problem, ProblemKind};
use bpp_core::time::rfc3339_utc;
use byom_store::effects::Effect;
use byom_store::rows;
use byom_store::{CrashHooks, CursorMint, MutationScope, Prepared, Store};
use rusqlite::Connection;
use serde_json::{json, Map, Value};

use crate::episode_ops::{
    effect_head_closure, head_row, protected, verify_runtime_token, RuntimeChannel,
};
use crate::gov_ops::{check_meta_binding, db_err, digest_json, mint, obj_pairs, run};
use crate::part_ops::event;
use crate::state;

/// The narrow trusted effect-admission adapter's actor string (§14.7
/// `effect_outcome_admit` row).
pub const ACTOR_EFFECT_ADAPTER: &str = "kovee-adapter:effect-outcome";

/// The three §13.2 disposition-head states.
pub const HEAD_ACTIVE_AMBIGUOUS: &str = "active_ambiguous";
pub const HEAD_SOURCE_ADVANCED: &str = "source_advanced";
pub const HEAD_RESOLVED_LATE: &str = "resolved_late";

fn head_conflict(detail: &str) -> Problem {
    Problem::new(
        ProblemKind::StaleRevision,
        "the effect head is not in the state this transition requires",
    )
    .with_status(409)
    .with_detail(detail.to_owned())
}

fn opt_json(v: &Option<String>) -> Value {
    v.as_ref().map(|s| json!(s)).unwrap_or(Value::Null)
}

// ============================================== effect_outcome_admit =====

/// `effect_outcome_admit` (runtime, create; R35). The trusted Kovee
/// effect service records its authoritative outcome even if the
/// requesting Episode loses its lease; byom VERIFIES that source
/// revision and records only the idempotent EffectOutcomeAdmission — it
/// never rewrites the host Effect or receipt, and this path has no
/// GovernanceDecision field at all (§13.1 step 8, §13.2 path 1).
pub fn effect_outcome_admit(
    store: &mut Store,
    token: &str,
    req: &ops::EffectOutcomeAdmitRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    verify_runtime_token(
        store,
        token,
        RuntimeChannel::Worker,
        &format!("{}|{}", req.episode_ref, req.generation),
    )?;
    let episode = rows::get_row(store.conn(), "episodes", "episode_id", &req.episode_ref)
        .map_err(db_err)?
        .ok_or_else(state::not_found)?;
    let society_id = rows::str_of(&episode, "society_id").to_owned();
    check_meta_binding(store, &req.meta, &society_id)?;
    let admission_id = mint(store, "eoa")?;
    let admit_event = mint(store, "evt")?;
    let fence_event = mint(store, "evt")?;
    let admitted_at = rfc3339_utc(now);
    let scope = MutationScope {
        society_id: society_id.clone(),
        operation: "effect_outcome_admit".into(),
        actor: ACTOR_EFFECT_ADAPTER.into(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let req_c = req.clone();
    let society_c = society_id.clone();
    run(store, scope, now, hooks, move |conn, _| {
        // The DUAL fences gate the admission exactly as they gate every
        // other runtime mutation (family contract L21/R30).
        let p = protected(
            conn,
            &req_c.episode_ref,
            req_c.generation,
            &req_c.byom_attempt_ref,
            req_c.byom_fence_epoch,
            req_c.kovee_invocation_fence,
        )?;

        // -- LOCK 1: the EOA head, always before the disposition head --
        let eoa_head = head_row(
            conn,
            "effect_outcome_admission_heads",
            "intent_ref",
            &req_c.intent_ref,
            "stable_execution_key",
            &req_c.stable_execution_key,
        )?;
        // A different host receipt cannot reuse the same source
        // uniqueness key (§13.2
        // UNIQUE(host_endpoint_ref, host_effect_ref, host_receipt_digest)).
        if let Some(clash) = source_key_clash(conn, &req_c)? {
            if rows::str_of(&clash, "intent_ref") != req_c.intent_ref
                || rows::str_of(&clash, "stable_execution_key") != req_c.stable_execution_key
            {
                return Err(state::stale_binding(
                    "this host receipt is already admitted under another intent / execution key",
                ));
            }
            // Exact replay returns the same record.
            return Ok(Prepared {
                result: eoa_result(conn, &clash, &req_c)?,
                revision: Some(rows::u64_of(&clash, "revision")),
                cursor: CursorMint::AfterEvents {
                    society_id: society_c.clone(),
                },
                effects: Vec::new(),
                events: Vec::new(),
            });
        }

        let (revision, previous_digest) = match &eoa_head {
            None => {
                if req_c.reconciles_admission_ref.is_some() {
                    return Err(head_conflict("revision 1 has no predecessor to reconcile"));
                }
                (1u64, Value::Null)
            }
            Some(head) => {
                let current_outcome = rows::str_of(head, "current_outcome").to_owned();
                // No source-final outcome returns to ambiguous, and
                // every ambiguous-to-final path requires
                // `reconciles_admission_*` (§13.2).
                if current_outcome != "ambiguous" {
                    return Err(head_conflict(&format!(
                        "the source EOA head is already final ({current_outcome}); a final \
                         source outcome never returns to ambiguous"
                    )));
                }
                if req_c.outcome == "ambiguous" {
                    return Err(head_conflict(
                        "an ambiguous successor cannot supersede an ambiguous head",
                    ));
                }
                let cited = req_c
                    .reconciles_admission_ref
                    .as_deref()
                    .unwrap_or_default();
                if cited != rows::str_of(head, "current_admission_ref") {
                    return Err(head_conflict(
                        "the ambiguous-to-final path must cite the EXACT current ambiguous \
                         admission (reconciles_admission_*)",
                    ));
                }
                let cited_digest = req_c
                    .reconciles_admission_digest
                    .as_ref()
                    .ok_or_else(|| head_conflict("reconciles_admission_digest is required"))?;
                if !cited_digest.same_ref_json(&rows::json_of(head, "current_admission_digest")) {
                    return Err(head_conflict(
                        "reconciles_admission_digest does not pin the current admission",
                    ));
                }
                (
                    rows::u64_of(head, "current_admission_revision") + 1,
                    rows::json_of(head, "current_admission_digest"),
                )
            }
        };

        let record = json!({
            "admission_id": admission_id,
            "revision": revision,
            "intent_ref": req_c.intent_ref,
            "stable_execution_key": req_c.stable_execution_key,
            "host_protocol": req_c.host_protocol,
            "host_endpoint_ref": req_c.host_endpoint_ref,
            "host_effect_ref": req_c.host_effect_ref,
            "host_effect_digest": digest_json(&req_c.host_effect_digest),
            "host_receipt_ref": req_c.host_receipt_ref,
            "host_receipt_digest": digest_json(&req_c.host_receipt_digest),
            "verification_status": req_c.verification_status,
            "outcome": req_c.outcome,
            "admitted_at": admitted_at,
        });
        let digest = crate::part_common::conn_record_digest(
            conn,
            &society_c,
            &admission_id,
            "bpp-effect-outcome-admission-v0",
            &record,
        )?;
        let row = obj_pairs([
            ("admission_id", json!(admission_id)),
            ("society_id", json!(society_c)),
            ("revision", json!(revision)),
            ("previous_admission_digest", previous_digest),
            ("intent_ref", json!(req_c.intent_ref)),
            ("intent_digest", digest_json(&req_c.intent_digest)),
            ("stable_execution_key", json!(req_c.stable_execution_key)),
            ("episode_ref", json!(req_c.episode_ref)),
            ("host_protocol", json!(req_c.host_protocol)),
            ("host_endpoint_ref", json!(req_c.host_endpoint_ref)),
            ("host_effect_ref", json!(req_c.host_effect_ref)),
            ("host_effect_digest", digest_json(&req_c.host_effect_digest)),
            ("host_receipt_ref", json!(req_c.host_receipt_ref)),
            (
                "host_receipt_digest",
                digest_json(&req_c.host_receipt_digest),
            ),
            (
                "host_cursor_or_signature_ref",
                json!(req_c.host_cursor_or_signature_ref),
            ),
            ("verification_status", json!(req_c.verification_status)),
            ("outcome", json!(req_c.outcome)),
            ("result_ref", opt_json(&req_c.result_ref)),
            (
                "result_digest",
                req_c
                    .result_digest
                    .as_ref()
                    .map(digest_json)
                    .unwrap_or(Value::Null),
            ),
            (
                "usage_settlement_ref",
                opt_json(&req_c.usage_settlement_ref),
            ),
            (
                "reconciles_admission_ref",
                opt_json(&req_c.reconciles_admission_ref),
            ),
            (
                "reconciles_admission_digest",
                req_c
                    .reconciles_admission_digest
                    .as_ref()
                    .map(digest_json)
                    .unwrap_or(Value::Null),
            ),
            ("admitted_by_service", json!(ACTOR_EFFECT_ADAPTER)),
            ("admitted_at", json!(admitted_at)),
            ("digest", digest_json(&digest)),
        ]);
        let mut effects = vec![
            Effect::Upsert {
                table: "effect_outcome_admissions".into(),
                row: row.clone(),
            },
            Effect::Upsert {
                table: "effect_outcome_admission_heads".into(),
                row: obj_pairs([
                    ("intent_ref", json!(req_c.intent_ref)),
                    ("stable_execution_key", json!(req_c.stable_execution_key)),
                    ("society_id", json!(society_c)),
                    ("current_admission_ref", json!(admission_id)),
                    ("current_admission_revision", json!(revision)),
                    ("current_admission_digest", digest_json(&digest)),
                    ("current_outcome", json!(req_c.outcome)),
                    (
                        "revision",
                        json!(
                            eoa_head
                                .as_ref()
                                .map(|h| rows::u64_of(h, "revision"))
                                .unwrap_or(0)
                                + 1
                        ),
                    ),
                    ("updated_at", json!(admitted_at)),
                ]),
            },
        ];
        let mut events = vec![event(
            &society_c,
            &admit_event,
            "effect-outcome-admission.admitted",
            &admission_id,
            revision,
            rows::str_of(&p.episode, "participant_ref"),
            ACTOR_EFFECT_ADAPTER,
            &req_c.meta,
            json!({"outcome": req_c.outcome, "revision": revision,
                   "source_only": true,
                   "governance_decision": Value::Null,
                   "host_receipt_ref": req_c.host_receipt_ref}),
        )];

        // -- LOCK 2: the disposition head, read and FENCED without any
        // caller-supplied expected revision (§13.2). A source advance
        // marks an active ambiguous disposition `source_advanced`: the
        // verified result still gets its ClassificationAdmission, but
        // its materialization and use stay QUARANTINED while that head
        // is `source_advanced`.
        if req_c.outcome != "ambiguous" {
            if let Some(disp_head) = head_row(
                conn,
                "effect_governance_disposition_heads",
                "intent_ref",
                &req_c.intent_ref,
                "stable_execution_key",
                &req_c.stable_execution_key,
            )? {
                if rows::str_of(&disp_head, "state") == HEAD_ACTIVE_AMBIGUOUS {
                    let head_revision = rows::u64_of(&disp_head, "revision") + 1;
                    let mut advanced = disp_head.clone();
                    advanced.insert("state".into(), json!(HEAD_SOURCE_ADVANCED));
                    advanced.insert("revision".into(), json!(head_revision));
                    advanced.insert("updated_at".into(), json!(admitted_at));
                    effects.push(Effect::Upsert {
                        table: "effect_governance_disposition_heads".into(),
                        row: advanced,
                    });
                    events.push(event(
                        &society_c,
                        &fence_event,
                        "effect-governance-disposition.source-advanced",
                        rows::str_of(&disp_head, "current_disposition_ref"),
                        head_revision,
                        rows::str_of(&p.episode, "participant_ref"),
                        ACTOR_EFFECT_ADAPTER,
                        &req_c.meta,
                        json!({"state": HEAD_SOURCE_ADVANCED,
                               "result_use": "quarantined while source_advanced",
                               "requires": "a fresh GovernanceDecision and a late_source \
                                            disposition to release"}),
                    ));
                }
            }
        }

        let closure = closure_after(conn, &effects, &req_c.episode_ref)?;
        Ok(Prepared {
            result: json!({
                "admission_id": admission_id,
                "revision": revision,
                "intent_ref": req_c.intent_ref,
                "stable_execution_key": req_c.stable_execution_key,
                "outcome": req_c.outcome,
                "verification_status": req_c.verification_status,
                "digest": digest_json(&digest),
                "lock_order": ["effect_outcome_admission_head",
                               "effect_governance_disposition_head"],
                "dependency_closure": closure,
            }),
            revision: Some(revision),
            cursor: CursorMint::AfterEvents {
                society_id: society_c.clone(),
            },
            effects,
            events,
        })
    })
}

/// The staged closure: the committed heads plus the ones this transition
/// is about to write, so the reply already names both heads a downstream
/// consumer must check.
fn closure_after(
    conn: &Connection,
    effects: &[Effect],
    episode_ref: &str,
) -> Result<Value, Problem> {
    let mut closure = effect_head_closure(conn, episode_ref)?;
    let mut eoa: Vec<Value> = closure
        .get("effect_outcome_admission_heads")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut disp: Vec<Value> = closure
        .get("effect_governance_disposition_heads")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for effect in effects {
        let Effect::Upsert { table, row } = effect;
        match table.as_str() {
            "effect_outcome_admission_heads" => {
                let entry = json!({
                    "intent_ref": rows::str_of(row, "intent_ref"),
                    "stable_execution_key": rows::str_of(row, "stable_execution_key"),
                    "current_admission_ref": rows::str_of(row, "current_admission_ref"),
                    "current_admission_revision":
                        rows::u64_of(row, "current_admission_revision"),
                    "current_admission_digest": rows::json_of(row, "current_admission_digest"),
                    "current_outcome": rows::str_of(row, "current_outcome"),
                    "revision": rows::u64_of(row, "revision"),
                });
                eoa.retain(|e| e["intent_ref"] != entry["intent_ref"]);
                eoa.push(entry);
            }
            "effect_governance_disposition_heads" => {
                let entry = json!({
                    "intent_ref": rows::str_of(row, "intent_ref"),
                    "stable_execution_key": rows::str_of(row, "stable_execution_key"),
                    "current_disposition_ref": rows::str_of(row, "current_disposition_ref"),
                    "current_disposition_revision":
                        rows::u64_of(row, "current_disposition_revision"),
                    "current_disposition_digest":
                        rows::json_of(row, "current_disposition_digest"),
                    "state": rows::str_of(row, "state"),
                    "revision": rows::u64_of(row, "revision"),
                });
                disp.retain(|e| e["intent_ref"] != entry["intent_ref"]);
                disp.push(entry);
            }
            _ => {}
        }
    }
    if let Some(map) = closure.as_object_mut() {
        map.insert("effect_outcome_admission_heads".into(), json!(eoa));
        map.insert("effect_governance_disposition_heads".into(), json!(disp));
    }
    Ok(closure)
}

fn source_key_clash(
    conn: &Connection,
    req: &ops::EffectOutcomeAdmitRequest,
) -> Result<Option<Map<String, Value>>, Problem> {
    for row in rows::rows_where(
        conn,
        "effect_outcome_admissions",
        "host_effect_ref",
        &req.host_effect_ref,
        "admission_id",
    )
    .map_err(db_err)?
    {
        if rows::str_of(&row, "host_endpoint_ref") == req.host_endpoint_ref
            && req
                .host_receipt_digest
                .same_ref_json(&rows::json_of(&row, "host_receipt_digest"))
        {
            return Ok(Some(row));
        }
    }
    Ok(None)
}

fn eoa_result(
    conn: &Connection,
    row: &Map<String, Value>,
    req: &ops::EffectOutcomeAdmitRequest,
) -> Result<Value, Problem> {
    Ok(json!({
        "admission_id": rows::str_of(row, "admission_id"),
        "revision": rows::u64_of(row, "revision"),
        "intent_ref": rows::str_of(row, "intent_ref"),
        "stable_execution_key": rows::str_of(row, "stable_execution_key"),
        "outcome": rows::str_of(row, "outcome"),
        "verification_status": rows::str_of(row, "verification_status"),
        "digest": rows::json_of(row, "digest"),
        "replayed": true,
        "lock_order": ["effect_outcome_admission_head",
                       "effect_governance_disposition_head"],
        "dependency_closure": effect_head_closure(conn, &req.episode_ref)?,
    }))
}

// ================================================== effect_reconcile =====

/// `effect_reconcile` (governance, create; R38). It runs ONLY after an
/// exact GovernanceDecision with a fresh challenge, appends an
/// independent EffectGovernanceDisposition against the exact source
/// admission, and moves ONLY its own head. It never advances the EOA
/// head, never releases an ambiguity-reserved budget, never creates a
/// result, and never claims Kovee's Effect became factually succeeded or
/// failed (§13.2).
pub fn effect_reconcile(
    store: &mut Store,
    req: &ops::EffectReconcileRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    let head = head_row(
        store.conn(),
        "effect_outcome_admission_heads",
        "intent_ref",
        &req.intent_ref,
        "stable_execution_key",
        &req.stable_execution_key,
    )?
    .ok_or_else(state::not_found)?;
    let society_id = rows::str_of(&head, "society_id").to_owned();
    check_meta_binding(store, &req.meta, &society_id)?;
    let disposition_id = mint(store, "egd")?;
    let disposition_event = mint(store, "evt")?;
    let created_at = rfc3339_utc(now);
    // The decision is derived from the subject it decides plus the fresh
    // challenge, so a second disposition needs a NEW challenge (§13.2:
    // "a subsequent effect_reconcile requires a fresh exact
    // GovernanceDecision"; UNIQUE(governance_decision_ref)).
    let decision_id = format!(
        "dec-effect-{}-{}",
        req.stable_execution_key, req.fresh_challenge_ref
    );
    let scope = MutationScope {
        society_id: society_id.clone(),
        operation: "effect_reconcile".into(),
        actor: crate::gov_ops::ACTOR_GOVERNANCE.into(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let req_c = req.clone();
    let society_c = society_id.clone();
    let decision_c = decision_id.clone();
    run(store, scope, now, hooks, move |conn, _| {
        // -- LOCK 1: the EOA head, before the disposition head ---------
        let eoa_head = head_row(
            conn,
            "effect_outcome_admission_heads",
            "intent_ref",
            &req_c.intent_ref,
            "stable_execution_key",
            &req_c.stable_execution_key,
        )?
        .ok_or_else(state::not_found)?;
        let source_outcome = rows::str_of(&eoa_head, "current_outcome").to_owned();
        // The disposition binds ONE exact source-admission revision; a
        // stale source conflicts.
        if rows::str_of(&eoa_head, "current_admission_ref") != req_c.basis_source_admission_ref
            || rows::u64_of(&eoa_head, "current_admission_revision")
                != req_c.basis_source_admission_revision
        {
            return Err(head_conflict(
                "the cited basis source admission is not the current EOA head revision",
            ));
        }
        let basis = rows::get_row(
            conn,
            "effect_outcome_admissions",
            "admission_id",
            &req_c.basis_source_admission_ref,
        )
        .map_err(db_err)?
        .ok_or_else(state::not_found)?;
        if !req_c
            .basis_source_admission_digest
            .same_ref_json(&rows::json_of(&eoa_head, "current_admission_digest"))
        {
            return Err(head_conflict(
                "basis_source_admission_digest does not pin the current source admission",
            ));
        }

        // -- LOCK 2: the disposition head ------------------------------
        let disp_head = head_row(
            conn,
            "effect_governance_disposition_heads",
            "intent_ref",
            &req_c.intent_ref,
            "stable_execution_key",
            &req_c.stable_execution_key,
        )?;
        let (revision, previous, head_state) = match req_c.phase.as_str() {
            // The ambiguous-source branch requires the CURRENT source
            // outcome to be ambiguous: a concurrent effect_outcome_admit
            // that committed first fences this branch and forces the
            // late-source one.
            "ambiguous_source" => {
                if source_outcome != "ambiguous" {
                    return Err(head_conflict(&format!(
                        "the current source EOA outcome is {source_outcome}: an ambiguous_source \
                         disposition is fenced — use the late_source branch (§13.2)"
                    )));
                }
                match &disp_head {
                    None => (1u64, None, HEAD_ACTIVE_AMBIGUOUS),
                    Some(h) if rows::str_of(h, "state") == HEAD_ACTIVE_AMBIGUOUS => (
                        rows::u64_of(h, "current_disposition_revision") + 1,
                        Some(h.clone()),
                        HEAD_ACTIVE_AMBIGUOUS,
                    ),
                    Some(h) => {
                        return Err(head_conflict(&format!(
                            "the disposition head is {}; an ambiguous_source disposition needs \
                             none or active_ambiguous",
                            rows::str_of(h, "state")
                        )))
                    }
                }
            }
            // The late-source branch requires the final source outcome
            // AND the exact `source_advanced` predecessor head.
            "late_source" => {
                if source_outcome == "ambiguous" {
                    return Err(head_conflict(
                        "a late_source disposition requires the source EOA outcome succeeded or \
                         failed",
                    ));
                }
                let Some(h) = &disp_head else {
                    return Err(head_conflict(
                        "a late_source disposition requires the exact source_advanced \
                         predecessor head",
                    ));
                };
                if rows::str_of(h, "state") != HEAD_SOURCE_ADVANCED {
                    return Err(head_conflict(&format!(
                        "the disposition head is {}; a late_source disposition requires \
                         source_advanced",
                        rows::str_of(h, "state")
                    )));
                }
                (
                    rows::u64_of(h, "current_disposition_revision") + 1,
                    Some(h.clone()),
                    HEAD_RESOLVED_LATE,
                )
            }
            _ => return Err(state::invalid("unknown disposition phase")),
        };
        // A released late-source result binds its classification
        // admission in the DECISION subject, and only an existing
        // verified result may be released at all.
        if req_c.result_use == "released" && rows::str_of(&basis, "result_ref").is_empty() {
            return Err(head_conflict(
                "result_use: released requires an existing verified result on the final source \
                 admission",
            ));
        }
        // UNIQUE(governance_decision_ref): a second disposition needs a
        // FRESH challenge, so the derived decision id must be new.
        if rows::get_row(conn, "governance_decisions", "decision_id", &decision_c)
            .map_err(db_err)?
            .is_some()
        {
            return Err(crate::gov_decision::decision_incomplete(
                "this GovernanceDecision already decided a disposition: a subsequent \
                 effect_reconcile requires a FRESH exact decision (§13.2)",
            ));
        }

        let subject_body = json!({
            "intent_ref": req_c.intent_ref,
            "stable_execution_key": req_c.stable_execution_key,
            "phase": req_c.phase,
            "basis_source_admission_ref": req_c.basis_source_admission_ref,
            "basis_source_admission_revision": req_c.basis_source_admission_revision,
            "basis_source_outcome": source_outcome,
            "local_outcome": req_c.local_outcome,
            "result_use": req_c.result_use,
            "classification_admission_ref": req_c.classification_admission_ref,
            "fresh_challenge_ref": req_c.fresh_challenge_ref,
        });
        let subject_digest = crate::part_common::conn_record_digest(
            conn,
            &society_c,
            &decision_c,
            "bpp-effect-reconcile-subject-v0",
            &subject_body,
        )?;
        let sovereign = rows::sovereign_participant(conn, &society_c)
            .map_err(db_err)?
            .ok_or_else(|| state::internal("no sovereign participant"))?;
        let mut effects = vec![crate::gov_decision::form(
            conn,
            &decision_c,
            &society_c,
            "effect_reconciliation",
            "act_intent",
            &req_c.intent_ref,
            &subject_digest,
            "charter:reconciliation",
            &[crate::gov_decision::DecisionSeat {
                seat_ref: format!("seat-sovereign-{society_c}"),
                participant_ref: sovereign.participant_id.clone(),
                actor_ref: crate::gov_ops::ACTOR_GOVERNANCE.to_owned(),
                participant_binding_epoch: sovereign.binding_epoch,
            }],
            &[],
            "sovereign_seat_assent",
            crate::gov_ops::ACTOR_GOVERNANCE,
            now,
        )?];

        let record = json!({
            "disposition_id": disposition_id,
            "revision": revision,
            "intent_ref": req_c.intent_ref,
            "stable_execution_key": req_c.stable_execution_key,
            "phase": req_c.phase,
            "basis_source_admission_ref": req_c.basis_source_admission_ref,
            "basis_source_admission_revision": req_c.basis_source_admission_revision,
            "basis_source_outcome": source_outcome,
            "governance_decision_ref": decision_c,
            "local_outcome": req_c.local_outcome,
            "result_use": req_c.result_use,
            "late_source_policy": req_c.late_source_policy,
            "created_at": created_at,
        });
        let digest = crate::part_common::conn_record_digest(
            conn,
            &society_c,
            &disposition_id,
            "bpp-effect-governance-disposition-v0",
            &record,
        )?;
        effects.push(Effect::Upsert {
            table: "effect_governance_dispositions".into(),
            row: obj_pairs([
                ("disposition_id", json!(disposition_id)),
                ("society_id", json!(society_c)),
                ("revision", json!(revision)),
                (
                    "previous_disposition_ref",
                    previous
                        .as_ref()
                        .map(|h| json!(rows::str_of(h, "current_disposition_ref")))
                        .unwrap_or(Value::Null),
                ),
                (
                    "previous_disposition_revision",
                    previous
                        .as_ref()
                        .map(|h| json!(rows::u64_of(h, "current_disposition_revision")))
                        .unwrap_or(Value::Null),
                ),
                (
                    "previous_disposition_digest",
                    previous
                        .as_ref()
                        .map(|h| rows::json_of(h, "current_disposition_digest"))
                        .unwrap_or(Value::Null),
                ),
                ("intent_ref", json!(req_c.intent_ref)),
                ("intent_digest", digest_json(&req_c.intent_digest)),
                ("stable_execution_key", json!(req_c.stable_execution_key)),
                ("phase", json!(req_c.phase)),
                (
                    "basis_source_admission_ref",
                    json!(req_c.basis_source_admission_ref),
                ),
                (
                    "basis_source_admission_revision",
                    json!(req_c.basis_source_admission_revision),
                ),
                (
                    "basis_source_admission_digest",
                    digest_json(&req_c.basis_source_admission_digest),
                ),
                ("basis_source_outcome", json!(source_outcome)),
                ("governance_decision_ref", json!(decision_c)),
                ("governance_decision_digest", digest_json(&subject_digest)),
                ("local_outcome", json!(req_c.local_outcome)),
                ("result_use", json!(req_c.result_use)),
                (
                    "classification_admission_ref",
                    opt_json(&req_c.classification_admission_ref),
                ),
                (
                    "classification_admission_digest",
                    req_c
                        .classification_admission_digest
                        .as_ref()
                        .map(digest_json)
                        .unwrap_or(Value::Null),
                ),
                ("late_source_policy", opt_json(&req_c.late_source_policy)),
                ("created_at", json!(created_at)),
                ("digest", digest_json(&digest)),
            ]),
        });
        effects.push(Effect::Upsert {
            table: "effect_governance_disposition_heads".into(),
            row: obj_pairs([
                ("intent_ref", json!(req_c.intent_ref)),
                ("stable_execution_key", json!(req_c.stable_execution_key)),
                ("society_id", json!(society_c)),
                ("current_disposition_ref", json!(disposition_id)),
                ("current_disposition_revision", json!(revision)),
                ("current_disposition_digest", digest_json(&digest)),
                ("state", json!(head_state)),
                (
                    "revision",
                    json!(
                        disp_head
                            .as_ref()
                            .map(|h| rows::u64_of(h, "revision"))
                            .unwrap_or(0)
                            + 1
                    ),
                ),
                ("updated_at", json!(created_at)),
            ]),
        });
        let episode_ref = rows::str_of(&basis, "episode_ref").to_owned();
        let closure = closure_after(conn, &effects, &episode_ref)?;
        Ok(Prepared {
            result: json!({
                "disposition_id": disposition_id,
                "revision": revision,
                "phase": req_c.phase,
                "basis_source_outcome": source_outcome,
                "governance_decision_ref": decision_c,
                "local_outcome": req_c.local_outcome,
                "result_use": req_c.result_use,
                "disposition_head_state": head_state,
                "source_head_unchanged": {
                    "current_admission_ref": rows::str_of(&eoa_head, "current_admission_ref"),
                    "current_admission_revision":
                        rows::u64_of(&eoa_head, "current_admission_revision"),
                    "current_outcome": source_outcome,
                },
                "digest": digest_json(&digest),
                "lock_order": ["effect_outcome_admission_head",
                               "effect_governance_disposition_head"],
                "dependency_closure": closure,
            }),
            revision: Some(revision),
            cursor: CursorMint::AfterEvents {
                society_id: society_c.clone(),
            },
            effects,
            events: vec![event(
                &society_c,
                &disposition_event,
                "effect-governance-disposition.recorded",
                &disposition_id,
                revision,
                &sovereign.participant_id,
                crate::gov_ops::ACTOR_GOVERNANCE,
                &req_c.meta,
                json!({"phase": req_c.phase, "local_outcome": req_c.local_outcome,
                       "result_use": req_c.result_use,
                       "disposition_head_state": head_state,
                       "source_axis": "unchanged (this operation never advances the EOA head)",
                       "governance_decision_ref": decision_c}),
            )],
        })
    })
}
