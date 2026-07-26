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
//! pinned            the kovee commit this binary was COMPILED against (build.rs
//!                   re-derives it from the linked path dependency and refuses
//!                   to compile against another one) — R3-I02
//! host-binding      the inert host-binding document byomd is configured with,
//!                   derived by KOVEE's own hostint from the binding+mapping
//!                   the greenfield saga committed (amendment A2)
//! seed-bindings     re-seed the shipped provider bindings from THIS process's
//!                   environment (so a placeholder key marks them active)
//! attention-notice  byom's attention_notice_record, SENT BY KOVEE's own client
//!                   for an event read out of koveed's OWN ledger
//! episode-activate  the four-stage activation: episode_request (byom stages
//!                   1-3) -> place (Kovee's PlacementBinding) -> placement_admit
//!                   -> episode_claim + episode_start
//! stage             the §16.2 DisclosureManifest, COMMITTED, so byom's
//!                   model_egress act can be prepared over its exact digest
//! complete          the whole broker chain: prepare -> execution_permit_consume
//!                   -> dispatch -> usage_report, over a chosen transport
//! effect-admit      byom's effect_outcome_admit on kovee's WORKER channel, with
//!                   the host effect/receipt digests derived by kovee's own
//!                   hostint over kovee's own rows (R35, source facts only)
//! episode-yield     episode::yield_episode — the Continuation hand-off a
//!                   successor attempt resumes from
//! episode-settle    episode::settle (usage_report on the meter channel)
//! episode-complete  episode::complete (terminalize, release the reservation)
//! effect-show       the effect/attempt/consumption/usage rows, for assertions
//! onboarding-consume / onboarding-claim / onboarding-complete
//!                   the §7.4 one-shot OnboardingCompute path, driven as the
//!                   HOSTED candidate's runtime
//! ```
//!
//! Nothing here invents authority: every record is written by kovee's own
//! code, every byom call rides a workload token byomd itself published, and
//! `--transport recording` stamps `recording-test-double` on the effect so a
//! stub run can never be mistaken for a provider call.
//!
//! Three byom runtime channel classes have no `Workload` arm in kovee
//! (`attention`, `broker`, `onboarding`), because kovee ships no subsystem
//! that owns them. For those, the driver reads the token byomd published and
//! sends through kovee's own `Endpoint::call_with_preamble`: the wire, the
//! framing and the reply handling are kovee's, and the evidence says plainly
//! that the DECISION to send is the scenario's.

use std::path::{Path, PathBuf};

use kovee_byom::bpp::{self, Endpoint, Surface};
use kovee_byom::credential::GATEWAY_ISSUER_REF;
use kovee_byom::episode::Fences;
use kovee_byom::hostint;
use kovee_byom::records::{KoveeRealmByomBinding, KoveeSocietyMapping};
use kovee_byom::runtime::{self as byom_runtime, Workload};
use kovee_core::family::DigestRef;
use kovee_core::problem::{Problem, ProblemKind};
use kovee_effects::{Egress, HttpsTransport, RecordingTransport};
use kovee_store::Store;
use koveed::episode::{self, Notice, Runtime, Seam};
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
        "pinned" => pinned(&args),
        "host-binding" => host_binding(&args),
        "seed-bindings" => seed_bindings(&args),
        "attention-notice" => attention_notice(&args),
        "episode-activate" => episode_activate(&args),
        "stage" => stage(&args),
        "complete" => complete(&args),
        "effect-admit" => effect_admit(&args),
        "episode-yield" => episode_yield(&args),
        "episode-settle" => episode_settle(&args),
        "episode-complete" => episode_complete(&args),
        "effect-show" => effect_show(&args),
        "onboarding-consume" => onboarding_consume(&args),
        "onboarding-claim" => onboarding_claim(&args),
        "onboarding-complete" => onboarding_complete(&args),
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

/// The revision this binary CONTAINS (R3-I02). `build.rs` derives all three
/// from the linked path dependency's own git tree and fails the build when
/// the harness names a different commit, so this is a report of a
/// machine-checked fact, not a claim.
fn pinned(_args: &Value) -> Result<Value, Problem> {
    Ok(json!({
        "kovee_commit": env!("I1_KOVEE_COMMIT"),
        "kovee_path": env!("I1_KOVEE_PATH"),
        "kovee_worktree_dirty": env!("I1_KOVEE_DIRTY") == "true",
    }))
}

/// The byom runtime surface for a channel class kovee has no `Workload` arm
/// for. The token is byomd's own published file — read verbatim, never
/// derived — and the call is kovee's own `Endpoint::call_with_preamble`.
fn runtime_call_with_token(
    args: &Value,
    token_file: &str,
    request: &Value,
) -> Result<Value, Problem> {
    let channels = PathBuf::from(text(args, "byom_channels_dir"));
    let path = channels.join(token_file);
    let line = std::fs::read_to_string(&path)
        .map(|t| t.trim().to_owned())
        .ok()
        .filter(|t| !t.is_empty())
        .ok_or_else(|| {
            Problem::new(
                ProblemKind::Forbidden,
                "byomd published no workload token for this subject",
            )
            .with_detail(path.display().to_string())
        })?;
    let endpoint = Endpoint::at(
        &maybe_text(args, "byom_endpoint_ref").unwrap_or("local".into()),
        &PathBuf::from(text(args, "byom_run_dir")),
    );
    let reply = endpoint
        .call_with_preamble(Surface::Runtime, Some(&line), request)
        .map_err(|e| bpp::passthrough(&e))?;
    Ok(reply.result)
}

/// The active seam's wire facts (byomd's endpoint incarnation and the
/// Society's recovery epoch) — kovee's own, read from kovee's own binding.
fn seam_of(args: &Value) -> Result<Seam, Problem> {
    let store = store_of(args);
    episode::seam_of_binding(store.conn(), &realm_of(args))
}

fn create_meta(seam: &Seam, what: &str, key: &str) -> Value {
    json!({
        "request_id": format!("kovee-{what}-{key}"),
        "idempotency_key": format!("kovee-{what}-{key}"),
        "expected_endpoint_incarnation": seam.endpoint_incarnation,
        "expected_recovery_epoch": seam.recovery_epoch,
    })
}

fn update_meta(seam: &Seam, what: &str, key: &str, expected_revision: u64) -> Value {
    let mut meta = create_meta(seam, what, key);
    if let Some(map) = meta.as_object_mut() {
        map.insert("expected_revision".to_owned(), json!(expected_revision));
    }
    meta
}

// -------------------------------------------------- the attention notice ----

/// byom's `attention_notice_record` for an event KOVEE committed.
///
/// Honest scope (R3-I01 f): kovee's `kovee-attention` crate is a two-line
/// stub, so no AttentionContract subsystem exists to DECIDE to notify, and
/// kovee has no `Workload::Attention` channel class either. What this
/// command does own is everything else: it verifies the event exists in
/// koveed's OWN ledger, derives the cross-boundary `source_event_digest`
/// with kovee's own hashing, and sends the notice over kovee's own byom
/// client. The trigger is still the scenario's, and the evidence says so.
fn attention_notice(args: &Value) -> Result<Value, Problem> {
    let store = store_of(args);
    let event_ref = text(args, "source_event_ref");
    let found: i64 = store
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM events WHERE event_id = ?1",
            [event_ref.as_str()],
            |r| r.get(0),
        )
        .map_err(store_fault)?;
    if found != 1 {
        return Err(Problem::new(
            ProblemKind::Forbidden,
            "kovee notifies byom only of an event it has itself committed",
        )
        .with_detail(format!("{event_ref} is not in koveed's ledger")));
    }
    let stream = text(args, "activity_stream_ref");
    let generation = args.get("generation").and_then(Value::as_u64).unwrap_or(1);
    let seam = seam_of(args)?;
    let key = text(args, "stable_notice_key");
    let request = json!({
        "version": "0.2",
        "op": "attention_notice_record",
        "meta": create_meta(&seam, "att", &key),
        "source_protocol": "kovee",
        "source_endpoint_ref": maybe_text(args, "source_endpoint_ref")
            .unwrap_or_else(|| "kovee-endpoint-local".to_owned()),
        "source_event_ref": event_ref,
        "source_event_digest": DigestRef::portable_public(
            kovee_core::family::sha256_hex(event_ref.as_bytes()),
        ),
        "activity_stream_ref": stream,
        "generation": generation,
        "stable_notice_key": key,
    });
    let result =
        runtime_call_with_token(args, &format!("runtime-attention-{stream}.token"), &request)?;
    Ok(json!({"notice": result, "sender": "kovee-byom client (kovee-attention is a stub)"}))
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
/// The activation notice. Every byom-owned reference here is one byom
/// DERIVES from a name the caller already supplied, so the driver can only
/// match it — and the BUDGET facts are not here at all any more.
///
/// `episode_request` publishes them as a frozen `portable_public` fragment
/// which `episode_activate` verifies below (R3-L02, disposition D-R3-3). The
/// three references this function used to fabricate (`rset-…`, `bridge-…`,
/// `sub-…`) and the two parent facts it took from its own caller's arguments
/// (`parent_account_ref`, `worst_case_amount`) were the last out-of-band
/// budget step: a wrong parent was undetectable on this side.
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
        resource_allocation_ref: allocation,
        // Replaced by byom's own published pin after `episode_request`.
        resource_allocation_digest: DigestRef::portable_public("0".repeat(64)),
        mandate_use_refs: vec![],
        // Replaced by byom's own published FRAGMENT after `episode_request`.
        parent_budget: Value::Null,
        context_manifest_ref: maybe_text(args, "context_manifest_ref")
            .unwrap_or_else(|| "kovee-ctxman-i1".to_owned()),
    }
}

/// Kovee's capacity-ledger counters for one realm — the ACCOUNT, not row
/// arithmetic (R3-U03).
fn capacity_of(store: &Store, realm: &str) -> Value {
    match koveed::budget::account(
        store.conn(),
        &koveed::budget::realm_account_ref(realm),
        "unit",
    ) {
        Ok(Some(a)) => json!({
            "account_ref": a.account_ref,
            "ceiling": a.ceiling,
            "remaining": a.remaining,
            "reserved": a.reserved,
            "committed": a.committed,
            "uncertain": a.uncertain,
            "delegated_to_children": a.delegated_to_children,
            "conserves": a.conserves(),
        }),
        _ => Value::Null,
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
            // The frozen parent-budget fragment, ECHOED from what byom's own
            // reply published (R3-L02). There is no arm that composes it.
            parent_budget: pin.get("parent_budget").filter(|v| !v.is_null()).cloned(),
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
    notice.parent_budget = requested
        .parent_budget
        .clone()
        .unwrap_or_else(|| fail("episode_request published no parent-budget fragment"));
    // CONSUMED AND VERIFIED here, before a single parent fact is used: both
    // portable digests must re-derive from exactly the published members.
    let parent = koveed::budget::verify_parent_fragment(
        &notice.parent_budget,
        &notice.society_ref,
        notice.recovery_epoch,
    )?;
    // The realm's capacity ceiling has to exist before a subordinate
    // reservation can be debited against it (R3-U03).
    koveed::budget::provision_realm_capacity(&mut store, &realm, 0)?;

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
            "parent_budget": notice.parent_budget,
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
            // Kovee's OWN ledger, which the scenario can now assert against
            // byom's (R3-U03).
            "kovee_capacity_account": capacity_of(&store, &realm),
            "verified_parent": {
                "byom_budget_reservation_set_ref": parent.byom_reservation_set_ref,
                "external_budget_bridge_ref": parent.external_budget_bridge_ref,
                "stable_external_reservation_key": parent.stable_external_reservation_key,
                "parent_worst_case_amount": parent.ceiling("unit"),
                "fragment_digest": parent.fragment_digest,
            },
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
    // The wire is SEALED (disposition D-R3-1): `Egress` is the only thing
    // `complete` accepts, the recording double exists only under
    // kovee-effects' `testing` feature, and either arm stamps its own profile
    // on the effect so a receipt cannot claim a provider call it never made.
    let transport: Egress<'_> = match &recording {
        Some(double) => Egress::recording(double),
        None => Egress::live(&https),
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
    // R3-I03: the send count is written to a DURABLE file before the outcome
    // is propagated, so a REFUSAL path cannot quietly drop the one number
    // that makes "not one byte left the process" checkable. The refusal
    // cells used to return a problem and lose it.
    if let Some(path) = maybe_text(args, "send_counter") {
        let record = json!({
            "sends": sends,
            "transport": kind,
            "refused": outcome.is_err(),
            "state": outcome.as_ref().ok().map(|c| c.state.as_str()),
        });
        std::fs::write(&path, format!("{record}\n"))
            .unwrap_or_else(|e| fail(&format!("write send counter {path}: {e}")));
    }
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

// ------------------------------------------- the effect-outcome admission ----

/// The `$domain` tags this driver derives kovee-owned cross-boundary digests
/// under. They are the driver's own (kovee ships no EffectOutcomeAdmission
/// sender), and the derivation is kovee's `hostint::portable_digest`, so both
/// values are unkeyed `portable_public` — the class byom's R35 shape requires
/// for a host-owned object (amendment A8).
const TAG_HOST_EFFECT: &str = "kovee-model-effect-source-v0";
const TAG_HOST_RECEIPT: &str = "kovee-effect-consumption-receipt-v0";

/// byom's `effect_outcome_admit` (R35) for one model effect, on KOVEE's own
/// worker channel: SOURCE FACTS ONLY — there is no decision member in this
/// shape at all, and byom's reconciliation seat is a separate governance op.
fn effect_admit(args: &Value) -> Result<Value, Problem> {
    let store = store_of(args);
    let key = text(args, "execution_key");
    let binding_key = text(args, "stable_binding_key");
    let bound = match episode::read_binding(store.conn(), &binding_key)? {
        Some(bound) => bound,
        None => fail(&format!("no episode binding for {binding_key}")),
    };
    let Some(effect) = model_broker::effect_by_execution_key(store.conn(), &key)? else {
        return Err(Problem::new(
            ProblemKind::NotFound,
            "kovee has no model effect for this execution key",
        ));
    };
    // The attempt kovee actually recorded, and the outcome it actually
    // reached: `ambiguous` is admitted as `ambiguous`, never as failed.
    let (attempt_ordinal, attempt_state, retry_frozen, transport_profile, observation) = store
        .conn()
        .query_row(
            "SELECT attempt_ordinal, state, retry_frozen, transport_profile,
                    COALESCE(observation, '')
             FROM model_effect_attempts WHERE effect_id = ?1
             ORDER BY attempt_ordinal DESC LIMIT 1",
            [effect.effect_id.as_str()],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)? != 0,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                ))
            },
        )
        .map_err(store_fault)?;
    let outcome = match attempt_state.as_str() {
        "completed" => "succeeded",
        "ambiguous" => "ambiguous",
        _ => "failed",
    };
    // kovee's own consumption record of byom's permit: the receipt side of
    // the admission. `owner_receipt_ref` is byom's own receipt id, echoed.
    let (consumption_id, owner_receipt_ref, consumption_state) = store
        .conn()
        .query_row(
            "SELECT consumption_id, COALESCE(owner_receipt_ref, ''), state
             FROM external_authorization_consumptions WHERE execution_key = ?1",
            [key.as_str()],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            },
        )
        .map_err(store_fault)?;
    // The event kovee committed for this outcome — the cursor byom's
    // admission cites as the source position it verified.
    let cursor: String = store
        .conn()
        .query_row(
            "SELECT event_id FROM events
             WHERE type LIKE 'dev.kovee.model-effect%' AND resource_ref = ?1
             ORDER BY stream_sequence DESC LIMIT 1",
            [effect.effect_id.as_str()],
            |r| r.get(0),
        )
        .map_err(store_fault)?;
    let host_effect_digest = hostint::portable_digest(
        TAG_HOST_EFFECT,
        &json!({
            "effect_id": effect.effect_id,
            "execution_key": key,
            "act_intent_ref": effect.act_intent_ref,
            "episode_ref": effect.episode_ref,
            "disclosure_manifest_ref": effect.disclosure_manifest_ref,
            "state": effect.state,
            "attempt_ordinal": attempt_ordinal,
            "attempt_state": attempt_state,
            "retry_frozen": retry_frozen,
            "transport_profile": transport_profile,
            "observation": observation,
        }),
    )
    .map_err(|e| {
        Problem::new(ProblemKind::Internal, "host effect digest").with_detail(e.to_string())
    })?;
    let host_receipt_digest = hostint::portable_digest(
        TAG_HOST_RECEIPT,
        &json!({
            "consumption_id": consumption_id,
            "execution_key": key,
            "owner_protocol": "byom",
            "owner_receipt_ref": owner_receipt_ref,
            "state": consumption_state,
        }),
    )
    .map_err(|e| {
        Problem::new(ProblemKind::Internal, "host receipt digest").with_detail(e.to_string())
    })?;
    let seam = seam_of(args)?;
    let token = byom_runtime::token(
        &PathBuf::from(text(args, "byom_channels_dir")),
        Workload::Worker,
        &bound.episode_ref,
    )
    .map_err(|e| {
        Problem::new(ProblemKind::Forbidden, "byom worker channel").with_detail(e.to_string())
    })?;
    let endpoint = Endpoint::at(
        &maybe_text(args, "byom_endpoint_ref").unwrap_or("local".into()),
        &PathBuf::from(text(args, "byom_run_dir")),
    );
    let request = json!({
        "version": "0.2",
        "op": "effect_outcome_admit",
        "meta": create_meta(&seam, "eoa", &key),
        "episode_ref": bound.episode_ref,
        "generation": bound.record.generation,
        "byom_attempt_ref": bound.byom_attempt_ref,
        "byom_fence_epoch": bound.fences.byom,
        "kovee_invocation_fence": bound.fences.kovee,
        // byom's OWN keyed values, echoed exactly as byom published them.
        "intent_ref": text(args, "act_intent_ref"),
        "intent_digest": digest(args, "act_intent_digest"),
        "stable_execution_key": key,
        "host_protocol": "kovee",
        "host_endpoint_ref": maybe_text(args, "host_endpoint_ref")
            .unwrap_or_else(|| "kovee-endpoint-local".to_owned()),
        "host_effect_ref": effect.effect_id,
        "host_effect_digest": host_effect_digest,
        "host_receipt_ref": consumption_id,
        "host_receipt_digest": host_receipt_digest,
        "host_cursor_or_signature_ref": cursor,
        "verification_status": "verified",
        "outcome": outcome,
    });
    let reply =
        byom_runtime::call(&endpoint, &token, &request).map_err(|e| bpp::passthrough(&e))?;
    Ok(json!({
        "admission": reply.result,
        "source": {
            "effect_id": effect.effect_id,
            "attempt_state": attempt_state,
            "retry_frozen": retry_frozen,
            "outcome": outcome,
            "host_effect_digest": host_effect_digest,
            "host_receipt_ref": consumption_id,
            "host_receipt_digest": host_receipt_digest,
            "host_cursor_or_signature_ref": cursor,
        },
    }))
}

// --------------------------------------------------- episode teardown ----

/// `episode_yield` with the Continuation hand-off: byom's Episode leaves
/// `running`, and kovee records WHICH continuation a successor must resume
/// from. The successor needs a new binding — kovee says so in the reply.
fn episode_yield(args: &Value) -> Result<Value, Problem> {
    let mut store = store_of(args);
    let runtime = runtime_of(args);
    let key = text(args, "stable_binding_key");
    let fences = bound_fences(&store, &key)?;
    let result = episode::yield_episode(
        &mut store,
        &runtime,
        &key,
        fences,
        &text(args, "continuation_ref"),
        0,
    )?;
    Ok(json!({"yielded": result}))
}

// ------------------------------------ the §7.4 one-shot onboarding path ----

/// byom's `onboarding_compute_permit_consume` (R32) on byom's BROKER runtime
/// channel — the hosted candidate's ONE compute use, ever.
///
/// Honest scope: kovee has no onboarding code at all (`grep -r onboarding
/// kovee/crates` is empty), so no kovee subsystem owns this call; and byom's
/// shape demands `local_erasure_safe` (byom-keyed) digests for three
/// KOVEE-owned objects (provider context manifest, disclosure manifest,
/// model profile), which no kovee derivation can produce — the same A8
/// direction R3-L01 fixed for `execution_permit_consume`, still open here.
/// Those three digests therefore arrive from the scenario and the evidence
/// records it; every value the RECEIPT is checked against is byom's own.
fn onboarding_consume(args: &Value) -> Result<Value, Problem> {
    let seam = seam_of(args)?;
    let intent = text(args, "compute_intent_ref");
    let key = text(args, "stable_compute_key");
    // The REQUEST identity is separate from the one-shot subject key: with
    // one idempotency key, byom's generic "changed request" guard answers
    // first and the §7.4 one-shot rule is never reached. A distinct
    // `meta_key` per attempt is what makes the one-shot refusal observable.
    let meta_key = maybe_text(args, "meta_key").unwrap_or_else(|| key.clone());
    let request = json!({
        "version": "0.2",
        "op": "onboarding_compute_permit_consume",
        "meta": update_meta(&seam, "occ", &meta_key, number(args, "expected_revision")),
        "compute_intent_ref": intent,
        "compute_intent_digest": digest(args, "compute_intent_digest"),
        "stable_compute_key": key,
        "onboarding_fence_epoch": number(args, "onboarding_fence_epoch"),
        "kovee_invocation_ref": text(args, "kovee_invocation_ref"),
        "provider_context_manifest_ref": text(args, "provider_context_manifest_ref"),
        "provider_context_manifest_digest": digest(args, "provider_context_manifest_digest"),
        "disclosure_manifest_ref": text(args, "disclosure_manifest_ref"),
        "disclosure_manifest_digest": digest(args, "disclosure_manifest_digest"),
        "model_profile_ref": text(args, "model_profile_ref"),
        "model_profile_digest": digest(args, "model_profile_digest"),
    });
    let result =
        runtime_call_with_token(args, &format!("runtime-broker-{intent}.token"), &request)?;
    Ok(json!({"receipt": result}))
}

/// byom's `onboarding_episode_claim` (R31) as the HOSTED candidate workload:
/// the holder runtime binding is kovee's own deployment, and the claim cites
/// the OnboardingComputeReceipt.
fn onboarding_claim(args: &Value) -> Result<Value, Problem> {
    let seam = seam_of(args)?;
    let onboarding = text(args, "onboarding_ref");
    let mut request = json!({
        "version": "0.2",
        "op": "onboarding_episode_claim",
        "meta": create_meta(&seam, "onbclm", &text(args, "stable_claim_key")),
        "onboarding_ref": onboarding,
        "candidate_participant_ref": text(args, "candidate_participant_ref"),
        "proposed_manifestation_ref": text(args, "proposed_manifestation_ref"),
        "proposed_manifestation_digest": digest(args, "proposed_manifestation_digest"),
        "onboarding_fence_epoch": number(args, "onboarding_fence_epoch"),
        "holder_runtime_binding": text(args, "holder_runtime_binding"),
        "stable_claim_key": text(args, "stable_claim_key"),
    });
    if let (Some(receipt), Some(map)) = (
        maybe_text(args, "compute_receipt_ref"),
        request.as_object_mut(),
    ) {
        map.insert("compute_receipt_ref".to_owned(), json!(receipt));
        map.insert(
            "compute_receipt_digest".to_owned(),
            serde_json::to_value(digest(args, "compute_receipt_digest")).unwrap_or(Value::Null),
        );
    }
    let result = runtime_call_with_token(
        args,
        &format!("runtime-onboarding-{onboarding}.token"),
        &request,
    )?;
    Ok(json!({"claim": result}))
}

/// byom's `onboarding_episode_complete`: EVIDENCE ONLY — the reply itself
/// says what did not happen (no acceptance, no Standing, no authority).
fn onboarding_complete(args: &Value) -> Result<Value, Problem> {
    let seam = seam_of(args)?;
    let onboarding = text(args, "onboarding_ref");
    let episode = text(args, "onboarding_episode_ref");
    let request = json!({
        "version": "0.2",
        "op": "onboarding_episode_complete",
        "meta": update_meta(&seam, "onbcmp", &episode, number(args, "expected_revision")),
        "onboarding_episode_ref": episode,
        "onboarding_ref": onboarding,
        "onboarding_fence_epoch": number(args, "onboarding_fence_epoch"),
        "outcome": maybe_text(args, "outcome").unwrap_or_else(|| "completed".to_owned()),
        "output_refs": args.get("output_refs").cloned().unwrap_or(json!([])),
        "evidence_refs": args.get("evidence_refs").cloned().unwrap_or(json!([])),
    });
    let result = runtime_call_with_token(
        args,
        &format!("runtime-onboarding-{onboarding}.token"),
        &request,
    )?;
    Ok(json!({"completion": result}))
}

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
