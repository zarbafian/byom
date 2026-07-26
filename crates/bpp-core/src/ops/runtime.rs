//! Runtime-surface and reconciliation request shapes (B3 slice 2;
//! §11.1–§11.4, §13.1/§13.2, §14.6 `runtime`/`budgets`/`recovery`
//! families; registry R30/R33/R35/R38).
//!
//! What you write (one protected runtime command; both fences always):
//! ```
//! use bpp_core::ops::EpisodeStartRequest;
//! let body = serde_json::json!({
//!     "version": "0.2", "op": "episode_start",
//!     "meta": {"request_id": "r", "idempotency_key": "k",
//!              "expected_endpoint_incarnation": "inc",
//!              "expected_recovery_epoch": 0, "expected_revision": 1},
//!     "episode_ref": "ep-1", "generation": 1,
//!     "byom_attempt_ref": "att-1", "byom_fence_epoch": 1,
//!     "kovee_invocation_fence": 7});
//! let req = EpisodeStartRequest::parse(&body).unwrap();
//! assert_eq!(req.byom_fence_epoch, 1);
//! // A mutation carrying only ONE fence is the committed negative
//! // vector (family contract L21): the closed shape refuses it.
//! let mut one = body.clone();
//! one.as_object_mut().unwrap().remove("kovee_invocation_fence");
//! assert!(EpisodeStartRequest::parse(&one).is_err());
//! ```

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    check_create_meta, check_id_array, check_identifier, check_local_erasure_safe, check_op,
    check_opt_identifier, check_opt_local_erasure_safe, check_timestamp, check_update_meta,
    check_version, parse_closed,
};
use crate::canonical::SAFE_MAX;
use crate::digest::{DigestClass, DigestRef};
use crate::envelope::MutationMeta;

/// The CROSS-BOUNDARY class rule (family contract, PROFILE.md §6.2): a
/// digest one protocol demands from the other MUST be `portable_public`,
/// because the counterparty has to derive the same value from the same
/// bytes. A `local_erasure_safe` value is an HMAC under the OWNER's
/// per-object secret: a counterparty could only echo an opaque blob it can
/// never check, and D-R1-2 forbids re-deriving it from a shared key. Every
/// field here that byom recomputes from its OWN committed state keeps
/// `local_erasure_safe` — and is therefore never asked for on the wire.
fn check_portable(name: &str, d: &DigestRef) -> Result<(), String> {
    d.require_class(DigestClass::PortablePublic)
        .map_err(|e| format!("{name}: {e}"))
}

fn check_opt_portable(name: &str, d: &Option<DigestRef>) -> Result<(), String> {
    match d {
        Some(d) => check_portable(name, d),
        None => Ok(()),
    }
}

fn check_safe(name: &str, v: u64) -> Result<(), String> {
    if v > SAFE_MAX {
        return Err(format!("{name} exceeds the safe range"));
    }
    Ok(())
}

/// The DUAL fence block every protected runtime command presents
/// (family contract L21/R30): the Byom lease fence AND the Kovee
/// invocation fence. Neither alone authorizes a mutation.
fn check_dual_fences(
    episode_ref: &str,
    generation: u64,
    attempt_ref: &str,
    byom_fence_epoch: u64,
    kovee_invocation_fence: u64,
) -> Result<(), String> {
    check_identifier("episode_ref", episode_ref)?;
    check_identifier("byom_attempt_ref", attempt_ref)?;
    check_safe("generation", generation)?;
    check_safe("byom_fence_epoch", byom_fence_epoch)?;
    check_safe("kovee_invocation_fence", kovee_invocation_fence)
}

// -------------------------------------------------- placement_admit ----

/// One `byom_subordinate` reservation item, each pinned to its exact
/// parent §11.4 reservation item (`byom-subordinate-reservation`
/// schema): `amount <= parent_worst_case_amount` with identical
/// dimension and unit — never above parent (family contract L32).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubordinateItem {
    pub kovee_account_ref: String,
    pub dimension: String,
    pub unit: String,
    pub amount: u64,
    pub parent_account_ref: String,
    pub parent_account_revision: u64,
    pub parent_dimension: String,
    pub parent_unit: String,
    pub parent_worst_case_amount: u64,
}

impl SubordinateItem {
    fn validate(&self) -> Result<(), String> {
        for (name, v) in [
            ("kovee_account_ref", &self.kovee_account_ref),
            ("dimension", &self.dimension),
            ("unit", &self.unit),
            ("parent_account_ref", &self.parent_account_ref),
            ("parent_dimension", &self.parent_dimension),
            ("parent_unit", &self.parent_unit),
        ] {
            check_identifier(name, v)?;
        }
        check_safe("amount", self.amount)?;
        check_safe("parent_account_revision", self.parent_account_revision)?;
        check_safe("parent_worst_case_amount", self.parent_worst_case_amount)
    }
}

/// The subordinate-reservation outcome the narrow Kovee placement
/// adapter reports (`spec/descriptors/subordinate-reservation.json`):
/// Kovee may narrow or deny, never parallel-charge; an unknown result
/// stays `uncertain` and spend stays blocked (§11.4, L33).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubordinateOutcome {
    pub stable_external_reservation_key: String,
    pub outcome: String,
    #[serde(default)]
    pub subordinate_reservation_ref: Option<String>,
    #[serde(default)]
    pub revision: Option<u64>,
    #[serde(default)]
    pub digest: Option<DigestRef>,
    #[serde(default)]
    pub items: Vec<SubordinateItem>,
}

/// The closed §11.4 `ExternalBudgetBridge.state` list, verbatim.
pub const BRIDGE_STATES: [&str; 6] = [
    "requested",
    "confirmed",
    "denied",
    "uncertain",
    "settled",
    "released",
];

impl SubordinateOutcome {
    fn validate(&self) -> Result<(), String> {
        check_identifier(
            "stable_external_reservation_key",
            &self.stable_external_reservation_key,
        )?;
        if !matches!(self.outcome.as_str(), "confirmed" | "denied" | "uncertain") {
            return Err("outcome is not a closed subordinate saga outcome".to_owned());
        }
        check_opt_identifier(
            "subordinate_reservation_ref",
            &self.subordinate_reservation_ref,
        )?;
        check_opt_portable("digest", &self.digest)?;
        if let Some(r) = self.revision {
            check_safe("revision", r)?;
        }
        if self.items.len() > 64 {
            return Err("items is out of bounds".to_owned());
        }
        for item in &self.items {
            item.validate()?;
        }
        // The confirmed arm is the only one that names a committed
        // Kovee row; a denial or unknown result names none (the ref /
        // revision / digest triple is all-or-none, §11.4).
        let named = self.subordinate_reservation_ref.is_some()
            && self.revision.is_some()
            && self.digest.is_some();
        match self.outcome.as_str() {
            "confirmed" => {
                if !named || self.items.is_empty() {
                    return Err(
                        "a confirmed subordinate reservation names its exact ref, revision, \
                         digest and at least one item (§11.4)"
                            .to_owned(),
                    );
                }
            }
            _ => {
                if named || !self.items.is_empty() {
                    return Err(
                        "only a confirmed subordinate reservation carries a committed \
                         ref/revision/digest and items"
                            .to_owned(),
                    );
                }
            }
        }
        Ok(())
    }
}

/// placement_admit (runtime, create; R33): the narrow Kovee placement
/// adapter bound to the exact ResourceAllocation. Byom records only a
/// PlacementAdmission after source verification — Kovee alone authors
/// the PlacementBinding (§11.1).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlacementAdmitRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub resource_allocation_ref: String,
    pub resource_allocation_digest: DigestRef,
    pub kovee_placement_ref: String,
    pub kovee_placement_revision: u64,
    pub kovee_placement_digest: DigestRef,
    pub source_binding_epoch: u64,
    pub selected_manifestation_ref: String,
    pub kovee_invocation_ref: String,
    pub kovee_fence_epoch: u64,
    pub subordinate_reservation: SubordinateOutcome,
}

impl PlacementAdmitRequest {
    pub fn parse(body: &Value) -> Result<PlacementAdmitRequest, String> {
        let req: PlacementAdmitRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "placement_admit")?;
        check_create_meta(&req.meta)?;
        check_identifier("resource_allocation_ref", &req.resource_allocation_ref)?;
        // Cross-boundary: Kovee pins the allocation `episode_request`
        // created, so the digest is `portable_public` over the published
        // `bpp-resource-allocation-binding-v0` fragment — the value byom
        // RETURNS from `episode_request`, derivable by both sides (S-1/S-2).
        check_portable(
            "resource_allocation_digest",
            &req.resource_allocation_digest,
        )?;
        check_identifier("kovee_placement_ref", &req.kovee_placement_ref)?;
        check_safe("kovee_placement_revision", req.kovee_placement_revision)?;
        check_portable("kovee_placement_digest", &req.kovee_placement_digest)?;
        check_safe("source_binding_epoch", req.source_binding_epoch)?;
        check_identifier(
            "selected_manifestation_ref",
            &req.selected_manifestation_ref,
        )?;
        check_identifier("kovee_invocation_ref", &req.kovee_invocation_ref)?;
        check_safe("kovee_fence_epoch", req.kovee_fence_epoch)?;
        req.subordinate_reservation.validate()?;
        Ok(req)
    }
}

// ---------------------------------------------------- episode_claim ----

/// episode_claim (runtime, create; R30): the compare-and-swap on the ONE
/// EpisodeLeaseHead. The claimer supplies its workload identity, its
/// proposed lease TTL, and the exact Kovee invocation binding the
/// committed `ByomEpisodeBinding` row carries (C2, field-verbatim).
///
/// There is NO `claim_subject_digest` member: the claim subject is byom's
/// own authority subject over byom's own staged attempt, so byom computes
/// it (`local_erasure_safe`, per-object — PROFILE.md §6.2) and never asks
/// a counterparty for a value that counterparty cannot derive (S-2).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EpisodeClaimRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub episode_ref: String,
    pub generation: u64,
    pub holder_runtime_binding: String,
    pub lease_ttl_seconds: u64,
    pub kovee_invocation_ref: String,
    pub kovee_invocation_fence: u64,
    pub stable_binding_key: String,
    pub context_manifest_ref: String,
    pub context_manifest_digest: DigestRef,
    pub context_source_digest: DigestRef,
    pub mandate_use_refs: Vec<String>,
    pub allowed_local_commitments: Vec<String>,
    #[serde(default)]
    pub kovee_context_assembly_ref: Option<String>,
    #[serde(default)]
    pub kovee_context_assembly_digest: Option<DigestRef>,
    #[serde(default)]
    pub provider_context_manifest_ref: Option<String>,
    #[serde(default)]
    pub provider_context_manifest_digest: Option<DigestRef>,
}

/// Lease TTL bounds (§11.2 fixes no number; this bundle pins the
/// negotiated window — recorded deviation).
pub const LEASE_TTL_MIN_SECONDS: u64 = 1;
pub const LEASE_TTL_MAX_SECONDS: u64 = 86_400;

impl EpisodeClaimRequest {
    pub fn parse(body: &Value) -> Result<EpisodeClaimRequest, String> {
        let req: EpisodeClaimRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "episode_claim")?;
        check_create_meta(&req.meta)?;
        check_identifier("episode_ref", &req.episode_ref)?;
        check_safe("generation", req.generation)?;
        check_identifier("holder_runtime_binding", &req.holder_runtime_binding)?;
        if !(LEASE_TTL_MIN_SECONDS..=LEASE_TTL_MAX_SECONDS).contains(&req.lease_ttl_seconds) {
            return Err("lease_ttl_seconds is outside the negotiated lease window".to_owned());
        }
        check_identifier("kovee_invocation_ref", &req.kovee_invocation_ref)?;
        check_safe("kovee_invocation_fence", req.kovee_invocation_fence)?;
        check_identifier("stable_binding_key", &req.stable_binding_key)?;
        check_identifier("context_manifest_ref", &req.context_manifest_ref)?;
        // Cross-boundary: the ContextManifest is KOVEE's object — byom holds
        // only the ref, so it cannot re-derive a keyed digest over content it
        // does not have. It is also preimage material for the
        // `portable_public` `context_source_digest`, and D-R1-2 forbids a
        // keyed value inside a class both sides must derive (S-2).
        check_portable("context_manifest_digest", &req.context_manifest_digest)?;
        check_portable("context_source_digest", &req.context_source_digest)?;
        check_id_array("mandate_use_refs", &req.mandate_use_refs, 0, 256)?;
        check_id_array(
            "allowed_local_commitments",
            &req.allowed_local_commitments,
            0,
            64,
        )?;
        check_opt_identifier(
            "kovee_context_assembly_ref",
            &req.kovee_context_assembly_ref,
        )?;
        check_opt_local_erasure_safe(
            "kovee_context_assembly_digest",
            &req.kovee_context_assembly_digest,
        )?;
        check_opt_identifier(
            "provider_context_manifest_ref",
            &req.provider_context_manifest_ref,
        )?;
        check_opt_local_erasure_safe(
            "provider_context_manifest_digest",
            &req.provider_context_manifest_digest,
        )?;
        // The Δ5 context pairs are all-or-none (frozen C2 oneOf).
        if req.kovee_context_assembly_ref.is_some() != req.kovee_context_assembly_digest.is_some() {
            return Err("kovee_context_assembly ref/digest is an all-or-none pair".to_owned());
        }
        if req.provider_context_manifest_ref.is_some()
            != req.provider_context_manifest_digest.is_some()
        {
            return Err("provider_context_manifest ref/digest is an all-or-none pair".to_owned());
        }
        Ok(req)
    }
}

// ------------------------------------- protected per-attempt commands ----

/// episode_start (runtime, update; R30): only the current holder under
/// the current fences, CASing the lease head revision.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EpisodeStartRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub episode_ref: String,
    pub generation: u64,
    pub byom_attempt_ref: String,
    pub byom_fence_epoch: u64,
    pub kovee_invocation_fence: u64,
}

impl EpisodeStartRequest {
    pub fn parse(body: &Value) -> Result<EpisodeStartRequest, String> {
        let req: EpisodeStartRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "episode_start")?;
        check_update_meta(&req.meta)?;
        check_dual_fences(
            &req.episode_ref,
            req.generation,
            &req.byom_attempt_ref,
            req.byom_fence_epoch,
            req.kovee_invocation_fence,
        )?;
        Ok(req)
    }
}

/// checkpoint_commit (runtime, create; R30): one immutable
/// EpisodeAttemptEvent under both current fences.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointCommitRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub episode_ref: String,
    pub generation: u64,
    pub byom_attempt_ref: String,
    pub byom_fence_epoch: u64,
    pub kovee_invocation_fence: u64,
    pub expected_lease_revision: u64,
    pub checkpoint_ref: String,
    pub checkpoint_digest: DigestRef,
}

impl CheckpointCommitRequest {
    pub fn parse(body: &Value) -> Result<CheckpointCommitRequest, String> {
        let req: CheckpointCommitRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "checkpoint_commit")?;
        check_create_meta(&req.meta)?;
        check_dual_fences(
            &req.episode_ref,
            req.generation,
            &req.byom_attempt_ref,
            req.byom_fence_epoch,
            req.kovee_invocation_fence,
        )?;
        check_safe("expected_lease_revision", req.expected_lease_revision)?;
        check_identifier("checkpoint_ref", &req.checkpoint_ref)?;
        // Cross-boundary: the checkpoint is the WORKLOAD's content. byom
        // records the commitment and holds no bytes to re-derive it from, so
        // the class has to be one the worker and every later reader can
        // derive (S-2).
        check_portable("checkpoint_digest", &req.checkpoint_digest)?;
        Ok(req)
    }
}

/// episode_yield (runtime, update; R30): the running Episode returns the
/// lease; `target_state` is the closed §14.8 discriminator.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EpisodeYieldRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub episode_ref: String,
    pub generation: u64,
    pub byom_attempt_ref: String,
    pub byom_fence_epoch: u64,
    pub kovee_invocation_fence: u64,
    pub target_state: String,
    #[serde(default)]
    pub reason_ref: Option<String>,
}

impl EpisodeYieldRequest {
    pub fn parse(body: &Value) -> Result<EpisodeYieldRequest, String> {
        let req: EpisodeYieldRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "episode_yield")?;
        check_update_meta(&req.meta)?;
        check_dual_fences(
            &req.episode_ref,
            req.generation,
            &req.byom_attempt_ref,
            req.byom_fence_epoch,
            req.kovee_invocation_fence,
        )?;
        if !matches!(req.target_state.as_str(), "yielded" | "waiting") {
            return Err("target_state is not a closed episode yield target".to_owned());
        }
        check_opt_identifier("reason_ref", &req.reason_ref)?;
        Ok(req)
    }
}

/// episode_complete (runtime, update; R30): completion is EVIDENCE only
/// — the Delivery stays separate and pledgor-authored (§11.2).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EpisodeCompleteRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub episode_ref: String,
    pub generation: u64,
    pub byom_attempt_ref: String,
    pub byom_fence_epoch: u64,
    pub kovee_invocation_fence: u64,
    pub output_refs: Vec<String>,
    pub evidence_refs: Vec<String>,
    pub usage_report_refs: Vec<String>,
}

impl EpisodeCompleteRequest {
    pub fn parse(body: &Value) -> Result<EpisodeCompleteRequest, String> {
        let req: EpisodeCompleteRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "episode_complete")?;
        check_update_meta(&req.meta)?;
        check_dual_fences(
            &req.episode_ref,
            req.generation,
            &req.byom_attempt_ref,
            req.byom_fence_epoch,
            req.kovee_invocation_fence,
        )?;
        check_id_array("output_refs", &req.output_refs, 0, 256)?;
        check_id_array("evidence_refs", &req.evidence_refs, 0, 256)?;
        check_id_array("usage_report_refs", &req.usage_report_refs, 0, 256)?;
        Ok(req)
    }
}

/// episode_fail (runtime, update; R30).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EpisodeFailRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub episode_ref: String,
    pub generation: u64,
    pub byom_attempt_ref: String,
    pub byom_fence_epoch: u64,
    pub kovee_invocation_fence: u64,
    pub failure_reason_ref: String,
    pub evidence_refs: Vec<String>,
}

impl EpisodeFailRequest {
    pub fn parse(body: &Value) -> Result<EpisodeFailRequest, String> {
        let req: EpisodeFailRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "episode_fail")?;
        check_update_meta(&req.meta)?;
        check_dual_fences(
            &req.episode_ref,
            req.generation,
            &req.byom_attempt_ref,
            req.byom_fence_epoch,
            req.kovee_invocation_fence,
        )?;
        check_identifier("failure_reason_ref", &req.failure_reason_ref)?;
        check_id_array("evidence_refs", &req.evidence_refs, 0, 256)?;
        Ok(req)
    }
}

// ----------------------------------------------------- usage_report ----

/// One measured or reported quantity.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Quantity {
    pub dimension: String,
    pub unit: String,
    pub amount: u64,
}

impl Quantity {
    fn validate(&self) -> Result<(), String> {
        check_identifier("dimension", &self.dimension)?;
        check_identifier("unit", &self.unit)?;
        check_safe("amount", self.amount)
    }
}

/// usage_report (runtime, create; R30). Two arms, and the CHANNEL — not
/// a flag — decides which is admissible: a worker report is evidence and
/// settles nothing; only the narrow trusted-meter adapter may carry the
/// measured settlement (§11.4, family contract L33).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UsageReportRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub episode_ref: String,
    pub generation: u64,
    pub byom_attempt_ref: String,
    pub byom_fence_epoch: u64,
    pub kovee_invocation_fence: u64,
    pub source: String,
    pub stable_report_key: String,
    pub quantities: Vec<Quantity>,
    #[serde(default)]
    pub meter_ref: Option<String>,
    #[serde(default)]
    pub meter_attestation_ref: Option<String>,
    #[serde(default)]
    pub pricing_revision_ref: Option<String>,
    #[serde(default)]
    pub stable_settlement_key: Option<String>,
    #[serde(default)]
    pub charged_quantities: Option<Vec<Quantity>>,
}

impl UsageReportRequest {
    pub fn parse(body: &Value) -> Result<UsageReportRequest, String> {
        let req: UsageReportRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "usage_report")?;
        check_create_meta(&req.meta)?;
        check_dual_fences(
            &req.episode_ref,
            req.generation,
            &req.byom_attempt_ref,
            req.byom_fence_epoch,
            req.kovee_invocation_fence,
        )?;
        if !matches!(req.source.as_str(), "worker_report" | "trusted_meter") {
            return Err("source is not a closed usage-report source".to_owned());
        }
        check_identifier("stable_report_key", &req.stable_report_key)?;
        if req.quantities.is_empty() || req.quantities.len() > 64 {
            return Err("quantities is out of bounds".to_owned());
        }
        for q in &req.quantities {
            q.validate()?;
        }
        check_opt_identifier("meter_ref", &req.meter_ref)?;
        check_opt_identifier("meter_attestation_ref", &req.meter_attestation_ref)?;
        check_opt_identifier("pricing_revision_ref", &req.pricing_revision_ref)?;
        check_opt_identifier("stable_settlement_key", &req.stable_settlement_key)?;
        if let Some(charged) = &req.charged_quantities {
            if charged.is_empty() || charged.len() > 64 {
                return Err("charged_quantities is out of bounds".to_owned());
            }
            for q in charged {
                q.validate()?;
            }
        }
        let settles = req.meter_ref.is_some()
            && req.meter_attestation_ref.is_some()
            && req.stable_settlement_key.is_some()
            && req.charged_quantities.is_some();
        let any_settlement_member = req.meter_ref.is_some()
            || req.meter_attestation_ref.is_some()
            || req.stable_settlement_key.is_some()
            || req.charged_quantities.is_some()
            || req.pricing_revision_ref.is_some();
        match req.source.as_str() {
            "trusted_meter" if !settles => Err(
                "a trusted_meter report carries the complete settlement group (meter_ref, \
                 meter_attestation_ref, stable_settlement_key, charged_quantities)"
                    .to_owned(),
            ),
            "worker_report" if any_settlement_member => Err(
                "participant and worker reports are evidence, not meters: a worker_report \
                 carries no settlement member (§11.4, family contract L33)"
                    .to_owned(),
            ),
            _ => Ok(req),
        }
    }
}

// ---------------------------------------------- effect_outcome_admit ----

/// effect_outcome_admit (runtime, create; R35): the narrow trusted
/// effect-admission adapter. SOURCE FACTS ONLY — this path has no
/// GovernanceDecision field at all (§13.2).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectOutcomeAdmitRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub episode_ref: String,
    pub generation: u64,
    pub byom_attempt_ref: String,
    pub byom_fence_epoch: u64,
    pub kovee_invocation_fence: u64,
    pub intent_ref: String,
    pub intent_digest: DigestRef,
    pub stable_execution_key: String,
    pub host_protocol: String,
    pub host_endpoint_ref: String,
    pub host_effect_ref: String,
    pub host_effect_digest: DigestRef,
    pub host_receipt_ref: String,
    pub host_receipt_digest: DigestRef,
    pub host_cursor_or_signature_ref: String,
    pub verification_status: String,
    pub outcome: String,
    #[serde(default)]
    pub result_ref: Option<String>,
    #[serde(default)]
    pub result_digest: Option<DigestRef>,
    #[serde(default)]
    pub usage_settlement_ref: Option<String>,
    #[serde(default)]
    pub reconciles_admission_ref: Option<String>,
    #[serde(default)]
    pub reconciles_admission_digest: Option<DigestRef>,
}

impl EffectOutcomeAdmitRequest {
    pub fn parse(body: &Value) -> Result<EffectOutcomeAdmitRequest, String> {
        let req: EffectOutcomeAdmitRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "effect_outcome_admit")?;
        check_create_meta(&req.meta)?;
        check_dual_fences(
            &req.episode_ref,
            req.generation,
            &req.byom_attempt_ref,
            req.byom_fence_epoch,
            req.kovee_invocation_fence,
        )?;
        check_identifier("intent_ref", &req.intent_ref)?;
        check_local_erasure_safe("intent_digest", &req.intent_digest)?;
        check_identifier("stable_execution_key", &req.stable_execution_key)?;
        if req.host_protocol != "kovee" {
            return Err("host_protocol has exactly one admissible value: kovee".to_owned());
        }
        for (name, v) in [
            ("host_endpoint_ref", &req.host_endpoint_ref),
            ("host_effect_ref", &req.host_effect_ref),
            ("host_receipt_ref", &req.host_receipt_ref),
            (
                "host_cursor_or_signature_ref",
                &req.host_cursor_or_signature_ref,
            ),
        ] {
            check_identifier(name, v)?;
        }
        check_portable("host_effect_digest", &req.host_effect_digest)?;
        check_portable("host_receipt_digest", &req.host_receipt_digest)?;
        if req.verification_status != "verified" {
            return Err(
                "byom admits an EffectOutcomeAdmission only from a VERIFIED source revision \
                 (§13.2)"
                    .to_owned(),
            );
        }
        if !matches!(req.outcome.as_str(), "succeeded" | "failed" | "ambiguous") {
            return Err("outcome is not a closed effect outcome".to_owned());
        }
        check_opt_identifier("result_ref", &req.result_ref)?;
        check_opt_local_erasure_safe("result_digest", &req.result_digest)?;
        check_opt_identifier("usage_settlement_ref", &req.usage_settlement_ref)?;
        check_opt_identifier("reconciles_admission_ref", &req.reconciles_admission_ref)?;
        // The reconciled predecessor is byom's OWN admission record, so
        // its digest carries the local erasure class (the host-side
        // digests above are the portable cross-boundary ones).
        check_opt_local_erasure_safe(
            "reconciles_admission_digest",
            &req.reconciles_admission_digest,
        )?;
        if req.result_ref.is_some() != req.result_digest.is_some() {
            return Err("result ref/digest is an all-or-none pair".to_owned());
        }
        if req.reconciles_admission_ref.is_some() != req.reconciles_admission_digest.is_some() {
            return Err("reconciles_admission ref/digest is an all-or-none pair".to_owned());
        }
        if req.outcome == "ambiguous"
            && (req.result_ref.is_some() || req.reconciles_admission_ref.is_some())
        {
            return Err(
                "an ambiguous source admission carries no result and reconciles nothing \
                 (§13.2)"
                    .to_owned(),
            );
        }
        Ok(req)
    }
}

// -------------------------------------------------- effect_reconcile ----

/// effect_reconcile (governance, create; R38): the exact reconciliation
/// seat. It appends an independent EffectGovernanceDisposition against
/// the exact source admission; it never advances the EOA head, never
/// claims the host Effect became factually succeeded or failed (§13.2).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectReconcileRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub intent_ref: String,
    pub intent_digest: DigestRef,
    pub stable_execution_key: String,
    pub phase: String,
    pub basis_source_admission_ref: String,
    pub basis_source_admission_revision: u64,
    pub basis_source_admission_digest: DigestRef,
    pub local_outcome: String,
    pub result_use: String,
    pub fresh_challenge_ref: String,
    #[serde(default)]
    pub classification_admission_ref: Option<String>,
    #[serde(default)]
    pub classification_admission_digest: Option<DigestRef>,
    #[serde(default)]
    pub late_source_policy: Option<String>,
}

impl EffectReconcileRequest {
    pub fn parse(body: &Value) -> Result<EffectReconcileRequest, String> {
        let req: EffectReconcileRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "effect_reconcile")?;
        check_create_meta(&req.meta)?;
        check_identifier("intent_ref", &req.intent_ref)?;
        check_local_erasure_safe("intent_digest", &req.intent_digest)?;
        check_identifier("stable_execution_key", &req.stable_execution_key)?;
        if !matches!(req.phase.as_str(), "ambiguous_source" | "late_source") {
            return Err("phase is not a closed disposition phase".to_owned());
        }
        check_identifier(
            "basis_source_admission_ref",
            &req.basis_source_admission_ref,
        )?;
        check_safe(
            "basis_source_admission_revision",
            req.basis_source_admission_revision,
        )?;
        check_local_erasure_safe(
            "basis_source_admission_digest",
            &req.basis_source_admission_digest,
        )?;
        if !matches!(req.local_outcome.as_str(), "succeeded" | "failed") {
            return Err("local_outcome is not a closed local outcome".to_owned());
        }
        if !matches!(
            req.result_use.as_str(),
            "unavailable" | "quarantined" | "released"
        ) {
            return Err("result_use is not a closed result use".to_owned());
        }
        check_identifier("fresh_challenge_ref", &req.fresh_challenge_ref)?;
        check_opt_identifier(
            "classification_admission_ref",
            &req.classification_admission_ref,
        )?;
        check_opt_local_erasure_safe(
            "classification_admission_digest",
            &req.classification_admission_digest,
        )?;
        if req.classification_admission_ref.is_some()
            != req.classification_admission_digest.is_some()
        {
            return Err("classification_admission ref/digest is an all-or-none pair".to_owned());
        }
        if let Some(policy) = &req.late_source_policy {
            if policy != "quarantine_and_redecide" {
                return Err("late_source_policy has one admissible value".to_owned());
            }
        }
        // The closed §13.2 disposition union.
        match req.phase.as_str() {
            "ambiguous_source" => {
                if req.result_use != "unavailable" {
                    return Err(
                        "an ambiguous_source disposition requires result_use: unavailable"
                            .to_owned(),
                    );
                }
                if req.late_source_policy.as_deref() != Some("quarantine_and_redecide") {
                    return Err(
                        "an ambiguous_source disposition requires late_source_policy: \
                         quarantine_and_redecide"
                            .to_owned(),
                    );
                }
                if req.classification_admission_ref.is_some() {
                    return Err(
                        "ambiguous-source records forbid classification-admission fields"
                            .to_owned(),
                    );
                }
            }
            _ => {
                if req.late_source_policy.is_some() {
                    return Err("a late_source disposition forbids late_source_policy".to_owned());
                }
                if req.result_use == "unavailable" && req.classification_admission_ref.is_some() {
                    return Err(
                        "an unavailable late_source result carries no classification admission"
                            .to_owned(),
                    );
                }
                if req.result_use != "unavailable" && req.classification_admission_ref.is_none() {
                    return Err(
                        "a quarantined or released late_source result binds its exact \
                         classification admission"
                            .to_owned(),
                    );
                }
            }
        }
        Ok(req)
    }
}

// -------------------------------------------------- budget_reconcile ----

/// budget_reconcile (governance, create; R38): the ONLY release out of
/// an `uncertain` external budget bridge — a governance decision, never
/// a timeout (family contract L33;
/// proof/specs/SubordinateReservation.tla
/// `UncertainReleaseNeedsGovernance`).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetReconcileRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub external_budget_bridge_ref: String,
    pub stable_external_reservation_key: String,
    pub fresh_challenge_ref: String,
    pub reason_ref: String,
}

impl BudgetReconcileRequest {
    pub fn parse(body: &Value) -> Result<BudgetReconcileRequest, String> {
        let req: BudgetReconcileRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "budget_reconcile")?;
        check_create_meta(&req.meta)?;
        check_identifier(
            "external_budget_bridge_ref",
            &req.external_budget_bridge_ref,
        )?;
        check_identifier(
            "stable_external_reservation_key",
            &req.stable_external_reservation_key,
        )?;
        check_identifier("fresh_challenge_ref", &req.fresh_challenge_ref)?;
        check_identifier("reason_ref", &req.reason_ref)?;
        Ok(req)
    }
}

// ----------------------------------------------------- episode_request ----

/// episode_request (participant, create; R29). The caller supplies the
/// exact WakeIntent and the ActivationAdmission ref; manifestation,
/// context manifest, resource allocation and placement fields are
/// kernel/saga-derived and NO STAGE CAN BE SKIPPED (§11.1). The
/// admission ref is derived from the subject it decides
/// (`adm-<wake_intent>-r<revision>`), so the request can only match the
/// server value.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EpisodeRequestRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub activity_stream_ref: String,
    pub generation: u64,
    pub wake_intent_ref: String,
    pub activation_admission_ref: String,
    #[serde(default)]
    pub pledge_revision: Option<u64>,
    #[serde(default)]
    pub deadline: Option<String>,
}

impl EpisodeRequestRequest {
    pub fn parse(body: &Value) -> Result<EpisodeRequestRequest, String> {
        let req: EpisodeRequestRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "episode_request")?;
        check_create_meta(&req.meta)?;
        check_identifier("activity_stream_ref", &req.activity_stream_ref)?;
        check_safe("generation", req.generation)?;
        check_identifier("wake_intent_ref", &req.wake_intent_ref)?;
        check_identifier("activation_admission_ref", &req.activation_admission_ref)?;
        if let Some(r) = req.pledge_revision {
            check_safe("pledge_revision", r)?;
        }
        if let Some(d) = &req.deadline {
            check_timestamp("deadline", d)?;
        }
        Ok(req)
    }
}
