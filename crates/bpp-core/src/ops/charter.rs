//! Charter request shapes (§6.2; registry R5/R6/R8/R4): a complete
//! restatement against the exact current revision, human-seat positions
//! (no assent-mode members — G41), adoption, and history.

use serde::Deserialize;
use serde_json::Value;

use super::{
    check_bpa1, check_create_meta, check_id_array, check_identifier, check_local_erasure_safe,
    check_op, check_opt_timestamp, check_update_meta, check_version, parse_closed, DecisionRuleRef,
};
use crate::digest::DigestRef;
use crate::envelope::MutationMeta;

/// charter_propose (participant, create; v2).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharterProposeRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub charter_id: String,
    pub previous_digest: DigestRef,
    pub human_sovereign_seats: Vec<String>,
    pub admission_rule: DecisionRuleRef,
    pub suspension_rule: DecisionRuleRef,
    pub obligation_disposition_rule: DecisionRuleRef,
    pub decision_rule_set: Vec<DecisionRuleRef>,
    pub delegable_power_set: Vec<String>,
    pub non_delegable_power_set: Vec<String>,
    pub standing_classes: Vec<String>,
    pub assembly_constraints: Value,
    pub mandate_constraints: Value,
    pub pledge_constraints: Value,
    pub budget_and_concurrency_ceilings: Value,
    pub data_and_retention_policy_refs: Vec<String>,
    pub emergency_hold_rule: DecisionRuleRef,
    pub dispute_rule: DecisionRuleRef,
    pub dissolution_rule: DecisionRuleRef,
    #[serde(default)]
    pub effective_at: Option<String>,
}

impl CharterProposeRequest {
    pub fn parse(body: &Value) -> Result<CharterProposeRequest, String> {
        let req: CharterProposeRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "charter_propose")?;
        check_create_meta(&req.meta)?;
        check_identifier("charter_id", &req.charter_id)?;
        check_local_erasure_safe("previous_digest", &req.previous_digest)?;
        check_id_array("human_sovereign_seats", &req.human_sovereign_seats, 1, 256)?;
        for (name, rule) in [
            ("admission_rule", &req.admission_rule),
            ("suspension_rule", &req.suspension_rule),
            (
                "obligation_disposition_rule",
                &req.obligation_disposition_rule,
            ),
            ("emergency_hold_rule", &req.emergency_hold_rule),
            ("dispute_rule", &req.dispute_rule),
            ("dissolution_rule", &req.dissolution_rule),
        ] {
            rule.validate(name)?;
        }
        if req.decision_rule_set.is_empty() || req.decision_rule_set.len() > 256 {
            return Err("decision_rule_set is out of bounds (1..=256)".to_owned());
        }
        for rule in &req.decision_rule_set {
            rule.validate("decision_rule_set item")?;
        }
        check_id_array("delegable_power_set", &req.delegable_power_set, 0, 256)?;
        check_id_array(
            "non_delegable_power_set",
            &req.non_delegable_power_set,
            1,
            256,
        )?;
        check_id_array("standing_classes", &req.standing_classes, 1, 256)?;
        check_bpa1("assembly_constraints", &req.assembly_constraints)?;
        check_bpa1("mandate_constraints", &req.mandate_constraints)?;
        check_bpa1("pledge_constraints", &req.pledge_constraints)?;
        check_bpa1(
            "budget_and_concurrency_ceilings",
            &req.budget_and_concurrency_ceilings,
        )?;
        check_id_array(
            "data_and_retention_policy_refs",
            &req.data_and_retention_policy_refs,
            0,
            256,
        )?;
        check_opt_timestamp("effective_at", &req.effective_at)?;
        Ok(req)
    }
}

/// charter_finalize (governance, update).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharterFinalizeRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub charter_id: String,
    pub subject_digest: DigestRef,
}

impl CharterFinalizeRequest {
    pub fn parse(body: &Value) -> Result<CharterFinalizeRequest, String> {
        let req: CharterFinalizeRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "charter_finalize")?;
        check_update_meta(&req.meta)?;
        check_identifier("charter_id", &req.charter_id)?;
        check_local_erasure_safe("subject_digest", &req.subject_digest)?;
        Ok(req)
    }
}

/// charter_history (projection, read; paged 1..=256).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharterHistoryRequest {
    pub version: String,
    pub op: String,
    pub charter_id: String,
    #[serde(default)]
    pub continuation: Option<String>,
    pub page_size: u64,
}

impl CharterHistoryRequest {
    pub fn parse(body: &Value) -> Result<CharterHistoryRequest, String> {
        let req: CharterHistoryRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "charter_history")?;
        check_identifier("charter_id", &req.charter_id)?;
        if let Some(token) = &req.continuation {
            if token.is_empty()
                || token.len() > 4096
                || !token.bytes().all(|b| (0x21..=0x7e).contains(&b))
            {
                return Err("continuation is not an opaque visible-ASCII token".to_owned());
            }
        }
        if req.page_size < 1 || req.page_size > 256 {
            return Err("page_size is out of bounds (1..=256)".to_owned());
        }
        Ok(req)
    }
}
