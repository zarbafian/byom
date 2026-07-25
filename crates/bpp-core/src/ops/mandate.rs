//! Mandate-chain request shapes (§10.1/§10.2; registry R15–R17):
//! server-prepared proposals, the dual-surface position, issue with
//! budget reservation, never-widening derivation, hold and revoke.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    check_create_meta, check_id_array, check_identifier, check_local_erasure_safe, check_op,
    check_opt_bpa1, check_opt_identifier, check_position_value, check_timestamp, check_update_meta,
    check_version, parse_closed,
};
use crate::digest::DigestRef;
use crate::envelope::{is_operation_id, MutationMeta};

/// The verbatim §10.1 delegation ceiling.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Delegation {
    pub allowed: bool,
    pub max_depth: u64,
    pub max_children: u64,
    pub grantee_selectors: Vec<String>,
}

impl Delegation {
    pub fn validate(&self, name: &str) -> Result<(), String> {
        if self.max_depth > crate::canonical::SAFE_MAX
            || self.max_children > crate::canonical::SAFE_MAX
        {
            return Err(format!("{name} ceiling exceeds the safe range"));
        }
        check_id_array(
            &format!("{name}.grantee_selectors"),
            &self.grantee_selectors,
            0,
            256,
        )
    }
}

fn check_allowed_operations(ops: &[String]) -> Result<(), String> {
    if ops.is_empty() || ops.len() > 256 {
        return Err("allowed_operations is out of bounds (1..=256)".to_owned());
    }
    let mut seen = std::collections::BTreeSet::new();
    for op in ops {
        if !is_operation_id(op) {
            return Err("allowed_operations item is not an operation id".to_owned());
        }
        if !seen.insert(op) {
            return Err("allowed_operations items must be unique".to_owned());
        }
    }
    Ok(())
}

/// mandate_prepare (participant, create; v2).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MandatePrepareRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub grantee_participant_ref: String,
    #[serde(default)]
    pub issuer_ref: Option<String>,
    pub purpose_ref: String,
    pub allowed_operations: Vec<String>,
    pub resource_selectors: Vec<String>,
    pub data_class_selectors: Vec<String>,
    pub destination_selectors: Vec<String>,
    #[serde(default)]
    pub context_ceiling_ref: Option<String>,
    pub budget_ceiling_set_ref: String,
    pub concurrency_ceiling: u64,
    #[serde(default)]
    pub manifestation_selector: Option<Value>,
    pub delegation: Delegation,
    #[serde(default)]
    pub pledge_ref: Option<String>,
    pub expires_at: String,
}

impl MandatePrepareRequest {
    pub fn parse(body: &Value) -> Result<MandatePrepareRequest, String> {
        let req: MandatePrepareRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "mandate_prepare")?;
        check_create_meta(&req.meta)?;
        check_identifier("grantee_participant_ref", &req.grantee_participant_ref)?;
        check_opt_identifier("issuer_ref", &req.issuer_ref)?;
        check_identifier("purpose_ref", &req.purpose_ref)?;
        check_allowed_operations(&req.allowed_operations)?;
        check_id_array("resource_selectors", &req.resource_selectors, 0, 256)?;
        check_id_array("data_class_selectors", &req.data_class_selectors, 0, 256)?;
        check_id_array("destination_selectors", &req.destination_selectors, 0, 256)?;
        check_opt_identifier("context_ceiling_ref", &req.context_ceiling_ref)?;
        check_identifier("budget_ceiling_set_ref", &req.budget_ceiling_set_ref)?;
        if req.concurrency_ceiling > crate::canonical::SAFE_MAX {
            return Err("concurrency_ceiling exceeds the safe range".to_owned());
        }
        check_opt_bpa1("manifestation_selector", &req.manifestation_selector)?;
        req.delegation.validate("delegation")?;
        check_opt_identifier("pledge_ref", &req.pledge_ref)?;
        check_timestamp("expires_at", &req.expires_at)?;
        Ok(req)
    }
}

/// mandate_derive (participant, create; v2): the never-widening child.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MandateDeriveRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub parent_mandate_ref: String,
    pub parent_mandate_revision: u64,
    pub parent_mandate_digest: DigestRef,
    pub grantee_participant_ref: String,
    pub purpose_ref: String,
    #[serde(default)]
    pub allowed_operations: Option<Vec<String>>,
    #[serde(default)]
    pub resource_selectors: Option<Vec<String>>,
    #[serde(default)]
    pub data_class_selectors: Option<Vec<String>>,
    #[serde(default)]
    pub destination_selectors: Option<Vec<String>>,
    #[serde(default)]
    pub context_ceiling_ref: Option<String>,
    pub budget_ceiling_set_ref: String,
    pub concurrency_ceiling: u64,
    #[serde(default)]
    pub manifestation_selector: Option<Value>,
    pub delegation: Delegation,
    #[serde(default)]
    pub pledge_ref: Option<String>,
    pub expires_at: String,
}

impl MandateDeriveRequest {
    pub fn parse(body: &Value) -> Result<MandateDeriveRequest, String> {
        let req: MandateDeriveRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "mandate_derive")?;
        check_create_meta(&req.meta)?;
        check_identifier("parent_mandate_ref", &req.parent_mandate_ref)?;
        if req.parent_mandate_revision > crate::canonical::SAFE_MAX {
            return Err("parent_mandate_revision exceeds the safe range".to_owned());
        }
        check_local_erasure_safe("parent_mandate_digest", &req.parent_mandate_digest)?;
        check_identifier("grantee_participant_ref", &req.grantee_participant_ref)?;
        check_identifier("purpose_ref", &req.purpose_ref)?;
        if let Some(ops) = &req.allowed_operations {
            check_allowed_operations(ops)?;
        }
        for (name, sel) in [
            ("resource_selectors", &req.resource_selectors),
            ("data_class_selectors", &req.data_class_selectors),
            ("destination_selectors", &req.destination_selectors),
        ] {
            if let Some(sel) = sel {
                check_id_array(name, sel, 0, 256)?;
            }
        }
        check_opt_identifier("context_ceiling_ref", &req.context_ceiling_ref)?;
        check_identifier("budget_ceiling_set_ref", &req.budget_ceiling_set_ref)?;
        if req.concurrency_ceiling > crate::canonical::SAFE_MAX {
            return Err("concurrency_ceiling exceeds the safe range".to_owned());
        }
        check_opt_bpa1("manifestation_selector", &req.manifestation_selector)?;
        req.delegation.validate("delegation")?;
        check_opt_identifier("pledge_ref", &req.pledge_ref)?;
        check_timestamp("expires_at", &req.expires_at)?;
        Ok(req)
    }
}

/// The shared position request shape (mandate/endeavor/pledge/charter;
/// §14.6 position family). `participant_ref` is channel-derived and never
/// a request field — the closed shape rejects it. Per-operation policy:
/// pledge couples `assent_mode`/`derived_assent_receipt_ref` in a closed
/// oneOf; charter forbids the assent-mode members entirely.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PositionRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub proposal_ref: String,
    pub proposal_revision: u64,
    pub subject_digest: DigestRef,
    pub seat_ref: String,
    pub value: String,
    #[serde(default)]
    pub reason_ref: Option<String>,
    #[serde(default)]
    pub prior_position_digest: Option<DigestRef>,
    #[serde(default)]
    pub target_status: Option<String>,
    #[serde(default)]
    pub assent_mode: Option<String>,
    #[serde(default)]
    pub derived_assent_receipt_ref: Option<String>,
}

/// Which assent-mode members a position operation admits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionAssentRule {
    /// Flat optionals (mandate_position, endeavor_position).
    Flat,
    /// The pledge oneOf: policy-derived modes REQUIRE the receipt,
    /// direct modes FORBID it, or neither member present.
    PledgeCoupled,
    /// charter_position: no assent-mode members at all.
    Forbidden,
}

impl PositionRequest {
    pub fn parse(
        body: &Value,
        op: &str,
        rule: PositionAssentRule,
    ) -> Result<PositionRequest, String> {
        let req: PositionRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, op)?;
        check_create_meta(&req.meta)?;
        check_identifier("proposal_ref", &req.proposal_ref)?;
        if req.proposal_revision > crate::canonical::SAFE_MAX {
            return Err("proposal_revision exceeds the safe range".to_owned());
        }
        check_local_erasure_safe("subject_digest", &req.subject_digest)?;
        check_identifier("seat_ref", &req.seat_ref)?;
        check_position_value(&req.value)?;
        check_opt_identifier("reason_ref", &req.reason_ref)?;
        if let Some(d) = &req.prior_position_digest {
            check_local_erasure_safe("prior_position_digest", d)?;
        }
        if let Some(status) = &req.target_status {
            if !matches!(status.as_str(), "active" | "withdrawn") {
                return Err("target_status is not active|withdrawn".to_owned());
            }
        }
        const MODES: [&str; 5] = [
            "direct_participant",
            "controller_mediated_direct",
            "participant_policy_derived",
            "candidate_policy_derived",
            "controller_policy_derived",
        ];
        if let Some(mode) = &req.assent_mode {
            if !MODES.contains(&mode.as_str()) {
                return Err("assent_mode is not a closed assent mode".to_owned());
            }
        }
        check_opt_identifier(
            "derived_assent_receipt_ref",
            &req.derived_assent_receipt_ref,
        )?;
        match rule {
            PositionAssentRule::Flat => {}
            PositionAssentRule::PledgeCoupled => {
                match (&req.assent_mode, &req.derived_assent_receipt_ref) {
                    (None, None) => {}
                    (None, Some(_)) => {
                        return Err(
                            "derived_assent_receipt_ref without assent_mode fails the closed oneOf"
                                .to_owned(),
                        )
                    }
                    (Some(mode), receipt) => {
                        let derived = mode.ends_with("_policy_derived");
                        if derived && receipt.is_none() {
                            return Err(
                                "policy-derived assent_mode requires derived_assent_receipt_ref"
                                    .to_owned(),
                            );
                        }
                        if !derived && receipt.is_some() {
                            return Err(
                                "direct assent_mode forbids derived_assent_receipt_ref".to_owned()
                            );
                        }
                    }
                }
            }
            PositionAssentRule::Forbidden => {
                if req.assent_mode.is_some() || req.derived_assent_receipt_ref.is_some() {
                    return Err(
                        "assent-mode members do not exist on this position schema".to_owned()
                    );
                }
            }
        }
        Ok(req)
    }
}

/// mandate_issue (governance, update).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MandateIssueRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub mandate_id: String,
    pub subject_digest: DigestRef,
}

impl MandateIssueRequest {
    pub fn parse(body: &Value) -> Result<MandateIssueRequest, String> {
        let req: MandateIssueRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "mandate_issue")?;
        check_update_meta(&req.meta)?;
        check_identifier("mandate_id", &req.mandate_id)?;
        check_local_erasure_safe("subject_digest", &req.subject_digest)?;
        Ok(req)
    }
}

/// mandate_hold / mandate_revoke (governance, update).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MandateHoldRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub mandate_id: String,
    pub held_by_decision_ref: String,
    #[serde(default)]
    pub hold_reason_ref: Option<String>,
}

impl MandateHoldRequest {
    pub fn parse(body: &Value) -> Result<MandateHoldRequest, String> {
        let req: MandateHoldRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "mandate_hold")?;
        check_update_meta(&req.meta)?;
        check_identifier("mandate_id", &req.mandate_id)?;
        check_identifier("held_by_decision_ref", &req.held_by_decision_ref)?;
        check_opt_identifier("hold_reason_ref", &req.hold_reason_ref)?;
        Ok(req)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MandateRevokeRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub mandate_id: String,
    pub revoked_by_decision_ref: String,
    #[serde(default)]
    pub revocation_reason_ref: Option<String>,
}

impl MandateRevokeRequest {
    pub fn parse(body: &Value) -> Result<MandateRevokeRequest, String> {
        let req: MandateRevokeRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "mandate_revoke")?;
        check_update_meta(&req.meta)?;
        check_identifier("mandate_id", &req.mandate_id)?;
        check_identifier("revoked_by_decision_ref", &req.revoked_by_decision_ref)?;
        check_opt_identifier("revocation_reason_ref", &req.revocation_reason_ref)?;
        Ok(req)
    }
}
