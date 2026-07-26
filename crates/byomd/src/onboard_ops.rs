//! B3 slice 3 — the §7.4 bounded onboarding path and its ONE-SHOT hosted
//! compute.
//!
//! ```text
//! onboarding_offer                  governance  the Society FUNDS the
//!                                               invitation (never assent)
//! onboarding_compute_permit_consume runtime     Kovee model broker, exact
//!                                               one-shot key + onboarding
//!                                               fence -> ONE receipt
//! onboarding_episode_claim          runtime     candidate workload, one
//!                                               offer fence, at most ONE
//!                                               episode
//! onboarding_episode_complete       runtime     EVIDENCE ONLY — never
//!                                               acceptance
//! ```
//!
//! Three things this module refuses to let happen:
//!
//! - a SECOND compute use under one offer (`max_uses: 1` by construction);
//! - runtime output becoming membership assent (§16.6 item 12) — the
//!   completion shape has no acceptance member, no Standing is created, and
//!   the MembershipOffer state is untouched;
//! - authority surviving a refusal — `membership_refuse` advances the
//!   onboarding fence, and every onboarding channel's token subject
//!   CONTAINS that fence, so the workload's own credential stops matching.

use bpp_core::ops;
use bpp_core::problem::{Problem, ProblemKind};
use bpp_core::time::rfc3339_utc;
use byom_store::effects::Effect;
use byom_store::rows;
use byom_store::{CrashHooks, CursorMint, MutationScope, Prepared, Store};
use rusqlite::Connection;
use serde_json::{json, Map, Value};

use crate::episode_ops::{
    ensure_runtime_token_files, onboarding_subject, verify_runtime_token, RuntimeChannel,
};
use crate::gov_ops::{
    check_meta_binding, db_err, digest_json, mint, obj_pairs, run, ACTOR_GOVERNANCE,
};
use crate::part_common::conn_record_digest;
use crate::part_ops::event;
use crate::{gov_decision, state};

/// The Kovee model broker's actor string (§14.7
/// `onboarding_compute_permit_consume` row).
pub const ACTOR_MODEL_BROKER: &str = "kovee-adapter:model-broker";

/// §7.4 verbatim: the three operations an onboarding candidate channel may
/// reach, and the three its compute OUTPUT may drive.
pub const ALLOWED_OPERATIONS: [&str; 3] = [
    "membership_refuse",
    "membership_accept",
    "candidate_self_policy_propose",
];
/// §7.4 verbatim, `refuse` included as written (the catalog and
/// OnboardingActivationOffer say `membership_refuse` — recorded gap).
pub const ALLOWED_OUTPUT_OPERATIONS: [&str; 3] = [
    "refuse",
    "membership_accept",
    "candidate_self_policy_propose",
];

/// §7.4 fixes no output ceiling; pinned here (recorded deviation, G47).
pub const MAX_OUTPUT_BYTES: u64 = 65_536;
/// §7.4 pins presence, not a value shape, for the provider binding, region
/// and retention/training claims; these are the endpoint's derived labels
/// (recorded derivation, G47). Kovee's FINAL manifests arrive at consume.
pub const PROVIDER_BINDING_REF: &str = "kovee-model-broker";
pub const PROVIDER_REGION: &str = "kovee-realm-region";
pub const RETENTION_AND_TRAINING_CLAIMS: &str = "kovee-provider-retention-claims";

fn onboarding_fenced(detail: &str) -> Problem {
    Problem::new(
        ProblemKind::StaleBinding,
        "the onboarding fence advanced; unused onboarding authority is revoked",
    )
    .with_status(409)
    .with_detail(detail.to_owned())
}

fn json_text(v: &Value) -> Value {
    json!(v.to_string())
}

pub fn onboarding_ref(membership_offer_ref: &str) -> String {
    format!("onb-{membership_offer_ref}")
}

pub fn compute_intent_ref(onboarding_id: &str) -> String {
    format!("oci-{onboarding_id}")
}

pub fn stable_compute_key(compute_intent_id: &str) -> String {
    format!("occ-{compute_intent_id}")
}

/// The onboarding offer, its membership offer, and the CURRENT fence every
/// onboarding operation must present. A terminal membership offer or a
/// stale fence answers here, before anything is prepared.
struct Gate {
    onboarding: Map<String, Value>,
    fence_epoch: u64,
}

fn onboarding_gate(
    conn: &Connection,
    onboarding_id: &str,
    presented_fence: u64,
) -> Result<Gate, Problem> {
    let onboarding = rows::get_row(conn, "onboarding_offers", "onboarding_id", onboarding_id)
        .map_err(db_err)?
        .ok_or_else(state::not_found)?;
    let state_now = rows::str_of(&onboarding, "state").to_owned();
    if !matches!(state_now.as_str(), "offered" | "active") {
        return Err(onboarding_fenced(&format!(
            "the OnboardingActivationOffer is {state_now}: a terminal offer never admits, and a \
             new invitation requires a new offer, subject digest, candidate credential and fence"
        )));
    }
    let membership = rows::get_offer(conn, rows::str_of(&onboarding, "membership_offer_ref"))
        .map_err(db_err)?
        .ok_or_else(state::not_found)?;
    if !matches!(membership.state.as_str(), "offered" | "onboarding") {
        return Err(onboarding_fenced(&format!(
            "the MembershipOffer is {}: refusal, revocation and expiry all fence the workload",
            membership.state
        )));
    }
    let fence_epoch = membership.fence_epoch;
    if presented_fence != fence_epoch {
        return Err(onboarding_fenced(&format!(
            "presented onboarding fence {presented_fence}, current {fence_epoch}"
        )));
    }
    if rows::u64_of(&onboarding, "fence_epoch") != fence_epoch {
        return Err(onboarding_fenced(
            "the OnboardingActivationOffer fence is behind the MembershipOffer's",
        ));
    }
    Ok(Gate {
        onboarding,
        fence_epoch,
    })
}

// ==================================================== onboarding_offer ===

/// `onboarding_offer` (governance, create; §7.4, R10). Governance may fund
/// a bounded activation offer bound to the exact MembershipOffer, candidate
/// id, proposed Manifestation digest, minimal disclosed context, ONE
/// Episode, no general effect/child authority, resource ceiling and expiry.
/// When the invitation includes a hosted model call, it mints the
/// Society-authorized one-shot OnboardingComputeIntent in the SAME
/// transition.
pub fn onboarding_offer(
    store: &mut Store,
    req: &ops::OnboardingOfferRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    let society = rows::sole_society(store.conn())
        .map_err(db_err)?
        .ok_or_else(state::not_found)?;
    check_meta_binding(store, &req.meta, &society.society_id)?;
    let onboarding_id = onboarding_ref(&req.membership_offer_ref);
    let compute_id = compute_intent_ref(&onboarding_id);
    let compute_key = stable_compute_key(&compute_id);
    let offer_event = mint(store, "evt")?;
    let compute_event = mint(store, "evt")?;
    let created_at = rfc3339_utc(now);
    let scope = MutationScope {
        society_id: society.society_id.clone(),
        operation: "onboarding_offer".into(),
        actor: ACTOR_GOVERNANCE.into(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let req_c = req.clone();
    let sid = society.society_id.clone();
    let reply = run(store, scope, now, hooks, move |conn, _| {
        let membership = rows::get_offer(conn, &req_c.membership_offer_ref)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        if membership.state != "offered" {
            return Err(state::stale_binding(&format!(
                "the MembershipOffer is {}: onboarding is funded only against an offered \
                 invitation",
                membership.state
            )));
        }
        if membership.participant_ref != req_c.candidate_participant_ref {
            return Err(state::invalid(
                "candidate_participant_ref is not this MembershipOffer's candidate",
            ));
        }
        // The Society decision funding the invitation, resolved in full
        // (BY-A1): this is the Society's invitation/disclosure authority,
        // never candidate assent.
        let subject: bpp_core::digest::DigestRef = serde_json::from_str(&membership.subject_digest)
            .map_err(|_| state::internal("offer subject digest is not canonical"))?;
        gov_decision::resolve(
            conn,
            &req_c.adopted_by_decision_ref,
            &gov_decision::Expect {
                society_id: &sid,
                kind: gov_decision::KIND_MEMBERSHIP_ADMISSION,
                subject_kind: "membership_offer",
                subject_ref: &req_c.membership_offer_ref,
                subject_digest: &subject,
                actor: ACTOR_GOVERNANCE,
            },
        )?;
        if rows::get_row(conn, "onboarding_offers", "onboarding_id", &onboarding_id)
            .map_err(db_err)?
            .is_some()
        {
            return Err(state::stale_binding(
                "this MembershipOffer already carries an OnboardingActivationOffer: at most one \
                 Episode and one compute use per offer (§7.4)",
            ));
        }
        let fence_epoch = membership.fence_epoch;
        let record = json!({
            "onboarding_id": onboarding_id,
            "membership_offer_ref": req_c.membership_offer_ref,
            "candidate_participant_ref": req_c.candidate_participant_ref,
            "proposed_manifestation_ref": req_c.proposed_manifestation_ref,
            "proposed_manifestation_digest": digest_json(&req_c.proposed_manifestation_digest),
            "exact_context_ref": req_c.exact_context_ref,
            "exact_context_digest": digest_json(&req_c.exact_context_digest),
            "resource_reservation_ref": req_c.resource_reservation_ref,
            "max_episodes": 1,
            "allowed_operations": ALLOWED_OPERATIONS,
            "general_effect_and_child_authority": "none",
            "fence_epoch": fence_epoch,
            "expires_at": req_c.expires_at,
            "adopted_by_decision_ref": req_c.adopted_by_decision_ref,
            "state": "offered",
            "revision": 1,
        });
        let digest = conn_record_digest(
            conn,
            &sid,
            &onboarding_id,
            "bpp-onboarding-activation-offer-v0",
            &record,
        )?;
        let hosted = req_c.onboarding_compute_intent_ref.is_some();
        let mut effects = Vec::new();
        let mut events = Vec::new();
        if hosted {
            let cited = req_c
                .onboarding_compute_intent_ref
                .as_deref()
                .unwrap_or_default();
            if cited != compute_id {
                return Err(state::invalid(&format!(
                    "onboarding_compute_intent_ref must be the kernel-derived {compute_id}: the \
                     one-shot intent id is derived from the offer it funds, so a request can only \
                     match the server value"
                )));
            }
            // The §7.4 field list, verbatim. The Society authorizes the
            // EXACT disclosed context and proposed Manifestation; Kovee's
            // FINAL ProviderContextManifest / DisclosureManifest /
            // ModelProfile arrive at consume and land on the receipt
            // (§16.6 item 12; recorded derivation, gap note G47).
            let compute_record = json!({
                "compute_intent_id": compute_id,
                "onboarding_ref": onboarding_id,
                "society_id": sid,
                "proposed_manifestation_ref": req_c.proposed_manifestation_ref,
                "proposed_manifestation_digest":
                    digest_json(&req_c.proposed_manifestation_digest),
                "provider_context_manifest_ref": req_c.exact_context_ref,
                "provider_context_manifest_digest": digest_json(&req_c.exact_context_digest),
                "disclosure_manifest_ref": req_c.exact_context_ref,
                "disclosure_manifest_digest": digest_json(&req_c.exact_context_digest),
                "model_profile_ref": req_c.proposed_manifestation_ref,
                "model_profile_digest": digest_json(&req_c.proposed_manifestation_digest),
                "provider_binding_ref": PROVIDER_BINDING_REF,
                "region": PROVIDER_REGION,
                "retention_and_training_claims": RETENTION_AND_TRAINING_CLAIMS,
                "budget_reservation_set_ref": req_c.resource_reservation_ref,
                "candidate_fence_epoch": fence_epoch,
                "maximum_output_bytes": MAX_OUTPUT_BYTES,
                "allowed_output_operations": ALLOWED_OUTPUT_OPERATIONS,
                "tools_network_workspace_children": "none",
                "authorized_by_decision_ref": req_c.adopted_by_decision_ref,
                "expires_at": req_c.expires_at,
                "state": "authorized",
            });
            let compute_digest = conn_record_digest(
                conn,
                &sid,
                &compute_id,
                "bpp-onboarding-compute-intent-v0",
                &compute_record,
            )?;
            let mut full = compute_record.clone();
            if let Some(map) = full.as_object_mut() {
                map.insert("digest".into(), digest_json(&compute_digest));
            }
            effects.push(Effect::Upsert {
                table: "onboarding_compute_intents".into(),
                row: obj_pairs([
                    ("compute_intent_id", json!(compute_id)),
                    ("society_id", json!(sid)),
                    ("onboarding_ref", json!(onboarding_id)),
                    ("record", json_text(&full)),
                    ("candidate_fence_epoch", json!(fence_epoch)),
                    ("stable_compute_key", json!(compute_key)),
                    ("state", json!("authorized")),
                    ("receipt_ref", Value::Null),
                    ("expires_at", json!(req_c.expires_at)),
                    ("created_at", json!(created_at)),
                    ("digest", digest_json(&compute_digest)),
                ]),
            });
            events.push(event(
                &sid,
                &compute_event,
                "onboarding-compute-intent.authorized",
                &compute_id,
                1,
                &req_c.candidate_participant_ref,
                ACTOR_GOVERNANCE,
                &req_c.meta,
                json!({"state": "authorized", "max_uses": 1,
                       "tools_network_workspace_children": "none",
                       "authority": "the Society's invitation/disclosure authority, never \
                                     candidate assent (§7.4)"}),
            ));
        }
        effects.push(Effect::Upsert {
            table: "onboarding_offers".into(),
            row: obj_pairs([
                ("onboarding_id", json!(onboarding_id)),
                ("society_id", json!(sid)),
                ("membership_offer_ref", json!(req_c.membership_offer_ref)),
                (
                    "candidate_participant_ref",
                    json!(req_c.candidate_participant_ref),
                ),
                (
                    "proposed_manifestation_ref",
                    json!(req_c.proposed_manifestation_ref),
                ),
                (
                    "proposed_manifestation_digest",
                    digest_json(&req_c.proposed_manifestation_digest),
                ),
                ("exact_context_ref", json!(req_c.exact_context_ref)),
                (
                    "exact_context_digest",
                    digest_json(&req_c.exact_context_digest),
                ),
                (
                    "resource_reservation_ref",
                    json!(req_c.resource_reservation_ref),
                ),
                ("max_episodes", json!(1)),
                ("allowed_operations", json_text(&json!(ALLOWED_OPERATIONS))),
                (
                    "onboarding_compute_intent_ref",
                    if hosted {
                        json!(compute_id)
                    } else {
                        Value::Null
                    },
                ),
                ("general_effect_and_child_authority", json!("none")),
                ("fence_epoch", json!(fence_epoch)),
                ("expires_at", json!(req_c.expires_at)),
                (
                    "adopted_by_decision_ref",
                    json!(req_c.adopted_by_decision_ref),
                ),
                ("state", json!("offered")),
                ("revision", json!(1)),
                ("created_at", json!(created_at)),
                ("digest", digest_json(&digest)),
            ]),
        });
        // The MembershipOffer enters `onboarding` (§7.4 state list).
        let mut membership_row = membership.to_effect_row();
        membership_row.insert("state".into(), json!("onboarding"));
        membership_row.insert("revision".into(), json!(membership.revision + 1));
        effects.push(Effect::Upsert {
            table: "membership_offers".into(),
            row: membership_row,
        });
        events.push(event(
            &sid,
            &offer_event,
            "onboarding-activation-offer.offered",
            &onboarding_id,
            1,
            &req_c.candidate_participant_ref,
            ACTOR_GOVERNANCE,
            &req_c.meta,
            json!({"state": "offered", "max_episodes": 1,
                   "general_effect_and_child_authority": "none",
                   "hosted_compute": hosted,
                   "fence_epoch": fence_epoch}),
        ));
        let mut result = json!({
            "onboarding_id": onboarding_id,
            "revision": 1,
            "state": "offered",
            "fence_epoch": fence_epoch,
            "max_episodes": 1,
            "allowed_operations": ALLOWED_OPERATIONS,
            "general_effect_and_child_authority": "none",
            "membership_offer_state": "onboarding",
            "digest": digest_json(&digest),
        });
        if hosted {
            result["onboarding_compute_intent_ref"] = json!(compute_id);
            result["stable_compute_key"] = json!(compute_key);
        }
        Ok(Prepared {
            result,
            revision: Some(1),
            cursor: CursorMint::AfterEvents {
                society_id: sid.clone(),
            },
            effects,
            events,
        })
    })?;
    ensure_runtime_token_files(store);
    Ok(reply)
}

// =============================== onboarding_compute_permit_consume (R32) ==

/// `onboarding_compute_permit_consume` (runtime, update; R32). ONE compute
/// use per offer: the receipt's `max_uses` is 1 by construction, a second
/// consume under the same intent is refused, and an exact retry returns the
/// stored receipt.
pub fn onboarding_compute_permit_consume(
    store: &mut Store,
    token: &str,
    req: &ops::OnboardingComputePermitConsumeRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    // The broker channel is bound to the exact one-shot key.
    verify_runtime_token(
        store,
        token,
        RuntimeChannel::Broker,
        &req.stable_compute_key,
    )?;
    let intent = rows::get_row(
        store.conn(),
        "onboarding_compute_intents",
        "compute_intent_id",
        &req.compute_intent_ref,
    )
    .map_err(db_err)?
    .ok_or_else(state::not_found)?;
    let society_id = rows::str_of(&intent, "society_id").to_owned();
    check_meta_binding(store, &req.meta, &society_id)?;
    let receipt_id = mint(store, "ocr")?;
    let consume_event = mint(store, "evt")?;
    let issued_at = rfc3339_utc(now);
    let scope = MutationScope {
        society_id: society_id.clone(),
        operation: "onboarding_compute_permit_consume".into(),
        actor: ACTOR_MODEL_BROKER.into(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let req_c = req.clone();
    let society_c = society_id.clone();
    let reply = run(store, scope, now, hooks, move |conn, _| {
        let intent = rows::get_row(
            conn,
            "onboarding_compute_intents",
            "compute_intent_id",
            &req_c.compute_intent_ref,
        )
        .map_err(db_err)?
        .ok_or_else(state::not_found)?;
        if rows::str_of(&intent, "stable_compute_key") != req_c.stable_compute_key {
            return Err(state::stale_binding(
                "stable_compute_key is not this OnboardingComputeIntent's one-shot key",
            ));
        }
        let onboarding_id = rows::str_of(&intent, "onboarding_ref").to_owned();
        let gate = onboarding_gate(conn, &onboarding_id, req_c.onboarding_fence_epoch)?;
        if rows::u64_of(&intent, "candidate_fence_epoch") != gate.fence_epoch {
            return Err(onboarding_fenced(
                "the compute intent's candidate fence is stale: refusal, revocation or expiry \
                 revokes unused onboarding compute authority (§7.4)",
            ));
        }
        if req_c.meta.expected_revision != Some(rows::u64_of(&gate.onboarding, "revision")) {
            return Err(state::stale_revision());
        }
        if !req_c
            .compute_intent_digest
            .same_ref_json(&rows::json_of(&intent, "digest"))
        {
            return Err(state::stale_binding(
                "compute_intent_digest does not pin the authorized OnboardingComputeIntent",
            ));
        }
        let intent_state = rows::str_of(&intent, "state").to_owned();
        if intent_state != "authorized" {
            // At most ONE compute use per offer. Only the byte-identical
            // canonical binding replays the stored receipt.
            let stored = rows::get_row(
                conn,
                "onboarding_compute_receipts",
                "compute_intent_ref",
                &req_c.compute_intent_ref,
            )
            .map_err(db_err)?;
            if let Some(stored) = stored {
                let record: Value =
                    serde_json::from_str(rows::str_of(&stored, "record")).unwrap_or(Value::Null);
                let same = record
                    .get("provider_context_manifest_digest")
                    .is_some_and(|d| req_c.provider_context_manifest_digest.same_ref_json(d))
                    && record
                        .get("disclosure_manifest_digest")
                        .is_some_and(|d| req_c.disclosure_manifest_digest.same_ref_json(d))
                    && record
                        .get("model_profile_digest")
                        .is_some_and(|d| req_c.model_profile_digest.same_ref_json(d))
                    && rows::str_of(&stored, "kovee_invocation_ref") == req_c.kovee_invocation_ref;
                if same {
                    return Ok(Prepared {
                        result: receipt_result(&stored, true),
                        revision: Some(rows::u64_of(&gate.onboarding, "revision")),
                        cursor: CursorMint::AfterEvents {
                            society_id: society_c.clone(),
                        },
                        effects: Vec::new(),
                        events: Vec::new(),
                    });
                }
                return Err(Problem::new(
                    ProblemKind::IdempotencyMismatch,
                    "same one-shot compute key, different canonical request",
                )
                .with_status(409)
                .with_detail(
                    "at most ONE compute use per offer (§7.4 max_uses: 1): only the \
                     byte-identical canonical request replays the stored receipt"
                        .to_owned(),
                ));
            }
            return Err(state::stale_binding(&format!(
                "the OnboardingComputeIntent is {intent_state}: the one-shot compute authority is \
                 spent or revoked"
            )));
        }
        // The receipt: §7.4 field list verbatim, carrying KOVEE's FINAL
        // manifests (§16.6 item 12).
        let record = json!({
            "receipt_id": receipt_id,
            "compute_intent_ref": req_c.compute_intent_ref,
            "compute_intent_digest": rows::json_of(&intent, "digest"),
            "kovee_invocation_ref": req_c.kovee_invocation_ref,
            "candidate_fence_epoch": gate.fence_epoch,
            "provider_context_manifest_digest":
                digest_json(&req_c.provider_context_manifest_digest),
            "disclosure_manifest_digest": digest_json(&req_c.disclosure_manifest_digest),
            "model_profile_digest": digest_json(&req_c.model_profile_digest),
            "budget_reservation_set_ref":
                rows::str_of(&gate.onboarding, "resource_reservation_ref"),
            "max_uses": 1,
            "issued_at": issued_at,
            "expires_at": rows::str_of(&intent, "expires_at"),
        });
        let digest = conn_record_digest(
            conn,
            &society_c,
            &receipt_id,
            "bpp-onboarding-compute-receipt-v0",
            &record,
        )?;
        let mut full = record.clone();
        if let Some(map) = full.as_object_mut() {
            map.insert("digest".into(), digest_json(&digest));
            map.insert(
                "provider_context_manifest_ref".into(),
                json!(req_c.provider_context_manifest_ref),
            );
            map.insert(
                "disclosure_manifest_ref".into(),
                json!(req_c.disclosure_manifest_ref),
            );
            map.insert("model_profile_ref".into(), json!(req_c.model_profile_ref));
        }
        let receipt_row = obj_pairs([
            ("receipt_id", json!(receipt_id)),
            ("society_id", json!(society_c)),
            ("compute_intent_ref", json!(req_c.compute_intent_ref)),
            ("stable_compute_key", json!(req_c.stable_compute_key)),
            ("record", json_text(&full)),
            ("max_uses", json!(1)),
            ("candidate_fence_epoch", json!(gate.fence_epoch)),
            ("kovee_invocation_ref", json!(req_c.kovee_invocation_ref)),
            ("issued_at", json!(issued_at)),
            ("expires_at", json!(rows::str_of(&intent, "expires_at"))),
            ("digest", digest_json(&digest)),
        ]);
        let mut consumed = intent.clone();
        consumed.insert("state".into(), json!("consumed"));
        consumed.insert("receipt_ref".into(), json!(receipt_id));
        // §14.8 OnboardingActivationOffer: offered -> active via
        // onboarding_compute_permit_consume.
        let onboarding_revision = rows::u64_of(&gate.onboarding, "revision") + 1;
        let mut offer_row = gate.onboarding.clone();
        offer_row.insert("state".into(), json!("active"));
        offer_row.insert("revision".into(), json!(onboarding_revision));
        Ok(Prepared {
            result: receipt_result(&receipt_row, false),
            revision: Some(onboarding_revision),
            cursor: CursorMint::AfterEvents {
                society_id: society_c.clone(),
            },
            effects: vec![
                Effect::Upsert {
                    table: "onboarding_compute_receipts".into(),
                    row: receipt_row.clone(),
                },
                Effect::Upsert {
                    table: "onboarding_compute_intents".into(),
                    row: consumed,
                },
                Effect::Upsert {
                    table: "onboarding_offers".into(),
                    row: offer_row,
                },
            ],
            events: vec![event(
                &society_c,
                &consume_event,
                "onboarding-compute-intent.consumed",
                &req_c.compute_intent_ref,
                1,
                rows::str_of(&gate.onboarding, "candidate_participant_ref"),
                ACTOR_MODEL_BROKER,
                &req_c.meta,
                json!({"state": "consumed", "max_uses": 1,
                       "receipt_ref": receipt_id,
                       "assent": "starting the compute is the Society's invitation, not the \
                                  candidate's assent (§7.4)"}),
            )],
        })
    })?;
    ensure_runtime_token_files(store);
    Ok(reply)
}

fn receipt_result(row: &Map<String, Value>, replayed: bool) -> Value {
    let record = rows::json_of(row, "record");
    let mut out = json!({
        "receipt_id": rows::str_of(row, "receipt_id"),
        "compute_intent_ref": rows::str_of(row, "compute_intent_ref"),
        "stable_compute_key": rows::str_of(row, "stable_compute_key"),
        "candidate_fence_epoch": rows::u64_of(row, "candidate_fence_epoch"),
        "kovee_invocation_ref": rows::str_of(row, "kovee_invocation_ref"),
        "max_uses": 1,
        "issued_at": rows::str_of(row, "issued_at"),
        "expires_at": rows::str_of(row, "expires_at"),
        "digest": rows::json_of(row, "digest"),
        "onboarding_compute_receipt": record,
        "grants": {
            "tools": "none", "network": "none", "workspace": "none",
            "children": "none", "reusable_participant_authority": "none",
        },
    });
    if replayed {
        out["replayed"] = json!(true);
    }
    out
}

// ======================================= onboarding_episode_claim (R31) ==

/// `onboarding_episode_claim` (runtime, create; R31): the candidate
/// workload claims the ONE onboarding Episode this offer funds.
pub fn onboarding_episode_claim(
    store: &mut Store,
    token: &str,
    req: &ops::OnboardingEpisodeClaimRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    verify_runtime_token(
        store,
        token,
        RuntimeChannel::Onboarding,
        &onboarding_subject(&req.onboarding_ref, req.onboarding_fence_epoch),
    )?;
    let onboarding = rows::get_row(
        store.conn(),
        "onboarding_offers",
        "onboarding_id",
        &req.onboarding_ref,
    )
    .map_err(db_err)?
    .ok_or_else(state::not_found)?;
    let society_id = rows::str_of(&onboarding, "society_id").to_owned();
    check_meta_binding(store, &req.meta, &society_id)?;
    let episode_id = mint(store, "onbep")?;
    let claim_event = mint(store, "evt")?;
    let claimed_at = rfc3339_utc(now);
    let scope = MutationScope {
        society_id: society_id.clone(),
        operation: "onboarding_episode_claim".into(),
        actor: format!("candidate-runtime:{}", req.holder_runtime_binding),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let req_c = req.clone();
    let society_c = society_id.clone();
    let reply = run(store, scope, now, hooks, move |conn, scope| {
        let gate = onboarding_gate(conn, &req_c.onboarding_ref, req_c.onboarding_fence_epoch)?;
        if rows::str_of(&gate.onboarding, "candidate_participant_ref")
            != req_c.candidate_participant_ref
        {
            return Err(state::invalid(
                "candidate_participant_ref is not this offer's candidate",
            ));
        }
        if rows::str_of(&gate.onboarding, "proposed_manifestation_ref")
            != req_c.proposed_manifestation_ref
            || !req_c
                .proposed_manifestation_digest
                .same_ref_json(&rows::json_of(
                    &gate.onboarding,
                    "proposed_manifestation_digest",
                ))
        {
            return Err(state::stale_binding(
                "the claim does not pin the exact proposed Manifestation the Society authorized",
            ));
        }
        // The exact retry returns the identical row.
        if let Some(existing) = rows::get_row(
            conn,
            "onboarding_episodes",
            "stable_claim_key",
            &req_c.stable_claim_key,
        )
        .map_err(db_err)?
        {
            return Ok(Prepared {
                result: episode_result(&existing, true),
                revision: Some(rows::u64_of(&existing, "revision")),
                cursor: CursorMint::AfterEvents {
                    society_id: society_c.clone(),
                },
                effects: Vec::new(),
                events: Vec::new(),
            });
        }
        // max_episodes: 1 (§7.4) — a second claim under a different key is
        // refused, whatever its state.
        if !rows::rows_where(
            conn,
            "onboarding_episodes",
            "onboarding_ref",
            &req_c.onboarding_ref,
            "onboarding_episode_id",
        )
        .map_err(db_err)?
        .is_empty()
        {
            return Err(state::stale_binding(
                "this OnboardingActivationOffer already spent its ONE Episode (max_episodes: 1)",
            ));
        }
        // A hosted run cites the exact OnboardingComputeReceipt (R31).
        let receipt_ref = match (&req_c.compute_receipt_ref, &req_c.compute_receipt_digest) {
            (Some(cited), Some(digest)) => {
                let receipt =
                    rows::get_row(conn, "onboarding_compute_receipts", "receipt_id", cited)
                        .map_err(db_err)?
                        .ok_or_else(state::not_found)?;
                if rows::str_of(&receipt, "compute_intent_ref")
                    != rows::str_of(&gate.onboarding, "onboarding_compute_intent_ref")
                {
                    return Err(state::stale_binding(
                        "the cited OnboardingComputeReceipt belongs to another offer",
                    ));
                }
                if !digest.same_ref_json(&rows::json_of(&receipt, "digest")) {
                    return Err(state::stale_binding(
                        "compute_receipt_digest does not pin the minted receipt",
                    ));
                }
                if rows::u64_of(&receipt, "candidate_fence_epoch") != gate.fence_epoch {
                    return Err(onboarding_fenced(
                        "the cited receipt was minted under a superseded onboarding fence",
                    ));
                }
                Some(cited.clone())
            }
            _ => None,
        };
        let record = json!({
            "onboarding_episode_id": episode_id,
            "onboarding_ref": req_c.onboarding_ref,
            "candidate_participant_ref": req_c.candidate_participant_ref,
            "proposed_manifestation_ref": req_c.proposed_manifestation_ref,
            "compute_receipt_ref": receipt_ref,
            "onboarding_fence_epoch": gate.fence_epoch,
            "holder_runtime_binding": req_c.holder_runtime_binding,
            "stable_claim_key": req_c.stable_claim_key,
            "state": "running",
            "claimed_at": claimed_at,
        });
        let digest = conn_record_digest(
            conn,
            &society_c,
            &episode_id,
            "bpp-onboarding-episode-v0",
            &record,
        )?;
        let row = obj_pairs([
            ("onboarding_episode_id", json!(episode_id)),
            ("society_id", json!(society_c)),
            ("onboarding_ref", json!(req_c.onboarding_ref)),
            (
                "candidate_participant_ref",
                json!(req_c.candidate_participant_ref),
            ),
            (
                "proposed_manifestation_ref",
                json!(req_c.proposed_manifestation_ref),
            ),
            (
                "compute_receipt_ref",
                receipt_ref
                    .as_ref()
                    .map(|r| json!(r))
                    .unwrap_or(Value::Null),
            ),
            ("onboarding_fence_epoch", json!(gate.fence_epoch)),
            (
                "holder_runtime_binding",
                json!(req_c.holder_runtime_binding),
            ),
            ("stable_claim_key", json!(req_c.stable_claim_key)),
            ("revision", json!(1)),
            ("state", json!("running")),
            ("outcome", Value::Null),
            ("output_refs", json_text(&json!([]))),
            ("evidence_refs", json_text(&json!([]))),
            // Named on the row itself: no acceptance, ever, from runtime.
            ("acceptance_effect", json!("none")),
            ("claimed_at", json!(claimed_at)),
            ("completed_at", Value::Null),
            ("digest", digest_json(&digest)),
        ]);
        // §14.8: offered -> active via onboarding_episode_claim.
        let onboarding_revision = rows::u64_of(&gate.onboarding, "revision") + 1;
        let mut offer_row = gate.onboarding.clone();
        offer_row.insert("state".into(), json!("active"));
        offer_row.insert("revision".into(), json!(onboarding_revision));
        Ok(Prepared {
            result: episode_result(&row, false),
            revision: Some(1),
            cursor: CursorMint::AfterEvents {
                society_id: society_c.clone(),
            },
            effects: vec![
                Effect::Upsert {
                    table: "onboarding_episodes".into(),
                    row,
                },
                Effect::Upsert {
                    table: "onboarding_offers".into(),
                    row: offer_row,
                },
            ],
            events: vec![event(
                &society_c,
                &claim_event,
                "onboarding-episode.claimed",
                &episode_id,
                1,
                &req_c.candidate_participant_ref,
                &scope.actor,
                &req_c.meta,
                json!({"state": "running", "max_episodes": 1,
                       "compute_receipt_ref": receipt_ref,
                       "general_effect_and_child_authority": "none"}),
            )],
        })
    })?;
    ensure_runtime_token_files(store);
    Ok(reply)
}

// ==================================== onboarding_episode_complete (R31) ==

/// `onboarding_episode_complete` (runtime, update; R31). Completion is
/// EVIDENCE ONLY: it records outputs and evidence, moves the offer to
/// `completed`, and creates NO MembershipAcceptance, NO Standing, and no
/// Participant authority of any kind. Silence or failure expires; it never
/// becomes acceptance (§7.4).
pub fn onboarding_episode_complete(
    store: &mut Store,
    token: &str,
    req: &ops::OnboardingEpisodeCompleteRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    verify_runtime_token(
        store,
        token,
        RuntimeChannel::Onboarding,
        &onboarding_subject(&req.onboarding_ref, req.onboarding_fence_epoch),
    )?;
    let onboarding = rows::get_row(
        store.conn(),
        "onboarding_offers",
        "onboarding_id",
        &req.onboarding_ref,
    )
    .map_err(db_err)?
    .ok_or_else(state::not_found)?;
    let society_id = rows::str_of(&onboarding, "society_id").to_owned();
    check_meta_binding(store, &req.meta, &society_id)?;
    let complete_event = mint(store, "evt")?;
    let completed_at = rfc3339_utc(now);
    let scope = MutationScope {
        society_id: society_id.clone(),
        operation: "onboarding_episode_complete".into(),
        actor: ACTOR_MODEL_BROKER.into(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let req_c = req.clone();
    let society_c = society_id.clone();
    let reply = run(store, scope, now, hooks, move |conn, _| {
        let gate = onboarding_gate(conn, &req_c.onboarding_ref, req_c.onboarding_fence_epoch)?;
        let episode = rows::get_row(
            conn,
            "onboarding_episodes",
            "onboarding_episode_id",
            &req_c.onboarding_episode_ref,
        )
        .map_err(db_err)?
        .ok_or_else(state::not_found)?;
        if rows::str_of(&episode, "onboarding_ref") != req_c.onboarding_ref {
            return Err(state::invalid(
                "the onboarding Episode belongs to another offer",
            ));
        }
        if req_c.meta.expected_revision != Some(rows::u64_of(&episode, "revision")) {
            return Err(state::stale_revision());
        }
        if rows::str_of(&episode, "state") != "running" {
            return Err(state::stale_binding(&format!(
                "the onboarding Episode is {}: it completes exactly once",
                rows::str_of(&episode, "state")
            )));
        }
        let revision = rows::u64_of(&episode, "revision") + 1;
        let mut completed = episode.clone();
        completed.insert("revision".into(), json!(revision));
        completed.insert("state".into(), json!("completed"));
        completed.insert("outcome".into(), json!(req_c.outcome));
        completed.insert("output_refs".into(), json_text(&json!(req_c.output_refs)));
        completed.insert(
            "evidence_refs".into(),
            json_text(&json!(req_c.evidence_refs)),
        );
        completed.insert("acceptance_effect".into(), json!("none"));
        completed.insert("completed_at".into(), json!(completed_at));
        // §14.8: active -> completed via onboarding_episode_complete. The
        // MembershipOffer is NOT touched: acceptance is a candidate act on
        // the candidate surface, never a runtime outcome.
        let onboarding_revision = rows::u64_of(&gate.onboarding, "revision") + 1;
        let mut offer_row = gate.onboarding.clone();
        offer_row.insert("state".into(), json!("completed"));
        offer_row.insert("revision".into(), json!(onboarding_revision));
        Ok(Prepared {
            result: json!({
                "onboarding_episode_ref": req_c.onboarding_episode_ref,
                "revision": revision,
                "state": "completed",
                "outcome": req_c.outcome,
                "onboarding_state": "completed",
                "output_refs": req_c.output_refs,
                "evidence_refs": req_c.evidence_refs,
                // The whole point, in the reply.
                "completion_is_evidence_only": true,
                "acceptance": {
                    "membership_accepted": false,
                    "membership_acceptance_ref": Value::Null,
                    "standing_created": false,
                    "participant_authority_granted": false,
                },
                "membership_offer_state": "onboarding (untouched: acceptance is a candidate act \
                                           on the candidate surface, §7.4)",
                "completed_at": completed_at,
            }),
            revision: Some(revision),
            cursor: CursorMint::AfterEvents {
                society_id: society_c.clone(),
            },
            effects: vec![
                Effect::Upsert {
                    table: "onboarding_episodes".into(),
                    row: completed,
                },
                Effect::Upsert {
                    table: "onboarding_offers".into(),
                    row: offer_row,
                },
            ],
            events: vec![event(
                &society_c,
                &complete_event,
                "onboarding-episode.completed",
                &req_c.onboarding_episode_ref,
                revision,
                rows::str_of(&gate.onboarding, "candidate_participant_ref"),
                ACTOR_MODEL_BROKER,
                &req_c.meta,
                json!({"state": "completed", "outcome": req_c.outcome,
                       "acceptance": "none — runtime output is never membership assent \
                                      (§16.6 item 12)",
                       "standing_created": false}),
            )],
        })
    })?;
    ensure_runtime_token_files(store);
    Ok(reply)
}

fn episode_result(row: &Map<String, Value>, replayed: bool) -> Value {
    let mut out = json!({
        "onboarding_episode_id": rows::str_of(row, "onboarding_episode_id"),
        "onboarding_ref": rows::str_of(row, "onboarding_ref"),
        "revision": rows::u64_of(row, "revision"),
        "state": rows::str_of(row, "state"),
        "onboarding_fence_epoch": rows::u64_of(row, "onboarding_fence_epoch"),
        "compute_receipt_ref": match row.get("compute_receipt_ref").and_then(Value::as_str) {
            Some(s) if !s.is_empty() => json!(s),
            _ => Value::Null,
        },
        "stable_claim_key": rows::str_of(row, "stable_claim_key"),
        "max_episodes": 1,
        "acceptance_effect": "none",
        "allowed_output_operations": ALLOWED_OUTPUT_OPERATIONS,
        "general_effect_and_child_authority": "none",
        "claimed_at": rows::str_of(row, "claimed_at"),
        "digest": rows::json_of(row, "digest"),
    });
    if replayed {
        out["replayed"] = json!(true);
    }
    out
}

/// Fences the onboarding path when a refusal, revocation or expiry advances
/// the MembershipOffer fence: the OnboardingActivationOffer goes terminal
/// and unused compute authority is revoked (§7.4). Returns the effects to
/// stage inside the SAME transaction.
pub fn fence_onboarding(
    conn: &Connection,
    membership_offer_ref: &str,
    new_state: &str,
    new_fence: u64,
) -> Result<Vec<Effect>, Problem> {
    let mut effects = Vec::new();
    let Some(onboarding) = rows::rows_where(
        conn,
        "onboarding_offers",
        "membership_offer_ref",
        membership_offer_ref,
        "onboarding_id",
    )
    .map_err(db_err)?
    .into_iter()
    .next() else {
        return Ok(effects);
    };
    let onboarding_id = rows::str_of(&onboarding, "onboarding_id").to_owned();
    let mut row = onboarding.clone();
    row.insert("state".into(), json!(new_state));
    row.insert("fence_epoch".into(), json!(new_fence));
    row.insert(
        "revision".into(),
        json!(rows::u64_of(&onboarding, "revision") + 1),
    );
    effects.push(Effect::Upsert {
        table: "onboarding_offers".into(),
        row,
    });
    // Unused compute authority is revoked, never left consumable.
    for intent in rows::rows_where(
        conn,
        "onboarding_compute_intents",
        "onboarding_ref",
        &onboarding_id,
        "compute_intent_id",
    )
    .map_err(db_err)?
    {
        if rows::str_of(&intent, "state") == "authorized" {
            let mut revoked = intent.clone();
            revoked.insert("state".into(), json!("failed"));
            effects.push(Effect::Upsert {
                table: "onboarding_compute_intents".into(),
                row: revoked,
            });
        }
    }
    Ok(effects)
}
