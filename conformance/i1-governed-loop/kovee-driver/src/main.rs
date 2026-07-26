//! The I1 gate's kovee-side caller: one subcommand per kovee library step
//! the governed loop needs and no socket operation exposes.
//!
//! ```text
//! echo '<json args>' | i1-kovee-driver <command>
//!   -> {"ok":true,"result":{...}}  |  {"ok":false,"problem":{...}}
//! ```
//!
//! Commands, in the order the loop uses them:
//!
//! ```text
//! host-binding      the inert host-binding document byomd is configured with,
//!                   derived by KOVEE's own hostint from the binding+mapping
//!                   the greenfield saga committed (amendment A2)
//! seed-bindings     re-seed the shipped provider bindings from THIS process's
//!                   environment (so a placeholder key marks them active)
//! episode-activate  the four-stage activation: episode_request (byom stages
//!                   1-3) -> place (Kovee's PlacementBinding) -> placement_admit
//!                   -> episode_claim + episode_start
//! stage             the §16.2 DisclosureManifest, COMMITTED, so byom's
//!                   model_egress act can be prepared over its exact digest
//! complete          the whole broker chain: prepare -> execution_permit_consume
//!                   -> dispatch -> usage_report, over a chosen transport
//! episode-settle    episode::settle (usage_report on the meter channel)
//! episode-complete  episode::complete (terminalize, release the reservation)
//! effect-show       the effect/attempt/consumption/usage rows, for assertions
//! ```
//!
//! Nothing here invents authority: every record is written by kovee's own
//! code, every byom call rides a workload token byomd itself published, and
//! `--transport recording` stamps `recording-test-double` on the effect so a
//! stub run can never be mistaken for a provider call.

use std::path::{Path, PathBuf};

use kovee_byom::bpp::Endpoint;
use kovee_byom::credential::GATEWAY_ISSUER_REF;
use kovee_byom::episode::Fences;
use kovee_byom::hostint;
use kovee_byom::records::{KoveeRealmByomBinding, KoveeSocietyMapping};
use kovee_core::family::DigestRef;
use kovee_core::problem::Problem;
use kovee_effects::{HttpsTransport, RecordingTransport, Transport};
use kovee_store::Store;
use koveed::episode::{self, Notice, ParentItem, Runtime};
use koveed::model_broker::{self, ActAuthorization, CompleteRequest, Fault};
use serde_json::{json, Value};

const REALM: &str = "realm-personal";

fn main() {
    let command = std::env::args().nth(1).unwrap_or_default();
    let mut raw = String::new();
    if std::io::Read::read_to_string(&mut std::io::stdin(), &mut raw).is_err() {
        fail("stdin is not readable");
    }
    let args: Value = if raw.trim().is_empty() {
        json!({})
    } else {
        match serde_json::from_str(&raw) {
            Ok(v) => v,
            Err(e) => fail(&format!("arguments are not JSON: {e}")),
        }
    };
    let outcome = match command.as_str() {
        "host-binding" => host_binding(&args),
        "seed-bindings" => seed_bindings(&args),
        "episode-activate" => episode_activate(&args),
        "stage" => stage(&args),
        "complete" => complete(&args),
        "episode-settle" => episode_settle(&args),
        "episode-complete" => episode_complete(&args),
        "effect-show" => effect_show(&args),
        other => fail(&format!("unknown command {other:?}")),
    };
    match outcome {
        Ok(result) => print(&json!({"ok": true, "result": result})),
        Err(problem) => {
            print(&json!({"ok": false,
                          "problem": serde_json::to_value(&problem).unwrap_or(Value::Null)}));
            std::process::exit(3);
        }
    }
}

fn print(value: &Value) {
    println!("{value}");
}

fn fail(message: &str) -> ! {
    print(&json!({"ok": false, "error": message}));
    std::process::exit(2);
}

// ------------------------------------------------------------- plumbing ----

fn text(args: &Value, key: &str) -> String {
    args.get(key)
        .and_then(Value::as_str)
        .unwrap_or_else(|| fail(&format!("missing string argument {key:?}")))
        .to_owned()
}

fn maybe_text(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .filter(|s| !s.is_empty())
}

fn number(args: &Value, key: &str) -> u64 {
    args.get(key)
        .and_then(Value::as_u64)
        .unwrap_or_else(|| fail(&format!("missing integer argument {key:?}")))
}

fn digest(args: &Value, key: &str) -> DigestRef {
    serde_json::from_value(args.get(key).cloned().unwrap_or(Value::Null))
        .unwrap_or_else(|e| fail(&format!("argument {key:?} is not a DigestRef: {e}")))
}

/// The store the running koveed serves — the driver writes kovee's records
/// into kovee's own database, never a copy.
fn store_of(args: &Value) -> Store {
    let path = PathBuf::from(text(args, "store"));
    Store::open(&path).unwrap_or_else(|e| fail(&format!("open {}: {e}", path.display())))
}

/// byomd's runtime (socket) and channels (published workload token)
/// directories: the daemon's own configuration, exactly as kovee reads it.
fn runtime_of(args: &Value) -> Runtime {
    let run_dir = PathBuf::from(text(args, "byom_run_dir"));
    let channels = PathBuf::from(text(args, "byom_channels_dir"));
    let endpoint = Endpoint::at(
        &maybe_text(args, "byom_endpoint_ref").unwrap_or("local".into()),
        &run_dir,
    );
    Runtime::new(&endpoint, &channels)
}

fn realm_of(args: &Value) -> String {
    maybe_text(args, "realm").unwrap_or_else(|| REALM.to_owned())
}

// -------------------------------------------------------- host binding ----

/// The wire projection of the binding and mapping the greenfield saga
/// committed — derived by kovee's own `hostint`, so byomd recomputes the
/// same cross-boundary digests. Amendment A2: Kovee may configure and bind
/// byomd, never author Society state.
fn host_binding(args: &Value) -> Result<Value, Problem> {
    let binding: KoveeRealmByomBinding =
        serde_json::from_value(args.get("binding").cloned().unwrap_or(Value::Null))
            .unwrap_or_else(|e| fail(&format!("binding: {e}")));
    let mapping: KoveeSocietyMapping =
        serde_json::from_value(args.get("mapping").cloned().unwrap_or(Value::Null))
            .unwrap_or_else(|e| fail(&format!("mapping: {e}")));
    let endpoint_root_id = text(args, "endpoint_root_id");
    let document = hostint::host_binding_document(
        &binding,
        &mapping,
        &[GATEWAY_ISSUER_REF.to_owned()],
        &endpoint_root_id,
    )
    .unwrap_or_else(|e| fail(&format!("host binding document: {e}")));
    let dir = Path::new(&text(args, "byom_data_dir")).join("kovee");
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| fail(&format!("create {dir:?}: {e}")));
    let path = dir.join("host-binding.json");
    std::fs::write(&path, document.to_string())
        .unwrap_or_else(|e| fail(&format!("write {path:?}: {e}")));
    Ok(json!({
        "path": path.to_string_lossy(),
        "binding_ref": document["realm_byom_binding"]["binding_ref"],
        "delegated_principal_issuers": document["delegated_principal_issuers"],
        "document": document,
    }))
}

fn seed_bindings(args: &Value) -> Result<Value, Problem> {
    let mut store = store_of(args);
    let realm = realm_of(args);
    model_broker::seed_default_bindings(&mut store, &realm, 0)?;
    let mut rows = Vec::new();
    {
        let conn = store.conn();
        let mut stmt = conn
            .prepare(
                "SELECT model_provider_binding_id, provider_kind, status, endpoint_host,
                        credential_secret_ref
                 FROM model_provider_bindings WHERE realm_ref = ?1
                 ORDER BY model_provider_binding_id",
            )
            .map_err(store_fault)?;
        let mapped = stmt
            .query_map([realm.as_str()], |r| {
                Ok(json!({
                    "binding_ref": r.get::<_, String>(0)?,
                    "provider_kind": r.get::<_, String>(1)?,
                    "status": r.get::<_, String>(2)?,
                    "endpoint_host": r.get::<_, String>(3)?,
                    // The REFERENCE, never a secret: `env:NAME`.
                    "credential_secret_ref": r.get::<_, String>(4)?,
                }))
            })
            .map_err(store_fault)?;
        for row in mapped {
            rows.push(row.map_err(store_fault)?);
        }
    }
    Ok(json!({"bindings": rows}))
}

fn store_fault(e: rusqlite::Error) -> Problem {
    Problem::new(kovee_core::problem::ProblemKind::Internal, "store read")
        .with_detail(e.to_string())
}

// ----------------------------------------------------- the activation ----

/// The byom-owned notice: every reference is byom's, derived the way byom
/// derives it, so Kovee can only match.
fn notice_of(args: &Value) -> Notice {
    let wake = text(args, "wake_intent_ref");
    let allocation = format!("alloc-{wake}-r1");
    Notice {
        society_ref: text(args, "society_ref"),
        recovery_epoch: args
            .get("recovery_epoch")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        participant_ref: text(args, "participant_ref"),
        participant_binding_epoch: number(args, "participant_binding_epoch"),
        manifestation_ref: text(args, "manifestation_ref"),
        activity_stream_ref: text(args, "activity_stream_ref"),
        generation: args.get("generation").and_then(Value::as_u64).unwrap_or(1),
        activation_admission_ref: format!("adm-{wake}-r1"),
        wake_intent_ref: wake,
        resource_allocation_ref: allocation.clone(),
        // Replaced by byom's own published pin after `episode_request`.
        resource_allocation_digest: DigestRef::portable_public("0".repeat(64)),
        mandate_use_refs: vec![],
        byom_budget_reservation_ref: format!("rset-{allocation}"),
        byom_reservation_set_revision: 1,
        external_budget_bridge_ref: format!("bridge-{allocation}"),
        stable_external_reservation_key: format!("sub-{allocation}"),
        parent_reservation_items: vec![ParentItem {
            account_ref: text(args, "parent_account_ref"),
            account_revision: 1,
            dimension: "unit".to_owned(),
            unit: "unit".to_owned(),
            worst_case_amount: args
                .get("worst_case_amount")
                .and_then(Value::as_u64)
                .unwrap_or(256),
        }],
        context_manifest_ref: maybe_text(args, "context_manifest_ref")
            .unwrap_or_else(|| "kovee-ctxman-i1".to_owned()),
    }
}

fn episode_activate(args: &Value) -> Result<Value, Problem> {
    let mut store = store_of(args);
    let runtime = runtime_of(args);
    let realm = realm_of(args);
    let mut notice = notice_of(args);

    // Stages 1-3 are byom's, and in the I1 loop the PARTICIPANT itself
    // drives `episode_request` over its own channel (plan §0.1) — so this
    // driver is normally handed what byom's reply published. When no
    // `requested` block is supplied it calls `episode_request` itself, on
    // the participant channel byomd published (kovee's own K2 path), naming
    // the stage ids byom DERIVED so it can only match them.
    let requested = match args.get("requested") {
        Some(pin) => episode::Requested {
            episode_ref: text(pin, "episode_ref"),
            generation: pin.get("generation").and_then(Value::as_u64).unwrap_or(1),
            state: maybe_text(pin, "state").unwrap_or_else(|| "eligible".to_owned()),
            resource_allocation_ref: maybe_text(pin, "resource_allocation_ref"),
            resource_allocation_digest: Some(digest(pin, "resource_allocation_digest")),
        },
        None => {
            let channel = runtime.participant_channel(&notice.participant_ref)?;
            episode::request(&mut store, &runtime, &channel, &notice, 0)?
        }
    };
    notice.resource_allocation_ref = requested
        .resource_allocation_ref
        .clone()
        .unwrap_or_else(|| fail("episode_request published no allocation id"));
    notice.resource_allocation_digest = requested
        .resource_allocation_digest
        .clone()
        .unwrap_or_else(|| fail("episode_request published no allocation digest"));

    // Stage 4: Kovee's own PlacementBinding, then byom's narrow adapter.
    let placed = episode::place(
        &mut store,
        &realm,
        &notice,
        &maybe_text(args, "kovee_invocation_ref").unwrap_or_else(|| "kovee-inv-i1".to_owned()),
        0,
    )?;
    let admitted = episode::admit(&mut store, &runtime, &placed.placement_id, &notice, 0)?;
    let bound = episode::start(
        &mut store,
        &runtime,
        &placed.placement_id,
        &notice,
        &requested.episode_ref,
        args.get("lease_ttl_seconds")
            .and_then(Value::as_u64)
            .unwrap_or(600),
        0,
    )?;
    Ok(json!({
        "requested": {
            "episode_ref": requested.episode_ref,
            "state": requested.state,
            "resource_allocation_ref": notice.resource_allocation_ref,
            "resource_allocation_digest": notice.resource_allocation_digest,
        },
        "placement": {
            "placement_id": placed.placement_id,
            "kovee_fence_epoch": placed.kovee_fence_epoch,
        },
        "admitted": {
            "admission_ref": admitted.admission_ref,
            "bridge_state": admitted.bridge_state,
            "episode_queued": admitted.episode_queued,
            "subordinate_reservation_ref": admitted.subordinate_reservation_ref,
        },
        "bound": {
            "stable_binding_key": bound.stable_binding_key,
            "episode_ref": bound.episode_ref,
            "byom_attempt_ref": bound.byom_attempt_ref,
            "byom_fence_epoch": bound.fences.byom,
            "kovee_invocation_fence": bound.fences.kovee,
            "lease_revision": bound.lease_revision,
        },
    }))
}

// ------------------------------------------------------ the model call ----

/// One owned worker request — everything the broker needs and nothing it
/// does not: no provider, host, header or credential is expressible here.
struct Call {
    realm: String,
    project: Option<String>,
    attempt_id: String,
    fence_epoch: u64,
    model_profile_ref: String,
    purpose_ref: String,
    classification_ref: String,
    system: Option<String>,
    prompt: String,
    max_output_tokens: u64,
    stable_binding_key: Option<String>,
}

impl Call {
    fn of(args: &Value) -> Call {
        Call {
            realm: realm_of(args),
            project: maybe_text(args, "project"),
            attempt_id: text(args, "attempt_id"),
            fence_epoch: number(args, "fence_epoch"),
            model_profile_ref: text(args, "model_profile_ref"),
            purpose_ref: text(args, "purpose_ref"),
            classification_ref: text(args, "classification_ref"),
            system: maybe_text(args, "system"),
            prompt: text(args, "prompt"),
            max_output_tokens: args
                .get("max_output_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(64),
            stable_binding_key: maybe_text(args, "stable_binding_key"),
        }
    }

    fn request(&self) -> CompleteRequest<'_> {
        CompleteRequest {
            realm: &self.realm,
            project: self.project.as_deref(),
            attempt_id: &self.attempt_id,
            fence_epoch: self.fence_epoch,
            model_profile_ref: &self.model_profile_ref,
            purpose_ref: &self.purpose_ref,
            classification_ref: &self.classification_ref,
            system: self.system.as_deref(),
            prompt: &self.prompt,
            max_output_tokens: self.max_output_tokens,
            stable_binding_key: self.stable_binding_key.as_deref(),
        }
    }
}

fn stage(args: &Value) -> Result<Value, Problem> {
    let mut store = store_of(args);
    let call = Call::of(args);
    let (profile, binding) =
        model_broker::read_profile(store.conn(), &call.realm, &call.model_profile_ref)?;
    let staged = model_broker::stage(&mut store, &call.request(), &profile, &binding, 0)?;
    Ok(json!({
        "disclosure_manifest_ref": staged.disclosure_manifest_ref(),
        "disclosure_manifest_digest": staged.disclosure_digest(),
        "provider_claims": staged.disclosure.provider_claims,
        "recipient_binding": staged.disclosure.recipient_binding,
        "purpose": staged.disclosure.purpose,
        "total_bytes": staged.disclosure.total_bytes,
        "exact_items": staged.disclosure.exact_items,
        "provider_binding": {
            "binding_ref": binding.model_provider_binding_id,
            "status": binding.status.as_str(),
            "provider_kind": binding.provider_kind.as_str(),
            "endpoint": binding.endpoint,
        },
        "model_selector": profile.model_selector,
    }))
}

fn authorization_of(args: &Value) -> ActAuthorization {
    let a = args
        .get("authorization")
        .unwrap_or_else(|| fail("missing authorization"));
    ActAuthorization {
        act_intent_ref: text(a, "act_intent_ref"),
        act_intent_digest: digest(a, "act_intent_digest"),
        act_revision: number(a, "act_revision"),
        subject_digest: digest(a, "subject_digest"),
        stable_execution_key: text(a, "stable_execution_key"),
        budget_reservation_set_ref: text(a, "budget_reservation_set_ref"),
    }
}

fn complete(args: &Value) -> Result<Value, Problem> {
    let mut store = store_of(args);
    let runtime = runtime_of(args);
    let call = Call::of(args);
    let authorization = authorization_of(args);
    let fault = match maybe_text(args, "fault").unwrap_or_default().as_str() {
        "" | "none" => Fault::None,
        "after_prepare" => Fault::AbortAfterPrepare,
        "after_dispatch_record" => Fault::AbortAfterDispatchRecord,
        other => fail(&format!("unknown fault {other:?}")),
    };
    // The transport is the ONE thing a stub run substitutes, and it is
    // recorded on the effect either way.
    let kind = maybe_text(args, "transport").unwrap_or_else(|| "recording".to_owned());
    let recording = match kind.as_str() {
        "recording" => Some(RecordingTransport::answering(
            maybe_text(args, "reply_body")
                .unwrap_or_else(|| fail("recording transport needs reply_body"))
                .as_bytes(),
        )),
        "recording_uncertain" => Some(RecordingTransport::uncertain(
            &maybe_text(args, "uncertain_reason").unwrap_or_else(|| "connection reset".to_owned()),
        )),
        "https" => None,
        other => fail(&format!("unknown transport {other:?}")),
    };
    let https = HttpsTransport::new();
    let transport: &dyn Transport = match &recording {
        Some(double) => double,
        None => &https,
    };
    let outcome = model_broker::complete(
        &mut store,
        &runtime,
        transport,
        &call.request(),
        &authorization,
        0,
        fault,
    );
    let sends = recording.as_ref().map(|r| r.send_count());
    let completion = outcome?;
    Ok(json!({
        "effect_id": completion.effect_id,
        "effect_attempt_id": completion.effect_attempt_id,
        "state": completion.state.as_str(),
        "text": completion.text,
        "usage": {
            "input_tokens": completion.usage.input_tokens,
            "output_tokens": completion.usage.output_tokens,
        },
        "model": completion.model,
        "stop_reason": completion.stop_reason,
        "provider_ref": completion.external_ref,
        "disclosure_manifest_ref": completion.disclosure_manifest_ref,
        "provider_context_manifest_ref": completion.provider_context_manifest_ref,
        "transport_profile": completion.transport_profile,
        "retry_frozen": completion.retry_frozen,
        "usage_reported": completion.usage_reported,
        "observation": completion.observation,
        // The stub's own count: "not one byte left" is machine-checkable.
        "transport_send_count": sends,
    }))
}

// --------------------------------------------------- episode teardown ----

fn bound_fences(store: &Store, key: &str) -> Result<Fences, Problem> {
    let bound = episode::read_binding(store.conn(), key)?
        .unwrap_or_else(|| fail(&format!("no episode binding for {key}")));
    Ok(bound.fences)
}

fn episode_settle(args: &Value) -> Result<Value, Problem> {
    let mut store = store_of(args);
    let runtime = runtime_of(args);
    let key = text(args, "stable_binding_key");
    let fences = bound_fences(&store, &key)?;
    let result = episode::settle(
        &mut store,
        &runtime,
        &key,
        fences,
        number(args, "charge"),
        0,
    )?;
    Ok(json!({"usage_report": result}))
}

fn episode_complete(args: &Value) -> Result<Value, Problem> {
    let mut store = store_of(args);
    let runtime = runtime_of(args);
    let key = text(args, "stable_binding_key");
    let fences = bound_fences(&store, &key)?;
    let result = episode::complete(&mut store, &runtime, &key, fences, 0)?;
    Ok(json!({"episode_complete": result}))
}

// -------------------------------------------------------------- reads ----

/// The broker's own rows for one execution key — read through kovee's own
/// accessors so the scenario asserts against kovee's records, not a
/// re-derivation of them.
fn effect_show(args: &Value) -> Result<Value, Problem> {
    let store = store_of(args);
    let key = text(args, "execution_key");
    let Some(row) = model_broker::effect_by_execution_key(store.conn(), &key)? else {
        return Ok(json!({"effect": Value::Null}));
    };
    let attempts = {
        let conn = store.conn();
        let mut stmt = conn
            .prepare(
                "SELECT effect_attempt_id, attempt_ordinal, state, retry_frozen,
                        transport_profile, input_tokens, output_tokens, observation
                 FROM model_effect_attempts WHERE effect_id = ?1 ORDER BY attempt_ordinal",
            )
            .map_err(store_fault)?;
        let mapped = stmt
            .query_map([row.effect_id.as_str()], |r| {
                Ok(json!({
                    "effect_attempt_id": r.get::<_, String>(0)?,
                    "attempt_ordinal": r.get::<_, i64>(1)?,
                    "state": r.get::<_, String>(2)?,
                    "retry_frozen": r.get::<_, i64>(3)? != 0,
                    "transport_profile": r.get::<_, String>(4)?,
                    "input_tokens": r.get::<_, Option<i64>>(5)?,
                    "output_tokens": r.get::<_, Option<i64>>(6)?,
                    "observation": r.get::<_, Option<String>>(7)?,
                }))
            })
            .map_err(store_fault)?;
        let mut out = Vec::new();
        for a in mapped {
            out.push(a.map_err(store_fault)?);
        }
        out
    };
    let consumptions = {
        let conn = store.conn();
        let mut stmt = conn
            .prepare(
                "SELECT consumption_id, owner_protocol, phase, state, owner_receipt_ref,
                        mandate_use_ref
                 FROM external_authorization_consumptions WHERE execution_key = ?1",
            )
            .map_err(store_fault)?;
        let mapped = stmt
            .query_map([key.as_str()], |r| {
                Ok(json!({
                    "consumption_id": r.get::<_, String>(0)?,
                    "owner_protocol": r.get::<_, String>(1)?,
                    "phase": r.get::<_, String>(2)?,
                    "state": r.get::<_, String>(3)?,
                    "owner_receipt_ref": r.get::<_, Option<String>>(4)?,
                    "mandate_use_ref": r.get::<_, Option<String>>(5)?,
                }))
            })
            .map_err(store_fault)?;
        let mut out = Vec::new();
        for c in mapped {
            out.push(c.map_err(store_fault)?);
        }
        out
    };
    let usage = {
        let conn = store.conn();
        let mut stmt = conn
            .prepare(
                "SELECT stable_report_key, episode_ref, input_tokens, output_tokens,
                        settled_by_byom, result
                 FROM model_usage_reports WHERE effect_attempt_id IN
                     (SELECT effect_attempt_id FROM model_effect_attempts WHERE effect_id = ?1)",
            )
            .map_err(store_fault)?;
        let mapped = stmt
            .query_map([row.effect_id.as_str()], |r| {
                Ok(json!({
                    "stable_report_key": r.get::<_, String>(0)?,
                    "episode_ref": r.get::<_, String>(1)?,
                    "input_tokens": r.get::<_, i64>(2)?,
                    "output_tokens": r.get::<_, i64>(3)?,
                    "settled_by_byom": r.get::<_, i64>(4)? != 0,
                    "result": serde_json::from_str::<Value>(&r.get::<_, String>(5)?)
                        .unwrap_or(Value::Null),
                }))
            })
            .map_err(store_fault)?;
        let mut out = Vec::new();
        for u in mapped {
            out.push(u.map_err(store_fault)?);
        }
        out
    };
    let disclosure = model_broker::read_disclosure(store.conn(), &row.disclosure_manifest_ref)?
        .map(|d| {
            json!({
                "disclosure_id": d.disclosure_id,
                "recipient_binding": d.recipient_binding,
                "purpose": d.purpose,
                "provider_claims": d.provider_claims,
                "total_bytes": d.total_bytes,
                "exact_items": d.exact_items.len(),
            })
        })
        .unwrap_or(Value::Null);
    Ok(json!({
        "effect": {
            "effect_id": row.effect_id,
            "state": row.state,
            "execution_key": row.execution_key,
            "act_intent_ref": row.act_intent_ref,
            "episode_ref": row.episode_ref,
            "disclosure_manifest_ref": row.disclosure_manifest_ref,
        },
        "attempts": attempts,
        "consumptions": consumptions,
        "usage_reports": usage,
        "disclosure": disclosure,
    }))
}
