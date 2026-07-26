//! Shared participant-surface machinery: channel-derived caller
//! resolution (§14.3 — the actor is never a request field), the closed
//! terminal-receipt replay through a fenced participant channel, the
//! RT-03 seat/position discipline, §10.5 preparation traces, and the
//! §11.4 budget-conservation ledger helpers.

use bpp_core::envelope::MutationMeta;
use bpp_core::ops;
use bpp_core::problem::{Problem, ProblemKind};
use bpp_core::time::rfc3339_utc;
use byom_store::effects::Effect;
use byom_store::rows::{self, ParticipantRow};
use byom_store::{MutationScope, Store};
use serde_json::{json, Map, Value};

use crate::gov_ops::{db_err, digest_json, mint};
use crate::state;

/// The authenticated participant-surface caller. The sovereign human
/// reaches the participant surface over the same-UID socket with an
/// empty channel preamble (developer profile); an agent participant
/// presents its sender-constrained participant-channel token.
#[derive(Debug, Clone)]
pub struct Caller {
    pub participant: ParticipantRow,
    pub society_id: String,
    /// The channel-derived actor binding string.
    pub actor: String,
    /// The presenting channel, when token-authenticated.
    pub channel: Option<rows::ChannelRow>,
}

/// Resolves the participant-surface caller from the token preamble.
/// Returns `Err(problem)` for unknown tokens (non-enumerating) and
/// `Ok(Err(channel))` for a CLOSED channel — the caller may only replay
/// an exact terminal receipt (§7.4 discipline, reused for cease).
pub fn resolve_caller(
    store: &Store,
    presented: &str,
    operation: &str,
    peer: crate::channel::Peer,
    now: i64,
) -> Result<Result<Caller, rows::ChannelRow>, Problem> {
    if presented.is_empty() {
        // The sovereign human of the sole Society (developer profile:
        // same-UID peer is the sovereign; the fresh phishing-resistant
        // challenge remains the recorded developer-profile stub).
        let society = rows::sole_society(store.conn())
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        let participant = rows::sovereign_participant(store.conn(), &society.society_id)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        let actor = format!("participant:{}", participant.participant_id);
        return Ok(Ok(Caller {
            society_id: participant.society_id.clone(),
            actor,
            participant,
            channel: None,
        }));
    }
    // An agent participant presents a sender-constrained PROOF bound to
    // its connection and this exact operation (BY-C1), never a reusable
    // bearer token.
    let channel = crate::channel::verify(
        store,
        crate::channel::AUDIENCE_PARTICIPANT,
        operation,
        presented,
        peer,
        now,
    )?
    .channel;
    if channel.state != "open" {
        return Ok(Err(channel));
    }
    let participant = rows::get_participant(store.conn(), &channel.scope_ref)
        .map_err(db_err)?
        .ok_or_else(state::forbidden)?;
    if participant.state != "active" {
        return Ok(Err(channel));
    }
    let actor = format!("participant:{}", participant.participant_id);
    Ok(Ok(Caller {
        society_id: participant.society_id.clone(),
        actor,
        participant,
        channel: Some(channel),
    }))
}

/// A closed participant channel serves exactly one thing: the
/// byte-identical replay of a terminal receipt for the exact same
/// request. Everything else is non-enumerating `forbidden`.
pub fn closed_channel_replay(
    store: &Store,
    channel: &rows::ChannelRow,
    operation: &str,
    meta: &MutationMeta,
    body: &Value,
) -> Result<Vec<u8>, Problem> {
    // BY-C2: only the exact terminal call that closed the channel
    // replays, and only under its own idempotency domain.
    let (Some(closing_op), Some(closing_domain)) = (
        channel.closed_by_operation.as_deref(),
        channel.closed_by_domain_digest.as_deref(),
    ) else {
        return Err(state::forbidden());
    };
    if closing_op != operation {
        return Err(state::forbidden());
    }
    let scope = MutationScope {
        society_id: channel.society_id.clone(),
        operation: operation.to_owned(),
        actor: format!("participant:{}", channel.scope_ref),
        meta: meta.clone(),
        body: body.clone(),
    };
    let digest = store
        .domain_digest(&scope)
        .map_err(|e| state::internal(&e.to_string()))?;
    if digest.value_hex != closing_domain {
        return Err(state::forbidden());
    }
    let request_digest =
        Store::request_digest(body).map_err(|e| state::internal(&e.to_string()))?;
    match store
        .lookup_idempotency(&digest.value_hex)
        .map_err(|e| state::internal(&e.to_string()))?
    {
        Some((stored, result)) if stored == request_digest => Ok(result),
        _ => Err(state::forbidden()),
    }
}

// -------------------------------------------------- preparation trace ----

/// One §10.5 output-pointer provenance row.
pub fn source_row(output: &str, request_id: &str, source_pointer: &str, transform: &str) -> Value {
    json!({
        "output_pointer": output,
        "source_ref": format!("req:{request_id}"),
        "source_pointer": source_pointer,
        "transform_id": transform,
    })
}

/// Builds the complete PreparationTrace of one server-prepared subject
/// (RT-04): input digest over the exact request, output subject digest,
/// named field sources, no semantic defaults.
#[allow(clippy::too_many_arguments)]
pub fn prepare_trace(
    store: &Store,
    society_id: &str,
    operation: &str,
    actor: &str,
    request_id: &str,
    body: &Value,
    subject_digest: &bpp_core::digest::DigestRef,
    dependency_set_ref: &str,
    field_sources: Vec<Value>,
    now: i64,
) -> Result<Value, Problem> {
    let trace_id = mint(store, "trace")?;
    let input_digest = store
        .mint_object_digest(
            &format!("society-key:{society_id}/object:{trace_id}-input"),
            "bpp-preparation-input-v0",
            body,
        )
        .map_err(|e| state::internal(&e.to_string()))?;
    let mut trace = json!({
        "trace_id": trace_id,
        "operation": operation,
        "actor_binding_ref": actor,
        "input_ref": format!("req:{request_id}"),
        "input_digest": digest_json(&input_digest),
        "output_subject_digest": digest_json(subject_digest),
        "field_sources": field_sources,
        "policy_algebra_version": "bpa-1",
        "dependency_set_ref": dependency_set_ref,
        "created_at": rfc3339_utc(now),
    });
    let trace_digest = store
        .mint_object_digest(
            &format!("society-key:{society_id}/object:{trace_id}"),
            "bpp-preparation-trace-v0",
            &trace,
        )
        .map_err(|e| state::internal(&e.to_string()))?;
    trace["digest"] = digest_json(&trace_digest);
    Ok(trace)
}

// ------------------------------------------------- seat/position CAS ----

/// One prepared seat: who may fill it, on which surface.
#[derive(Debug, Clone)]
pub struct Seat {
    pub seat_ref: String,
    pub kind: String,
    pub participant_ref: String,
    pub surface: String,
}

pub fn seats_json(seats: &[Seat]) -> Value {
    Value::Array(
        seats
            .iter()
            .map(|s| {
                json!({
                    "seat_ref": s.seat_ref,
                    "kind": s.kind,
                    "participant_ref": s.participant_ref,
                    "surface": s.surface,
                })
            })
            .collect(),
    )
}

pub fn seats_from_json(v: &Value) -> Vec<Seat> {
    v.as_array()
        .map(|a| {
            a.iter()
                .map(|s| Seat {
                    seat_ref: s["seat_ref"].as_str().unwrap_or_default().to_owned(),
                    kind: s["kind"].as_str().unwrap_or_default().to_owned(),
                    participant_ref: s["participant_ref"].as_str().unwrap_or_default().to_owned(),
                    surface: s["surface"].as_str().unwrap_or_default().to_owned(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Pre-minted identifiers for one position write (minted outside the
/// prepare closure, stable across CAS revalidation).
pub struct PositionMint {
    pub position_id: String,
    pub event_id: String,
}

pub fn mint_position(store: &Store) -> Result<PositionMint, Problem> {
    Ok(PositionMint {
        position_id: mint(store, "pos")?,
        event_id: mint(store, "evt")?,
    })
}

/// A `local_erasure_safe` record digest minted inside an open prepare
/// transaction, under a RANDOM per-object secret retained wrapped under
/// the Society key (D-R1-2) — the same construction as
/// `Store::record_digest`, so both sides agree and each object stays
/// individually destroyable.
pub fn conn_record_digest(
    conn: &rusqlite::Connection,
    society_id: &str,
    object_id: &str,
    tag: &str,
    object: &Value,
) -> Result<bpp_core::digest::DigestRef, Problem> {
    use bpp_core::canonical::{hex, hmac_sha256, tagged_canonical};
    let root = byom_store::schema::meta_get(conn, "index_root_key")
        .map_err(|e| state::internal(&e.to_string()))?
        .ok_or_else(|| state::internal("store is not bootstrapped"))?;
    let wrap_key = hmac_sha256(&root, format!("object-secret-wrap:{society_id}").as_bytes());
    let key_ref = format!("society-key:{society_id}/object:{object_id}");
    let secret = byom_store::object_secrets::ensure(conn, &wrap_key, &key_ref, society_id, tag)
        .map_err(|e| state::internal(&e.to_string()))?;
    let preimage = tagged_canonical(tag, object).map_err(|e| state::internal(&e.to_string()))?;
    Ok(bpp_core::digest::DigestRef::local_erasure_safe(
        &key_ref,
        hex(&hmac_sha256(&secret, &preimage)),
    ))
}

/// The RT-03 position discipline inside an open prepare transaction:
/// exact proposal revision, exact subject digest (the COMPLETE canonical
/// `DigestRef`, BY-D1), an eligible prepared seat on THIS surface for
/// THIS actor, and the one-current-seat-head CAS
/// (`prior_position_digest` consumes the head; a second head without it
/// is `stale_revision`).
///
/// PositionRevisions are APPEND-ONLY (BY-P1): superseding never rewrites
/// the prior row's status — it appends a new immutable revision carrying
/// the prior position digest, the participant binding epoch, the
/// endpoint incarnation, the Society recovery epoch and the
/// authentication observation, and CASes the SEPARATE seat-head row.
/// Returns `(effects, result)`.
#[allow(clippy::too_many_arguments)]
pub fn record_position(
    conn: &rusqlite::Connection,
    minted: &PositionMint,
    proposal_kind: &str,
    society_id: &str,
    current_revision: u64,
    proposal_subject: &bpp_core::digest::DigestRef,
    seats: &[Seat],
    req: &ops::PositionRequest,
    actor_participant: &str,
    actor: &str,
    surface: &str,
    now: i64,
) -> Result<(Vec<Effect>, Value), Problem> {
    if req.proposal_revision != current_revision {
        return Err(state::stale_revision());
    }
    if !req.subject_digest.same_ref(proposal_subject) {
        return Err(state::invalid(
            "subject_digest does not commit to the exact prepared subject",
        ));
    }
    let Some(seat) = seats.iter().find(|s| s.seat_ref == req.seat_ref) else {
        return Err(position_ineligible("seat_ref is not a prepared seat"));
    };
    if seat.surface != surface {
        return Err(position_ineligible(
            "this seat is not fillable on this surface",
        ));
    }
    if seat.participant_ref != actor_participant {
        return Err(position_ineligible(
            "position operations fill only the authenticated actor's eligible seat",
        ));
    }
    let head = rows::position_seat_head(conn, proposal_kind, &req.proposal_ref, &req.seat_ref)
        .map_err(db_err)?;
    let live_head = head
        .as_ref()
        .filter(|h| rows::str_of(h, "status") == "active");
    let target_status = req.target_status.as_deref().unwrap_or("active");
    let mut effects = Vec::new();
    let (next_revision, prior_digest) = match live_head {
        Some(head_row) => {
            let head_digest = rows::json_of(head_row, "digest");
            // The COMPLETE canonical DigestRef must match, not the
            // 32-byte value alone (BY-D1).
            let cited_matches = req
                .prior_position_digest
                .as_ref()
                .is_some_and(|d| d.same_ref_json(&head_digest));
            if !cited_matches {
                return Err(Problem::new(
                    ProblemKind::StaleRevision,
                    "the seat head is taken; superseding requires prior_position_digest",
                )
                .with_status(409));
            }
            (
                rows::u64_of(head_row, "revision") + 1,
                Some(head_digest.to_string()),
            )
        }
        None => {
            if req.prior_position_digest.is_some() {
                return Err(state::stale_binding(
                    "prior_position_digest cites no current seat head",
                ));
            }
            if target_status == "withdrawn" {
                return Err(state::stale_binding("no position to withdraw"));
            }
            (
                head.as_ref()
                    .map(|h| rows::u64_of(h, "revision"))
                    .unwrap_or(0)
                    + 1,
                None,
            )
        }
    };
    let created_at = rfc3339_utc(now);
    let participant = rows::get_participant(conn, actor_participant)
        .map_err(db_err)?
        .ok_or_else(state::not_found)?;
    let incarnation = byom_store::schema::meta_get_text(conn, "endpoint_incarnation")
        .map_err(|e| state::internal(&e.to_string()))?
        .unwrap_or_default();
    let recovery_epoch = rows::get_society(conn, society_id)
        .map_err(db_err)?
        .map(|s| s.recovery_epoch)
        .unwrap_or(0);
    // The §14.5 authentication observation of the authoring channel —
    // honestly the developer-profile same-UID observation at this slice.
    let authentication_observation = format!("same-uid-peer:{surface}");
    let position_digest = conn_record_digest(
        conn,
        society_id,
        &minted.position_id,
        "bpp-position-v0",
        &json!({
                "position_id": minted.position_id,
                "proposal_kind": proposal_kind,
                "proposal_ref": req.proposal_ref,
                "proposal_revision": req.proposal_revision,
                "seat_ref": req.seat_ref,
                "participant_ref": actor_participant,
                "participant_binding_epoch": participant.binding_epoch,
                "endpoint_incarnation": incarnation,
                "recovery_epoch": recovery_epoch,
                "authentication_observation": authentication_observation,
                "prior_position_digest": prior_digest,
                "value": req.value,
                "status": target_status,
                "created_at": created_at,
        }),
    )?;
    let opt = |v: &Option<String>| v.as_ref().map(|s| json!(s)).unwrap_or(Value::Null);
    effects.push(Effect::Upsert {
        table: "position_revisions".into(),
        row: crate::gov_ops::obj_pairs([
            ("position_id", json!(minted.position_id)),
            ("society_id", json!(society_id)),
            ("proposal_kind", json!(proposal_kind)),
            ("proposal_ref", json!(req.proposal_ref)),
            ("proposal_revision", json!(req.proposal_revision)),
            ("seat_ref", json!(req.seat_ref)),
            ("participant_ref", json!(actor_participant)),
            (
                "participant_binding_epoch",
                json!(participant.binding_epoch),
            ),
            ("actor_ref", json!(actor)),
            (
                "authentication_observation",
                json!(authentication_observation),
            ),
            ("endpoint_incarnation", json!(incarnation)),
            ("recovery_epoch", json!(recovery_epoch)),
            ("value", json!(req.value)),
            ("status", json!(target_status)),
            ("revision", json!(next_revision)),
            ("assent_mode", opt(&req.assent_mode)),
            (
                "derived_assent_receipt_ref",
                opt(&req.derived_assent_receipt_ref),
            ),
            ("reason_ref", opt(&req.reason_ref)),
            ("subject_digest", json!(digest_json(&req.subject_digest))),
            (
                "prior_position_digest",
                prior_digest.map(Value::from).unwrap_or(Value::Null),
            ),
            ("digest", json!(digest_json(&position_digest))),
            ("created_at", json!(created_at)),
        ]),
    });
    // The SEPARATE current-seat-head CAS row.
    effects.push(Effect::Upsert {
        table: "position_seat_heads".into(),
        row: crate::gov_ops::obj_pairs([
            ("proposal_kind", json!(proposal_kind)),
            ("proposal_ref", json!(req.proposal_ref)),
            ("seat_ref", json!(req.seat_ref)),
            ("society_id", json!(society_id)),
            ("position_ref", json!(minted.position_id)),
            ("revision", json!(next_revision)),
            ("value", json!(req.value)),
            ("status", json!(target_status)),
            ("digest", json!(digest_json(&position_digest))),
            ("updated_at", json!(created_at)),
        ]),
    });
    let mut result = json!({
        "position_id": minted.position_id,
        "revision": next_revision,
        "proposal_ref": req.proposal_ref,
        "proposal_revision": req.proposal_revision,
        "seat_ref": req.seat_ref,
        "value": req.value,
        "status": target_status,
        "created_at": created_at,
        "digest": digest_json(&position_digest),
    });
    if let Some(mode) = &req.assent_mode {
        result["assent_mode"] = json!(mode);
    }
    Ok((effects, result))
}

/// Parses a stored/serialized digest column into a complete canonical
/// `DigestRef` (BY-D1). A column that is not a well-formed DigestRef is
/// an internal fault, never a `value_hex`-only comparison.
pub fn digest_of(stored: &Value) -> Result<bpp_core::digest::DigestRef, Problem> {
    serde_json::from_value(stored.clone())
        .map_err(|_| state::internal("stored digest is not a canonical DigestRef"))
}

pub fn position_ineligible(detail: &str) -> Problem {
    Problem::new(
        ProblemKind::PositionIneligible,
        "the actor holds no eligible prepared seat",
    )
    .with_status(403)
    .with_detail(detail.to_owned())
}

/// Do all seats hold a current `assent` head? (`decision_incomplete`
/// otherwise — the caller names the missing seat.)
pub fn all_seats_assent(
    conn: &rusqlite::Connection,
    proposal_kind: &str,
    proposal_ref: &str,
    seats: &[Seat],
) -> Result<(), Problem> {
    for seat in seats {
        let head = rows::active_position(conn, proposal_kind, proposal_ref, &seat.seat_ref)
            .map_err(db_err)?;
        let ok = head
            .as_ref()
            .is_some_and(|h| rows::str_of(h, "value") == "assent");
        if !ok {
            return Err(Problem::new(
                ProblemKind::DecisionIncomplete,
                "the complete required seat set has not assented",
            )
            .with_status(409)
            .with_detail(format!("seat {} has no current assent", seat.seat_ref)));
        }
    }
    Ok(())
}

// -------------------------------------------------- budget conservation ----

/// Developer-profile root ceiling per dimension (honestly a stub: the
/// hosted profile binds real account engines).
pub const ROOT_CEILING: u64 = 1_000_000;
/// Default delegated child ceiling at mandate issue.
pub const MANDATE_CEILING: u64 = 1_024;
/// Default delegated child ceiling at endeavor formation.
pub const ENDEAVOR_CEILING: u64 = 65_536;
/// The default dimension of ref-only ceilings.
pub const UNIT_DIMENSION: &str = "unit";

#[allow(clippy::too_many_arguments)]
fn account_row(
    society_id: &str,
    account_ref: &str,
    dimension: &str,
    ceiling: u64,
    remaining: u64,
    reserved: u64,
    committed: u64,
    delegated: u64,
    parent: Option<&str>,
    revision: u64,
    created_at: &str,
) -> Map<String, Value> {
    crate::gov_ops::obj_pairs([
        ("account_ref", json!(account_ref)),
        ("dimension", json!(dimension)),
        ("society_id", json!(society_id)),
        ("ceiling", json!(ceiling)),
        ("remaining", json!(remaining)),
        ("reserved", json!(reserved)),
        ("committed", json!(committed)),
        ("uncertain", json!(0)),
        ("delegated_to_children", json!(delegated)),
        (
            "parent_account_ref",
            parent.map(|p| json!(p)).unwrap_or(Value::Null),
        ),
        ("revision", json!(revision)),
        ("created_at", json!(created_at)),
    ])
}

/// Loads (or lazily initializes at the developer-profile root ceiling)
/// one `(account_ref, dimension)` ledger row.
fn load_or_root(
    conn: &rusqlite::Connection,
    society_id: &str,
    account_ref: &str,
    dimension: &str,
    now: i64,
) -> Result<Map<String, Value>, Problem> {
    match rows::budget_account(conn, account_ref, dimension).map_err(db_err)? {
        Some(row) => Ok(row),
        None => Ok(account_row(
            society_id,
            account_ref,
            dimension,
            ROOT_CEILING,
            ROOT_CEILING,
            0,
            0,
            0,
            None,
            1,
            &rfc3339_utc(now),
        )),
    }
}

/// Atomic §11.4 delegation: moves `amount` from the parent's remaining
/// bucket into `delegated_to_children` and creates the child ceiling in
/// the same transition. `budget_exceeded` when the parent cannot cover
/// it. Conservation holds on both rows.
#[allow(clippy::too_many_arguments)]
pub fn delegate_child(
    conn: &rusqlite::Connection,
    effects: &mut Vec<Effect>,
    society_id: &str,
    parent_ref: &str,
    child_ref: &str,
    dimension: &str,
    amount: u64,
    now: i64,
) -> Result<(), Problem> {
    let mut parent = load_or_root(conn, society_id, parent_ref, dimension, now)?;
    let remaining = rows::u64_of(&parent, "remaining");
    if remaining < amount {
        return Err(budget_exceeded(parent_ref, dimension, amount, remaining));
    }
    if rows::budget_account(conn, child_ref, dimension)
        .map_err(db_err)?
        .is_some()
    {
        return Err(state::stale_binding("child budget account already exists"));
    }
    parent.insert("remaining".into(), json!(remaining - amount));
    parent.insert(
        "delegated_to_children".into(),
        json!(rows::u64_of(&parent, "delegated_to_children") + amount),
    );
    parent.insert(
        "revision".into(),
        json!(rows::u64_of(&parent, "revision") + 1),
    );
    effects.push(Effect::Upsert {
        table: "budget_accounts".into(),
        row: parent,
    });
    effects.push(Effect::Upsert {
        table: "budget_accounts".into(),
        row: account_row(
            society_id,
            child_ref,
            dimension,
            amount,
            amount,
            0,
            0,
            0,
            Some(parent_ref),
            1,
            &rfc3339_utc(now),
        ),
    });
    Ok(())
}

/// Reserves `amount` on an existing (or lazily root-delegated) account:
/// remaining → reserved plus a reservation row (§11.4: all Byom-owned
/// dimensions reserve in one Byom transaction — the caller passes the
/// shared `effects` of that transaction).
#[allow(clippy::too_many_arguments)]
pub fn reserve(
    conn: &rusqlite::Connection,
    effects: &mut Vec<Effect>,
    society_id: &str,
    account_ref: &str,
    dimension: &str,
    amount: u64,
    reservation_id: &str,
    holder_kind: &str,
    holder_ref: &str,
    now: i64,
) -> Result<(), Problem> {
    // Prefer effects already staged in this transition (multi-dimension
    // reservations against the same account row).
    let staged = effects.iter().rev().find_map(|e| match e {
        Effect::Upsert { table, row }
            if table == "budget_accounts"
                && rows::str_of(row, "account_ref") == account_ref
                && rows::str_of(row, "dimension") == dimension =>
        {
            Some(row.clone())
        }
        _ => None,
    });
    let mut account = match staged {
        Some(row) => row,
        None => load_or_root(conn, society_id, account_ref, dimension, now)?,
    };
    let remaining = rows::u64_of(&account, "remaining");
    if remaining < amount {
        return Err(budget_exceeded(account_ref, dimension, amount, remaining));
    }
    account.insert("remaining".into(), json!(remaining - amount));
    account.insert(
        "reserved".into(),
        json!(rows::u64_of(&account, "reserved") + amount),
    );
    account.insert(
        "revision".into(),
        json!(rows::u64_of(&account, "revision") + 1),
    );
    effects.push(Effect::Upsert {
        table: "budget_accounts".into(),
        row: account,
    });
    effects.push(Effect::Upsert {
        table: "budget_reservations".into(),
        row: crate::gov_ops::obj_pairs([
            ("reservation_id", json!(reservation_id)),
            ("society_id", json!(society_id)),
            ("account_ref", json!(account_ref)),
            ("dimension", json!(dimension)),
            ("holder_kind", json!(holder_kind)),
            ("holder_ref", json!(holder_ref)),
            ("amount", json!(amount)),
            ("state", json!("reserved")),
            ("created_at", json!(rfc3339_utc(now))),
        ]),
    });
    Ok(())
}

/// Settles every `reserved` reservation of one holder: `commit` moves
/// reserved → committed; otherwise reserved → remaining (released).
/// Conservation holds; nothing is spent twice.
pub fn settle_holder(
    conn: &rusqlite::Connection,
    effects: &mut Vec<Effect>,
    holder_kind: &str,
    holder_ref: &str,
    commit: bool,
) -> Result<(), Problem> {
    let mut stmt = conn
        .prepare(
            "SELECT reservation_id FROM budget_reservations
             WHERE holder_kind = ?1 AND holder_ref = ?2 AND state = 'reserved'",
        )
        .map_err(db_err)?;
    let ids: Vec<String> = stmt
        .query_map([holder_kind, holder_ref], |r| r.get(0))
        .map_err(db_err)?
        .collect::<Result<_, _>>()
        .map_err(db_err)?;
    for id in ids {
        let Some(mut reservation) =
            rows::get_row(conn, "budget_reservations", "reservation_id", &id).map_err(db_err)?
        else {
            continue;
        };
        let account_ref = rows::str_of(&reservation, "account_ref").to_owned();
        let dimension = rows::str_of(&reservation, "dimension").to_owned();
        let amount = rows::u64_of(&reservation, "amount");
        let Some(mut account) =
            rows::budget_account(conn, &account_ref, &dimension).map_err(db_err)?
        else {
            return Err(state::internal("reservation without its account row"));
        };
        let reserved = rows::u64_of(&account, "reserved");
        if reserved < amount {
            return Err(state::internal("reserved bucket underflow"));
        }
        account.insert("reserved".into(), json!(reserved - amount));
        if commit {
            account.insert(
                "committed".into(),
                json!(rows::u64_of(&account, "committed") + amount),
            );
        } else {
            account.insert(
                "remaining".into(),
                json!(rows::u64_of(&account, "remaining") + amount),
            );
        }
        account.insert(
            "revision".into(),
            json!(rows::u64_of(&account, "revision") + 1),
        );
        effects.push(Effect::Upsert {
            table: "budget_accounts".into(),
            row: account,
        });
        reservation.insert(
            "state".into(),
            json!(if commit { "committed" } else { "released" }),
        );
        effects.push(Effect::Upsert {
            table: "budget_reservations".into(),
            row: reservation,
        });
    }
    Ok(())
}

pub fn budget_exceeded(account_ref: &str, dimension: &str, want: u64, have: u64) -> Problem {
    Problem::new(ProblemKind::BudgetExceeded, "budget ceiling exceeded")
        .with_status(409)
        .with_detail(format!(
            "account {account_ref} dimension {dimension}: requested {want}, remaining {have}"
        ))
}
