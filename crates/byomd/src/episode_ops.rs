//! B3 slice 2 — the episode half of the Kovee-native society:
//! DESIGN.md §11.1 (the FOUR-STAGE activation), §11.2 (Episode /
//! EpisodeAttempt / EpisodeLeaseHead with CLOCKED expiry), §11.4
//! (budgets, the `byom_subordinate` bridge saga, measured settlement),
//! and the C2 `ByomEpisodeBinding` record (family contract L19–L22,
//! L31–L33).
//!
//! Three things the code makes structural rather than trusted:
//!
//! 1. **No stage is skippable.** Activation has four records with four
//!    owners. `activation_admit` can only evaluate a COMMITTED
//!    WakeIntent; `resource_allocate` can only reserve an ADMITTED one;
//!    the Episode only queues behind BOTH reservation sets; a claim only
//!    reaches a QUEUED Episode. Each stage's id is DERIVED from the
//!    subject it decides, so the request can only match the server value
//!    and a caller cannot invent a stage it skipped. Arrival, Kovee
//!    attention, a host cron, or a model score reach none of them: the
//!    only author of a WakeIntent is the participant channel (registry).
//! 2. **The lease is clocked, not liveness-guessed.** `expires_at_unix`
//!    is minted at claim as `now + lease_ttl_seconds`; the head becomes
//!    `lease_expired` only when the authoritative server clock has
//!    STRICTLY passed it, and only an expired (or voluntarily yielded)
//!    head is re-claimable. A crashed or silent worker changes nothing —
//!    there is no liveness probe anywhere in this file
//!    (proof/specs/EpisodeLease.tla `NoPrematureExpiry`,
//!    `ReclaimNeedsExpiryOrYield`).
//! 3. **Both fences, always.** Every protected runtime command presents
//!    the Byom lease fence AND the Kovee invocation fence, and both are
//!    compared against the committed `ByomEpisodeBinding`. A mutation
//!    carrying one current fence and one stale fence is refused (family
//!    contract L21).

use bpp_core::digest::DigestRef;
use bpp_core::ops;
use bpp_core::problem::{Problem, ProblemKind};
use bpp_core::time::{parse_rfc3339_utc, rfc3339_utc};
use byom_store::effects::Effect;
use byom_store::rows;
use byom_store::{CrashHooks, CursorMint, MutationScope, Prepared, Store};
use rusqlite::Connection;
use serde_json::{json, Map, Value};

use crate::gov_ops::{check_meta_binding, db_err, digest_json, mint, obj_pairs, run};
use crate::part_common::{self, Caller};
use crate::part_ops::event;
use crate::state;

// ------------------------------------------------------- constants ----

/// The kernel policy version every ActivationAdmission records (§11.1).
pub const KERNEL_POLICY_VERSION: &str = "byom-kernel-activation-v0";
/// The kernel's actor string for the two named internal transitions.
/// They are NOT callable BPP operations: nothing on any surface reaches
/// them, and the participant request that drives them cannot choose
/// their outcome.
pub const ACTOR_KERNEL: &str = "kernel:activation";
/// The narrow Kovee adapters' actor strings (§14.7 runtime row).
pub const ACTOR_PLACEMENT: &str = "kovee-adapter:placement";
pub const ACTOR_METER: &str = "kovee-adapter:meter";

/// The per-Episode worst-case reservation on the mandate's §11.4
/// ceiling set (`budget_ceiling_set_ref`, dimension `unit`). DESIGN.md
/// fixes no per-Episode worst case; this bundle pins one (recorded
/// deviation) so the conservation ledger has real quantities to move.
pub const EPISODE_WORST_CASE_UNITS: u64 = 256;
/// The mandate's episode allowance: the quantity `mandate_issue`
/// reserved on the ceiling set. Open Episode reservations never exceed
/// it — the budget-exhausted refusal.
pub const MANDATE_EPISODE_ALLOWANCE: u64 = part_common::MANDATE_CEILING;

/// Episode states that still hold their reservation / count against the
/// mandate's rate ceiling.
const LIVE_EPISODE_STATES: [&str; 7] = [
    "prepared",
    "eligible",
    "queued",
    "running",
    "yielded",
    "waiting",
    "ambiguous",
];

// ----------------------------------------------------- problem pins ----

/// A stage of the §11.1 activation pipeline is missing or not in the
/// state the next stage requires.
pub fn stage_required(detail: &str) -> Problem {
    Problem::new(
        ProblemKind::AdmissionRequired,
        "the previous activation stage is not committed",
    )
    .with_status(409)
    .with_detail(detail.to_owned())
}

/// A stale Byom or Kovee fence, or a lease another attempt holds.
pub fn stale_lease(detail: &str) -> Problem {
    Problem::new(ProblemKind::StaleLease, "the episode lease is not current")
        .with_status(409)
        .with_detail(detail.to_owned())
}

fn ambiguous(detail: &str) -> Problem {
    Problem::new(
        ProblemKind::EffectAmbiguous,
        "the outcome is ambiguous and is never blindly repeated",
    )
    .with_status(409)
    .with_detail(detail.to_owned())
}

// ------------------------------------------- runtime workload tokens ----

/// The three narrow runtime channels of this slice. Each token is
/// byomd-MINTED (never caller-chosen) and derived from the store root
/// over the exact subject, published `0600` beside the candidate and
/// participant channel files. mTLS / attested workload identity is
/// honestly NOT claimed at the developer profile (§11.5); what IS
/// structural is the separation: the worker channel cannot present a
/// meter settlement, and the meter channel cannot claim a lease.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeChannel {
    /// The workload identity bound to one exact Episode/generation
    /// (R30: claim/start/checkpoint/yield/complete/fail, worker usage
    /// evidence, effect outcome admission).
    Worker,
    /// The narrow TRUSTED METER adapter: the only channel whose
    /// `usage_report` may settle (§11.4, family contract L33).
    Meter,
    /// The narrow Kovee placement adapter bound to one exact
    /// ResourceAllocation (R33).
    Placement,
}

impl RuntimeChannel {
    fn tag(self) -> &'static str {
        match self {
            RuntimeChannel::Worker => "worker",
            RuntimeChannel::Meter => "meter",
            RuntimeChannel::Placement => "placement",
        }
    }

    fn prefix(self) -> &'static str {
        match self {
            RuntimeChannel::Worker => "rwk1.",
            RuntimeChannel::Meter => "rmt1.",
            RuntimeChannel::Placement => "rpl1.",
        }
    }
}

/// The token for one exact subject (episode+generation, or allocation).
pub fn runtime_token(store: &Store, channel: RuntimeChannel, subject: &str) -> Option<String> {
    let key = store.scope_key("runtime-workload-channel").ok()?;
    let bound = format!("{}|{}", channel.tag(), subject);
    Some(format!(
        "{}{}",
        channel.prefix(),
        bpp_core::canonical::hex(&bpp_core::canonical::hmac_sha256(&key, bound.as_bytes()))
    ))
}

fn episode_subject(episode_ref: &str, generation: u64) -> String {
    format!("{episode_ref}|{generation}")
}

/// Verifies a presented runtime preamble against the exact subject. The
/// comparison is over byomd's own derivation, so a token for another
/// Episode, another generation, or another channel class never matches.
pub fn verify_runtime_token(
    store: &Store,
    presented: &str,
    channel: RuntimeChannel,
    subject: &str,
) -> Result<(), Problem> {
    let expected = runtime_token(store, channel, subject)
        .ok_or_else(|| state::internal("runtime channel key unavailable"))?;
    let a = expected.as_bytes();
    let b = presented.trim().as_bytes();
    let same = a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0;
    if same {
        Ok(())
    } else {
        Err(state::forbidden_detail(
            "the runtime workload token does not bind this exact subject and channel",
        ))
    }
}

/// Reconciles the runtime token files with committed state: a live
/// Episode has its worker and meter token files, a reserved allocation
/// has its placement token file, and terminal subjects have none.
/// Idempotent and crash-safe — rerun after every runtime mutation and at
/// startup (the `ensure_channel_files` discipline).
pub fn ensure_runtime_token_files(store: &Store) {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = crate::gov_ops::channels_dir(store);
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    let mut want: Vec<(String, String)> = Vec::new();
    if let Ok(mut stmt) = store
        .conn()
        .prepare("SELECT episode_id, generation, state FROM episodes")
    {
        if let Ok(list) = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
            ))
        }) {
            for (episode_id, generation, ep_state) in list.flatten() {
                let live = LIVE_EPISODE_STATES.contains(&ep_state.as_str());
                let subject = episode_subject(&episode_id, generation.max(0) as u64);
                for channel in [RuntimeChannel::Worker, RuntimeChannel::Meter] {
                    let name = format!("runtime-{}-{}.token", channel.tag(), episode_id);
                    if live {
                        if let Some(token) = runtime_token(store, channel, &subject) {
                            want.push((name, token));
                        }
                    } else {
                        let _ = std::fs::remove_file(dir.join(&name));
                    }
                }
            }
        }
    }
    if let Ok(mut stmt) = store
        .conn()
        .prepare("SELECT allocation_id, state FROM resource_allocations")
    {
        if let Ok(list) =
            stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        {
            for (allocation_id, alloc_state) in list.flatten() {
                let name = format!("runtime-placement-{allocation_id}.token");
                if matches!(alloc_state.as_str(), "reserved" | "bridged") {
                    if let Some(token) =
                        runtime_token(store, RuntimeChannel::Placement, &allocation_id)
                    {
                        want.push((name, token));
                    }
                } else {
                    let _ = std::fs::remove_file(dir.join(&name));
                }
            }
        }
    }
    for (name, token) in want {
        let path = dir.join(&name);
        let current = std::fs::read_to_string(&path).unwrap_or_default();
        if current.trim() != token {
            let _ = std::fs::write(&path, format!("{token}\n"));
        }
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
}

// ------------------------------------------------------ small helpers ----

pub(crate) fn head_row(
    conn: &Connection,
    table: &str,
    col_a: &str,
    val_a: &str,
    col_b: &str,
    val_b: &str,
) -> Result<Option<Map<String, Value>>, Problem> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT * FROM {table} WHERE {col_a} = ?1 AND {col_b} = ?2"
        ))
        .map_err(db_err)?;
    let names: Vec<String> = stmt
        .column_names()
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    let mut query = stmt
        .query(rusqlite::params![val_a, val_b])
        .map_err(db_err)?;
    match query.next().map_err(db_err)? {
        Some(r) => {
            let mut m = Map::new();
            for (i, name) in names.iter().enumerate() {
                let v: rusqlite::types::Value = r.get(i).map_err(db_err)?;
                m.insert(
                    name.clone(),
                    match v {
                        rusqlite::types::Value::Null => Value::Null,
                        rusqlite::types::Value::Integer(i) => json!(i),
                        rusqlite::types::Value::Real(f) => json!(f),
                        rusqlite::types::Value::Text(t) => json!(t),
                        rusqlite::types::Value::Blob(_) => Value::Null,
                    },
                );
            }
            Ok(Some(m))
        }
        None => Ok(None),
    }
}

fn opt_json(v: &Option<String>) -> Value {
    v.as_ref().map(|s| json!(s)).unwrap_or(Value::Null)
}

fn json_text(v: &Value) -> Value {
    json!(v.to_string())
}

/// The derived ActivationAdmission id — the §11.1
/// `UNIQUE(wake_intent_ref, wake_intent_revision)` expressed as a
/// primary key. Derived from the subject it decides so the request can
/// only match the server value (the `gov_decision` idiom).
pub fn admission_ref(wake_intent_ref: &str, wake_intent_revision: u64) -> String {
    format!("adm-{wake_intent_ref}-r{wake_intent_revision}")
}

/// The derived ResourceAllocation id —
/// `UNIQUE(activation_admission_ref, stable_allocation_key)`.
pub fn allocation_ref(wake_intent_ref: &str, wake_intent_revision: u64) -> String {
    format!("alloc-{wake_intent_ref}-r{wake_intent_revision}")
}

/// The kernel-derived stable allocation key (never caller-chosen).
pub fn stable_allocation_key(wake_intent_ref: &str, wake_intent_revision: u64) -> String {
    format!("wake-{wake_intent_ref}-r{wake_intent_revision}")
}

pub fn bridge_ref(allocation_id: &str) -> String {
    format!("bridge-{allocation_id}")
}

pub fn reservation_set_ref(allocation_id: &str) -> String {
    format!("rset-{allocation_id}")
}

/// The stable external reservation key the bridge persists BEFORE
/// queueing (§11.4): kernel-derived, so Kovee can only echo it.
pub fn stable_external_key(allocation_id: &str) -> String {
    format!("sub-{allocation_id}")
}

fn record_digest(
    conn: &Connection,
    society_id: &str,
    object: &str,
    tag: &str,
    body: &Value,
) -> Result<DigestRef, Problem> {
    part_common::conn_record_digest(conn, society_id, object, tag, body)
}

// ============================================ stage 2: activation_admit ==

/// `activation_admit` (§11.1 named internal kernel transition, NOT a
/// callable BPP operation): deterministic evaluation of a COMMITTED
/// WakeIntent against the participant's ActivationPolicy, mandate, rate
/// ceiling and budget. The kernel may DENY but cannot invent an
/// interest — there is no path here that creates a WakeIntent.
///
/// The denial is a committed record, not a dropped request: the caller
/// receives the typed refusal AND the `admission_denied` row stays as
/// evidence (§14.8: "retry returns same admission").
fn activation_admit(
    store: &mut Store,
    caller: &Caller,
    req: &ops::EpisodeRequestRequest,
    body: &Value,
    now: i64,
) -> Result<Map<String, Value>, Problem> {
    let scope = MutationScope {
        society_id: caller.society_id.clone(),
        operation: "activation_admit".into(),
        actor: caller.actor.clone(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let admit_event = mint(store, "evt")?;
    let decided_at = rfc3339_utc(now);
    // The kernel transition is its OWN §15.3 authority transaction, so a
    // crash cell targets it by its transition name (§11.1: it is not a
    // callable operation, so no request op names it).
    let hooks = crate::dispatch::internal_hooks("activation_admit");
    let caller_c = caller.clone();
    let req_c = req.clone();
    run(store, scope, now, hooks, move |conn, _| {
        let intent = rows::get_row(
            conn,
            "wake_intents",
            "wake_intent_id",
            &req_c.wake_intent_ref,
        )
        .map_err(db_err)?
        .ok_or_else(|| {
            stage_required("stage 1 (WakeIntent) is not committed: no such wake intent")
        })?;
        if rows::str_of(&intent, "participant_ref") != caller_c.participant.participant_id {
            return Err(state::not_found());
        }
        let wi_revision = rows::u64_of(&intent, "revision");
        let admission_id = admission_ref(&req_c.wake_intent_ref, wi_revision);
        // The request can only MATCH the derived stage id.
        if req_c.activation_admission_ref != admission_id {
            return Err(stage_required(
                "activation_admission_ref is not the kernel-derived admission for this \
                 WakeIntent revision (the stage is kernel-authored, §11.1)",
            ));
        }
        // Crash retry: the one committed decision is served again, never
        // a second one (§14.8 ActivationAdmission row).
        if let Some(existing) =
            rows::get_row(conn, "activation_admissions", "admission_id", &admission_id)
                .map_err(db_err)?
        {
            return Ok(Prepared {
                result: admission_result(&existing),
                revision: Some(1),
                cursor: CursorMint::AfterEvents {
                    society_id: caller_c.society_id.clone(),
                },
                effects: Vec::new(),
                events: Vec::new(),
            });
        }
        let stream = rows::get_row(
            conn,
            "activity_streams",
            "activity_stream_id",
            &req_c.activity_stream_ref,
        )
        .map_err(db_err)?
        .ok_or_else(state::not_found)?;
        if rows::str_of(&stream, "participant_ref") != caller_c.participant.participant_id {
            return Err(state::not_found());
        }
        if rows::str_of(&intent, "activity_stream_ref") != req_c.activity_stream_ref
            || rows::u64_of(&intent, "generation") != req_c.generation
        {
            return Err(stage_required(
                "the WakeIntent does not name this ActivityStream generation",
            ));
        }
        if rows::u64_of(&stream, "generation") != req_c.generation {
            return Err(state::stale_binding("stale activity generation fence"));
        }

        // -- the deterministic eligibility evaluation -------------------
        let mut reasons: Vec<String> = Vec::new();
        let mut denied: Option<Problem> = None;
        let deny = |code: &str, problem: Problem, reasons: &mut Vec<String>| {
            reasons.push(code.to_owned());
            problem
        };

        if rows::str_of(&intent, "state") != "submitted" {
            denied = Some(deny(
                "wake_intent_not_pending",
                state::stale_binding("the WakeIntent is withdrawn or expired"),
                &mut reasons,
            ));
        } else if parse_rfc3339_utc(rows::str_of(&intent, "expires_at")).is_some_and(|t| t <= now) {
            denied = Some(deny(
                "wake_intent_expired",
                state::stale_binding("the WakeIntent has expired"),
                &mut reasons,
            ));
        } else if !matches!(
            rows::str_of(&stream, "state"),
            "ready" | "active" | "waiting"
        ) {
            denied = Some(deny(
                "activity_not_activatable",
                state::stale_binding("the ActivityStream cannot start an Episode"),
                &mut reasons,
            ));
        }

        // The mandate chain: absent, held, revoked, expired all deny.
        let mandate_refs: Vec<String> =
            serde_json::from_value(rows::json_of(&stream, "mandate_refs")).unwrap_or_default();
        let mandate_ref = mandate_refs.first().cloned().unwrap_or_default();
        if denied.is_none() {
            if mandate_ref.is_empty() {
                denied = Some(deny(
                    "no_mandate",
                    state::forbidden_detail("the ActivityStream binds no mandate (§11.1)"),
                    &mut reasons,
                ));
            } else if let Err(problem) = part_ops_mandate_gate(
                conn,
                &mandate_ref,
                &caller_c.participant.participant_id,
                rows::str_of(&stream, "purpose_ref"),
            ) {
                let code = if problem.kind == ProblemKind::MandateHeld {
                    "mandate_held"
                } else {
                    "mandate_unusable"
                };
                denied = Some(deny(code, problem, &mut reasons));
            }
        }

        // The activation policy a policy-derived intent cites must still
        // be the participant's ACTIVE one (revoking activation policy
        // fences queued work; §11.1).
        if denied.is_none() && rows::str_of(&intent, "origin") == "participant_activation_policy" {
            let cited = rows::str_of(&intent, "activation_policy_ref").to_owned();
            let active =
                rows::active_self_policy(conn, &caller_c.participant.participant_id, "activation")
                    .map_err(db_err)?;
            if active.is_none_or(|p| rows::str_of(&p, "policy_id") != cited) {
                denied = Some(deny(
                    "activation_policy_revoked",
                    state::stale_binding(
                        "the cited activation policy is no longer the participant's active one",
                    ),
                    &mut reasons,
                ));
            }
        }

        let mandate = if mandate_ref.is_empty() {
            None
        } else {
            rows::get_row(conn, "mandates", "mandate_id", &mandate_ref).map_err(db_err)?
        };

        // The RATE ceiling: the mandate's concurrency ceiling bounds
        // Episodes in flight under it (§11.4 fairness bounds).
        if denied.is_none() {
            if let Some(mandate) = &mandate {
                let ceiling = rows::u64_of(mandate, "concurrency_ceiling");
                let live = live_episodes_for_mandate(conn, &mandate_ref)?;
                if live >= ceiling {
                    denied = Some(deny(
                        "rate_ceiling",
                        part_common::budget_exceeded(
                            &mandate_ref,
                            "concurrency",
                            live + 1,
                            ceiling,
                        ),
                        &mut reasons,
                    ));
                }
            }
        }

        // The BUDGET ceiling: open Episode reservations never exceed the
        // mandate's §11.4 allowance on its ceiling set.
        if denied.is_none() {
            if let Some(mandate) = &mandate {
                let account = rows::str_of(mandate, "budget_ceiling_set_ref").to_owned();
                let held = open_episode_units(conn, &account)?;
                if held + EPISODE_WORST_CASE_UNITS > MANDATE_EPISODE_ALLOWANCE {
                    denied = Some(deny(
                        "budget_exhausted",
                        part_common::budget_exceeded(
                            &account,
                            part_common::UNIT_DIMENSION,
                            held + EPISODE_WORST_CASE_UNITS,
                            MANDATE_EPISODE_ALLOWANCE,
                        ),
                        &mut reasons,
                    ));
                }
            }
        }

        if denied.is_none() {
            reasons.push("wake_intent_committed".to_owned());
            reasons.push("mandate_current".to_owned());
            reasons.push("within_rate_ceiling".to_owned());
            reasons.push("within_budget_ceiling".to_owned());
        }
        let state = if denied.is_some() {
            "denied"
        } else {
            "admitted"
        };

        let dependency_set_ref = format!("depset-{admission_id}");
        let dependency_body = json!({
            "wake_intent_ref": req_c.wake_intent_ref,
            "wake_intent_revision": wi_revision,
            "activity_stream_ref": req_c.activity_stream_ref,
            "generation": req_c.generation,
            "mandate_ref": mandate_ref,
            "kernel_policy_version": KERNEL_POLICY_VERSION,
            "eligibility_reason_codes": reasons,
        });
        let dependency_digest = record_digest(
            conn,
            &caller_c.society_id,
            &dependency_set_ref,
            "bpp-activation-dependency-set-v0",
            &dependency_body,
        )?;
        let wi_digest = record_digest(
            conn,
            &caller_c.society_id,
            &req_c.wake_intent_ref,
            "bpp-wake-intent-v0",
            &json!({
                "wake_intent_id": req_c.wake_intent_ref,
                "revision": wi_revision,
                "stable_wake_key": rows::str_of(&intent, "stable_wake_key"),
                "exact_cause_ref": rows::str_of(&intent, "exact_cause_ref"),
                "origin": rows::str_of(&intent, "origin"),
            }),
        )?;
        let record = json!({
            "admission_id": admission_id,
            "wake_intent_ref": req_c.wake_intent_ref,
            "wake_intent_revision": wi_revision,
            "activity_stream_ref": req_c.activity_stream_ref,
            "generation": req_c.generation,
            "kernel_policy_version": KERNEL_POLICY_VERSION,
            "dependency_set_ref": dependency_set_ref,
            "eligibility_reason_codes": reasons,
            "state": state,
            "decided_at": decided_at,
        });
        let digest = record_digest(
            conn,
            &caller_c.society_id,
            &admission_id,
            "bpp-activation-admission-v0",
            &record,
        )?;
        let row = obj_pairs([
            ("admission_id", json!(admission_id)),
            ("society_id", json!(caller_c.society_id)),
            ("wake_intent_ref", json!(req_c.wake_intent_ref)),
            ("wake_intent_revision", json!(wi_revision)),
            ("wake_intent_digest", digest_json(&wi_digest)),
            ("activity_stream_ref", json!(req_c.activity_stream_ref)),
            ("generation", json!(req_c.generation)),
            (
                "participant_ref",
                json!(caller_c.participant.participant_id),
            ),
            ("kernel_policy_version", json!(KERNEL_POLICY_VERSION)),
            ("dependency_set_ref", json!(dependency_set_ref)),
            ("dependency_digest", digest_json(&dependency_digest)),
            ("eligibility_reason_codes", json_text(&json!(reasons))),
            ("state", json!(state)),
            ("decided_at", json!(decided_at)),
            ("digest", digest_json(&digest)),
        ]);
        let result = admission_result(&row);
        let refusal = denied
            .as_ref()
            .map(|p| p.detail.clone().unwrap_or_default());
        Ok(Prepared {
            result,
            revision: Some(1),
            cursor: CursorMint::AfterEvents {
                society_id: caller_c.society_id.clone(),
            },
            effects: vec![Effect::Upsert {
                table: "activation_admissions".into(),
                row,
            }],
            events: vec![event(
                &caller_c.society_id,
                &admit_event,
                if state == "admitted" {
                    "activation-admission.admitted"
                } else {
                    "activation-admission.denied"
                },
                &admission_id,
                1,
                &caller_c.participant.participant_id,
                ACTOR_KERNEL,
                &req_c.meta,
                json!({"state": state, "eligibility_reason_codes": reasons,
                       "refusal": refusal}),
            )],
        })
    })?;
    // The committed admission is the kernel's answer; the caller's
    // refusal (when denied) is raised by the caller of this function.
    let admission_id = current_admission_id(store, &req.wake_intent_ref)?;
    rows::get_row(
        store.conn(),
        "activation_admissions",
        "admission_id",
        &admission_id,
    )
    .map_err(db_err)?
    .ok_or_else(|| state::internal("the committed ActivationAdmission is missing"))
}

fn current_admission_id(store: &Store, wake_intent_ref: &str) -> Result<String, Problem> {
    let intent = rows::get_row(
        store.conn(),
        "wake_intents",
        "wake_intent_id",
        wake_intent_ref,
    )
    .map_err(db_err)?
    .ok_or_else(state::not_found)?;
    Ok(admission_ref(
        wake_intent_ref,
        rows::u64_of(&intent, "revision"),
    ))
}

fn admission_result(row: &Map<String, Value>) -> Value {
    json!({
        "admission_id": rows::str_of(row, "admission_id"),
        "wake_intent_ref": rows::str_of(row, "wake_intent_ref"),
        "wake_intent_revision": rows::u64_of(row, "wake_intent_revision"),
        "state": rows::str_of(row, "state"),
        "eligibility_reason_codes": rows::json_of(row, "eligibility_reason_codes"),
        "dependency_digest": rows::json_of(row, "dependency_digest"),
        "decided_at": rows::str_of(row, "decided_at"),
    })
}

/// The §11.1 mandate gate reused verbatim at admission time.
fn part_ops_mandate_gate(
    conn: &Connection,
    mandate_ref: &str,
    participant: &str,
    purpose_ref: &str,
) -> Result<(), Problem> {
    crate::part_ops::mandate_gate(conn, mandate_ref, participant, purpose_ref)
}

fn live_episodes_for_mandate(conn: &Connection, mandate_ref: &str) -> Result<u64, Problem> {
    let placeholders = LIVE_EPISODE_STATES
        .iter()
        .map(|s| format!("'{s}'"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT COUNT(*) FROM episodes WHERE mandate_ref = ?1 AND state IN ({placeholders})"
    );
    conn.query_row(&sql, [mandate_ref], |r| r.get::<_, i64>(0))
        .map(|n| n.max(0) as u64)
        .map_err(db_err)
}

/// Units currently held (reserved or uncertain) by Episode allocations
/// against one §11.4 ceiling set.
fn open_episode_units(conn: &Connection, account_ref: &str) -> Result<u64, Problem> {
    conn.query_row(
        "SELECT COALESCE(SUM(amount), 0) FROM budget_reservations
         WHERE holder_kind = 'episode_allocation' AND account_ref = ?1
           AND state IN ('reserved', 'uncertain')",
        [account_ref],
        |r| r.get::<_, i64>(0),
    )
    .map(|n| n.max(0) as u64)
    .map_err(db_err)
}

// ========================================== stage 3: resource_allocate ==

/// `resource_allocate` (§11.1 named internal kernel transition): can
/// only reserve an ADMITTED WakeIntent. It binds the current Mandate
/// uses, reserves every Byom-owned dimension in ONE Byom transaction,
/// and persists the `ExternalBudgetBridge` under its stable key BEFORE
/// queueing — the allocation stops at `reserved` until Kovee's exact
/// subordinate reservation confirms (§11.4: queueing requires BOTH
/// reservation sets).
fn resource_allocate(
    store: &mut Store,
    caller: &Caller,
    req: &ops::EpisodeRequestRequest,
    admission: &Map<String, Value>,
    body: &Value,
    now: i64,
) -> Result<Map<String, Value>, Problem> {
    let wi_revision = rows::u64_of(admission, "wake_intent_revision");
    let allocation_id = allocation_ref(&req.wake_intent_ref, wi_revision);
    let scope = MutationScope {
        society_id: caller.society_id.clone(),
        operation: "resource_allocate".into(),
        actor: caller.actor.clone(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let reserve_event = mint(store, "evt")?;
    let mandate_use_id = mint(store, "muse")?;
    let hooks = crate::dispatch::internal_hooks("resource_allocate");
    let created_at = rfc3339_utc(now);
    // The Kovee endpoint/binding epoch the bridge persists (§11.4) is
    // endpoint CONFIGURATION, read outside the prepare closure; an
    // endpoint with no installed seam records the unbound marker and the
    // bridge can only ever be resolved by governance.
    let (endpoint_ref, binding_epoch) = match crate::host_config::HostConfig::load(store) {
        Ok(cfg) => (
            cfg.realm_byom_binding.byom_endpoint_ref.clone(),
            cfg.realm_byom_binding.binding_epoch,
        ),
        Err(_) => ("kovee-endpoint-unbound".to_owned(), 0),
    };
    let caller_c = caller.clone();
    let req_c = req.clone();
    let admission_c = admission.clone();
    let allocation_c = allocation_id.clone();
    run(store, scope, now, hooks, move |conn, _| {
        if let Some(existing) =
            rows::get_row(conn, "resource_allocations", "allocation_id", &allocation_c)
                .map_err(db_err)?
        {
            return Ok(Prepared {
                result: allocation_result(&existing),
                revision: Some(rows::u64_of(&existing, "revision")),
                cursor: CursorMint::AfterEvents {
                    society_id: caller_c.society_id.clone(),
                },
                effects: Vec::new(),
                events: Vec::new(),
            });
        }
        // Stage 2 must be committed AND admitted — no allocation for a
        // denied or absent admission (§11.1).
        let admission_id = rows::str_of(&admission_c, "admission_id").to_owned();
        let committed = rows::get_row(conn, "activation_admissions", "admission_id", &admission_id)
            .map_err(db_err)?
            .ok_or_else(|| stage_required("stage 2 (ActivationAdmission) is not committed"))?;
        if rows::str_of(&committed, "state") != "admitted" {
            return Err(stage_required(
                "stage 3 (ResourceAllocation) can only reserve an ADMITTED WakeIntent",
            ));
        }
        let stream = rows::get_row(
            conn,
            "activity_streams",
            "activity_stream_id",
            &req_c.activity_stream_ref,
        )
        .map_err(db_err)?
        .ok_or_else(state::not_found)?;
        let mandate_refs: Vec<String> =
            serde_json::from_value(rows::json_of(&stream, "mandate_refs")).unwrap_or_default();
        let mandate_ref = mandate_refs.first().cloned().unwrap_or_default();
        let mandate = rows::get_row(conn, "mandates", "mandate_id", &mandate_ref)
            .map_err(db_err)?
            .ok_or_else(|| stage_required("the bound mandate is gone"))?;
        let account = rows::str_of(&mandate, "budget_ceiling_set_ref").to_owned();

        let mut effects = Vec::new();
        // Every Byom-owned dimension reserves in ONE Byom transaction
        // (§11.4). Kovee platform capacity lives under another owner and
        // is therefore not part of it.
        part_common::reserve(
            conn,
            &mut effects,
            &caller_c.society_id,
            &account,
            part_common::UNIT_DIMENSION,
            EPISODE_WORST_CASE_UNITS,
            &format!("res-{allocation_c}"),
            "episode_allocation",
            &allocation_c,
            now,
        )?;

        let set_ref = reservation_set_ref(&allocation_c);
        let items = json!([{
            "account_ref": account,
            "account_revision": 1,
            "dimension": part_common::UNIT_DIMENSION,
            "unit": part_common::UNIT_DIMENSION,
            "worst_case_amount": EPISODE_WORST_CASE_UNITS,
        }]);
        let set_digest = record_digest(
            conn,
            &caller_c.society_id,
            &set_ref,
            "bpp-budget-reservation-set-v0",
            &json!({"reservation_set_id": set_ref, "revision": 1, "items": items}),
        )?;

        // The idempotent bridge saga persists the ExternalBudgetBridge
        // with its stable key BEFORE queueing (§11.4).
        let bridge_id = bridge_ref(&allocation_c);
        let stable_key = stable_external_key(&allocation_c);
        let bridge_body = json!({
            "bridge_id": bridge_id,
            "revision": 1,
            "byom_reservation_set_ref": set_ref,
            "stable_external_reservation_key": stable_key,
            "external_owner": "kovee",
            "external_endpoint_ref": endpoint_ref,
            "external_binding_epoch": binding_epoch,
            "state": "requested",
        });
        let bridge_digest = record_digest(
            conn,
            &caller_c.society_id,
            &bridge_id,
            "bpp-external-budget-bridge-v0",
            &bridge_body,
        )?;
        effects.push(Effect::Upsert {
            table: "external_budget_bridges".into(),
            row: obj_pairs([
                ("bridge_id", json!(bridge_id)),
                ("society_id", json!(caller_c.society_id)),
                ("revision", json!(1)),
                ("byom_reservation_set_ref", json!(set_ref)),
                ("byom_reservation_set_revision", json!(1)),
                ("byom_reservation_set_digest", digest_json(&set_digest)),
                ("external_owner", json!("kovee")),
                ("external_endpoint_ref", json!(endpoint_ref)),
                ("external_binding_epoch", json!(binding_epoch)),
                ("stable_external_reservation_key", json!(stable_key)),
                ("subordinate_reservation_ref", Value::Null),
                ("subordinate_reservation_revision", Value::Null),
                ("subordinate_reservation_digest", Value::Null),
                ("state", json!("requested")),
                ("reconcile_decision_ref", Value::Null),
                ("settled_charge", json!(0)),
                ("created_at", json!(created_at)),
                ("digest", digest_json(&bridge_digest)),
            ]),
        });

        let allocation_body = json!({
            "allocation_id": allocation_c,
            "revision": 1,
            "activation_admission_ref": admission_id,
            "activity_stream_ref": req_c.activity_stream_ref,
            "generation": req_c.generation,
            "mandate_use_refs": [mandate_use_id],
            "byom_budget_reservation_set_ref": set_ref,
            "external_budget_bridge_ref": bridge_id,
            "stable_allocation_key": stable_allocation_key(&req_c.wake_intent_ref, wi_revision),
            "state": "reserved",
        });
        let allocation_digest = record_digest(
            conn,
            &caller_c.society_id,
            &allocation_c,
            "bpp-resource-allocation-v0",
            &allocation_body,
        )?;
        let expires_at = rfc3339_utc(now + 86_400);
        let row = obj_pairs([
            ("allocation_id", json!(allocation_c)),
            ("society_id", json!(caller_c.society_id)),
            ("revision", json!(1)),
            ("activation_admission_ref", json!(admission_id)),
            ("activity_stream_ref", json!(req_c.activity_stream_ref)),
            ("generation", json!(req_c.generation)),
            (
                "participant_ref",
                json!(caller_c.participant.participant_id),
            ),
            ("mandate_ref", json!(mandate_ref)),
            ("mandate_use_refs", json_text(&json!([mandate_use_id]))),
            ("byom_budget_reservation_set_ref", json!(set_ref)),
            ("reservation_items", json_text(&items)),
            ("external_budget_bridge_ref", json!(bridge_id)),
            ("rate_counter_use_refs", json_text(&json!([mandate_ref]))),
            (
                "stable_allocation_key",
                json!(stable_allocation_key(&req_c.wake_intent_ref, wi_revision)),
            ),
            ("expires_at", json!(expires_at)),
            ("state", json!("reserved")),
            (
                "dependency_digest",
                rows::json_of(&committed, "dependency_digest"),
            ),
            ("digest", digest_json(&allocation_digest)),
            ("created_at", json!(created_at)),
        ]);
        let result = allocation_result(&row);
        effects.push(Effect::Upsert {
            table: "resource_allocations".into(),
            row,
        });
        Ok(Prepared {
            result,
            revision: Some(1),
            cursor: CursorMint::AfterEvents {
                society_id: caller_c.society_id.clone(),
            },
            effects,
            events: vec![event(
                &caller_c.society_id,
                &reserve_event,
                "resource-allocation.reserved",
                &allocation_c,
                1,
                &caller_c.participant.participant_id,
                ACTOR_KERNEL,
                &req_c.meta,
                json!({"state": "reserved", "external_budget_bridge_ref": bridge_id,
                       "stable_external_reservation_key": stable_key,
                       "worst_case_units": EPISODE_WORST_CASE_UNITS}),
            )],
        })
    })?;
    rows::get_row(
        store.conn(),
        "resource_allocations",
        "allocation_id",
        &allocation_id,
    )
    .map_err(db_err)?
    .ok_or_else(|| state::internal("the committed ResourceAllocation is missing"))
}

fn allocation_result(row: &Map<String, Value>) -> Value {
    json!({
        "allocation_id": rows::str_of(row, "allocation_id"),
        "revision": rows::u64_of(row, "revision"),
        "activation_admission_ref": rows::str_of(row, "activation_admission_ref"),
        "state": rows::str_of(row, "state"),
        "byom_budget_reservation_set_ref": rows::str_of(row, "byom_budget_reservation_set_ref"),
        "external_budget_bridge_ref": rows::str_of(row, "external_budget_bridge_ref"),
        "digest": rows::json_of(row, "digest"),
    })
}

// ============================================= budget ledger movements ==

/// Moves `amount` between two §11.4 buckets of one `(account, dimension)`
/// row, preserving
/// `ceiling = remaining + reserved + committed + uncertain + delegated_to_children`
/// on every move. Reads effects already staged in this transition so
/// several moves compose inside one transaction.
fn account_move(
    conn: &Connection,
    effects: &mut Vec<Effect>,
    account_ref: &str,
    dimension: &str,
    from: &str,
    to: &str,
    amount: u64,
) -> Result<(), Problem> {
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
        None => rows::budget_account(conn, account_ref, dimension)
            .map_err(db_err)?
            .ok_or_else(|| state::internal("budget account row is missing"))?,
    };
    let have = rows::u64_of(&account, from);
    if have < amount {
        return Err(state::internal(&format!(
            "budget bucket {from} underflow on {account_ref}/{dimension}"
        )));
    }
    account.insert(from.into(), json!(have - amount));
    account.insert(to.into(), json!(rows::u64_of(&account, to) + amount));
    account.insert(
        "revision".into(),
        json!(rows::u64_of(&account, "revision") + 1),
    );
    effects.push(Effect::Upsert {
        table: "budget_accounts".into(),
        row: account,
    });
    Ok(())
}

/// The Episode allocation's one reservation row plus its account
/// coordinates.
fn allocation_reservation(
    conn: &Connection,
    allocation_id: &str,
) -> Result<Map<String, Value>, Problem> {
    rows::get_row(
        conn,
        "budget_reservations",
        "reservation_id",
        &format!("res-{allocation_id}"),
    )
    .map_err(db_err)?
    .ok_or_else(|| state::internal("the allocation's reservation row is missing"))
}

fn set_reservation(
    effects: &mut Vec<Effect>,
    mut reservation: Map<String, Value>,
    amount: u64,
    state_name: &str,
) {
    reservation.insert("amount".into(), json!(amount));
    reservation.insert("state".into(), json!(state_name));
    effects.push(Effect::Upsert {
        table: "budget_reservations".into(),
        row: reservation,
    });
}

// ======================================== stage 1..3 driver: episode_request

/// `episode_request` (participant, create; R29). ONE participant call
/// drives the three byom-owned activation stages in order, each as its
/// OWN §15.3 authority transaction with its own idempotency domain, so a
/// crash between two stages recovers the committed prefix instead of
/// re-deciding it:
///
/// ```text
/// wake_intent_submit (B1)  stage 1  WakeIntent            participant
/// activation_admit         stage 2  ActivationAdmission   kernel
/// resource_allocate        stage 3  ResourceAllocation    kernel
/// episode_request          Episode prepared -> eligible   participant
/// placement_admit          stage 4  PlacementAdmission    Kovee adapter
/// ```
pub fn episode_request(
    store: &mut Store,
    caller: &Caller,
    req: &ops::EpisodeRequestRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    check_meta_binding(store, &req.meta, &caller.society_id)?;
    // Stage 2 — the kernel decides; a denial is a committed record AND a
    // typed refusal.
    let admission = activation_admit(store, caller, req, body, now)?;
    if rows::str_of(&admission, "state") != "admitted" {
        let codes: Vec<String> =
            serde_json::from_value(rows::json_of(&admission, "eligibility_reason_codes"))
                .unwrap_or_default();
        let code = codes
            .first()
            .cloned()
            .unwrap_or_else(|| "denied".to_owned());
        let admission_id = rows::str_of(&admission, "admission_id").to_owned();
        return Err(match code.as_str() {
            "budget_exhausted" | "rate_ceiling" => {
                Problem::new(ProblemKind::BudgetExceeded, "budget ceiling exceeded")
                    .with_status(409)
                    .with_detail(format!(
                        "ActivationAdmission {admission_id} is denied ({code}); the denial is \
                         committed evidence and a retry returns the same admission"
                    ))
            }
            "mandate_held" => Problem::new(
                ProblemKind::MandateHeld,
                "the bound mandate is held; new uses are fenced",
            )
            .with_status(409)
            .with_detail(format!(
                "ActivationAdmission {admission_id} is denied ({code})"
            )),
            "no_mandate" | "mandate_unusable" => state::forbidden_detail(&format!(
                "ActivationAdmission {admission_id} is denied ({code})"
            )),
            _ => state::stale_binding(&format!(
                "ActivationAdmission {admission_id} is denied ({code})"
            )),
        });
    }
    // Stage 3 — reserve every byom dimension and persist the bridge.
    let allocation = resource_allocate(store, caller, req, &admission, body, now)?;

    // The Episode itself: prepared -> eligible. It does NOT queue here —
    // queueing requires both exact reservation sets (§11.4), which only
    // Kovee's subordinate confirmation completes.
    let episode_id = mint(store, "ep")?;
    let create_event = mint(store, "evt")?;
    let created_at = rfc3339_utc(now);
    let incarnation = store
        .incarnation()
        .map_err(|e| state::internal(&e.to_string()))?;
    let recovery_epoch = store
        .recovery_epoch(&caller.society_id)
        .map_err(|e| state::internal(&e.to_string()))?;
    let scope = MutationScope {
        society_id: caller.society_id.clone(),
        operation: "episode_request".into(),
        actor: caller.actor.clone(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let caller_c = caller.clone();
    let req_c = req.clone();
    let allocation_c = allocation.clone();
    let admission_c = admission.clone();
    let reply = run(store, scope, now, hooks, move |conn, _| {
        let allocation_id = rows::str_of(&allocation_c, "allocation_id").to_owned();
        let committed = rows::get_row(
            conn,
            "resource_allocations",
            "allocation_id",
            &allocation_id,
        )
        .map_err(db_err)?
        .ok_or_else(|| stage_required("stage 3 (ResourceAllocation) is not committed"))?;
        if !matches!(rows::str_of(&committed, "state"), "reserved" | "bridged") {
            return Err(stage_required(
                "the ResourceAllocation is released, revoked or uncertain",
            ));
        }
        // One Episode per allocation: a retry replays through
        // idempotency, and a second allocation-less Episode is refused.
        if let Some(existing) = rows::rows_where(
            conn,
            "episodes",
            "resource_allocation_ref",
            &allocation_id,
            "episode_id",
        )
        .map_err(db_err)?
        .into_iter()
        .next()
        {
            return Ok(Prepared {
                result: episode_result(&existing),
                revision: Some(rows::u64_of(&existing, "revision")),
                cursor: CursorMint::AfterEvents {
                    society_id: caller_c.society_id.clone(),
                },
                effects: Vec::new(),
                events: Vec::new(),
            });
        }
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
        let intent = rows::get_row(
            conn,
            "wake_intents",
            "wake_intent_id",
            &req_c.wake_intent_ref,
        )
        .map_err(db_err)?
        .ok_or_else(state::not_found)?;
        let deadline_unix = req_c
            .deadline
            .as_deref()
            .and_then(parse_rfc3339_utc)
            .map(|t| json!(t))
            .unwrap_or(Value::Null);
        let row = obj_pairs([
            ("episode_id", json!(episode_id)),
            ("society_id", json!(caller_c.society_id)),
            ("activity_stream_ref", json!(req_c.activity_stream_ref)),
            ("generation", json!(req_c.generation)),
            ("revision", json!(1)),
            ("endpoint_incarnation", json!(incarnation)),
            ("recovery_epoch", json!(recovery_epoch)),
            (
                "participant_ref",
                json!(caller_c.participant.participant_id),
            ),
            ("manifestation_ref", Value::Null),
            (
                "mandate_ref",
                json!(rows::str_of(&committed, "mandate_ref")),
            ),
            ("wake_intent_ref", json!(req_c.wake_intent_ref)),
            (
                "activation_admission_ref",
                json!(rows::str_of(&admission_c, "admission_id")),
            ),
            ("resource_allocation_ref", json!(allocation_id)),
            ("placement_admission_ref", Value::Null),
            (
                "wake_cause_ref",
                json!(rows::str_of(&intent, "exact_cause_ref")),
            ),
            (
                "admission_cursor",
                json!(rows::str_of(&intent, "stable_wake_key")),
            ),
            ("context_manifest_ref", Value::Null),
            ("context_manifest_digest", Value::Null),
            (
                "mandate_use_refs",
                json!(rows::str_of(&committed, "mandate_use_refs")),
            ),
            (
                "budget_reservation_set_ref",
                json!(rows::str_of(&committed, "byom_budget_reservation_set_ref")),
            ),
            ("deadline", opt_json(&req_c.deadline)),
            ("deadline_unix", deadline_unix),
            ("state", json!("eligible")),
            ("created_at", json!(created_at)),
            ("terminal_at", Value::Null),
        ]);
        let result = episode_result(&row);
        let mut effects = vec![Effect::Upsert {
            table: "episodes".into(),
            row,
        }];
        // The ActivityStream generation opens (§14.8: ready/waiting ->
        // active via episode_request).
        if matches!(rows::str_of(&stream, "state"), "ready" | "waiting") {
            let mut active = stream.clone();
            active.insert("state".into(), json!("active"));
            active.insert(
                "revision".into(),
                json!(rows::u64_of(&stream, "revision") + 1),
            );
            effects.push(Effect::Upsert {
                table: "activity_streams".into(),
                row: active,
            });
        }
        Ok(Prepared {
            result,
            revision: Some(1),
            cursor: CursorMint::AfterEvents {
                society_id: caller_c.society_id.clone(),
            },
            effects,
            events: vec![event(
                &caller_c.society_id,
                &create_event,
                "episode.eligible",
                &episode_id,
                1,
                &caller_c.participant.participant_id,
                &caller_c.actor,
                &req_c.meta,
                json!({"transitions": ["absent->prepared", "prepared->eligible"],
                       "state": "eligible",
                       "resource_allocation_ref": allocation_id,
                       "queued": false,
                       "queue_blocked_on":
                         "both exact reservation sets (§11.4): the byom_subordinate \
                          bridge is still requested"}),
            )],
        })
    })?;
    ensure_runtime_token_files(store);
    Ok(reply)
}

fn episode_result(row: &Map<String, Value>) -> Value {
    json!({
        "episode_id": rows::str_of(row, "episode_id"),
        "activity_stream_id": rows::str_of(row, "activity_stream_ref"),
        "generation": rows::u64_of(row, "generation"),
        "revision": rows::u64_of(row, "revision"),
        "state": rows::str_of(row, "state"),
        "created_at": rows::str_of(row, "created_at"),
    })
}

// ============================================= stage 4: placement_admit ==

/// `placement_admit` (runtime, create; R33): the narrow Kovee placement
/// adapter bound to the exact ResourceAllocation. It records only source
/// FACTS after verification — Kovee alone authors the PlacementBinding
/// (§11.1) — and it carries the `byom_subordinate` saga outcome, because
/// §14.6 defines no byom-side catalog operation for the Kovee-owned saga
/// verbs and byom holds no outbound Kovee client in this slice (recorded
/// deviation). Guards, record shape and conservation are exactly the
/// committed descriptor's.
///
/// `confirmed` completes stage 3 (`reserved -> bridged`) and queues the
/// Episode; `denied` releases only the demonstrably unspent byom
/// reservation; `uncertain` moves the hold into the §11.4 `uncertain`
/// bucket, leaves the Episode UNQUEUED, and blocks spend until the R38
/// `budget_reconcile` seat decides (family contract L33).
pub fn placement_admit(
    store: &mut Store,
    token: &str,
    req: &ops::PlacementAdmitRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    let allocation = rows::get_row(
        store.conn(),
        "resource_allocations",
        "allocation_id",
        &req.resource_allocation_ref,
    )
    .map_err(db_err)?
    .ok_or_else(state::not_found)?;
    verify_runtime_token(
        store,
        token,
        RuntimeChannel::Placement,
        &req.resource_allocation_ref,
    )?;
    let society_id = rows::str_of(&allocation, "society_id").to_owned();
    check_meta_binding(store, &req.meta, &society_id)?;
    let admission_id = format!("plc-{}", req.kovee_placement_ref);
    let admit_event = mint(store, "evt")?;
    let queue_event = mint(store, "evt")?;
    let admitted_at = rfc3339_utc(now);
    let scope = MutationScope {
        society_id: society_id.clone(),
        operation: "placement_admit".into(),
        actor: ACTOR_PLACEMENT.into(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let req_c = req.clone();
    let society_c = society_id.clone();
    let reply = run(store, scope, now, hooks, move |conn, _| {
        let allocation = rows::get_row(
            conn,
            "resource_allocations",
            "allocation_id",
            &req_c.resource_allocation_ref,
        )
        .map_err(db_err)?
        .ok_or_else(state::not_found)?;
        if !req_c
            .resource_allocation_digest
            .same_ref_json(&rows::json_of(&allocation, "digest"))
        {
            return Err(state::stale_binding(
                "resource_allocation_digest does not pin the committed allocation",
            ));
        }
        let allocation_id = rows::str_of(&allocation, "allocation_id").to_owned();
        let bridge_id = rows::str_of(&allocation, "external_budget_bridge_ref").to_owned();
        let bridge = rows::get_row(conn, "external_budget_bridges", "bridge_id", &bridge_id)
            .map_err(db_err)?
            .ok_or_else(|| stage_required("the ExternalBudgetBridge is not committed"))?;
        let sub = &req_c.subordinate_reservation;
        if sub.stable_external_reservation_key
            != rows::str_of(&bridge, "stable_external_reservation_key")
        {
            return Err(state::stale_binding(
                "the subordinate reservation does not echo the kernel-derived stable key",
            ));
        }
        let account = {
            let items = rows::json_of(&allocation, "reservation_items");
            items
                .get(0)
                .and_then(|i| i.get("account_ref"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };

        // -- CreateOnce: the exact retry returns the identical row ------
        let already = rows::str_of(&bridge, "state").to_owned();
        if already != "requested" {
            // Only the uncertain -> confirmed/denied stable-query
            // resolution may advance a non-requested bridge, and it can
            // only surface what Kovee reports (ResolutionIsReal).
            if already != "uncertain" {
                let existing = rows::get_row(
                    conn,
                    "placement_admissions",
                    "resource_allocation_ref",
                    &allocation_id,
                )
                .map_err(db_err)?;
                if let Some(existing) = existing {
                    if rows::str_of(&existing, "kovee_placement_ref") == req_c.kovee_placement_ref
                        && rows::u64_of(&existing, "kovee_placement_revision")
                            == req_c.kovee_placement_revision
                    {
                        return Ok(Prepared {
                            result: placement_result(&existing, &bridge),
                            revision: Some(1),
                            cursor: CursorMint::AfterEvents {
                                society_id: society_c.clone(),
                            },
                            effects: Vec::new(),
                            events: Vec::new(),
                        });
                    }
                }
                return Err(state::stale_binding(&format!(
                    "the external budget bridge is already {already}; a released bridge never \
                     revives and new work is a fresh saga row under a fresh stable key"
                )));
            }
        }

        let mut effects: Vec<Effect> = Vec::new();
        let mut events = Vec::new();
        let mut bridge_row = bridge.clone();
        let bridge_revision = rows::u64_of(&bridge, "revision") + 1;
        bridge_row.insert("revision".into(), json!(bridge_revision));
        let mut allocation_row = allocation.clone();
        let reservation = allocation_reservation(conn, &allocation_id)?;
        let held = rows::u64_of(&reservation, "amount");
        // Which §11.4 bucket currently holds the byom quantity: a bridge
        // that already went `uncertain` parked it in `uncertain`, and the
        // stable-query resolution moves it from there.
        let held_bucket = if already == "uncertain" {
            "uncertain"
        } else {
            "reserved"
        };

        match sub.outcome.as_str() {
            "confirmed" => {
                // NEVER ABOVE PARENT: identical dimension and unit, and
                // `amount <= parent_worst_case_amount`, per item
                // (§11.4, family contract L32;
                // SubordinateReservation.tla NeverAboveParent).
                let parent_items = rows::json_of(&allocation, "reservation_items");
                for item in &sub.items {
                    if item.dimension != item.parent_dimension || item.unit != item.parent_unit {
                        return Err(state::invalid(
                            "a subordinate reservation may narrow or deny but never reshape the \
                             dimension (§11.4)",
                        ));
                    }
                    if item.amount > item.parent_worst_case_amount {
                        return Err(part_common::budget_exceeded(
                            &item.parent_account_ref,
                            &item.parent_dimension,
                            item.amount,
                            item.parent_worst_case_amount,
                        ));
                    }
                    // The pinned parent item must be an item this
                    // allocation actually reserved.
                    let matched = parent_items.as_array().into_iter().flatten().any(|p| {
                        p.get("account_ref").and_then(Value::as_str)
                            == Some(item.parent_account_ref.as_str())
                            && p.get("dimension").and_then(Value::as_str)
                                == Some(item.parent_dimension.as_str())
                            && p.get("worst_case_amount").and_then(Value::as_u64)
                                == Some(item.parent_worst_case_amount)
                    });
                    if !matched {
                        return Err(state::stale_binding(
                            "the subordinate item does not pin an exact parent §11.4 \
                             reservation item",
                        ));
                    }
                }
                let sub_ref = sub.subordinate_reservation_ref.clone().unwrap_or_default();
                let sub_revision = sub.revision.unwrap_or_default();
                let sub_digest = sub.digest.clone();
                let record = json!({
                    "subordinate_reservation_ref": sub_ref,
                    "revision": sub_revision,
                    "reservation_class": "byom_subordinate",
                    "external_budget_bridge_ref": bridge_id,
                    "stable_external_reservation_key": sub.stable_external_reservation_key,
                    "byom_reservation_set_ref":
                        rows::str_of(&bridge, "byom_reservation_set_ref"),
                    "byom_reservation_set_revision":
                        rows::u64_of(&bridge, "byom_reservation_set_revision"),
                    "items": serde_json::to_value(&sub.items).unwrap_or(Value::Null),
                    "state": "confirmed",
                    "created_at": admitted_at,
                });
                effects.push(Effect::Upsert {
                    table: "subordinate_reservations".into(),
                    row: obj_pairs([
                        ("subordinate_reservation_ref", json!(sub_ref)),
                        ("society_id", json!(society_c)),
                        ("external_budget_bridge_ref", json!(bridge_id)),
                        (
                            "stable_external_reservation_key",
                            json!(sub.stable_external_reservation_key),
                        ),
                        ("revision", json!(sub_revision)),
                        ("reservation_class", json!("byom_subordinate")),
                        ("record", json_text(&record)),
                        ("state", json!("confirmed")),
                        ("created_at", json!(admitted_at)),
                        (
                            "digest",
                            sub_digest.as_ref().map(digest_json).unwrap_or(Value::Null),
                        ),
                    ]),
                });
                // A stable-query resolution brings the parked hold back
                // into `reserved` — the byom parent stays held the whole
                // time (no parallel charge, no early release).
                if held_bucket == "uncertain" {
                    account_move(
                        conn,
                        &mut effects,
                        &account,
                        part_common::UNIT_DIMENSION,
                        "uncertain",
                        "reserved",
                        held,
                    )?;
                    set_reservation(&mut effects, reservation.clone(), held, "reserved");
                }
                bridge_row.insert("state".into(), json!("confirmed"));
                bridge_row.insert("subordinate_reservation_ref".into(), json!(sub_ref));
                bridge_row.insert(
                    "subordinate_reservation_revision".into(),
                    json!(sub_revision),
                );
                bridge_row.insert(
                    "subordinate_reservation_digest".into(),
                    sub_digest.as_ref().map(digest_json).unwrap_or(Value::Null),
                );
                // Stage 3 completes: reserved -> bridged.
                allocation_row.insert("state".into(), json!("bridged"));
                allocation_row.insert(
                    "revision".into(),
                    json!(rows::u64_of(&allocation, "revision") + 1),
                );
                events.push(event(
                    &society_c,
                    &admit_event,
                    "subordinate-reservation.confirmed",
                    &sub_ref,
                    sub_revision,
                    rows::str_of(&allocation, "participant_ref"),
                    ACTOR_PLACEMENT,
                    &req_c.meta,
                    json!({"state": "confirmed", "bridge": bridge_id,
                           "items": serde_json::to_value(&sub.items).unwrap_or(Value::Null)}),
                ));
            }
            "denied" => {
                // A denial releases only demonstrably unspent byom
                // reservations (§11.4).
                bridge_row.insert("state".into(), json!("released"));
                account_move(
                    conn,
                    &mut effects,
                    &account,
                    part_common::UNIT_DIMENSION,
                    held_bucket,
                    "remaining",
                    held,
                )?;
                set_reservation(&mut effects, reservation.clone(), held, "released");
                allocation_row.insert("state".into(), json!("released"));
                allocation_row.insert(
                    "revision".into(),
                    json!(rows::u64_of(&allocation, "revision") + 1),
                );
                events.push(event(
                    &society_c,
                    &admit_event,
                    "subordinate-reservation.denied",
                    &bridge_id,
                    bridge_revision,
                    rows::str_of(&allocation, "participant_ref"),
                    ACTOR_PLACEMENT,
                    &req_c.meta,
                    json!({"state": "denied", "released_unspent": held}),
                ));
            }
            _ => {
                // Unknown result: the byom reservation is NOT released
                // and spend stays blocked (§11.4, L33). The hold moves
                // into the ledger's `uncertain` bucket, so conservation
                // still holds and nothing returns to `remaining`
                // without the R38 decision.
                if already != "uncertain" {
                    bridge_row.insert("state".into(), json!("uncertain"));
                    account_move(
                        conn,
                        &mut effects,
                        &account,
                        part_common::UNIT_DIMENSION,
                        "reserved",
                        "uncertain",
                        held,
                    )?;
                    set_reservation(&mut effects, reservation.clone(), held, "uncertain");
                }
                allocation_row.insert("state".into(), json!("uncertain"));
                allocation_row.insert(
                    "revision".into(),
                    json!(rows::u64_of(&allocation, "revision") + 1),
                );
                events.push(event(
                    &society_c,
                    &admit_event,
                    "subordinate-reservation.uncertain",
                    &bridge_id,
                    bridge_revision,
                    rows::str_of(&allocation, "participant_ref"),
                    ACTOR_PLACEMENT,
                    &req_c.meta,
                    json!({"state": "uncertain",
                           "byom_reservation_released": false,
                           "spend": "blocked until the R38 budget_reconcile seat decides"}),
                ));
            }
        }

        let bridge_digest = record_digest(
            conn,
            &society_c,
            &bridge_id,
            "bpp-external-budget-bridge-v0",
            &json!({
                "bridge_id": bridge_id,
                "revision": bridge_revision,
                "byom_reservation_set_ref": rows::str_of(&bridge, "byom_reservation_set_ref"),
                "stable_external_reservation_key": sub.stable_external_reservation_key,
                "external_owner": "kovee",
                "external_endpoint_ref": rows::str_of(&bridge, "external_endpoint_ref"),
                "external_binding_epoch": rows::u64_of(&bridge, "external_binding_epoch"),
                "state": rows::str_of(&bridge_row, "state"),
            }),
        )?;
        bridge_row.insert("digest".into(), digest_json(&bridge_digest));
        effects.push(Effect::Upsert {
            table: "external_budget_bridges".into(),
            row: bridge_row.clone(),
        });

        // The PlacementAdmission itself — only on a confirmed bridge:
        // Kovee cannot place work whose external capacity is denied or
        // unknown.
        let mut result_extra = Value::Null;
        if sub.outcome == "confirmed" {
            let record = json!({
                "admission_id": admission_id,
                "resource_allocation_ref": allocation_id,
                "kovee_placement_ref": req_c.kovee_placement_ref,
                "kovee_placement_revision": req_c.kovee_placement_revision,
                "source_binding_epoch": req_c.source_binding_epoch,
                "selected_manifestation_ref": req_c.selected_manifestation_ref,
                "kovee_invocation_ref": req_c.kovee_invocation_ref,
                "kovee_fence_epoch": req_c.kovee_fence_epoch,
                "verification_status": "verified",
                "admitted_at": admitted_at,
            });
            let digest = record_digest(
                conn,
                &society_c,
                &admission_id,
                "bpp-placement-admission-v0",
                &record,
            )?;
            let placement_row = obj_pairs([
                ("admission_id", json!(admission_id)),
                ("society_id", json!(society_c)),
                ("resource_allocation_ref", json!(allocation_id)),
                (
                    "resource_allocation_digest",
                    rows::json_of(&allocation, "digest"),
                ),
                ("kovee_placement_ref", json!(req_c.kovee_placement_ref)),
                (
                    "kovee_placement_revision",
                    json!(req_c.kovee_placement_revision),
                ),
                (
                    "kovee_placement_digest",
                    digest_json(&req_c.kovee_placement_digest),
                ),
                ("source_binding_epoch", json!(req_c.source_binding_epoch)),
                (
                    "selected_manifestation_ref",
                    json!(req_c.selected_manifestation_ref),
                ),
                ("kovee_invocation_ref", json!(req_c.kovee_invocation_ref)),
                ("kovee_fence_epoch", json!(req_c.kovee_fence_epoch)),
                ("verification_status", json!("verified")),
                ("admitted_at", json!(admitted_at)),
                ("digest", digest_json(&digest)),
            ]);
            result_extra = placement_result(&placement_row, &bridge_row);
            effects.push(Effect::Upsert {
                table: "placement_admissions".into(),
                row: placement_row,
            });
            // eligible -> queued via resource_allocate (§14.8): the
            // Episode queues ONLY now, behind both exact reservation
            // sets and an already eligible Manifestation.
            if let Some(episode) = rows::rows_where(
                conn,
                "episodes",
                "resource_allocation_ref",
                &allocation_id,
                "episode_id",
            )
            .map_err(db_err)?
            .into_iter()
            .next()
            {
                if rows::str_of(&episode, "state") == "eligible" {
                    let episode_id = rows::str_of(&episode, "episode_id").to_owned();
                    let revision = rows::u64_of(&episode, "revision") + 1;
                    let mut queued = episode.clone();
                    queued.insert("state".into(), json!("queued"));
                    queued.insert("revision".into(), json!(revision));
                    queued.insert(
                        "manifestation_ref".into(),
                        json!(req_c.selected_manifestation_ref),
                    );
                    queued.insert("placement_admission_ref".into(), json!(admission_id));
                    effects.push(Effect::Upsert {
                        table: "episodes".into(),
                        row: queued,
                    });
                    events.push(event(
                        &society_c,
                        &queue_event,
                        "episode.queued",
                        &episode_id,
                        revision,
                        rows::str_of(&episode, "participant_ref"),
                        ACTOR_KERNEL,
                        &req_c.meta,
                        json!({"state": "queued",
                               "via": "resource_allocate (both exact reservation sets)",
                               "placement_admission_ref": admission_id}),
                    ));
                }
            }
        }
        effects.push(Effect::Upsert {
            table: "resource_allocations".into(),
            row: allocation_row,
        });
        let result = if result_extra.is_null() {
            json!({
                "resource_allocation_ref": allocation_id,
                "placement_admitted": false,
                "external_budget_bridge_ref": bridge_id,
                "bridge_state": rows::str_of(&bridge_row, "state"),
                "episode_queued": false,
            })
        } else {
            result_extra
        };
        Ok(Prepared {
            result,
            revision: Some(bridge_revision),
            cursor: CursorMint::AfterEvents {
                society_id: society_c.clone(),
            },
            effects,
            events,
        })
    })?;
    ensure_runtime_token_files(store);
    Ok(reply)
}

fn placement_result(row: &Map<String, Value>, bridge: &Map<String, Value>) -> Value {
    json!({
        "admission_id": rows::str_of(row, "admission_id"),
        "resource_allocation_ref": rows::str_of(row, "resource_allocation_ref"),
        "kovee_placement_ref": rows::str_of(row, "kovee_placement_ref"),
        "kovee_placement_revision": rows::u64_of(row, "kovee_placement_revision"),
        "verification_status": rows::str_of(row, "verification_status"),
        "placement_admitted": true,
        "external_budget_bridge_ref": rows::str_of(bridge, "bridge_id"),
        "bridge_state": rows::str_of(bridge, "state"),
        "episode_queued": true,
        "admitted_at": rows::str_of(row, "admitted_at"),
        "digest": rows::json_of(row, "digest"),
    })
}

// ================================================ the episode lease ======

/// The state one protected per-attempt command resolves: the Episode,
/// its ONE lease head, and the committed `ByomEpisodeBinding` whose DUAL
/// fences the command had to present.
pub struct Protected {
    pub episode: Map<String, Value>,
    pub lease: Map<String, Value>,
    pub binding: Map<String, Value>,
    pub society_id: String,
}

/// Every protected command names the exact Episode, generation, attempt,
/// Byom fence, and expected lease revision (§11.2) — and, per family
/// contract L21, the Kovee invocation fence too. Both are compared here;
/// each staleness answers with its own detail, so a mutation carrying one
/// current fence and one stale fence is refused for the exact reason.
pub fn protected(
    conn: &Connection,
    episode_ref: &str,
    generation: u64,
    attempt_ref: &str,
    byom_fence: u64,
    kovee_fence: u64,
) -> Result<Protected, Problem> {
    let episode = rows::get_row(conn, "episodes", "episode_id", episode_ref)
        .map_err(db_err)?
        .ok_or_else(state::not_found)?;
    if rows::u64_of(&episode, "generation") != generation {
        return Err(state::stale_binding("stale activity generation fence"));
    }
    let lease = rows::get_row(conn, "episode_lease_heads", "episode_id", episode_ref)
        .map_err(db_err)?
        .ok_or_else(|| stale_lease("the Episode holds no lease"))?;
    if rows::str_of(&lease, "current_attempt_ref") != attempt_ref {
        return Err(stale_lease(
            "byom_attempt_ref is not the lease head's current attempt: the old worker is stale",
        ));
    }
    if rows::u64_of(&lease, "byom_fence_epoch") != byom_fence {
        return Err(stale_lease(
            "stale byom_fence_epoch: a superseded attempt cannot submit a delivery, consume a \
             mandate, create child work, append a continuation, or settle usage (§11.2)",
        ));
    }
    let binding = head_row(
        conn,
        "byom_episode_bindings",
        "episode_ref",
        episode_ref,
        "byom_attempt_ref",
        attempt_ref,
    )?
    .ok_or_else(|| stale_lease("no ByomEpisodeBinding binds this attempt"))?;
    if rows::str_of(&binding, "state") != "bound" {
        return Err(stale_lease(
            "the ByomEpisodeBinding is fenced or released; a successor attempt gets a NEW \
             binding row",
        ));
    }
    if rows::u64_of(&binding, "kovee_invocation_fence") != kovee_fence {
        return Err(stale_lease(
            "stale kovee_invocation_fence: a mutation carrying only ONE of the DUAL fences is \
             invalid (family contract L21)",
        ));
    }
    Ok(Protected {
        society_id: rows::str_of(&episode, "society_id").to_owned(),
        episode,
        lease,
        binding,
    })
}

/// The `server_time` sweep (§14.8 named server transition), run as its
/// OWN authority transaction before a claim so the model's two steps stay
/// two steps: `lease_leased -> lease_expired` when the AUTHORITATIVE
/// clock has strictly passed the deadline minted at claim, and
/// `running -> ambiguous` when a running Episode's lease deadline passed
/// with unknown external use. Expiry never deletes the head or reuses a
/// fence. Nothing here observes worker liveness: a crash or silence
/// sweeps nothing.
fn sweep_server_time(
    store: &mut Store,
    episode_ref: &str,
    meta: &bpp_core::envelope::MutationMeta,
    body: &Value,
    now: i64,
) -> Result<(), Problem> {
    let Some(lease) = rows::get_row(
        store.conn(),
        "episode_lease_heads",
        "episode_id",
        episode_ref,
    )
    .map_err(db_err)?
    else {
        return Ok(());
    };
    let deadline = rows::u64_of(&lease, "expires_at_unix") as i64;
    if now <= deadline {
        // The clock has NOT passed the deadline: there is nothing to
        // sweep, and no other fact can make the head reclaimable.
        return Ok(());
    }
    let state = rows::str_of(&lease, "state").to_owned();
    if !matches!(state.as_str(), "lease_leased" | "lease_running") {
        return Ok(());
    }
    let society_id = rows::str_of(&lease, "society_id").to_owned();
    let expiry_event = mint(store, "evt")?;
    let ambiguous_event = mint(store, "evt")?;
    let hooks = crate::dispatch::internal_hooks("server_time");
    let scope = MutationScope {
        society_id: society_id.clone(),
        operation: "server_time".into(),
        actor: crate::gov_ops::ACTOR_SERVER.into(),
        meta: meta.clone(),
        body: body.clone(),
    };
    let episode_c = episode_ref.to_owned();
    let meta_c = meta.clone();
    run(store, scope, now, hooks, move |conn, _| {
        let lease = rows::get_row(conn, "episode_lease_heads", "episode_id", &episode_c)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        let episode = rows::get_row(conn, "episodes", "episode_id", &episode_c)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        let deadline = rows::u64_of(&lease, "expires_at_unix") as i64;
        let mut effects = Vec::new();
        let mut events = Vec::new();
        let mut lease_state = rows::str_of(&lease, "state").to_owned();
        if now > deadline && lease_state == "lease_leased" {
            let revision = rows::u64_of(&lease, "revision") + 1;
            let mut expired = lease.clone();
            expired.insert("state".into(), json!("lease_expired"));
            expired.insert("revision".into(), json!(revision));
            expired.insert(
                "expiry_count".into(),
                json!(rows::u64_of(&lease, "expiry_count") + 1),
            );
            effects.push(Effect::Upsert {
                table: "episode_lease_heads".into(),
                row: expired,
            });
            lease_state = "lease_expired".to_owned();
            events.push(event(
                &society_id,
                &expiry_event,
                "episode-lease.expired",
                &episode_c,
                revision,
                rows::str_of(&episode, "participant_ref"),
                crate::gov_ops::ACTOR_SERVER,
                &meta_c,
                json!({"state": "lease_expired",
                       "via": "server_time (authoritative clock strictly past the deadline)",
                       "byom_fence_epoch": rows::u64_of(&lease, "byom_fence_epoch"),
                       "head_retained": true}),
            ));
        }
        if now > deadline
            && lease_state == "lease_running"
            && rows::str_of(&episode, "state") == "running"
        {
            let revision = rows::u64_of(&episode, "revision") + 1;
            let mut amb = episode.clone();
            amb.insert("state".into(), json!("ambiguous"));
            amb.insert("revision".into(), json!(revision));
            effects.push(Effect::Upsert {
                table: "episodes".into(),
                row: amb,
            });
            events.push(event(
                &society_id,
                &ambiguous_event,
                "episode.ambiguous",
                &episode_c,
                revision,
                rows::str_of(&episode, "participant_ref"),
                crate::gov_ops::ACTOR_SERVER,
                &meta_c,
                json!({"state": "ambiguous",
                       "via": "server_time (lease deadline passed with unknown external use)",
                       "settlement": "conservative; never blindly repeated"}),
            ));
        }
        Ok(Prepared {
            result: json!({"episode_id": episode_c, "lease_state": lease_state,
                           "swept": !effects.is_empty()}),
            revision: Some(rows::u64_of(&lease, "revision")),
            cursor: CursorMint::AfterEvents {
                society_id: society_id.clone(),
            },
            effects,
            events,
        })
    })?;
    Ok(())
}

/// `episode_claim` (runtime, create; R30): compare-and-swap on the ONE
/// EpisodeLeaseHead. A successful claim increments the Byom fence, mints
/// one immutable EpisodeAttempt, and commits the C2
/// `ByomEpisodeBinding` in the SAME transaction (family contract L19–L22:
/// idempotent over `stable_binding_key`). A live leased head is not
/// stealable; only an authoritatively EXPIRED or voluntarily YIELDED head
/// is re-claimable.
pub fn episode_claim(
    store: &mut Store,
    token: &str,
    req: &ops::EpisodeClaimRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    verify_runtime_token(
        store,
        token,
        RuntimeChannel::Worker,
        &episode_subject(&req.episode_ref, req.generation),
    )?;
    let episode = rows::get_row(store.conn(), "episodes", "episode_id", &req.episode_ref)
        .map_err(db_err)?
        .ok_or_else(state::not_found)?;
    let society_id = rows::str_of(&episode, "society_id").to_owned();
    check_meta_binding(store, &req.meta, &society_id)?;
    // Two model steps stay two steps: the authoritative-clock sweep
    // commits first, then the claim reads the swept head.
    sweep_server_time(store, &req.episode_ref, &req.meta, body, now)?;

    let attempt_id = mint(store, "att")?;
    let binding_id = mint(store, "bind")?;
    let attempt_event_id = mint(store, "aev")?;
    let claim_event = mint(store, "evt")?;
    let acquired_at = rfc3339_utc(now);
    let expires_at_unix = now + req.lease_ttl_seconds as i64;
    let expires_at = rfc3339_utc(expires_at_unix);
    let incarnation = store
        .incarnation()
        .map_err(|e| state::internal(&e.to_string()))?;
    let recovery_epoch = store
        .recovery_epoch(&society_id)
        .map_err(|e| state::internal(&e.to_string()))?;
    let byom_endpoint_ref = match crate::host_config::HostConfig::load(store) {
        Ok(cfg) => cfg.realm_byom_binding.byom_endpoint_ref.clone(),
        Err(_) => "byom-endpoint-local".to_owned(),
    };
    let scope = MutationScope {
        society_id: society_id.clone(),
        operation: "episode_claim".into(),
        actor: format!("runtime:{}", req.holder_runtime_binding),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let req_c = req.clone();
    let society_c = society_id.clone();
    let reply = run(store, scope, now, hooks, move |conn, _| {
        let episode = rows::get_row(conn, "episodes", "episode_id", &req_c.episode_ref)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        if rows::u64_of(&episode, "generation") != req_c.generation {
            return Err(state::stale_binding("stale activity generation fence"));
        }
        // L22 idempotent create at the claim CAS: the exact retry returns
        // the identical binding row (and therefore the identical claim).
        if let Some(existing) = rows::get_row(
            conn,
            "byom_episode_bindings",
            "stable_binding_key",
            &req_c.stable_binding_key,
        )
        .map_err(db_err)?
        {
            let lease = rows::get_row(
                conn,
                "episode_lease_heads",
                "episode_id",
                &req_c.episode_ref,
            )
            .map_err(db_err)?
            .ok_or_else(|| state::internal("bound attempt without a lease head"))?;
            return Ok(Prepared {
                result: claim_result(&lease, &existing),
                revision: Some(rows::u64_of(&lease, "revision")),
                cursor: CursorMint::AfterEvents {
                    society_id: society_c.clone(),
                },
                effects: Vec::new(),
                events: Vec::new(),
            });
        }
        let episode_state = rows::str_of(&episode, "state").to_owned();
        if episode_state == "ambiguous" {
            return Err(ambiguous(
                "the Episode is ambiguous (unknown external use): it is never blindly repeated \
                 (§11.2/§14.8)",
            ));
        }
        let head = rows::get_row(
            conn,
            "episode_lease_heads",
            "episode_id",
            &req_c.episode_ref,
        )
        .map_err(db_err)?;
        // Only a QUEUED Episode can be claimed for the first time —
        // nothing queues without both exact reservation sets and an
        // admitted placement (§11.1/§11.4).
        match &head {
            None => {
                if episode_state != "queued" {
                    return Err(stage_required(&format!(
                        "the Episode is {episode_state}, not queued: eligibility comes from the \
                         four activation stages, never from arrival, attention ranking, a host \
                         cron, or a model score (§11.1/§11.2)"
                    )));
                }
            }
            Some(head) => match rows::str_of(head, "state") {
                // The ONLY two re-claimable heads (D-RT-6).
                "lease_expired" | "lease_yielding" => {}
                "lease_leased" | "lease_running" | "lease_completing" => {
                    return Err(stale_lease(
                        "a live lease head is not stealable: reclaim needs the authoritative \
                         clock to pass the deadline minted at claim (a crash or silence enables \
                         nothing)",
                    ))
                }
                _ => return Err(stale_lease("the Episode lease is terminal")),
            },
        }

        let prior_fence = head
            .as_ref()
            .map(|h| rows::u64_of(h, "byom_fence_epoch"))
            .unwrap_or(0);
        let fence = prior_fence + 1;
        let claim_ordinal = head
            .as_ref()
            .map(|h| rows::u64_of(h, "attempt_count"))
            .unwrap_or(0)
            + 1;
        let lease_revision = head
            .as_ref()
            .map(|h| rows::u64_of(h, "revision"))
            .unwrap_or(0)
            + 1;

        let mut effects = Vec::new();
        let mut events = Vec::new();

        // Either DUAL fence advancing invalidates every prior binding
        // for every further mutation (L21): the prior rows go `fenced`
        // and are retained for audit and orphan-result diagnostics.
        for prior in rows::rows_where(
            conn,
            "byom_episode_bindings",
            "episode_ref",
            &req_c.episode_ref,
            "binding_id",
        )
        .map_err(db_err)?
        {
            if rows::str_of(&prior, "state") == "bound" {
                let mut fenced = prior.clone();
                fenced.insert("state".into(), json!("fenced"));
                effects.push(Effect::Upsert {
                    table: "byom_episode_bindings".into(),
                    row: fenced,
                });
            }
        }

        let attempt_record = json!({
            "attempt_id": attempt_id,
            "episode_id": req_c.episode_ref,
            "generation": req_c.generation,
            "claim_ordinal": claim_ordinal,
            "holder_runtime_binding": req_c.holder_runtime_binding,
            "byom_fence_epoch": fence,
            "acquired_at": acquired_at,
            "initial_expires_at": expires_at,
        });
        let attempt_digest = record_digest(
            conn,
            &society_c,
            &attempt_id,
            "bpp-episode-attempt-v0",
            &attempt_record,
        )?;
        effects.push(Effect::Upsert {
            table: "episode_attempts".into(),
            row: obj_pairs([
                ("attempt_id", json!(attempt_id)),
                ("society_id", json!(society_c)),
                ("episode_id", json!(req_c.episode_ref)),
                ("generation", json!(req_c.generation)),
                ("claim_ordinal", json!(claim_ordinal)),
                (
                    "holder_runtime_binding",
                    json!(req_c.holder_runtime_binding),
                ),
                (
                    "manifestation_ref",
                    json!(rows::str_of(&episode, "manifestation_ref")),
                ),
                ("byom_fence_epoch", json!(fence)),
                ("acquired_at", json!(acquired_at)),
                ("initial_expires_at", json!(expires_at)),
                ("kovee_invocation_ref", json!(req_c.kovee_invocation_ref)),
                ("kovee_attempt_ref", Value::Null),
                ("kovee_fence_digest", Value::Null),
                (
                    "claim_subject_digest",
                    digest_json(&req_c.claim_subject_digest),
                ),
                ("created_at", json!(acquired_at)),
                ("digest", digest_json(&attempt_digest)),
            ]),
        });

        // The C2 ByomEpisodeBinding, field-verbatim.
        let participant_ref = rows::str_of(&episode, "participant_ref").to_owned();
        let participant = rows::get_participant(conn, &participant_ref)
            .map_err(db_err)?
            .ok_or_else(state::not_found)?;
        let allocation_id = rows::str_of(&episode, "resource_allocation_ref").to_owned();
        let allocation = rows::get_row(
            conn,
            "resource_allocations",
            "allocation_id",
            &allocation_id,
        )
        .map_err(db_err)?
        .ok_or_else(|| stage_required("the ResourceAllocation is gone"))?;
        let bridge_id = rows::str_of(&allocation, "external_budget_bridge_ref").to_owned();
        let bridge = rows::get_row(conn, "external_budget_bridges", "bridge_id", &bridge_id)
            .map_err(db_err)?
            .ok_or_else(|| stage_required("the ExternalBudgetBridge is gone"))?;
        let set_ref = rows::str_of(&allocation, "byom_budget_reservation_set_ref").to_owned();
        let set_digest = rows::json_of(&bridge, "byom_reservation_set_digest");
        let mut record = Map::new();
        record.insert("byom_endpoint_ref".into(), json!(byom_endpoint_ref));
        record.insert("endpoint_incarnation".into(), json!(incarnation));
        record.insert("society_ref".into(), json!(society_c));
        record.insert("recovery_epoch".into(), json!(recovery_epoch));
        record.insert("participant_ref".into(), json!(participant_ref));
        record.insert(
            "participant_binding_epoch".into(),
            json!(participant.binding_epoch),
        );
        record.insert(
            "manifestation_ref".into(),
            json!(rows::str_of(&episode, "manifestation_ref")),
        );
        record.insert(
            "activity_stream_ref".into(),
            json!(rows::str_of(&episode, "activity_stream_ref")),
        );
        record.insert("episode_ref".into(), json!(req_c.episode_ref));
        record.insert("generation".into(), json!(req_c.generation));
        record.insert("byom_attempt_ref".into(), json!(attempt_id));
        record.insert("byom_fence_epoch".into(), json!(fence));
        record.insert(
            "kovee_invocation_ref".into(),
            json!(req_c.kovee_invocation_ref),
        );
        record.insert(
            "kovee_invocation_fence".into(),
            json!(req_c.kovee_invocation_fence),
        );
        record.insert(
            "mandate_use_refs".into(),
            json!(req_c.mandate_use_refs.clone()),
        );
        record.insert(
            "context_source_digest".into(),
            digest_json(&req_c.context_source_digest),
        );
        record.insert("byom_budget_reservation_ref".into(), json!(set_ref));
        record.insert("byom_budget_reservation_digest".into(), set_digest);
        record.insert("external_budget_bridge_ref".into(), json!(bridge_id));
        record.insert(
            "kovee_subordinate_reservation_ref".into(),
            json!(rows::str_of(&bridge, "subordinate_reservation_ref")),
        );
        record.insert(
            "kovee_subordinate_reservation_digest".into(),
            rows::json_of(&bridge, "subordinate_reservation_digest"),
        );
        record.insert(
            "dependency_digest".into(),
            rows::json_of(&allocation, "dependency_digest"),
        );
        record.insert("stable_binding_key".into(), json!(req_c.stable_binding_key));
        record.insert(
            "allowed_local_commitments".into(),
            json!(req_c.allowed_local_commitments.clone()),
        );
        record.insert(
            "context_manifest_ref".into(),
            json!(req_c.context_manifest_ref),
        );
        record.insert(
            "context_manifest_digest".into(),
            digest_json(&req_c.context_manifest_digest),
        );
        if let (Some(r), Some(d)) = (
            &req_c.kovee_context_assembly_ref,
            &req_c.kovee_context_assembly_digest,
        ) {
            record.insert("kovee_context_assembly_ref".into(), json!(r));
            record.insert("kovee_context_assembly_digest".into(), digest_json(d));
        }
        if let (Some(r), Some(d)) = (
            &req_c.provider_context_manifest_ref,
            &req_c.provider_context_manifest_digest,
        ) {
            record.insert("provider_context_manifest_ref".into(), json!(r));
            record.insert("provider_context_manifest_digest".into(), digest_json(d));
        }
        let binding_digest = record_digest(
            conn,
            &society_c,
            &binding_id,
            "bpp-byom-episode-binding-v0",
            &Value::Object(record.clone()),
        )?;
        record.insert("digest".into(), digest_json(&binding_digest));
        let binding_row = obj_pairs([
            ("binding_id", json!(binding_id)),
            ("society_id", json!(society_c)),
            ("stable_binding_key", json!(req_c.stable_binding_key)),
            ("episode_ref", json!(req_c.episode_ref)),
            ("byom_attempt_ref", json!(attempt_id)),
            ("kovee_invocation_ref", json!(req_c.kovee_invocation_ref)),
            ("byom_fence_epoch", json!(fence)),
            (
                "kovee_invocation_fence",
                json!(req_c.kovee_invocation_fence),
            ),
            ("record", json_text(&Value::Object(record))),
            ("state", json!("bound")),
            ("created_at", json!(acquired_at)),
            ("digest", digest_json(&binding_digest)),
        ]);
        effects.push(Effect::Upsert {
            table: "byom_episode_bindings".into(),
            row: binding_row.clone(),
        });

        let head_row_new = obj_pairs([
            ("episode_id", json!(req_c.episode_ref)),
            ("society_id", json!(society_c)),
            ("generation", json!(req_c.generation)),
            ("revision", json!(lease_revision)),
            ("current_attempt_ref", json!(attempt_id)),
            (
                "holder_runtime_binding",
                json!(req_c.holder_runtime_binding),
            ),
            ("byom_fence_epoch", json!(fence)),
            ("renewed_at", json!(acquired_at)),
            ("expires_at", json!(expires_at)),
            ("expires_at_unix", json!(expires_at_unix)),
            ("state", json!("lease_leased")),
            ("last_attempt_event_ref", json!(attempt_event_id)),
            ("attempt_count", json!(claim_ordinal)),
            (
                "expiry_count",
                json!(head
                    .as_ref()
                    .map(|h| rows::u64_of(h, "expiry_count"))
                    .unwrap_or(0)),
            ),
            (
                "yield_count",
                json!(head
                    .as_ref()
                    .map(|h| rows::u64_of(h, "yield_count"))
                    .unwrap_or(0)),
            ),
        ]);
        effects.push(Effect::Upsert {
            table: "episode_lease_heads".into(),
            row: head_row_new.clone(),
        });
        effects.push(attempt_event_effect(
            conn,
            &society_c,
            &attempt_event_id,
            &req_c.episode_ref,
            &attempt_id,
            lease_revision,
            fence,
            "claimed",
            &attempt_record,
            now,
        )?);
        events.push(event(
            &society_c,
            &claim_event,
            "episode-lease.claimed",
            &req_c.episode_ref,
            lease_revision,
            &participant_ref,
            &format!("runtime:{}", req_c.holder_runtime_binding),
            &req_c.meta,
            json!({"state": "lease_leased", "byom_fence_epoch": fence,
                   "claim_ordinal": claim_ordinal,
                   "kovee_invocation_fence": req_c.kovee_invocation_fence,
                   "expires_at": expires_at,
                   "byom_episode_binding_ref": binding_id}),
        ));
        Ok(Prepared {
            result: claim_result(&head_row_new, &binding_row),
            revision: Some(lease_revision),
            cursor: CursorMint::AfterEvents {
                society_id: society_c.clone(),
            },
            effects,
            events,
        })
    })?;
    ensure_runtime_token_files(store);
    Ok(reply)
}

#[allow(clippy::too_many_arguments)]
fn attempt_event_effect(
    conn: &Connection,
    society_id: &str,
    event_id: &str,
    episode_ref: &str,
    attempt_ref: &str,
    expected_lease_revision: u64,
    fence: u64,
    kind: &str,
    payload: &Value,
    now: i64,
) -> Result<Effect, Problem> {
    let payload_digest = record_digest(
        conn,
        society_id,
        event_id,
        "bpp-episode-attempt-event-v0",
        payload,
    )?;
    let record = json!({
        "event_id": event_id,
        "attempt_ref": attempt_ref,
        "expected_lease_revision": expected_lease_revision,
        "byom_fence_epoch": fence,
        "kind": kind,
        "payload_digest": digest_json(&payload_digest),
    });
    let digest = record_digest(
        conn,
        society_id,
        &format!("{event_id}-self"),
        "bpp-episode-attempt-event-v0",
        &record,
    )?;
    Ok(Effect::Upsert {
        table: "episode_attempt_events".into(),
        row: obj_pairs([
            ("event_id", json!(event_id)),
            ("society_id", json!(society_id)),
            ("episode_id", json!(episode_ref)),
            ("attempt_ref", json!(attempt_ref)),
            ("expected_lease_revision", json!(expected_lease_revision)),
            ("byom_fence_epoch", json!(fence)),
            ("kind", json!(kind)),
            ("payload_digest", digest_json(&payload_digest)),
            ("occurred_at", json!(rfc3339_utc(now))),
            ("digest", digest_json(&digest)),
        ]),
    })
}

fn claim_result(lease: &Map<String, Value>, binding: &Map<String, Value>) -> Value {
    json!({
        "episode_id": rows::str_of(lease, "episode_id"),
        "generation": rows::u64_of(lease, "generation"),
        "lease_revision": rows::u64_of(lease, "revision"),
        "lease_state": rows::str_of(lease, "state"),
        "byom_attempt_ref": rows::str_of(lease, "current_attempt_ref"),
        "byom_fence_epoch": rows::u64_of(lease, "byom_fence_epoch"),
        "claim_ordinal": rows::u64_of(lease, "attempt_count"),
        "expires_at": rows::str_of(lease, "expires_at"),
        "byom_episode_binding_ref": rows::str_of(binding, "binding_id"),
        "byom_episode_binding": rows::json_of(binding, "record"),
        "kovee_invocation_fence": rows::u64_of(binding, "kovee_invocation_fence"),
    })
}

// ================================== protected per-attempt transitions ====

/// The shared shape of the five protected lease transitions.
struct LeaseStep {
    /// Lease head states the transition may leave.
    from_lease: &'static [&'static str],
    /// Episode states the transition may leave (empty = any).
    from_episode: &'static [&'static str],
    to_lease: &'static str,
    attempt_event_kind: &'static str,
}

/// `episode_start` (runtime, update; R30): only the current holder under
/// both current fences, CASing the one lease head.
pub fn episode_start(
    store: &mut Store,
    token: &str,
    req: &ops::EpisodeStartRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    let step = LeaseStep {
        from_lease: &["lease_leased"],
        from_episode: &["queued"],
        to_lease: "lease_running",
        attempt_event_kind: "started",
    };
    lease_transition(
        store,
        token,
        "episode_start",
        &req.episode_ref,
        req.generation,
        &req.byom_attempt_ref,
        req.byom_fence_epoch,
        req.kovee_invocation_fence,
        &req.meta,
        body,
        now,
        hooks,
        step,
        "running",
        json!({}),
    )
}

/// `episode_yield` (runtime, update; R30).
pub fn episode_yield(
    store: &mut Store,
    token: &str,
    req: &ops::EpisodeYieldRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    let step = LeaseStep {
        from_lease: &["lease_running"],
        from_episode: &["running"],
        to_lease: "lease_yielding",
        attempt_event_kind: "yielded",
    };
    lease_transition(
        store,
        token,
        "episode_yield",
        &req.episode_ref,
        req.generation,
        &req.byom_attempt_ref,
        req.byom_fence_epoch,
        req.kovee_invocation_fence,
        &req.meta,
        body,
        now,
        hooks,
        step,
        &req.target_state.clone(),
        json!({"reason_ref": req.reason_ref}),
    )
}

/// `checkpoint_commit` (runtime, create; R30): one immutable
/// EpisodeAttemptEvent; the Episode and the lease head keep their state.
pub fn checkpoint_commit(
    store: &mut Store,
    token: &str,
    req: &ops::CheckpointCommitRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    verify_runtime_token(
        store,
        token,
        RuntimeChannel::Worker,
        &episode_subject(&req.episode_ref, req.generation),
    )?;
    let society_id = society_of_episode(store, &req.episode_ref)?;
    check_meta_binding(store, &req.meta, &society_id)?;
    let attempt_event_id = mint(store, "aev")?;
    let commit_event = mint(store, "evt")?;
    let scope = MutationScope {
        society_id: society_id.clone(),
        operation: "checkpoint_commit".into(),
        actor: format!("runtime:{}", req.byom_attempt_ref),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let req_c = req.clone();
    let society_c = society_id.clone();
    run(store, scope, now, hooks, move |conn, _| {
        let p = protected(
            conn,
            &req_c.episode_ref,
            req_c.generation,
            &req_c.byom_attempt_ref,
            req_c.byom_fence_epoch,
            req_c.kovee_invocation_fence,
        )?;
        if rows::str_of(&p.lease, "state") != "lease_running" {
            return Err(stale_lease("checkpoints commit only under a running lease"));
        }
        if rows::u64_of(&p.lease, "revision") != req_c.expected_lease_revision {
            return Err(state::stale_revision());
        }
        let payload = json!({
            "checkpoint_ref": req_c.checkpoint_ref,
            "checkpoint_digest": digest_json(&req_c.checkpoint_digest),
        });
        let mut lease = p.lease.clone();
        let revision = rows::u64_of(&p.lease, "revision") + 1;
        lease.insert("revision".into(), json!(revision));
        lease.insert("renewed_at".into(), json!(rfc3339_utc(now)));
        lease.insert("last_attempt_event_ref".into(), json!(attempt_event_id));
        let effects = vec![
            attempt_event_effect(
                conn,
                &society_c,
                &attempt_event_id,
                &req_c.episode_ref,
                &req_c.byom_attempt_ref,
                req_c.expected_lease_revision,
                req_c.byom_fence_epoch,
                "checkpoint",
                &payload,
                now,
            )?,
            Effect::Upsert {
                table: "episode_lease_heads".into(),
                row: lease,
            },
        ];
        Ok(Prepared {
            result: json!({
                "episode_id": req_c.episode_ref,
                "attempt_event_ref": attempt_event_id,
                "lease_revision": revision,
                "byom_fence_epoch": req_c.byom_fence_epoch,
                "kovee_invocation_fence": req_c.kovee_invocation_fence,
                "checkpoint_ref": req_c.checkpoint_ref,
            }),
            revision: Some(revision),
            cursor: CursorMint::AfterEvents {
                society_id: society_c.clone(),
            },
            effects,
            events: vec![event(
                &society_c,
                &commit_event,
                "episode-attempt.checkpoint",
                &req_c.episode_ref,
                revision,
                rows::str_of(&p.episode, "participant_ref"),
                &format!("runtime:{}", req_c.byom_attempt_ref),
                &req_c.meta,
                json!({"checkpoint_ref": req_c.checkpoint_ref,
                       "byom_fence_epoch": req_c.byom_fence_epoch,
                       "kovee_invocation_fence": req_c.kovee_invocation_fence}),
            )],
        })
    })
}

fn society_of_episode(store: &Store, episode_ref: &str) -> Result<String, Problem> {
    let episode = rows::get_row(store.conn(), "episodes", "episode_id", episode_ref)
        .map_err(db_err)?
        .ok_or_else(state::not_found)?;
    Ok(rows::str_of(&episode, "society_id").to_owned())
}

/// The shared driver of `episode_start` and `episode_yield`: guard both
/// fences, CAS the lease head, move the Episode, cascade the
/// ActivityStream, and append one immutable EpisodeAttemptEvent.
#[allow(clippy::too_many_arguments)]
fn lease_transition(
    store: &mut Store,
    token: &str,
    operation: &str,
    episode_ref: &str,
    generation: u64,
    attempt_ref: &str,
    byom_fence: u64,
    kovee_fence: u64,
    meta: &bpp_core::envelope::MutationMeta,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
    step: LeaseStep,
    to_episode: &str,
    payload_extra: Value,
) -> Result<Vec<u8>, Problem> {
    verify_runtime_token(
        store,
        token,
        RuntimeChannel::Worker,
        &episode_subject(episode_ref, generation),
    )?;
    let society_id = society_of_episode(store, episode_ref)?;
    check_meta_binding(store, meta, &society_id)?;
    let attempt_event_id = mint(store, "aev")?;
    let move_event = mint(store, "evt")?;
    let scope = MutationScope {
        society_id: society_id.clone(),
        operation: operation.to_owned(),
        actor: format!("runtime:{attempt_ref}"),
        meta: meta.clone(),
        body: body.clone(),
    };
    let episode_c = episode_ref.to_owned();
    let attempt_c = attempt_ref.to_owned();
    let meta_c = meta.clone();
    let society_c = society_id.clone();
    let to_episode_c = to_episode.to_owned();
    let operation_c = operation.to_owned();
    let reply = run(store, scope, now, hooks, move |conn, _| {
        let p = protected(
            conn,
            &episode_c,
            generation,
            &attempt_c,
            byom_fence,
            kovee_fence,
        )?;
        let lease_state = rows::str_of(&p.lease, "state");
        if !step.from_lease.contains(&lease_state) {
            return Err(stale_lease(&format!(
                "the lease head is {lease_state}; {operation_c} needs one of {:?}",
                step.from_lease
            )));
        }
        let episode_state = rows::str_of(&p.episode, "state");
        if !step.from_episode.is_empty() && !step.from_episode.contains(&episode_state) {
            return Err(state::stale_binding(&format!(
                "the Episode is {episode_state}; {operation_c} needs one of {:?}",
                step.from_episode
            )));
        }
        if meta_c.expected_revision != Some(rows::u64_of(&p.lease, "revision")) {
            return Err(state::stale_revision());
        }
        let lease_revision = rows::u64_of(&p.lease, "revision") + 1;
        let episode_revision = rows::u64_of(&p.episode, "revision") + 1;
        let mut lease = p.lease.clone();
        lease.insert("state".into(), json!(step.to_lease));
        lease.insert("revision".into(), json!(lease_revision));
        lease.insert("renewed_at".into(), json!(rfc3339_utc(now)));
        lease.insert("last_attempt_event_ref".into(), json!(attempt_event_id));
        if step.to_lease == "lease_yielding" {
            lease.insert(
                "yield_count".into(),
                json!(rows::u64_of(&p.lease, "yield_count") + 1),
            );
        }
        let mut episode = p.episode.clone();
        episode.insert("state".into(), json!(to_episode_c));
        episode.insert("revision".into(), json!(episode_revision));
        let mut effects = vec![
            Effect::Upsert {
                table: "episode_lease_heads".into(),
                row: lease,
            },
            Effect::Upsert {
                table: "episodes".into(),
                row: episode,
            },
            attempt_event_effect(
                conn,
                &society_c,
                &attempt_event_id,
                &episode_c,
                &attempt_c,
                rows::u64_of(&p.lease, "revision"),
                byom_fence,
                step.attempt_event_kind,
                &payload_extra,
                now,
            )?,
        ];
        // The ActivityStream cascade (§14.8): active -> waiting on yield.
        if step.to_lease == "lease_yielding" {
            let stream_ref = rows::str_of(&p.episode, "activity_stream_ref").to_owned();
            if let Some(stream) =
                rows::get_row(conn, "activity_streams", "activity_stream_id", &stream_ref)
                    .map_err(db_err)?
            {
                if rows::str_of(&stream, "state") == "active" {
                    let mut waiting = stream.clone();
                    waiting.insert("state".into(), json!("waiting"));
                    waiting.insert(
                        "revision".into(),
                        json!(rows::u64_of(&stream, "revision") + 1),
                    );
                    effects.push(Effect::Upsert {
                        table: "activity_streams".into(),
                        row: waiting,
                    });
                }
            }
        }
        Ok(Prepared {
            result: json!({
                "episode_id": episode_c,
                "generation": generation,
                "state": to_episode_c,
                "lease_state": step.to_lease,
                "lease_revision": lease_revision,
                "byom_attempt_ref": attempt_c,
                "byom_fence_epoch": byom_fence,
                "kovee_invocation_fence": kovee_fence,
                "attempt_event_ref": attempt_event_id,
            }),
            revision: Some(lease_revision),
            cursor: CursorMint::AfterEvents {
                society_id: society_c.clone(),
            },
            effects,
            events: vec![event(
                &society_c,
                &move_event,
                &format!("episode.{to_episode_c}"),
                &episode_c,
                episode_revision,
                rows::str_of(&p.episode, "participant_ref"),
                &format!("runtime:{attempt_c}"),
                &meta_c,
                json!({"state": to_episode_c, "lease_state": step.to_lease,
                       "byom_fence_epoch": byom_fence,
                       "kovee_invocation_fence": kovee_fence}),
            )],
        })
    })?;
    ensure_runtime_token_files(store);
    Ok(reply)
}

// ============================================= terminalization + budget ==

/// `episode_complete` (runtime, update; R30). Completion is EVIDENCE
/// only: it never authors a Delivery. In the same transaction the lease
/// head settles terminal, the binding is released, and the §11.4 bridge
/// hands off to settlement — measured when a trusted meter has settled,
/// otherwise to the CONSERVATIVE MAXIMUM (§11.4: unknown or underivable
/// cost keeps the reservation or settles to the conservative maximum).
pub fn episode_complete(
    store: &mut Store,
    token: &str,
    req: &ops::EpisodeCompleteRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    terminalize(
        store,
        token,
        "episode_complete",
        Terminal {
            episode_ref: req.episode_ref.clone(),
            generation: req.generation,
            attempt_ref: req.byom_attempt_ref.clone(),
            byom_fence: req.byom_fence_epoch,
            kovee_fence: req.kovee_invocation_fence,
            outcome: "completed".into(),
            output_refs: req.output_refs.clone(),
            evidence_refs: req.evidence_refs.clone(),
            usage_report_refs: req.usage_report_refs.clone(),
            failure_reason_ref: None,
        },
        &req.meta,
        body,
        now,
        hooks,
    )
}

/// `episode_fail` (runtime, update; R30).
pub fn episode_fail(
    store: &mut Store,
    token: &str,
    req: &ops::EpisodeFailRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    terminalize(
        store,
        token,
        "episode_fail",
        Terminal {
            episode_ref: req.episode_ref.clone(),
            generation: req.generation,
            attempt_ref: req.byom_attempt_ref.clone(),
            byom_fence: req.byom_fence_epoch,
            kovee_fence: req.kovee_invocation_fence,
            outcome: "failed".into(),
            output_refs: Vec::new(),
            evidence_refs: req.evidence_refs.clone(),
            usage_report_refs: Vec::new(),
            failure_reason_ref: Some(req.failure_reason_ref.clone()),
        },
        &req.meta,
        body,
        now,
        hooks,
    )
}

struct Terminal {
    episode_ref: String,
    generation: u64,
    attempt_ref: String,
    byom_fence: u64,
    kovee_fence: u64,
    outcome: String,
    output_refs: Vec<String>,
    evidence_refs: Vec<String>,
    usage_report_refs: Vec<String>,
    failure_reason_ref: Option<String>,
}

#[allow(clippy::too_many_arguments)]
fn terminalize(
    store: &mut Store,
    token: &str,
    operation: &str,
    t: Terminal,
    meta: &bpp_core::envelope::MutationMeta,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    verify_runtime_token(
        store,
        token,
        RuntimeChannel::Worker,
        &episode_subject(&t.episode_ref, t.generation),
    )?;
    let society_id = society_of_episode(store, &t.episode_ref)?;
    check_meta_binding(store, meta, &society_id)?;
    let completion_id = mint(store, "cmpl")?;
    let attempt_event_id = mint(store, "aev")?;
    let terminal_event = mint(store, "evt")?;
    let settle_event = mint(store, "evt")?;
    let settlement_id = mint(store, "settle")?;
    let created_at = rfc3339_utc(now);
    let scope = MutationScope {
        society_id: society_id.clone(),
        operation: operation.to_owned(),
        actor: format!("runtime:{}", t.attempt_ref),
        meta: meta.clone(),
        body: body.clone(),
    };
    let meta_c = meta.clone();
    let society_c = society_id.clone();
    let operation_c = operation.to_owned();
    let reply = run(store, scope, now, hooks, move |conn, _| {
        let p = protected(
            conn,
            &t.episode_ref,
            t.generation,
            &t.attempt_ref,
            t.byom_fence,
            t.kovee_fence,
        )?;
        if rows::str_of(&p.lease, "state") != "lease_running" {
            return Err(stale_lease(&format!(
                "{operation_c} needs a running lease head"
            )));
        }
        if rows::str_of(&p.episode, "state") != "running" {
            return Err(state::stale_binding("the Episode is not running"));
        }
        if meta_c.expected_revision != Some(rows::u64_of(&p.lease, "revision")) {
            return Err(state::stale_revision());
        }
        let lease_revision = rows::u64_of(&p.lease, "revision") + 1;
        let episode_revision = rows::u64_of(&p.episode, "revision") + 1;
        let mut effects = Vec::new();
        let mut events = Vec::new();

        // The lease head: running -> completing -> terminal for a
        // completion (both §14.8 transitions, one transaction), and
        // running -> terminal for a failure.
        let mut lease = p.lease.clone();
        lease.insert("state".into(), json!("lease_terminal"));
        lease.insert("revision".into(), json!(lease_revision));
        lease.insert("renewed_at".into(), json!(created_at));
        lease.insert("last_attempt_event_ref".into(), json!(attempt_event_id));
        effects.push(Effect::Upsert {
            table: "episode_lease_heads".into(),
            row: lease,
        });

        let mut episode = p.episode.clone();
        episode.insert("state".into(), json!(t.outcome));
        episode.insert("revision".into(), json!(episode_revision));
        episode.insert("terminal_at".into(), json!(created_at));
        effects.push(Effect::Upsert {
            table: "episodes".into(),
            row: episode,
        });

        // The immutable EpisodeCompletion (evidence).
        let completion_body = json!({
            "completion_id": completion_id,
            "episode_ref": t.episode_ref,
            "attempt_ref": t.attempt_ref,
            "byom_fence_epoch": t.byom_fence,
            "output_refs": t.output_refs,
            "evidence_refs": t.evidence_refs,
            "usage_report_refs": t.usage_report_refs,
            "outcome": t.outcome,
            "failure_reason_ref": t.failure_reason_ref,
            "created_at": created_at,
        });
        let completion_digest = record_digest(
            conn,
            &society_c,
            &completion_id,
            "bpp-episode-completion-v0",
            &completion_body,
        )?;
        effects.push(Effect::Upsert {
            table: "episode_completions".into(),
            row: obj_pairs([
                ("completion_id", json!(completion_id)),
                ("society_id", json!(society_c)),
                ("episode_ref", json!(t.episode_ref)),
                ("attempt_ref", json!(t.attempt_ref)),
                ("byom_fence_epoch", json!(t.byom_fence)),
                (
                    "runtime_binding_ref",
                    json!(rows::str_of(&p.lease, "holder_runtime_binding")),
                ),
                ("output_refs", json_text(&json!(t.output_refs))),
                ("evidence_refs", json_text(&json!(t.evidence_refs))),
                ("usage_report_refs", json_text(&json!(t.usage_report_refs))),
                ("outcome", json!(t.outcome)),
                ("created_at", json!(created_at)),
                ("digest", digest_json(&completion_digest)),
            ]),
        });
        effects.push(attempt_event_effect(
            conn,
            &society_c,
            &attempt_event_id,
            &t.episode_ref,
            &t.attempt_ref,
            rows::u64_of(&p.lease, "revision"),
            t.byom_fence,
            &t.outcome,
            &completion_body,
            now,
        )?);

        // The binding: bound -> released (orderly close; the budget
        // reservation refs hand off to §11.4 settlement).
        let mut binding = p.binding.clone();
        binding.insert("state".into(), json!("released"));
        effects.push(Effect::Upsert {
            table: "byom_episode_bindings".into(),
            row: binding,
        });

        // -- §11.4 settlement hand-off --------------------------------
        let allocation_id = rows::str_of(&p.episode, "resource_allocation_ref").to_owned();
        let allocation = rows::get_row(
            conn,
            "resource_allocations",
            "allocation_id",
            &allocation_id,
        )
        .map_err(db_err)?
        .ok_or_else(|| state::internal("the ResourceAllocation is missing"))?;
        let bridge_id = rows::str_of(&allocation, "external_budget_bridge_ref").to_owned();
        let bridge = rows::get_row(conn, "external_budget_bridges", "bridge_id", &bridge_id)
            .map_err(db_err)?
            .ok_or_else(|| state::internal("the ExternalBudgetBridge is missing"))?;
        let reservation = allocation_reservation(conn, &allocation_id)?;
        let held = rows::u64_of(&reservation, "amount");
        let account = rows::str_of(&reservation, "account_ref").to_owned();
        let bridge_state = rows::str_of(&bridge, "state").to_owned();
        let mut bridge_row = bridge.clone();
        bridge_row.insert(
            "revision".into(),
            json!(rows::u64_of(&bridge, "revision") + 1),
        );
        let mut settlement_note = json!({"bridge_state": bridge_state, "released": 0});
        match bridge_state.as_str() {
            // Never measured: settle to the CONSERVATIVE MAXIMUM, then
            // release nothing (there is no remainder).
            "confirmed" => {
                account_move(
                    conn,
                    &mut effects,
                    &account,
                    part_common::UNIT_DIMENSION,
                    "reserved",
                    "committed",
                    held,
                )?;
                set_reservation(&mut effects, reservation.clone(), held, "committed");
                let record = json!({
                    "settlement_id": settlement_id,
                    "revision": 1,
                    "stable_settlement_key": format!("conservative-{allocation_id}"),
                    "reservation_set_ref":
                        rows::str_of(&allocation, "byom_budget_reservation_set_ref"),
                    "status": "conservatively_maxed",
                    "charged_quantities": [{"dimension": part_common::UNIT_DIMENSION,
                                            "unit": part_common::UNIT_DIMENSION,
                                            "amount": held}],
                    "created_at": created_at,
                });
                let digest = record_digest(
                    conn,
                    &society_c,
                    &settlement_id,
                    "bpp-usage-settlement-v0",
                    &record,
                )?;
                let set_ref =
                    rows::str_of(&allocation, "byom_budget_reservation_set_ref").to_owned();
                let stable = format!("conservative-{allocation_id}");
                effects.push(Effect::Upsert {
                    table: "usage_settlements".into(),
                    row: obj_pairs([
                        ("settlement_id", json!(settlement_id)),
                        ("society_id", json!(society_c)),
                        ("revision", json!(1)),
                        ("previous_settlement_digest", Value::Null),
                        ("stable_settlement_key", json!(stable)),
                        ("reservation_set_ref", json!(set_ref)),
                        ("meter_ref", json!("meter:none")),
                        ("meter_attestation_ref", json!("attestation:none")),
                        ("pricing_revision_ref", Value::Null),
                        ("measured_quantities", json_text(&json!([]))),
                        (
                            "charged_quantities",
                            json_text(&json!([{"dimension": part_common::UNIT_DIMENSION,
                                               "unit": part_common::UNIT_DIMENSION,
                                               "amount": held}])),
                        ),
                        ("status", json!("conservatively_maxed")),
                        ("created_at", json!(created_at)),
                        ("digest", digest_json(&digest)),
                    ]),
                });
                effects.push(Effect::Upsert {
                    table: "usage_settlement_heads".into(),
                    row: obj_pairs([
                        ("reservation_set_ref", json!(set_ref)),
                        ("stable_settlement_key", json!(stable)),
                        ("society_id", json!(society_c)),
                        ("current_settlement_ref", json!(settlement_id)),
                        ("current_settlement_revision", json!(1)),
                        ("current_settlement_digest", digest_json(&digest)),
                        ("revision", json!(1)),
                        ("updated_at", json!(created_at)),
                    ]),
                });
                bridge_row.insert("state".into(), json!("released"));
                bridge_row.insert("settled_charge".into(), json!(held));
                settlement_note = json!({"bridge_state": "released",
                                         "status": "conservatively_maxed",
                                         "charged": held, "released": 0});
                events.push(event(
                    &society_c,
                    &settle_event,
                    "subordinate-reservation.settled",
                    &bridge_id,
                    rows::u64_of(&bridge_row, "revision"),
                    rows::str_of(&p.episode, "participant_ref"),
                    ACTOR_METER,
                    &meta_c,
                    json!({"status": "conservatively_maxed", "charged": held,
                           "reason": "no trusted meter settled this use (§11.4)"}),
                ));
            }
            // Measured: release exactly the reserved remainder above the
            // settled charge; `released_lifetime` is an audit counter,
            // never an available bucket.
            "settled" => {
                if held > 0 {
                    account_move(
                        conn,
                        &mut effects,
                        &account,
                        part_common::UNIT_DIMENSION,
                        "reserved",
                        "remaining",
                        held,
                    )?;
                }
                set_reservation(&mut effects, reservation.clone(), held, "released");
                bridge_row.insert("state".into(), json!("released"));
                settlement_note = json!({"bridge_state": "released",
                                         "status": "measured",
                                         "charged": rows::u64_of(&bridge, "settled_charge"),
                                         "released": held});
                events.push(event(
                    &society_c,
                    &settle_event,
                    "subordinate-reservation.released",
                    &bridge_id,
                    rows::u64_of(&bridge_row, "revision"),
                    rows::str_of(&p.episode, "participant_ref"),
                    ACTOR_METER,
                    &meta_c,
                    json!({"released_remainder": held,
                           "settled_charge": rows::u64_of(&bridge, "settled_charge")}),
                ));
            }
            // An unknown outcome NEVER releases without the R38 seat.
            _ => {
                settlement_note = json!({"bridge_state": bridge_state,
                                         "released": 0,
                                         "spend": "blocked; only budget_reconcile (R38) releases \
                                                   an uncertain bridge"});
            }
        }
        effects.push(Effect::Upsert {
            table: "external_budget_bridges".into(),
            row: bridge_row,
        });
        let mut allocation_row = allocation.clone();
        if matches!(bridge_state.as_str(), "confirmed" | "settled") {
            allocation_row.insert("state".into(), json!("released"));
            allocation_row.insert(
                "revision".into(),
                json!(rows::u64_of(&allocation, "revision") + 1),
            );
        }
        effects.push(Effect::Upsert {
            table: "resource_allocations".into(),
            row: allocation_row,
        });

        // The ActivityStream cascade (§14.8): active -> ready on
        // completion.
        if t.outcome == "completed" {
            let stream_ref = rows::str_of(&p.episode, "activity_stream_ref").to_owned();
            if let Some(stream) =
                rows::get_row(conn, "activity_streams", "activity_stream_id", &stream_ref)
                    .map_err(db_err)?
            {
                if rows::str_of(&stream, "state") == "active" {
                    let mut ready = stream.clone();
                    ready.insert("state".into(), json!("ready"));
                    ready.insert(
                        "revision".into(),
                        json!(rows::u64_of(&stream, "revision") + 1),
                    );
                    effects.push(Effect::Upsert {
                        table: "activity_streams".into(),
                        row: ready,
                    });
                }
            }
        }

        events.push(event(
            &society_c,
            &terminal_event,
            &format!("episode.{}", t.outcome),
            &t.episode_ref,
            episode_revision,
            rows::str_of(&p.episode, "participant_ref"),
            &format!("runtime:{}", t.attempt_ref),
            &meta_c,
            json!({"state": t.outcome, "lease_state": "lease_terminal",
                   "completion_ref": completion_id,
                   "byom_fence_epoch": t.byom_fence,
                   "kovee_invocation_fence": t.kovee_fence,
                   "delivery": "separate and pledgor-authored; completion is evidence only"}),
        ));

        Ok(Prepared {
            result: json!({
                "episode_id": t.episode_ref,
                "generation": t.generation,
                "state": t.outcome,
                "lease_state": "lease_terminal",
                "lease_revision": lease_revision,
                "completion_ref": completion_id,
                "completion_digest": digest_json(&completion_digest),
                "byom_episode_binding_state": "released",
                "settlement": settlement_note,
                "dependency_closure": effect_head_closure(
                    conn, &t.episode_ref)?,
            }),
            revision: Some(lease_revision),
            cursor: CursorMint::AfterEvents {
                society_id: society_c.clone(),
            },
            effects,
            events,
        })
    })?;
    ensure_runtime_token_files(store);
    Ok(reply)
}

/// The §13.2 downstream dependency closure: EVERY materializer and
/// local-consequence consumer checks BOTH current heads and the
/// disposition state, so both enter this closure whenever an Episode's
/// effects are cited.
pub fn effect_head_closure(conn: &Connection, episode_ref: &str) -> Result<Value, Problem> {
    let mut eoa = Vec::new();
    let mut disp = Vec::new();
    for admission in rows::rows_where(
        conn,
        "effect_outcome_admissions",
        "episode_ref",
        episode_ref,
        "admission_id",
    )
    .map_err(db_err)?
    {
        let intent = rows::str_of(&admission, "intent_ref").to_owned();
        let key = rows::str_of(&admission, "stable_execution_key").to_owned();
        if let Some(head) = head_row(
            conn,
            "effect_outcome_admission_heads",
            "intent_ref",
            &intent,
            "stable_execution_key",
            &key,
        )? {
            let entry = json!({
                "intent_ref": intent,
                "stable_execution_key": key,
                "current_admission_ref": rows::str_of(&head, "current_admission_ref"),
                "current_admission_revision": rows::u64_of(&head, "current_admission_revision"),
                "current_admission_digest": rows::json_of(&head, "current_admission_digest"),
                "current_outcome": rows::str_of(&head, "current_outcome"),
                "revision": rows::u64_of(&head, "revision"),
            });
            if !eoa.contains(&entry) {
                eoa.push(entry);
            }
        }
        if let Some(head) = head_row(
            conn,
            "effect_governance_disposition_heads",
            "intent_ref",
            &intent,
            "stable_execution_key",
            &key,
        )? {
            let entry = json!({
                "intent_ref": intent,
                "stable_execution_key": key,
                "current_disposition_ref": rows::str_of(&head, "current_disposition_ref"),
                "current_disposition_revision":
                    rows::u64_of(&head, "current_disposition_revision"),
                "current_disposition_digest": rows::json_of(&head, "current_disposition_digest"),
                "state": rows::str_of(&head, "state"),
                "revision": rows::u64_of(&head, "revision"),
            });
            if !disp.contains(&entry) {
                disp.push(entry);
            }
        }
    }
    Ok(json!({
        "effect_outcome_admission_heads": eoa,
        "effect_governance_disposition_heads": disp,
        "lock_order": ["effect_outcome_admission_head", "effect_governance_disposition_head"],
    }))
}

// ====================================================== usage_report =====

/// `usage_report` (runtime, create; R30). The CHANNEL decides what a
/// report can do: the worker's Episode token may only file EVIDENCE
/// (family contract L33 — participant and worker reports are evidence,
/// not meters), and only the narrow trusted-meter adapter's token may
/// carry the measured settlement, which is monotonic, stable-keyed, and
/// applied once on both sides (§11.4; SubordinateReservation.tla
/// `SettleOnce`, `ChargeWithinReservation`).
pub fn usage_report(
    store: &mut Store,
    token: &str,
    req: &ops::UsageReportRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    let subject = episode_subject(&req.episode_ref, req.generation);
    let channel = if req.source == "trusted_meter" {
        RuntimeChannel::Meter
    } else {
        RuntimeChannel::Worker
    };
    verify_runtime_token(store, token, channel, &subject)?;
    let society_id = society_of_episode(store, &req.episode_ref)?;
    check_meta_binding(store, &req.meta, &society_id)?;
    let report_id = mint(store, "urep")?;
    let settlement_id = mint(store, "settle")?;
    let report_event = mint(store, "evt")?;
    let created_at = rfc3339_utc(now);
    let actor = if channel == RuntimeChannel::Meter {
        ACTOR_METER.to_owned()
    } else {
        format!("runtime:{}", req.byom_attempt_ref)
    };
    let scope = MutationScope {
        society_id: society_id.clone(),
        operation: "usage_report".into(),
        actor: actor.clone(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let req_c = req.clone();
    let society_c = society_id.clone();
    let actor_c = actor.clone();
    run(store, scope, now, hooks, move |conn, _| {
        let p = protected(
            conn,
            &req_c.episode_ref,
            req_c.generation,
            &req_c.byom_attempt_ref,
            req_c.byom_fence_epoch,
            req_c.kovee_invocation_fence,
        )?;
        let mut effects = Vec::new();
        let mut events = Vec::new();
        let mut settlement_note = json!({"settled": false,
                                         "reason": "evidence only (family contract L33)"});
        let mut settlement_ref = Value::Null;

        if req_c.source == "trusted_meter" {
            let allocation_id = rows::str_of(&p.episode, "resource_allocation_ref").to_owned();
            let allocation = rows::get_row(
                conn,
                "resource_allocations",
                "allocation_id",
                &allocation_id,
            )
            .map_err(db_err)?
            .ok_or_else(|| state::internal("the ResourceAllocation is missing"))?;
            let set_ref = rows::str_of(&allocation, "byom_budget_reservation_set_ref").to_owned();
            let bridge_id = rows::str_of(&allocation, "external_budget_bridge_ref").to_owned();
            let bridge = rows::get_row(conn, "external_budget_bridges", "bridge_id", &bridge_id)
                .map_err(db_err)?
                .ok_or_else(|| state::internal("the ExternalBudgetBridge is missing"))?;
            let stable = req_c.stable_settlement_key.clone().unwrap_or_default();
            // SettleOnce: the exact retry under the same stable key
            // returns the stored settlement; a CHANGED key cannot settle
            // the same use twice (§11.4 unique reservation/key head).
            if let Some(head) = head_row(
                conn,
                "usage_settlement_heads",
                "reservation_set_ref",
                &set_ref,
                "stable_settlement_key",
                &stable,
            )? {
                return Ok(Prepared {
                    result: json!({
                        "episode_id": req_c.episode_ref,
                        "source": req_c.source,
                        "settlement": {
                            "settled": true,
                            "replayed": true,
                            "settlement_ref": rows::str_of(&head, "current_settlement_ref"),
                            "revision": rows::u64_of(&head, "current_settlement_revision"),
                        },
                    }),
                    revision: Some(rows::u64_of(&head, "revision")),
                    cursor: CursorMint::AfterEvents {
                        society_id: society_c.clone(),
                    },
                    effects: Vec::new(),
                    events: Vec::new(),
                });
            }
            if rows::str_of(&bridge, "state") != "confirmed" {
                return Err(state::stale_binding(&format!(
                    "nothing settles on a bridge that is {}: only a confirmed subordinate \
                     reservation may be charged (SubordinateReservation.tla NoChargeWithoutCommit)",
                    rows::str_of(&bridge, "state")
                )));
            }
            let reservation = allocation_reservation(conn, &allocation_id)?;
            let held = rows::u64_of(&reservation, "amount");
            let account = rows::str_of(&reservation, "account_ref").to_owned();
            let charged: u64 = req_c
                .charged_quantities
                .as_ref()
                .map(|q| {
                    q.iter()
                        .filter(|q| q.dimension == part_common::UNIT_DIMENSION)
                        .map(|q| q.amount)
                        .sum()
                })
                .unwrap_or_default();
            // ChargeWithinReservation: the settled charge never exceeds
            // the reserved amount (and therefore never the parent worst
            // case).
            if charged > held {
                return Err(part_common::budget_exceeded(
                    &account,
                    part_common::UNIT_DIMENSION,
                    charged,
                    held,
                ));
            }
            if charged > 0 {
                account_move(
                    conn,
                    &mut effects,
                    &account,
                    part_common::UNIT_DIMENSION,
                    "reserved",
                    "committed",
                    charged,
                )?;
            }
            // The reserved REMAINDER stays held until the saga releases
            // it (`settled -> released`).
            set_reservation(
                &mut effects,
                reservation.clone(),
                held - charged,
                "reserved",
            );
            let record = json!({
                "settlement_id": settlement_id,
                "revision": 1,
                "stable_settlement_key": stable,
                "reservation_set_ref": set_ref,
                "meter_ref": req_c.meter_ref,
                "meter_attestation_ref": req_c.meter_attestation_ref,
                "pricing_revision_ref": req_c.pricing_revision_ref,
                "measured_quantities": serde_json::to_value(&req_c.quantities)
                    .unwrap_or(Value::Null),
                "charged_quantities": serde_json::to_value(&req_c.charged_quantities)
                    .unwrap_or(Value::Null),
                "status": "measured",
                "created_at": created_at,
            });
            let digest = record_digest(
                conn,
                &society_c,
                &settlement_id,
                "bpp-usage-settlement-v0",
                &record,
            )?;
            effects.push(Effect::Upsert {
                table: "usage_settlements".into(),
                row: obj_pairs([
                    ("settlement_id", json!(settlement_id)),
                    ("society_id", json!(society_c)),
                    ("revision", json!(1)),
                    ("previous_settlement_digest", Value::Null),
                    ("stable_settlement_key", json!(stable)),
                    ("reservation_set_ref", json!(set_ref)),
                    ("meter_ref", opt_json(&req_c.meter_ref)),
                    (
                        "meter_attestation_ref",
                        opt_json(&req_c.meter_attestation_ref),
                    ),
                    (
                        "pricing_revision_ref",
                        opt_json(&req_c.pricing_revision_ref),
                    ),
                    (
                        "measured_quantities",
                        json_text(&serde_json::to_value(&req_c.quantities).unwrap_or(Value::Null)),
                    ),
                    (
                        "charged_quantities",
                        json_text(
                            &serde_json::to_value(&req_c.charged_quantities).unwrap_or(Value::Null),
                        ),
                    ),
                    ("status", json!("measured")),
                    ("created_at", json!(created_at)),
                    ("digest", digest_json(&digest)),
                ]),
            });
            effects.push(Effect::Upsert {
                table: "usage_settlement_heads".into(),
                row: obj_pairs([
                    ("reservation_set_ref", json!(set_ref)),
                    ("stable_settlement_key", json!(stable)),
                    ("society_id", json!(society_c)),
                    ("current_settlement_ref", json!(settlement_id)),
                    ("current_settlement_revision", json!(1)),
                    ("current_settlement_digest", digest_json(&digest)),
                    ("revision", json!(1)),
                    ("updated_at", json!(created_at)),
                ]),
            });
            let mut bridge_row = bridge.clone();
            bridge_row.insert("state".into(), json!("settled"));
            bridge_row.insert(
                "revision".into(),
                json!(rows::u64_of(&bridge, "revision") + 1),
            );
            bridge_row.insert("settled_charge".into(), json!(charged));
            effects.push(Effect::Upsert {
                table: "external_budget_bridges".into(),
                row: bridge_row,
            });
            settlement_ref = json!(settlement_id);
            settlement_note = json!({"settled": true, "status": "measured",
                                     "settlement_ref": settlement_id,
                                     "charged": charged,
                                     "reserved_remainder": held - charged});
            events.push(event(
                &society_c,
                &report_event,
                "subordinate-reservation.settled",
                &bridge_id,
                1,
                rows::str_of(&p.episode, "participant_ref"),
                ACTOR_METER,
                &req_c.meta,
                json!({"status": "measured", "charged": charged,
                       "meter_ref": req_c.meter_ref,
                       "applied_once_on_both_sides": true}),
            ));
        } else {
            events.push(event(
                &society_c,
                &report_event,
                "usage-report.recorded",
                &report_id,
                1,
                rows::str_of(&p.episode, "participant_ref"),
                &actor_c,
                &req_c.meta,
                json!({"source": "worker_report", "settles": false,
                       "note": "participant and worker reports are evidence, not meters \
                                (§11.4, family contract L33)"}),
            ));
        }

        let report_body = json!({
            "report_id": report_id,
            "episode_ref": req_c.episode_ref,
            "attempt_ref": req_c.byom_attempt_ref,
            "source": req_c.source,
            "stable_report_key": req_c.stable_report_key,
            "quantities": serde_json::to_value(&req_c.quantities).unwrap_or(Value::Null),
            "created_at": created_at,
        });
        let report_digest = record_digest(
            conn,
            &society_c,
            &report_id,
            "bpp-usage-report-v0",
            &report_body,
        )?;
        effects.push(Effect::Upsert {
            table: "usage_reports".into(),
            row: obj_pairs([
                ("report_id", json!(report_id)),
                ("society_id", json!(society_c)),
                ("episode_ref", json!(req_c.episode_ref)),
                ("attempt_ref", json!(req_c.byom_attempt_ref)),
                ("byom_fence_epoch", json!(req_c.byom_fence_epoch)),
                ("source", json!(req_c.source)),
                ("stable_report_key", json!(req_c.stable_report_key)),
                (
                    "quantities",
                    json_text(&serde_json::to_value(&req_c.quantities).unwrap_or(Value::Null)),
                ),
                ("settlement_ref", settlement_ref.clone()),
                ("created_at", json!(created_at)),
                ("digest", digest_json(&report_digest)),
            ]),
        });
        Ok(Prepared {
            result: json!({
                "report_id": report_id,
                "episode_id": req_c.episode_ref,
                "source": req_c.source,
                "stable_report_key": req_c.stable_report_key,
                "settlement": settlement_note,
                "digest": digest_json(&report_digest),
            }),
            revision: Some(1),
            cursor: CursorMint::AfterEvents {
                society_id: society_c.clone(),
            },
            effects,
            events,
        })
    })
}

// =================================================== budget_reconcile ====

/// `budget_reconcile` (governance, create; R38): the exact reconciliation
/// seat with a FRESH challenge. It is the ONLY release out of an
/// `uncertain` external budget bridge — a governance decision, never a
/// timeout (family contract L33;
/// proof/specs/SubordinateReservation.tla `UncertainReleaseNeedsGovernance`).
/// Services prepare evidence only.
pub fn budget_reconcile(
    store: &mut Store,
    req: &ops::BudgetReconcileRequest,
    body: &Value,
    now: i64,
    hooks: CrashHooks,
) -> Result<Vec<u8>, Problem> {
    let bridge = rows::get_row(
        store.conn(),
        "external_budget_bridges",
        "bridge_id",
        &req.external_budget_bridge_ref,
    )
    .map_err(db_err)?
    .ok_or_else(state::not_found)?;
    let society_id = rows::str_of(&bridge, "society_id").to_owned();
    check_meta_binding(store, &req.meta, &society_id)?;
    let decision_id = format!("dec-budget-{}", req.external_budget_bridge_ref);
    let release_event = mint(store, "evt")?;
    let created_at = rfc3339_utc(now);
    let scope = MutationScope {
        society_id: society_id.clone(),
        operation: "budget_reconcile".into(),
        actor: crate::gov_ops::ACTOR_GOVERNANCE.into(),
        meta: req.meta.clone(),
        body: body.clone(),
    };
    let req_c = req.clone();
    let society_c = society_id.clone();
    run(store, scope, now, hooks, move |conn, _| {
        let bridge = rows::get_row(
            conn,
            "external_budget_bridges",
            "bridge_id",
            &req_c.external_budget_bridge_ref,
        )
        .map_err(db_err)?
        .ok_or_else(state::not_found)?;
        if rows::str_of(&bridge, "stable_external_reservation_key")
            != req_c.stable_external_reservation_key
        {
            return Err(state::stale_binding(
                "the request does not name this bridge's stable external reservation key",
            ));
        }
        if rows::str_of(&bridge, "state") != "uncertain" {
            return Err(state::stale_binding(&format!(
                "the bridge is {}; the R38 seat releases only an UNCERTAIN bridge",
                rows::str_of(&bridge, "state")
            )));
        }
        // The allocation whose hold this decision releases.
        let allocation = rows::rows_where(
            conn,
            "resource_allocations",
            "external_budget_bridge_ref",
            &req_c.external_budget_bridge_ref,
            "allocation_id",
        )
        .map_err(db_err)?
        .into_iter()
        .next()
        .ok_or_else(|| state::internal("the bridge has no ResourceAllocation"))?;
        let allocation_id = rows::str_of(&allocation, "allocation_id").to_owned();
        let reservation = allocation_reservation(conn, &allocation_id)?;
        let held = rows::u64_of(&reservation, "amount");
        let account = rows::str_of(&reservation, "account_ref").to_owned();
        let subject_body = json!({
            "external_budget_bridge_ref": req_c.external_budget_bridge_ref,
            "stable_external_reservation_key": req_c.stable_external_reservation_key,
            "released_amount": held,
            "reason_ref": req_c.reason_ref,
            "fresh_challenge_ref": req_c.fresh_challenge_ref,
        });
        let subject_digest = record_digest(
            conn,
            &society_c,
            &req_c.external_budget_bridge_ref,
            "bpp-budget-reconcile-subject-v0",
            &subject_body,
        )?;
        let sovereign = rows::sovereign_participant(conn, &society_c)
            .map_err(db_err)?
            .ok_or_else(|| state::internal("no sovereign participant"))?;
        let mut effects = vec![crate::gov_decision::form(
            conn,
            &decision_id,
            &society_c,
            "budget_reconciliation",
            "external_budget_bridge",
            &req_c.external_budget_bridge_ref,
            &subject_digest,
            "charter:reconciliation",
            &[crate::gov_decision::DecisionSeat {
                seat_ref: format!("seat-sovereign-{society_c}"),
                participant_ref: sovereign.participant_id.clone(),
                actor_ref: crate::gov_ops::ACTOR_GOVERNANCE.to_owned(),
                participant_binding_epoch: sovereign.binding_epoch,
            }],
            &[],
            "sovereign_seat_assent",
            crate::gov_ops::ACTOR_GOVERNANCE,
            now,
        )?];
        // The unknown quantity returns to `remaining` ONLY here.
        account_move(
            conn,
            &mut effects,
            &account,
            part_common::UNIT_DIMENSION,
            "uncertain",
            "remaining",
            held,
        )?;
        set_reservation(&mut effects, reservation.clone(), held, "released");
        let revision = rows::u64_of(&bridge, "revision") + 1;
        let mut bridge_row = bridge.clone();
        bridge_row.insert("state".into(), json!("released"));
        bridge_row.insert("revision".into(), json!(revision));
        bridge_row.insert("reconcile_decision_ref".into(), json!(decision_id));
        effects.push(Effect::Upsert {
            table: "external_budget_bridges".into(),
            row: bridge_row,
        });
        let mut allocation_row = allocation.clone();
        allocation_row.insert("state".into(), json!("released"));
        allocation_row.insert(
            "revision".into(),
            json!(rows::u64_of(&allocation, "revision") + 1),
        );
        effects.push(Effect::Upsert {
            table: "resource_allocations".into(),
            row: allocation_row,
        });
        Ok(Prepared {
            result: json!({
                "external_budget_bridge_ref": req_c.external_budget_bridge_ref,
                "state": "released",
                "revision": revision,
                "governance_decision_ref": decision_id,
                "released_amount": held,
                "released_from_bucket": "uncertain",
                "created_at": created_at,
            }),
            revision: Some(revision),
            cursor: CursorMint::AfterEvents {
                society_id: society_c.clone(),
            },
            effects,
            events: vec![event(
                &society_c,
                &release_event,
                "subordinate-reservation.released",
                &req_c.external_budget_bridge_ref,
                revision,
                &sovereign.participant_id,
                crate::gov_ops::ACTOR_GOVERNANCE,
                &req_c.meta,
                json!({"state": "released", "released_amount": held,
                       "governance_decision_ref": decision_id,
                       "note": "the only release out of uncertain is this R38 seat — never a \
                                timeout (family contract L33)"}),
            )],
        })
    })
}
