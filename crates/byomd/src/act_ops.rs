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

use bpp_core::canonical::{hex, hmac_sha256, sha256_hex, tagged_canonical};
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
    ensure_runtime_token_files, head_row, runtime_token, verify_runtime_token, RuntimeChannel,
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

/// The consumption presents no valid registration for the host Effect it
/// names (R3-A02). The permit is bound to ONE exact prepared Effect, so a
/// caller-chosen ref and digest are refused before any state is read.
fn unregistered_effect(detail: &str) -> Problem {
    state::forbidden_detail(&format!(
        "the permit is bound to one exact prepared host Effect: {detail} (§13.1 step 3-4 — the \
         host durably creates its Effect and registers it under this act's permit credential \
         BEFORE consuming; an unregistered or different Effect can never consume this permit)"
    ))
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

// ============================== the FROZEN assented act subject (A01) ====

/// The canonicalization domain of the act subject a seat assents to.
pub const ACT_SUBJECT_TAG: &str = "bpp-act-intent-subject-v0";

/// The FROZEN member set of that subject, in order (R3-A01). This is the
/// authority the gate seat positions on, so its membership is pinned here
/// and asserted verbatim by the suite: dropping a member — the context
/// pair, say — would silently shrink what the seat assented to while every
/// consumption still passed, which is exactly how the disclosure
/// substitution survived its own test.
///
/// `assented_subject` fails CLOSED on a missing or unknown member, so a
/// projection that loses one cannot be prepared at all.
pub const ACT_SUBJECT_FIELDS: [&str; 18] = [
    "intent_id",
    "kind",
    "act_class_subject",
    "execution_kind",
    "subject_ref",
    "subject_revision",
    "requested_by_participant",
    "mandate_ref",
    "mandate_revision",
    "mandate_digest",
    "context_manifest_ref",
    "context_manifest_digest",
    "disclosure_manifest_ref",
    "disclosure_manifest_digest",
    "driver_audience",
    "budget_reservation_set_ref",
    "preconditions",
    "stable_execution_key",
];

/// Composes the assented subject from EXACTLY the frozen member set. An
/// absent optional binding is an explicit `null` here — the member is
/// always present, so "the act names no disclosure" and "the act's
/// disclosure was dropped from the subject" can never look alike.
fn assented_subject(
    members: impl IntoIterator<Item = (&'static str, Value)>,
) -> Result<Value, Problem> {
    let mut subject = Map::new();
    for (name, value) in members {
        if !ACT_SUBJECT_FIELDS.contains(&name) {
            return Err(state::internal(&format!(
                "{ACT_SUBJECT_TAG}: {name} is not a frozen act-subject member"
            )));
        }
        subject.insert(name.to_owned(), value);
    }
    for name in ACT_SUBJECT_FIELDS {
        if !subject.contains_key(name) {
            return Err(state::internal(&format!(
                "{ACT_SUBJECT_TAG}: the assented subject does not carry its frozen member {name}"
            )));
        }
    }
    Ok(Value::Object(subject))
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
    // The assented subject carries BOTH manifest bindings as exact
    // ref-AND-digest pairs (R3-A01), and its membership is the frozen
    // `ACT_SUBJECT_FIELDS` set: a subject that named the disclosure
    // without pinning its content let the first consumption substitute a
    // different manifest under the same reference (the seat assented to
    // "some disclosure called X", and the receipt then published whatever
    // the caller sent), and a subject that quietly LOST a pair would have
    // done the same while every consumption still passed. Both pairs are
    // presented again and compared, member for member, when the permit is
    // consumed.
    let subject = assented_subject([
        ("intent_id", json!(intent_id)),
        ("kind", json!(req.kind)),
        ("act_class_subject", json!(class_subject)),
        ("execution_kind", json!(req.execution_kind)),
        ("subject_ref", json!(req.subject_ref)),
        ("subject_revision", json!(req.subject_revision)),
        (
            "requested_by_participant",
            json!(caller.participant.participant_id),
        ),
        ("mandate_ref", json!(req.mandate_ref)),
        ("mandate_revision", json!(req.mandate_revision)),
        ("mandate_digest", digest_json(&req.mandate_digest)),
        ("context_manifest_ref", opt_json(&req.context_manifest_ref)),
        (
            "context_manifest_digest",
            opt_digest(&req.context_manifest_digest),
        ),
        (
            "disclosure_manifest_ref",
            opt_json(&req.disclosure_manifest_ref),
        ),
        (
            "disclosure_manifest_digest",
            opt_digest(&req.disclosure_manifest_digest),
        ),
        ("driver_audience", opt_json(&req.driver_audience)),
        ("budget_reservation_set_ref", json!(set_ref)),
        ("preconditions", preconditions.clone()),
        ("stable_execution_key", json!(key)),
    ])?;
    let subject_digest = store
        .mint_object_digest(
            &format!("society-key:{}/object:{intent_id}", caller.society_id),
            ACT_SUBJECT_TAG,
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

// ============================== the locked active Position revisions =====

/// A required seat's assent is not an exact, current PositionRevision any
/// more: the position was superseded, its subject changed, or the
/// participant's binding epoch moved (DESIGN.md §1106 — a changed binding
/// epoch invalidates pending positions rather than silently recasting
/// them).
fn positions_invalidated(detail: &str) -> Problem {
    Problem::new(
        ProblemKind::DecisionIncomplete,
        "the required seats hold no exact current Position revisions",
    )
    .with_status(409)
    .with_detail(detail.to_owned())
}

/// The EXACT active PositionRevision behind each required seat, resolved
/// and revalidated (R3-A03). Finalization used to check only that a
/// seat's head said `assent` and then synthesize a seat descriptor: the
/// decision named no position at all, so nothing tied the authority to
/// the immutable revision that carried it.
///
/// `expected_proposal_revision` is `Some` at finalization (the positions
/// must be against the exact revision being finalized) and `None` at
/// consumption, where the act head has already moved — there the
/// position's own ref and digest are the lock, and a PositionRevision is
/// immutable, so a different `proposal_revision` would be a different
/// row.
fn locked_slots(
    conn: &Connection,
    proposal_kind: &str,
    proposal_ref: &str,
    subject_digest: &Value,
    seats: &[Seat],
    expected_proposal_revision: Option<u64>,
) -> Result<Vec<Value>, Problem> {
    let mut slots = Vec::new();
    for (index, seat) in seats.iter().enumerate() {
        let position = rows::active_position(conn, proposal_kind, proposal_ref, &seat.seat_ref)
            .map_err(db_err)?
            .ok_or_else(|| {
                positions_invalidated(&format!(
                    "seat {} holds no active Position revision",
                    seat.seat_ref
                ))
            })?;
        if rows::str_of(&position, "value") != "assent" {
            return Err(positions_invalidated(&format!(
                "seat {} holds {:?}, not assent",
                seat.seat_ref,
                rows::str_of(&position, "value")
            )));
        }
        if rows::str_of(&position, "participant_ref") != seat.participant_ref {
            return Err(positions_invalidated(&format!(
                "seat {}'s active position was authored for another Participant",
                seat.seat_ref
            )));
        }
        // The position committed to the subject as it stands NOW.
        if rows::json_of(&position, "subject_digest") != *subject_digest {
            return Err(positions_invalidated(&format!(
                "seat {}'s position commits to a superseded subject digest",
                seat.seat_ref
            )));
        }
        if let Some(expected) = expected_proposal_revision {
            if rows::u64_of(&position, "proposal_revision") != expected {
                return Err(positions_invalidated(&format!(
                    "seat {}'s position was cast against another proposal revision",
                    seat.seat_ref
                )));
            }
        }
        // The binding epoch, revalidated against the participant's CURRENT
        // standing: a rebound principal invalidates its position.
        let participant = rows::get_participant(conn, &seat.participant_ref)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        if participant.state != "active" {
            return Err(positions_invalidated(&format!(
                "seat {}'s Participant holds no active Standing",
                seat.seat_ref
            )));
        }
        if rows::u64_of(&position, "participant_binding_epoch") != participant.binding_epoch {
            return Err(positions_invalidated(&format!(
                "seat {}'s position was cast at binding epoch {}, and the Participant is now at \
                 {}: a changed binding epoch invalidates the position instead of recasting it",
                seat.seat_ref,
                rows::u64_of(&position, "participant_binding_epoch"),
                participant.binding_epoch
            )));
        }
        slots.push(json!({
            "slot_ref": format!("slot-{}", index + 1),
            "seat_ref": seat.seat_ref,
            "seat_kind": seat.kind,
            "participant_ref": seat.participant_ref,
            "actor_ref": rows::str_of(&position, "actor_ref"),
            "position_ref": rows::str_of(&position, "position_id"),
            "position_revision": rows::u64_of(&position, "revision"),
            "position_digest": rows::json_of(&position, "digest"),
            "participant_binding_epoch": participant.binding_epoch,
            "value": rows::str_of(&position, "value"),
        }));
    }
    Ok(slots)
}

/// The act's `authorization_slot_snapshot_digest`: byom's own keyed
/// commitment over the LOCKED slots — position refs and digests included —
/// so a consumption can recompute it from current state and refuse if the
/// slots it would execute under are not the slots that were authorized.
fn slot_snapshot_digest(
    conn: &Connection,
    society_id: &str,
    intent_id: &str,
    subject_digest: &Value,
    slots: &[Value],
    seats: &Value,
) -> Result<DigestRef, Problem> {
    conn_record_digest(
        conn,
        society_id,
        &format!("{intent_id}-slot-snapshot"),
        "bpp-act-slot-snapshot-v0",
        &json!({
            "intent_ref": intent_id,
            "subject_digest": subject_digest,
            "seats": seats,
            "slot_snapshot": slots,
        }),
    )
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
        // The EXACT active Position revisions this finalization locks
        // (R3-A03): each resolved, its binding epoch revalidated, and its
        // ref and digest committed — never a synthesized seat descriptor.
        let revision_positioned = rows::u64_of(&intent, "revision");
        let slots = locked_slots(
            conn,
            "act_intent",
            &req_c.intent_id,
            &subject,
            &seats,
            Some(revision_positioned),
        )?;
        // The decision receives the exact locked positions — ref, immutable
        // revision AND record digest (R3-A03). A decision carrying only
        // references named which rows existed; the digests are what tie the
        // authority to the immutable revisions that carried it, and they are
        // covered by the decision's own record digest.
        let positions: Vec<gov_decision::DecisionPosition> = slots
            .iter()
            .map(|s| gov_decision::DecisionPosition {
                position_ref: s["position_ref"].as_str().unwrap_or_default().to_owned(),
                position_revision: s["position_revision"].as_u64().unwrap_or_default(),
                position_digest: s["position_digest"].clone(),
            })
            .collect();
        let decision_seats: Vec<gov_decision::DecisionSeat> = slots
            .iter()
            .map(|s| gov_decision::DecisionSeat {
                seat_ref: s["seat_ref"].as_str().unwrap_or_default().to_owned(),
                participant_ref: s["participant_ref"].as_str().unwrap_or_default().to_owned(),
                // The actor that AUTHORED the locked position, read off the
                // PositionRevision — not a constant.
                actor_ref: s["actor_ref"].as_str().unwrap_or_default().to_owned(),
                participant_binding_epoch: s["participant_binding_epoch"]
                    .as_u64()
                    .unwrap_or_default(),
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
            &positions,
            "act_intent_finalize",
            crate::gov_ops::ACTOR_GOVERNANCE,
            now,
        )?;
        // The exact active slot snapshot this finalization locks: the
        // position refs AND digests, so the consumption can prove the
        // authorized slots are still the slots it executes under.
        let snapshot_digest = slot_snapshot_digest(
            conn,
            &society,
            &req_c.intent_id,
            &subject,
            &slots,
            &seats_json(&seats),
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
                // The locked slots, published: the exact Position revision
                // each seat holds, so the caller can see what was locked
                // instead of trusting a derived decision id (R3-A03).
                "authorization_slot_snapshot": slots,
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

/// One HOST manifest binding the assented act subject pins, compared at
/// consumption member for member (R3-A01). `what` is the manifest's family
/// name, so the request members it reports are exactly the ones the closed
/// shape carries: `{what}_manifest_ref` and `{what}_digest`.
///
/// An act that pins no such manifest commits the empty ref and a `null`
/// digest, so a consumption that presents one anyway is refused by the same
/// comparison — the pair is never "optional if you leave it out".
fn compare_manifest_binding(
    what: &str,
    presented_ref: &str,
    presented_digest: &Value,
    committed_ref: &str,
    committed_digest: &Value,
) -> Result<(), Problem> {
    if presented_ref != committed_ref {
        return Err(state::stale_binding(&format!(
            "{what}_manifest_ref {presented_ref:?} is not the {what} manifest this act was \
             authorized for ({committed_ref:?}): the assented subject pins the exact manifest, \
             and a consumption never substitutes another (R3-A01)"
        )));
    }
    if presented_digest != committed_digest {
        return Err(state::stale_binding(&format!(
            "{what}_digest does not pin the exact {what} manifest the assented subject committed \
             to: the same reference carrying different content is precisely the substitution the \
             pair exists to refuse (R3-A01)"
        )));
    }
    Ok(())
}

/// `execution_permit_consume` (runtime, update; R34). The one-shot
/// consumption protocol of §13.1 steps 4-6: byom atomically rechecks
/// charter, standing, Mandate, decisions, the locked Position revisions,
/// dependencies, ceilings, expiry and BOTH fences, inserts the MandateUse
/// once, and returns ONE immutable ExecutionConsumptionReceipt.
///
/// Five things the consumption is bound to, in order:
///
/// 1. the exact prepared ACT — the permit channel token (R34);
/// 2. the exact prepared host EFFECT — the registration credential
///    (R3-A02): the permit consumes for one attested Effect, never for a
///    ref and digest a caller merely names;
/// 3. the exact assented CONTEXT — ref and digest compared with the
///    committed act subject, and the committed pair published on the
///    consumption event (R3-A01);
/// 4. the exact assented DISCLOSURE — ref and digest compared the same
///    way, and the receipt renders the COMMITTED value (R3-A01);
/// 5. the exact locked SLOTS — the slot snapshot recomputed from the
///    current active Position revisions (R3-A03).
///
/// Repeating the identical semantic request under the same key returns the
/// same receipt; a request that changed ANY substantive member conflicts
/// with the frozen digest byom STORED when it consumed (R3-A04); a
/// different key cannot consume the spent decision. byom's own committed
/// digests are recomputed here, never request members (A8).
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
    // ... and the permit is bound to the exact prepared host EFFECT
    // (R3-A02), verified BEFORE any consumption is attempted: the
    // registration credential authenticates {intent, one-shot key, effect
    // ref, effect digest} as one tuple, so the effect ref and digest are
    // no longer values byom merely stores because a caller sent them.
    verify_effect_registration(store, req)?;
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
            // Only the IDENTICAL semantic request replays (R3-A04). The
            // comparison is one frozen digest over EVERY substantive member,
            // and the value it is compared against is the one byom STORED
            // when it consumed — not a second recomputation. A recomputed
            // "committed" side can only ever prove that today's rebuild
            // equals today's rebuild; the stored digest is what makes the
            // consumption's own commitment load-bearing. A hand-listed
            // member set is exactly how the disclosure pair, the budget set
            // and the episode binding fell out of the check.
            let presented = presented_semantic_request(&req_c);
            let committed = committed_semantic_request(&stored, &intent);
            let differing = changed_members(&presented, &committed);
            let presented_digest =
                semantic_request_digest(conn, &society_c, &req_c.intent_ref, &presented)?;
            let stored_digest = rows::json_of(&stored, "semantic_request_digest");
            if stored_digest.is_null() {
                return Err(state::internal(
                    "the retained receipt carries no frozen semantic-request digest: a \
                     consumption commits to the exact request it honored (R3-A04)",
                ));
            }
            if !presented_digest.same_ref_json(&stored_digest) {
                // A byte-identical request whose digest still disagrees with
                // the STORED one means the retained commitment is not the one
                // this consumption wrote — an edited receipt row, not a
                // changed request. It is refused for the same reason: the
                // replay answers from byom's own commitment or not at all. A
                // recomputed "committed" side cannot tell these two apart,
                // because it never reads the commitment.
                let why = if differing.is_empty() {
                    "no presented member differs from the retained receipt, so the STORED frozen \
                     digest is not the one this consumption committed: an edited commitment does \
                     not serve a replay"
                        .to_owned()
                } else {
                    format!(
                        "these members differ from the consumption byom committed: {differing:?}"
                    )
                };
                return Err(Problem::new(
                    ProblemKind::IdempotencyMismatch,
                    "same one-shot key, different canonical request",
                )
                .with_status(409)
                .with_detail(format!(
                    "only the identical semantic request replays to the retained receipt (§13.1 \
                     step 6); {why}"
                )));
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
        // The intent record and act subject digests are byom's OWN, taken
        // from this committed row and published on the receipt (A8): there
        // is nothing to compare, because there is no caller echo to
        // compare against.
        //
        // BOTH host manifest bindings are the caller's, and BOTH are
        // compared, ref and digest, against the pairs the seats assented to
        // (R3-A01). The first consumption used to accept any disclosure pair
        // at all and copy it onto the receipt — so the authorized disclosure
        // and the receipted one could differ, with nothing in the record
        // showing it — and the CONTEXT pair was never presented at all, so
        // the act could execute under a context no seat ever saw.
        //
        // Both the comparison and the later RENDERING read this one value,
        // whose only constructor is byom's committed row: the confirmation's
        // mutation replaced the rendering source with the caller's echoes and
        // nothing noticed, because an echo that reached this point had already
        // been proved equal. There is no longer an argument to swap.
        // ONE value, compared AND rendered. The comparison below proves the
        // caller echoed exactly these, which is what makes rendering them
        // safe: with the two bound together a rendering that drifts from the
        // compared value is not expressible, and a comparison that drifts from
        // the rendered value refuses every substitution probe in
        // `a_substituted_disclosure_cannot_consume_the_permit`.
        let committed = CommittedActBinding::from_committed_intent(&intent);
        let rendered_context_ref = committed.context_manifest_ref().to_owned();
        let rendered_context_digest = committed.context_digest().clone();
        let rendered_disclosure_digest = committed.disclosure_digest().clone();
        compare_manifest_binding(
            "context",
            &req_c.context_manifest_ref.clone().unwrap_or_default(),
            &opt_digest(&req_c.context_digest),
            &rendered_context_ref,
            &rendered_context_digest,
        )?;
        compare_manifest_binding(
            "disclosure",
            &req_c.disclosure_manifest_ref.clone().unwrap_or_default(),
            &opt_digest(&req_c.disclosure_digest),
            committed.disclosure_manifest_ref(),
            &rendered_disclosure_digest,
        )?;
        // And the host's own `host_effect_digest` is now DERIVED here, from
        // the very members just compared plus this row's one-shot key, rather
        // than stored because a caller authenticated a tuple containing it
        // (R3-L01, D-R3-3). The registration credential above still proves WHO
        // sent it; this proves WHAT it is the digest of.
        verify_host_effect_binding(&req_c, &committed)?;
        // The slots the decision locked are still the slots this
        // consumption would execute under: the snapshot is recomputed from
        // the CURRENT active Position revisions and compared with the
        // committed digest (R3-A03).
        let seats = seats_from_json(&rows::json_of(&intent, "required_seat_refs"));
        let current_slots = locked_slots(
            conn,
            "act_intent",
            &req_c.intent_ref,
            &rows::json_of(&intent, "subject_digest"),
            &seats,
            None,
        )?;
        let current_snapshot = slot_snapshot_digest(
            conn,
            &society_c,
            &req_c.intent_ref,
            &rows::json_of(&intent, "subject_digest"),
            &current_slots,
            &seats_json(&seats),
        )?;
        if !current_snapshot.same_ref_json(&rows::json_of(
            &intent,
            "authorization_slot_snapshot_digest",
        )) {
            return Err(state::stale_binding(
                "the active Position revisions are no longer the ones this act's authorization \
                 locked: a superseded position or a moved binding epoch invalidates the permit \
                 rather than silently executing under it",
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
        // Yields the digest of the COMMITTED ByomEpisodeBinding, which is
        // what the receipt publishes: byom names its own committed record
        // and no longer asks the caller to echo it (A8 — the echo was a
        // keyed value the host could never verify, and byom compared its
        // own digest against itself).
        let episode_fence: Option<Value> = match &req_c.episode_ref {
            Some(episode_ref) => {
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
                Some(rows::json_of(&binding, "digest"))
            }
            None => {
                if rows::str_of(&intent, "execution_kind") == "external_effect" {
                    return Err(state::stale_binding(
                        "an external-effect act binds the exact Episode and BOTH fences; the \
                         episode reference is not optional here (family contract L21)",
                    ));
                }
                None
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
        // byom's OWN record commitment: keyed, per-object, erasable, and
        // demanded from nobody (PROFILE.md §6.2 converse half).
        let use_digest = conn_record_digest(
            conn,
            &society_c,
            &mandate_use_id,
            "bpp-mandate-use-v0",
            &use_record,
        )?;
        // The CROSS-BOUNDARY pin the receipt publishes instead (A8): the
        // consumer holds all four members and re-derives this value.
        let use_binding_digest = mandate_use_binding_digest(
            &mandate_use_id,
            &req_c.intent_ref,
            &req_c.stable_execution_key,
            &issued_at,
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
        // The frozen semantic request this consumption commits to
        // (R3-A04). It is STORED on the receipt — that stored value is what
        // a later presentation of the same one-shot key is compared against
        // — and published on the consumption event, so the commitment is
        // durable and visible in byom's own ledger.
        let request_digest = semantic_request_digest(
            conn,
            &society_c,
            &req_c.intent_ref,
            &presented_semantic_request(&req_c),
        )?;

        // -- the ONE immutable ExecutionConsumptionReceipt ---------------
        // Every §13.1 member is composed ONCE, here. The published receipt
        // and the digest's preimage are the same fragment of this row
        // (`receipt_fragment`), so a member that is rendered is digested
        // and a member that is digested is rendered — the two can never
        // drift apart, and a rendered `null` is impossible by shape.
        let mut receipt_row = obj_pairs([
            ("receipt_id", json!(receipt_id)),
            ("society_id", json!(society_c)),
            ("byom_endpoint_ref", json!(byom_endpoint_ref)),
            ("endpoint_incarnation", json!(incarnation)),
            ("recovery_epoch", json!(recovery_epoch)),
            ("intent_ref", json!(req_c.intent_ref)),
            ("intent_digest", rows::json_of(&intent, "intent_digest")),
            ("mandate_use_ref", json!(mandate_use_id)),
            ("mandate_use_digest", digest_json(&use_binding_digest)),
            ("stable_execution_key", json!(req_c.stable_execution_key)),
            // The COMMITTED authority subject and the COMMITTED disclosure
            // (R3-A01): every authority member the receipt publishes is
            // read out of byom's own row, never copied from the request.
            ("subject_digest", rows::json_of(&intent, "subject_digest")),
            ("disclosure_digest", rendered_disclosure_digest.clone()),
            ("driver_audience", json!(req_c.driver_audience)),
            ("participant_ref", json!(participant_ref)),
            ("episode_ref", opt_json(&req_c.episode_ref)),
            (
                "episode_fence_digest",
                episode_fence.clone().unwrap_or(Value::Null),
            ),
            (
                "budget_reservation_set_ref",
                json!(req_c.budget_reservation_set_ref),
            ),
            ("host_effect_ref", json!(req_c.host_effect_ref)),
            ("host_effect_digest", digest_json(&req_c.host_effect_digest)),
            // The two host-owned members of the L01 binding fragment
            // (R3-A04, second wave). They are retained so a refusal can NAME
            // which of them changed; the comparison itself is against the
            // frozen digest below, which already covers them.
            (
                "host_effect_external_idempotency_key",
                json!(req_c.host_effect_external_idempotency_key),
            ),
            (
                "host_effect_request_byte_digest",
                digest_json(&req_c.host_effect_request_byte_digest),
            ),
            ("byom_fence_epoch", json!(req_c.byom_fence_epoch)),
            ("host_fence_epoch", json!(req_c.host_fence_epoch)),
            // The FROZEN semantic request this receipt was minted for
            // (R3-A04): stored beside the receipt, never a receipt member —
            // the receipt publishes the consumption, not the summary of the
            // request that asked for it — and compared on every replay.
            ("semantic_request_digest", digest_json(&request_digest)),
            ("issued_at", json!(issued_at)),
            ("expires_at", json!(expires_at)),
            ("max_uses", json!(1)),
            ("digest", Value::Null),
        ]);
        let receipt_digest = receipt_binding_digest(&receipt_fragment(&receipt_row))?;
        receipt_row.insert("digest".to_owned(), digest_json(&receipt_digest));
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
                       "episode_bound": episode_fence.is_some(),
                       "host_effect_ref": req_c.host_effect_ref,
                       // The COMMITTED context this act was authorized
                       // under, rendered from byom's own act (R3-A01). The
                       // receipt shape is frozen with the consuming host, so
                       // byom's ledger — not the receipt — is where the
                       // committed context binding is published.
                       "context_manifest_ref": rendered_context_ref,
                       "context_digest": rendered_context_digest,
                       "semantic_request_digest": digest_json(&request_digest),
                       "receipt_ref": receipt_id}),
            )],
        })
    })
}

// ==================== the host-effect registration credential (A02) ======
//
// A permit is consumable only for the ONE host Effect the trusted host
// effect service durably created for this act (§13.1 step 3). byom cannot
// read kovee's store, so what it verifies is an AUTHENTICATED registration
// of that Effect: a 32-byte authenticator over the frozen effect tuple,
// keyed by the permit channel credential byomd itself minted and published
// `0600` for this exact ActIntent.
//
// What it buys, precisely:
//
// - the effect ref and digest stop being values byom stores because a
//   caller sent them: they are covered by an authenticator byom recomputes,
//   so a consumption for a DIFFERENT effect than the registered one, or for
//   an effect no one ever registered, is refused before any state is read;
// - the tuple includes the one-shot key and the intent, so a credential
//   minted for one act's effect is useless for another;
// - only a holder of that act's permit token can mint it — the same trust
//   anchor the channel already rests on, and the token is unforgeable
//   client-side (it is an HMAC under byom's own store key).
//
// What it does NOT claim: byom does not see kovee's `model_effects` row, so
// this is the host's authenticated attestation of the Effect it created,
// not proof of a row in another database. A byom-side registration RECORD
// (its own table and operation, resolved like a GovernanceDecision) is the
// stronger form and is recorded as a follow-on: it needs a new operation
// and a new table, both outside this bundle's paths.

/// The canonicalization domain of the host-effect registration tuple.
pub const HOST_EFFECT_REGISTRATION_TAG: &str = "bpp-host-effect-registration-v0";

/// Its frozen member set, in order. Every member is one the host already
/// holds, so the host derives the identical authenticator.
pub const HOST_EFFECT_REGISTRATION_FIELDS: [&str; 4] = [
    "intent_ref",
    "stable_execution_key",
    "host_effect_ref",
    "host_effect_digest",
];

/// The authenticator byom expects for one exact registered host Effect:
/// `HMAC-SHA-256(permit channel token, $domain-tagged canonical tuple)`.
fn host_effect_registration_credential(
    permit_token: &str,
    intent_ref: &str,
    stable_execution_key: &str,
    host_effect_ref: &str,
    host_effect_digest: &DigestRef,
) -> Result<String, Problem> {
    let tuple = obj_pairs([
        ("intent_ref", json!(intent_ref)),
        ("stable_execution_key", json!(stable_execution_key)),
        ("host_effect_ref", json!(host_effect_ref)),
        ("host_effect_digest", digest_json(host_effect_digest)),
    ]);
    let bytes = tagged_canonical(HOST_EFFECT_REGISTRATION_TAG, &Value::Object(tuple))
        .map_err(|e| state::internal(&e.to_string()))?;
    Ok(hex(&hmac_sha256(permit_token.trim().as_bytes(), &bytes)))
}

/// Verifies the presented registration in constant time, BEFORE any
/// consumption state is read (R3-A02).
fn verify_effect_registration(
    store: &Store,
    req: &ops::ExecutionPermitConsumeRequest,
) -> Result<(), Problem> {
    let token = runtime_token(store, RuntimeChannel::Permit, &req.intent_ref)
        .ok_or_else(|| state::internal("runtime channel key unavailable"))?;
    let expected = host_effect_registration_credential(
        &token,
        &req.intent_ref,
        &req.stable_execution_key,
        &req.host_effect_ref,
        &req.host_effect_digest,
    )?;
    let a = expected.as_bytes();
    let b = req.host_effect_credential.as_bytes();
    let same = a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0;
    if same {
        Ok(())
    } else {
        Err(unregistered_effect(&format!(
            "host_effect_credential does not register host effect {:?} with digest {} under this \
             act's one-shot key",
            req.host_effect_ref, req.host_effect_digest.value_hex
        )))
    }
}

// ============ the host's frozen binding fragment (L01, D-R3-3) ===========
//
// What `verify_effect_registration` above proves: the addressed host, and only
// it, composed this {intent, key, effect ref, effect digest} tuple. What it
// does NOT prove: that the effect digest is the digest OF anything. It
// authenticates a tuple CONTAINING the supplied value, so byom stored an
// assertion.
//
// D-R3-3 fixes the class of defect in both directions. byom's own owner-
// recomputed digests are never request members (done: A8's converse). And a
// PEER-owned digest byom must verify travels as a frozen `portable_public`
// fragment whose members byom holds — which is exactly the shape kovee already
// consumes for byom's `bpp-parent-budget-fragment-v0` parent budget. This is
// the converse instance of it.
//
// byom holds SIX of the nine members in its own committed ActIntent, takes the
// effect reference from the request, and is handed the two remaining
// kovee-owned members explicitly. It then re-derives the digest. A digest that
// does not re-derive is refused before any consumption state changes.
//
// The six were originally passed to the deriver as loose `&str`/`&Value`
// arguments read at the call site, and the confirmation's mutation simply
// replaced five of them with the caller's echoes — which every act test
// survived, because the echoes are compared for equality a few lines earlier
// and so can never differ at this point. The claim "rebuilt from its own
// committed act" was therefore unfalsifiable from the wire.
//
// [`CommittedActBinding`] closes that: its fields are private, its ONLY
// constructor takes the committed `act_intents` row, and the deriver takes
// nothing else. There is no call-site argument left to swap for a request
// echo, and `the_committed_act_binding_reads_only_byoms_own_row` pins which
// column each member comes from.

/// kovee's `$domain` for the host-effect binding fragment. It is the HOST's
/// domain, exactly as `bpp-parent-budget-fragment-v0` is byom's: the verifier
/// re-derives the producer's value and never mints one of its own.
pub const HOST_EFFECT_BINDING_TAG: &str = "kovee-host-effect-binding-v1";

/// Its frozen member set, in the host's published order.
pub const HOST_EFFECT_BINDING_FIELDS: [&str; 9] = [
    "context_digest",
    "context_manifest_ref",
    "disclosure_digest",
    "disclosure_manifest_ref",
    "external_idempotency_key",
    "final_provider_request_typed_byte_digest",
    "host_effect_ref",
    "intent_ref",
    "stable_execution_key",
];

/// The SIX fragment members byom holds itself, read out of one committed
/// `act_intents` row and out of nothing else (R3-L01).
///
/// The fields are private and [`CommittedActBinding::from_committed_intent`] is
/// the only way to make one, so there is no call site at which a caller's echo
/// can be substituted for byom's own committed value. That substitution — five
/// of these six replaced by request members — is the mutation every act test
/// survived, because the echoes are compared for equality earlier and so are
/// indistinguishable from the row by the time the deriver runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommittedActBinding {
    intent_ref: String,
    stable_execution_key: String,
    context_manifest_ref: String,
    context_digest: Value,
    disclosure_manifest_ref: String,
    disclosure_digest: Value,
}

impl CommittedActBinding {
    /// The one constructor: byom's own committed ActIntent row. The column
    /// each member comes from is pinned by
    /// `the_committed_act_binding_reads_only_byoms_own_row`.
    pub fn from_committed_intent(intent: &Map<String, Value>) -> CommittedActBinding {
        CommittedActBinding {
            // `intent_id`, not the request's `intent_ref`: the row is the
            // authority on its own identity even though it was looked up by
            // that key. The confirmation counted this as the ninth member that
            // still came through the request.
            intent_ref: rows::str_of(intent, "intent_id").to_owned(),
            stable_execution_key: rows::str_of(intent, "stable_execution_key").to_owned(),
            context_manifest_ref: rows::str_of(intent, "context_manifest_ref").to_owned(),
            context_digest: rows::json_of(intent, "context_manifest_digest"),
            disclosure_manifest_ref: rows::str_of(intent, "disclosure_manifest_ref").to_owned(),
            disclosure_digest: rows::json_of(intent, "disclosure_manifest_digest"),
        }
    }

    /// The committed one-shot key.
    pub fn stable_execution_key(&self) -> &str {
        &self.stable_execution_key
    }

    /// The committed act's own identity.
    pub fn intent_ref(&self) -> &str {
        &self.intent_ref
    }

    /// The CONTEXT pair the seats assented to — compared against the
    /// caller's echo, and RENDERED on the consumption event (R3-A01).
    pub fn context_manifest_ref(&self) -> &str {
        &self.context_manifest_ref
    }

    pub fn context_digest(&self) -> &Value {
        &self.context_digest
    }

    /// The DISCLOSURE pair the seats assented to — compared against the
    /// caller's echo, and RENDERED on the receipt (R3-A01).
    pub fn disclosure_manifest_ref(&self) -> &str {
        &self.disclosure_manifest_ref
    }

    pub fn disclosure_digest(&self) -> &Value {
        &self.disclosure_digest
    }
}

/// The frozen member set, exactly: a member added on one side and not the
/// other would silently change a preimage the other side already verified, so
/// neither a widened nor a narrowed fragment may be digested at all.
///
/// Extracted so it is REACHABLE (R3-L01 mutation gap). As an inline `if` inside
/// the deriver it guarded an input the deriver itself composes, so deleting it
/// changed no observable behaviour and every act test survived. Called out
/// here, `the_frozen_binding_member_set_is_the_only_digestible_shape` fails
/// when its body goes.
pub fn check_frozen_binding_members(fragment: &Value) -> Result<(), Problem> {
    let mut emitted: Vec<&str> = fragment
        .as_object()
        .map(|o| o.keys().map(String::as_str).collect())
        .unwrap_or_default();
    let mut frozen = HOST_EFFECT_BINDING_FIELDS.to_vec();
    emitted.sort_unstable();
    frozen.sort_unstable();
    if emitted != frozen {
        return Err(state::internal(&format!(
            "the rebuilt host-effect binding fragment is not the frozen member set: \
             {HOST_EFFECT_BINDING_TAG} is exactly {frozen:?}, got {emitted:?}"
        )));
    }
    Ok(())
}

/// Rebuilds the host's fragment from byom's OWN committed act plus the exact
/// host-owned members presented, and re-derives its `portable_public` digest.
fn host_effect_binding_digest(
    req: &ops::ExecutionPermitConsumeRequest,
    committed: &CommittedActBinding,
) -> Result<(DigestRef, Value), Problem> {
    let fragment = json!({
        "context_digest": committed.context_digest,
        "context_manifest_ref": committed.context_manifest_ref,
        "disclosure_digest": committed.disclosure_digest,
        "disclosure_manifest_ref": committed.disclosure_manifest_ref,
        "external_idempotency_key": req.host_effect_external_idempotency_key,
        "final_provider_request_typed_byte_digest":
            req.host_effect_request_byte_digest.value_hex,
        "host_effect_ref": req.host_effect_ref,
        "intent_ref": committed.intent_ref,
        "stable_execution_key": committed.stable_execution_key,
    });
    check_frozen_binding_members(&fragment)?;
    let bytes = tagged_canonical(HOST_EFFECT_BINDING_TAG, &fragment)
        .map_err(|e| state::internal(&e.to_string()))?;
    Ok((DigestRef::portable_public(sha256_hex(&bytes)), fragment))
}

/// Verifies the presented `host_effect_digest` DERIVES from members byom
/// holds (R3-L01, D-R3-3), inside the consumption transaction where the
/// committed ActIntent is in hand.
pub fn verify_host_effect_binding(
    req: &ops::ExecutionPermitConsumeRequest,
    committed: &CommittedActBinding,
) -> Result<(), Problem> {
    // The idempotency key is not a free member: the host derives it from the
    // one-shot key byom minted and the first 16 hex of the request-byte
    // digest, so byom re-derives it too. Without this the only member a
    // caller could still choose freely would be one byom never checks.
    let expected_key = format!(
        "kovee-model-{}-{}",
        committed.stable_execution_key,
        req.host_effect_request_byte_digest
            .value_hex
            .chars()
            .take(16)
            .collect::<String>()
    );
    if req.host_effect_external_idempotency_key != expected_key {
        return Err(unregistered_effect(&format!(
            "host_effect_external_idempotency_key must be {expected_key:?} for this act's \
             one-shot key and request bytes; {:?} was presented",
            req.host_effect_external_idempotency_key
        )));
    }
    let (expected, fragment) = host_effect_binding_digest(req, committed)?;
    if expected.same_ref(&req.host_effect_digest) {
        Ok(())
    } else {
        Err(unregistered_effect(&format!(
            "host_effect_digest does not re-derive from the frozen \
             {HOST_EFFECT_BINDING_TAG} fragment byom rebuilt from its own committed act: \
             expected {} over {fragment}, got {}",
            expected.value_hex, req.host_effect_digest.value_hex
        )))
    }
}

// ======================= the frozen semantic request (A04) ===============

/// The canonicalization domain of the semantic consumption request.
pub const CONSUME_REQUEST_TAG: &str = "bpp-execution-permit-consume-request-v0";

/// EVERY substantive member of a consumption, frozen (R3-A04). The replay
/// comparison used to be a hand-listed subset — it omitted the intent
/// digest, the disclosure pair, the budget set, the episode ref and the
/// episode fence — so a changed request could replay the old receipt.
///
/// A hand-listed set is also how a LATER wave reopened the hole: R3-L01 added
/// `host_effect_external_idempotency_key` and
/// `host_effect_request_byte_digest` to the wire and nobody extended this
/// list, so changing either on a consumed request replayed the old receipt
/// again. The list is therefore closed the other way round too — every wire
/// member of `ExecutionPermitConsumeRequest` is either named here or named in
/// [`CONSUME_REQUEST_TRANSPORT`], and
/// `the_frozen_semantic_set_covers_every_wire_member` fails the build's tests
/// if a new member joins neither.
///
/// Anything in `CONSUME_REQUEST_TRANSPORT` is transport, not semantics:
/// `version`/`op`/`meta` name the attempt, and `host_effect_credential` is a
/// deterministic function of four members that are already here (so a changed
/// effect cannot hide behind it).
pub const CONSUME_REQUEST_FIELDS: [&str; 15] = [
    "stable_execution_key",
    "intent_ref",
    "host_effect_ref",
    "host_effect_digest",
    "host_effect_external_idempotency_key",
    "host_effect_request_byte_digest",
    "context_manifest_ref",
    "context_digest",
    "disclosure_manifest_ref",
    "disclosure_digest",
    "driver_audience",
    "budget_reservation_set_ref",
    "episode_ref",
    "byom_fence_epoch",
    "host_fence_epoch",
];

/// The wire members that are deliberately NOT semantic. Named, not implied:
/// the pair of lists is what makes "every substantive member" checkable
/// instead of asserted (R3-A04, second wave).
pub const CONSUME_REQUEST_TRANSPORT: [&str; 4] =
    ["version", "op", "meta", "host_effect_credential"];

fn insert_present(fragment: &mut Map<String, Value>, name: &str, value: Value) {
    if !value.is_null() && value != json!("") {
        fragment.insert(name.to_owned(), value);
    }
}

/// The semantic request AS PRESENTED. An absent optional member is ABSENT,
/// never null: two different requests can never share a preimage.
fn presented_semantic_request(req: &ops::ExecutionPermitConsumeRequest) -> Map<String, Value> {
    let mut fragment = Map::new();
    insert_present(
        &mut fragment,
        "stable_execution_key",
        json!(req.stable_execution_key),
    );
    insert_present(&mut fragment, "intent_ref", json!(req.intent_ref));
    insert_present(&mut fragment, "host_effect_ref", json!(req.host_effect_ref));
    insert_present(
        &mut fragment,
        "host_effect_digest",
        digest_json(&req.host_effect_digest),
    );
    // The two host-owned members of the L01 binding fragment. They are the
    // only inputs to `host_effect_digest` a caller still chooses, so leaving
    // them out let a consumed request change what it was the digest OF and
    // still replay (R3-A04, second wave).
    insert_present(
        &mut fragment,
        "host_effect_external_idempotency_key",
        json!(req.host_effect_external_idempotency_key),
    );
    insert_present(
        &mut fragment,
        "host_effect_request_byte_digest",
        digest_json(&req.host_effect_request_byte_digest),
    );
    insert_present(
        &mut fragment,
        "context_manifest_ref",
        opt_json(&req.context_manifest_ref),
    );
    insert_present(
        &mut fragment,
        "context_digest",
        opt_digest(&req.context_digest),
    );
    insert_present(
        &mut fragment,
        "disclosure_manifest_ref",
        opt_json(&req.disclosure_manifest_ref),
    );
    insert_present(
        &mut fragment,
        "disclosure_digest",
        opt_digest(&req.disclosure_digest),
    );
    insert_present(&mut fragment, "driver_audience", json!(req.driver_audience));
    insert_present(
        &mut fragment,
        "budget_reservation_set_ref",
        json!(req.budget_reservation_set_ref),
    );
    insert_present(&mut fragment, "episode_ref", opt_json(&req.episode_ref));
    fragment.insert("byom_fence_epoch".to_owned(), json!(req.byom_fence_epoch));
    fragment.insert("host_fence_epoch".to_owned(), json!(req.host_fence_epoch));
    fragment
}

/// The same fragment REBUILT FROM COMMITTED STATE: the retained receipt
/// row carries every member verbatim except the two manifest bindings,
/// which are the ActIntent's own committed pairs (the consumption that
/// minted the receipt had to present exactly those, or it never reached the
/// receipt). Nothing is stored twice, so the summary can never drift from
/// the rows it summarizes.
///
/// This rebuild NAMES the members that differ in a refusal; it is not the
/// comparison itself. The comparison is against the frozen digest byom
/// STORED when it consumed (R3-A04).
fn committed_semantic_request(
    receipt: &Map<String, Value>,
    intent: &Map<String, Value>,
) -> Map<String, Value> {
    let mut fragment = Map::new();
    for name in [
        "stable_execution_key",
        "intent_ref",
        "host_effect_ref",
        "driver_audience",
        "budget_reservation_set_ref",
        "episode_ref",
    ] {
        insert_present(&mut fragment, name, json!(rows::str_of(receipt, name)));
    }
    insert_present(
        &mut fragment,
        "host_effect_external_idempotency_key",
        json!(rows::str_of(
            receipt,
            "host_effect_external_idempotency_key"
        )),
    );
    for name in [
        "host_effect_digest",
        "host_effect_request_byte_digest",
        "disclosure_digest",
    ] {
        insert_present(&mut fragment, name, rows::json_of(receipt, name));
    }
    insert_present(
        &mut fragment,
        "context_manifest_ref",
        json!(rows::str_of(intent, "context_manifest_ref")),
    );
    insert_present(
        &mut fragment,
        "context_digest",
        rows::json_of(intent, "context_manifest_digest"),
    );
    insert_present(
        &mut fragment,
        "disclosure_manifest_ref",
        json!(rows::str_of(intent, "disclosure_manifest_ref")),
    );
    fragment.insert(
        "byom_fence_epoch".to_owned(),
        json!(rows::u64_of(receipt, "byom_fence_epoch")),
    );
    fragment.insert(
        "host_fence_epoch".to_owned(),
        json!(rows::u64_of(receipt, "host_fence_epoch")),
    );
    fragment
}

/// The members that differ between two semantic requests — named in the
/// refusal, so a mismatch is diagnosable instead of mysterious.
fn changed_members(presented: &Map<String, Value>, committed: &Map<String, Value>) -> Vec<String> {
    CONSUME_REQUEST_FIELDS
        .iter()
        .filter(|name| presented.get(**name) != committed.get(**name))
        .map(|name| (*name).to_owned())
        .collect()
}

/// byom's OWN keyed commitment to a semantic request: per-object,
/// erasable, demanded from nobody (PROFILE.md §6.2 converse half). It is
/// never a wire member — the receipt publishes the consumption, not this
/// summary of the request that asked for it.
fn semantic_request_digest(
    conn: &Connection,
    society_id: &str,
    intent_ref: &str,
    fragment: &Map<String, Value>,
) -> Result<DigestRef, Problem> {
    for name in fragment.keys() {
        if !CONSUME_REQUEST_FIELDS.contains(&name.as_str()) {
            return Err(state::internal(&format!(
                "{CONSUME_REQUEST_TAG}: {name} is not a frozen semantic-request member"
            )));
        }
    }
    conn_record_digest(
        conn,
        society_id,
        &format!("{intent_ref}-consume-request"),
        CONSUME_REQUEST_TAG,
        &Value::Object(fragment.clone()),
    )
}

// ============================ the CROSS-BOUNDARY receipt digests (A8) ====
//
// The receipt is the one artifact the consumer must hold before egress, so
// every digest ON it has to be one the consumer can actually check
// (PROFILE.md §6.2 cross-boundary class rule, amendment §A8):
//
// - `intent_digest`, `subject_digest` and `episode_fence_digest` stay
//   `local_erasure_safe` AND stop being request members (A8's converse
//   half, R3-L01): each is byom's own committed record digest, published
//   here so the consumer can carry it in its own audit, recomputed by byom
//   rather than echoed by the caller. `subject_digest` is an authority
//   subject, which §6.2 requires to be per-object keyed and forbids from
//   ever taking a public hash.
// - `disclosure_digest` is the HOST's own object, so it is
//   `portable_public` (A8's demanded half): the host presents it, byom
//   compares it against the pair the seats assented to, and the receipt
//   publishes the COMMITTED value — never the presented one.
// - `mandate_use_digest` and `digest` name records the consumer never
//   supplied and holds no key for. A keyed class there would be an opaque
//   blob it could only echo, so both are `portable_public` over a FROZEN
//   cross-boundary fragment whose every member is published on the receipt
//   itself — exactly the `resource_allocation_digest` construction (gap
//   note G48). byom's own keyed `mandate_uses.digest` record commitment is
//   unchanged and is demanded from nobody.
//
// The keyed refs that appear inside these public preimages are published
// bytes, not values the consumer must derive: the receipt carries them
// verbatim, so both sides hold the identical fragment. Destroying an
// object secret still erases exactly what it always erased — the keyed
// member's own verifiability — while the portable pins only ever proved
// that the receipt's own bytes are the bytes byom committed.

/// The canonicalization domain of the MandateUse's cross-boundary fragment.
pub const MANDATE_USE_BINDING_TAG: &str = "bpp-mandate-use-binding-v0";

/// The frozen member set of that fragment, in order. Every member is on
/// the receipt under its §13.1 name — `mandate_use_id` as
/// `mandate_use_ref`, `use_key` as `stable_execution_key`, `consumed_at`
/// as `issued_at`, `intent_ref` as itself — so a holder of the receipt
/// derives the same bytes. The MandateUse's byom-internal members
/// (`mandate_ref`, `mandate_digest`, `use_ordinal`,
/// `ceiling_reservation_refs`, `decision_refs`) are deliberately OUT: the
/// pin names the consumption's cross-boundary identity, and a member the
/// consumer cannot hold could never be re-derived.
pub const MANDATE_USE_BINDING_FIELDS: [&str; 4] =
    ["mandate_use_id", "intent_ref", "use_key", "consumed_at"];

/// The canonicalization domain of the receipt's cross-boundary fragment.
pub const RECEIPT_BINDING_TAG: &str = "bpp-execution-consumption-receipt-binding-v0";

/// The frozen member set of the receipt fragment: EXACTLY the §13.1
/// `ExecutionConsumptionReceipt` members, minus `digest` itself (which
/// commits to these bytes). The stored row's host-side and fence columns
/// (`host_effect_ref`, `host_effect_digest`, `byom_fence_epoch`,
/// `host_fence_epoch`) and `society_id` are NOT receipt members and stay
/// out of the fragment, so the preimage is never byom's whole record.
pub const RECEIPT_BINDING_FIELDS: [&str; 19] = [
    "receipt_id",
    "byom_endpoint_ref",
    "endpoint_incarnation",
    "recovery_epoch",
    "intent_ref",
    "intent_digest",
    "mandate_use_ref",
    "mandate_use_digest",
    "stable_execution_key",
    "subject_digest",
    "disclosure_digest",
    "driver_audience",
    "participant_ref",
    "episode_ref",
    "episode_fence_digest",
    "budget_reservation_set_ref",
    "issued_at",
    "expires_at",
    "max_uses",
];

/// The receipt members that are never optional (§13.1; the two optional
/// bindings are exact pairs, enforced by the request's closed shape).
const RECEIPT_REQUIRED_FIELDS: [&str; 16] = [
    "receipt_id",
    "byom_endpoint_ref",
    "endpoint_incarnation",
    "recovery_epoch",
    "intent_ref",
    "intent_digest",
    "mandate_use_ref",
    "mandate_use_digest",
    "stable_execution_key",
    "subject_digest",
    "driver_audience",
    "participant_ref",
    "budget_reservation_set_ref",
    "issued_at",
    "expires_at",
    "max_uses",
];

/// SHA-256 over the `$domain`-tagged canonical bytes of EXACTLY a frozen
/// cross-boundary fragment (`context_source_digest`/
/// `allocation_binding_digest` construction). A member outside the frozen
/// set, a missing required member, or a `null` member fails closed: a
/// silent addition would change a digest a counterparty already pinned,
/// and a `null` would let two different fragments share a preimage.
fn cross_boundary_digest(
    tag: &str,
    fragment: &Map<String, Value>,
    frozen: &[&str],
    required: &[&str],
) -> Result<DigestRef, Problem> {
    for name in fragment.keys() {
        if !frozen.contains(&name.as_str()) {
            return Err(state::internal(&format!(
                "{tag}: {name} is not a member of the frozen cross-boundary fragment"
            )));
        }
    }
    for name in required {
        if fragment.get(*name).is_none_or(Value::is_null) {
            return Err(state::internal(&format!(
                "{tag}: the fragment does not carry its required member {name}"
            )));
        }
    }
    if fragment.values().any(Value::is_null) {
        return Err(state::internal(&format!(
            "{tag}: an absent optional member is ABSENT, never null"
        )));
    }
    let bytes = tagged_canonical(tag, &Value::Object(fragment.clone()))
        .map_err(|e| state::internal(&e.to_string()))?;
    Ok(DigestRef::portable_public(sha256_hex(&bytes)))
}

/// The `portable_public` MandateUse pin the receipt publishes as
/// `mandate_use_digest`.
pub fn mandate_use_binding_digest(
    mandate_use_id: &str,
    intent_ref: &str,
    use_key: &str,
    consumed_at: &str,
) -> Result<DigestRef, Problem> {
    let fragment = obj_pairs([
        ("mandate_use_id", json!(mandate_use_id)),
        ("intent_ref", json!(intent_ref)),
        ("use_key", json!(use_key)),
        ("consumed_at", json!(consumed_at)),
    ]);
    cross_boundary_digest(
        MANDATE_USE_BINDING_TAG,
        &fragment,
        &MANDATE_USE_BINDING_FIELDS,
        &MANDATE_USE_BINDING_FIELDS,
    )
}

/// The receipt's own published members, exactly as the result renders
/// them: the frozen fragment, absent optionals absent (never null),
/// `digest` excluded. This is BOTH the digest preimage and the rendered
/// result body, so the receipt the consumer re-derives is the receipt
/// byom digested.
fn receipt_fragment(row: &Map<String, Value>) -> Map<String, Value> {
    let mut out = Map::new();
    for name in RECEIPT_BINDING_FIELDS {
        let value = match name {
            "recovery_epoch" => json!(rows::u64_of(row, name)),
            // Invariant constant: an ExecutionConsumptionReceipt is
            // one-shot (§13.1), never a stored number that could drift.
            "max_uses" => json!(1),
            digest if digest.ends_with("_digest") => rows::json_of(row, digest),
            plain => match rows::str_of(row, plain) {
                "" => Value::Null,
                text => json!(text),
            },
        };
        if !value.is_null() {
            out.insert(name.to_owned(), value);
        }
    }
    out
}

/// The `portable_public` receipt pin the receipt publishes as `digest`.
fn receipt_binding_digest(fragment: &Map<String, Value>) -> Result<DigestRef, Problem> {
    cross_boundary_digest(
        RECEIPT_BINDING_TAG,
        fragment,
        &RECEIPT_BINDING_FIELDS,
        &RECEIPT_REQUIRED_FIELDS,
    )
}

/// The §13.1 receipt as `execution_permit_consume` returns it — the ONE
/// renderer both the minting and the replay path go through, so a receipt
/// recovered after a crash is byte-identical to the one first returned.
fn receipt_result(row: &Map<String, Value>, replayed: bool) -> Value {
    let mut out = receipt_fragment(row);
    out.insert("digest".to_owned(), rows::json_of(row, "digest"));
    if replayed {
        out.insert("replayed".to_owned(), json!(true));
    }
    Value::Object(out)
}
