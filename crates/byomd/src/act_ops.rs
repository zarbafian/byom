//! B3 slice 3 — the §13.1 act/effect chain and the Δ4 act-class subject
//! taxonomy.
//!
//! ```text
//! act_intent_prepare    participant  server-prepared ActIntent + the Δ4
//!                                    class subject compiled by the kernel
//! act_intent_position   participant  eligible seats fill their own seat
//!                       governance   (human-authority gate seat)
//! act_intent_finalize   participant  deterministic finalization; ONE
//!                       governance   GovernanceDecision bound to the digest
//! execution_permit_consume runtime   trusted host effect service, one-shot
//!                                    key, DUAL fences -> the immutable
//!                                    ExecutionConsumptionReceipt Kovee's
//!                                    broker must hold before egress
//! ```
//!
//! The class subject is NEVER caller-shaped (§10.6): `subject_atoms` is
//! compiled from the dependency closure — the Mandate's purpose and data
//! classes, the Society's classification binding, the act's driver audience
//! and the egress ceiling — and every mandatory domain of the act's class
//! must be present or preparation fails closed.

use bpp_core::digest::DigestRef;
use bpp_core::ops;
use bpp_core::problem::{Problem, ProblemKind};
use bpp_core::time::{parse_rfc3339_utc, rfc3339_utc};
use byom_store::effects::Effect;
use byom_store::rows;
use byom_store::{CrashHooks, CursorMint, MutationScope, Prepared, Store};
use rusqlite::Connection;
use serde_json::{json, Map, Value};

use crate::episode_ops::{
    ensure_runtime_token_files, head_row, verify_runtime_token, RuntimeChannel,
};
use crate::gov_ops::{check_meta_binding, db_err, digest_json, mint, obj_pairs, run};
use crate::part_common::{
    self, all_seats_assent, conn_record_digest, digest_of, mint_position, prepare_trace,
    record_position, seats_from_json, seats_json, source_row, Caller, Seat,
};
use crate::part_ops::{event, expire_mandate_if_due};
use crate::{gov_decision, state};

/// The trusted host effect service's actor string (§14.7
/// `execution_permit_consume` row).
pub const ACTOR_EFFECT_SERVICE: &str = "kovee-adapter:effect-service";

/// The per-act reservation on the mandate's §11.4 ceiling set (dimension
/// `unit`). §13.1 fixes no per-act worst case; this bundle pins one
/// (recorded deviation, gap note G47) so the receipt names a real
/// reservation set and conservation has quantities to move.
pub const ACT_CEILING: u64 = 64;

/// The `model_egress` quantity ceiling the class subject carries: the
/// output-byte dimension Δ4 makes mandatory for a model call. §13.4 fixes
/// no number; pinned here (recorded deviation).
pub const MODEL_EGRESS_OUTPUT_BYTES: u64 = 262_144;

/// The compiled act-class subject omits a domain its class makes mandatory,
/// or pins a binding the caller is not. Fails closed: a subject that omits
/// a constrained domain can never satisfy an allow rule (Δ4).
pub fn class_subject_incomplete(detail: &str) -> Problem {
    Problem::new(
        ProblemKind::PolicyConflict,
        "the act-class subject does not carry its mandatory domains",
    )
    .with_status(409)
    .with_detail(detail.to_owned())
}

/// No consumable permit: the act is not `authorized` (§13.1 steps 4-5).
fn no_permit(detail: &str) -> Problem {
    Problem::new(
        ProblemKind::DecisionIncomplete,
        "the act carries no consumable execution permit",
    )
    .with_status(409)
    .with_detail(detail.to_owned())
}

fn opt_json(v: &Option<String>) -> Value {
    v.as_ref().map(|s| json!(s)).unwrap_or(Value::Null)
}

fn opt_digest(v: &Option<DigestRef>) -> Value {
    v.as_ref().map(digest_json).unwrap_or(Value::Null)
}

fn json_text(v: &Value) -> Value {
    json!(v.to_string())
}

pub fn stable_execution_key(intent_id: &str) -> String {
    format!("exec-{intent_id}")
}

pub fn reservation_set_ref(intent_id: &str) -> String {
    format!("rset-{intent_id}")
}

// ============================================ the Δ4 act-class subject ===

/// Compiles the Δ4 class subject from the dependency closure: one concrete
/// value per supplied domain, none of them caller-shaped. Returns `None`
/// when `kind` is not one of the five act classes — an open-`kind` act
/// carries no class subject at all, and therefore cannot reach a
/// class-bound driver.
#[allow(clippy::too_many_arguments)]
fn compile_class_subject(
    conn: &Connection,
    society_id: &str,
    intent_id: &str,
    kind: &str,
    mandate: &Map<String, Value>,
    driver_audience: Option<&str>,
    subject_ref: &str,
) -> Result<Option<Value>, Problem> {
    let Some(mandatory) = ops::mandatory_domains(kind) else {
        return Ok(None);
    };
    let purpose_ref = rows::str_of(mandate, "purpose_ref").to_owned();
    let mut atoms = Map::new();
    // operation: the versioned BPP operation this act is consumed through.
    atoms.insert("operation".into(), json!("execution_permit_consume"));
    // purpose: the exact node in a byom-pinned acyclic purpose snapshot.
    // §10.5 requires the pin; §13.1 names no snapshot record, so byom
    // pins its own over the Mandate's purpose (recorded derivation).
    if !purpose_ref.is_empty() {
        let snapshot = conn_record_digest(
            conn,
            society_id,
            &format!("{intent_id}-purpose-snapshot"),
            "bpp-purpose-snapshot-v0",
            &json!({"purpose_ref": purpose_ref}),
        )?;
        atoms.insert(
            "purpose".into(),
            json!({"snapshot": digest_json(&snapshot), "path": [purpose_ref]}),
        );
    }
    // classification: one element of the Society's pinned lattice.
    let society = rows::get_society(conn, society_id)
        .map_err(db_err)?
        .ok_or_else(state::not_found)?;
    let classes: Vec<String> =
        serde_json::from_value(rows::json_of(mandate, "data_class_selectors")).unwrap_or_default();
    if let Some(element) = classes.first() {
        let lattice: Value =
            serde_json::from_str(&society.classification_binding_digest).unwrap_or(Value::Null);
        atoms.insert(
            "classification".into(),
            json!({"lattice": lattice, "element": element}),
        );
    }
    // binding: the EXACT provider/driver binding bytes leave through.
    if let Some(audience) = driver_audience {
        atoms.insert("binding".into(), json!(format!("kovee:{audience}")));
    }
    // object: the exact governed object this act names.
    atoms.insert("object".into(), json!(format!("byom:{subject_ref}")));
    // quantity: the exact ceiling. `model_egress` and `budget` differ in
    // dimension, so each names its own.
    let quantity = match kind {
        "model_egress" => json!({
            "dimension": "output_bytes", "canonical_unit": "byte",
            "scale": 0, "amount": MODEL_EGRESS_OUTPUT_BYTES,
        }),
        _ => json!({
            "dimension": "unit", "canonical_unit": "unit",
            "scale": 0, "amount": ACT_CEILING,
        }),
    };
    atoms.insert("quantity".into(), quantity);
    // Fail closed on a missing mandatory domain: the compiled subject is
    // the authorization subject, and a domain the policy constrains but
    // the subject omits can never match an allow rule (Δ4).
    let missing: Vec<&str> = mandatory
        .iter()
        .copied()
        .filter(|d| !atoms.contains_key(*d))
        .collect();
    if !missing.is_empty() {
        return Err(class_subject_incomplete(&format!(
            "act class {kind} makes {missing:?} mandatory; the dependency closure supplies no \
             value for it (a Mandate without data classes cannot authorize a classified act, and \
             a model-egress act without a driver audience pins no provider binding)"
        )));
    }
    // Domains this class does not make mandatory are dropped: extra
    // domains only narrow further, and an unnecessary one would fence the
    // act against policies that do not constrain it.
    let mut kept = Map::new();
    for domain in mandatory {
        if let Some(v) = atoms.get(*domain) {
            kept.insert((*domain).to_owned(), v.clone());
        }
    }
    Ok(Some(json!({"act_class": kind, "subject_atoms": kept})))
}

/// Rechecks a committed class subject at consume time: every mandatory
/// domain still present, and the `binding` domain pins EXACTLY the
/// consuming driver audience.
fn recheck_class_subject(
    stored: &Value,
    act_class: &str,
    driver_audience: &str,
) -> Result<(), Problem> {
    let Some(mandatory) = ops::mandatory_domains(act_class) else {
        return Err(class_subject_incomplete(&format!(
            "{act_class} is not one of the five Δ4 act classes"
        )));
    };
    let atoms = stored.get("subject_atoms").cloned().unwrap_or(Value::Null);
    for domain in mandatory {
        if atoms.get(*domain).is_none() {
            return Err(class_subject_incomplete(&format!(
                "the committed class subject lost the mandatory {domain} domain"
            )));
        }
    }
    if mandatory.contains(&"binding") {
        let want = format!("kovee:{driver_audience}");
        if atoms.get("binding").and_then(Value::as_str) != Some(want.as_str()) {
            return Err(class_subject_incomplete(&format!(
                "the class subject pins provider binding {:?}, not {want:?}: a broker with \
                 another audience cannot consume this act",
                atoms.get("binding").and_then(Value::as_str).unwrap_or("")
            )));
        }
    }
    Ok(())
}

// ================================================= act_intent_prepare ====

/// `act_intent_prepare` (participant, create; R19).
pub fn act_intent_prepare(
    store: &mut Store,
    caller: &Caller,
    req: &ops::ActIntentPrepareRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    check_meta_binding(store, &req.meta, &caller.society_id)?;
    expire_mandate_if_due(store, &req.mandate_ref, now)?;
    let reply = act_intent_prepare_inner(store, caller, req, body, now, hooks)?;
    // The permit channel is published for the prepared act, so its own
    // state check — not an opaque forbidden — is what refuses a consumption
    // before authorization.
    ensure_runtime_token_files(store);
    Ok(reply)
}

fn act_intent_prepare_inner(
    store: &mut Store,
    caller: &Caller,
    req: &ops::ActIntentPrepareRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    let sovereign = rows::sovereign_participant(store.conn(), &caller.society_id)
        .map_err(db_err)?
        .ok_or_else(state::not_found)?;
    let intent_id = mint(store, "act")?;
    let seat_ref = mint(store, "seat-human")?;
    let dependency_set_ref = mint(store, "deps")?;
    let reservation_id = mint(store, "rsv")?;
    let prepare_event = mint(store, "evt")?;
    let created_at = rfc3339_utc(now);
    let key = stable_execution_key(&intent_id);
    let set_ref = reservation_set_ref(&intent_id);
    let incarnation = store
        .incarnation()
        .map_err(|e| state::internal(&e.to_string()))?;
    let recovery_epoch = store
        .recovery_epoch(&caller.society_id)
        .map_err(|e| state::internal(&e.to_string()))?;

    // The class subject is compiled BEFORE the subject digest: it is part
    // of the subject a seat assents to.
    let mandate = rows::get_row(store.conn(), "mandates", "mandate_id", &req.mandate_ref)
        .map_err(db_err)?
        .ok_or_else(state::not_found)?;
    let class_subject = compile_class_subject(
        store.conn(),
        &caller.society_id,
        &intent_id,
        &req.kind,
        &mandate,
        req.driver_audience.as_deref(),
        &req.subject_ref,
    )?;
    // The §13.1 preconditions, server-derived from the cited bindings.
    let preconditions = json!([
        {"kind": "mandate_current", "mandate_ref": req.mandate_ref,
         "mandate_revision": req.mandate_revision},
        {"kind": "participant_standing_active",
         "participant_ref": caller.participant.participant_id},
        {"kind": "dual_fences_current",
         "detail": "the exact Episode lease fence and the Kovee invocation fence"},
    ]);
    let subject = json!({
        "intent_id": intent_id,
        "kind": req.kind,
        "act_class_subject": class_subject,
        "execution_kind": req.execution_kind,
        "subject_ref": req.subject_ref,
        "subject_revision": req.subject_revision,
        "requested_by_participant": caller.participant.participant_id,
        "mandate_ref": req.mandate_ref,
        "mandate_revision": req.mandate_revision,
        "mandate_digest": digest_json(&req.mandate_digest),
        "context_manifest_ref": opt_json(&req.context_manifest_ref),
        "disclosure_manifest_ref": opt_json(&req.disclosure_manifest_ref),
        "driver_audience": opt_json(&req.driver_audience),
        "budget_reservation_set_ref": set_ref,
        "preconditions": preconditions,
        "stable_execution_key": key,
    });
    let subject_digest = store
        .mint_object_digest(
            &format!("society-key:{}/object:{intent_id}", caller.society_id),
            "bpp-act-intent-subject-v0",
            &subject,
        )
        .map_err(|e| state::internal(&e.to_string()))?;
    let mut field_sources = vec![
        source_row(
            "/intent_id",
            &req.meta.request_id,
            "/meta/request_id",
            "t-mint-id",
        ),
        source_row("/revision", &req.meta.request_id, "", "t-server-constant"),
        source_row("/state", &req.meta.request_id, "", "t-server-constant"),
        source_row("/kind", &req.meta.request_id, "/kind", "t-copy"),
        source_row(
            "/execution_kind",
            &req.meta.request_id,
            "/execution_kind",
            "t-copy",
        ),
        source_row(
            "/requested_by_participant",
            &req.meta.request_id,
            "",
            "t-channel-derived-actor",
        ),
        source_row(
            "/subject_ref",
            &req.meta.request_id,
            "/subject_ref",
            "t-copy",
        ),
        source_row(
            "/subject_revision",
            &req.meta.request_id,
            "/subject_revision",
            "t-copy",
        ),
        source_row(
            "/subject_digest",
            &req.meta.request_id,
            "",
            "t-digest-prepared-subject",
        ),
        source_row(
            "/preconditions",
            &req.meta.request_id,
            "",
            "t-compile-dependency-closure",
        ),
        source_row(
            "/mandate_ref",
            &req.meta.request_id,
            "/mandate_ref",
            "t-copy",
        ),
        source_row(
            "/mandate_revision",
            &req.meta.request_id,
            "/mandate_revision",
            "t-copy",
        ),
        source_row(
            "/mandate_digest",
            &req.meta.request_id,
            "/mandate_digest",
            "t-copy",
        ),
        source_row(
            "/authorization_dependency_set_ref",
            &req.meta.request_id,
            "",
            "t-mint-id",
        ),
        source_row(
            "/dependency_digest",
            &req.meta.request_id,
            "",
            "t-digest-dependency-closure",
        ),
        source_row(
            "/stable_execution_key",
            &req.meta.request_id,
            "",
            "t-derive-one-shot-key",
        ),
        source_row(
            "/required_seat_refs",
            &req.meta.request_id,
            "",
            "t-compile-required-seats",
        ),
        source_row("/expires_at", &req.meta.request_id, "", "t-server-time"),
        source_row(
            "/budget_reservation_set_ref",
            &req.meta.request_id,
            "",
            "t-derive-reservation-set",
        ),
    ];
    if class_subject.is_some() {
        field_sources.push(source_row(
            "/act_class_subject",
            &req.meta.request_id,
            "/kind",
            "t-compile-act-class-subject",
        ));
    }
    for (pointer, source) in [
        ("/context_manifest_ref", req.context_manifest_ref.is_some()),
        (
            "/context_manifest_digest",
            req.context_manifest_digest.is_some(),
        ),
        (
            "/disclosure_manifest_ref",
            req.disclosure_manifest_ref.is_some(),
        ),
        (
            "/disclosure_manifest_digest",
            req.disclosure_manifest_digest.is_some(),
        ),
        ("/driver_audience", req.driver_audience.is_some()),
        ("/endeavor_ref", req.endeavor_ref.is_some()),
        ("/pledge_ref", req.pledge_ref.is_some()),
    ] {
        if source {
            field_sources.push(source_row(pointer, &req.meta.request_id, pointer, "t-copy"));
        }
    }
    let trace = prepare_trace(
        store,
        &caller.society_id,
        "act_intent_prepare",
        &caller.actor,
        &req.meta.request_id,
        body,
        &subject_digest,
        &dependency_set_ref,
        field_sources,
        now,
    )?;
    let dependency_digest = store
        .mint_object_digest(
            &format!(
                "society-key:{}/object:{dependency_set_ref}",
                caller.society_id
            ),
            "bpp-act-dependency-set-v0",
            &json!({
                "dependency_set_ref": dependency_set_ref,
                "mandate_ref": req.mandate_ref,
                "mandate_revision": req.mandate_revision,
                "participant_ref": caller.participant.participant_id,
                "participant_binding_epoch": caller.participant.binding_epoch,
                "endpoint_incarnation": incarnation,
                "recovery_epoch": recovery_epoch,
            }),
        )
        .map_err(|e| state::internal(&e.to_string()))?;
    // The one required seat of this slice: the human-authority GATE seat,
    // filled on the governance surface (R21; family contract L48-L50 — the
    // gate inbox renders pending prepared intents and eligible seats).
    let seats = vec![Seat {
        seat_ref: seat_ref.clone(),
        kind: "human_authority".into(),
        participant_ref: sovereign.participant_id.clone(),
        surface: "governance".into(),
    }];
    // A one-hour preparation window (§13.1 fixes none; pinned here).
    let expires_at = rfc3339_utc(now + 3_600);
    let intent_record = json!({
        "intent_id": intent_id,
        "revision": 1,
        "state": "prepared",
        "subject_digest": digest_json(&subject_digest),
        "dependency_digest": digest_json(&dependency_digest),
        "stable_execution_key": key,
        "expires_at": expires_at,
        "created_at": created_at,
    });
    let intent_digest = store
        .mint_object_digest(
            &format!(
                "society-key:{}/object:{intent_id}-record",
                caller.society_id
            ),
            "bpp-act-intent-v0",
            &intent_record,
        )
        .map_err(|e| state::internal(&e.to_string()))?;

    let scope = MutationScope {
        society_id: caller.society_id.clone(),
        operation: "act_intent_prepare".into(),
        actor: caller.actor.clone(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let caller_c = caller.clone();
    let req_c = req.clone();
    let class_c = class_subject.clone();
    let trace_c = trace.clone();
    run(store, scope, now, hooks, move |conn, _| {
        // The complete dependency closure, revalidated inside the prepare
        // transaction (§10.6): the Mandate is current and covers this act
        // class, the caller is its grantee, and its Standing is active.
        let mandate = rows::get_row(conn, "mandates", "mandate_id", &req_c.mandate_ref)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        if rows::u64_of(&mandate, "revision") != req_c.mandate_revision {
            return Err(state::stale_revision());
        }
        if !req_c
            .mandate_digest
            .same_ref_json(&rows::json_of(&mandate, "subject_digest"))
        {
            return Err(state::invalid(
                "mandate_digest does not pin the exact current Mandate subject",
            ));
        }
        act_mandate_gate(&mandate, &caller_c.participant.participant_id, &req_c.kind)?;
        let row = obj_pairs([
            ("intent_id", json!(intent_id)),
            ("society_id", json!(caller_c.society_id)),
            ("revision", json!(1)),
            ("endpoint_incarnation", json!(incarnation)),
            ("recovery_epoch", json!(recovery_epoch)),
            (
                "requested_by_participant",
                json!(caller_c.participant.participant_id),
            ),
            ("actor_ref", json!(caller_c.actor)),
            ("endeavor_ref", opt_json(&req_c.endeavor_ref)),
            ("pledge_ref", opt_json(&req_c.pledge_ref)),
            ("preparation_trace_ref", trace_c["trace_id"].clone()),
            ("preparation_trace_digest", trace_c["digest"].clone()),
            ("preparation_trace", json_text(&trace_c)),
            ("kind", json!(req_c.kind)),
            (
                "act_class",
                class_c
                    .as_ref()
                    .map(|c| c["act_class"].clone())
                    .unwrap_or(Value::Null),
            ),
            (
                "act_class_subject",
                class_c.as_ref().map(json_text).unwrap_or(Value::Null),
            ),
            ("execution_kind", json!(req_c.execution_kind)),
            ("subject_ref", json!(req_c.subject_ref)),
            ("subject_revision", json!(req_c.subject_revision)),
            ("subject_digest", digest_json(&subject_digest)),
            ("intent_digest", digest_json(&intent_digest)),
            ("preconditions", json_text(&preconditions)),
            (
                "context_manifest_ref",
                opt_json(&req_c.context_manifest_ref),
            ),
            (
                "context_manifest_digest",
                opt_digest(&req_c.context_manifest_digest),
            ),
            (
                "disclosure_manifest_ref",
                opt_json(&req_c.disclosure_manifest_ref),
            ),
            (
                "disclosure_manifest_digest",
                opt_digest(&req_c.disclosure_manifest_digest),
            ),
            ("driver_audience", opt_json(&req_c.driver_audience)),
            ("budget_reservation_set_ref", json!(set_ref)),
            ("mandate_ref", json!(req_c.mandate_ref)),
            ("mandate_revision", json!(req_c.mandate_revision)),
            ("mandate_digest", digest_json(&req_c.mandate_digest)),
            (
                "authorization_dependency_set_ref",
                json!(dependency_set_ref),
            ),
            ("dependency_digest", digest_json(&dependency_digest)),
            ("authorization_decision_ref", Value::Null),
            ("authorization_slot_snapshot_digest", Value::Null),
            ("required_seat_refs", json_text(&seats_json(&seats))),
            ("stable_execution_key", json!(key)),
            ("expires_at", json!(expires_at)),
            ("state", json!("prepared")),
            ("created_at", json!(created_at)),
        ]);
        // The act reserves its ceiling in the SAME transition: no
        // unreserved authorization (§11.4/§13.1).
        let mut effects = Vec::new();
        part_common::reserve(
            conn,
            &mut effects,
            &caller_c.society_id,
            rows::str_of(&mandate, "budget_ceiling_set_ref"),
            part_common::UNIT_DIMENSION,
            ACT_CEILING,
            &reservation_id,
            "act_intent",
            &intent_id,
            now,
        )?;
        effects.push(Effect::Upsert {
            table: "act_intents".into(),
            row,
        });
        let mut result = json!({
            "intent_id": intent_id,
            "revision": 1,
            "state": "prepared",
            "kind": req_c.kind,
            "execution_kind": req_c.execution_kind,
            "requested_by_participant": caller_c.participant.participant_id,
            "subject_ref": req_c.subject_ref,
            "subject_revision": req_c.subject_revision,
            "subject_digest": digest_json(&subject_digest),
            "preconditions": preconditions,
            "mandate_ref": req_c.mandate_ref,
            "mandate_revision": req_c.mandate_revision,
            "mandate_digest": digest_json(&req_c.mandate_digest),
            "preparation_trace": trace_c,
            "authorization_dependency_set_ref": dependency_set_ref,
            "dependency_digest": digest_json(&dependency_digest),
            "stable_execution_key": key,
            "budget_reservation_set_ref": set_ref,
            "required_seat_refs": [seat_ref],
            "expires_at": expires_at,
        });
        if let Some(class) = &class_c {
            result["act_class"] = class["act_class"].clone();
            result["act_class_subject"] = class.clone();
        }
        for (name, value) in [
            (
                "context_manifest_ref",
                opt_json(&req_c.context_manifest_ref),
            ),
            (
                "context_manifest_digest",
                opt_digest(&req_c.context_manifest_digest),
            ),
            (
                "disclosure_manifest_ref",
                opt_json(&req_c.disclosure_manifest_ref),
            ),
            (
                "disclosure_manifest_digest",
                opt_digest(&req_c.disclosure_manifest_digest),
            ),
            ("driver_audience", opt_json(&req_c.driver_audience)),
            ("endeavor_ref", opt_json(&req_c.endeavor_ref)),
            ("pledge_ref", opt_json(&req_c.pledge_ref)),
        ] {
            if !value.is_null() {
                result[name] = value;
            }
        }
        Ok(Prepared {
            result,
            revision: Some(1),
            cursor: CursorMint::AfterEvents {
                society_id: caller_c.society_id.clone(),
            },
            effects,
            events: vec![event(
                &caller_c.society_id,
                &prepare_event,
                "act-intent.prepared",
                &intent_id,
                1,
                &caller_c.participant.participant_id,
                &caller_c.actor,
                &req_c.meta,
                json!({"state": "prepared", "kind": req_c.kind,
                       "act_class": class_c.as_ref().map(|c| c["act_class"].clone())
                                          .unwrap_or(Value::Null),
                       "stable_execution_key": key,
                       "required_seat_refs": [seat_ref]}),
            )],
        })
    })
}

/// The §10.1 Mandate gate reused for an act: current, granted to THIS
/// participant, and covering this act class or kind.
fn act_mandate_gate(
    mandate: &Map<String, Value>,
    caller_participant: &str,
    kind: &str,
) -> Result<(), Problem> {
    match rows::str_of(mandate, "state") {
        "active" => {}
        "held" => {
            return Err(Problem::new(
                ProblemKind::MandateHeld,
                "the bound mandate is held; new uses are fenced",
            )
            .with_status(409))
        }
        "prepared" => {
            return Err(Problem::new(
                ProblemKind::DecisionIncomplete,
                "the bound mandate has not been issued",
            )
            .with_status(409))
        }
        other => {
            return Err(state::stale_binding(&format!(
                "the bound mandate is {other}"
            )))
        }
    }
    if rows::str_of(mandate, "grantee_participant_ref") != caller_participant {
        return Err(state::forbidden_detail(
            "an ActIntent is prepared only by the Mandate's own grantee",
        ));
    }
    let allowed: Vec<String> =
        serde_json::from_value(rows::json_of(mandate, "allowed_operations")).unwrap_or_default();
    if !allowed.iter().any(|a| a == kind) {
        return Err(state::forbidden_detail(&format!(
            "the mandate's allowed_operations do not cover the act class {kind}: Mandates bound \
             the Δ4 act classes, and a class outside the grant is never authorized"
        )));
    }
    Ok(())
}

// ================================================ act_intent_position ====

/// `act_intent_position` (participant R20 / governance R21): the actor
/// fills ONLY its own exact prepared seat, on the surface that seat names.
#[allow(clippy::too_many_arguments)]
pub fn act_intent_position(
    store: &mut Store,
    society_id: &str,
    actor_participant: &str,
    actor: &str,
    surface: &str,
    req: &ops::PositionRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    check_meta_binding(store, &req.meta, society_id)?;
    let minted = mint_position(store)?;
    let scope = MutationScope {
        society_id: society_id.to_owned(),
        operation: "act_intent_position".into(),
        actor: actor.to_owned(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let req_c = req.clone();
    let society = society_id.to_owned();
    let actor_participant = actor_participant.to_owned();
    let actor_c = actor.to_owned();
    let surface_c = surface.to_owned();
    run(store, scope, now, hooks, move |conn, _| {
        let intent = rows::get_row(conn, "act_intents", "intent_id", &req_c.proposal_ref)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        let intent_state = rows::str_of(&intent, "state").to_owned();
        if !matches!(intent_state.as_str(), "prepared" | "awaiting_decision") {
            return Err(state::stale_binding(&format!(
                "the ActIntent is {intent_state}: only a prepared or awaiting_decision intent is \
                 positionable"
            )));
        }
        let seats = seats_from_json(&rows::json_of(&intent, "required_seat_refs"));
        let subject = rows::json_of(&intent, "subject_digest");
        let (mut effects, result) = record_position(
            conn,
            &minted,
            "act_intent",
            &society,
            rows::u64_of(&intent, "revision"),
            &digest_of(&subject)?,
            &seats,
            &req_c,
            &actor_participant,
            &actor_c,
            &surface_c,
            now,
        )?;
        // prepared -> awaiting_decision on the FIRST seat position (§14.8
        // ActIntent row). The head revision does not move: finalization
        // CASes the prepared revision, exactly like the mandate chain.
        if intent_state == "prepared" {
            let mut awaiting = intent.clone();
            awaiting.insert("state".into(), json!("awaiting_decision"));
            effects.push(Effect::Upsert {
                table: "act_intents".into(),
                row: awaiting,
            });
        }
        Ok(Prepared {
            result,
            revision: None,
            cursor: CursorMint::AfterEvents {
                society_id: society.clone(),
            },
            effects,
            events: vec![event(
                &society,
                &minted.event_id,
                "act-intent.awaiting_decision",
                &req_c.proposal_ref,
                rows::u64_of(&intent, "revision"),
                rows::str_of(&intent, "requested_by_participant"),
                &actor_c,
                &req_c.meta,
                json!({"seat_ref": req_c.seat_ref, "value": req_c.value,
                       "state": "awaiting_decision", "surface": surface_c}),
            )],
        })
    })
}

// ================================================ act_intent_finalize ====

/// `act_intent_finalize` (participant R22 / governance R23): deterministic
/// finalization. It authors NO seat — the complete required set must
/// already hold a current `assent` head — and binds ONE immutable
/// GovernanceDecision to the exact prepared intent digest.
#[allow(clippy::too_many_arguments)]
pub fn act_intent_finalize(
    store: &mut Store,
    society_id: &str,
    actor: &str,
    req: &ops::ActIntentFinalizeRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    check_meta_binding(store, &req.meta, society_id)?;
    let decision_ref = gov_decision::act_decision_ref(&req.intent_id);
    let finalize_event = mint(store, "evt")?;
    let scope = MutationScope {
        society_id: society_id.to_owned(),
        operation: "act_intent_finalize".into(),
        actor: actor.to_owned(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let req_c = req.clone();
    let society = society_id.to_owned();
    let actor_c = actor.to_owned();
    let decision_c = decision_ref.clone();
    let reply = run(store, scope, now, hooks, move |conn, _| {
        let intent = rows::get_row(conn, "act_intents", "intent_id", &req_c.intent_id)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        if req_c.meta.expected_revision != Some(rows::u64_of(&intent, "revision")) {
            return Err(state::stale_revision());
        }
        if rows::str_of(&intent, "state") != "awaiting_decision" {
            return Err(state::stale_binding(&format!(
                "the ActIntent is {}: finalization requires awaiting_decision",
                rows::str_of(&intent, "state")
            )));
        }
        let subject = rows::json_of(&intent, "subject_digest");
        if !req_c.subject_digest.same_ref_json(&subject) {
            return Err(state::invalid(
                "subject_digest does not commit to the exact prepared act subject",
            ));
        }
        if parse_rfc3339_utc(rows::str_of(&intent, "expires_at")).is_some_and(|t| t <= now) {
            return Err(state::stale_binding(
                "the ActIntent expired; a new act needs a fresh preparation",
            ));
        }
        let seats = seats_from_json(&rows::json_of(&intent, "required_seat_refs"));
        all_seats_assent(conn, "act_intent", &req_c.intent_id, &seats)?;
        let subject_digest: DigestRef = digest_of(&subject)?;
        let decision_seats: Vec<gov_decision::DecisionSeat> = seats
            .iter()
            .map(|s| {
                let epoch = rows::get_participant(conn, &s.participant_ref)
                    .ok()
                    .flatten()
                    .map(|p| p.binding_epoch)
                    .unwrap_or(0);
                gov_decision::DecisionSeat {
                    seat_ref: s.seat_ref.clone(),
                    participant_ref: s.participant_ref.clone(),
                    actor_ref: crate::gov_ops::ACTOR_GOVERNANCE.to_owned(),
                    participant_binding_epoch: epoch,
                }
            })
            .collect();
        let decision = gov_decision::form(
            conn,
            &decision_c,
            &society,
            gov_decision::KIND_ACT_AUTHORIZATION,
            "act_intent",
            &req_c.intent_id,
            &subject_digest,
            rows::str_of(&intent, "authorization_dependency_set_ref"),
            &decision_seats,
            &[],
            "act_intent_finalize",
            crate::gov_ops::ACTOR_GOVERNANCE,
            now,
        )?;
        // The exact active slot snapshot this finalization locks.
        let snapshot_digest = conn_record_digest(
            conn,
            &society,
            &format!("{}-slot-snapshot", req_c.intent_id),
            "bpp-act-slot-snapshot-v0",
            &json!({
                "intent_ref": req_c.intent_id,
                "subject_digest": subject,
                "seats": seats_json(&seats),
            }),
        )?;
        let revision = rows::u64_of(&intent, "revision") + 1;
        let mut authorized = intent.clone();
        authorized.insert("state".into(), json!("authorized"));
        authorized.insert("revision".into(), json!(revision));
        authorized.insert("authorization_decision_ref".into(), json!(decision_c));
        authorized.insert(
            "authorization_slot_snapshot_digest".into(),
            digest_json(&snapshot_digest),
        );
        Ok(Prepared {
            result: json!({
                "intent_id": req_c.intent_id,
                "revision": revision,
                "state": "authorized",
                "authorization_decision_ref": decision_c,
                "authorization_slot_snapshot_digest": digest_json(&snapshot_digest),
                "stable_execution_key": rows::str_of(&intent, "stable_execution_key"),
                "expires_at": rows::str_of(&intent, "expires_at"),
            }),
            revision: Some(revision),
            cursor: CursorMint::AfterEvents {
                society_id: society.clone(),
            },
            effects: vec![
                decision,
                Effect::Upsert {
                    table: "act_intents".into(),
                    row: authorized,
                },
            ],
            events: vec![event(
                &society,
                &finalize_event,
                "act-intent.authorized",
                &req_c.intent_id,
                revision,
                rows::str_of(&intent, "requested_by_participant"),
                &actor_c,
                &req_c.meta,
                json!({"state": "authorized",
                       "authorization_decision_ref": decision_c,
                       "one_shot": "the permit is consumable exactly once, by the trusted host \
                                    effect service, under both current fences"}),
            )],
        })
    })?;
    // The permit channel exists only while an act is authorized.
    ensure_runtime_token_files(store);
    Ok(reply)
}

// ======================================== execution_permit_consume =======

/// `execution_permit_consume` (runtime, update; R34). The one-shot
/// consumption protocol of §13.1 steps 4-6: byom atomically rechecks
/// charter, standing, Mandate, decisions, dependencies, ceilings, expiry
/// and BOTH fences, inserts the MandateUse once, and returns ONE immutable
/// ExecutionConsumptionReceipt. Repeating the same canonical request and
/// key returns the same receipt; a changed request conflicts; a different
/// key cannot consume the spent decision.
pub fn execution_permit_consume(
    store: &mut Store,
    token: &str,
    req: &ops::ExecutionPermitConsumeRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    // The permit channel is bound to the EXACT prepared act (R34 workload
    // identity): a token for another act never matches, and a consumption
    // presenting another one-shot key still reaches byom's spent-decision
    // check rather than a silent forbidden.
    verify_runtime_token(store, token, RuntimeChannel::Permit, &req.intent_ref)?;
    let intent = rows::get_row(store.conn(), "act_intents", "intent_id", &req.intent_ref)
        .map_err(db_err)?
        .ok_or_else(state::not_found)?;
    let society_id = rows::str_of(&intent, "society_id").to_owned();
    check_meta_binding(store, &req.meta, &society_id)?;
    let receipt_id = mint(store, "ecr")?;
    let mandate_use_id = mint(store, "muse")?;
    let consume_event = mint(store, "evt")?;
    let issued_at = rfc3339_utc(now);
    let expires_at = rfc3339_utc(now + 3_600);
    let byom_endpoint_ref = match crate::host_config::HostConfig::load(store) {
        Ok(cfg) => cfg.realm_byom_binding.byom_endpoint_ref.clone(),
        Err(_) => "byom-endpoint-local".to_owned(),
    };
    let incarnation = store
        .incarnation()
        .map_err(|e| state::internal(&e.to_string()))?;
    let recovery_epoch = store
        .recovery_epoch(&society_id)
        .map_err(|e| state::internal(&e.to_string()))?;
    let scope = MutationScope {
        society_id: society_id.clone(),
        operation: "execution_permit_consume".into(),
        actor: ACTOR_EFFECT_SERVICE.into(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let req_c = req.clone();
    let society_c = society_id.clone();
    run(store, scope, now, hooks, move |conn, _| {
        let intent = rows::get_row(conn, "act_intents", "intent_id", &req_c.intent_ref)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        let intent_state = rows::str_of(&intent, "state").to_owned();
        // -- the SPENT one-shot decision (§13.1 step 6, gap note G37) ----
        // Checked BEFORE the key equality: a DIFFERENT key against a spent
        // decision is `stale_revision` — it can never claim the exhausted
        // one-shot slot — while only the identical canonical request and
        // key recover the retained receipt.
        if intent_state == "consumed" {
            let stored = rows::get_row(
                conn,
                "execution_consumption_receipts",
                "stable_execution_key",
                rows::str_of(&intent, "stable_execution_key"),
            )
            .map_err(db_err)?
            .ok_or_else(|| state::internal("consumed act without its receipt"))?;
            if rows::str_of(&stored, "stable_execution_key") != req_c.stable_execution_key {
                return Err(Problem::new(
                    ProblemKind::StaleRevision,
                    "expected revision is no longer current",
                )
                .with_status(409)
                .with_detail(
                    "a different stable_execution_key can never consume the SPENT one-shot \
                     decision (§13.1 step 6): only the identical canonical request and key \
                     recover the retained receipt"
                        .to_owned(),
                ));
            }
            // Only the byte-identical canonical binding replays.
            let changed = rows::str_of(&stored, "host_effect_ref") != req_c.host_effect_ref
                || !req_c
                    .host_effect_digest
                    .same_ref_json(&rows::json_of(&stored, "host_effect_digest"))
                || !req_c
                    .subject_digest
                    .same_ref_json(&rows::json_of(&stored, "subject_digest"))
                || rows::str_of(&stored, "driver_audience") != req_c.driver_audience
                || rows::u64_of(&stored, "byom_fence_epoch") != req_c.byom_fence_epoch
                || rows::u64_of(&stored, "host_fence_epoch") != req_c.host_fence_epoch;
            if changed {
                return Err(Problem::new(
                    ProblemKind::IdempotencyMismatch,
                    "same one-shot key, different canonical request",
                )
                .with_status(409)
                .with_detail(
                    "only the byte-identical canonical request replays to the retained receipt \
                     (§13.1 step 6)"
                        .to_owned(),
                ));
            }
            return Ok(Prepared {
                result: receipt_result(&stored, true),
                revision: Some(rows::u64_of(&intent, "revision")),
                cursor: CursorMint::AfterEvents {
                    society_id: society_c.clone(),
                },
                effects: Vec::new(),
                events: Vec::new(),
            });
        }
        if intent_state != "authorized" {
            return Err(match intent_state.as_str() {
                "prepared" | "awaiting_decision" => no_permit(&format!(
                    "the ActIntent is {intent_state}: no GovernanceDecision authorizes it yet, so \
                     there is nothing to consume"
                )),
                other => state::stale_binding(&format!(
                    "the ActIntent is {other}; a terminal act never becomes consumable"
                )),
            });
        }
        if rows::str_of(&intent, "stable_execution_key") != req_c.stable_execution_key {
            return Err(state::stale_binding(
                "stable_execution_key is not this ActIntent's one-shot key",
            ));
        }
        // The CAS on the authorized head revision.
        if req_c.meta.expected_revision != Some(rows::u64_of(&intent, "revision")) {
            return Err(state::stale_revision());
        }
        // The exact intent, subject and disclosure the decision bound.
        if !req_c
            .intent_digest
            .same_ref_json(&rows::json_of(&intent, "intent_digest"))
        {
            return Err(state::stale_binding(
                "intent_digest does not pin the committed ActIntent record",
            ));
        }
        if !req_c
            .subject_digest
            .same_ref_json(&rows::json_of(&intent, "subject_digest"))
        {
            return Err(state::stale_binding(
                "subject_digest does not pin the exact authorized act subject",
            ));
        }
        if parse_rfc3339_utc(rows::str_of(&intent, "expires_at")).is_some_and(|t| t <= now) {
            return Err(state::stale_binding("the authorized ActIntent expired"));
        }
        if rows::str_of(&intent, "budget_reservation_set_ref") != req_c.budget_reservation_set_ref {
            return Err(state::stale_binding(
                "budget_reservation_set_ref is not the act's reserved set",
            ));
        }
        let stored_audience = rows::str_of(&intent, "driver_audience").to_owned();
        if stored_audience != req_c.driver_audience {
            return Err(state::stale_binding(
                "driver_audience is not the audience the act was authorized for",
            ));
        }
        // -- the Δ4 class-subject recheck --------------------------------
        let act_class = rows::str_of(&intent, "act_class").to_owned();
        if act_class.is_empty() {
            return Err(class_subject_incomplete(
                "this act carries no Δ4 act-class subject: an external effect leaves only \
                 through one of the five closed act classes",
            ));
        }
        let stored_subject: Value =
            serde_json::from_str(rows::str_of(&intent, "act_class_subject")).unwrap_or(Value::Null);
        recheck_class_subject(&stored_subject, &act_class, &req_c.driver_audience)?;

        // -- the authorizing decision, still current ---------------------
        gov_decision::resolve(
            conn,
            rows::str_of(&intent, "authorization_decision_ref"),
            &gov_decision::Expect {
                society_id: &society_c,
                kind: gov_decision::KIND_ACT_AUTHORIZATION,
                subject_kind: "act_intent",
                subject_ref: &req_c.intent_ref,
                subject_digest: &digest_of(&rows::json_of(&intent, "subject_digest"))?,
                actor: crate::gov_ops::ACTOR_GOVERNANCE,
            },
        )?;

        // -- charter/standing/Mandate recheck ----------------------------
        let participant_ref = rows::str_of(&intent, "requested_by_participant").to_owned();
        let participant = rows::get_participant(conn, &participant_ref)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        if participant.state != "active" {
            return Err(state::stale_binding(
                "the requesting Participant holds no active Standing",
            ));
        }
        let mandate = rows::get_row(
            conn,
            "mandates",
            "mandate_id",
            rows::str_of(&intent, "mandate_ref"),
        )
        .map_err(db_err)?
        .ok_or_else(state::not_found)?;
        act_mandate_gate(&mandate, &participant_ref, &act_class)?;

        // -- the DUAL fences (family contract L21) -----------------------
        let episode_fence = match (&req_c.episode_ref, &req_c.episode_fence_digest) {
            (Some(episode_ref), Some(fence_digest)) => {
                let lease = rows::get_row(conn, "episode_lease_heads", "episode_id", episode_ref)
                    .map_err(db_err)?
                    .ok_or_else(state::stale_revision)?;
                if rows::u64_of(&lease, "byom_fence_epoch") != req_c.byom_fence_epoch {
                    return Err(Problem::new(
                        ProblemKind::StaleRevision,
                        "expected revision is no longer current",
                    )
                    .with_status(409)
                    .with_detail(
                        "stale byom_fence_epoch: a superseded lease attempt cannot consume an \
                         execution permit (§11.2/§13.1 step 5)"
                            .to_owned(),
                    ));
                }
                let binding = head_row(
                    conn,
                    "byom_episode_bindings",
                    "episode_ref",
                    episode_ref,
                    "byom_attempt_ref",
                    rows::str_of(&lease, "current_attempt_ref"),
                )?
                .ok_or_else(state::stale_revision)?;
                if rows::u64_of(&binding, "kovee_invocation_fence") != req_c.host_fence_epoch {
                    return Err(Problem::new(
                        ProblemKind::StaleRevision,
                        "expected revision is no longer current",
                    )
                    .with_status(409)
                    .with_detail(
                        "stale host_fence_epoch: a mutation carrying only ONE of the DUAL fences \
                         is refused (family contract L21)"
                            .to_owned(),
                    ));
                }
                if !fence_digest.same_ref_json(&rows::json_of(&binding, "digest")) {
                    return Err(state::stale_binding(
                        "episode_fence_digest does not pin the committed ByomEpisodeBinding",
                    ));
                }
                true
            }
            _ => {
                if rows::str_of(&intent, "execution_kind") == "external_effect" {
                    return Err(state::stale_binding(
                        "an external-effect act binds the exact Episode and BOTH fences; the \
                         episode ref/fence pair is not optional here (family contract L21)",
                    ));
                }
                false
            }
        };

        // -- MandateUse, inserted exactly ONCE ---------------------------
        let mandate_ref = rows::str_of(&intent, "mandate_ref").to_owned();
        let use_ordinal = conn
            .query_row(
                "SELECT COUNT(*) FROM mandate_uses WHERE mandate_ref = ?1",
                [&mandate_ref],
                |r| r.get::<_, i64>(0),
            )
            .map_err(db_err)?
            .max(0) as u64
            + 1;
        if rows::rows_where(
            conn,
            "mandate_uses",
            "use_key",
            &req_c.stable_execution_key,
            "mandate_use_id",
        )
        .map_err(db_err)?
        .iter()
        .any(|r| rows::str_of(r, "mandate_ref") == mandate_ref)
        {
            return Err(state::stale_revision());
        }
        let reservation_refs: Vec<String> = {
            let mut stmt = conn
                .prepare(
                    "SELECT reservation_id FROM budget_reservations
                     WHERE holder_kind = 'act_intent' AND holder_ref = ?1",
                )
                .map_err(db_err)?;
            let rows_iter = stmt
                .query_map([&req_c.intent_ref], |r| r.get(0))
                .map_err(db_err)?;
            let mut out = Vec::new();
            for row in rows_iter {
                out.push(row.map_err(db_err)?);
            }
            out
        };
        let use_record = json!({
            "mandate_use_id": mandate_use_id,
            "mandate_ref": mandate_ref,
            "mandate_digest": rows::json_of(&intent, "mandate_digest"),
            "intent_ref": req_c.intent_ref,
            "intent_digest": rows::json_of(&intent, "intent_digest"),
            "use_key": req_c.stable_execution_key,
            "use_ordinal": use_ordinal,
            "ceiling_reservation_refs": reservation_refs,
            "consumed_at": issued_at,
        });
        let use_digest = conn_record_digest(
            conn,
            &society_c,
            &mandate_use_id,
            "bpp-mandate-use-v0",
            &use_record,
        )?;
        let mut effects = vec![Effect::Upsert {
            table: "mandate_uses".into(),
            row: obj_pairs([
                ("mandate_use_id", json!(mandate_use_id)),
                ("society_id", json!(society_c)),
                ("mandate_ref", json!(mandate_ref)),
                ("mandate_digest", rows::json_of(&intent, "mandate_digest")),
                ("intent_ref", json!(req_c.intent_ref)),
                ("intent_digest", rows::json_of(&intent, "intent_digest")),
                ("use_key", json!(req_c.stable_execution_key)),
                ("use_ordinal", json!(use_ordinal)),
                (
                    "ceiling_reservation_refs",
                    json_text(&json!(reservation_refs)),
                ),
                (
                    "decision_refs",
                    json_text(&json!([rows::str_of(
                        &intent,
                        "authorization_decision_ref"
                    )])),
                ),
                ("consumed_at", json!(issued_at)),
                ("digest", digest_json(&use_digest)),
            ]),
        }];
        // The act's reservation is COMMITTED in the same transition: the
        // authorized quantity is spent exactly once (§11.4 conservation).
        part_common::settle_holder(conn, &mut effects, "act_intent", &req_c.intent_ref, true)?;

        // -- the ONE immutable ExecutionConsumptionReceipt ---------------
        let receipt_record = json!({
            "receipt_id": receipt_id,
            "byom_endpoint_ref": byom_endpoint_ref,
            "endpoint_incarnation": incarnation,
            "recovery_epoch": recovery_epoch,
            "intent_ref": req_c.intent_ref,
            "intent_digest": rows::json_of(&intent, "intent_digest"),
            "mandate_use_ref": mandate_use_id,
            "mandate_use_digest": digest_json(&use_digest),
            "stable_execution_key": req_c.stable_execution_key,
            "subject_digest": digest_json(&req_c.subject_digest),
            "disclosure_digest": opt_digest(&req_c.disclosure_digest),
            "driver_audience": req_c.driver_audience,
            "participant_ref": participant_ref,
            "episode_ref": opt_json(&req_c.episode_ref),
            "episode_fence_digest": opt_digest(&req_c.episode_fence_digest),
            "budget_reservation_set_ref": req_c.budget_reservation_set_ref,
            "issued_at": issued_at,
            "expires_at": expires_at,
            "max_uses": 1,
        });
        let receipt_digest = conn_record_digest(
            conn,
            &society_c,
            &receipt_id,
            "bpp-execution-consumption-receipt-v0",
            &receipt_record,
        )?;
        let receipt_row = obj_pairs([
            ("receipt_id", json!(receipt_id)),
            ("society_id", json!(society_c)),
            ("byom_endpoint_ref", json!(byom_endpoint_ref)),
            ("endpoint_incarnation", json!(incarnation)),
            ("recovery_epoch", json!(recovery_epoch)),
            ("intent_ref", json!(req_c.intent_ref)),
            ("intent_digest", rows::json_of(&intent, "intent_digest")),
            ("mandate_use_ref", json!(mandate_use_id)),
            ("mandate_use_digest", digest_json(&use_digest)),
            ("stable_execution_key", json!(req_c.stable_execution_key)),
            ("subject_digest", digest_json(&req_c.subject_digest)),
            ("disclosure_digest", opt_digest(&req_c.disclosure_digest)),
            ("driver_audience", json!(req_c.driver_audience)),
            ("participant_ref", json!(participant_ref)),
            ("episode_ref", opt_json(&req_c.episode_ref)),
            (
                "episode_fence_digest",
                opt_digest(&req_c.episode_fence_digest),
            ),
            (
                "budget_reservation_set_ref",
                json!(req_c.budget_reservation_set_ref),
            ),
            ("host_effect_ref", json!(req_c.host_effect_ref)),
            ("host_effect_digest", digest_json(&req_c.host_effect_digest)),
            ("byom_fence_epoch", json!(req_c.byom_fence_epoch)),
            ("host_fence_epoch", json!(req_c.host_fence_epoch)),
            ("issued_at", json!(issued_at)),
            ("expires_at", json!(expires_at)),
            ("max_uses", json!(1)),
            ("digest", digest_json(&receipt_digest)),
        ]);
        effects.push(Effect::Upsert {
            table: "execution_consumption_receipts".into(),
            row: receipt_row.clone(),
        });
        let revision = rows::u64_of(&intent, "revision") + 1;
        let mut consumed = intent.clone();
        consumed.insert("state".into(), json!("consumed"));
        consumed.insert("revision".into(), json!(revision));
        effects.push(Effect::Upsert {
            table: "act_intents".into(),
            row: consumed,
        });
        Ok(Prepared {
            result: receipt_result(&receipt_row, false),
            revision: Some(revision),
            cursor: CursorMint::AfterEvents {
                society_id: society_c.clone(),
            },
            effects,
            events: vec![event(
                &society_c,
                &consume_event,
                "act-intent.consumed",
                &req_c.intent_ref,
                revision,
                &participant_ref,
                ACTOR_EFFECT_SERVICE,
                &req_c.meta,
                json!({"state": "consumed", "max_uses": 1,
                       "act_class": act_class,
                       "mandate_use_ref": mandate_use_id,
                       "episode_bound": episode_fence,
                       "receipt_ref": receipt_id}),
            )],
        })
    })
}

fn receipt_result(row: &Map<String, Value>, replayed: bool) -> Value {
    let mut out = json!({
        "receipt_id": rows::str_of(row, "receipt_id"),
        "byom_endpoint_ref": rows::str_of(row, "byom_endpoint_ref"),
        "endpoint_incarnation": rows::str_of(row, "endpoint_incarnation"),
        "recovery_epoch": rows::u64_of(row, "recovery_epoch"),
        "intent_ref": rows::str_of(row, "intent_ref"),
        "intent_digest": rows::json_of(row, "intent_digest"),
        "mandate_use_ref": rows::str_of(row, "mandate_use_ref"),
        "mandate_use_digest": rows::json_of(row, "mandate_use_digest"),
        "stable_execution_key": rows::str_of(row, "stable_execution_key"),
        "subject_digest": rows::json_of(row, "subject_digest"),
        "driver_audience": rows::str_of(row, "driver_audience"),
        "participant_ref": rows::str_of(row, "participant_ref"),
        "budget_reservation_set_ref": rows::str_of(row, "budget_reservation_set_ref"),
        "issued_at": rows::str_of(row, "issued_at"),
        "expires_at": rows::str_of(row, "expires_at"),
        "max_uses": 1,
        "digest": rows::json_of(row, "digest"),
    });
    let disclosure = rows::json_of(row, "disclosure_digest");
    if !disclosure.is_null() {
        out["disclosure_digest"] = disclosure;
    }
    let episode = rows::str_of(row, "episode_ref").to_owned();
    if !episode.is_empty() {
        out["episode_ref"] = json!(episode);
        out["episode_fence_digest"] = rows::json_of(row, "episode_fence_digest");
    }
    if replayed {
        out["replayed"] = json!(true);
    }
    out
}
