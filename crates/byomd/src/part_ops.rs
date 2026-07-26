//! Participant-surface mutations, part 1: self-policies (§7.3),
//! `continuity_root_update`, the mandate chain's participant side
//! (prepare / derive with §10.2 never-widening / position), activity
//! streams with ENFORCED mandate binding (§11.1), wake intents
//! (accepted-and-pending at I0), the §11.3 continuation head CAS, and
//! `participation_cease` (§7.4: self-only, unconditional, immediate).

use bpp_core::ops;
use bpp_core::problem::{Problem, ProblemKind};
use bpp_core::time::{parse_rfc3339_utc, rfc3339_utc};
use byom_store::effects::{Effect, NewEvent};
use byom_store::rows;
use byom_store::{CrashHooks, CursorMint, MutationScope, Prepared, Store};
use serde_json::{json, Map, Value};

use crate::gov_ops::{
    causation_of, check_meta_binding, correlation_of, db_err, digest_json, ensure_channel_files,
    mint, obj_pairs, run,
};
use crate::part_common::{
    self, digest_of, mint_position, prepare_trace, record_position, seats_from_json, seats_json,
    source_row, Caller, Seat,
};
use crate::state;

fn opt_json(v: &Option<String>) -> Value {
    v.as_ref().map(|s| json!(s)).unwrap_or(Value::Null)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn event(
    society_id: &str,
    event_id: &str,
    kind: &str,
    object: &str,
    revision: u64,
    participant: &str,
    actor: &str,
    meta: &bpp_core::envelope::MutationMeta,
    payload: Value,
) -> NewEvent {
    NewEvent {
        event_id: event_id.to_owned(),
        society_id: society_id.to_owned(),
        kind: kind.to_owned(),
        object_ref: object.to_owned(),
        object_revision: revision,
        participant_ref: Some(participant.to_owned()),
        actor_ref: actor.to_owned(),
        causation_ref: causation_of(meta),
        correlation_ref: correlation_of(meta),
        payload,
        visibility_scope_ref: "scope:society".into(),
    }
}

// ------------------------------------------------------ self-policies ----

/// assent_policy_adopt / activation_policy_adopt: the owning Participant
/// channel only; replacement adoption chains through `previous_digest`.
#[allow(clippy::too_many_arguments)]
pub fn self_policy_adopt(
    store: &mut Store,
    caller: &Caller,
    kind: &str,
    operation: &str,
    body_record: Value,
    adoption_mode: &str,
    previous_digest: Option<&bpp_core::digest::DigestRef>,
    effective_at: &str,
    expires_at: &str,
    meta: &bpp_core::envelope::MutationMeta,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    check_meta_binding(store, meta, &caller.society_id)?;
    // The participant channel carries direct provenance; a claimed
    // controller/candidate mode over it is not this channel's to assert.
    if adoption_mode != "direct_participant" {
        return Err(state::invalid(
            "the participant channel adopts with direct_participant provenance",
        ));
    }
    let policy_id = mint(store, "selfpol")?;
    let adopt_event = mint(store, "evt")?;
    let created_at = rfc3339_utc(now);
    let body_digest = store
        .record_digest(
            &caller.society_id,
            &policy_id,
            "bpp-self-policy-v0",
            &body_record,
        )
        .map_err(|e| state::internal(&e.to_string()))?;

    let scope = MutationScope {
        society_id: caller.society_id.clone(),
        operation: operation.to_owned(),
        actor: caller.actor.clone(),
        meta: meta.clone(),
        body: body.clone(),
    };
    let caller = caller.clone();
    let kind = kind.to_owned();
    let operation = operation.to_owned();
    let adoption_mode = adoption_mode.to_owned();
    let previous_digest = previous_digest.cloned();
    let (effective_at, expires_at) = (effective_at.to_owned(), expires_at.to_owned());
    let meta = meta.clone();
    run(store, scope, now, hooks, move |conn, _| {
        let current = rows::active_self_policy(conn, &caller.participant.participant_id, &kind)
            .map_err(db_err)?;
        let mut effects = Vec::new();
        let (revision, superseded_ref) = match &current {
            Some(head) => {
                let head_digest = rows::json_of(head, "body_digest");
                let cited = previous_digest
                    .as_ref()
                    .is_some_and(|d| d.same_ref_json(&head_digest));
                if !cited {
                    return Err(state::stale_binding(
                        "replacement adoption must cite the current policy digest in previous_digest",
                    ));
                }
                let mut superseded = head.clone();
                superseded.insert("status".into(), json!("superseded"));
                effects.push(Effect::Upsert {
                    table: "self_policies".into(),
                    row: superseded,
                });
                (
                    rows::u64_of(head, "revision") + 1,
                    Some(rows::str_of(head, "policy_id").to_owned()),
                )
            }
            None => {
                if previous_digest.is_some() {
                    return Err(state::stale_binding(
                        "previous_digest cites no active policy",
                    ));
                }
                (1, None)
            }
        };
        effects.push(Effect::Upsert {
            table: "self_policies".into(),
            row: obj_pairs([
                ("policy_id", json!(policy_id)),
                ("society_id", json!(caller.society_id)),
                ("participant_ref", json!(caller.participant.participant_id)),
                ("kind", json!(kind)),
                ("revision", json!(revision)),
                ("status", json!("active")),
                ("body", json!(body_record.to_string())),
                ("body_digest", json!(digest_json(&body_digest))),
                ("adoption_mode", json!(adoption_mode)),
                ("provenance", json!("participant_adopted")),
                (
                    "previous_policy_ref",
                    superseded_ref
                        .as_ref()
                        .map(|s| json!(s))
                        .unwrap_or(Value::Null),
                ),
                ("effective_at", json!(effective_at)),
                ("expires_at", json!(expires_at)),
                ("created_at", json!(created_at)),
            ]),
        });
        let events = vec![event(
            &caller.society_id,
            &adopt_event,
            "self-policy.adopted",
            &policy_id,
            revision,
            &caller.participant.participant_id,
            &caller.actor,
            &meta,
            json!({"kind": kind, "adoption_mode": adoption_mode,
                   "provenance": "participant_adopted", "operation": operation}),
        )];
        let mut result = json!({
            "policy_id": policy_id,
            "revision": revision,
            "status": "active",
            "digest": digest_json(&body_digest),
        });
        if let Some(superseded) = &superseded_ref {
            result["superseded_policy_ref"] = json!(superseded);
        }
        Ok(Prepared {
            result,
            revision: Some(revision),
            cursor: CursorMint::AfterEvents {
                society_id: caller.society_id.clone(),
            },
            effects,
            events,
        })
    })
}

/// assent_policy_revoke / activation_policy_revoke: owner-only.
#[allow(clippy::too_many_arguments)]
pub fn self_policy_revoke(
    store: &mut Store,
    caller: &Caller,
    kind: &str,
    operation: &str,
    req: &ops::PolicyRevokeRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    check_meta_binding(store, &req.meta, &caller.society_id)?;
    let revoke_event = mint(store, "evt")?;
    let scope = MutationScope {
        society_id: caller.society_id.clone(),
        operation: operation.to_owned(),
        actor: caller.actor.clone(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let caller = caller.clone();
    let kind = kind.to_owned();
    let req = req.clone();
    run(store, scope, now, hooks, move |conn, _| {
        let policy = rows::get_row(conn, "self_policies", "policy_id", &req.policy_ref)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        if rows::str_of(&policy, "participant_ref") != caller.participant.participant_id {
            // Owner-only: another participant's policy is not enumerable.
            return Err(state::not_found());
        }
        if rows::str_of(&policy, "kind") != kind {
            return Err(state::not_found());
        }
        if req.meta.expected_revision != Some(rows::u64_of(&policy, "revision")) {
            return Err(state::stale_revision());
        }
        if rows::str_of(&policy, "status") != "active" {
            return Err(state::stale_binding("policy is not active"));
        }
        let revision = rows::u64_of(&policy, "revision") + 1;
        let mut revoked = policy.clone();
        revoked.insert("status".into(), json!("revoked"));
        revoked.insert("revision".into(), json!(revision));
        let effects = vec![Effect::Upsert {
            table: "self_policies".into(),
            row: revoked,
        }];
        let events = vec![event(
            &caller.society_id,
            &revoke_event,
            "self-policy.revoked",
            &req.policy_ref,
            revision,
            &caller.participant.participant_id,
            &caller.actor,
            &req.meta,
            json!({"kind": kind}),
        )];
        Ok(Prepared {
            result: json!({
                "policy_ref": req.policy_ref,
                "revision": revision,
                "status": "revoked",
            }),
            revision: Some(revision),
            cursor: CursorMint::AfterEvents {
                society_id: caller.society_id.clone(),
            },
            effects,
            events,
        })
    })
}

// ------------------------------------------------ continuity_root_update ----

pub fn continuity_root_update(
    store: &mut Store,
    caller: &Caller,
    req: &ops::ContinuityRootUpdateRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    check_meta_binding(store, &req.meta, &caller.society_id)?;
    let minted_root = mint(store, "croot")?;
    let update_event = mint(store, "evt")?;
    let created_at = rfc3339_utc(now);
    let scope = MutationScope {
        society_id: caller.society_id.clone(),
        operation: "continuity_root_update".into(),
        actor: caller.actor.clone(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let caller = caller.clone();
    let req = req.clone();
    run(store, scope, now, hooks, move |conn, _| {
        let existing = rows::rows_where(
            conn,
            "continuity_roots",
            "participant_ref",
            &caller.participant.participant_id,
            "created_at",
        )
        .map_err(db_err)?;
        let existing = existing.into_iter().next();
        let (root_id, revision, prior_status) = match (&existing, &req.continuity_root_ref) {
            (None, None) => {
                // First adoption: absent → active only.
                if req.meta.expected_revision != Some(0) {
                    return Err(state::stale_revision());
                }
                if req.target_status != "active" {
                    return Err(state::stale_binding("first adoption targets active"));
                }
                (minted_root.clone(), 1, None)
            }
            (Some(root), Some(cited)) => {
                if rows::str_of(root, "root_id") != cited {
                    return Err(state::not_found());
                }
                if req.meta.expected_revision != Some(rows::u64_of(root, "revision")) {
                    return Err(state::stale_revision());
                }
                (
                    cited.clone(),
                    rows::u64_of(root, "revision") + 1,
                    Some(rows::str_of(root, "status").to_owned()),
                )
            }
            (Some(_), None) => {
                return Err(state::stale_binding(
                    "continuity_root_ref is absent exactly on first adoption",
                ))
            }
            (None, Some(_)) => return Err(state::not_found()),
        };
        // The closed status machine: active→active|sealed|retired,
        // sealed→retired; the Society never authors or unseals a root.
        match (prior_status.as_deref(), req.target_status.as_str()) {
            (None, "active")
            | (Some("active"), "active")
            | (Some("active"), "sealed")
            | (Some("active"), "retired")
            | (Some("sealed"), "retired") => {}
            _ => return Err(state::stale_binding("closed continuity-root transition")),
        }
        let body_record = json!({
            "opaque_provider_ref": opt_json(&req.opaque_provider_ref),
            "current_state_ref": opt_json(&req.current_state_ref),
            "current_state_digest": req.current_state_digest.as_ref()
                .map(digest_json).unwrap_or(Value::Null),
            "compatibility_selector": req.compatibility_selector.clone().unwrap_or(Value::Null),
            "classification_ref": opt_json(&req.classification_ref),
            "declared_influence_classes": req.declared_influence_classes.clone()
                .map(Value::from).unwrap_or(Value::Null),
            "retention_policy_ref": opt_json(&req.retention_policy_ref),
        });
        let effects = vec![Effect::Upsert {
            table: "continuity_roots".into(),
            row: obj_pairs([
                ("root_id", json!(root_id)),
                ("society_id", json!(caller.society_id)),
                ("participant_ref", json!(caller.participant.participant_id)),
                ("revision", json!(revision)),
                ("status", json!(req.target_status)),
                ("body", json!(body_record.to_string())),
                ("created_at", json!(created_at)),
            ]),
        }];
        let events = vec![event(
            &caller.society_id,
            &update_event,
            "continuity-root.updated",
            &root_id,
            revision,
            &caller.participant.participant_id,
            &caller.actor,
            &req.meta,
            json!({"status": req.target_status}),
        )];
        let mut result = json!({
            "continuity_root_id": root_id,
            "revision": revision,
            "status": req.target_status,
        });
        if let Some(d) = &req.current_state_digest {
            result["current_state_digest"] = digest_json(d);
        }
        Ok(Prepared {
            result,
            revision: Some(revision),
            cursor: CursorMint::AfterEvents {
                society_id: caller.society_id.clone(),
            },
            effects,
            events,
        })
    })
}

// ------------------------------------------------------ mandate chain ----

/// Builds the mandate subject value the digest commits to.
fn mandate_subject(mandate_id: &str, fields: &Value) -> Value {
    json!({"mandate_id": mandate_id, "scope": fields})
}

struct MandateMint {
    mandate_id: String,
    seat_ref: String,
    dependency_set_ref: String,
    event_id: String,
}

fn mint_mandate(store: &Store) -> Result<MandateMint, Problem> {
    Ok(MandateMint {
        mandate_id: mint(store, "mnd")?,
        seat_ref: mint(store, "seat-human")?,
        dependency_set_ref: mint(store, "deps")?,
        event_id: mint(store, "evt")?,
    })
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn mandate_row_and_result(
    store: &Store,
    caller: &Caller,
    minted: &MandateMint,
    sovereign: &str,
    scope_fields: Value,
    parent: Option<(&str, u64, Value)>,
    request_id: &str,
    body: &Value,
    now: i64,
) -> Result<(Map<String, Value>, Value, Vec<Seat>), Problem> {
    let subject = mandate_subject(&minted.mandate_id, &scope_fields);
    let subject_digest = store
        .mint_object_digest(
            &format!(
                "society-key:{}/object:{}",
                caller.society_id, minted.mandate_id
            ),
            "bpp-mandate-subject-v0",
            &subject,
        )
        .map_err(|e| state::internal(&e.to_string()))?;
    // The single required seat of the attached slice: the sovereign's
    // human-authority seat, filled on the governance surface (R17).
    let seats = vec![Seat {
        seat_ref: minted.seat_ref.clone(),
        kind: "human_authority".into(),
        participant_ref: sovereign.to_owned(),
        surface: "governance".into(),
    }];
    let mut field_sources = vec![
        source_row("/mandate_id", request_id, "/meta/request_id", "t-mint-id"),
        source_row("/scope", request_id, "", "t-copy-typed-scope"),
    ];
    if parent.is_some() {
        field_sources.push(source_row(
            "/scope/parent_mandate_ref",
            request_id,
            "/parent_mandate_ref",
            "t-copy",
        ));
    }
    let trace = prepare_trace(
        store,
        &caller.society_id,
        if parent.is_some() {
            "mandate_derive"
        } else {
            "mandate_prepare"
        },
        &caller.actor,
        request_id,
        body,
        &subject_digest,
        &minted.dependency_set_ref,
        field_sources,
        now,
    )?;
    let created_at = rfc3339_utc(now);
    let f = |k: &str| scope_fields[k].clone();
    let text = |v: &Value| json!(v.to_string());
    let row = obj_pairs([
        ("mandate_id", json!(minted.mandate_id)),
        ("society_id", json!(caller.society_id)),
        ("revision", json!(1)),
        ("state", json!("prepared")),
        ("grantee_participant_ref", f("grantee_participant_ref")),
        ("issuer_ref", f("issuer_ref")),
        ("purpose_ref", f("purpose_ref")),
        ("allowed_operations", text(&f("allowed_operations"))),
        ("resource_selectors", text(&f("resource_selectors"))),
        ("data_class_selectors", text(&f("data_class_selectors"))),
        ("destination_selectors", text(&f("destination_selectors"))),
        ("context_ceiling_ref", f("context_ceiling_ref")),
        ("budget_ceiling_set_ref", f("budget_ceiling_set_ref")),
        ("concurrency_ceiling", f("concurrency_ceiling")),
        (
            "manifestation_selector",
            if f("manifestation_selector").is_null() {
                Value::Null
            } else {
                text(&f("manifestation_selector"))
            },
        ),
        ("delegation", text(&f("delegation"))),
        ("pledge_ref", f("pledge_ref")),
        (
            "parent_mandate_ref",
            parent
                .as_ref()
                .map(|(p, _, _)| json!(p))
                .unwrap_or(Value::Null),
        ),
        ("subject_digest", json!(digest_json(&subject_digest))),
        ("required_seat_refs", json!(seats_json(&seats).to_string())),
        ("preparation_trace", json!(trace.to_string())),
        ("dependency_set_ref", json!(minted.dependency_set_ref)),
        ("decision_refs", Value::Null),
        ("issued_at", Value::Null),
        ("held_by_decision_ref", Value::Null),
        ("revoked_by_decision_ref", Value::Null),
        ("expires_at", f("expires_at")),
        ("created_at", json!(created_at)),
    ]);
    let mut result = json!({
        "mandate_id": minted.mandate_id,
        "revision": 1,
        "state": "prepared",
        "subject_digest": digest_json(&subject_digest),
        "preparation_trace_ref": trace["trace_id"],
        "preparation_trace_digest": trace["digest"],
        "required_seat_refs": [minted.seat_ref],
        "dependency_set_ref": minted.dependency_set_ref,
        "expires_at": f("expires_at"),
        "created_at": created_at,
        "preparation_trace": trace,
    });
    if let Some((parent_ref, parent_revision, parent_digest)) = parent {
        result["parent_mandate_ref"] = json!(parent_ref);
        result["parent_mandate_revision"] = json!(parent_revision);
        result["parent_mandate_digest"] = parent_digest;
    }
    Ok((row, result, seats))
}

pub fn mandate_prepare(
    store: &mut Store,
    caller: &Caller,
    req: &ops::MandatePrepareRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    check_meta_binding(store, &req.meta, &caller.society_id)?;
    let sovereign = rows::sovereign_participant(store.conn(), &caller.society_id)
        .map_err(db_err)?
        .ok_or_else(state::not_found)?;
    let minted = mint_mandate(store)?;
    let scope_fields = json!({
        "grantee_participant_ref": req.grantee_participant_ref,
        "issuer_ref": req.issuer_ref.clone().unwrap_or_else(|| sovereign.participant_id.clone()),
        "purpose_ref": req.purpose_ref,
        "allowed_operations": req.allowed_operations,
        "resource_selectors": req.resource_selectors,
        "data_class_selectors": req.data_class_selectors,
        "destination_selectors": req.destination_selectors,
        "context_ceiling_ref": opt_json(&req.context_ceiling_ref),
        "budget_ceiling_set_ref": req.budget_ceiling_set_ref,
        "concurrency_ceiling": req.concurrency_ceiling,
        "manifestation_selector": req.manifestation_selector.clone().unwrap_or(Value::Null),
        "delegation": serde_json::to_value(&req.delegation).unwrap_or(Value::Null),
        "pledge_ref": opt_json(&req.pledge_ref),
        "expires_at": req.expires_at,
    });
    let (row, result, _seats) = mandate_row_and_result(
        store,
        caller,
        &minted,
        &sovereign.participant_id,
        scope_fields,
        None,
        &req.meta.request_id,
        body,
        now,
    )?;
    let scope = MutationScope {
        society_id: caller.society_id.clone(),
        operation: "mandate_prepare".into(),
        actor: caller.actor.clone(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let caller = caller.clone();
    let req = req.clone();
    run(store, scope, now, hooks, move |conn, _| {
        // The proposed grantee must hold Standing (§10.1).
        let grantee = rows::get_participant(conn, &req.grantee_participant_ref)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        if grantee.state != "active" {
            return Err(state::stale_binding("grantee holds no active Standing"));
        }
        if parse_rfc3339_utc(&req.expires_at).is_some_and(|t| t <= now) {
            return Err(state::invalid("expires_at is already past"));
        }
        let events = vec![event(
            &caller.society_id,
            &minted.event_id,
            "mandate.prepared",
            &minted.mandate_id,
            1,
            &req.grantee_participant_ref,
            &caller.actor,
            &req.meta,
            json!({"state": "prepared", "purpose_ref": req.purpose_ref}),
        )];
        Ok(Prepared {
            result: result.clone(),
            revision: Some(1),
            cursor: CursorMint::AfterEvents {
                society_id: caller.society_id.clone(),
            },
            effects: vec![Effect::Upsert {
                table: "mandates".into(),
                row: row.clone(),
            }],
            events,
        })
    })
}

/// The §10.2 mechanical-subset checks of one derivation, against the
/// parent row. Every violation is `authority_widening`.
fn check_never_widening(parent: &Map<String, Value>, child_scope: &Value) -> Result<(), Problem> {
    let widening = |detail: &str| {
        Problem::new(
            ProblemKind::AuthorityWidening,
            "a child Mandate must be a mechanical subset of every parent",
        )
        .with_status(409)
        .with_detail(detail.to_owned())
    };
    let parent_set = |key: &str| -> std::collections::BTreeSet<String> {
        rows::json_of(parent, key)
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    };
    let child_set = |key: &str| -> std::collections::BTreeSet<String> {
        child_scope[key]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    };
    for key in [
        "allowed_operations",
        "resource_selectors",
        "data_class_selectors",
        "destination_selectors",
    ] {
        if !child_set(key).is_subset(&parent_set(key)) {
            return Err(widening(&format!("{key} is not a subset of the parent's")));
        }
    }
    // No later expiry.
    let parent_expiry = parse_rfc3339_utc(rows::str_of(parent, "expires_at"));
    let child_expiry = parse_rfc3339_utc(child_scope["expires_at"].as_str().unwrap_or_default());
    match (parent_expiry, child_expiry) {
        (Some(p), Some(c)) if c <= p => {}
        _ => return Err(widening("expires_at is later than the parent's")),
    }
    // No larger concurrency ceiling.
    if child_scope["concurrency_ceiling"].as_u64() > parent["concurrency_ceiling"].as_u64() {
        return Err(widening("concurrency_ceiling exceeds the parent's"));
    }
    // No greater delegation depth or fanout; parent must permit it.
    let parent_delegation = rows::json_of(parent, "delegation");
    let child_delegation = &child_scope["delegation"];
    if parent_delegation["allowed"].as_bool() != Some(true)
        || parent_delegation["max_depth"].as_u64().unwrap_or(0) == 0
    {
        return Err(widening("the parent delegation ceiling permits no child"));
    }
    let parent_depth = parent_delegation["max_depth"].as_u64().unwrap_or(0);
    if child_delegation["max_depth"].as_u64().unwrap_or(0) > parent_depth.saturating_sub(1) {
        return Err(widening("delegation depth exceeds the parent ceiling"));
    }
    if child_delegation["max_children"].as_u64() > parent_delegation["max_children"].as_u64() {
        return Err(widening("delegation fanout exceeds the parent ceiling"));
    }
    let parent_grantees: std::collections::BTreeSet<&str> = parent_delegation["grantee_selectors"]
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    if !parent_grantees.is_empty() {
        let child_grantees: std::collections::BTreeSet<&str> = child_delegation
            ["grantee_selectors"]
            .as_array()
            .map(|a| a.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        if !child_grantees.is_subset(&parent_grantees) {
            return Err(widening("grantee_selectors widen the parent ceiling"));
        }
    }
    // Purpose restricted to the parent purpose (attached slice: exact).
    if child_scope["purpose_ref"] != parent["purpose_ref"] {
        return Err(widening("purpose_ref is outside the parent purpose"));
    }
    // Manifestation selector: BPA-1 subset over the frozen AST.
    let parent_selector = rows::json_of(parent, "manifestation_selector");
    let child_selector = &child_scope["manifestation_selector"];
    if !child_selector.is_null() && !parent_selector.is_null() {
        match bpp_core::bpa1::is_subset(child_selector, &parent_selector) {
            Ok(true) => {}
            Ok(false) => return Err(widening("manifestation_selector widens the parent's")),
            Err(e) => {
                return Err(Problem::new(
                    ProblemKind::PolicyConflict,
                    "incomparable policy domains block derivation",
                )
                .with_status(409)
                .with_detail(e.to_string()))
            }
        }
    }
    if child_selector.is_null() && !parent_selector.is_null() {
        // Absent narrows to the parent's own selector (inherited).
    }
    Ok(())
}

pub fn mandate_derive(
    store: &mut Store,
    caller: &Caller,
    req: &ops::MandateDeriveRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    check_meta_binding(store, &req.meta, &caller.society_id)?;
    expire_mandate_if_due(store, &req.parent_mandate_ref, now)?;
    let sovereign = rows::sovereign_participant(store.conn(), &caller.society_id)
        .map_err(db_err)?
        .ok_or_else(state::not_found)?;
    let parent = rows::get_row(
        store.conn(),
        "mandates",
        "mandate_id",
        &req.parent_mandate_ref,
    )
    .map_err(db_err)?
    .ok_or_else(state::not_found)?;
    // Inherit absent scope members from the parent (never wider).
    let inherit_list = |child: &Option<Vec<String>>, key: &str| -> Value {
        match child {
            Some(list) => json!(list),
            None => rows::json_of(&parent, key),
        }
    };
    let child_scope = json!({
        "grantee_participant_ref": req.grantee_participant_ref,
        "issuer_ref": rows::str_of(&parent, "grantee_participant_ref"),
        "purpose_ref": req.purpose_ref,
        "allowed_operations": inherit_list(&req.allowed_operations, "allowed_operations"),
        "resource_selectors": inherit_list(&req.resource_selectors, "resource_selectors"),
        "data_class_selectors": inherit_list(&req.data_class_selectors, "data_class_selectors"),
        "destination_selectors": inherit_list(&req.destination_selectors, "destination_selectors"),
        "context_ceiling_ref": opt_json(&req.context_ceiling_ref),
        "budget_ceiling_set_ref": req.budget_ceiling_set_ref,
        "concurrency_ceiling": req.concurrency_ceiling,
        "manifestation_selector": req.manifestation_selector.clone().unwrap_or(Value::Null),
        "delegation": serde_json::to_value(&req.delegation).unwrap_or(Value::Null),
        "pledge_ref": opt_json(&req.pledge_ref),
        "expires_at": req.expires_at,
    });
    let minted = mint_mandate(store)?;
    let parent_digest_json = rows::json_of(&parent, "subject_digest");
    let (row, result, _seats) = mandate_row_and_result(
        store,
        caller,
        &minted,
        &sovereign.participant_id,
        child_scope.clone(),
        Some((
            &req.parent_mandate_ref,
            rows::u64_of(&parent, "revision"),
            parent_digest_json,
        )),
        &req.meta.request_id,
        body,
        now,
    )?;
    let scope = MutationScope {
        society_id: caller.society_id.clone(),
        operation: "mandate_derive".into(),
        actor: caller.actor.clone(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let caller = caller.clone();
    let req = req.clone();
    run(store, scope, now, hooks, move |conn, _| {
        let parent = rows::get_row(conn, "mandates", "mandate_id", &req.parent_mandate_ref)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        if req.parent_mandate_revision != rows::u64_of(&parent, "revision") {
            return Err(state::stale_revision());
        }
        let parent_digest = rows::json_of(&parent, "subject_digest");
        if !req.parent_mandate_digest.same_ref_json(&parent_digest) {
            return Err(state::stale_binding(
                "parent_mandate_digest does not pin the current parent",
            ));
        }
        if rows::str_of(&parent, "state") != "active" {
            return Err(state::stale_binding("parent mandate is not active"));
        }
        // Only the parent grantee derives within its own scope.
        if rows::str_of(&parent, "grantee_participant_ref") != caller.participant.participant_id {
            return Err(state::forbidden());
        }
        let grantee = rows::get_participant(conn, &req.grantee_participant_ref)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        if grantee.state != "active" {
            return Err(state::stale_binding("grantee holds no active Standing"));
        }
        check_never_widening(&parent, &child_scope)?;
        let events = vec![event(
            &caller.society_id,
            &minted.event_id,
            "mandate.derived",
            &minted.mandate_id,
            1,
            &req.grantee_participant_ref,
            &caller.actor,
            &req.meta,
            json!({"state": "prepared", "parent_mandate_ref": req.parent_mandate_ref}),
        )];
        Ok(Prepared {
            result: result.clone(),
            revision: Some(1),
            cursor: CursorMint::AfterEvents {
                society_id: caller.society_id.clone(),
            },
            effects: vec![Effect::Upsert {
                table: "mandates".into(),
                row: row.clone(),
            }],
            events,
        })
    })
}

/// mandate_position on the PARTICIPANT surface (R16): the attached
/// slice prepares only the sovereign's governance-surface human seat, so
/// every participant-surface position is `position_ineligible` — decided
/// by the prepared seat records, not by op absence.
pub fn mandate_position(
    store: &mut Store,
    caller: &Caller,
    surface: &str,
    req: &ops::PositionRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    check_meta_binding(store, &req.meta, &caller.society_id)?;
    let minted = mint_position(store)?;
    let scope = MutationScope {
        society_id: caller.society_id.clone(),
        operation: "mandate_position".into(),
        actor: caller.actor.clone(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let caller = caller.clone();
    let surface = surface.to_owned();
    let req = req.clone();
    run(store, scope, now, hooks, move |conn, _| {
        let mandate = rows::get_row(conn, "mandates", "mandate_id", &req.proposal_ref)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        if rows::str_of(&mandate, "state") != "prepared" {
            return Err(state::stale_binding("mandate is not in state prepared"));
        }
        let seats = seats_from_json(&rows::json_of(&mandate, "required_seat_refs"));
        let subject = rows::json_of(&mandate, "subject_digest");
        let (effects, result) = record_position(
            conn,
            &minted,
            "mandate",
            &caller.society_id,
            rows::u64_of(&mandate, "revision"),
            &digest_of(&subject)?,
            &seats,
            &req,
            &caller.participant.participant_id,
            &caller.actor,
            &surface,
            now,
        )?;
        let events = vec![event(
            &caller.society_id,
            &minted.event_id,
            "mandate.position_recorded",
            &req.proposal_ref,
            rows::u64_of(&mandate, "revision"),
            &caller.participant.participant_id,
            &caller.actor,
            &req.meta,
            json!({"seat_ref": req.seat_ref, "value": req.value}),
        )];
        Ok(Prepared {
            result,
            revision: None,
            cursor: CursorMint::AfterEvents {
                society_id: caller.society_id.clone(),
            },
            effects,
            events,
        })
    })
}

/// Deterministic server-time mandate expiry: its own journaled
/// transition, racing use through the same head CAS.
pub fn expire_mandate_if_due(store: &mut Store, mandate_id: &str, now: i64) -> Result<(), Problem> {
    let Some(mandate) =
        rows::get_row(store.conn(), "mandates", "mandate_id", mandate_id).map_err(db_err)?
    else {
        return Ok(());
    };
    if !matches!(rows::str_of(&mandate, "state"), "prepared" | "active") {
        return Ok(());
    }
    let Some(expiry) = parse_rfc3339_utc(rows::str_of(&mandate, "expires_at")) else {
        return Ok(());
    };
    if expiry > now {
        return Ok(());
    }
    let society_id = rows::str_of(&mandate, "society_id").to_owned();
    let incarnation = store
        .incarnation()
        .map_err(|e| state::internal(&e.to_string()))?;
    let epoch = store
        .recovery_epoch(&society_id)
        .map_err(|e| state::internal(&e.to_string()))?;
    let expire_event = mint(store, "evt")?;
    let scope = MutationScope {
        society_id: society_id.clone(),
        operation: "server_time".into(),
        actor: crate::gov_ops::ACTOR_SERVER.into(),
        meta: bpp_core::envelope::MutationMeta {
            request_id: format!("expire-{mandate_id}"),
            idempotency_key: format!("expire-mandate-{mandate_id}"),
            expected_endpoint_incarnation: incarnation,
            expected_recovery_epoch: epoch,
            expected_revision: Some(rows::u64_of(&mandate, "revision")),
            causation_event_ref: None,
            correlation_ref: None,
        },
        body: json!({"op": "server_time", "mandate_id": mandate_id, "transition": "expired"}),
    };
    let mandate_id = mandate_id.to_owned();
    let outcome = run(store, scope, now, CrashHooks::NONE, move |conn, scope| {
        let mandate = rows::get_row(conn, "mandates", "mandate_id", &mandate_id)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        if !matches!(rows::str_of(&mandate, "state"), "prepared" | "active") {
            return Err(state::stale_revision());
        }
        let revision = rows::u64_of(&mandate, "revision") + 1;
        let mut expired = mandate.clone();
        expired.insert("state".into(), json!("expired"));
        expired.insert("revision".into(), json!(revision));
        let grantee = rows::str_of(&mandate, "grantee_participant_ref").to_owned();
        Ok(Prepared {
            result: json!({"mandate_id": mandate_id, "state": "expired"}),
            revision: Some(revision),
            cursor: CursorMint::None,
            effects: vec![Effect::Upsert {
                table: "mandates".into(),
                row: expired,
            }],
            events: vec![NewEvent {
                event_id: expire_event.clone(),
                society_id: scope.society_id.clone(),
                kind: "mandate.expired".into(),
                object_ref: mandate_id.clone(),
                object_revision: revision,
                participant_ref: Some(grantee),
                actor_ref: crate::gov_ops::ACTOR_SERVER.into(),
                causation_ref: format!("req:{}", scope.meta.request_id),
                correlation_ref: scope.meta.request_id.clone(),
                payload: json!({"state": "expired"}),
                visibility_scope_ref: "scope:society".into(),
            }],
        })
    });
    match outcome {
        Ok(_) => Ok(()),
        Err(p) if p.kind == ProblemKind::StaleRevision => Ok(()),
        Err(p) => Err(p),
    }
}

// ------------------------------------------------- mandate use gate ----

/// The §11.1 mandate-binding gate at `activity_open`: refuses an absent,
/// held, revoked, expired, exhausted, or insufficient mandate — each with
/// its typed problem.
pub fn mandate_gate(
    conn: &rusqlite::Connection,
    mandate_id: &str,
    caller_participant: &str,
    purpose_ref: &str,
) -> Result<(), Problem> {
    let Some(mandate) =
        rows::get_row(conn, "mandates", "mandate_id", mandate_id).map_err(db_err)?
    else {
        return Err(state::not_found());
    };
    match rows::str_of(&mandate, "state") {
        "active" => {}
        "held" => {
            return Err(Problem::new(
                ProblemKind::MandateHeld,
                "the bound mandate is held; new uses are fenced",
            )
            .with_status(409))
        }
        "revoked" => return Err(state::stale_binding("the bound mandate is revoked")),
        "expired" => return Err(state::stale_binding("the bound mandate is expired")),
        "prepared" => {
            return Err(Problem::new(
                ProblemKind::DecisionIncomplete,
                "the bound mandate has not been issued",
            )
            .with_status(409))
        }
        _ => return Err(state::stale_binding("the bound mandate is not usable")),
    }
    if rows::str_of(&mandate, "grantee_participant_ref") != caller_participant {
        return Err(state::forbidden());
    }
    // Insufficient: outside the mandate's operation or purpose scope.
    let allowed = rows::json_of(&mandate, "allowed_operations");
    let allows_open = allowed
        .as_array()
        .is_some_and(|a| a.iter().any(|v| v.as_str() == Some("activity_open")));
    if !allows_open {
        return Err(state::forbidden_detail(
            "the mandate's allowed_operations do not cover activity_open",
        ));
    }
    if rows::str_of(&mandate, "purpose_ref") != purpose_ref {
        return Err(state::forbidden_detail(
            "the activity purpose is outside the mandate's purpose",
        ));
    }
    // Exhausted: the concurrency ceiling is a use ceiling at B1.
    let ceiling = rows::u64_of(&mandate, "concurrency_ceiling");
    let open = rows::open_activities_citing_mandate(conn, mandate_id).map_err(db_err)?;
    if open >= ceiling {
        return Err(part_common::budget_exceeded(
            mandate_id,
            "concurrency",
            open + 1,
            ceiling,
        ));
    }
    Ok(())
}

// --------------------------------------------------------- activities ----

pub fn activity_open(
    store: &mut Store,
    caller: &Caller,
    req: &ops::ActivityOpenRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    check_meta_binding(store, &req.meta, &caller.society_id)?;
    for mandate_ref in &req.mandate_refs {
        expire_mandate_if_due(store, mandate_ref, now)?;
    }
    let activity_id = mint(store, "act")?;
    let open_event = mint(store, "evt")?;
    let pledge_event = mint(store, "evt")?;
    let created_at = rfc3339_utc(now);
    let scope = MutationScope {
        society_id: caller.society_id.clone(),
        operation: "activity_open".into(),
        actor: caller.actor.clone(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let caller = caller.clone();
    let req = req.clone();
    run(store, scope, now, hooks, move |conn, _| {
        let mut effects = Vec::new();
        let mut events = Vec::new();
        let mut generation = 1u64;
        match (req.kind.as_str(), &req.pledge_binding) {
            ("pledge_work", Some(binding)) => {
                let pledge = rows::get_row(conn, "pledges", "pledge_id", &binding.pledge_id)
                    .map_err(db_err)?
                    .ok_or_else(state::not_found)?;
                if binding.pledge_revision != rows::u64_of(&pledge, "revision") {
                    return Err(state::stale_revision());
                }
                let terms = rows::json_of(&pledge, "terms_digest");
                if !binding.terms_digest.same_ref_json(&terms) {
                    return Err(state::invalid(
                        "pledge_binding.terms_digest does not pin the committed terms",
                    ));
                }
                if rows::str_of(&pledge, "pledgor_ref") != caller.participant.participant_id {
                    return Err(state::forbidden());
                }
                // Only a committed, un-opened obligation binds a fresh
                // workstream; an interrupted one resumes first
                // (pledge_resume: waiting → active).
                if rows::str_of(&pledge, "state") != "active" {
                    return Err(state::stale_binding("pledge is not openable"));
                }
                generation = rows::u64_of(&pledge, "workstream_generation") + 1;
                let revision = rows::u64_of(&pledge, "revision") + 1;
                let mut underway = pledge.clone();
                underway.insert("state".into(), json!("underway"));
                underway.insert("revision".into(), json!(revision));
                underway.insert("workstream_ref".into(), json!(activity_id));
                underway.insert("workstream_generation".into(), json!(generation));
                effects.push(Effect::Upsert {
                    table: "pledges".into(),
                    row: underway,
                });
                events.push(event(
                    &caller.society_id,
                    &pledge_event,
                    "pledge.underway",
                    &binding.pledge_id,
                    revision,
                    &caller.participant.participant_id,
                    &caller.actor,
                    &req.meta,
                    json!({"workstream_ref": activity_id, "generation": generation}),
                ));
            }
            ("pledge_work", None) => {
                return Err(state::invalid(
                    "pledge_work requires the exact committed pledge_binding",
                ))
            }
            (_, Some(_)) => {
                return Err(state::invalid(
                    "pledge_binding binds only a pledge_work stream",
                ))
            }
            (_, None) => {
                // §11.1/B1: the mandate chain comes before any
                // non-pledged ActivityStream.
                if req.mandate_refs.is_empty() {
                    return Err(state::forbidden_detail(
                        "a non-pledged ActivityStream requires a bound mandate",
                    ));
                }
            }
        }
        for mandate_ref in &req.mandate_refs {
            mandate_gate(
                conn,
                mandate_ref,
                &caller.participant.participant_id,
                &req.purpose_ref,
            )?;
        }
        effects.push(Effect::Upsert {
            table: "activity_streams".into(),
            row: obj_pairs([
                ("activity_stream_id", json!(activity_id)),
                ("society_id", json!(caller.society_id)),
                ("participant_ref", json!(caller.participant.participant_id)),
                ("generation", json!(generation)),
                ("revision", json!(1)),
                ("kind", json!(req.kind)),
                ("state", json!("ready")),
                ("purpose_ref", json!(req.purpose_ref)),
                ("purpose_digest", json!(digest_json(&req.purpose_digest))),
                (
                    "pledge_binding",
                    req.pledge_binding
                        .as_ref()
                        .and_then(|b| serde_json::to_value(b).ok())
                        .map(|v| json!(v.to_string()))
                        .unwrap_or(Value::Null),
                ),
                (
                    "activation_policy_ref",
                    opt_json(&req.activation_policy_ref),
                ),
                ("mandate_refs", json!(json!(req.mandate_refs).to_string())),
                ("budget_account_set_ref", json!(req.budget_account_set_ref)),
                ("continuation_head_ref", Value::Null),
                ("continuation_head_revision", json!(0)),
                ("created_at", json!(created_at)),
            ]),
        });
        events.push(event(
            &caller.society_id,
            &open_event,
            "activity.opened",
            &activity_id,
            1,
            &caller.participant.participant_id,
            &caller.actor,
            &req.meta,
            json!({"kind": req.kind, "state": "ready", "generation": generation,
                   "mandate_refs": req.mandate_refs}),
        ));
        Ok(Prepared {
            result: json!({
                "activity_stream_id": activity_id,
                "generation": generation,
                "revision": 1,
                "kind": req.kind,
                "state": "ready",
                "created_at": created_at,
            }),
            revision: Some(1),
            cursor: CursorMint::AfterEvents {
                society_id: caller.society_id.clone(),
            },
            effects,
            events,
        })
    })
}

fn owned_activity(
    conn: &rusqlite::Connection,
    caller: &Caller,
    activity_ref: &str,
    generation: u64,
) -> Result<Map<String, Value>, Problem> {
    let activity = rows::get_row(conn, "activity_streams", "activity_stream_id", activity_ref)
        .map_err(db_err)?
        .ok_or_else(state::not_found)?;
    if rows::str_of(&activity, "participant_ref") != caller.participant.participant_id {
        return Err(state::not_found());
    }
    if rows::u64_of(&activity, "generation") != generation {
        return Err(state::stale_binding("stale activity generation fence"));
    }
    Ok(activity)
}

pub fn activity_hold(
    store: &mut Store,
    caller: &Caller,
    req: &ops::ActivityHoldRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    check_meta_binding(store, &req.meta, &caller.society_id)?;
    let hold_event = mint(store, "evt")?;
    let scope = MutationScope {
        society_id: caller.society_id.clone(),
        operation: "activity_hold".into(),
        actor: caller.actor.clone(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let caller = caller.clone();
    let req = req.clone();
    run(store, scope, now, hooks, move |conn, _| {
        let activity = owned_activity(conn, &caller, &req.activity_stream_ref, req.generation)?;
        if req.meta.expected_revision != Some(rows::u64_of(&activity, "revision")) {
            return Err(state::stale_revision());
        }
        if !matches!(
            rows::str_of(&activity, "state"),
            "ready" | "active" | "waiting" | "reviewing"
        ) {
            return Err(state::stale_binding("activity is not holdable"));
        }
        let revision = rows::u64_of(&activity, "revision") + 1;
        let mut held = activity.clone();
        held.insert("state".into(), json!("held"));
        held.insert("revision".into(), json!(revision));
        Ok(Prepared {
            result: json!({
                "activity_stream_id": req.activity_stream_ref,
                "generation": req.generation,
                "revision": revision,
                "state": "held",
            }),
            revision: Some(revision),
            cursor: CursorMint::AfterEvents {
                society_id: caller.society_id.clone(),
            },
            effects: vec![Effect::Upsert {
                table: "activity_streams".into(),
                row: held,
            }],
            events: vec![event(
                &caller.society_id,
                &hold_event,
                "activity.held",
                &req.activity_stream_ref,
                revision,
                &caller.participant.participant_id,
                &caller.actor,
                &req.meta,
                json!({"state": "held"}),
            )],
        })
    })
}

pub fn activity_close(
    store: &mut Store,
    caller: &Caller,
    req: &ops::ActivityCloseRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    check_meta_binding(store, &req.meta, &caller.society_id)?;
    let close_event = mint(store, "evt")?;
    let pledge_wait_event = mint(store, "evt")?;
    let scope = MutationScope {
        society_id: caller.society_id.clone(),
        operation: "activity_close".into(),
        actor: caller.actor.clone(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let caller = caller.clone();
    let req = req.clone();
    run(store, scope, now, hooks, move |conn, _| {
        let activity = owned_activity(conn, &caller, &req.activity_stream_ref, req.generation)?;
        if req.meta.expected_revision != Some(rows::u64_of(&activity, "revision")) {
            return Err(state::stale_revision());
        }
        if !matches!(
            rows::str_of(&activity, "state"),
            "ready" | "active" | "waiting" | "reviewing" | "held"
        ) {
            return Err(state::stale_binding("activity is already terminal"));
        }
        let revision = rows::u64_of(&activity, "revision") + 1;
        let mut closed = activity.clone();
        closed.insert("state".into(), json!(req.target_state));
        closed.insert("revision".into(), json!(revision));
        let mut effects = vec![Effect::Upsert {
            table: "activity_streams".into(),
            row: closed,
        }];
        let mut events = vec![event(
            &caller.society_id,
            &close_event,
            "activity.closed",
            &req.activity_stream_ref,
            revision,
            &caller.participant.participant_id,
            &caller.actor,
            &req.meta,
            json!({"state": req.target_state}),
        )];
        // Closing a pledge_work stream whose obligation is still underway
        // interrupts the work, never the obligation: the pledge parks at
        // `waiting` and only pledge_resume (pledgor-only) reactivates it.
        let binding = rows::json_of(&activity, "pledge_binding");
        if let Some(pledge_id) = binding["pledge_id"].as_str() {
            if let Some(pledge) =
                rows::get_row(conn, "pledges", "pledge_id", pledge_id).map_err(db_err)?
            {
                if rows::str_of(&pledge, "state") == "underway"
                    && rows::str_of(&pledge, "workstream_ref") == req.activity_stream_ref
                {
                    let pledge_revision = rows::u64_of(&pledge, "revision") + 1;
                    let mut waiting = pledge.clone();
                    waiting.insert("state".into(), json!("waiting"));
                    waiting.insert("revision".into(), json!(pledge_revision));
                    effects.push(Effect::Upsert {
                        table: "pledges".into(),
                        row: waiting,
                    });
                    events.push(event(
                        &caller.society_id,
                        &pledge_wait_event,
                        "pledge.waiting",
                        pledge_id,
                        pledge_revision,
                        &caller.participant.participant_id,
                        &caller.actor,
                        &req.meta,
                        json!({"state": "waiting",
                               "reason": "workstream closed before delivery"}),
                    ));
                }
            }
        }
        Ok(Prepared {
            result: json!({
                "activity_stream_id": req.activity_stream_ref,
                "generation": req.generation,
                "revision": revision,
                "state": req.target_state,
            }),
            revision: Some(revision),
            cursor: CursorMint::AfterEvents {
                society_id: caller.society_id.clone(),
            },
            effects,
            events,
        })
    })
}

// -------------------------------------------------------- wake intents ----

pub fn wake_intent_submit(
    store: &mut Store,
    caller: &Caller,
    req: &ops::WakeIntentSubmitRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    check_meta_binding(store, &req.meta, &caller.society_id)?;
    let intent_id = mint(store, "wake")?;
    let submit_event = mint(store, "evt")?;
    let created_at = rfc3339_utc(now);
    let scope = MutationScope {
        society_id: caller.society_id.clone(),
        operation: "wake_intent_submit".into(),
        actor: caller.actor.clone(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let caller = caller.clone();
    let req = req.clone();
    run(store, scope, now, hooks, move |conn, _| {
        let activity = owned_activity(conn, &caller, &req.activity_stream_ref, req.generation)?;
        if !matches!(
            rows::str_of(&activity, "state"),
            "ready" | "active" | "waiting" | "reviewing"
        ) {
            return Err(state::stale_binding(
                "the activity cannot accept wake intents",
            ));
        }
        if parse_rfc3339_utc(&req.expires_at).is_some_and(|t| t <= now) {
            return Err(state::invalid("expires_at is already past"));
        }
        // §11.1: UNIQUE(participant_ref, stable_wake_key) — enforced
        // here (see the schema V2 note on upsert-vs-UNIQUE).
        if rows::wake_intent_by_key(
            conn,
            &caller.participant.participant_id,
            &req.stable_wake_key,
        )
        .map_err(db_err)?
        .is_some()
        {
            return Err(state::stale_binding(
                "stable_wake_key already names a wake intent",
            ));
        }
        // A policy-derived intent must cite the participant's ACTIVE
        // activation policy; no other origin can author one (§11.1).
        if req.origin == "participant_activation_policy" {
            let cited = req
                .activation_policy_ref
                .as_deref()
                .ok_or_else(|| state::invalid("policy origin requires activation_policy_ref"))?;
            let active =
                rows::active_self_policy(conn, &caller.participant.participant_id, "activation")
                    .map_err(db_err)?;
            let ok = active
                .as_ref()
                .is_some_and(|p| rows::str_of(p, "policy_id") == cited);
            if !ok {
                return Err(state::stale_binding(
                    "activation_policy_ref is not the participant's active activation policy",
                ));
            }
        }
        let effects = vec![Effect::Upsert {
            table: "wake_intents".into(),
            row: obj_pairs([
                ("wake_intent_id", json!(intent_id)),
                ("society_id", json!(caller.society_id)),
                ("participant_ref", json!(caller.participant.participant_id)),
                ("activity_stream_ref", json!(req.activity_stream_ref)),
                ("generation", json!(req.generation)),
                ("revision", json!(1)),
                ("origin", json!(req.origin)),
                (
                    "activation_policy_ref",
                    opt_json(&req.activation_policy_ref),
                ),
                ("exact_cause_ref", json!(req.exact_cause_ref)),
                (
                    "exact_cause_digest",
                    json!(digest_json(&req.exact_cause_digest)),
                ),
                ("purpose_ref", json!(req.purpose_ref)),
                ("stable_wake_key", json!(req.stable_wake_key)),
                ("state", json!("submitted")),
                ("expires_at", json!(req.expires_at)),
                ("created_at", json!(created_at)),
            ]),
        }];
        // Accepted and left PENDING: activation_admit/resource_allocate
        // are internal kernel transitions absent from this slice — no
        // admission, no placement, no episode.
        let events = vec![event(
            &caller.society_id,
            &submit_event,
            "wake-intent.submitted",
            &intent_id,
            1,
            &caller.participant.participant_id,
            &caller.actor,
            &req.meta,
            json!({"state": "submitted", "origin": req.origin,
                   "pending": "no activation machinery in this slice (I0)"}),
        )];
        Ok(Prepared {
            result: json!({
                "wake_intent_id": intent_id,
                "revision": 1,
                "activity_stream_ref": req.activity_stream_ref,
                "generation": req.generation,
                "origin": req.origin,
                "stable_wake_key": req.stable_wake_key,
                "state": "submitted",
                "submitted_at": created_at,
                "expires_at": req.expires_at,
            }),
            revision: Some(1),
            cursor: CursorMint::AfterEvents {
                society_id: caller.society_id.clone(),
            },
            effects,
            events,
        })
    })
}

pub fn wake_intent_withdraw(
    store: &mut Store,
    caller: &Caller,
    req: &ops::WakeIntentWithdrawRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    check_meta_binding(store, &req.meta, &caller.society_id)?;
    let withdraw_event = mint(store, "evt")?;
    let scope = MutationScope {
        society_id: caller.society_id.clone(),
        operation: "wake_intent_withdraw".into(),
        actor: caller.actor.clone(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let caller = caller.clone();
    let req = req.clone();
    run(store, scope, now, hooks, move |conn, _| {
        let intent = rows::get_row(conn, "wake_intents", "wake_intent_id", &req.wake_intent_ref)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        if rows::str_of(&intent, "participant_ref") != caller.participant.participant_id {
            return Err(state::not_found());
        }
        if req.meta.expected_revision != Some(rows::u64_of(&intent, "revision")) {
            return Err(state::stale_revision());
        }
        if rows::str_of(&intent, "state") != "submitted" {
            return Err(state::stale_binding("wake intent is not pending"));
        }
        let revision = rows::u64_of(&intent, "revision") + 1;
        let mut withdrawn = intent.clone();
        withdrawn.insert("state".into(), json!("withdrawn"));
        withdrawn.insert("revision".into(), json!(revision));
        Ok(Prepared {
            result: json!({
                "wake_intent_id": req.wake_intent_ref,
                "revision": revision,
                "state": "withdrawn",
            }),
            revision: Some(revision),
            cursor: CursorMint::AfterEvents {
                society_id: caller.society_id.clone(),
            },
            effects: vec![Effect::Upsert {
                table: "wake_intents".into(),
                row: withdrawn,
            }],
            events: vec![event(
                &caller.society_id,
                &withdraw_event,
                "wake-intent.withdrawn",
                &req.wake_intent_ref,
                revision,
                &caller.participant.participant_id,
                &caller.actor,
                &req.meta,
                json!({"state": "withdrawn"}),
            )],
        })
    })
}

// -------------------------------------------------- continuation head ----

pub fn continuation_write(
    store: &mut Store,
    caller: &Caller,
    req: &ops::ContinuationWriteRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    check_meta_binding(store, &req.meta, &caller.society_id)?;
    let continuation_id = mint(store, "cont")?;
    let write_event = mint(store, "evt")?;
    let created_at = rfc3339_utc(now);
    let scope = MutationScope {
        society_id: caller.society_id.clone(),
        operation: "continuation_write".into(),
        actor: caller.actor.clone(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let caller = caller.clone();
    let req = req.clone();
    run(store, scope, now, hooks, move |conn, _| {
        let activity = owned_activity(conn, &caller, &req.activity_stream_ref, req.generation)?;
        if matches!(
            rows::str_of(&activity, "state"),
            "completed" | "failed" | "canceled"
        ) {
            return Err(state::stale_binding("activity is terminal"));
        }
        // §11.3: a STALE Episode/Manifestation may retain its bytes as
        // local diagnostic evidence but cannot advance the head. When the
        // writer cites an Episode it must present that Episode's CURRENT
        // Byom lease fence; a superseded attempt is refused here, before
        // any CAS.
        if let Some(episode_ref) = &req.episode_ref {
            let episode = rows::get_row(conn, "episodes", "episode_id", episode_ref)
                .map_err(db_err)?
                .ok_or_else(state::not_found)?;
            if rows::str_of(&episode, "activity_stream_ref") != req.activity_stream_ref
                || rows::u64_of(&episode, "generation") != req.generation
            {
                return Err(state::stale_binding(
                    "the cited Episode does not belong to this ActivityStream generation",
                ));
            }
            let fence = req.byom_fence_epoch.ok_or_else(|| {
                state::invalid("citing an Episode requires its byom_fence_epoch (§11.3)")
            })?;
            let lease = rows::get_row(conn, "episode_lease_heads", "episode_id", episode_ref)
                .map_err(db_err)?
                .ok_or_else(|| {
                    crate::episode_ops::stale_lease("the cited Episode holds no lease")
                })?;
            if rows::u64_of(&lease, "byom_fence_epoch") != fence {
                return Err(crate::episode_ops::stale_lease(
                    "stale byom_fence_epoch: a superseded Episode attempt may keep its bytes as \
                     local evidence but cannot append a continuation (§11.3)",
                ));
            }
        }
        let head_revision = rows::u64_of(&activity, "continuation_head_revision");
        if req.expected_head_revision != head_revision {
            return Err(
                Problem::new(ProblemKind::StaleRevision, "continuation head conflict")
                    .with_status(409)
                    .with_detail(format!(
                        "expected_head_revision {} is stale; the current head is revision {} \
                 (opaque); reconcile and deliberately prepare a successor — Byom never \
                 auto-merges private state or silently selects a branch",
                        req.expected_head_revision, head_revision
                    )),
            );
        }
        let current_head = rows::str_of(&activity, "continuation_head_ref");
        match (&req.prior_continuation_ref, head_revision) {
            (None, 0) => {}
            (Some(cited), rev) if rev > 0 && cited == current_head => {}
            _ => {
                return Err(state::stale_binding(
                    "the predecessor is absent exactly at head revision zero and \
                     otherwise cites the exact current head",
                ))
            }
        }
        let new_head = head_revision + 1;
        let continuation_body = json!({
            "summary_ref": req.summary_ref,
            "unresolved_refs": req.unresolved_refs,
            "exact_state_refs": req.exact_state_refs,
            "source_event_cursor": req.source_event_cursor,
            "classification_ref": req.classification_ref,
        });
        let digest = digest_json(&part_common::conn_record_digest(
            conn,
            &caller.society_id,
            &continuation_id,
            "bpp-continuation-v0",
            &continuation_body,
        )?);
        let mut effects = vec![Effect::Upsert {
            table: "continuations".into(),
            row: obj_pairs([
                ("continuation_id", json!(continuation_id)),
                ("society_id", json!(caller.society_id)),
                ("activity_stream_ref", json!(req.activity_stream_ref)),
                ("generation", json!(req.generation)),
                ("sequence", json!(new_head)),
                ("head_revision", json!(new_head)),
                ("summary_ref", json!(req.summary_ref)),
                ("body", json!(continuation_body.to_string())),
                ("digest", digest.clone()),
                (
                    "prior_continuation_ref",
                    opt_json(&req.prior_continuation_ref),
                ),
                ("created_at", json!(created_at)),
            ]),
        }];
        let activity_revision = rows::u64_of(&activity, "revision") + 1;
        let mut advanced = activity.clone();
        advanced.insert("continuation_head_ref".into(), json!(continuation_id));
        advanced.insert("continuation_head_revision".into(), json!(new_head));
        advanced.insert("revision".into(), json!(activity_revision));
        effects.push(Effect::Upsert {
            table: "activity_streams".into(),
            row: advanced,
        });
        Ok(Prepared {
            result: json!({
                "continuation_id": continuation_id,
                "activity_stream_id": req.activity_stream_ref,
                "generation": req.generation,
                "sequence": new_head,
                "head_revision": new_head,
                "created_at": created_at,
                "digest": digest,
            }),
            revision: Some(new_head),
            cursor: CursorMint::AfterEvents {
                society_id: caller.society_id.clone(),
            },
            effects,
            events: vec![event(
                &caller.society_id,
                &write_event,
                "continuation.written",
                &continuation_id,
                new_head,
                &caller.participant.participant_id,
                &caller.actor,
                &req.meta,
                json!({"head_revision": new_head,
                       "classification_ref": req.classification_ref}),
            )],
        })
    })
}

// --------------------------------------------------- participation_cease ----

pub fn participation_cease(
    store: &mut Store,
    caller: &Caller,
    req: &ops::ParticipationCeaseRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    check_meta_binding(store, &req.meta, &caller.society_id)?;
    let cease_event = mint(store, "evt")?;
    let standing_event = mint(store, "evt")?;
    let channel_event = mint(store, "evt")?;
    let ceased_at = rfc3339_utc(now);
    let scope = MutationScope {
        society_id: caller.society_id.clone(),
        operation: "participation_cease".into(),
        actor: caller.actor.clone(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    // The idempotency domain of THIS cease: the one receipt the fenced
    // participant channel replays afterwards (BY-C2).
    let cease_domain = store
        .domain_digest(&scope)
        .map_err(|e| state::internal(&e.to_string()))?
        .value_hex;
    let caller = caller.clone();
    let req = req.clone();
    let bytes = run(store, scope, now, hooks, move |conn, _| {
        let participant = rows::get_participant(conn, &caller.participant.participant_id)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        if req.meta.expected_revision != Some(participant.revision) {
            return Err(state::stale_revision());
        }
        if participant.state != "active" {
            return Err(state::stale_binding("participant is not active"));
        }
        let standing_ref = participant
            .standing_ref
            .clone()
            .ok_or_else(|| state::internal("active participant without standing"))?;
        let revision = participant.revision + 1;
        let mut effects = vec![Effect::Upsert {
            table: "participants".into(),
            row: obj_pairs([
                ("participant_id", json!(participant.participant_id)),
                ("society_id", json!(participant.society_id)),
                ("kind", json!(participant.kind)),
                ("revision", json!(revision)),
                ("binding_epoch", json!(participant.binding_epoch + 1)),
                (
                    "display_profile_ref",
                    json!(participant.display_profile_ref),
                ),
                ("standing_ref", json!(standing_ref)),
                ("state", json!("retiring")),
                ("created_at", json!(participant.created_at)),
            ]),
        }];
        let standing = rows::get_row(conn, "standing_revisions", "standing_id", &standing_ref)
            .map_err(db_err)?;
        if let Some(standing) = standing {
            let mut ceased = standing.clone();
            ceased.insert("status".into(), json!("ceased"));
            ceased.insert(
                "revision".into(),
                json!(rows::u64_of(&standing, "revision") + 1),
            );
            effects.push(Effect::Upsert {
                table: "standing_revisions".into(),
                row: ceased,
            });
        }
        let mut events = vec![
            event(
                &caller.society_id,
                &cease_event,
                "participation.ceased",
                &participant.participant_id,
                revision,
                &participant.participant_id,
                &caller.actor,
                &req.meta,
                json!({
                    "state": "retiring",
                    "statement_ref": opt_json(&req.statement_ref),
                    "fencing": "new positions, derived assents, wakeups, mandates, \
                                disclosures and effects are blocked immediately",
                    "obligations": "accepted Pledges are dispositioned independently \
                                    under their own cancellation terms",
                }),
            ),
            event(
                &caller.society_id,
                &standing_event,
                "standing.ceased",
                &standing_ref,
                revision,
                &participant.participant_id,
                &caller.actor,
                &req.meta,
                json!({"status": "ceased"}),
            ),
        ];
        // Immediate credential fencing: the participant channel closes in
        // the same authority transition.
        if let Some(channel) = &caller.channel {
            effects.push(Effect::Upsert {
                table: "participant_channels".into(),
                row: crate::gov_ops::channel_row(
                    "participant_channels",
                    &channel.channel_id,
                    &channel.society_id,
                    &channel.scope_ref,
                    &channel.token,
                    &channel.token_path,
                    "closed",
                    &ceased_at,
                    Some("participation_cease"),
                    Some(&cease_domain),
                ),
            });
            effects.push(Effect::Upsert {
                table: "channel_credentials".into(),
                row: crate::gov_ops::closed_credential(conn, &channel.channel_id, &ceased_at)?,
            });
            events.push(event(
                &caller.society_id,
                &channel_event,
                "channel.participant_closed",
                &channel.channel_id,
                1,
                &participant.participant_id,
                &caller.actor,
                &req.meta,
                json!({"reason": "participation ceased"}),
            ));
        }
        Ok(Prepared {
            result: json!({
                "participant_ref": participant.participant_id,
                "revision": revision,
                "participant_state": "retiring",
                "standing_ref": standing_ref,
                "standing_status": "ceased",
            }),
            revision: Some(revision),
            cursor: CursorMint::AfterEvents {
                society_id: caller.society_id.clone(),
            },
            effects,
            events,
        })
    })?;
    ensure_channel_files(store);
    Ok(bytes)
}
