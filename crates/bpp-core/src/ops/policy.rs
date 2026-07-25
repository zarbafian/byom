//! Self-policy request shapes (§7.3; registry R13/R11): the
//! participant-owned assent/activation policy adoptions and revocations
//! (v2 schemas — every authority-bearing field is a canonical BPA-1
//! expression or an exact digest-pinned reference), the candidate's
//! pre-admission self-policy proposal, and `continuity_root_update`.

use serde::Deserialize;
use serde_json::Value;

use super::{
    check_bpa1, check_create_meta, check_id_array, check_identifier, check_local_erasure_safe,
    check_op, check_opt_bpa1, check_opt_identifier, check_opt_local_erasure_safe, check_timestamp,
    check_update_meta, check_version, parse_closed,
};
use crate::bpa1;
use crate::digest::{DigestClass, DigestRef};
use crate::envelope::MutationMeta;

const ADOPTION_MODES: [&str; 4] = [
    "direct_participant",
    "controller_mediated",
    "direct_candidate",
    "controller_mediated_candidate",
];

fn check_adoption_mode(v: &str) -> Result<(), String> {
    if ADOPTION_MODES.contains(&v) {
        Ok(())
    } else {
        Err("adoption_mode is not a closed adoption mode".to_owned())
    }
}

/// assent_policy_adopt (participant, create; v2).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssentPolicyAdoptRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub proposal_kind_set: Vec<String>,
    pub endeavor_selectors: Vec<String>,
    pub beneficiary_selectors: Vec<String>,
    pub outcome_and_evidence_schema_selectors: Vec<String>,
    pub terms_constraints: Value,
    pub minimum_cancellation_rights: super::TermsRef,
    pub context_and_disclosure_ceilings: Value,
    pub budget_and_obligation_ceilings: Value,
    pub allowed_manifestation_selector: Value,
    pub maximum_derived_assents: u64,
    pub rate_limit: Value,
    pub adoption_mode: String,
    pub adoption_control_domain_ref: String,
    pub adoption_control_domain_digest: DigestRef,
    pub root_authentication_evidence_ref: String,
    pub effective_at: String,
    pub expires_at: String,
    #[serde(default)]
    pub previous_digest: Option<DigestRef>,
}

impl AssentPolicyAdoptRequest {
    pub fn parse(body: &Value) -> Result<AssentPolicyAdoptRequest, String> {
        let req: AssentPolicyAdoptRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "assent_policy_adopt")?;
        check_create_meta(&req.meta)?;
        check_id_array("proposal_kind_set", &req.proposal_kind_set, 1, 256)?;
        check_id_array("endeavor_selectors", &req.endeavor_selectors, 0, 256)?;
        check_id_array("beneficiary_selectors", &req.beneficiary_selectors, 0, 256)?;
        check_id_array(
            "outcome_and_evidence_schema_selectors",
            &req.outcome_and_evidence_schema_selectors,
            0,
            256,
        )?;
        check_bpa1("terms_constraints", &req.terms_constraints)?;
        req.minimum_cancellation_rights
            .validate("minimum_cancellation_rights")?;
        check_bpa1(
            "context_and_disclosure_ceilings",
            &req.context_and_disclosure_ceilings,
        )?;
        check_bpa1(
            "budget_and_obligation_ceilings",
            &req.budget_and_obligation_ceilings,
        )?;
        check_bpa1(
            "allowed_manifestation_selector",
            &req.allowed_manifestation_selector,
        )?;
        if req.maximum_derived_assents > crate::canonical::SAFE_MAX {
            return Err("maximum_derived_assents exceeds the safe range".to_owned());
        }
        bpa1::validate_rate_atom(&req.rate_limit).map_err(|e| format!("rate_limit: {e}"))?;
        check_adoption_mode(&req.adoption_mode)?;
        check_identifier(
            "adoption_control_domain_ref",
            &req.adoption_control_domain_ref,
        )?;
        check_local_erasure_safe(
            "adoption_control_domain_digest",
            &req.adoption_control_domain_digest,
        )?;
        check_identifier(
            "root_authentication_evidence_ref",
            &req.root_authentication_evidence_ref,
        )?;
        check_timestamp("effective_at", &req.effective_at)?;
        check_timestamp("expires_at", &req.expires_at)?;
        check_opt_local_erasure_safe("previous_digest", &req.previous_digest)?;
        Ok(req)
    }
}

/// activation_policy_adopt (participant, create; v2).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationPolicyAdoptRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub activity_kind_set: Vec<String>,
    pub interest_and_event_selectors: Vec<String>,
    pub purpose_and_context_ceilings: Value,
    pub mandate_selectors: Vec<String>,
    pub budget_rate_and_concurrency_ceilings: Value,
    pub allowed_manifestation_selector: Value,
    #[serde(default)]
    pub schedule_constraints: Option<Value>,
    pub adoption_mode: String,
    pub adoption_control_domain_ref: String,
    pub adoption_control_domain_digest: DigestRef,
    pub root_authentication_evidence_ref: String,
    pub effective_at: String,
    pub expires_at: String,
    #[serde(default)]
    pub previous_digest: Option<DigestRef>,
}

impl ActivationPolicyAdoptRequest {
    pub fn parse(body: &Value) -> Result<ActivationPolicyAdoptRequest, String> {
        let req: ActivationPolicyAdoptRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "activation_policy_adopt")?;
        check_create_meta(&req.meta)?;
        check_id_array("activity_kind_set", &req.activity_kind_set, 1, 256)?;
        check_id_array(
            "interest_and_event_selectors",
            &req.interest_and_event_selectors,
            0,
            256,
        )?;
        check_bpa1(
            "purpose_and_context_ceilings",
            &req.purpose_and_context_ceilings,
        )?;
        check_id_array("mandate_selectors", &req.mandate_selectors, 0, 256)?;
        check_bpa1(
            "budget_rate_and_concurrency_ceilings",
            &req.budget_rate_and_concurrency_ceilings,
        )?;
        check_bpa1(
            "allowed_manifestation_selector",
            &req.allowed_manifestation_selector,
        )?;
        check_opt_bpa1("schedule_constraints", &req.schedule_constraints)?;
        check_adoption_mode(&req.adoption_mode)?;
        check_identifier(
            "adoption_control_domain_ref",
            &req.adoption_control_domain_ref,
        )?;
        check_local_erasure_safe(
            "adoption_control_domain_digest",
            &req.adoption_control_domain_digest,
        )?;
        check_identifier(
            "root_authentication_evidence_ref",
            &req.root_authentication_evidence_ref,
        )?;
        check_timestamp("effective_at", &req.effective_at)?;
        check_timestamp("expires_at", &req.expires_at)?;
        check_opt_local_erasure_safe("previous_digest", &req.previous_digest)?;
        Ok(req)
    }
}

/// assent_policy_revoke / activation_policy_revoke (participant, update).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyRevokeRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub policy_ref: String,
}

impl PolicyRevokeRequest {
    pub fn parse(body: &Value, op: &str) -> Result<PolicyRevokeRequest, String> {
        let req: PolicyRevokeRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, op)?;
        check_update_meta(&req.meta)?;
        check_identifier("policy_ref", &req.policy_ref)?;
        Ok(req)
    }
}

/// candidate_self_policy_propose (candidate, create; v2).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateSelfPolicyProposeRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub onboarding_ref: String,
    pub proposed_policy_kind: String,
    pub proposed_policy_body: Value,
    pub proposed_policy_digest: DigestRef,
    pub adoption_mode: String,
    pub adoption_control_domain_ref: String,
}

impl CandidateSelfPolicyProposeRequest {
    pub fn parse(body: &Value) -> Result<CandidateSelfPolicyProposeRequest, String> {
        let req: CandidateSelfPolicyProposeRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "candidate_self_policy_propose")?;
        check_create_meta(&req.meta)?;
        check_identifier("onboarding_ref", &req.onboarding_ref)?;
        if !matches!(
            req.proposed_policy_kind.as_str(),
            "assent" | "activation" | "continuity"
        ) {
            return Err("proposed_policy_kind is not a closed policy kind".to_owned());
        }
        check_bpa1("proposed_policy_body", &req.proposed_policy_body)?;
        check_local_erasure_safe("proposed_policy_digest", &req.proposed_policy_digest)?;
        if !matches!(
            req.adoption_mode.as_str(),
            "direct_candidate" | "controller_mediated_candidate"
        ) {
            return Err("adoption_mode is not a candidate adoption mode".to_owned());
        }
        check_identifier(
            "adoption_control_domain_ref",
            &req.adoption_control_domain_ref,
        )?;
        Ok(req)
    }
}

/// continuity_root_update (participant, update; v2, G11 discriminated).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContinuityRootUpdateRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub target_status: String,
    #[serde(default)]
    pub continuity_root_ref: Option<String>,
    #[serde(default)]
    pub opaque_provider_ref: Option<String>,
    #[serde(default)]
    pub current_state_ref: Option<String>,
    #[serde(default)]
    pub current_state_digest: Option<DigestRef>,
    #[serde(default)]
    pub compatibility_selector: Option<Value>,
    #[serde(default)]
    pub classification_ref: Option<String>,
    #[serde(default)]
    pub declared_influence_classes: Option<Vec<String>>,
    #[serde(default)]
    pub retention_policy_ref: Option<String>,
    #[serde(default)]
    pub adoption_mode: Option<String>,
    #[serde(default)]
    pub adoption_control_domain_ref: Option<String>,
}

impl ContinuityRootUpdateRequest {
    pub fn parse(body: &Value) -> Result<ContinuityRootUpdateRequest, String> {
        let req: ContinuityRootUpdateRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "continuity_root_update")?;
        check_update_meta(&req.meta)?;
        if !matches!(req.target_status.as_str(), "active" | "sealed" | "retired") {
            return Err("target_status is not a closed status".to_owned());
        }
        check_opt_identifier("continuity_root_ref", &req.continuity_root_ref)?;
        check_opt_identifier("opaque_provider_ref", &req.opaque_provider_ref)?;
        check_opt_identifier("current_state_ref", &req.current_state_ref)?;
        if let Some(d) = &req.current_state_digest {
            d.require_class(DigestClass::CiphertextPublic)
                .map_err(|e| format!("current_state_digest: {e}"))?;
        }
        check_opt_bpa1("compatibility_selector", &req.compatibility_selector)?;
        check_opt_identifier("classification_ref", &req.classification_ref)?;
        if let Some(classes) = &req.declared_influence_classes {
            check_id_array("declared_influence_classes", classes, 0, 256)?;
        }
        check_opt_identifier("retention_policy_ref", &req.retention_policy_ref)?;
        if let Some(mode) = &req.adoption_mode {
            check_adoption_mode(mode)?;
        }
        check_opt_identifier(
            "adoption_control_domain_ref",
            &req.adoption_control_domain_ref,
        )?;
        Ok(req)
    }
}

/// participation_cease (participant, update; R12): self-only,
/// argument-free besides the envelope — the affected Participant is
/// channel-derived; a "conditional exit" member fails the closed schema.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParticipationCeaseRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    #[serde(default)]
    pub statement_ref: Option<String>,
}

impl ParticipationCeaseRequest {
    pub fn parse(body: &Value) -> Result<ParticipationCeaseRequest, String> {
        let req: ParticipationCeaseRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "participation_cease")?;
        check_update_meta(&req.meta)?;
        check_opt_identifier("statement_ref", &req.statement_ref)?;
        Ok(req)
    }
}
