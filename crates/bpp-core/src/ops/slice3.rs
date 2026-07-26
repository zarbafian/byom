//! B3 slice-3 request shapes outside the frozen B0.1 wires: the §7.4
//! onboarding path (`onboarding_offer`, the one-shot
//! `onboarding_compute_permit_consume`, `onboarding_episode_claim` /
//! `onboarding_episode_complete`), the §11.1/§16.4 attention intake
//! (`attention_notice_record`), and the §12.1 provider-context source-field
//! read (`context_manifest_show`).
//!
//! What you write (the notice that is never a wake):
//! ```
//! use bpp_core::ops::AttentionNoticeRecordRequest;
//! let body = serde_json::json!({
//!     "version": "0.2", "op": "attention_notice_record",
//!     "meta": {"request_id": "r", "idempotency_key": "k",
//!              "expected_endpoint_incarnation": "inc",
//!              "expected_recovery_epoch": 0},
//!     "source_protocol": "kovee",
//!     "source_endpoint_ref": "kovee-endpoint-1",
//!     "source_event_ref": "kovee-event-9",
//!     "source_event_digest": {"class": "portable_public",
//!         "algorithm": "sha-256", "value_hex": "a".repeat(64)},
//!     "activity_stream_ref": "act-1", "generation": 1,
//!     "stable_notice_key": "notice-1"});
//! let req = AttentionNoticeRecordRequest::parse(&body).unwrap();
//! assert_eq!(req.source_protocol, "kovee");
//! // A notice carries NO wake member at all: there is no field through
//! // which attention could author an interest (§11.1, family contract L25).
//! let mut wake = body.clone();
//! wake.as_object_mut().unwrap()
//!     .insert("wake_intent_ref".into(), serde_json::json!("wake-1"));
//! assert!(AttentionNoticeRecordRequest::parse(&wake).is_err());
//! ```

use serde::Deserialize;
use serde_json::Value;

use super::{
    check_create_meta, check_id_array, check_identifier, check_local_erasure_safe, check_op,
    check_opt_identifier, check_opt_local_erasure_safe, check_timestamp, check_update_meta,
    check_version, parse_closed,
};
use crate::canonical::SAFE_MAX;
use crate::digest::{DigestClass, DigestRef};
use crate::envelope::MutationMeta;

fn check_safe(name: &str, v: u64) -> Result<(), String> {
    if v > SAFE_MAX {
        return Err(format!("{name} exceeds the safe range"));
    }
    Ok(())
}

fn check_portable(name: &str, d: &DigestRef) -> Result<(), String> {
    d.require_class(DigestClass::PortablePublic)
        .map_err(|e| format!("{name}: {e}"))
}

// ---------------------------------------------------- onboarding_offer ----

/// onboarding_offer (governance, create; §7.4, R10): the Society funds a
/// bounded OnboardingActivationOffer against the exact MembershipOffer.
/// `max_episodes: 1`, `allowed_operations` and
/// `general_effect_and_child_authority: none` are constants of the record,
/// never request members.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OnboardingOfferRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub membership_offer_ref: String,
    pub candidate_participant_ref: String,
    pub proposed_manifestation_ref: String,
    pub proposed_manifestation_digest: DigestRef,
    pub exact_context_ref: String,
    pub exact_context_digest: DigestRef,
    pub resource_reservation_ref: String,
    #[serde(default)]
    pub onboarding_compute_intent_ref: Option<String>,
    pub expires_at: String,
    pub adopted_by_decision_ref: String,
}

impl OnboardingOfferRequest {
    pub fn parse(body: &Value) -> Result<OnboardingOfferRequest, String> {
        let req: OnboardingOfferRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "onboarding_offer")?;
        check_create_meta(&req.meta)?;
        check_identifier("membership_offer_ref", &req.membership_offer_ref)?;
        check_identifier("candidate_participant_ref", &req.candidate_participant_ref)?;
        check_identifier(
            "proposed_manifestation_ref",
            &req.proposed_manifestation_ref,
        )?;
        check_local_erasure_safe(
            "proposed_manifestation_digest",
            &req.proposed_manifestation_digest,
        )?;
        check_identifier("exact_context_ref", &req.exact_context_ref)?;
        check_local_erasure_safe("exact_context_digest", &req.exact_context_digest)?;
        check_identifier("resource_reservation_ref", &req.resource_reservation_ref)?;
        check_opt_identifier(
            "onboarding_compute_intent_ref",
            &req.onboarding_compute_intent_ref,
        )?;
        check_timestamp("expires_at", &req.expires_at)?;
        check_identifier("adopted_by_decision_ref", &req.adopted_by_decision_ref)?;
        Ok(req)
    }
}

// ------------------------------ onboarding_compute_permit_consume (R32) ----

/// onboarding_compute_permit_consume (runtime, update; §7.4, R32): the
/// Kovee model broker bound to the exact OnboardingComputeIntent, over the
/// kernel-derived one-shot key and the current onboarding fence. It mints
/// the OnboardingComputeReceipt with `max_uses: 1`; a SECOND consume under
/// the same intent is refused, and an exact retry returns the stored
/// receipt.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OnboardingComputePermitConsumeRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub compute_intent_ref: String,
    pub compute_intent_digest: DigestRef,
    pub stable_compute_key: String,
    pub onboarding_fence_epoch: u64,
    pub kovee_invocation_ref: String,
    pub provider_context_manifest_ref: String,
    pub provider_context_manifest_digest: DigestRef,
    pub disclosure_manifest_ref: String,
    pub disclosure_manifest_digest: DigestRef,
    pub model_profile_ref: String,
    pub model_profile_digest: DigestRef,
}

impl OnboardingComputePermitConsumeRequest {
    pub fn parse(body: &Value) -> Result<OnboardingComputePermitConsumeRequest, String> {
        let req: OnboardingComputePermitConsumeRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "onboarding_compute_permit_consume")?;
        check_update_meta(&req.meta)?;
        check_identifier("compute_intent_ref", &req.compute_intent_ref)?;
        check_local_erasure_safe("compute_intent_digest", &req.compute_intent_digest)?;
        check_identifier("stable_compute_key", &req.stable_compute_key)?;
        check_safe("onboarding_fence_epoch", req.onboarding_fence_epoch)?;
        check_identifier("kovee_invocation_ref", &req.kovee_invocation_ref)?;
        check_identifier(
            "provider_context_manifest_ref",
            &req.provider_context_manifest_ref,
        )?;
        check_local_erasure_safe(
            "provider_context_manifest_digest",
            &req.provider_context_manifest_digest,
        )?;
        check_identifier("disclosure_manifest_ref", &req.disclosure_manifest_ref)?;
        check_local_erasure_safe(
            "disclosure_manifest_digest",
            &req.disclosure_manifest_digest,
        )?;
        check_identifier("model_profile_ref", &req.model_profile_ref)?;
        check_local_erasure_safe("model_profile_digest", &req.model_profile_digest)?;
        Ok(req)
    }
}

// ------------------------------------- onboarding_episode_claim (R31) ----

/// onboarding_episode_claim (runtime, create; §7.4, R31): the candidate
/// workload bound to the exact offer and proposed Manifestation, under the
/// ONE offer fence. When the run is hosted it cites the
/// OnboardingComputeReceipt (ref/digest all-or-none).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OnboardingEpisodeClaimRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub onboarding_ref: String,
    pub candidate_participant_ref: String,
    pub proposed_manifestation_ref: String,
    pub proposed_manifestation_digest: DigestRef,
    pub onboarding_fence_epoch: u64,
    pub holder_runtime_binding: String,
    pub stable_claim_key: String,
    #[serde(default)]
    pub compute_receipt_ref: Option<String>,
    #[serde(default)]
    pub compute_receipt_digest: Option<DigestRef>,
}

impl OnboardingEpisodeClaimRequest {
    pub fn parse(body: &Value) -> Result<OnboardingEpisodeClaimRequest, String> {
        let req: OnboardingEpisodeClaimRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "onboarding_episode_claim")?;
        check_create_meta(&req.meta)?;
        check_identifier("onboarding_ref", &req.onboarding_ref)?;
        check_identifier("candidate_participant_ref", &req.candidate_participant_ref)?;
        check_identifier(
            "proposed_manifestation_ref",
            &req.proposed_manifestation_ref,
        )?;
        check_local_erasure_safe(
            "proposed_manifestation_digest",
            &req.proposed_manifestation_digest,
        )?;
        check_safe("onboarding_fence_epoch", req.onboarding_fence_epoch)?;
        check_identifier("holder_runtime_binding", &req.holder_runtime_binding)?;
        check_identifier("stable_claim_key", &req.stable_claim_key)?;
        check_opt_identifier("compute_receipt_ref", &req.compute_receipt_ref)?;
        check_opt_local_erasure_safe("compute_receipt_digest", &req.compute_receipt_digest)?;
        if req.compute_receipt_ref.is_some() != req.compute_receipt_digest.is_some() {
            return Err("compute_receipt ref/digest is an all-or-none pair".to_owned());
        }
        Ok(req)
    }
}

// ---------------------------------- onboarding_episode_complete (R31) ----

/// onboarding_episode_complete (runtime, update; §7.4, R31): the candidate
/// workload closes its ONE onboarding Episode. Completion is EVIDENCE
/// ONLY: runtime output is never membership assent (§16.6 item 12), so the
/// shape carries no acceptance member of any kind.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OnboardingEpisodeCompleteRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub onboarding_episode_ref: String,
    pub onboarding_ref: String,
    pub onboarding_fence_epoch: u64,
    pub outcome: String,
    pub output_refs: Vec<String>,
    pub evidence_refs: Vec<String>,
}

impl OnboardingEpisodeCompleteRequest {
    pub fn parse(body: &Value) -> Result<OnboardingEpisodeCompleteRequest, String> {
        let req: OnboardingEpisodeCompleteRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "onboarding_episode_complete")?;
        check_update_meta(&req.meta)?;
        check_identifier("onboarding_episode_ref", &req.onboarding_episode_ref)?;
        check_identifier("onboarding_ref", &req.onboarding_ref)?;
        check_safe("onboarding_fence_epoch", req.onboarding_fence_epoch)?;
        if !matches!(req.outcome.as_str(), "completed" | "failed" | "ambiguous") {
            return Err("outcome is not completed|failed|ambiguous".to_owned());
        }
        check_id_array("output_refs", &req.output_refs, 0, 64)?;
        check_id_array("evidence_refs", &req.evidence_refs, 0, 64)?;
        Ok(req)
    }
}

// -------------------------------------------- attention_notice_record ----

/// attention_notice_record (runtime, create; §11.1/§16.4, family contract
/// L25 — DERIVED operation name, gap note G47). Kovee Attention may notify
/// byom of a COMMITTED source-state change; byom records the notice as
/// evidence. There is deliberately no wake, admission, allocation, or
/// episode member: a notification can at most make a participant's OWN
/// already-submitted WakeIntent eligible under its already-adopted
/// ActivationPolicy, and the effect is server-computed.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttentionNoticeRecordRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub source_protocol: String,
    pub source_endpoint_ref: String,
    pub source_event_ref: String,
    pub source_event_digest: DigestRef,
    pub activity_stream_ref: String,
    pub generation: u64,
    pub stable_notice_key: String,
}

impl AttentionNoticeRecordRequest {
    pub fn parse(body: &Value) -> Result<AttentionNoticeRecordRequest, String> {
        let req: AttentionNoticeRecordRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "attention_notice_record")?;
        check_create_meta(&req.meta)?;
        if req.source_protocol != "kovee" {
            return Err("source_protocol is not the closed value \"kovee\"".to_owned());
        }
        check_identifier("source_endpoint_ref", &req.source_endpoint_ref)?;
        check_identifier("source_event_ref", &req.source_event_ref)?;
        check_portable("source_event_digest", &req.source_event_digest)?;
        check_identifier("activity_stream_ref", &req.activity_stream_ref)?;
        check_safe("generation", req.generation)?;
        check_identifier("stable_notice_key", &req.stable_notice_key)?;
        Ok(req)
    }
}

// ---------------------------------------------- context_manifest_show ----

/// context_manifest_show (projection, read; §12.1, R4): the byom source
/// fields Kovee binds into its ProviderContextManifest before any model
/// call. Possession grants nothing — the read rechecks the exact
/// Episode/attempt binding and the exact ContextManifest ref.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextManifestShowRequest {
    pub version: String,
    pub op: String,
    pub episode_ref: String,
    pub byom_attempt_ref: String,
    pub context_manifest_ref: String,
}

impl ContextManifestShowRequest {
    pub fn parse(body: &Value) -> Result<ContextManifestShowRequest, String> {
        let req: ContextManifestShowRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "context_manifest_show")?;
        check_identifier("episode_ref", &req.episode_ref)?;
        check_identifier("byom_attempt_ref", &req.byom_attempt_ref)?;
        check_identifier("context_manifest_ref", &req.context_manifest_ref)?;
        Ok(req)
    }
}
