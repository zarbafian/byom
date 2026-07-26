//! B3 slice 3 — the two byom-side seams a Kovee turn needs around an
//! Episode:
//!
//! 1. **`attention_notice_record`** (runtime, create; §11.1/§16.4, family
//!    contract L25 — DERIVED name, gap note G47). Kovee Attention may
//!    notify byom that a source state changed. A notice is EVIDENCE: it
//!    may at most make a participant's OWN already-submitted WakeIntent
//!    eligible under its already-adopted ActivationPolicy. It never
//!    creates a WakeIntent, an ActivationAdmission, a ResourceAllocation,
//!    or an Episode — the request shape carries no member through which it
//!    could, and the record names the four things it did not create.
//!
//! 2. **`context_manifest_show`** (projection, read; §12.1, R4) — the
//!    EXACT byom source fields §16.6 item 5 adds to Kovee's
//!    ProviderContextManifest, field-verbatim per the frozen C2 fragment.
//!    Possession grants nothing: the read rechecks the exact
//!    Episode/attempt binding and the exact ContextManifest ref, and
//!    `ByomEpisodeBinding.context_source_digest` is the digest over
//!    exactly this canonical fragment.

use bpp_core::canonical::{sha256_hex, tagged_canonical};
use bpp_core::digest::DigestRef;
use bpp_core::envelope::Success;
use bpp_core::ops;
use bpp_core::problem::Problem;
use bpp_core::time::rfc3339_utc;
use byom_store::effects::Effect;
use byom_store::rows;
use byom_store::{CrashHooks, CursorMint, MutationScope, Prepared, Store};
use rusqlite::Connection;
use serde_json::{json, Map, Value};

use crate::episode_ops::{episode_subject, head_row, verify_runtime_token, RuntimeChannel};
use crate::gov_ops::{check_meta_binding, db_err, digest_json, mint, obj_pairs, run};
use crate::part_common::conn_record_digest;
use crate::part_ops::event;
use crate::state;

/// The narrow Kovee attention adapter's actor string.
pub const ACTOR_ATTENTION: &str = "kovee-adapter:attention";

/// The canonicalization domain of the §12.1 source-field fragment.
pub const CONTEXT_SOURCE_TAG: &str = "bpp-provider-context-source-v0";

/// The frozen required member set of
/// `spec/governed-work/provider-context-manifest-byom-fields.schema.json`,
/// transcribed verbatim and in schema order. The composer emits EXACTLY
/// these members — no more, no fewer.
pub const CONTEXT_SOURCE_FIELDS: [&str; 17] = [
    "byom_endpoint_ref",
    "society_ref",
    "participant_ref",
    "participant_binding_epoch",
    "activity_stream_ref",
    "episode_ref",
    "byom_attempt_ref",
    "byom_fence_epoch",
    "context_manifest_ref",
    "context_manifest_digest",
    "ordered_source_items",
    "classification_overlay_digest",
    "purpose_ref",
    "mandate_use_refs",
    "disclosure_ceiling_ref",
    "explicit_omissions",
    "authorization_dependency_digest",
];

/// The two §11.1 effects a notification may have. Neither is a wake.
pub const EFFECT_NONE: &str = "no_effect";
pub const EFFECT_ELIGIBLE: &str = "wake_intent_eligible";

fn json_text(v: &Value) -> Value {
    json!(v.to_string())
}

/// A nullable TEXT column as a JSON string (never a parsed body).
fn opt_str(row: &Map<String, Value>, key: &str) -> Value {
    match row.get(key).and_then(Value::as_str) {
        Some(s) if !s.is_empty() => json!(s),
        _ => Value::Null,
    }
}

// ============================================ the §12.1 source fragment ==

/// Everything the §12.1 fragment needs that is NOT a stable committed row:
/// the attempt and its fence, the Episode's ContextManifest pin, and the
/// MandateUse refs the binding carries. `episode_claim` passes its STAGED
/// values (the attempt row is not committed yet); the projection read
/// passes the committed ones.
pub struct ContextSourceInput<'a> {
    pub byom_endpoint_ref: &'a str,
    pub episode: &'a Map<String, Value>,
    pub byom_attempt_ref: &'a str,
    pub byom_fence_epoch: u64,
    pub context_manifest_ref: &'a str,
    pub context_manifest_digest: Value,
    pub mandate_use_refs: Value,
}

/// Composes the EXACT §12.1 byom source fields for one bound
/// Episode/attempt, from committed state only, plus the `portable_public`
/// digest over exactly that canonical fragment.
pub fn context_source_fields(
    conn: &Connection,
    input: &ContextSourceInput<'_>,
) -> Result<(Value, DigestRef), Problem> {
    let episode = input.episode;
    let society_id = rows::str_of(episode, "society_id").to_owned();
    let episode_ref = rows::str_of(episode, "episode_id").to_owned();
    let byom_attempt_ref = input.byom_attempt_ref;
    let context_manifest_ref = input.context_manifest_ref.to_owned();
    let participant_ref = rows::str_of(episode, "participant_ref").to_owned();
    let participant = rows::get_participant(conn, &participant_ref)
        .map_err(db_err)?
        .ok_or_else(state::not_found)?;
    let society = rows::get_society(conn, &society_id)
        .map_err(db_err)?
        .ok_or_else(state::not_found)?;
    let stream = rows::get_row(
        conn,
        "activity_streams",
        "activity_stream_id",
        rows::str_of(episode, "activity_stream_ref"),
    )
    .map_err(db_err)?
    .ok_or_else(state::not_found)?;
    let mandate = rows::get_row(
        conn,
        "mandates",
        "mandate_id",
        rows::str_of(episode, "mandate_ref"),
    )
    .map_err(db_err)?
    .ok_or_else(state::not_found)?;
    let allocation = rows::get_row(
        conn,
        "resource_allocations",
        "allocation_id",
        rows::str_of(episode, "resource_allocation_ref"),
    )
    .map_err(db_err)?
    .ok_or_else(state::not_found)?;
    let wake = rows::get_row(
        conn,
        "wake_intents",
        "wake_intent_id",
        rows::str_of(episode, "wake_intent_ref"),
    )
    .map_err(db_err)?
    .ok_or_else(state::not_found)?;
    let mandate_use_refs = input.mandate_use_refs.clone();
    // The ordered source items: byom's SOURCE order, exactly the immutable
    // inputs this Episode admits. Kovee owns the final provider-visible
    // ordering and bytes.
    let ordered_source_items = json!([
        {"ref": context_manifest_ref,
         "digest": input.context_manifest_digest},
        {"ref": rows::str_of(&wake, "exact_cause_ref"),
         "digest": rows::json_of(&wake, "exact_cause_digest")},
    ]);
    // §12.1 names a disclosure ceiling but no record; the Mandate's
    // context ceiling is that ceiling when set, else the ceiling derived
    // from the Mandate itself (recorded derivation, gap note G47).
    let ceiling = rows::str_of(&mandate, "context_ceiling_ref").to_owned();
    let disclosure_ceiling_ref = if ceiling.is_empty() {
        format!("ceiling-{}", rows::str_of(&mandate, "mandate_id"))
    } else {
        ceiling
    };
    let mut fragment = Map::new();
    fragment.insert("byom_endpoint_ref".into(), json!(input.byom_endpoint_ref));
    fragment.insert("society_ref".into(), json!(society_id));
    fragment.insert("participant_ref".into(), json!(participant_ref));
    fragment.insert(
        "participant_binding_epoch".into(),
        json!(participant.binding_epoch),
    );
    fragment.insert(
        "activity_stream_ref".into(),
        json!(rows::str_of(episode, "activity_stream_ref")),
    );
    fragment.insert("episode_ref".into(), json!(episode_ref));
    fragment.insert("byom_attempt_ref".into(), json!(byom_attempt_ref));
    fragment.insert("byom_fence_epoch".into(), json!(input.byom_fence_epoch));
    fragment.insert("context_manifest_ref".into(), json!(context_manifest_ref));
    fragment.insert(
        "context_manifest_digest".into(),
        input.context_manifest_digest.clone(),
    );
    fragment.insert("ordered_source_items".into(), ordered_source_items);
    fragment.insert(
        "classification_overlay_digest".into(),
        serde_json::from_str(&society.classification_binding_digest).unwrap_or(Value::Null),
    );
    fragment.insert(
        "purpose_ref".into(),
        json!(rows::str_of(&stream, "purpose_ref")),
    );
    fragment.insert("mandate_use_refs".into(), mandate_use_refs);
    fragment.insert(
        "disclosure_ceiling_ref".into(),
        json!(disclosure_ceiling_ref),
    );
    // Nothing is silently omitted: an omission is explicit or absent.
    fragment.insert("explicit_omissions".into(), json!([]));
    fragment.insert(
        "authorization_dependency_digest".into(),
        rows::json_of(&allocation, "dependency_digest"),
    );
    // The fragment is EXACTLY the frozen member set: no convenience context
    // may be appended outside Kovee's final manifest chain (§12.1), and a
    // missing member would leave Kovee binding an incomplete source
    // relation. Checked here, so the composer cannot drift from the C2
    // contract silently.
    let mut emitted: Vec<&str> = fragment.keys().map(String::as_str).collect();
    let mut frozen = CONTEXT_SOURCE_FIELDS.to_vec();
    emitted.sort_unstable();
    frozen.sort_unstable();
    if emitted != frozen {
        return Err(state::internal(
            "the composed provider-context source fragment is not the frozen §12.1 member set",
        ));
    }
    let value = Value::Object(fragment);
    let bytes = tagged_canonical(CONTEXT_SOURCE_TAG, &value)
        .map_err(|e| state::internal(&e.to_string()))?;
    let digest = DigestRef::portable_public(sha256_hex(&bytes));
    Ok((value, digest))
}

/// The endpoint reference the fragment names.
pub fn byom_endpoint_ref(store: &Store) -> String {
    match crate::host_config::HostConfig::load(store) {
        Ok(cfg) => cfg.realm_byom_binding.byom_endpoint_ref.clone(),
        Err(_) => "byom-endpoint-local".to_owned(),
    }
}

/// `context_manifest_show` (projection, read; R4). Refuses when the
/// Episode/attempt refs do not pin a committed bound binding, or when
/// `context_manifest_ref` is not the Episode's committed ContextManifest.
pub fn context_manifest_show(
    store: &Store,
    req: &ops::ContextManifestShowRequest,
) -> Result<Vec<u8>, Problem> {
    let conn = store.conn();
    let episode = rows::get_row(conn, "episodes", "episode_id", &req.episode_ref)
        .map_err(db_err)?
        .ok_or_else(state::not_found)?;
    let binding = head_row(
        conn,
        "byom_episode_bindings",
        "episode_ref",
        &req.episode_ref,
        "byom_attempt_ref",
        &req.byom_attempt_ref,
    )?
    .ok_or_else(state::not_found)?;
    if rows::str_of(&binding, "state") != "bound" {
        return Err(state::stale_binding(
            "the ByomEpisodeBinding is fenced or released: a superseded attempt materializes \
             nothing",
        ));
    }
    let attempt = rows::get_row(
        conn,
        "episode_attempts",
        "attempt_id",
        &req.byom_attempt_ref,
    )
    .map_err(db_err)?
    .ok_or_else(state::not_found)?;
    let committed = rows::str_of(&episode, "context_manifest_ref").to_owned();
    if committed.is_empty() {
        return Err(state::stale_binding(
            "the Episode carries no committed ContextManifest: the source fields exist only from \
             the claim that bound one",
        ));
    }
    if committed != req.context_manifest_ref {
        return Err(state::stale_binding(
            "context_manifest_ref does not name the Episode's committed ContextManifest: an \
             erased or revoked input fails materialization, and a new manifest is never silently \
             substituted (§12.1)",
        ));
    }
    let record: Value =
        serde_json::from_str(rows::str_of(&binding, "record")).unwrap_or(Value::Null);
    let mandate_use_refs = record
        .get("mandate_use_refs")
        .cloned()
        .unwrap_or_else(|| json!([]));
    let (fields, digest) = context_source_fields(
        conn,
        &ContextSourceInput {
            byom_endpoint_ref: &byom_endpoint_ref(store),
            episode: &episode,
            byom_attempt_ref: &req.byom_attempt_ref,
            byom_fence_epoch: rows::u64_of(&attempt, "byom_fence_epoch"),
            context_manifest_ref: &committed,
            context_manifest_digest: rows::json_of(&episode, "context_manifest_digest"),
            mandate_use_refs,
        },
    )?;
    let result = json!({
        "episode_ref": req.episode_ref,
        "byom_attempt_ref": req.byom_attempt_ref,
        "context_manifest_ref": req.context_manifest_ref,
        "context_manifest_digest": rows::json_of(&episode, "context_manifest_digest"),
        "provider_context_manifest_byom_fields": fields,
        "context_source_digest": digest_json(&digest),
        "byom_episode_binding_context_source_digest":
            record.get("context_source_digest_recomputed").cloned()
                .unwrap_or(Value::Null),
        "owner": "kovee owns the ProviderContextManifest and the final provider-visible \
                  ordering and bytes; this is a SOURCE fragment, not a manifest",
        "materialization": "possession grants nothing: every materialization rechecks current \
                            visibility, admission, erasure, classification, standing, purpose \
                            and Mandate (§12.1)",
    });
    serde_json::to_vec(&Success::new(result)).map_err(|e| state::internal(&e.to_string()))
}

// ============================================== attention_notice_record ==

/// `attention_notice_record` (runtime, create). NOTIFICATION IS NEVER A
/// WAKE (§11.1, family contract L25): the notice is committed as evidence
/// and the server computes its at-most effect.
pub fn attention_notice_record(
    store: &mut Store,
    token: &str,
    req: &ops::AttentionNoticeRecordRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    verify_runtime_token(
        store,
        token,
        RuntimeChannel::Attention,
        &episode_subject(&req.activity_stream_ref, req.generation),
    )?;
    let stream = rows::get_row(
        store.conn(),
        "activity_streams",
        "activity_stream_id",
        &req.activity_stream_ref,
    )
    .map_err(db_err)?
    .ok_or_else(state::not_found)?;
    let society_id = rows::str_of(&stream, "society_id").to_owned();
    check_meta_binding(store, &req.meta, &society_id)?;
    let notice_id = mint(store, "atn")?;
    let notice_event = mint(store, "evt")?;
    let received_at = rfc3339_utc(now);
    let scope = MutationScope {
        society_id: society_id.clone(),
        operation: "attention_notice_record".into(),
        actor: ACTOR_ATTENTION.into(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let req_c = req.clone();
    let society_c = society_id.clone();
    run(store, scope, now, hooks, move |conn, _| {
        let stream = rows::get_row(
            conn,
            "activity_streams",
            "activity_stream_id",
            &req_c.activity_stream_ref,
        )
        .map_err(db_err)?
        .ok_or_else(state::not_found)?;
        if rows::u64_of(&stream, "generation") != req_c.generation {
            return Err(state::stale_binding("stale activity generation fence"));
        }
        // The exact retry returns the identical notice.
        if let Some(existing) = rows::get_row(
            conn,
            "attention_notices",
            "stable_notice_key",
            &req_c.stable_notice_key,
        )
        .map_err(db_err)?
        {
            return Ok(Prepared {
                result: notice_result(&existing, true),
                revision: Some(1),
                cursor: CursorMint::AfterEvents {
                    society_id: society_c.clone(),
                },
                effects: Vec::new(),
                events: Vec::new(),
            });
        }
        let participant_ref = rows::str_of(&stream, "participant_ref").to_owned();
        // The AT MOST effect: a notification may make the participant's
        // OWN already-submitted WakeIntent eligible under its ALREADY
        // ADOPTED ActivationPolicy. It cannot author one, and no policy of
        // any other participant is consulted.
        let policy = rows::active_self_policy(conn, &participant_ref, "activation")
            .map_err(db_err)?
            .map(|p| rows::str_of(&p, "policy_id").to_owned());
        let eligible = match &policy {
            None => None,
            Some(_) => rows::rows_where(
                conn,
                "wake_intents",
                "activity_stream_ref",
                &req_c.activity_stream_ref,
                "wake_intent_id",
            )
            .map_err(db_err)?
            .into_iter()
            .find(|w| {
                rows::str_of(w, "state") == "submitted"
                    && rows::str_of(w, "participant_ref") == participant_ref
                    && rows::u64_of(w, "generation") == req_c.generation
                    && rows::str_of(w, "exact_cause_ref") == req_c.source_event_ref
            })
            .map(|w| rows::str_of(&w, "wake_intent_id").to_owned()),
        };
        let effect = if eligible.is_some() {
            EFFECT_ELIGIBLE
        } else {
            EFFECT_NONE
        };
        let record = json!({
            "notice_id": notice_id,
            "source_protocol": req_c.source_protocol,
            "source_endpoint_ref": req_c.source_endpoint_ref,
            "source_event_ref": req_c.source_event_ref,
            "source_event_digest": digest_json(&req_c.source_event_digest),
            "activity_stream_ref": req_c.activity_stream_ref,
            "generation": req_c.generation,
            "participant_ref": participant_ref,
            "stable_notice_key": req_c.stable_notice_key,
            "eligibility_effect": effect,
            "eligible_wake_intent_ref": eligible,
            "activation_policy_ref": policy,
            "received_at": received_at,
        });
        let digest = conn_record_digest(
            conn,
            &society_c,
            &notice_id,
            "bpp-attention-notice-v0",
            &record,
        )?;
        let row = obj_pairs([
            ("notice_id", json!(notice_id)),
            ("society_id", json!(society_c)),
            ("source_protocol", json!(req_c.source_protocol)),
            ("source_endpoint_ref", json!(req_c.source_endpoint_ref)),
            ("source_event_ref", json!(req_c.source_event_ref)),
            (
                "source_event_digest",
                digest_json(&req_c.source_event_digest),
            ),
            ("activity_stream_ref", json!(req_c.activity_stream_ref)),
            ("generation", json!(req_c.generation)),
            ("participant_ref", json!(participant_ref)),
            ("stable_notice_key", json!(req_c.stable_notice_key)),
            ("eligibility_effect", json!(effect)),
            (
                "eligible_wake_intent_ref",
                eligible.as_ref().map(|w| json!(w)).unwrap_or(Value::Null),
            ),
            (
                "activation_policy_ref",
                policy.as_ref().map(|p| json!(p)).unwrap_or(Value::Null),
            ),
            ("received_at", json!(received_at)),
            ("digest", digest_json(&digest)),
        ]);
        Ok(Prepared {
            result: notice_result(&row, false),
            revision: Some(1),
            cursor: CursorMint::AfterEvents {
                society_id: society_c.clone(),
            },
            // The ONLY effect is the notice row itself.
            effects: vec![Effect::Upsert {
                table: "attention_notices".into(),
                row,
            }],
            events: vec![event(
                &society_c,
                &notice_event,
                "attention-notice.recorded",
                &notice_id,
                1,
                &participant_ref,
                ACTOR_ATTENTION,
                &req_c.meta,
                json!({"eligibility_effect": effect,
                       "created_wake_intent": false,
                       "created_activation_admission": false,
                       "created_resource_allocation": false,
                       "created_episode": false,
                       "rule": "arrival is not admission; admission is not attention; attention \
                                is not activation (§11.1/§16.4, family contract L25)"}),
            )],
        })
    })
}

fn notice_result(row: &Map<String, Value>, replayed: bool) -> Value {
    let mut out = json!({
        "notice_id": rows::str_of(row, "notice_id"),
        "activity_stream_ref": rows::str_of(row, "activity_stream_ref"),
        "generation": rows::u64_of(row, "generation"),
        "source_event_ref": rows::str_of(row, "source_event_ref"),
        "stable_notice_key": rows::str_of(row, "stable_notice_key"),
        "eligibility_effect": rows::str_of(row, "eligibility_effect"),
        "eligible_wake_intent_ref": opt_str(row, "eligible_wake_intent_ref"),
        "activation_policy_ref": opt_str(row, "activation_policy_ref"),
        // What a notification did NOT do, named in the reply itself.
        "created": {
            "wake_intent": false,
            "activation_admission": false,
            "resource_allocation": false,
            "episode": false,
        },
        "required_stages": [
            "wake_intent_submit (participant, R29)",
            "activation_admit (kernel)",
            "resource_allocate (kernel)",
            "PlacementBinding (kovee) -> placement_admit (R33)",
            "episode_request (participant, R29)",
            "episode_claim/start (runtime, R30)",
        ],
        "received_at": rows::str_of(row, "received_at"),
        "digest": rows::json_of(row, "digest"),
    });
    if replayed {
        out["replayed"] = json!(true);
    }
    out
}

/// Folds byom's OWN derivation of the §12.1 fragment into a staged
/// `ByomEpisodeBinding` record, beside the `context_source_digest` Kovee
/// echoed. The recorded value is byom's, so the projection read shows the
/// source relation byom actually owns.
pub fn with_recomputed_source_digest(
    row: &mut Map<String, Value>,
    fragment: &Value,
    digest: &DigestRef,
) {
    let mut record: Value =
        serde_json::from_str(rows::str_of(row, "record")).unwrap_or_else(|_| json!({}));
    if let Some(map) = record.as_object_mut() {
        map.insert(
            "provider_context_manifest_byom_fields".into(),
            fragment.clone(),
        );
        map.insert(
            "context_source_digest_recomputed".into(),
            digest_json(digest),
        );
    }
    row.insert("record".into(), json_text(&record));
}
