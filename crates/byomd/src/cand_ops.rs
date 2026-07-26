//! Candidate-surface mutations over the offer-scoped channel token
//! (§7.4, R11): only the candidate authors acceptance and refusal, and
//! only for the exact MembershipOffer its credential is scoped to. A
//! terminal transition closes the channel; an EXACT refusal retry still
//! returns the retained receipt (idempotent replay through the closed
//! channel), while anything else over a closed channel is
//! non-enumerating `forbidden`.

use bpp_core::ops;
use bpp_core::problem::Problem;
use bpp_core::time::rfc3339_utc;
use byom_store::effects::{Effect, NewEvent};
use byom_store::rows::{self, ChannelRow};
use byom_store::{CrashHooks, CursorMint, MutationScope, Prepared, Store};
use serde_json::{json, Value};

use crate::channel::{self, Peer};
use crate::gov_ops::{
    channel_row, check_meta_binding, closed_credential, ensure_channel_files, expire_offer_if_due,
    obj_pairs, run,
};
use crate::state;

/// The channel-derived candidate actor: sender-constrained by token
/// possession (developer profile), never caller-selected.
fn candidate_actor(channel: &ChannelRow) -> String {
    format!("candidate:{}", channel.channel_id)
}

/// Verifies the presented candidate PROOF (BY-C1) and resolves its
/// channel. The proof is bound to the connection, the exact offer, the
/// Manifestation binding, the fence, the audience, the operation and a
/// short expiry, and its nonce is spent once. Anything else is
/// non-enumerating `forbidden`.
pub fn resolve_channel(
    store: &Store,
    presented: &str,
    operation: &str,
    peer: Peer,
    now: i64,
) -> Result<ChannelRow, Problem> {
    if presented.is_empty() {
        return Err(state::forbidden());
    }
    Ok(channel::verify(
        store,
        channel::AUDIENCE_CANDIDATE,
        operation,
        presented,
        peer,
        now,
    )?
    .channel)
}

/// A closed channel serves EXACTLY ONE thing (BY-C2): the byte-identical
/// replay of the exact refusal that closed it. Not "any retained result
/// for any operation" — the channel records which operation's refusal
/// closed it and that refusal's idempotency domain, so an acceptance or
/// self-policy call after admission, refusal, revocation or expiry
/// cannot borrow the replay path. Everything else is the terminal
/// non-enumerating `forbidden`.
fn closed_channel_replay(
    store: &Store,
    channel: &ChannelRow,
    operation: &str,
    meta: &bpp_core::envelope::MutationMeta,
    body: &Value,
) -> Result<Vec<u8>, Problem> {
    let (Some(closing_op), Some(closing_domain)) = (
        channel.closed_by_operation.as_deref(),
        channel.closed_by_domain_digest.as_deref(),
    ) else {
        // The channel closed by admission, revocation or expiry: no
        // candidate-authored refusal exists to replay.
        return Err(state::forbidden());
    };
    if closing_op != operation {
        return Err(state::forbidden());
    }
    let scope = MutationScope {
        society_id: channel.society_id.clone(),
        operation: operation.to_owned(),
        actor: candidate_actor(channel),
        meta: meta.clone(),
        body: body.clone(),
    };
    let digest = store
        .domain_digest(&scope)
        .map_err(|e| state::internal(&e.to_string()))?;
    if digest.value_hex != closing_domain {
        // A different idempotency key is a NEW command on a terminally
        // fenced channel, never a replay.
        return Err(state::forbidden());
    }
    let request_digest =
        Store::request_digest(body).map_err(|e| state::internal(&e.to_string()))?;
    match store
        .lookup_idempotency(&digest.value_hex)
        .map_err(|e| state::internal(&e.to_string()))?
    {
        Some((stored_digest, stored_result)) if stored_digest == request_digest => {
            Ok(stored_result)
        }
        _ => Err(state::forbidden()),
    }
}

/// Common candidate-mutation admission: channel state, offer scoping,
/// lazy server-time expiry, meta binding.
#[allow(clippy::too_many_arguments)]
fn admit_candidate_mutation(
    store: &mut Store,
    token: &str,
    peer: Peer,
    operation: &str,
    offer_ref: &str,
    meta: &bpp_core::envelope::MutationMeta,
    body: &Value,
    now: i64,
) -> Result<Result<ChannelRow, Vec<u8>>, Problem> {
    let channel = resolve_channel(store, token, operation, peer, now)?;
    if channel.state != "open" {
        // Terminal fence survives retry: only the exact receipt replays.
        return Ok(Err(closed_channel_replay(
            store, &channel, operation, meta, body,
        )?));
    }
    // The credential is offer-scoped: another offer's id is outside this
    // channel's authority (non-enumerating).
    if channel.scope_ref != offer_ref {
        return Err(state::forbidden());
    }
    check_meta_binding(store, meta, &channel.society_id)?;
    expire_offer_if_due(store, offer_ref, now)?;
    // Expiry may have closed the channel just now.
    let channel = rows::candidate_channel_by_id(store.conn(), &channel.channel_id)
        .map_err(|e| state::internal(&e.to_string()))?
        .ok_or_else(state::forbidden)?;
    if channel.state != "open" {
        return Ok(Err(closed_channel_replay(
            store, &channel, operation, meta, body,
        )?));
    }
    Ok(Ok(channel))
}

fn db_err(e: rusqlite::Error) -> Problem {
    state::internal(&e.to_string())
}

// ------------------------------------------------- membership_accept ----

pub fn membership_accept(
    store: &mut Store,
    token: &str,
    peer: Peer,
    req: &ops::MembershipAcceptRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    let channel = match admit_candidate_mutation(
        store,
        token,
        peer,
        "membership_accept",
        &req.offer_ref,
        &req.meta,
        body,
        now,
    )? {
        Ok(channel) => channel,
        Err(replayed) => return Ok(replayed),
    };
    let acceptance_id = store
        .new_id("acc")
        .map_err(|e| state::internal(&e.to_string()))?;
    let accept_event = store
        .new_id("evt")
        .map_err(|e| state::internal(&e.to_string()))?;
    let accepted_at = rfc3339_utc(now);
    let actor = candidate_actor(&channel);
    let acceptance_digest = store
        .record_digest(
            &channel.society_id,
            &acceptance_id,
            "bpp-membership-acceptance-v0",
            &json!({
                "acceptance_id": acceptance_id,
                "offer_ref": req.offer_ref,
                "subject_digest": serde_json::to_value(&req.subject_digest).unwrap_or(Value::Null),
                "accepted_by_actor_ref": actor,
                "accepted_at": accepted_at,
            }),
        )
        .map_err(|e| state::internal(&e.to_string()))?;

    let scope = MutationScope {
        society_id: channel.society_id.clone(),
        operation: "membership_accept".into(),
        actor: actor.clone(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let req = req.clone();
    run(store, scope, now, hooks, move |conn, scope| {
        let offer = rows::get_offer(conn, &req.offer_ref)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        // Revision CAS first (same-revision race discipline), then the
        // machine's transition guards.
        if req.meta.expected_revision != Some(offer.revision) {
            return Err(state::stale_revision());
        }
        match offer.state.as_str() {
            "offered" | "onboarding" => {}
            "accepted" => {
                return Err(state::stale_binding(
                    "an acceptance is already recorded for this offer",
                ))
            }
            _ => return Err(state::stale_binding("terminal offer")),
        }
        // Acceptance commits to the EXACT offer subject.
        let offer_subject: Value =
            serde_json::from_str(&offer.subject_digest).unwrap_or(Value::Null);
        if !req.subject_digest.same_ref_json(&offer_subject) {
            return Err(state::invalid(
                "subject_digest does not match the exact offer subject",
            ));
        }
        let mut offer_row = offer.to_effect_row();
        offer_row.insert("state".into(), json!("accepted"));
        offer_row.insert("revision".into(), json!(offer.revision + 1));
        offer_row.insert("acceptance_id".into(), json!(acceptance_id));
        offer_row.insert("accepted_at".into(), json!(accepted_at));
        let effects = vec![Effect::Upsert {
            table: "membership_offers".into(),
            row: offer_row,
        }];
        let events = vec![NewEvent {
            event_id: accept_event.clone(),
            society_id: offer.society_id.clone(),
            kind: "membership.accepted".into(),
            object_ref: offer.offer_id.clone(),
            object_revision: offer.revision + 1,
            participant_ref: Some(offer.participant_ref.clone()),
            actor_ref: scope.actor.clone(),
            causation_ref: format!("req:{}", req.meta.request_id),
            correlation_ref: req
                .meta
                .correlation_ref
                .clone()
                .unwrap_or_else(|| req.meta.request_id.clone()),
            payload: json!({"acceptance_id": acceptance_id, "state": "accepted"}),
            visibility_scope_ref: "scope:society".into(),
        }];
        Ok(Prepared {
            result: json!({
                "acceptance_id": acceptance_id,
                "offer_ref": offer.offer_id,
                "offer_state": "accepted",
                "accepted_at": accepted_at,
                "digest": serde_json::to_value(&acceptance_digest).unwrap_or(Value::Null),
            }),
            revision: Some(offer.revision + 1),
            cursor: CursorMint::AfterEvents {
                society_id: offer.society_id.clone(),
            },
            effects,
            events,
        })
    })
}

// -------------------------------------- candidate_self_policy_propose ----

/// candidate_self_policy_propose (candidate, create; §7.3): the candidate
/// authors its own pre-admission self-policy proposal over its
/// offer-scoped channel. The proposal is retained exactly as authored and
/// activates only at `participant_admit` — never before Standing.
pub fn candidate_self_policy_propose(
    store: &mut Store,
    token: &str,
    peer: Peer,
    req: &ops::CandidateSelfPolicyProposeRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    let channel = match admit_candidate_mutation(
        store,
        token,
        peer,
        "candidate_self_policy_propose",
        &req.onboarding_ref,
        &req.meta,
        body,
        now,
    )? {
        Ok(channel) => channel,
        Err(replayed) => return Ok(replayed),
    };
    let proposal_id = store
        .new_id("candpol")
        .map_err(|e| state::internal(&e.to_string()))?;
    let propose_event = store
        .new_id("evt")
        .map_err(|e| state::internal(&e.to_string()))?;
    let created_at = rfc3339_utc(now);
    let actor = candidate_actor(&channel);
    let scope = MutationScope {
        society_id: channel.society_id.clone(),
        operation: "candidate_self_policy_propose".into(),
        actor: actor.clone(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let req = req.clone();
    run(store, scope, now, hooks, move |conn, scope| {
        let offer = rows::get_offer(conn, &req.onboarding_ref)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        if !matches!(offer.state.as_str(), "offered" | "onboarding" | "accepted") {
            return Err(state::stale_binding("terminal offer"));
        }
        let effects = vec![Effect::Upsert {
            table: "candidate_policy_proposals".into(),
            row: obj_pairs([
                ("proposal_id", json!(proposal_id)),
                ("society_id", json!(offer.society_id)),
                ("offer_ref", json!(offer.offer_id)),
                ("participant_ref", json!(offer.participant_ref)),
                ("kind", json!(req.proposed_policy_kind)),
                ("state", json!("proposed")),
                ("body", json!(req.proposed_policy_body.to_string())),
                (
                    "body_digest",
                    serde_json::to_value(&req.proposed_policy_digest).unwrap_or(Value::Null),
                ),
                ("adoption_mode", json!(req.adoption_mode)),
                (
                    "adoption_control_domain_ref",
                    json!(req.adoption_control_domain_ref),
                ),
                ("activated_policy_ref", Value::Null),
                ("created_at", json!(created_at)),
            ]),
        }];
        let events = vec![NewEvent {
            event_id: propose_event.clone(),
            society_id: offer.society_id.clone(),
            kind: "candidate-self-policy.proposed".into(),
            object_ref: proposal_id.clone(),
            object_revision: 1,
            participant_ref: Some(offer.participant_ref.clone()),
            actor_ref: scope.actor.clone(),
            causation_ref: format!("req:{}", req.meta.request_id),
            correlation_ref: req
                .meta
                .correlation_ref
                .clone()
                .unwrap_or_else(|| req.meta.request_id.clone()),
            payload: json!({"kind": req.proposed_policy_kind, "state": "proposed",
                            "activation": "at participant_admit, never before Standing"}),
            visibility_scope_ref: "scope:society".into(),
        }];
        Ok(Prepared {
            result: json!({
                "proposal_id": proposal_id,
                "onboarding_ref": offer.offer_id,
                "kind": req.proposed_policy_kind,
                "state": "proposed",
                "created_at": created_at,
            }),
            revision: Some(1),
            cursor: CursorMint::AfterEvents {
                society_id: offer.society_id.clone(),
            },
            effects,
            events,
        })
    })
}

// ------------------------------------------------- membership_refuse ----

pub fn membership_refuse(
    store: &mut Store,
    token: &str,
    peer: Peer,
    req: &ops::MembershipRefuseRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    let channel = match admit_candidate_mutation(
        store,
        token,
        peer,
        "membership_refuse",
        &req.offer_ref,
        &req.meta,
        body,
        now,
    )? {
        Ok(channel) => channel,
        Err(replayed) => return Ok(replayed),
    };
    let refusal_id = store
        .new_id("refusal")
        .map_err(|e| state::internal(&e.to_string()))?;
    let refuse_event = store
        .new_id("evt")
        .map_err(|e| state::internal(&e.to_string()))?;
    let channel_event = store
        .new_id("evt")
        .map_err(|e| state::internal(&e.to_string()))?;
    let refused_at = rfc3339_utc(now);
    let actor = candidate_actor(&channel);
    let refusal_digest = store
        .record_digest(
            &channel.society_id,
            &refusal_id,
            "bpp-membership-refusal-v0",
            &json!({
                "refusal_id": refusal_id,
                "offer_ref": req.offer_ref,
                "offer_subject_digest":
                    serde_json::to_value(&req.offer_subject_digest).unwrap_or(Value::Null),
                "refused_by_actor_ref": actor,
                "refused_at": refused_at,
            }),
        )
        .map_err(|e| state::internal(&e.to_string()))?;

    let scope = MutationScope {
        society_id: channel.society_id.clone(),
        operation: "membership_refuse".into(),
        actor: actor.clone(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    // The exact idempotency domain of THIS refusal: the one receipt the
    // closed channel will replay (BY-C2).
    let refusal_domain = store
        .domain_digest(&scope)
        .map_err(|e| state::internal(&e.to_string()))?
        .value_hex;
    let req = req.clone();
    let channel = channel.clone();
    let bytes = run(store, scope, now, hooks, move |conn, scope| {
        let offer = rows::get_offer(conn, &req.offer_ref)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        if req.meta.expected_revision != Some(offer.revision) {
            return Err(state::stale_revision());
        }
        if !matches!(offer.state.as_str(), "offered" | "onboarding" | "accepted") {
            return Err(state::stale_binding("terminal offer"));
        }
        let offer_subject: Value =
            serde_json::from_str(&offer.subject_digest).unwrap_or(Value::Null);
        if !req.offer_subject_digest.same_ref_json(&offer_subject) {
            return Err(state::invalid(
                "offer_subject_digest does not match the exact offer subject",
            ));
        }
        // Retraction cites the prior acceptance, exactly (§7.4).
        let superseded = offer.acceptance_id.clone();
        if let Some(cited) = &req.superseded_acceptance_ref {
            if superseded.as_deref() != Some(cited.as_str()) {
                return Err(state::stale_binding(
                    "superseded_acceptance_ref does not cite the current acceptance",
                ));
            }
        }
        let new_revision = offer.revision + 1;
        let new_fence = offer.fence_epoch + 1;
        let mut offer_row = offer.to_effect_row();
        offer_row.insert("state".into(), json!("refused"));
        offer_row.insert("revision".into(), json!(new_revision));
        offer_row.insert("fence_epoch".into(), json!(new_fence));
        offer_row.insert("refusal_id".into(), json!(refusal_id));
        offer_row.insert("refused_at".into(), json!(refused_at));
        offer_row.insert(
            "superseded_acceptance_ref".into(),
            superseded.as_ref().map(|v| json!(v)).unwrap_or(Value::Null),
        );
        offer_row.insert(
            "refusal_reason_ref".into(),
            req.refusal_reason_ref
                .as_ref()
                .map(|v| json!(v))
                .unwrap_or(Value::Null),
        );
        let mut effects = vec![
            Effect::Upsert {
                table: "membership_offers".into(),
                row: offer_row,
            },
            // Refusal advances the fence and closes the candidate
            // channel (§7.4), recording THIS refusal as the ONLY call
            // the closed channel replays (BY-C2).
            Effect::Upsert {
                table: "candidate_channels".into(),
                row: channel_row(
                    "candidate_channels",
                    &channel.channel_id,
                    &channel.society_id,
                    &channel.scope_ref,
                    &channel.token,
                    &channel.token_path,
                    "closed",
                    &refused_at,
                    Some("membership_refuse"),
                    Some(&refusal_domain),
                ),
            },
            Effect::Upsert {
                table: "channel_credentials".into(),
                row: closed_credential(conn, &channel.channel_id, &refused_at)?,
            },
        ];
        // §7.4: the same CAS transaction moves any OnboardingActivationOffer
        // to `refused`, advances its fence and REVOKES unused onboarding
        // compute authority. Every onboarding channel's token subject
        // contains that fence, so the workload's own credential stops
        // matching (the refusal fences the workload).
        effects.extend(crate::onboard_ops::fence_onboarding(
            conn,
            &offer.offer_id,
            "refused",
            new_fence,
        )?);
        let events = vec![
            NewEvent {
                event_id: refuse_event.clone(),
                society_id: offer.society_id.clone(),
                kind: "membership.refused".into(),
                object_ref: offer.offer_id.clone(),
                object_revision: new_revision,
                participant_ref: Some(offer.participant_ref.clone()),
                actor_ref: scope.actor.clone(),
                causation_ref: format!("req:{}", req.meta.request_id),
                correlation_ref: req
                    .meta
                    .correlation_ref
                    .clone()
                    .unwrap_or_else(|| req.meta.request_id.clone()),
                payload: json!({"refusal_id": refusal_id, "state": "refused",
                                "superseded_acceptance_ref": superseded}),
                visibility_scope_ref: "scope:society".into(),
            },
            NewEvent {
                event_id: channel_event.clone(),
                society_id: offer.society_id.clone(),
                kind: "channel.candidate_closed".into(),
                object_ref: channel.channel_id.clone(),
                object_revision: 1,
                participant_ref: Some(offer.participant_ref.clone()),
                actor_ref: scope.actor.clone(),
                causation_ref: format!("req:{}", req.meta.request_id),
                correlation_ref: req.meta.request_id.clone(),
                payload: json!({"reason": "membership refused"}),
                visibility_scope_ref: "scope:society".into(),
            },
        ];
        let mut result = json!({
            "refusal_id": refusal_id,
            "offer_ref": offer.offer_id,
            "offer_state": "refused",
            "fence_epoch": new_fence,
            "refused_at": refused_at,
            "digest": serde_json::to_value(&refusal_digest).unwrap_or(Value::Null),
        });
        if let Some(superseded) = &superseded {
            result["superseded_acceptance_ref"] = json!(superseded);
        }
        Ok(Prepared {
            result,
            revision: Some(new_revision),
            cursor: CursorMint::AfterEvents {
                society_id: offer.society_id.clone(),
            },
            effects,
            events,
        })
    })?;
    ensure_channel_files(store);
    // The onboarding workload token file goes with the fenced offer.
    crate::episode_ops::ensure_runtime_token_files(store);
    Ok(bytes)
}
