//! Read-side handlers: the negotiation family (per-surface hello,
//! protocol_info, feature_info) and the projection reads (society_show,
//! participant_show, events_read). Reads never mutate and never carry
//! meta; projection problems never disclose hidden existence.

use bpp_core::envelope::Success;
use bpp_core::limits;
use bpp_core::problem::Problem;
use bpp_core::registry;
use byom_store::{rows, Store};
use serde_json::{json, Value};

use crate::socket::SocketSurface;
use crate::state;

fn ok_bytes(result: Value) -> Result<Vec<u8>, Problem> {
    serde_json::to_vec(&Success::new(result)).map_err(|e| state::internal(&e.to_string()))
}

/// The slice-1 implemented operations, advertised per §14.1 (a feature
/// is advertised only when fully implemented).
pub const SLICE1_OPS: [&str; 12] = [
    "hello",
    "protocol_info",
    "feature_info",
    "society_prepare",
    "society_bootstrap",
    "society_show",
    "membership_offer",
    "membership_accept",
    "membership_refuse",
    "participant_admit",
    "manifestation_admit",
    "participant_show",
];

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
    // Honest advertisement: the slice-1 bundle subset plus the journal
    // recovery profile label (§15.3: developer recovery only, never
    // production rollback resistance). The registry stays the frozen
    // dispatch truth; feature_info narrows to what is implemented.
    let mutation_ops: Vec<&str> = registry::all_rows()
        .iter()
        .map(|r| r.operation.as_str())
        .filter(|op| {
            SLICE1_OPS.contains(op)
                && registry::all_rows()
                    .iter()
                    .any(|r| r.operation == *op && r.class != registry::OpClass::Read)
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut features = vec![json!({
        "feature": "b0.1-slice1",
        "operations": SLICE1_OPS,
    })];
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

pub fn events_read(store: &Store, continuation: &str, page_size: u64) -> Result<Vec<u8>, Problem> {
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
    ok_bytes(json!({ "events": events_json, "continuation": next }))
}
