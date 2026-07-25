//! Governed-work request shapes (§8–§9; registry R18–R28): endeavors,
//! calls, pledges with the RT-03 slot/seat discipline and the D-RT-3
//! successor split, deliveries (pledgor-only), and reviews.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    check_bpa1, check_create_meta, check_id_array, check_identifier, check_local_erasure_safe,
    check_op, check_opt_bpa1, check_opt_identifier, check_opt_local_erasure_safe,
    check_opt_timestamp, check_timestamp, check_update_meta, check_version, parse_closed, TermsRef,
};
use crate::bpa1;
use crate::digest::DigestRef;
use crate::envelope::MutationMeta;

/// The closed RT-06 delegation ceiling (bounded variant).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DelegationCeiling {
    pub allowed: bool,
    pub max_depth: u64,
    pub max_children: u64,
    #[serde(default)]
    pub grantee_selectors: Option<Vec<String>>,
}

impl DelegationCeiling {
    fn validate(&self) -> Result<(), String> {
        if self.max_depth > 64 || self.max_children > 4096 {
            return Err("delegation_ceiling exceeds the closed bounds".to_owned());
        }
        if let Some(sel) = &self.grantee_selectors {
            check_id_array("delegation_ceiling.grantee_selectors", sel, 0, 256)?;
        }
        Ok(())
    }
}

/// The closed budget request set: each requested dimension is a BPA-1
/// quantity atom (fixed-scale safe integers; no open object, no floats).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetRequestSet {
    pub items: Vec<Value>,
}

impl BudgetRequestSet {
    fn validate(&self) -> Result<(), String> {
        if self.items.is_empty() || self.items.len() > 64 {
            return Err("budget_request_set.items is out of bounds (1..=64)".to_owned());
        }
        for (i, item) in self.items.iter().enumerate() {
            bpa1::validate_quantity_atom(item)
                .map_err(|e| format!("budget_request_set.items/{i}: {e}"))?;
        }
        Ok(())
    }
}

// ----------------------------------------------------------- endeavor ----

/// endeavor_propose (participant, create).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndeavorProposeRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub purpose_ref: String,
    pub purpose_digest: DigestRef,
    #[serde(default)]
    pub admitted_source_bundle_ref: Option<String>,
    #[serde(default)]
    pub admitted_source_bundle_digest: Option<DigestRef>,
    pub sponsor_participant_refs: Vec<String>,
    pub governance_rule_set_ref: String,
    pub outcome_schema_refs: Vec<String>,
    pub acceptance_rule_ref: String,
    pub classification_join_ref: String,
    pub budget_account_set_ref: String,
    #[serde(default)]
    pub deadline: Option<String>,
}

impl EndeavorProposeRequest {
    pub fn parse(body: &Value) -> Result<EndeavorProposeRequest, String> {
        let req: EndeavorProposeRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "endeavor_propose")?;
        check_create_meta(&req.meta)?;
        check_identifier("purpose_ref", &req.purpose_ref)?;
        check_local_erasure_safe("purpose_digest", &req.purpose_digest)?;
        check_opt_identifier(
            "admitted_source_bundle_ref",
            &req.admitted_source_bundle_ref,
        )?;
        check_opt_local_erasure_safe(
            "admitted_source_bundle_digest",
            &req.admitted_source_bundle_digest,
        )?;
        check_id_array(
            "sponsor_participant_refs",
            &req.sponsor_participant_refs,
            1,
            256,
        )?;
        check_identifier("governance_rule_set_ref", &req.governance_rule_set_ref)?;
        check_id_array("outcome_schema_refs", &req.outcome_schema_refs, 1, 256)?;
        check_identifier("acceptance_rule_ref", &req.acceptance_rule_ref)?;
        check_identifier("classification_join_ref", &req.classification_join_ref)?;
        check_identifier("budget_account_set_ref", &req.budget_account_set_ref)?;
        check_opt_timestamp("deadline", &req.deadline)?;
        Ok(req)
    }
}

/// endeavor_finalize (participant, update).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndeavorFinalizeRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub endeavor_id: String,
    pub subject_digest: DigestRef,
}

impl EndeavorFinalizeRequest {
    pub fn parse(body: &Value) -> Result<EndeavorFinalizeRequest, String> {
        let req: EndeavorFinalizeRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "endeavor_finalize")?;
        check_update_meta(&req.meta)?;
        check_identifier("endeavor_id", &req.endeavor_id)?;
        check_local_erasure_safe("subject_digest", &req.subject_digest)?;
        Ok(req)
    }
}

/// endeavor_hold / endeavor_release (participant, update).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndeavorHoldRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub endeavor_id: String,
    #[serde(default)]
    pub hold_reason_ref: Option<String>,
    #[serde(default)]
    pub release_reason_ref: Option<String>,
}

impl EndeavorHoldRequest {
    pub fn parse(body: &Value, op: &str) -> Result<EndeavorHoldRequest, String> {
        let req: EndeavorHoldRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, op)?;
        check_update_meta(&req.meta)?;
        check_identifier("endeavor_id", &req.endeavor_id)?;
        check_opt_identifier("hold_reason_ref", &req.hold_reason_ref)?;
        check_opt_identifier("release_reason_ref", &req.release_reason_ref)?;
        // Each closed schema names only its own reason member.
        if op == "endeavor_hold" && req.release_reason_ref.is_some() {
            return Err("release_reason_ref is not an endeavor_hold member".to_owned());
        }
        if op == "endeavor_release" && req.hold_reason_ref.is_some() {
            return Err("hold_reason_ref is not an endeavor_release member".to_owned());
        }
        Ok(req)
    }
}

/// endeavor_close (participant, update; target-state discriminated).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndeavorCloseRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub endeavor_id: String,
    pub target_state: String,
    #[serde(default)]
    pub closure_decision_ref: Option<String>,
    #[serde(default)]
    pub acceptance_evidence_refs: Option<Vec<String>>,
}

impl EndeavorCloseRequest {
    pub fn parse(body: &Value) -> Result<EndeavorCloseRequest, String> {
        let req: EndeavorCloseRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "endeavor_close")?;
        check_update_meta(&req.meta)?;
        check_identifier("endeavor_id", &req.endeavor_id)?;
        if !matches!(
            req.target_state.as_str(),
            "reviewing" | "fulfilled" | "failed" | "abandoned" | "dissolved"
        ) {
            return Err("target_state is not a closed endeavor terminal".to_owned());
        }
        check_opt_identifier("closure_decision_ref", &req.closure_decision_ref)?;
        if let Some(refs) = &req.acceptance_evidence_refs {
            check_id_array("acceptance_evidence_refs", refs, 0, 256)?;
        }
        Ok(req)
    }
}

// --------------------------------------------------------------- call ----

/// call_open (participant, create; v2).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CallOpenRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub endeavor_id: String,
    pub requested_outcome_schema_refs: Vec<String>,
    pub acceptance_criteria_refs: Vec<String>,
    pub evidence_requirements: Vec<String>,
    #[serde(default)]
    pub context_ceiling_ref: Option<String>,
    #[serde(default)]
    pub budget_ceiling_ref: Option<String>,
    #[serde(default)]
    pub eligible_participant_selector: Option<Value>,
    #[serde(default)]
    pub deadline: Option<String>,
    #[serde(default)]
    pub disclosure_ceiling_ref: Option<String>,
}

impl CallOpenRequest {
    pub fn parse(body: &Value) -> Result<CallOpenRequest, String> {
        let req: CallOpenRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "call_open")?;
        check_create_meta(&req.meta)?;
        check_identifier("endeavor_id", &req.endeavor_id)?;
        check_id_array(
            "requested_outcome_schema_refs",
            &req.requested_outcome_schema_refs,
            1,
            256,
        )?;
        check_id_array(
            "acceptance_criteria_refs",
            &req.acceptance_criteria_refs,
            1,
            256,
        )?;
        check_id_array("evidence_requirements", &req.evidence_requirements, 0, 256)?;
        check_opt_identifier("context_ceiling_ref", &req.context_ceiling_ref)?;
        check_opt_identifier("budget_ceiling_ref", &req.budget_ceiling_ref)?;
        check_opt_bpa1(
            "eligible_participant_selector",
            &req.eligible_participant_selector,
        )?;
        check_opt_timestamp("deadline", &req.deadline)?;
        check_opt_identifier("disclosure_ceiling_ref", &req.disclosure_ceiling_ref)?;
        Ok(req)
    }
}

/// call_withdraw (participant, update; opener only).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CallWithdrawRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub call_id: String,
}

impl CallWithdrawRequest {
    pub fn parse(body: &Value) -> Result<CallWithdrawRequest, String> {
        let req: CallWithdrawRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "call_withdraw")?;
        check_update_meta(&req.meta)?;
        check_identifier("call_id", &req.call_id)?;
        Ok(req)
    }
}

// ------------------------------------------------------------- pledge ----

/// The shared pledge terms fields (propose and amend restate them).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PledgeProposeRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub endeavor_id: String,
    #[serde(default)]
    pub call_ref: Option<String>,
    pub proposed_pledgor_ref: String,
    pub beneficiary_ref: String,
    pub exact_outcome_schema_refs: Vec<String>,
    pub acceptance_criteria_refs: Vec<String>,
    pub evidence_requirements: Vec<String>,
    pub reviewer_rule_ref: String,
    pub input_context_ref: String,
    pub input_context_digest: DigestRef,
    pub budget_request_set: BudgetRequestSet,
    #[serde(default)]
    pub disclosure_manifest_ref: Option<String>,
    pub allowed_manifestation_selector: Value,
    pub delegation_ceiling: DelegationCeiling,
    pub deadline: String,
    pub cancellation_terms: TermsRef,
    pub dependency_refs: Vec<String>,
}

impl PledgeProposeRequest {
    pub fn parse(body: &Value) -> Result<PledgeProposeRequest, String> {
        let req: PledgeProposeRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "pledge_propose")?;
        check_create_meta(&req.meta)?;
        check_identifier("endeavor_id", &req.endeavor_id)?;
        check_opt_identifier("call_ref", &req.call_ref)?;
        req.validate_terms()?;
        Ok(req)
    }

    fn validate_terms(&self) -> Result<(), String> {
        check_identifier("proposed_pledgor_ref", &self.proposed_pledgor_ref)?;
        check_identifier("beneficiary_ref", &self.beneficiary_ref)?;
        check_id_array(
            "exact_outcome_schema_refs",
            &self.exact_outcome_schema_refs,
            1,
            256,
        )?;
        check_id_array(
            "acceptance_criteria_refs",
            &self.acceptance_criteria_refs,
            1,
            256,
        )?;
        check_id_array("evidence_requirements", &self.evidence_requirements, 0, 256)?;
        check_identifier("reviewer_rule_ref", &self.reviewer_rule_ref)?;
        check_identifier("input_context_ref", &self.input_context_ref)?;
        check_local_erasure_safe("input_context_digest", &self.input_context_digest)?;
        self.budget_request_set.validate()?;
        check_opt_identifier("disclosure_manifest_ref", &self.disclosure_manifest_ref)?;
        check_bpa1(
            "allowed_manifestation_selector",
            &self.allowed_manifestation_selector,
        )?;
        self.delegation_ceiling.validate()?;
        check_timestamp("deadline", &self.deadline)?;
        self.cancellation_terms.validate("cancellation_terms")?;
        check_id_array("dependency_refs", &self.dependency_refs, 0, 256)?;
        Ok(())
    }
}

/// The exact predecessor pin of a pledge amendment (D-RT-3).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AmendmentOf {
    pub pledge_ref: String,
    pub pledge_revision: u64,
    pub prior_terms_digest: DigestRef,
}

/// pledge_amend (participant, create; v2): a separate proposed successor
/// occupying the one CAS successor slot.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PledgeAmendRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub amendment_of: AmendmentOf,
    #[serde(default)]
    pub proposed_pledgor_ref: Option<String>,
    #[serde(default)]
    pub beneficiary_ref: Option<String>,
    pub exact_outcome_schema_refs: Vec<String>,
    pub acceptance_criteria_refs: Vec<String>,
    pub evidence_requirements: Vec<String>,
    pub reviewer_rule_ref: String,
    pub input_context_ref: String,
    pub input_context_digest: DigestRef,
    pub budget_request_set: BudgetRequestSet,
    #[serde(default)]
    pub disclosure_manifest_ref: Option<String>,
    pub allowed_manifestation_selector: Value,
    pub delegation_ceiling: DelegationCeiling,
    pub deadline: String,
    pub cancellation_terms: TermsRef,
    pub dependency_refs: Vec<String>,
}

impl PledgeAmendRequest {
    pub fn parse(body: &Value) -> Result<PledgeAmendRequest, String> {
        let req: PledgeAmendRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "pledge_amend")?;
        check_create_meta(&req.meta)?;
        check_identifier("amendment_of.pledge_ref", &req.amendment_of.pledge_ref)?;
        if req.amendment_of.pledge_revision > crate::canonical::SAFE_MAX {
            return Err("amendment_of.pledge_revision exceeds the safe range".to_owned());
        }
        check_local_erasure_safe(
            "amendment_of.prior_terms_digest",
            &req.amendment_of.prior_terms_digest,
        )?;
        check_opt_identifier("proposed_pledgor_ref", &req.proposed_pledgor_ref)?;
        check_opt_identifier("beneficiary_ref", &req.beneficiary_ref)?;
        check_id_array(
            "exact_outcome_schema_refs",
            &req.exact_outcome_schema_refs,
            1,
            256,
        )?;
        check_id_array(
            "acceptance_criteria_refs",
            &req.acceptance_criteria_refs,
            1,
            256,
        )?;
        check_id_array("evidence_requirements", &req.evidence_requirements, 0, 256)?;
        check_identifier("reviewer_rule_ref", &req.reviewer_rule_ref)?;
        check_identifier("input_context_ref", &req.input_context_ref)?;
        check_local_erasure_safe("input_context_digest", &req.input_context_digest)?;
        req.budget_request_set.validate()?;
        check_opt_identifier("disclosure_manifest_ref", &req.disclosure_manifest_ref)?;
        check_bpa1(
            "allowed_manifestation_selector",
            &req.allowed_manifestation_selector,
        )?;
        req.delegation_ceiling.validate()?;
        check_timestamp("deadline", &req.deadline)?;
        req.cancellation_terms.validate("cancellation_terms")?;
        check_id_array("dependency_refs", &req.dependency_refs, 0, 256)?;
        Ok(req)
    }
}

/// pledge_finalize (participant, update): the successor CAS pair is
/// both-or-neither (D-RT-3).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PledgeFinalizeRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub proposal_ref: String,
    pub proposal_revision: u64,
    pub subject_digest: DigestRef,
    #[serde(default)]
    pub supersedes_pledge_ref: Option<String>,
    #[serde(default)]
    pub supersedes_pledge_revision: Option<u64>,
}

impl PledgeFinalizeRequest {
    pub fn parse(body: &Value) -> Result<PledgeFinalizeRequest, String> {
        let req: PledgeFinalizeRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "pledge_finalize")?;
        check_update_meta(&req.meta)?;
        check_identifier("proposal_ref", &req.proposal_ref)?;
        if req.proposal_revision > crate::canonical::SAFE_MAX {
            return Err("proposal_revision exceeds the safe range".to_owned());
        }
        check_local_erasure_safe("subject_digest", &req.subject_digest)?;
        match (&req.supersedes_pledge_ref, &req.supersedes_pledge_revision) {
            (None, None) => {}
            (Some(r), Some(rev)) => {
                check_identifier("supersedes_pledge_ref", r)?;
                if *rev > crate::canonical::SAFE_MAX {
                    return Err("supersedes_pledge_revision exceeds the safe range".to_owned());
                }
            }
            _ => {
                return Err(
                    "the successor CAS pair is both-or-neither (D-RT-3): a predecessor \
                     reference without its exact pinned revision fails the closed oneOf"
                        .to_owned(),
                )
            }
        }
        Ok(req)
    }
}

/// pledge_resume / pledge_relinquish (participant, update).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PledgeIdRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub pledge_id: String,
    #[serde(default)]
    pub statement_ref: Option<String>,
}

impl PledgeIdRequest {
    pub fn parse(body: &Value, op: &str) -> Result<PledgeIdRequest, String> {
        let req: PledgeIdRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, op)?;
        check_update_meta(&req.meta)?;
        check_identifier("pledge_id", &req.pledge_id)?;
        check_opt_identifier("statement_ref", &req.statement_ref)?;
        if op == "pledge_resume" && req.statement_ref.is_some() {
            return Err("statement_ref is not a pledge_resume member".to_owned());
        }
        Ok(req)
    }
}

// ------------------------------------------------- delivery + review ----

/// delivery_submit (participant, create; §9.5): the pledgor is
/// channel-derived — naming the deliverer fails the closed schema.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeliverySubmitRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub pledge_id: String,
    pub pledge_revision: u64,
    pub terms_digest: DigestRef,
    pub output_refs: Vec<String>,
    pub evidence_refs: Vec<String>,
    #[serde(default)]
    pub usage_digest: Option<DigestRef>,
    pub activity_stream_ref: String,
    #[serde(default)]
    pub episode_ref: Option<String>,
    #[serde(default)]
    pub byom_fence_epoch: Option<u64>,
    #[serde(default)]
    pub expected_lease_revision: Option<u64>,
}

impl DeliverySubmitRequest {
    pub fn parse(body: &Value) -> Result<DeliverySubmitRequest, String> {
        let req: DeliverySubmitRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "delivery_submit")?;
        check_create_meta(&req.meta)?;
        check_identifier("pledge_id", &req.pledge_id)?;
        if req.pledge_revision > crate::canonical::SAFE_MAX {
            return Err("pledge_revision exceeds the safe range".to_owned());
        }
        check_local_erasure_safe("terms_digest", &req.terms_digest)?;
        check_id_array("output_refs", &req.output_refs, 1, 256)?;
        check_id_array("evidence_refs", &req.evidence_refs, 0, 256)?;
        check_opt_local_erasure_safe("usage_digest", &req.usage_digest)?;
        check_identifier("activity_stream_ref", &req.activity_stream_ref)?;
        check_opt_identifier("episode_ref", &req.episode_ref)?;
        Ok(req)
    }
}

/// review_record (participant, create; exact reviewer seat only).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewRecordRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub pledge_id: String,
    pub pledge_revision: u64,
    pub delivery_id: String,
    pub reviewed_subject_digest: DigestRef,
    #[serde(default)]
    pub rubric_ref: Option<String>,
    pub outcome: String,
    #[serde(default)]
    pub rationale_ref: Option<String>,
    pub decision_or_mandate_use_ref: String,
}

impl ReviewRecordRequest {
    pub fn parse(body: &Value) -> Result<ReviewRecordRequest, String> {
        let req: ReviewRecordRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "review_record")?;
        check_create_meta(&req.meta)?;
        check_identifier("pledge_id", &req.pledge_id)?;
        if req.pledge_revision > crate::canonical::SAFE_MAX {
            return Err("pledge_revision exceeds the safe range".to_owned());
        }
        check_identifier("delivery_id", &req.delivery_id)?;
        check_local_erasure_safe("reviewed_subject_digest", &req.reviewed_subject_digest)?;
        check_opt_identifier("rubric_ref", &req.rubric_ref)?;
        if !matches!(
            req.outcome.as_str(),
            "fulfilled" | "revision_requested" | "rejected" | "disputed"
        ) {
            return Err("outcome is not a closed review outcome".to_owned());
        }
        check_opt_identifier("rationale_ref", &req.rationale_ref)?;
        check_identifier(
            "decision_or_mandate_use_ref",
            &req.decision_or_mandate_use_ref,
        )?;
        Ok(req)
    }
}
