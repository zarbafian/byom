//! The §13.1 act/effect request shapes (B3 slice 3; registry R19-R23 and
//! R34): `act_intent_prepare`, `act_intent_finalize`, and the one-shot
//! `execution_permit_consume`. `act_intent_position` reuses the one shared
//! closed `PositionRequest` (flat assent-mode optionals), exactly as
//! `mandate_position` does.
//!
//! What you write (the one-shot consumption; both fences always):
//! ```
//! use bpp_core::ops::ExecutionPermitConsumeRequest;
//! let d = |k: &str| serde_json::json!({
//!     "class": "local_erasure_safe", "algorithm": "hmac-sha-256",
//!     "key_ref": k, "value_hex": "9".repeat(64)});
//! let body = serde_json::json!({
//!     "version": "0.2", "op": "execution_permit_consume",
//!     "meta": {"request_id": "r", "idempotency_key": "k",
//!              "expected_endpoint_incarnation": "inc",
//!              "expected_recovery_epoch": 0, "expected_revision": 3},
//!     "stable_execution_key": "exec-key-1",
//!     "intent_ref": "intent-1", "intent_digest": d("k1"),
//!     "host_effect_ref": "kovee-effect-1", "host_effect_digest": d("k2"),
//!     "subject_digest": d("k3"),
//!     "driver_audience": "kovee-model-broker",
//!     "budget_reservation_set_ref": "rset-1",
//!     "byom_fence_epoch": 3, "host_fence_epoch": 5});
//! let req = ExecutionPermitConsumeRequest::parse(&body).unwrap();
//! assert_eq!(req.max_uses_is_one(), true);
//! // The episode ref/fence pair is all-or-none (the frozen oneOf).
//! let mut half = body.clone();
//! half.as_object_mut().unwrap()
//!     .insert("episode_ref".into(), serde_json::json!("ep-1"));
//! assert!(ExecutionPermitConsumeRequest::parse(&half).is_err());
//! ```

use serde::Deserialize;
use serde_json::Value;

use super::{
    check_create_meta, check_identifier, check_local_erasure_safe, check_op, check_opt_identifier,
    check_opt_local_erasure_safe, check_update_meta, check_version, parse_closed,
};
use crate::canonical::SAFE_MAX;
use crate::digest::DigestRef;
use crate::envelope::MutationMeta;

/// The closed Δ4 act-class list (family contract §4, verbatim), carried in
/// ActIntent subjects. `kind` stays an open identifier on the frozen wire
/// (gap note G34); a `kind` that IS one of these five compiles the Δ4
/// class subject, and only such an act can reach a class-bound driver.
pub const ACT_CLASSES: [&str; 5] = ["model_egress", "share", "outbound", "apply", "budget"];

/// The mandatory BPA-1 request domains per act class
/// (`spec/governed-work/act-class-subject.schema.json` oneOf arms,
/// transcribed verbatim — the C2 deliverable). Extra domains only narrow
/// further and are always allowed; a missing mandatory domain fails
/// closed.
pub fn mandatory_domains(act_class: &str) -> Option<&'static [&'static str]> {
    Some(match act_class {
        "model_egress" => &[
            "operation",
            "purpose",
            "binding",
            "classification",
            "quantity",
        ],
        "share" => &["operation", "purpose", "object", "classification"],
        "outbound" => &[
            "operation",
            "purpose",
            "network_destination",
            "classification",
        ],
        "apply" => &["operation", "purpose", "object", "path", "schema_evidence"],
        "budget" => &["operation", "purpose", "quantity"],
        _ => return None,
    })
}

fn check_safe(name: &str, v: u64) -> Result<(), String> {
    if v > SAFE_MAX {
        return Err(format!("{name} exceeds the safe range"));
    }
    Ok(())
}

// ------------------------------------------------- act_intent_prepare ----

/// act_intent_prepare (participant, create; R19). `intent_id`,
/// `requested_by_participant`, `actor_ref`, `subject_digest`,
/// `preconditions`, `stable_execution_key`, `budget_reservation_set_ref`,
/// the authorization dependency set, the Δ4 class subject, and
/// `expires_at` are ALL server-derived: subject atoms are compiled by the
/// kernel from the dependency closure (§10.6), never caller-shaped.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActIntentPrepareRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub kind: String,
    pub execution_kind: String,
    pub subject_ref: String,
    pub subject_revision: u64,
    #[serde(default)]
    pub endeavor_ref: Option<String>,
    #[serde(default)]
    pub pledge_ref: Option<String>,
    pub mandate_ref: String,
    pub mandate_revision: u64,
    pub mandate_digest: DigestRef,
    #[serde(default)]
    pub context_manifest_ref: Option<String>,
    #[serde(default)]
    pub context_manifest_digest: Option<DigestRef>,
    #[serde(default)]
    pub disclosure_manifest_ref: Option<String>,
    #[serde(default)]
    pub disclosure_manifest_digest: Option<DigestRef>,
    #[serde(default)]
    pub driver_audience: Option<String>,
}

impl ActIntentPrepareRequest {
    pub fn parse(body: &Value) -> Result<ActIntentPrepareRequest, String> {
        let req: ActIntentPrepareRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "act_intent_prepare")?;
        check_create_meta(&req.meta)?;
        check_identifier("kind", &req.kind)?;
        if !matches!(
            req.execution_kind.as_str(),
            "domain_transition" | "external_effect"
        ) {
            return Err("execution_kind is not domain_transition|external_effect".to_owned());
        }
        check_identifier("subject_ref", &req.subject_ref)?;
        check_safe("subject_revision", req.subject_revision)?;
        check_opt_identifier("endeavor_ref", &req.endeavor_ref)?;
        check_opt_identifier("pledge_ref", &req.pledge_ref)?;
        check_identifier("mandate_ref", &req.mandate_ref)?;
        check_safe("mandate_revision", req.mandate_revision)?;
        check_local_erasure_safe("mandate_digest", &req.mandate_digest)?;
        check_opt_identifier("context_manifest_ref", &req.context_manifest_ref)?;
        check_opt_local_erasure_safe("context_manifest_digest", &req.context_manifest_digest)?;
        check_opt_identifier("disclosure_manifest_ref", &req.disclosure_manifest_ref)?;
        check_opt_local_erasure_safe(
            "disclosure_manifest_digest",
            &req.disclosure_manifest_digest,
        )?;
        check_opt_identifier("driver_audience", &req.driver_audience)?;
        Ok(req)
    }

    /// The Δ4 act class this preparation compiles a class subject for, if
    /// `kind` names one.
    pub fn act_class(&self) -> Option<&str> {
        ACT_CLASSES
            .iter()
            .find(|c| **c == self.kind)
            .map(|c| *c as &str)
    }
}

// ------------------------------------------------ act_intent_finalize ----

/// act_intent_finalize (participant + governance, update; R22/R23):
/// deterministic finalization over the exact prepared intent digest. The
/// caller authors no seat — supplying one fails the closed shape.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActIntentFinalizeRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub intent_id: String,
    pub subject_digest: DigestRef,
}

impl ActIntentFinalizeRequest {
    pub fn parse(body: &Value) -> Result<ActIntentFinalizeRequest, String> {
        let req: ActIntentFinalizeRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "act_intent_finalize")?;
        check_update_meta(&req.meta)?;
        check_identifier("intent_id", &req.intent_id)?;
        check_local_erasure_safe("subject_digest", &req.subject_digest)?;
        Ok(req)
    }
}

// ------------------------------------------- execution_permit_consume ----

/// execution_permit_consume (runtime, update; R34): the trusted host
/// effect service bound to the exact prepared host Effect, presenting the
/// one-shot key and BOTH fences. Byom atomically rechecks charter,
/// standing, Mandate, decisions, dependencies, ceilings, expiry and both
/// fences, inserts the MandateUse once, and returns ONE immutable
/// ExecutionConsumptionReceipt (`max_uses: 1`).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionPermitConsumeRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub stable_execution_key: String,
    pub intent_ref: String,
    pub intent_digest: DigestRef,
    pub host_effect_ref: String,
    pub host_effect_digest: DigestRef,
    pub subject_digest: DigestRef,
    #[serde(default)]
    pub disclosure_manifest_ref: Option<String>,
    #[serde(default)]
    pub disclosure_digest: Option<DigestRef>,
    pub driver_audience: String,
    pub budget_reservation_set_ref: String,
    #[serde(default)]
    pub episode_ref: Option<String>,
    #[serde(default)]
    pub episode_fence_digest: Option<DigestRef>,
    pub byom_fence_epoch: u64,
    pub host_fence_epoch: u64,
}

impl ExecutionPermitConsumeRequest {
    pub fn parse(body: &Value) -> Result<ExecutionPermitConsumeRequest, String> {
        let req: ExecutionPermitConsumeRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "execution_permit_consume")?;
        check_update_meta(&req.meta)?;
        check_identifier("stable_execution_key", &req.stable_execution_key)?;
        check_identifier("intent_ref", &req.intent_ref)?;
        check_local_erasure_safe("intent_digest", &req.intent_digest)?;
        check_identifier("host_effect_ref", &req.host_effect_ref)?;
        check_local_erasure_safe("host_effect_digest", &req.host_effect_digest)?;
        check_local_erasure_safe("subject_digest", &req.subject_digest)?;
        check_opt_identifier("disclosure_manifest_ref", &req.disclosure_manifest_ref)?;
        check_opt_local_erasure_safe("disclosure_digest", &req.disclosure_digest)?;
        check_identifier("driver_audience", &req.driver_audience)?;
        check_identifier(
            "budget_reservation_set_ref",
            &req.budget_reservation_set_ref,
        )?;
        check_opt_identifier("episode_ref", &req.episode_ref)?;
        check_opt_local_erasure_safe("episode_fence_digest", &req.episode_fence_digest)?;
        check_safe("byom_fence_epoch", req.byom_fence_epoch)?;
        check_safe("host_fence_epoch", req.host_fence_epoch)?;
        // The frozen oneOf: each optional binding is an all-or-none pair.
        if req.disclosure_manifest_ref.is_some() != req.disclosure_digest.is_some() {
            return Err("disclosure_manifest ref/digest is an all-or-none pair".to_owned());
        }
        if req.episode_ref.is_some() != req.episode_fence_digest.is_some() {
            return Err("episode ref/fence digest is an all-or-none pair".to_owned());
        }
        Ok(req)
    }

    /// `max_uses` is 1 BY CONSTRUCTION — never a request field
    /// (§13.1: one-shot consumption).
    pub fn max_uses_is_one(&self) -> bool {
        true
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// The committed Δ4 taxonomy contract this module claims to implement.
    const ACT_CLASS_SUBJECT: &str =
        include_str!("../../../../spec/governed-work/act-class-subject.schema.json");

    #[test]
    fn the_mandatory_domain_table_is_the_committed_taxonomy() {
        // The C2 deliverable is the per-class mandatory-domain pinning; the
        // kernel's compiler and the frozen schema must agree EXACTLY, in
        // both directions, or a schema-valid subject could be compiled that
        // the driver refuses (or worse, one it accepts).
        let schema: Value = serde_json::from_str(ACT_CLASS_SUBJECT).unwrap();
        let arms = schema["oneOf"].as_array().expect("the closed class arms");
        assert_eq!(arms.len(), ACT_CLASSES.len(), "one arm per act class");
        for arm in arms {
            let class = arm["properties"]["act_class"]["const"]
                .as_str()
                .expect("each arm pins its class");
            assert!(ACT_CLASSES.contains(&class), "{class} is not a Δ4 class");
            let want: Vec<&str> = arm["properties"]["subject_atoms"]["required"]
                .as_array()
                .expect("each arm pins its mandatory domains")
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect();
            assert_eq!(
                mandatory_domains(class).unwrap(),
                want.as_slice(),
                "{class}: the compiler's mandatory domains diverge from the \
                 committed taxonomy"
            );
        }
        // Every class the compiler knows is an arm of the closed union.
        for class in ACT_CLASSES {
            assert!(
                arms.iter()
                    .any(|a| a["properties"]["act_class"]["const"] == class),
                "{class} has no committed arm"
            );
            assert!(mandatory_domains(class).is_some());
        }
        // And nothing else is an act class.
        assert!(mandatory_domains("legacy_tool_call").is_none());
        assert!(mandatory_domains("").is_none());
    }
}
