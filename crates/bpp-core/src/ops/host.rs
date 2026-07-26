//! The C2 host-integration operation shapes (DESIGN.md §16.3 verbatim;
//! registry rows R39 `kovee_endeavor_form`, R40
//! `external_command_terminalize`, R42
//! `external_command_result_query`), each the BPP envelope plus the
//! frozen `spec/governed-work/` argument members — nothing else.
//!
//! The one structural idea worth naming: `kovee_endeavor_form` splits
//! **stable semantic command bytes** from a **fresh per-attempt
//! authentication envelope**. `canonical_command_digest` covers only
//! `command`; a retry preserves those bytes (and therefore the
//! idempotency domain) while replacing only the expiring
//! proof/binding envelope.
//!
//! What you write:
//! ```
//! use bpp_core::hostint;
//! # use bpp_core::digest::DigestRef;
//! let command = serde_json::json!({"society_ref": "soc-1"});
//! // A retry changes the attempt, never the command digest.
//! assert_eq!(hostint::command_digest(&command).unwrap(),
//!            hostint::command_digest(&command).unwrap());
//! ```

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::digest::{DigestClass, DigestRef};
use crate::envelope::MutationMeta;
use crate::hostint;

use super::{check_create_meta, check_identifier, check_op, check_version};

fn parse_closed<T: for<'de> Deserialize<'de>>(body: &Value) -> Result<T, String> {
    serde_json::from_value(body.clone()).map_err(|e| e.to_string())
}

/// The cross-boundary digest class of every §16.3 field both parties
/// recompute (RT-02 contextual class binding).
fn portable(name: &str, d: &DigestRef) -> Result<(), String> {
    d.require_class(DigestClass::PortablePublic)
        .map_err(|e| format!("{name}: {e} (host-integration digests are portable_public)"))
}

/// The IdempotencyDomain digest is always the keyed `scope_erasure_safe`
/// class (§14.2; PROFILE §5) — a public class here is a conformance
/// failure.
fn domain_class(name: &str, d: &DigestRef) -> Result<(), String> {
    d.require_class(DigestClass::ScopeErasureSafe)
        .map_err(|e| format!("{name}: {e}"))
}

fn opaque(name: &str, v: &str) -> Result<(), String> {
    if v.is_empty()
        || v.len() > hostint::OPAQUE_MAX
        || !v.bytes().all(|b| (0x21..=0x7e).contains(&b))
    {
        return Err(format!("{name} is not a bounded opaque proof envelope"));
    }
    Ok(())
}

fn reason(name: &str, v: &str) -> Result<(), String> {
    if v.is_empty() || v.len() > 1024 {
        return Err(format!("{name} is out of bounds (1..=1024)"));
    }
    Ok(())
}

// ------------------------------------------- kovee_endeavor_form (R39) ----

/// `KoveeEndeavorFormCommand` (§16.3 verbatim): the stable semantic
/// command bytes. `canonical_command_digest` covers exactly this object.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KoveeEndeavorFormCommand {
    pub kovee_formation_intent_ref: String,
    pub byom_endpoint_ref: String,
    pub command_endpoint_incarnation: String,
    pub realm_byom_binding_ref: String,
    pub realm_byom_binding_revision: u64,
    pub realm_byom_binding_epoch: u64,
    pub realm_byom_binding_digest: DigestRef,
    pub society_ref: String,
    pub society_recovery_epoch: u64,
    pub source_principal_ref: String,
    pub source_actor_binding_digest: DigestRef,
    pub context_bundle_ref: String,
    pub context_bundle_digest: DigestRef,
    pub endeavor_proposal: Value,
    pub endeavor_proposal_digest: DigestRef,
    pub source_principal_position: Value,
    pub source_principal_position_digest: DigestRef,
    pub expected_governance_rule_set_ref: String,
    pub expected_slot_snapshot_digest: DigestRef,
    pub byom_command_idempotency_key: String,
    pub idempotency_domain_digest: DigestRef,
}

impl KoveeEndeavorFormCommand {
    fn validate(&self) -> Result<(), String> {
        for (name, v) in [
            (
                "kovee_formation_intent_ref",
                &self.kovee_formation_intent_ref,
            ),
            ("byom_endpoint_ref", &self.byom_endpoint_ref),
            (
                "command_endpoint_incarnation",
                &self.command_endpoint_incarnation,
            ),
            ("realm_byom_binding_ref", &self.realm_byom_binding_ref),
            ("society_ref", &self.society_ref),
            ("source_principal_ref", &self.source_principal_ref),
            ("context_bundle_ref", &self.context_bundle_ref),
            (
                "expected_governance_rule_set_ref",
                &self.expected_governance_rule_set_ref,
            ),
            (
                "byom_command_idempotency_key",
                &self.byom_command_idempotency_key,
            ),
        ] {
            check_identifier(name, v)?;
        }
        for (name, d) in [
            ("realm_byom_binding_digest", &self.realm_byom_binding_digest),
            (
                "source_actor_binding_digest",
                &self.source_actor_binding_digest,
            ),
            ("context_bundle_digest", &self.context_bundle_digest),
            ("endeavor_proposal_digest", &self.endeavor_proposal_digest),
            (
                "source_principal_position_digest",
                &self.source_principal_position_digest,
            ),
            (
                "expected_slot_snapshot_digest",
                &self.expected_slot_snapshot_digest,
            ),
        ] {
            portable(name, d)?;
        }
        domain_class("idempotency_domain_digest", &self.idempotency_domain_digest)?;
        if !self.endeavor_proposal.is_object() {
            return Err("endeavor_proposal must be the canonical proposal object".to_owned());
        }
        if !self.source_principal_position.is_object() {
            return Err("source_principal_position must be the Position object".to_owned());
        }
        Ok(())
    }

    /// The command's own canonical value (the `canonical_command_digest`
    /// preimage source).
    pub fn canonical(&self) -> Result<Value, String> {
        serde_json::to_value(self).map_err(|e| e.to_string())
    }
}

/// `kovee_endeavor_form` (governance, create; R39) — the §16.3 argument
/// envelope: the stable command plus the fresh per-attempt
/// authentication envelope. The DelegatedPrincipalCredential is CHANNEL
/// material and deliberately absent from this shape.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KoveeEndeavorFormRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub command: KoveeEndeavorFormCommand,
    pub canonical_command_digest: DigestRef,
    pub attempt_id: String,
    pub attempt_nonce: String,
    pub attempt_recovery_binding_ref: String,
    pub attempt_recovery_binding_revision: u64,
    pub attempt_recovery_binding_epoch: u64,
    pub attempt_recovery_binding_digest: DigestRef,
    pub authentication_observation_ref: String,
    pub authentication_observation_digest: DigestRef,
    pub authentication_proof: String,
}

impl KoveeEndeavorFormRequest {
    pub fn parse(body: &Value) -> Result<KoveeEndeavorFormRequest, String> {
        let req: KoveeEndeavorFormRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "kovee_endeavor_form")?;
        check_create_meta(&req.meta)?;
        req.command.validate()?;
        for (name, v) in [
            ("attempt_id", &req.attempt_id),
            ("attempt_nonce", &req.attempt_nonce),
            (
                "attempt_recovery_binding_ref",
                &req.attempt_recovery_binding_ref,
            ),
            (
                "authentication_observation_ref",
                &req.authentication_observation_ref,
            ),
        ] {
            check_identifier(name, v)?;
        }
        for (name, d) in [
            ("canonical_command_digest", &req.canonical_command_digest),
            (
                "attempt_recovery_binding_digest",
                &req.attempt_recovery_binding_digest,
            ),
            (
                "authentication_observation_digest",
                &req.authentication_observation_digest,
            ),
        ] {
            portable(name, d)?;
        }
        opaque("authentication_proof", &req.authentication_proof)?;
        // The stable/fresh split, structurally: the covered digest must
        // cover exactly the command object (§16.3).
        let canonical = req.command.canonical()?;
        if hostint::command_digest(&canonical)? != req.canonical_command_digest {
            return Err(
                "canonical_command_digest does not cover exactly the command bytes".to_owned(),
            );
        }
        // One Kovee intent, one byom key: the envelope's idempotency key
        // IS the command's (§16.3 — no hidden multi-command saga).
        if req.meta.idempotency_key != req.command.byom_command_idempotency_key {
            return Err(
                "meta.idempotency_key must equal command.byom_command_idempotency_key".to_owned(),
            );
        }
        Ok(req)
    }
}

// ------------------------------ external_command_result_query (R42) ----

/// `external_command_result_query` (projection, read; R42): the
/// read-only recovery query. It cannot submit, terminalize, modify, or
/// impersonate the original human.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalCommandResultQueryRequest {
    pub version: String,
    pub op: String,
    pub current_byom_endpoint_ref: String,
    pub current_endpoint_incarnation: String,
    pub current_recovery_binding_ref: String,
    pub current_recovery_binding_revision: u64,
    pub current_recovery_binding_epoch: u64,
    pub current_recovery_binding_digest: DigestRef,
    pub kovee_formation_intent_ref: String,
    pub target_byom_endpoint_ref: String,
    pub target_endpoint_incarnation: String,
    pub target_realm_byom_binding_ref: String,
    pub target_realm_byom_binding_revision: u64,
    pub target_realm_byom_binding_epoch: u64,
    pub target_realm_byom_binding_digest: DigestRef,
    pub target_society_ref: String,
    pub target_society_recovery_epoch: u64,
    pub source_principal_ref: String,
    pub source_actor_binding_digest: DigestRef,
    pub operation: String,
    pub byom_command_idempotency_key: String,
    pub canonical_command_digest: DigestRef,
    pub idempotency_domain_digest: DigestRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restore_lineage_proof_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restore_lineage_proof_digest: Option<DigestRef>,
}

impl ExternalCommandResultQueryRequest {
    pub fn parse(body: &Value) -> Result<ExternalCommandResultQueryRequest, String> {
        let req: ExternalCommandResultQueryRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "external_command_result_query")?;
        for (name, v) in [
            ("current_byom_endpoint_ref", &req.current_byom_endpoint_ref),
            (
                "current_endpoint_incarnation",
                &req.current_endpoint_incarnation,
            ),
            (
                "current_recovery_binding_ref",
                &req.current_recovery_binding_ref,
            ),
            (
                "kovee_formation_intent_ref",
                &req.kovee_formation_intent_ref,
            ),
            ("target_byom_endpoint_ref", &req.target_byom_endpoint_ref),
            (
                "target_endpoint_incarnation",
                &req.target_endpoint_incarnation,
            ),
            (
                "target_realm_byom_binding_ref",
                &req.target_realm_byom_binding_ref,
            ),
            ("target_society_ref", &req.target_society_ref),
            ("source_principal_ref", &req.source_principal_ref),
            ("operation", &req.operation),
            (
                "byom_command_idempotency_key",
                &req.byom_command_idempotency_key,
            ),
        ] {
            check_identifier(name, v)?;
        }
        for (name, d) in [
            (
                "current_recovery_binding_digest",
                &req.current_recovery_binding_digest,
            ),
            (
                "target_realm_byom_binding_digest",
                &req.target_realm_byom_binding_digest,
            ),
            (
                "source_actor_binding_digest",
                &req.source_actor_binding_digest,
            ),
            ("canonical_command_digest", &req.canonical_command_digest),
        ] {
            portable(name, d)?;
        }
        domain_class("idempotency_domain_digest", &req.idempotency_domain_digest)?;
        match (
            &req.restore_lineage_proof_ref,
            &req.restore_lineage_proof_digest,
        ) {
            (None, None) => {}
            (Some(r), Some(d)) => {
                check_identifier("restore_lineage_proof_ref", r)?;
                portable("restore_lineage_proof_digest", d)?;
            }
            _ => {
                return Err(
                    "restore_lineage_proof_ref and _digest are cited together or not at all"
                        .to_owned(),
                )
            }
        }
        Ok(req)
    }
}

// ------------------------------- external_command_terminalize (R40) ----

/// `external_command_terminalize` (governance, create; R40): the only
/// liveness mutation after an ambiguous formation. Create-classed, not
/// update-classed: it INSTALLS a new terminal record over an
/// IdempotencyDomain that has no head to CAS, and §16.3 fixes its answer
/// as a closed three-way union of the domain's own state — never a
/// caller-guessed revision (RT-01: a create meta carries no
/// `expected_revision`).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalCommandTerminalizeRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub kovee_formation_intent_ref: String,
    pub current_recovery_binding_ref: String,
    pub current_recovery_binding_revision: u64,
    pub current_recovery_binding_epoch: u64,
    pub current_recovery_binding_digest: DigestRef,
    pub target_byom_endpoint_ref: String,
    pub target_endpoint_incarnation: String,
    pub target_society_ref: String,
    pub target_society_recovery_epoch: u64,
    pub source_principal_ref: String,
    pub target_source_actor_binding_digest: DigestRef,
    pub current_source_actor_binding_digest: DigestRef,
    pub operation: String,
    pub byom_command_idempotency_key: String,
    pub canonical_command_digest: DigestRef,
    pub idempotency_domain_digest: DigestRef,
    pub reason: String,
    pub authentication_observation_ref: String,
    pub authentication_observation_digest: DigestRef,
    pub authentication_proof: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restore_lineage_proof_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restore_lineage_proof_digest: Option<DigestRef>,
}

impl ExternalCommandTerminalizeRequest {
    pub fn parse(body: &Value) -> Result<ExternalCommandTerminalizeRequest, String> {
        let req: ExternalCommandTerminalizeRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "external_command_terminalize")?;
        check_create_meta(&req.meta)?;
        if req.meta.idempotency_key != req.byom_command_idempotency_key {
            return Err(
                "meta.idempotency_key must equal byom_command_idempotency_key (one Kovee intent, \
                 one byom key)"
                    .to_owned(),
            );
        }
        for (name, v) in [
            (
                "kovee_formation_intent_ref",
                &req.kovee_formation_intent_ref,
            ),
            (
                "current_recovery_binding_ref",
                &req.current_recovery_binding_ref,
            ),
            ("target_byom_endpoint_ref", &req.target_byom_endpoint_ref),
            (
                "target_endpoint_incarnation",
                &req.target_endpoint_incarnation,
            ),
            ("target_society_ref", &req.target_society_ref),
            ("source_principal_ref", &req.source_principal_ref),
            ("operation", &req.operation),
            (
                "byom_command_idempotency_key",
                &req.byom_command_idempotency_key,
            ),
            (
                "authentication_observation_ref",
                &req.authentication_observation_ref,
            ),
        ] {
            check_identifier(name, v)?;
        }
        for (name, d) in [
            (
                "current_recovery_binding_digest",
                &req.current_recovery_binding_digest,
            ),
            (
                "target_source_actor_binding_digest",
                &req.target_source_actor_binding_digest,
            ),
            (
                "current_source_actor_binding_digest",
                &req.current_source_actor_binding_digest,
            ),
            ("canonical_command_digest", &req.canonical_command_digest),
            (
                "authentication_observation_digest",
                &req.authentication_observation_digest,
            ),
        ] {
            portable(name, d)?;
        }
        domain_class("idempotency_domain_digest", &req.idempotency_domain_digest)?;
        reason("reason", &req.reason)?;
        opaque("authentication_proof", &req.authentication_proof)?;
        match (
            &req.restore_lineage_proof_ref,
            &req.restore_lineage_proof_digest,
        ) {
            (None, None) => {}
            (Some(r), Some(d)) => {
                check_identifier("restore_lineage_proof_ref", r)?;
                portable("restore_lineage_proof_digest", d)?;
            }
            _ => {
                return Err(
                    "restore_lineage_proof_ref and _digest are cited together or not at all"
                        .to_owned(),
                )
            }
        }
        Ok(req)
    }
}
