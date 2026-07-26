//! Read-side handlers: the negotiation family (per-surface hello,
//! protocol_info, feature_info), the projection reads (society_show,
//! participant_show, activity_show, charter_history, snapshot_get,
//! events_read/wait, the SENSITIVE event_payload behind its committed
//! PrivacyAccessRecord), and the originating-surface recovery reads.
//! Reads never mutate and never carry meta; projection problems never
//! disclose hidden existence.

use bpp_core::envelope::Success;
use bpp_core::limits;
use bpp_core::ops;
use bpp_core::problem::{Problem, ProblemKind};
use bpp_core::registry;
use byom_store::privacy::{self, Access, Outcome};
use byom_store::{rows, MutationScope, Store, GENESIS_SCOPE};
use serde_json::{json, Value};

use crate::gov_ops::db_err;
use crate::socket::SocketSurface;
use crate::state;

fn ok_bytes(result: Value) -> Result<Vec<u8>, Problem> {
    serde_json::to_vec(&Success::new(result)).map_err(|e| state::internal(&e.to_string()))
}

/// The slice-1 implemented operations, advertised per §14.1 (a feature
/// is advertised only when fully implemented).
pub const SLICE1_OPS: [&str; 13] = [
    "hello",
    "protocol_info",
    "feature_info",
    "society_prepare",
    "society_bootstrap",
    "society_show",
    "membership_offer",
    "membership_offer_revoke",
    "membership_accept",
    "membership_refuse",
    "participant_admit",
    "manifestation_admit",
    "participant_show",
];

/// The slice-2 implemented operations (this attached slice): self-
/// policies, the mandate chain, governed work, activities, events and
/// the recovery-core reads.
pub const SLICE2_OPS: [&str; 42] = [
    "assent_policy_adopt",
    "assent_policy_revoke",
    "activation_policy_adopt",
    "activation_policy_revoke",
    "candidate_self_policy_propose",
    "continuity_root_update",
    "participation_cease",
    "mandate_prepare",
    "mandate_position",
    "mandate_issue",
    "mandate_derive",
    "mandate_hold",
    "mandate_revoke",
    "endeavor_propose",
    "endeavor_position",
    "endeavor_finalize",
    "endeavor_hold",
    "endeavor_release",
    "endeavor_close",
    "call_open",
    "call_withdraw",
    "pledge_propose",
    "pledge_position",
    "pledge_finalize",
    "pledge_amend",
    "pledge_resume",
    "pledge_relinquish",
    "delivery_submit",
    "review_record",
    "activity_open",
    "activity_show",
    "activity_hold",
    "activity_close",
    "wake_intent_submit",
    "wake_intent_withdraw",
    "continuation_write",
    "charter_propose",
    "charter_position",
    "charter_finalize",
    "charter_history",
    "snapshot_get",
    "events_wait",
];

/// The remaining slice-2 reads advertised with the events family.
pub const SLICE2_RECOVERY_OPS: [&str; 4] = [
    "event_payload",
    "idempotency_result",
    "cursor_recover",
    "recovery_checkpoint_show",
];

/// The B3 slice-1 host-integration operations (C2
/// `byom_governed_work_v1`; registry R39/R40/R42). Advertised as their
/// own feature bundle because §16.6 makes compatibility one explicit,
/// all-or-nothing bundle — a client either speaks the whole seam or none
/// of it.
pub const HOST_INTEGRATION_OPS: [&str; 3] = [
    "kovee_endeavor_form",
    "external_command_result_query",
    "external_command_terminalize",
];

/// The B3 slice-2 runtime bundle (C2 `byom_governed_work_v1`; §11.1-§11.4,
/// §13.2; registry R29/R30/R33/R35/R38): the four-stage activation's
/// participant entry point, the runtime-surface Episode lease commands,
/// measured usage, and the two reconciliation seats. Advertised as its own
/// feature bundle: a client either speaks the whole runtime seam or none
/// of it (§16.6 all-or-nothing compatibility).
pub const RUNTIME_OPS: [&str; 11] = [
    "episode_request",
    "placement_admit",
    "episode_claim",
    "episode_start",
    "checkpoint_commit",
    "episode_yield",
    "episode_complete",
    "episode_fail",
    "usage_report",
    "effect_outcome_admit",
    "effect_reconcile",
];

/// The §11.4 budget reconciliation seat (R38), advertised with the
/// runtime bundle it releases holds for.
pub const BUDGET_OPS: [&str; 1] = ["budget_reconcile"];

/// Is the operation implemented by THIS daemon (the honest feature_info
/// set and the conformance live-replay contract)?
pub fn implemented(op: &str) -> bool {
    SLICE1_OPS.contains(&op)
        || SLICE2_OPS.contains(&op)
        || SLICE2_RECOVERY_OPS.contains(&op)
        || HOST_INTEGRATION_OPS.contains(&op)
        || RUNTIME_OPS.contains(&op)
        || BUDGET_OPS.contains(&op)
        || op == "events_read"
}

pub fn hello(store: &Store, surface: SocketSurface) -> Result<Vec<u8>, Problem> {
    let incarnation = store
        .incarnation()
        .map_err(|e| state::internal(&e.to_string()))?;
    ok_bytes(json!({
        "versions": [bpp_core::PROTOCOL_VERSION],
        "surface": surface.name(),
        "endpoint_incarnation": incarnation,
    }))
}

pub fn protocol_info(_store: &Store) -> Result<Vec<u8>, Problem> {
    ok_bytes(json!({
        "versions": [bpp_core::PROTOCOL_VERSION],
        "limits": {
            "request_bytes_max": limits::REQUEST_MAX_BYTES,
            "response_bytes_max": limits::RESPONSE_MAX_BYTES,
            "identifier_bytes_max": limits::IDENTIFIER_MAX_BYTES,
            "mutation_list_items_max": limits::MUTATION_LIST_ITEMS_MAX,
            "events_page_items_max": limits::EVENTS_PAGE_ITEMS_MAX,
        },
        "limits_revision": limits::LIMITS_REVISION,
    }))
}

pub fn feature_info(store: &Store) -> Result<Vec<u8>, Problem> {
    // Honest advertisement: the implemented bundle subset plus the
    // journal recovery profile label (§15.3: developer recovery only,
    // never production rollback resistance). The registry stays the
    // frozen dispatch truth; feature_info narrows to what is
    // implemented.
    let mutation_ops: Vec<&str> = registry::all_rows()
        .iter()
        .map(|r| r.operation.as_str())
        .filter(|op| {
            implemented(op)
                && registry::all_rows()
                    .iter()
                    .any(|r| r.operation == *op && r.class != registry::OpClass::Read)
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut features = vec![
        json!({
            "feature": "b0.1-slice1",
            "operations": SLICE1_OPS,
        }),
        json!({
            "feature": "b0.1-slice2",
            "operations": SLICE2_OPS.to_vec(),
        }),
        json!({
            "feature": "byom_governed_work_v1",
            "operations": HOST_INTEGRATION_OPS,
        }),
        json!({
            "feature": "byom_governed_work_v1:runtime",
            "operations": RUNTIME_OPS.to_vec(),
        }),
        json!({
            "feature": "byom_governed_work_v1:budget-reconciliation",
            "operations": BUDGET_OPS,
        }),
        json!({
            "feature": "b0.1-events-recovery-core",
            "operations": ["events_read", "events_wait", "event_payload",
                           "idempotency_result", "cursor_recover",
                           "recovery_checkpoint_show"],
        }),
    ];
    if !store.sealed() {
        features.push(json!({
            "feature": "authority-journal:developer-recovery",
            "operations": mutation_ops,
        }));
    }
    ok_bytes(json!({ "features": features }))
}

fn digest_value(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or(Value::Null)
}

pub fn society_show(store: &Store, society_id: &str) -> Result<Vec<u8>, Problem> {
    let society = rows::get_society(store.conn(), society_id)
        .map_err(|e| state::internal(&e.to_string()))?
        .ok_or_else(state::not_found)?;
    let mut result = json!({
        "society_id": society.society_id,
        "revision": society.revision,
        "home_authority_ref": society.home_authority_ref,
        "charter_head_ref": society.charter_head_ref,
        "charter_head_digest": digest_value(&society.charter_head_digest),
        "classification_binding_ref": society.classification_binding_ref,
        "classification_binding_digest": digest_value(&society.classification_binding_digest),
        "root_budget_account_set_ref": society.root_budget_account_set_ref,
        "recovery_epoch": society.recovery_epoch,
        "state": society.state,
        "created_at": society.created_at,
    });
    if let Some(realm) = &society.kovee_realm_binding {
        result["kovee_realm_binding"] = Value::String(realm.clone());
    }
    if let Some(project) = &society.kovee_project_binding {
        result["kovee_project_binding"] = Value::String(project.clone());
    }
    ok_bytes(result)
}

pub fn participant_show(store: &Store, participant_ref: &str) -> Result<Vec<u8>, Problem> {
    let participant = rows::get_participant(store.conn(), participant_ref)
        .map_err(|e| state::internal(&e.to_string()))?
        .ok_or_else(state::not_found)?;
    // A proposed candidate has no Standing yet: it is not projected
    // (non-enumerating not_found; the closed result schema requires
    // standing_ref, which only exists from admission).
    let Some(standing_ref) = participant.standing_ref.clone() else {
        return Err(state::not_found());
    };
    ok_bytes(json!({
        "participant_id": participant.participant_id,
        "society_id": participant.society_id,
        "kind": participant.kind,
        "revision": participant.revision,
        "binding_epoch": participant.binding_epoch,
        "display_profile_ref": participant.display_profile_ref,
        "standing_ref": standing_ref,
        "state": participant.state,
        "created_at": participant.created_at,
    }))
}

pub fn activity_show(store: &Store, activity_stream_ref: &str) -> Result<Vec<u8>, Problem> {
    let activity = rows::get_row(
        store.conn(),
        "activity_streams",
        "activity_stream_id",
        activity_stream_ref,
    )
    .map_err(db_err)?
    .ok_or_else(state::not_found)?;
    ok_bytes(json!({
        "activity_stream_id": rows::str_of(&activity, "activity_stream_id"),
        "participant_ref": rows::str_of(&activity, "participant_ref"),
        "generation": rows::u64_of(&activity, "generation"),
        "revision": rows::u64_of(&activity, "revision"),
        "kind": rows::str_of(&activity, "kind"),
        "state": rows::str_of(&activity, "state"),
        "purpose_ref": rows::str_of(&activity, "purpose_ref"),
        "purpose_digest": rows::json_of(&activity, "purpose_digest"),
        "mandate_refs": rows::json_of(&activity, "mandate_refs"),
        "budget_account_set_ref": rows::str_of(&activity, "budget_account_set_ref"),
        "continuation_head_revision": rows::u64_of(&activity, "continuation_head_revision"),
        "created_at": rows::str_of(&activity, "created_at"),
    }))
}

/// charter_history (projection, read; paged 1..=256). `charter_id` must
/// name a charter this Society knows (the head ref, a revision body ref,
/// or the Society itself) — anything else is non-enumerating not_found.
pub fn charter_history(
    store: &Store,
    req: &ops::CharterHistoryRequest,
) -> Result<Vec<u8>, Problem> {
    let society = rows::sole_society(store.conn())
        .map_err(db_err)?
        .ok_or_else(state::not_found)?;
    let revisions = rows::rows_where(
        store.conn(),
        "charter_revisions",
        "society_id",
        &society.society_id,
        "revision",
    )
    .map_err(db_err)?;
    let known = req.charter_id == society.society_id
        || req.charter_id == society.charter_head_ref
        || revisions
            .iter()
            .any(|r| rows::str_of(r, "body_ref") == req.charter_id);
    if !known {
        return Err(state::not_found());
    }
    let offset = match &req.continuation {
        None => 0usize,
        Some(token) => token
            .strip_prefix("ch1-")
            .and_then(|s| s.parse::<usize>().ok())
            .ok_or_else(|| {
                Problem::new(ProblemKind::Invalid, "invalid continuation")
                    .with_status(400)
                    .with_detail("not a continuation this endpoint minted for this source")
            })?,
    };
    let page: Vec<Value> = revisions
        .iter()
        .skip(offset)
        .take(req.page_size as usize)
        .map(|r| {
            json!({
                "charter_revision_id": rows::str_of(r, "charter_revision_id"),
                "revision": rows::u64_of(r, "revision"),
                "body_ref": rows::str_of(r, "body_ref"),
                "body_digest": rows::json_of(r, "body_digest"),
                "state": rows::str_of(r, "state"),
                "adopted_by_decision_ref":
                    r.get("adopted_by_decision_ref").cloned().unwrap_or(Value::Null),
                "effective_at": r.get("effective_at").cloned().unwrap_or(Value::Null),
                "created_at": rows::str_of(r, "created_at"),
            })
        })
        .collect();
    let mut result = json!({
        "charter_id": req.charter_id,
        "revisions": page,
    });
    let next = offset + req.page_size as usize;
    if next < revisions.len() {
        result["continuation"] = json!(format!("ch1-{next}"));
    }
    ok_bytes(result)
}

const SNAPSHOT_KINDS: [&str; 6] = [
    "participants",
    "endeavors",
    "calls",
    "pledges",
    "activities",
    "mandates",
];

/// snapshot_get (projection, read): a consistent projection of the
/// Society's current record heads, filterable by kind and endeavor.
pub fn snapshot_get(store: &Store, req: &ops::SnapshotGetRequest) -> Result<Vec<u8>, Problem> {
    let society = rows::get_society(store.conn(), &req.society_id)
        .map_err(db_err)?
        .ok_or_else(state::not_found)?;
    let wanted = |kind: &str| -> bool {
        match &req.kinds {
            None => true,
            Some(kinds) => kinds.iter().any(|k| k == kind),
        }
    };
    let by_endeavor = |row: &serde_json::Map<String, Value>| -> bool {
        match &req.endeavor_ref {
            None => true,
            Some(e) => rows::str_of(row, "endeavor_ref") == e,
        }
    };
    let mut result = json!({
        "society_id": society.society_id,
        "revision": society.revision,
        "state": society.state,
        "charter_head_ref": society.charter_head_ref,
        "recovery_epoch": society.recovery_epoch,
        "as_of_event_sequence": society.next_event_sequence.saturating_sub(1),
    });
    let table_rows = |table: &str, order: &str| {
        rows::rows_where(
            store.conn(),
            table,
            "society_id",
            &society.society_id,
            order,
        )
        .map_err(db_err)
    };
    if wanted("participants") {
        let list: Vec<Value> = table_rows("participants", "created_at")?
            .iter()
            .map(|p| {
                json!({
                    "participant_id": rows::str_of(p, "participant_id"),
                    "kind": rows::str_of(p, "kind"),
                    "state": rows::str_of(p, "state"),
                    "revision": rows::u64_of(p, "revision"),
                })
            })
            .collect();
        result["participants"] = json!(list);
    }
    if wanted("endeavors") {
        let list: Vec<Value> = table_rows("endeavors", "created_at")?
            .iter()
            .filter(|e| match &req.endeavor_ref {
                None => true,
                Some(want) => rows::str_of(e, "endeavor_id") == want,
            })
            .map(|e| {
                json!({
                    "endeavor_id": rows::str_of(e, "endeavor_id"),
                    "state": rows::str_of(e, "state"),
                    "revision": rows::u64_of(e, "revision"),
                    "purpose_ref": rows::str_of(e, "purpose_ref"),
                })
            })
            .collect();
        result["endeavors"] = json!(list);
    }
    if wanted("calls") {
        let list: Vec<Value> = table_rows("calls", "created_at")?
            .iter()
            .filter(|c| by_endeavor(c))
            .map(|c| {
                json!({
                    "call_id": rows::str_of(c, "call_id"),
                    "endeavor_ref": rows::str_of(c, "endeavor_ref"),
                    "state": rows::str_of(c, "state"),
                })
            })
            .collect();
        result["calls"] = json!(list);
    }
    if wanted("pledges") {
        let list: Vec<Value> = table_rows("pledges", "created_at")?
            .iter()
            .filter(|p| by_endeavor(p))
            .map(|p| {
                json!({
                    "pledge_id": rows::str_of(p, "pledge_id"),
                    "endeavor_ref": rows::str_of(p, "endeavor_ref"),
                    "state": rows::str_of(p, "state"),
                    "revision": rows::u64_of(p, "revision"),
                    "pledgor_ref": rows::str_of(p, "pledgor_ref"),
                })
            })
            .collect();
        result["pledges"] = json!(list);
    }
    if wanted("activities") {
        let list: Vec<Value> = table_rows("activity_streams", "created_at")?
            .iter()
            .map(|a| {
                json!({
                    "activity_stream_id": rows::str_of(a, "activity_stream_id"),
                    "kind": rows::str_of(a, "kind"),
                    "state": rows::str_of(a, "state"),
                    "generation": rows::u64_of(a, "generation"),
                })
            })
            .collect();
        result["activities"] = json!(list);
    }
    if wanted("mandates") {
        let list: Vec<Value> = table_rows("mandates", "created_at")?
            .iter()
            .map(|m| {
                json!({
                    "mandate_id": rows::str_of(m, "mandate_id"),
                    "state": rows::str_of(m, "state"),
                    "revision": rows::u64_of(m, "revision"),
                    "grantee_participant_ref": rows::str_of(m, "grantee_participant_ref"),
                })
            })
            .collect();
        result["mandates"] = json!(list);
    }
    // An explicitly requested unknown kind is a shape error, not a
    // silent omission.
    if let Some(kinds) = &req.kinds {
        for k in kinds {
            if !SNAPSHOT_KINDS.contains(&k.as_str()) {
                return Err(state::invalid(&format!("unknown snapshot kind {k:?}")));
            }
        }
    }
    ok_bytes(result)
}

/// The §15.4 release rule for event_payload: the covering `allowed`
/// PrivacyAccessRecord must COMMIT before any sensitive byte is served;
/// a failed record write blocks the read; a denied read still chains a
/// record. `BYOMD_PRIVACY_FAIL` (test-only) injects a record-write
/// failure.
#[allow(clippy::too_many_arguments)]
fn record_access(
    store: &Store,
    society_id: &str,
    operation: &str,
    actor: &str,
    query: Value,
    count: u64,
    bytes: u64,
    outcome: Outcome,
    now: i64,
) -> Result<(), Problem> {
    let blocked = || {
        Problem::new(
            ProblemKind::Unavailable,
            "privacy_access_record_commit_failed",
        )
        .with_status(503)
        .with_detail(
            "the covering PrivacyAccessRecord did not commit; unlogged bytes are never served"
                .to_owned(),
        )
    };
    if std::env::var_os("BYOMD_PRIVACY_FAIL").is_some() {
        return Err(blocked());
    }
    privacy::append_record(
        store,
        &Access {
            society_id: society_id.to_owned(),
            operation: operation.to_owned(),
            purpose_ref: "purpose:projection-read".to_owned(),
            actor: actor.to_owned(),
            query,
            result_object_count: count,
            result_bytes: bytes,
            outcome,
        },
        now,
    )
    .map(|_| ())
    .map_err(|_| blocked())
}

/// event_payload (projection, read; SENSITIVE §15.4).
pub fn event_payload(
    store: &Store,
    actor: &str,
    req: &ops::EventPayloadRequest,
    now: i64,
) -> Result<Vec<u8>, Problem> {
    let query = json!({"op": "event_payload", "event_id": req.event_id,
        "payload_digest": req.payload_digest.as_ref()
            .and_then(|d| serde_json::to_value(d).ok()).unwrap_or(Value::Null)});
    let found = rows::event_payload_row(store.conn(), &req.event_id).map_err(db_err)?;
    let Some((society_id, kind, payload_text, digest_text)) = found else {
        // A denied sensitive read still chains a record — against the
        // sole Society scope, since the event names none.
        let society = rows::sole_society(store.conn()).map_err(db_err)?;
        if let Some(society) = society {
            record_access(
                store,
                &society.society_id,
                "event_payload",
                actor,
                query,
                0,
                0,
                Outcome::Denied,
                now,
            )?;
        }
        return Err(state::not_found());
    };
    let digest: Value = serde_json::from_str(&digest_text).unwrap_or(Value::Null);
    if let Some(pinned) = &req.payload_digest {
        if !pinned.same_ref_json(&digest) {
            record_access(
                store,
                &society_id,
                "event_payload",
                actor,
                query,
                0,
                0,
                Outcome::Denied,
                now,
            )?;
            return Err(state::stale_binding(
                "payload_digest does not pin this event's payload",
            ));
        }
    }
    // The covering allowed record commits BEFORE the bytes are served.
    record_access(
        store,
        &society_id,
        "event_payload",
        actor,
        query,
        1,
        payload_text.len() as u64,
        Outcome::Allowed,
        now,
    )?;
    let payload: Value = serde_json::from_str(&payload_text).unwrap_or(Value::Null);
    ok_bytes(json!({
        "event_id": req.event_id,
        "kind": kind,
        "payload": payload,
        "payload_digest": digest,
    }))
}

/// idempotency_result (originating surface, read; R41): looks up the
/// retained receipt for (channel-derived actor, operation, key) — NEVER
/// re-executes.
pub fn idempotency_result(
    store: &Store,
    actor: &str,
    req: &ops::IdempotencyResultRequest,
) -> Result<Vec<u8>, Problem> {
    let mut scopes: Vec<String> = Vec::new();
    if let Some(society) = rows::sole_society(store.conn()).map_err(db_err)? {
        scopes.push(society.society_id);
    }
    scopes.push(GENESIS_SCOPE.to_owned());
    for society_id in scopes {
        let scope = MutationScope {
            society_id,
            operation: req.operation.clone(),
            actor: actor.to_owned(),
            meta: bpp_core::envelope::MutationMeta {
                request_id: "idempotency-result".to_owned(),
                idempotency_key: req.idempotency_key.clone(),
                expected_endpoint_incarnation: String::new(),
                expected_recovery_epoch: 0,
                expected_revision: None,
                causation_event_ref: None,
                correlation_ref: None,
            },
            body: Value::Null,
        };
        let digest = store
            .domain_digest(&scope)
            .map_err(|e| state::internal(&e.to_string()))?;
        if let Some((_request_digest, result)) = store
            .lookup_idempotency(&digest.value_hex)
            .map_err(|e| state::internal(&e.to_string()))?
        {
            let retained: Value =
                serde_json::from_slice(&result).map_err(|e| state::internal(&e.to_string()))?;
            return ok_bytes(json!({
                "operation": req.operation,
                "idempotency_key": req.idempotency_key,
                "state": "retained",
                "result": retained,
            }));
        }
    }
    Err(state::not_found())
}

/// cursor_recover (originating surface, read): re-mints a verified
/// continuation at its exact bound position.
pub fn cursor_recover(store: &Store, continuation: &str) -> Result<Vec<u8>, Problem> {
    let (society_id, seq) = store.parse_events_cursor_any(continuation)?;
    let fresh = store
        .mint_events_cursor(&society_id, seq)
        .map_err(|e| state::internal(&e.to_string()))?;
    ok_bytes(json!({
        "state": "recovered",
        "continuation": fresh,
    }))
}

/// recovery_checkpoint_show (projection, read): the diagnostic
/// remainder — it answers even on a sealed endpoint (§15.3).
pub fn recovery_checkpoint_show(
    store: &Store,
    society_id: &Option<String>,
) -> Result<Vec<u8>, Problem> {
    let mirror = store
        .journal_mirror_generation()
        .map_err(|e| state::internal(&e.to_string()))?;
    let witness_head = store
        .witness_head()
        .map_err(|e| state::internal(&e.to_string()))?;
    let mut result = json!({
        "endpoint_status": if store.sealed() { "sealed_diagnostic" } else { "active" },
        "journal_mirror_generation": mirror,
        "witness_head_generation": witness_head,
        "witness_profile": "developer-recovery",
    });
    if let Some(reason) = store.seal_reason() {
        if store.sealed() {
            result["seal_reason"] = json!(reason);
        }
    }
    if let Some(society_id) = society_id {
        if let Some(society) = rows::get_society(store.conn(), society_id).map_err(db_err)? {
            result["society_id"] = json!(society.society_id);
            result["recovery_epoch"] = json!(society.recovery_epoch);
            result["society_state"] = json!(society.state);
        } else {
            return Err(state::not_found());
        }
    }
    ok_bytes(result)
}

pub fn events_read(store: &Store, continuation: &str, page_size: u64) -> Result<Vec<u8>, Problem> {
    Ok(events_page(store, continuation, page_size)?.0)
}

/// One events page plus its item count (the events_wait long-poll checks
/// the count without holding the store lock between polls).
pub fn events_page(
    store: &Store,
    continuation: &str,
    page_size: u64,
) -> Result<(Vec<u8>, usize), Problem> {
    let (society_id, after_seq) = store.parse_events_cursor_any(continuation)?;
    let events = rows::events_after(store.conn(), &society_id, after_seq, page_size)
        .map_err(|e| state::internal(&e.to_string()))?;
    let last_seq = events.last().map(|e| e.sequence).unwrap_or(after_seq);
    let next = store
        .mint_events_cursor(&society_id, last_seq)
        .map_err(|e| state::internal(&e.to_string()))?;
    let events_json: Vec<Value> = events
        .iter()
        .map(|e| {
            let mut v = json!({
                "event_id": e.event_id,
                "kind": e.kind,
                "object_ref": e.object_ref,
                "object_revision": e.object_revision,
                "actor_ref": e.actor_ref,
                "causation_ref": e.causation_ref,
                "correlation_ref": e.correlation_ref,
                "payload_digest": digest_value(&e.payload_digest),
                "visibility_scope_ref": e.visibility_scope_ref,
                "occurred_at": e.occurred_at,
            });
            if let Some(p) = &e.participant_ref {
                v["participant_ref"] = Value::String(p.clone());
            }
            v
        })
        .collect();
    let count = events_json.len();
    let bytes = ok_bytes(json!({ "events": events_json, "continuation": next }))?;
    Ok((bytes, count))
}
