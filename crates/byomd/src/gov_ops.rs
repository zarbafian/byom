//! Governance-surface mutations: `society_prepare` + `society_bootstrap`
//! (atomic genesis, §6.1), `membership_offer` (+ candidate-channel
//! minting, §7.4), `participant_admit` (Standing activation, channel
//! conversion), and `manifestation_admit`. Every handler drives ONE
//! §15.3 authority transaction; dependency revalidation happens inside
//! the prepare closure against the open transaction.

use std::io::Write as _;
use std::os::unix::fs::PermissionsExt as _;

use bpp_core::canonical::{hex, hmac_sha256, tagged_canonical};
use bpp_core::envelope::MutationMeta;
use bpp_core::ops;
use bpp_core::problem::{Problem, ProblemKind};
use bpp_core::time::{parse_rfc3339_utc, rfc3339_utc};
use byom_store::effects::{Effect, NewEvent};
use byom_store::rows;
use byom_store::{
    CommandError, CrashHooks, CursorMint, MutationScope, Prepared, Store, GENESIS_SCOPE,
};
use rusqlite::Connection;
use serde_json::{json, Map, Value};

use crate::state;

/// The governance actor of the developer profile: the same-UID sovereign
/// human channel (§14.5; fresh phishing-resistant challenge is a
/// recorded developer-profile stub at this slice).
pub const ACTOR_GOVERNANCE: &str = "governance:sovereign";
/// Deterministic server-time transitions (offer expiry).
pub const ACTOR_SERVER: &str = "server:time";

/// Bootstrap preparation expiry (24 h).
const PREPARATION_TTL_SECS: i64 = 86_400;

pub fn run(
    store: &mut Store,
    scope: MutationScope,
    now: i64,
    hooks: CrashHooks,
    apply: impl Fn(&Connection, &MutationScope) -> Result<Prepared, Problem>,
) -> Result<Vec<u8>, Problem> {
    match store.authority_mutation(&scope, now, hooks, apply) {
        Ok(bytes) => Ok(bytes),
        Err(CommandError::Problem(p)) => Err(p),
        Err(CommandError::Store(e)) => Err(state::internal(&e.to_string())),
    }
}

/// §14.2: a mutation addressed to an old endpoint incarnation or Society
/// recovery epoch is rejected, never looked up in the new domain.
pub fn check_meta_binding(
    store: &Store,
    meta: &MutationMeta,
    society_id: &str,
) -> Result<(), Problem> {
    let incarnation = store
        .incarnation()
        .map_err(|e| state::internal(&e.to_string()))?;
    if meta.expected_endpoint_incarnation != incarnation {
        return Err(state::stale_binding(
            "expected_endpoint_incarnation is not the current incarnation",
        ));
    }
    let epoch = store
        .recovery_epoch(society_id)
        .map_err(|e| state::internal(&e.to_string()))?;
    if meta.expected_recovery_epoch != epoch {
        return Err(state::stale_binding(
            "expected_recovery_epoch is not the Society's current epoch",
        ));
    }
    Ok(())
}

pub fn mint(store: &Store, prefix: &str) -> Result<String, Problem> {
    store
        .new_id(prefix)
        .map_err(|e| state::internal(&e.to_string()))
}

pub fn causation_of(meta: &MutationMeta) -> String {
    meta.causation_event_ref
        .clone()
        .unwrap_or_else(|| format!("req:{}", meta.request_id))
}

pub fn correlation_of(meta: &MutationMeta) -> String {
    meta.correlation_ref
        .clone()
        .unwrap_or_else(|| meta.request_id.clone())
}

pub fn digest_json(d: &bpp_core::digest::DigestRef) -> Value {
    serde_json::to_value(d).unwrap_or(Value::Null)
}

pub fn db_err(e: rusqlite::Error) -> Problem {
    state::internal(&e.to_string())
}

// --------------------------------------------------- society_prepare ----

pub fn society_prepare(
    store: &mut Store,
    req: &ops::SocietyPrepareRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    check_meta_binding(store, &req.meta, GENESIS_SCOPE)?;

    // Deterministic compilation of the bootstrap subject (§10.5): every
    // projected member has a named source; no semantic defaults.
    let society_id = mint(store, "soc")?;
    let preparation_ref = mint(store, "prep")?;
    let sovereign_seat = mint(store, "seat-sov")?;
    let budget_root = mint(store, "budget-root")?;
    let charter_revision_id = mint(store, "charter-r1")?;
    let trace_id = mint(store, "trace")?;
    let event_id = mint(store, "evt")?;
    let created_at = rfc3339_utc(now);
    let expires_at = rfc3339_utc(now + PREPARATION_TTL_SECS);

    let mut subject = Map::new();
    subject.insert("society_id".into(), json!(society_id));
    subject.insert("home_authority_ref".into(), json!(req.home_authority_ref));
    if let Some(realm) = &req.kovee_realm_binding {
        subject.insert("kovee_realm_binding".into(), json!(realm));
    }
    if let Some(project) = &req.kovee_project_binding {
        subject.insert("kovee_project_binding".into(), json!(project));
    }
    subject.insert("charter_ref".into(), json!(req.proposed_charter_ref));
    subject.insert(
        "charter_digest".into(),
        digest_json(&req.proposed_charter_digest),
    );
    subject.insert(
        "classification_binding_ref".into(),
        json!(req.classification_binding_ref),
    );
    subject.insert(
        "classification_binding_digest".into(),
        digest_json(&req.classification_binding_digest),
    );
    subject.insert("sovereign_seat_set".into(), json!([sovereign_seat]));
    subject.insert("root_budget_account_set_ref".into(), json!(budget_root));
    let subject = Value::Object(subject);

    let (subject_digest, subject_secret) = store
        .mint_object_digest(
            &format!("society-key:{society_id}/object:bootstrap-subject"),
            "bpp-bootstrap-subject-v0",
            &subject,
        )
        .map_err(|e| state::internal(&e.to_string()))?;
    let (input_digest, _input_secret) = store
        .mint_object_digest(
            &format!("society-key:{society_id}/object:prepare-input"),
            "bpp-preparation-input-v0",
            body,
        )
        .map_err(|e| state::internal(&e.to_string()))?;

    // Complete output-pointer provenance (§10.5, RT-04): every projected
    // member of the prepared subject names its source.
    let mut field_sources = vec![
        source_row(
            "/society_id",
            &req.meta.request_id,
            "/meta/request_id",
            "t-mint-id",
        ),
        source_row(
            "/home_authority_ref",
            &req.meta.request_id,
            "/home_authority_ref",
            "t-copy",
        ),
        source_row(
            "/charter_ref",
            &req.meta.request_id,
            "/proposed_charter_ref",
            "t-copy",
        ),
        source_row(
            "/charter_digest",
            &req.meta.request_id,
            "/proposed_charter_digest",
            "t-copy",
        ),
        source_row(
            "/classification_binding_ref",
            &req.meta.request_id,
            "/classification_binding_ref",
            "t-copy",
        ),
        source_row(
            "/classification_binding_digest",
            &req.meta.request_id,
            "/classification_binding_digest",
            "t-copy",
        ),
        source_row(
            "/sovereign_seat_set",
            &req.meta.request_id,
            "/meta/request_id",
            "t-mint-sovereign-seat",
        ),
        source_row(
            "/root_budget_account_set_ref",
            &req.meta.request_id,
            "/meta/request_id",
            "t-mint-budget-root",
        ),
    ];
    if req.kovee_realm_binding.is_some() {
        field_sources.push(source_row(
            "/kovee_realm_binding",
            &req.meta.request_id,
            "/kovee_realm_binding",
            "t-copy",
        ));
    }
    if req.kovee_project_binding.is_some() {
        field_sources.push(source_row(
            "/kovee_project_binding",
            &req.meta.request_id,
            "/kovee_project_binding",
            "t-copy",
        ));
    }

    let mut trace = json!({
        "trace_id": trace_id,
        "operation": "society_prepare",
        "actor_binding_ref": ACTOR_GOVERNANCE,
        "input_ref": format!("req:{}", req.meta.request_id),
        "input_digest": digest_json(&input_digest),
        "output_subject_digest": digest_json(&subject_digest),
        "field_sources": field_sources,
        "policy_algebra_version": "bpa-1",
        "dependency_set_ref": "deps-genesis",
        "created_at": created_at,
    });
    let (trace_digest, _trace_secret) = store
        .mint_object_digest(
            &format!("society-key:{society_id}/object:{trace_id}"),
            "bpp-preparation-trace-v0",
            &trace,
        )
        .map_err(|e| state::internal(&e.to_string()))?;
    trace["digest"] = digest_json(&trace_digest);

    let preparation = json!({
        "preparation_ref": preparation_ref,
        "subject": subject,
        "subject_secret": subject_secret,
        "subject_digest": digest_json(&subject_digest),
        "sovereign_seat_set": [sovereign_seat],
        "charter_revision_id": charter_revision_id,
        "expires_at": expires_at,
        "trace": trace,
    });

    let result = json!({
        "society_id": society_id,
        "revision": 1,
        "state": "forming",
        "preparation_ref": preparation_ref,
        "subject_digest": digest_json(&subject_digest),
        "sovereign_seat_set": [sovereign_seat],
        "expires_at": expires_at,
        "preparation_trace": trace,
    });

    let society_effect = Effect::Upsert {
        table: "societies".into(),
        row: society_row(
            &society_id,
            1,
            "forming",
            req,
            &budget_root,
            &created_at,
            &preparation,
            None,
            1,
        ),
    };
    let charter_effect = Effect::Upsert {
        table: "charter_revisions".into(),
        row: obj_pairs([
            ("charter_revision_id", json!(charter_revision_id)),
            ("society_id", json!(society_id)),
            ("revision", json!(1)),
            ("body_ref", json!(req.proposed_charter_ref)),
            ("body_digest", digest_json(&req.proposed_charter_digest)),
            ("state", json!("proposed")),
            ("adopted_by_decision_ref", Value::Null),
            ("created_at", json!(created_at)),
            ("effective_at", Value::Null),
        ]),
    };
    let event = NewEvent {
        event_id,
        society_id: society_id.clone(),
        kind: "society.prepared".into(),
        object_ref: society_id.clone(),
        object_revision: 1,
        participant_ref: None,
        actor_ref: ACTOR_GOVERNANCE.into(),
        causation_ref: causation_of(&req.meta),
        correlation_ref: correlation_of(&req.meta),
        payload: json!({"state": "forming", "preparation_ref": preparation_ref}),
        visibility_scope_ref: "scope:society".into(),
    };

    let scope = MutationScope {
        society_id: GENESIS_SCOPE.into(),
        operation: "society_prepare".into(),
        actor: ACTOR_GOVERNANCE.into(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let sid = society_id.clone();
    run(store, scope, now, hooks, move |conn, _| {
        if rows::get_society(conn, &sid).map_err(db_err)?.is_some() {
            return Err(state::internal("minted society id collision"));
        }
        Ok(Prepared {
            result: result.clone(),
            revision: Some(1),
            cursor: CursorMint::AfterEvents {
                society_id: sid.clone(),
            },
            effects: vec![society_effect.clone(), charter_effect.clone()],
            events: vec![event.clone()],
        })
    })
}

fn source_row(output: &str, request_id: &str, source_pointer: &str, transform: &str) -> Value {
    json!({
        "output_pointer": output,
        "source_ref": format!("req:{request_id}"),
        "source_pointer": source_pointer,
        "transform_id": transform,
    })
}

pub fn obj_pairs<const N: usize>(entries: [(&str, Value); N]) -> Map<String, Value> {
    let mut m = Map::new();
    for (k, v) in entries {
        m.insert(k.to_owned(), v);
    }
    m
}

#[allow(clippy::too_many_arguments)]
fn society_row(
    society_id: &str,
    revision: u64,
    society_state: &str,
    req: &ops::SocietyPrepareRequest,
    budget_root: &str,
    created_at: &str,
    preparation: &Value,
    genesis_event_ref: Option<&str>,
    next_event_sequence: u64,
) -> Map<String, Value> {
    obj_pairs([
        ("society_id", json!(society_id)),
        ("revision", json!(revision)),
        ("state", json!(society_state)),
        ("home_authority_ref", json!(req.home_authority_ref)),
        (
            "kovee_realm_binding",
            req.kovee_realm_binding
                .as_ref()
                .map(|v| json!(v))
                .unwrap_or(Value::Null),
        ),
        (
            "kovee_project_binding",
            req.kovee_project_binding
                .as_ref()
                .map(|v| json!(v))
                .unwrap_or(Value::Null),
        ),
        ("charter_head_ref", json!(req.proposed_charter_ref)),
        (
            "charter_head_digest",
            digest_json(&req.proposed_charter_digest),
        ),
        (
            "classification_binding_ref",
            json!(req.classification_binding_ref),
        ),
        (
            "classification_binding_digest",
            digest_json(&req.classification_binding_digest),
        ),
        ("root_budget_account_set_ref", json!(budget_root)),
        ("recovery_epoch", json!(0)),
        ("created_at", json!(created_at)),
        ("preparation", preparation.clone()),
        (
            "genesis_event_ref",
            genesis_event_ref.map(|v| json!(v)).unwrap_or(Value::Null),
        ),
        ("next_event_sequence", json!(next_event_sequence)),
    ])
}

// -------------------------------------------------- society_bootstrap ----

pub fn society_bootstrap(
    store: &mut Store,
    req: &ops::SocietyBootstrapRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    let society = rows::get_society(store.conn(), &req.society_id)
        .map_err(db_err)?
        .ok_or_else(state::not_found)?;
    check_meta_binding(store, &req.meta, &society.society_id)?;

    // Stable mints for the genesis set (one command transaction, one
    // journal entry; §6.1 crash result: none or complete genesis).
    let decision_ref = mint(store, "dec-bootstrap")?;
    let participant_id = mint(store, "part-sov")?;
    let standing_id = mint(store, "standing")?;
    let genesis_event = mint(store, "evt")?;
    let charter_event = mint(store, "evt")?;
    let participant_event = mint(store, "evt")?;
    let standing_event = mint(store, "evt")?;
    let budget_event = mint(store, "evt")?;
    let created_at = rfc3339_utc(now);

    let scope = MutationScope {
        society_id: req.society_id.clone(),
        operation: "society_bootstrap".into(),
        actor: ACTOR_GOVERNANCE.into(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let req = req.clone();
    let store_incarnation = store
        .incarnation()
        .map_err(|e| state::internal(&e.to_string()))?;
    let bytes = run(store, scope, now, hooks, move |conn, _| {
        let society = rows::get_society(conn, &req.society_id)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        if req.meta.expected_revision != Some(society.revision) {
            return Err(state::stale_revision());
        }
        if society.state != "forming" {
            return Err(state::stale_binding("society is not in state forming"));
        }
        let preparation: Value = serde_json::from_str(&society.preparation)
            .map_err(|e| state::internal(&e.to_string()))?;
        if preparation["preparation_ref"].as_str() != Some(req.preparation_ref.as_str()) {
            return Err(state::not_found());
        }
        if let (Some(expiry_text), Some(now_secs)) = (preparation["expires_at"].as_str(), Some(now))
        {
            if parse_rfc3339_utc(expiry_text).is_some_and(|t| t < now_secs) {
                return Err(state::stale_binding("preparation expired"));
            }
        }
        // The server recomputes the exact prepared subject digest; the
        // sovereign confirms exactly what was prepared.
        let secret_hex = preparation["subject_secret"].as_str().unwrap_or_default();
        let secret = unhex(secret_hex).ok_or_else(|| state::internal("corrupt preparation"))?;
        let preimage = tagged_canonical("bpp-bootstrap-subject-v0", &preparation["subject"])
            .map_err(|e| state::internal(&e.to_string()))?;
        let recomputed = hex(&hmac_sha256(&secret, &preimage));
        if recomputed != req.subject_digest.value_hex {
            return Err(state::invalid(
                "subject_digest does not match the prepared bootstrap subject",
            ));
        }

        let charter_revision_id = preparation["charter_revision_id"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let seat = preparation["sovereign_seat_set"][0]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let new_revision = society.revision + 1;

        // The fresh phishing-resistant challenge of R2 is a developer
        // -profile stub at the B1 attached slice — honestly recorded in
        // the genesis event payload and the audit ledger.
        let auth_note =
            "developer-profile-stub: fresh phishing-resistant challenge lands with the hosted slice";

        let mut society_effect_row = society_effect_row(&society);
        society_effect_row.insert("revision".into(), json!(new_revision));
        society_effect_row.insert("state".into(), json!("active"));
        society_effect_row.insert("genesis_event_ref".into(), json!(genesis_event));
        let effects = vec![
            Effect::Upsert {
                table: "societies".into(),
                row: society_effect_row,
            },
            Effect::Upsert {
                table: "charter_revisions".into(),
                row: obj_pairs([
                    ("charter_revision_id", json!(charter_revision_id)),
                    ("society_id", json!(society.society_id)),
                    ("revision", json!(1)),
                    ("body_ref", json!(society.charter_head_ref)),
                    (
                        "body_digest",
                        serde_json::from_str(&society.charter_head_digest).unwrap_or(Value::Null),
                    ),
                    ("state", json!("adopted")),
                    ("adopted_by_decision_ref", json!(decision_ref)),
                    ("created_at", json!(society.created_at)),
                    ("effective_at", json!(created_at)),
                ]),
            },
            Effect::Upsert {
                table: "participants".into(),
                row: obj_pairs([
                    ("participant_id", json!(participant_id)),
                    ("society_id", json!(society.society_id)),
                    ("kind", json!("human")),
                    ("revision", json!(1)),
                    ("binding_epoch", json!(1)),
                    (
                        "display_profile_ref",
                        json!(format!("profile:{participant_id}")),
                    ),
                    ("standing_ref", json!(standing_id)),
                    ("state", json!("active")),
                    ("created_at", json!(created_at)),
                ]),
            },
            Effect::Upsert {
                table: "standing_revisions".into(),
                row: obj_pairs([
                    ("standing_id", json!(standing_id)),
                    ("society_id", json!(society.society_id)),
                    ("participant_ref", json!(participant_id)),
                    ("revision", json!(1)),
                    ("status", json!("active")),
                    ("offer_ref", Value::Null),
                    ("acceptance_ref", Value::Null),
                    ("decision_ref", json!(decision_ref)),
                    ("created_at", json!(created_at)),
                ]),
            },
        ];
        let causation = causation_of(&req.meta);
        let correlation = correlation_of(&req.meta);
        let ev =
            |event_id: &str, kind: &str, object: &str, revision: u64, payload: Value| NewEvent {
                event_id: event_id.to_owned(),
                society_id: society.society_id.clone(),
                kind: kind.to_owned(),
                object_ref: object.to_owned(),
                object_revision: revision,
                participant_ref: Some(participant_id.clone()),
                actor_ref: ACTOR_GOVERNANCE.into(),
                causation_ref: causation.clone(),
                correlation_ref: correlation.clone(),
                payload,
                visibility_scope_ref: "scope:society".into(),
            };
        let events = vec![
            ev(
                &genesis_event,
                "society.genesis",
                &society.society_id,
                new_revision,
                json!({
                    "sovereign_seat": seat,
                    "authentication": auth_note,
                    "endpoint_incarnation": store_incarnation,
                }),
            ),
            ev(
                &charter_event,
                "charter.adopted",
                &charter_revision_id,
                1,
                json!({"body_ref": society.charter_head_ref, "decision_ref": decision_ref}),
            ),
            ev(
                &participant_event,
                "participant.admitted",
                &participant_id,
                1,
                json!({"kind": "human", "seat": seat, "decision_ref": decision_ref}),
            ),
            ev(
                &standing_event,
                "standing.activated",
                &standing_id,
                1,
                json!({"participant_ref": participant_id, "decision_ref": decision_ref}),
            ),
            ev(
                &budget_event,
                "budget.roots_established",
                &society.root_budget_account_set_ref,
                1,
                json!({"root_budget_account_set_ref": society.root_budget_account_set_ref}),
            ),
        ];
        let mut result = json!({
            "society_id": society.society_id,
            "revision": new_revision,
            "home_authority_ref": society.home_authority_ref,
            "charter_head_ref": society.charter_head_ref,
            "charter_head_digest": serde_json::from_str::<Value>(&society.charter_head_digest)
                .unwrap_or(Value::Null),
            "classification_binding_ref": society.classification_binding_ref,
            "classification_binding_digest":
                serde_json::from_str::<Value>(&society.classification_binding_digest)
                    .unwrap_or(Value::Null),
            "root_budget_account_set_ref": society.root_budget_account_set_ref,
            "recovery_epoch": society.recovery_epoch,
            "state": "active",
            "created_at": society.created_at,
            "genesis_event_ref": genesis_event,
        });
        if let Some(realm) = &society.kovee_realm_binding {
            result["kovee_realm_binding"] = json!(realm);
        }
        if let Some(project) = &society.kovee_project_binding {
            result["kovee_project_binding"] = json!(project);
        }
        Ok(Prepared {
            result,
            revision: Some(new_revision),
            // The sovereign's genesis cursor replays the complete causal
            // timeline from the very beginning (§14.4 derivation).
            cursor: CursorMint::FromStart {
                society_id: society.society_id.clone(),
            },
            effects,
            events,
        })
    })?;
    Ok(bytes)
}

/// The full-row upsert map of one society row (every column named).
pub fn society_effect_row(s: &rows::SocietyRow) -> Map<String, Value> {
    obj_pairs([
        ("society_id", json!(s.society_id)),
        ("revision", json!(s.revision)),
        ("state", json!(s.state)),
        ("home_authority_ref", json!(s.home_authority_ref)),
        (
            "kovee_realm_binding",
            s.kovee_realm_binding
                .as_ref()
                .map(|v| json!(v))
                .unwrap_or(Value::Null),
        ),
        (
            "kovee_project_binding",
            s.kovee_project_binding
                .as_ref()
                .map(|v| json!(v))
                .unwrap_or(Value::Null),
        ),
        ("charter_head_ref", json!(s.charter_head_ref)),
        (
            "charter_head_digest",
            serde_json::from_str(&s.charter_head_digest).unwrap_or(Value::Null),
        ),
        (
            "classification_binding_ref",
            json!(s.classification_binding_ref),
        ),
        (
            "classification_binding_digest",
            serde_json::from_str(&s.classification_binding_digest).unwrap_or(Value::Null),
        ),
        (
            "root_budget_account_set_ref",
            json!(s.root_budget_account_set_ref),
        ),
        ("recovery_epoch", json!(s.recovery_epoch)),
        ("created_at", json!(s.created_at)),
        (
            "preparation",
            serde_json::from_str(&s.preparation).unwrap_or(Value::Null),
        ),
        (
            "genesis_event_ref",
            s.genesis_event_ref
                .as_ref()
                .map(|v| json!(v))
                .unwrap_or(Value::Null),
        ),
        ("next_event_sequence", json!(s.next_event_sequence)),
    ])
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

// -------------------------------------------------- membership_offer ----

pub fn membership_offer(
    store: &mut Store,
    req: &ops::MembershipOfferRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    // The attached slice serves one Society: the offer targets the sole
    // active Society of this endpoint.
    let society = rows::sole_society(store.conn())
        .map_err(db_err)?
        .ok_or_else(state::not_found)?;
    check_meta_binding(store, &req.meta, &society.society_id)?;

    let offer_id = mint(store, "offer")?;
    let channel_id = mint(store, "chan")?;
    let manifestation_id = mint(store, "manif")?;
    let token = mint(store, "cand-token")?;
    let offer_event = mint(store, "evt")?;
    let participant_event = mint(store, "evt")?;
    let manifestation_event = mint(store, "evt")?;
    let channel_event = mint(store, "evt")?;
    let created_at = rfc3339_utc(now);
    let token_path = channels_dir(store)
        .join(format!("candidate-{offer_id}.token"))
        .display()
        .to_string();
    let offer_record_digest = store
        .record_digest(
            &society.society_id,
            &offer_id,
            "bpp-membership-offer-v0",
            &json!({
                "offer_id": offer_id,
                "participant_ref": req.participant_ref,
                "proposed_standing_ref": req.proposed_standing_ref,
                "subject_digest": digest_json(&req.subject_digest),
                "offered_by_decision_ref": req.offered_by_decision_ref,
                "expires_at": req.expires_at,
                "state": "offered",
                "revision": 1,
            }),
        )
        .map_err(|e| state::internal(&e.to_string()))?;

    let scope = MutationScope {
        society_id: society.society_id.clone(),
        operation: "membership_offer".into(),
        actor: ACTOR_GOVERNANCE.into(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let req = req.clone();
    let sid = society.society_id.clone();
    let bytes = run(store, scope, now, hooks, move |conn, _| {
        let society = rows::get_society(conn, &sid)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        if society.state != "active" {
            return Err(state::stale_binding("society is not active"));
        }
        if parse_rfc3339_utc(&req.expires_at).is_some_and(|t| t <= now) {
            return Err(state::invalid("expires_at is already past"));
        }
        // The candidate Participant record: proposed at offer time. An
        // identity that already holds Standing cannot be re-offered.
        if let Some(existing) = rows::get_participant(conn, &req.participant_ref).map_err(db_err)? {
            if existing.state != "proposed" {
                return Err(state::forbidden());
            }
        }
        let effects = vec![
            Effect::Upsert {
                table: "membership_offers".into(),
                row: obj_pairs([
                    ("offer_id", json!(offer_id)),
                    ("society_id", json!(sid)),
                    ("participant_ref", json!(req.participant_ref)),
                    ("proposed_standing_ref", json!(req.proposed_standing_ref)),
                    ("subject_digest", digest_json(&req.subject_digest)),
                    (
                        "offered_by_decision_ref",
                        json!(req.offered_by_decision_ref),
                    ),
                    ("expires_at", json!(req.expires_at)),
                    ("state", json!("offered")),
                    ("revision", json!(1)),
                    ("fence_epoch", json!(1)),
                    ("acceptance_id", Value::Null),
                    ("accepted_at", Value::Null),
                    ("refusal_id", Value::Null),
                    ("refused_at", Value::Null),
                    ("superseded_acceptance_ref", Value::Null),
                    ("refusal_reason_ref", Value::Null),
                    ("created_at", json!(created_at)),
                ]),
            },
            Effect::Upsert {
                table: "participants".into(),
                row: obj_pairs([
                    ("participant_id", json!(req.participant_ref)),
                    ("society_id", json!(sid)),
                    ("kind", json!("agent")),
                    ("revision", json!(1)),
                    ("binding_epoch", json!(0)),
                    (
                        "display_profile_ref",
                        json!(format!("profile:{}", req.participant_ref)),
                    ),
                    ("standing_ref", Value::Null),
                    ("state", json!("proposed")),
                    ("created_at", json!(created_at)),
                ]),
            },
            // The proposed attached_harness ManifestationRevision the
            // offer binds (§7.2 onboarding binding); admitted separately
            // by manifestation_admit after Standing.
            Effect::Upsert {
                table: "manifestation_revisions".into(),
                row: obj_pairs([
                    ("manifestation_id", json!(manifestation_id)),
                    ("society_id", json!(sid)),
                    ("participant_ref", json!(req.participant_ref)),
                    ("revision", json!(1)),
                    ("kind", json!("attached_harness")),
                    ("body_digest", digest_json(&req.subject_digest)),
                    ("status", json!("proposed")),
                    ("admitted_by_decision_ref", Value::Null),
                    ("created_at", json!(created_at)),
                ]),
            },
            Effect::Upsert {
                table: "candidate_channels".into(),
                row: obj_pairs([
                    ("channel_id", json!(channel_id)),
                    ("society_id", json!(sid)),
                    ("offer_ref", json!(offer_id)),
                    ("token", json!(token)),
                    ("token_path", json!(token_path)),
                    ("state", json!("open")),
                    ("created_at", json!(created_at)),
                    ("closed_at", Value::Null),
                ]),
            },
        ];
        let causation = causation_of(&req.meta);
        let correlation = correlation_of(&req.meta);
        let ev = |event_id: &str, kind: &str, object: &str, payload: Value| NewEvent {
            event_id: event_id.to_owned(),
            society_id: sid.clone(),
            kind: kind.to_owned(),
            object_ref: object.to_owned(),
            object_revision: 1,
            participant_ref: Some(req.participant_ref.clone()),
            actor_ref: ACTOR_GOVERNANCE.into(),
            causation_ref: causation.clone(),
            correlation_ref: correlation.clone(),
            payload,
            visibility_scope_ref: "scope:society".into(),
        };
        let events = vec![
            ev(
                &offer_event,
                "membership.offered",
                &offer_id,
                json!({"state": "offered", "decision_ref": req.offered_by_decision_ref,
                       "expires_at": req.expires_at}),
            ),
            ev(
                &participant_event,
                "participant.proposed",
                &req.participant_ref,
                json!({"state": "proposed", "offer_ref": offer_id}),
            ),
            ev(
                &manifestation_event,
                "manifestation.proposed",
                &manifestation_id,
                json!({"status": "proposed", "kind": "attached_harness"}),
            ),
            ev(
                &channel_event,
                "channel.candidate_minted",
                &channel_id,
                json!({"offer_ref": offer_id, "constraint": "sender-constrained token file (developer profile)"}),
            ),
        ];
        Ok(Prepared {
            result: json!({
                "offer_id": offer_id,
                "revision": 1,
                "state": "offered",
                "expires_at": req.expires_at,
                "digest": digest_json(&offer_record_digest),
            }),
            revision: Some(1),
            cursor: CursorMint::AfterEvents {
                society_id: sid.clone(),
            },
            effects,
            events,
        })
    })?;
    ensure_channel_files(store);
    Ok(bytes)
}

// ------------------------------------------------- participant_admit ----

pub fn participant_admit(
    store: &mut Store,
    req: &ops::ParticipantAdmitRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    let offer = rows::get_offer(store.conn(), &req.offer_ref)
        .map_err(db_err)?
        .ok_or_else(state::not_found)?;
    check_meta_binding(store, &req.meta, &offer.society_id)?;
    // Deterministic server-time expiry races admission through the same
    // CAS: apply it first, as its own journaled transition.
    expire_offer_if_due(store, &req.offer_ref, now)?;

    let standing_id = mint(store, "standing")?;
    let participant_channel_id = mint(store, "chan")?;
    let participant_token = mint(store, "part-token")?;
    let admit_event = mint(store, "evt")?;
    let standing_event = mint(store, "evt")?;
    let channel_event = mint(store, "evt")?;
    let created_at = rfc3339_utc(now);
    let sid = offer.society_id.clone();
    let token_path = channels_dir(store)
        .join(format!("participant-{}.token", offer.participant_ref))
        .display()
        .to_string();

    let scope = MutationScope {
        society_id: sid.clone(),
        operation: "participant_admit".into(),
        actor: ACTOR_GOVERNANCE.into(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let req = req.clone();
    let bytes = run(store, scope, now, hooks, move |conn, _| {
        let offer = rows::get_offer(conn, &req.offer_ref)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        // The revision CAS decides races first (§7.4: refusal/expiry
        // race admission on the same offer revision; the loser is
        // stale_revision), then the machine's transition guards.
        if req.meta.expected_revision != Some(offer.revision) {
            return Err(state::stale_revision());
        }
        match offer.state.as_str() {
            "accepted" => {}
            "offered" | "onboarding" => {
                return Err(Problem::new(
                    ProblemKind::DecisionIncomplete,
                    "admission requires the candidate's current MembershipAcceptance",
                )
                .with_status(409));
            }
            // No terminal offer can later admit (§7.4/§14.8).
            _ => return Err(state::stale_binding("terminal offer cannot admit")),
        }
        // Admission binds the CURRENT acceptance, exactly.
        if offer.acceptance_id.as_deref() != Some(req.membership_acceptance_ref.as_str()) {
            return Err(state::stale_binding(
                "membership_acceptance_ref does not cite the current acceptance",
            ));
        }
        // The admission subject is the exact offer subject.
        let offer_subject: Value =
            serde_json::from_str(&offer.subject_digest).unwrap_or(Value::Null);
        if offer_subject["value_hex"].as_str()
            != Some(req.admission_subject_digest.value_hex.as_str())
        {
            return Err(state::invalid(
                "admission_subject_digest does not match the offer subject",
            ));
        }
        // Pre-admission candidate self-policy proposals activate HERE,
        // exactly as authored, never before Standing (B1 sheet). A cited
        // proposal must exist, belong to THIS offer, and still be
        // proposed; anything else is citing a record that does not exist.
        let mut policy_effects = Vec::new();
        let mut activated_policy_refs = Vec::new();
        if let Some(refs) = &req.included_self_policy_proposal_refs {
            for proposal_ref in refs {
                let proposal = rows::get_row(
                    conn,
                    "candidate_policy_proposals",
                    "proposal_id",
                    proposal_ref,
                )
                .map_err(db_err)?
                .ok_or_else(state::not_found)?;
                if rows::str_of(&proposal, "offer_ref") != offer.offer_id
                    || rows::str_of(&proposal, "state") != "proposed"
                {
                    return Err(state::not_found());
                }
                let policy_id = format!("selfpol-{proposal_ref}");
                let mut activated = proposal.clone();
                activated.insert("state".into(), json!("activated"));
                activated.insert("activated_policy_ref".into(), json!(policy_id));
                policy_effects.push(Effect::Upsert {
                    table: "candidate_policy_proposals".into(),
                    row: activated,
                });
                // The activated policy is the proposal body VERBATIM —
                // governance activates, never edits (§7.3).
                policy_effects.push(Effect::Upsert {
                    table: "self_policies".into(),
                    row: obj_pairs([
                        ("policy_id", json!(policy_id)),
                        ("society_id", json!(offer.society_id)),
                        ("participant_ref", json!(offer.participant_ref)),
                        ("kind", json!(rows::str_of(&proposal, "kind"))),
                        ("revision", json!(1)),
                        ("status", json!("active")),
                        ("body", json!(rows::str_of(&proposal, "body"))),
                        (
                            "body_digest",
                            json!(rows::json_of(&proposal, "body_digest")),
                        ),
                        (
                            "adoption_mode",
                            json!(rows::str_of(&proposal, "adoption_mode")),
                        ),
                        ("provenance", json!("candidate_authored")),
                        ("previous_policy_ref", Value::Null),
                        ("effective_at", json!(created_at)),
                        ("expires_at", json!("9999-12-31T23:59:59Z")),
                        ("created_at", json!(created_at)),
                    ]),
                });
                activated_policy_refs.push(policy_id);
            }
        }
        let participant = rows::get_participant(conn, &offer.participant_ref)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        let channel = rows::candidate_channel_for_offer(conn, &req.offer_ref)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;

        let new_offer_revision = offer.revision + 1;
        let new_binding_epoch = participant.binding_epoch + 1;
        let mut offer_row = offer.to_effect_row();
        offer_row.insert("state".into(), json!("admitted"));
        offer_row.insert("revision".into(), json!(new_offer_revision));
        offer_row.insert("fence_epoch".into(), json!(offer.fence_epoch + 1));
        let mut effects = vec![
            Effect::Upsert {
                table: "membership_offers".into(),
                row: offer_row,
            },
            Effect::Upsert {
                table: "participants".into(),
                row: obj_pairs([
                    ("participant_id", json!(participant.participant_id)),
                    ("society_id", json!(participant.society_id)),
                    ("kind", json!(participant.kind)),
                    ("revision", json!(participant.revision + 1)),
                    ("binding_epoch", json!(new_binding_epoch)),
                    (
                        "display_profile_ref",
                        json!(participant.display_profile_ref),
                    ),
                    ("standing_ref", json!(standing_id)),
                    ("state", json!("active")),
                    ("created_at", json!(participant.created_at)),
                ]),
            },
            Effect::Upsert {
                table: "standing_revisions".into(),
                row: obj_pairs([
                    ("standing_id", json!(standing_id)),
                    ("society_id", json!(offer.society_id)),
                    ("participant_ref", json!(offer.participant_ref)),
                    ("revision", json!(1)),
                    ("status", json!("active")),
                    ("offer_ref", json!(offer.offer_id)),
                    ("acceptance_ref", json!(req.membership_acceptance_ref)),
                    ("decision_ref", json!(req.admitted_by_decision_ref)),
                    ("created_at", json!(created_at)),
                ]),
            },
            // Admission atomically fences/converts the credential: the
            // candidate channel closes, a participant channel is minted.
            Effect::Upsert {
                table: "candidate_channels".into(),
                row: obj_pairs([
                    ("channel_id", json!(channel.channel_id)),
                    ("society_id", json!(channel.society_id)),
                    ("offer_ref", json!(channel.scope_ref)),
                    ("token", json!(channel.token)),
                    ("token_path", json!(channel.token_path)),
                    ("state", json!("closed")),
                    ("created_at", json!(created_at)),
                    ("closed_at", json!(created_at)),
                ]),
            },
            Effect::Upsert {
                table: "participant_channels".into(),
                row: obj_pairs([
                    ("channel_id", json!(participant_channel_id)),
                    ("society_id", json!(offer.society_id)),
                    ("participant_ref", json!(offer.participant_ref)),
                    ("token", json!(participant_token)),
                    ("token_path", json!(token_path)),
                    ("state", json!("open")),
                    ("created_at", json!(created_at)),
                    ("closed_at", Value::Null),
                ]),
            },
        ];
        effects.extend(policy_effects);
        let causation = causation_of(&req.meta);
        let correlation = correlation_of(&req.meta);
        let ev =
            |event_id: &str, kind: &str, object: &str, revision: u64, payload: Value| NewEvent {
                event_id: event_id.to_owned(),
                society_id: offer.society_id.clone(),
                kind: kind.to_owned(),
                object_ref: object.to_owned(),
                object_revision: revision,
                participant_ref: Some(offer.participant_ref.clone()),
                actor_ref: ACTOR_GOVERNANCE.into(),
                causation_ref: causation.clone(),
                correlation_ref: correlation.clone(),
                payload,
                visibility_scope_ref: "scope:society".into(),
            };
        let events = vec![
            ev(
                &admit_event,
                "membership.admitted",
                &offer.offer_id,
                new_offer_revision,
                json!({"decision_ref": req.admitted_by_decision_ref,
                       "acceptance_ref": req.membership_acceptance_ref,
                       "activated_self_policy_refs": activated_policy_refs.clone()}),
            ),
            ev(
                &standing_event,
                "standing.activated",
                &standing_id,
                1,
                json!({"participant_ref": offer.participant_ref,
                       "decision_ref": req.admitted_by_decision_ref}),
            ),
            ev(
                &channel_event,
                "channel.converted",
                &channel.channel_id,
                1,
                json!({"candidate_channel": "closed", "participant_channel": participant_channel_id}),
            ),
        ];
        Ok(Prepared {
            result: json!({
                "participant_ref": offer.participant_ref,
                "participant_revision": participant.revision + 1,
                "binding_epoch": new_binding_epoch,
                "participant_state": "active",
                "offer_ref": offer.offer_id,
                "offer_state": "admitted",
                "standing_ref": standing_id,
                "standing_status": "active",
                "activated_self_policy_refs": activated_policy_refs,
            }),
            revision: Some(new_offer_revision),
            cursor: CursorMint::AfterEvents {
                society_id: offer.society_id.clone(),
            },
            effects,
            events,
        })
    })?;
    ensure_channel_files(store);
    Ok(bytes)
}

// ------------------------------------------------ manifestation_admit ----

pub fn manifestation_admit(
    store: &mut Store,
    req: &ops::ManifestationAdmitRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    let manifestation = rows::get_manifestation(store.conn(), &req.manifestation_ref)
        .map_err(db_err)?
        .ok_or_else(state::not_found)?;
    check_meta_binding(store, &req.meta, &manifestation.society_id)?;

    let admit_event = mint(store, "evt")?;
    let scope = MutationScope {
        society_id: manifestation.society_id.clone(),
        operation: "manifestation_admit".into(),
        actor: ACTOR_GOVERNANCE.into(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let req = req.clone();
    run(store, scope, now, hooks, move |conn, _| {
        let m = rows::get_manifestation(conn, &req.manifestation_ref)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        if req.meta.expected_revision != Some(m.revision) {
            return Err(state::stale_revision());
        }
        if m.status != "proposed" {
            return Err(state::stale_binding("manifestation is not proposed"));
        }
        // Manifestation admission requires the Participant's Standing:
        // never before admission (§7.3).
        let participant = rows::get_participant(conn, &m.participant_ref)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        if participant.state != "active" {
            return Err(Problem::new(
                ProblemKind::AdmissionRequired,
                "manifestation admission requires an admitted Participant",
            )
            .with_status(409));
        }
        let new_revision = m.revision + 1;
        let effects = vec![Effect::Upsert {
            table: "manifestation_revisions".into(),
            row: obj_pairs([
                ("manifestation_id", json!(m.manifestation_id)),
                ("society_id", json!(m.society_id)),
                ("participant_ref", json!(m.participant_ref)),
                ("revision", json!(new_revision)),
                ("kind", json!(m.kind)),
                (
                    "body_digest",
                    serde_json::from_str(&m.body_digest).unwrap_or(Value::Null),
                ),
                ("status", json!("active")),
                (
                    "admitted_by_decision_ref",
                    json!(req.admitted_by_decision_ref),
                ),
                ("created_at", json!(m.created_at)),
            ]),
        }];
        let events = vec![NewEvent {
            event_id: admit_event.clone(),
            society_id: m.society_id.clone(),
            kind: "manifestation.admitted".into(),
            object_ref: m.manifestation_id.clone(),
            object_revision: new_revision,
            participant_ref: Some(m.participant_ref.clone()),
            actor_ref: ACTOR_GOVERNANCE.into(),
            causation_ref: causation_of(&req.meta),
            correlation_ref: correlation_of(&req.meta),
            payload: json!({"status": "active", "decision_ref": req.admitted_by_decision_ref,
                            "kind": m.kind}),
            visibility_scope_ref: "scope:society".into(),
        }];
        Ok(Prepared {
            result: json!({
                "manifestation_ref": m.manifestation_id,
                "revision": new_revision,
                "status": "active",
            }),
            revision: Some(new_revision),
            cursor: CursorMint::AfterEvents {
                society_id: m.society_id.clone(),
            },
            effects,
            events,
        })
    })
}

// ------------------------------------------------- server-time expiry ----

/// Deterministic offer expiry at `expires_at` (§7.4 `server_time`): its
/// own journaled authority transition — expiry races admission through
/// the same offer-revision CAS and performs the same authority fencing
/// without attributing a refusal to the candidate.
pub fn expire_offer_if_due(store: &mut Store, offer_id: &str, now: i64) -> Result<(), Problem> {
    let Some(offer) = rows::get_offer(store.conn(), offer_id).map_err(db_err)? else {
        return Ok(());
    };
    if !matches!(offer.state.as_str(), "offered" | "onboarding" | "accepted") {
        return Ok(());
    }
    let Some(expiry) = parse_rfc3339_utc(&offer.expires_at) else {
        return Ok(());
    };
    if expiry > now {
        return Ok(());
    }
    let incarnation = store
        .incarnation()
        .map_err(|e| state::internal(&e.to_string()))?;
    let epoch = store
        .recovery_epoch(&offer.society_id)
        .map_err(|e| state::internal(&e.to_string()))?;
    let expire_event = mint(store, "evt")?;
    let channel_event = mint(store, "evt")?;
    let scope = MutationScope {
        society_id: offer.society_id.clone(),
        operation: "server_time".into(),
        actor: ACTOR_SERVER.into(),
        meta: MutationMeta {
            request_id: format!("expire-{offer_id}"),
            idempotency_key: format!("expire-{offer_id}"),
            expected_endpoint_incarnation: incarnation,
            expected_recovery_epoch: epoch,
            expected_revision: Some(offer.revision),
            causation_event_ref: None,
            correlation_ref: None,
        },
        body: json!({"op": "server_time", "offer_ref": offer_id, "transition": "expired"}),
    };
    let offer_id = offer_id.to_owned();
    let closed_at = rfc3339_utc(now);
    let outcome = run(store, scope, now, CrashHooks::NONE, move |conn, scope| {
        let offer = rows::get_offer(conn, &offer_id)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        if !matches!(offer.state.as_str(), "offered" | "onboarding" | "accepted") {
            return Err(state::stale_revision());
        }
        let mut offer_row = offer.to_effect_row();
        offer_row.insert("state".into(), json!("expired"));
        offer_row.insert("revision".into(), json!(offer.revision + 1));
        offer_row.insert("fence_epoch".into(), json!(offer.fence_epoch + 1));
        let mut effects = vec![Effect::Upsert {
            table: "membership_offers".into(),
            row: offer_row,
        }];
        let mut events = vec![NewEvent {
            event_id: expire_event.clone(),
            society_id: offer.society_id.clone(),
            kind: "membership.expired".into(),
            object_ref: offer.offer_id.clone(),
            object_revision: offer.revision + 1,
            participant_ref: Some(offer.participant_ref.clone()),
            actor_ref: ACTOR_SERVER.into(),
            causation_ref: format!("req:{}", scope.meta.request_id),
            correlation_ref: scope.meta.request_id.clone(),
            payload: json!({"state": "expired", "expires_at": offer.expires_at}),
            visibility_scope_ref: "scope:society".into(),
        }];
        if let Some(channel) =
            rows::candidate_channel_for_offer(conn, &offer.offer_id).map_err(db_err)?
        {
            if channel.state == "open" {
                effects.push(Effect::Upsert {
                    table: "candidate_channels".into(),
                    row: obj_pairs([
                        ("channel_id", json!(channel.channel_id)),
                        ("society_id", json!(channel.society_id)),
                        ("offer_ref", json!(channel.scope_ref)),
                        ("token", json!(channel.token)),
                        ("token_path", json!(channel.token_path)),
                        ("state", json!("closed")),
                        ("created_at", json!(closed_at)),
                        ("closed_at", json!(closed_at)),
                    ]),
                });
                events.push(NewEvent {
                    event_id: channel_event.clone(),
                    society_id: offer.society_id.clone(),
                    kind: "channel.candidate_closed".into(),
                    object_ref: channel.channel_id.clone(),
                    object_revision: 1,
                    participant_ref: Some(offer.participant_ref.clone()),
                    actor_ref: ACTOR_SERVER.into(),
                    causation_ref: format!("req:{}", scope.meta.request_id),
                    correlation_ref: scope.meta.request_id.clone(),
                    payload: json!({"reason": "offer expired"}),
                    visibility_scope_ref: "scope:society".into(),
                });
            }
        }
        Ok(Prepared {
            result: json!({"offer_ref": offer.offer_id, "state": "expired"}),
            revision: Some(offer.revision + 1),
            cursor: CursorMint::None,
            effects,
            events,
        })
    });
    match outcome {
        Ok(_) => {
            ensure_channel_files(store);
            Ok(())
        }
        // The offer moved concurrently (or was already terminal): the
        // caller re-reads and answers from the current state.
        Err(p) if p.kind == ProblemKind::StaleRevision => Ok(()),
        Err(p) => Err(p),
    }
}

// ------------------------------------------------- channel token files ----

pub fn channels_dir(store: &Store) -> std::path::PathBuf {
    store.data_dir().join("channels")
}

/// Reconciles channel token files with channel state: an open channel's
/// sender-constrained token file exists (`0600`), a closed channel's is
/// removed. Idempotent and crash-safe: rerun after every channel
/// mutation and at startup.
pub fn ensure_channel_files(store: &Store) {
    let dir = channels_dir(store);
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    let channels: Vec<(String, String, String)> = {
        let Ok(mut stmt) = store.conn().prepare(
            "SELECT token_path, token, state FROM candidate_channels
             UNION ALL SELECT token_path, token, state FROM participant_channels",
        ) else {
            return;
        };
        let Ok(rows) = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))) else {
            return;
        };
        rows.flatten().collect()
    };
    for (path, token, channel_state) in channels {
        let path = std::path::PathBuf::from(path);
        if channel_state == "open" {
            if !path.exists() {
                if let Ok(mut f) = std::fs::File::create(&path) {
                    let _ = f.write_all(token.as_bytes());
                    let _ = f.write_all(b"\n");
                }
                let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
            }
        } else {
            let _ = std::fs::remove_file(&path);
        }
    }
}
