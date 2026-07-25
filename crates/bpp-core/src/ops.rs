//! Typed, closed request/result shapes for the slice-1 operations
//! (spec/schemas/ops, frozen with the B0.1 bundle). Every request schema
//! restates the envelope fields plus that operation's exact argument
//! fields; unknown members fail closed (`deny_unknown_fields`), digest
//! fields carry their contextual class binding (RT-02), and timestamps
//! take the G3/RT-17 wire derivation.
//!
//! What you write (parse one accepted body into one op):
//! ```
//! use bpp_core::ops::MembershipAcceptRequest;
//! let body = serde_json::json!({
//!     "version": "0.2", "op": "membership_accept",
//!     "meta": {"request_id": "r", "idempotency_key": "k",
//!              "expected_endpoint_incarnation": "inc",
//!              "expected_recovery_epoch": 0, "expected_revision": 2},
//!     "offer_ref": "offer-1",
//!     "subject_digest": {"class": "local_erasure_safe",
//!         "algorithm": "hmac-sha-256", "key_ref": "k-1",
//!         "value_hex": "a".repeat(64)}});
//! let req = MembershipAcceptRequest::parse(&body).unwrap();
//! assert_eq!(req.offer_ref, "offer-1");
//! ```

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::digest::{DigestClass, DigestRef};
use crate::envelope::{is_identifier, is_protocol_version, MutationMeta};
use crate::limits::EVENTS_PAGE_ITEMS_MAX;
use crate::time::parse_rfc3339_utc;

fn parse_closed<T: for<'de> Deserialize<'de>>(body: &Value) -> Result<T, String> {
    serde_json::from_value(body.clone()).map_err(|e| e.to_string())
}

fn check_version(version: &str) -> Result<(), String> {
    if is_protocol_version(version) {
        Ok(())
    } else {
        Err("version does not match the minor-version shape".to_owned())
    }
}

fn check_op(op: &str, expected: &str) -> Result<(), String> {
    if op == expected {
        Ok(())
    } else {
        Err(format!("op must be the literal {expected:?}"))
    }
}

fn check_identifier(name: &str, v: &str) -> Result<(), String> {
    if is_identifier(v) {
        Ok(())
    } else {
        Err(format!("{name} is not a valid identifier"))
    }
}

fn check_opt_identifier(name: &str, v: &Option<String>) -> Result<(), String> {
    match v {
        Some(v) => check_identifier(name, v),
        None => Ok(()),
    }
}

fn check_local_erasure_safe(name: &str, d: &DigestRef) -> Result<(), String> {
    d.require_class(DigestClass::LocalErasureSafe)
        .map_err(|e| format!("{name}: {e}"))
}

fn check_timestamp(name: &str, v: &str) -> Result<(), String> {
    parse_rfc3339_utc(v)
        .map(|_| ())
        .ok_or_else(|| format!("{name} is not a valid UTC instant"))
}

fn check_create_meta(meta: &MutationMeta) -> Result<(), String> {
    meta.validate()?;
    if meta.expected_revision.is_some() {
        return Err("create meta carries no expected_revision member".to_owned());
    }
    Ok(())
}

fn check_update_meta(meta: &MutationMeta) -> Result<(), String> {
    meta.validate()?;
    if meta.expected_revision.is_none() {
        return Err("update meta requires expected_revision".to_owned());
    }
    Ok(())
}

// ------------------------------------------------------- negotiation ----

/// hello / protocol_info / feature_info: argument-free reads whose closed
/// schemas pin the envelope only.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NegotiationRequest {
    pub version: String,
    pub op: String,
}

impl NegotiationRequest {
    pub fn parse(body: &Value, op: &str) -> Result<NegotiationRequest, String> {
        let req: NegotiationRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, op)?;
        Ok(req)
    }
}

/// hello result (§14.1/§14.5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HelloResult {
    pub versions: Vec<String>,
    pub surface: String,
    pub endpoint_incarnation: String,
}

impl HelloResult {
    pub fn validate(&self) -> Result<(), String> {
        if self.versions.is_empty() || self.versions.len() > 64 {
            return Err("versions out of bounds".to_owned());
        }
        for v in &self.versions {
            check_version(v)?;
        }
        const SURFACES: [&str; 6] = [
            "governance",
            "candidate",
            "participant",
            "runtime",
            "projection",
            "admin",
        ];
        if !SURFACES.contains(&self.surface.as_str()) {
            return Err("surface is not an advertised §14.5 surface".to_owned());
        }
        check_identifier("endpoint_incarnation", &self.endpoint_incarnation)
    }
}

/// protocol_info result: versions plus the revisioned §14.9 limits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolInfoResult {
    pub versions: Vec<String>,
    pub limits: ProtocolLimits,
    pub limits_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolLimits {
    pub request_bytes_max: u64,
    pub response_bytes_max: u64,
    pub identifier_bytes_max: u64,
    pub mutation_list_items_max: u64,
    pub events_page_items_max: u64,
}

impl ProtocolInfoResult {
    pub fn validate(&self) -> Result<(), String> {
        if self.versions.is_empty() || self.versions.len() > 64 {
            return Err("versions out of bounds".to_owned());
        }
        for v in &self.versions {
            check_version(v)?;
        }
        Ok(())
    }
}

/// feature_info result: explicit feature bundles naming their operations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureInfoResult {
    pub features: Vec<FeatureBundle>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureBundle {
    pub feature: String,
    pub operations: Vec<String>,
}

impl FeatureInfoResult {
    pub fn validate(&self) -> Result<(), String> {
        if self.features.len() > 256 {
            return Err("too many features".to_owned());
        }
        for f in &self.features {
            check_identifier("feature", &f.feature)?;
            if f.operations.is_empty() || f.operations.len() > 512 {
                return Err("operations out of bounds".to_owned());
            }
            for op in &f.operations {
                if !crate::envelope::is_operation_id(op) {
                    return Err("operation id shape".to_owned());
                }
            }
        }
        Ok(())
    }
}

// ----------------------------------------------------------- society ----

/// society_prepare (governance, create; R2/G1).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SocietyPrepareRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub home_authority_ref: String,
    #[serde(default)]
    pub kovee_realm_binding: Option<String>,
    #[serde(default)]
    pub kovee_project_binding: Option<String>,
    pub proposed_charter_ref: String,
    pub proposed_charter_digest: DigestRef,
    pub classification_binding_ref: String,
    pub classification_binding_digest: DigestRef,
}

impl SocietyPrepareRequest {
    pub fn parse(body: &Value) -> Result<SocietyPrepareRequest, String> {
        let req: SocietyPrepareRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "society_prepare")?;
        check_create_meta(&req.meta)?;
        check_identifier("home_authority_ref", &req.home_authority_ref)?;
        check_opt_identifier("kovee_realm_binding", &req.kovee_realm_binding)?;
        check_opt_identifier("kovee_project_binding", &req.kovee_project_binding)?;
        check_identifier("proposed_charter_ref", &req.proposed_charter_ref)?;
        check_local_erasure_safe("proposed_charter_digest", &req.proposed_charter_digest)?;
        check_identifier(
            "classification_binding_ref",
            &req.classification_binding_ref,
        )?;
        check_local_erasure_safe(
            "classification_binding_digest",
            &req.classification_binding_digest,
        )?;
        Ok(req)
    }
}

/// society_bootstrap (governance, update; R2/G1).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SocietyBootstrapRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub society_id: String,
    pub preparation_ref: String,
    pub subject_digest: DigestRef,
}

impl SocietyBootstrapRequest {
    pub fn parse(body: &Value) -> Result<SocietyBootstrapRequest, String> {
        let req: SocietyBootstrapRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "society_bootstrap")?;
        check_update_meta(&req.meta)?;
        check_identifier("society_id", &req.society_id)?;
        check_identifier("preparation_ref", &req.preparation_ref)?;
        check_local_erasure_safe("subject_digest", &req.subject_digest)?;
        Ok(req)
    }
}

/// society_show (projection, read; R4).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SocietyShowRequest {
    pub version: String,
    pub op: String,
    pub society_id: String,
}

impl SocietyShowRequest {
    pub fn parse(body: &Value) -> Result<SocietyShowRequest, String> {
        let req: SocietyShowRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "society_show")?;
        check_identifier("society_id", &req.society_id)?;
        Ok(req)
    }
}

// -------------------------------------------------------- onboarding ----

/// membership_offer (governance, create; §7.4, R10).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MembershipOfferRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub participant_ref: String,
    pub proposed_standing_ref: String,
    pub subject_digest: DigestRef,
    pub offered_by_decision_ref: String,
    pub expires_at: String,
}

impl MembershipOfferRequest {
    pub fn parse(body: &Value) -> Result<MembershipOfferRequest, String> {
        let req: MembershipOfferRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "membership_offer")?;
        check_create_meta(&req.meta)?;
        check_identifier("participant_ref", &req.participant_ref)?;
        check_identifier("proposed_standing_ref", &req.proposed_standing_ref)?;
        check_local_erasure_safe("subject_digest", &req.subject_digest)?;
        check_identifier("offered_by_decision_ref", &req.offered_by_decision_ref)?;
        check_timestamp("expires_at", &req.expires_at)?;
        Ok(req)
    }
}

/// membership_accept (candidate, update; §7.4, R11).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MembershipAcceptRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub offer_ref: String,
    pub subject_digest: DigestRef,
}

impl MembershipAcceptRequest {
    pub fn parse(body: &Value) -> Result<MembershipAcceptRequest, String> {
        let req: MembershipAcceptRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "membership_accept")?;
        check_update_meta(&req.meta)?;
        check_identifier("offer_ref", &req.offer_ref)?;
        check_local_erasure_safe("subject_digest", &req.subject_digest)?;
        Ok(req)
    }
}

/// membership_refuse (candidate, update; §7.4, R11).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MembershipRefuseRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub offer_ref: String,
    pub offer_subject_digest: DigestRef,
    #[serde(default)]
    pub superseded_acceptance_ref: Option<String>,
    #[serde(default)]
    pub refusal_reason_ref: Option<String>,
}

impl MembershipRefuseRequest {
    pub fn parse(body: &Value) -> Result<MembershipRefuseRequest, String> {
        let req: MembershipRefuseRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "membership_refuse")?;
        check_update_meta(&req.meta)?;
        check_identifier("offer_ref", &req.offer_ref)?;
        check_local_erasure_safe("offer_subject_digest", &req.offer_subject_digest)?;
        check_opt_identifier("superseded_acceptance_ref", &req.superseded_acceptance_ref)?;
        check_opt_identifier("refusal_reason_ref", &req.refusal_reason_ref)?;
        Ok(req)
    }
}

/// participant_admit (governance, update; §7.4, R8).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParticipantAdmitRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub offer_ref: String,
    pub membership_acceptance_ref: String,
    pub admitted_by_decision_ref: String,
    pub admission_subject_digest: DigestRef,
    #[serde(default)]
    pub included_self_policy_proposal_refs: Option<Vec<String>>,
}

impl ParticipantAdmitRequest {
    pub fn parse(body: &Value) -> Result<ParticipantAdmitRequest, String> {
        let req: ParticipantAdmitRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "participant_admit")?;
        check_update_meta(&req.meta)?;
        check_identifier("offer_ref", &req.offer_ref)?;
        check_identifier("membership_acceptance_ref", &req.membership_acceptance_ref)?;
        check_identifier("admitted_by_decision_ref", &req.admitted_by_decision_ref)?;
        check_local_erasure_safe("admission_subject_digest", &req.admission_subject_digest)?;
        if let Some(refs) = &req.included_self_policy_proposal_refs {
            if refs.len() > 256 {
                return Err("included_self_policy_proposal_refs over 256".to_owned());
            }
            let mut seen = std::collections::BTreeSet::new();
            for r in refs {
                check_identifier("included_self_policy_proposal_refs item", r)?;
                if !seen.insert(r) {
                    return Err("included_self_policy_proposal_refs must be unique".to_owned());
                }
            }
        }
        Ok(req)
    }
}

/// manifestation_admit (governance, update; §7.3).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestationAdmitRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub manifestation_ref: String,
    pub admitted_by_decision_ref: String,
    #[serde(default)]
    pub compatibility_review_ref: Option<String>,
}

impl ManifestationAdmitRequest {
    pub fn parse(body: &Value) -> Result<ManifestationAdmitRequest, String> {
        let req: ManifestationAdmitRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "manifestation_admit")?;
        check_update_meta(&req.meta)?;
        check_identifier("manifestation_ref", &req.manifestation_ref)?;
        check_identifier("admitted_by_decision_ref", &req.admitted_by_decision_ref)?;
        check_opt_identifier("compatibility_review_ref", &req.compatibility_review_ref)?;
        Ok(req)
    }
}

/// participant_show (projection, read).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParticipantShowRequest {
    pub version: String,
    pub op: String,
    pub participant_ref: String,
}

impl ParticipantShowRequest {
    pub fn parse(body: &Value) -> Result<ParticipantShowRequest, String> {
        let req: ParticipantShowRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "participant_show")?;
        check_identifier("participant_ref", &req.participant_ref)?;
        Ok(req)
    }
}

// ------------------------------------------------------------ events ----

/// events_read (projection, read; §14.4, G38): opaque audience-bound
/// continuation plus the required explicit page size.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventsReadRequest {
    pub version: String,
    pub op: String,
    pub continuation: String,
    pub page_size: u64,
}

impl EventsReadRequest {
    pub fn parse(body: &Value) -> Result<EventsReadRequest, String> {
        let req: EventsReadRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "events_read")?;
        if req.continuation.is_empty()
            || req.continuation.len() > 4096
            || !req.continuation.bytes().all(|b| (0x21..=0x7e).contains(&b))
        {
            return Err("continuation is not an opaque visible-ASCII token".to_owned());
        }
        if req.page_size < 1 || req.page_size > EVENTS_PAGE_ITEMS_MAX {
            return Err("maximum 512 events per page (§14.9)".to_owned());
        }
        Ok(req)
    }
}
