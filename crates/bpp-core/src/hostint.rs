//! The C2 `byom_governed_work_v1` host-integration records byom
//! consumes daemon-side (DESIGN.md §16.3/§16.6; family contract §2.A/2.B
//! rows L2/L5–L6/L10–L15, R39/R40/R42), typed exactly as the FROZEN
//! `spec/governed-work/*.schema.json` shapes.
//!
//! Two things live here and nowhere else:
//!
//! 1. the record shapes (`DelegatedPrincipalCredential`,
//!    `KoveeRealmByomBinding`, `KoveeSocietyMapping`, `RestoreLineage`,
//!    `RestoreLineageProof`), closed and fail-closed on unknown members;
//! 2. the **cross-boundary digest derivations** — the `portable_public`
//!    SHA-256 tags both Kovee and byom recompute independently, so a
//!    request field "can only match the server value" (§16.3) is a
//!    machine check rather than trust.
//!
//! What you write:
//! ```
//! use bpp_core::hostint;
//! let command = serde_json::json!({"society_ref": "soc-1"});
//! let d = hostint::command_digest(&command).unwrap();
//! assert_eq!(d.class, "portable_public");
//! // Both sides derive the same bytes from the same command.
//! assert_eq!(d, hostint::command_digest(&command).unwrap());
//! ```

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::canonical::{sha256_hex, tagged_canonical};
use crate::digest::{DigestClass, DigestRef};
use crate::envelope::is_identifier;
use crate::time::parse_rfc3339_utc;

/// `$domain` tag of the stable KoveeEndeavorFormCommand bytes — the
/// preimage of `canonical_command_digest`, which covers ONLY the command
/// (never attempt id/nonce, authentication observation/proof, transport
/// request id, or send time; §16.3).
pub const COMMAND_TAG: &str = "bpp-kovee-endeavor-form-command-v0";
/// `$domain` tag of the embedded EndeavorProposal body (§16.3; the
/// body's own shape is owned by the B0.1 `endeavor_propose` subject).
pub const PROPOSAL_TAG: &str = "bpp-kovee-endeavor-proposal-v0";
/// `$domain` tag of the embedded source-principal Position body.
pub const POSITION_TAG: &str = "bpp-kovee-source-principal-position-v0";
/// `$domain` tag of the computed formation slot snapshot (§16.3: the
/// server recomputes the slot snapshot; the request can only match it).
pub const SLOT_SNAPSHOT_TAG: &str = "bpp-kovee-formation-slot-snapshot-v0";
/// `$domain` tag of the KoveeEndeavorFormResult envelope.
pub const RESULT_TAG: &str = "bpp-kovee-endeavor-form-result-v0";
/// `$domain` tag of the DelegatedPrincipalCredential (minus `digest`).
pub const CREDENTIAL_TAG: &str = "bpp-delegated-principal-credential-v0";
/// `$domain` tag of the ExternalCommandResultQuery (`query_digest`).
pub const QUERY_TAG: &str = "bpp-external-command-result-query-v0";
/// `$domain` tag of the five-fact / three-way result envelopes.
pub const QUERY_RESULT_TAG: &str = "bpp-external-command-result-query-result-v0";
pub const TERMINALIZE_RESULT_TAG: &str = "bpp-external-command-terminalize-result-v0";
/// `$domain` tag of a non-reexecuting tombstone record.
pub const TOMBSTONE_TAG: &str = "bpp-external-command-tombstone-v0";
/// `$domain` tag of a historical fence receipt (§16.3: the signed
/// receipt proving the old command can no longer arrive).
pub const FENCE_RECEIPT_TAG: &str = "bpp-historical-fence-receipt-v0";
/// `$domain` tag of the RestoreLineage evidence a historical answer cites.
pub const LINEAGE_EVIDENCE_TAG: &str = "bpp-restore-lineage-evidence-v0";
/// `$domain` tag of the `not_terminalizable` blocking evidence.
pub const BLOCKING_EVIDENCE_TAG: &str = "bpp-external-command-blocking-evidence-v0";
/// `$domain` tag of the synchronous AuthorityJournalReceipt.
pub const JOURNAL_RECEIPT_TAG: &str = "bpp-authority-journal-receipt-v0";
/// `$domain` tag of the immutable GovernanceDecision record.
pub const DECISION_TAG: &str = "bpp-governance-decision-v0";
/// `$domain` tag of the canonical Endeavor projection Kovee links to.
pub const ENDEAVOR_TAG: &str = "bpp-kovee-endeavor-projection-v0";
/// `$domain` tag of the per-attempt authentication binding (§16.3:
/// `canonical_command_digest || idempotency_domain_digest ||
/// attempt_nonce || attempt_recovery_binding_digest` and the
/// server-derived current actor binding).
pub const ATTEMPT_PROOF_TAG: &str = "bpp-kovee-attempt-authentication-v0";

/// The bounded opaque proof/signature envelope (§14.4).
pub const OPAQUE_MAX: usize = 4096;

/// A `portable_public` SHA-256 digest over the `$domain`-tagged JCS
/// bytes of one object: the cross-boundary class (PROFILE §6.2, RT-02),
/// unkeyed exactly so the counterparty can recompute it.
pub fn portable_digest(tag: &str, object: &Value) -> Result<DigestRef, String> {
    let preimage = tagged_canonical(tag, object).map_err(|e| e.to_string())?;
    Ok(DigestRef::portable_public(sha256_hex(&preimage)))
}

/// `canonical_command_digest` over the stable command bytes (§16.3).
pub fn command_digest(command: &Value) -> Result<DigestRef, String> {
    portable_digest(COMMAND_TAG, command)
}

/// The `$domain`-tagged attempt-authentication binding (§16.3). The
/// developer profile presents it as `ap1.<64 hex>`; the endpoint
/// recomputes it exactly, so a replaced command, nonce, recovery
/// binding, or actor binding cannot ride an old proof. Phishing-resistant
/// key material is honestly NOT claimed at this assurance profile.
pub fn attempt_proof(
    canonical_command_digest: &DigestRef,
    idempotency_domain_digest: &DigestRef,
    attempt_nonce: &str,
    attempt_recovery_binding_digest: &DigestRef,
    server_actor_binding_digest: &DigestRef,
) -> Result<String, String> {
    let bound = serde_json::json!({
        "canonical_command_digest": canonical_command_digest.value_hex,
        "idempotency_domain_digest": idempotency_domain_digest.value_hex,
        "attempt_nonce": attempt_nonce,
        "attempt_recovery_binding_digest": attempt_recovery_binding_digest.value_hex,
        "source_actor_binding_digest": server_actor_binding_digest.value_hex,
    });
    let preimage = tagged_canonical(ATTEMPT_PROOF_TAG, &bound).map_err(|e| e.to_string())?;
    Ok(format!("ap1.{}", sha256_hex(&preimage)))
}

/// The digest a self-describing record carries over itself: the record
/// minus its own `digest` member, `$domain`-tagged.
pub fn self_digest(tag: &str, record: &Value) -> Result<DigestRef, String> {
    let mut projected = record.clone();
    if let Some(map) = projected.as_object_mut() {
        map.remove("digest");
    }
    portable_digest(tag, &projected)
}

// ------------------------------------------------------- shape helpers ----

fn ident(name: &str, v: &str) -> Result<(), String> {
    if is_identifier(v) {
        Ok(())
    } else {
        Err(format!("{name} is not a valid identifier"))
    }
}

fn timestamp(name: &str, v: &str) -> Result<i64, String> {
    parse_rfc3339_utc(v).ok_or_else(|| format!("{name} is not a valid UTC instant"))
}

fn class(name: &str, d: &DigestRef, want: DigestClass) -> Result<(), String> {
    d.require_class(want)
        .map_err(|e| format!("{name}: {e} (expected {})", want.as_str()))
}

fn parse_closed<T: for<'de> Deserialize<'de>>(body: &Value) -> Result<T, String> {
    serde_json::from_value(body.clone()).map_err(|e| e.to_string())
}

/// The transport-preamble prefix that carries a
/// DelegatedPrincipalCredential on the governance socket. The credential
/// is CHANNEL material: the closed per-operation request schemas carry no
/// credential member (the §7.4 discipline, reused here for R39/R40).
pub const DPC_PREAMBLE_PREFIX: &str = "dpc1.";

/// Encodes a credential as its preamble line (hex keeps the line free of
/// `{`, so a preamble is never mistaken for a request).
pub fn encode_credential(credential: &DelegatedPrincipalCredential) -> Result<String, String> {
    let bytes = serde_json::to_vec(credential).map_err(|e| e.to_string())?;
    Ok(format!(
        "{DPC_PREAMBLE_PREFIX}{}",
        crate::canonical::hex(&bytes)
    ))
}

/// Decodes a preamble line. `None` means "not a credential preamble" —
/// the connection is the ordinary same-UID sovereign channel.
pub fn decode_credential(token: &str) -> Option<Result<DelegatedPrincipalCredential, String>> {
    let body = token.trim().strip_prefix(DPC_PREAMBLE_PREFIX)?;
    if body.len() % 2 != 0 {
        return Some(Err("credential preamble is not hex".to_owned()));
    }
    let mut bytes = Vec::with_capacity(body.len() / 2);
    for i in (0..body.len()).step_by(2) {
        match body
            .get(i..i + 2)
            .and_then(|p| u8::from_str_radix(p, 16).ok())
        {
            Some(b) => bytes.push(b),
            None => return Some(Err("credential preamble is not hex".to_owned())),
        }
    }
    Some(
        serde_json::from_slice::<Value>(&bytes)
            .map_err(|e| e.to_string())
            .and_then(|v| DelegatedPrincipalCredential::parse(&v)),
    )
}

// ------------------------------------- DelegatedPrincipalCredential ----

/// The §14.4 sender constraint of a delegated-principal credential.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SenderConstraint {
    pub method: String,
    pub key_binding_digest: DigestRef,
}

/// `DelegatedPrincipalCredential` — the C2 DPC profile
/// (`spec/governed-work/delegated-principal-credential.schema.json`).
/// It is CHANNEL material: byomd takes it from the transport preamble of
/// the governance socket, never from a request body (§7.4/§14.3
/// discipline), and every actor-derived value (`source_principal_ref`,
/// `source_actor_binding_digest`, `bound_participant_ref`) comes from
/// here, so a request field can only match it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DelegatedPrincipalCredential {
    pub credential_id: String,
    pub issuer_ref: String,
    pub nonce: String,
    pub sender_constraint: SenderConstraint,
    pub source_principal_ref: String,
    pub source_actor_binding_digest: DigestRef,
    pub bound_participant_ref: String,
    pub participant_binding_epoch: u64,
    pub society_ref: String,
    pub society_recovery_epoch: u64,
    pub endpoint_incarnation: String,
    pub realm_byom_binding_ref: String,
    pub realm_byom_binding_revision: u64,
    pub realm_byom_binding_epoch: u64,
    pub realm_byom_binding_digest: DigestRef,
    pub audience: String,
    pub surface: String,
    pub allowed_operations: Vec<String>,
    pub delegated_principal_subject_digest: DigestRef,
    pub authentication_observation_ref: String,
    pub authentication_observation_digest: DigestRef,
    pub assurance_level: String,
    pub issued_at: String,
    pub expires_at: String,
    pub digest: DigestRef,
}

/// The closed R39/R40 operation family a DPC may carry.
pub const DPC_OPERATIONS: [&str; 2] = ["kovee_endeavor_form", "external_command_terminalize"];

impl DelegatedPrincipalCredential {
    pub fn parse(body: &Value) -> Result<DelegatedPrincipalCredential, String> {
        let c: DelegatedPrincipalCredential = parse_closed(body)?;
        c.validate()?;
        Ok(c)
    }

    /// Every check the frozen schema expresses, plus the self-digest
    /// recomputation (a forged or edited credential fails here).
    pub fn validate(&self) -> Result<(), String> {
        for (name, v) in [
            ("credential_id", &self.credential_id),
            ("issuer_ref", &self.issuer_ref),
            ("nonce", &self.nonce),
            ("source_principal_ref", &self.source_principal_ref),
            ("bound_participant_ref", &self.bound_participant_ref),
            ("society_ref", &self.society_ref),
            ("endpoint_incarnation", &self.endpoint_incarnation),
            ("realm_byom_binding_ref", &self.realm_byom_binding_ref),
            ("audience", &self.audience),
            (
                "authentication_observation_ref",
                &self.authentication_observation_ref,
            ),
            ("assurance_level", &self.assurance_level),
        ] {
            ident(name, v)?;
        }
        if !["mtls", "dpop", "channel_exporter"].contains(&self.sender_constraint.method.as_str()) {
            return Err("sender_constraint.method is not a §14.4 method".to_owned());
        }
        self.sender_constraint
            .key_binding_digest
            .validate_wire()
            .map_err(|e| format!("sender_constraint.key_binding_digest: {e}"))?;
        class(
            "source_actor_binding_digest",
            &self.source_actor_binding_digest,
            DigestClass::PortablePublic,
        )?;
        for (name, d) in [
            ("realm_byom_binding_digest", &self.realm_byom_binding_digest),
            (
                "delegated_principal_subject_digest",
                &self.delegated_principal_subject_digest,
            ),
            (
                "authentication_observation_digest",
                &self.authentication_observation_digest,
            ),
            ("digest", &self.digest),
        ] {
            class(name, d, DigestClass::PortablePublic)?;
        }
        if self.surface != "governance" {
            return Err("surface must be \"governance\" (R39/R40 rows)".to_owned());
        }
        if self.allowed_operations.is_empty() || self.allowed_operations.len() > 2 {
            return Err("allowed_operations is out of bounds".to_owned());
        }
        let mut seen = std::collections::BTreeSet::new();
        for op in &self.allowed_operations {
            if !DPC_OPERATIONS.contains(&op.as_str()) {
                return Err(format!(
                    "allowed_operations names {op:?}, outside the closed R39/R40 family"
                ));
            }
            if !seen.insert(op) {
                return Err("allowed_operations must be unique".to_owned());
            }
        }
        let issued = timestamp("issued_at", &self.issued_at)?;
        let expires = timestamp("expires_at", &self.expires_at)?;
        if expires <= issued {
            return Err("expires_at must follow issued_at (§14.4 short expiry)".to_owned());
        }
        let value = serde_json::to_value(self).map_err(|e| e.to_string())?;
        let recomputed = self_digest(CREDENTIAL_TAG, &value)?;
        if recomputed != self.digest {
            return Err("credential digest does not cover these exact bytes".to_owned());
        }
        Ok(())
    }

    /// The credential's short-expiry window against server time.
    pub fn live_at(&self, now: i64) -> bool {
        match (
            parse_rfc3339_utc(&self.issued_at),
            parse_rfc3339_utc(&self.expires_at),
        ) {
            (Some(issued), Some(expires)) => issued <= now && now < expires,
            _ => false,
        }
    }
}

// -------------------------------------------- endpoint host bindings ----

/// `KoveeRealmByomBinding` (§16.6; family contract L2/L8) — the
/// byom-normative side of one Realm's binding to this endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KoveeRealmByomBinding {
    pub binding_ref: String,
    pub realm_ref: String,
    pub binding_revision: u64,
    pub binding_epoch: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predecessor_binding_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predecessor_binding_digest: Option<DigestRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding_lineage_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding_lineage_digest: Option<DigestRef>,
    pub byom_endpoint_ref: String,
    pub endpoint_incarnation: String,
    pub compatibility_bundle: String,
    pub delegated_principal_audience: String,
    pub external_authorization_audience: String,
    pub historical_recovery_mode: String,
    pub recovery_authorization_policy_ref: String,
    pub recovery_authorization_policy_digest: DigestRef,
    pub status: String,
    pub dependency_digest: DigestRef,
    pub digest: DigestRef,
}

impl KoveeRealmByomBinding {
    pub fn validate(&self) -> Result<(), String> {
        for (name, v) in [
            ("binding_ref", &self.binding_ref),
            ("realm_ref", &self.realm_ref),
            ("byom_endpoint_ref", &self.byom_endpoint_ref),
            ("endpoint_incarnation", &self.endpoint_incarnation),
            (
                "delegated_principal_audience",
                &self.delegated_principal_audience,
            ),
            (
                "external_authorization_audience",
                &self.external_authorization_audience,
            ),
            (
                "recovery_authorization_policy_ref",
                &self.recovery_authorization_policy_ref,
            ),
            ("status", &self.status),
        ] {
            ident(name, v)?;
        }
        if self.compatibility_bundle != "byom_governed_work_v1" {
            return Err(
                "compatibility is one explicit all-or-nothing bundle: byom_governed_work_v1"
                    .to_owned(),
            );
        }
        if !["disabled", "exact_formation_intent_only"]
            .contains(&self.historical_recovery_mode.as_str())
        {
            return Err("historical_recovery_mode is not a §16.6 value".to_owned());
        }
        for (name, d) in [
            (
                "recovery_authorization_policy_digest",
                &self.recovery_authorization_policy_digest,
            ),
            ("dependency_digest", &self.dependency_digest),
            ("digest", &self.digest),
        ] {
            class(name, d, DigestClass::PortablePublic)?;
        }
        Ok(())
    }
}

/// `KoveeSocietyMapping` (§16.6; family contract L2/L4).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KoveeSocietyMapping {
    pub realm_ref: String,
    pub society_ref: String,
    pub society_recovery_epoch: u64,
    pub allowed_project_and_space_selectors: Vec<String>,
    pub classification_binding_ref: String,
    pub governance_owner_binding_ref: String,
    pub governance_owner_binding_digest: DigestRef,
    pub status: String,
    pub revision: u64,
    pub digest: DigestRef,
}

impl KoveeSocietyMapping {
    pub fn validate(&self) -> Result<(), String> {
        for (name, v) in [
            ("realm_ref", &self.realm_ref),
            ("society_ref", &self.society_ref),
            (
                "classification_binding_ref",
                &self.classification_binding_ref,
            ),
            (
                "governance_owner_binding_ref",
                &self.governance_owner_binding_ref,
            ),
            ("status", &self.status),
        ] {
            ident(name, v)?;
        }
        if self.allowed_project_and_space_selectors.is_empty()
            || self.allowed_project_and_space_selectors.len() > 256
        {
            return Err("allowed_project_and_space_selectors is out of bounds".to_owned());
        }
        for (name, d) in [
            (
                "governance_owner_binding_digest",
                &self.governance_owner_binding_digest,
            ),
            ("digest", &self.digest),
        ] {
            class(name, d, DigestClass::PortablePublic)?;
        }
        Ok(())
    }
}

/// A digest-pinned binding reference (`ref`, `revision`, `epoch`,
/// `digest`) — the §16.6 "binding validated per use" quadruple that
/// every host-facing row pins (family contract L8).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BindingPin {
    pub binding_ref: String,
    pub binding_revision: u64,
    pub binding_epoch: u64,
    pub digest: DigestRef,
}

impl BindingPin {
    pub fn validate(&self) -> Result<(), String> {
        ident("binding_ref", &self.binding_ref)?;
        class("digest", &self.digest, DigestClass::PortablePublic)
    }

    /// Does a request's four pinned members name exactly this binding?
    pub fn matches(&self, r: &str, revision: u64, epoch: u64, digest: &DigestRef) -> bool {
        self.binding_ref == r
            && self.binding_revision == revision
            && self.binding_epoch == epoch
            && &self.digest == digest
    }
}

// ------------------------------------------------ restore lineage ----

/// `RestoreLineage` (§16.3; created only by the §15.3 sealed restore
/// protocol and covered by the external witness).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestoreLineage {
    pub lineage_id: String,
    pub endpoint_root_id: String,
    pub predecessor_endpoint_incarnation: String,
    pub successor_endpoint_incarnation: String,
    pub society_ref: String,
    pub predecessor_society_recovery_epoch: u64,
    pub successor_society_recovery_epoch: u64,
    pub predecessor_authority_journal_head: String,
    pub predecessor_idempotency_checkpoint_ref: String,
    pub predecessor_idempotency_checkpoint_digest: DigestRef,
    pub idempotency_retention: String,
    pub predecessor_domain_execution: String,
    pub recovery_event_ref: String,
    pub external_witness_ref: String,
    pub external_witness_receipt_digest: DigestRef,
    pub issued_at: String,
    pub status: String,
    pub digest: DigestRef,
}

impl RestoreLineage {
    pub fn validate(&self) -> Result<(), String> {
        if !["complete", "incomplete", "unavailable"].contains(&self.idempotency_retention.as_str())
        {
            return Err("idempotency_retention is not a §16.3 value".to_owned());
        }
        if self.predecessor_domain_execution != "permanently_fenced" {
            return Err(
                "predecessor_domain_execution has exactly one admissible value: permanently_fenced"
                    .to_owned(),
            );
        }
        if !["current", "superseded"].contains(&self.status.as_str()) {
            return Err("status is not a §16.3 value".to_owned());
        }
        timestamp("issued_at", &self.issued_at)?;
        class("digest", &self.digest, DigestClass::PortablePublic)?;
        let value = serde_json::to_value(self).map_err(|e| e.to_string())?;
        if self_digest(LINEAGE_EVIDENCE_TAG, &value)? != self.digest {
            return Err("lineage digest does not cover these exact bytes".to_owned());
        }
        Ok(())
    }
}

/// One `ordered_hops[]` entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LineageHop {
    pub lineage_ref: String,
    pub lineage_digest: DigestRef,
}

/// `RestoreLineageProof` (§16.3): hops in target-to-current order.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestoreLineageProof {
    pub proof_id: String,
    pub endpoint_root_id: String,
    pub society_ref: String,
    pub target_endpoint_incarnation: String,
    pub target_society_recovery_epoch: u64,
    pub current_endpoint_incarnation: String,
    pub current_society_recovery_epoch: u64,
    pub hop_count: u64,
    pub ordered_hops: Vec<LineageHop>,
    pub target_idempotency_domain_digest: DigestRef,
    pub composed_at: String,
    pub verifier_version: String,
    pub digest: DigestRef,
}

/// This bundle's negotiated lineage ceiling (§16.3 fixes no number —
/// recorded gap 5 of `spec/governed-work/greenfield-saga.md`).
pub const LINEAGE_HOP_MAX: usize = 64;

/// Why a cited lineage cannot authorize a historical answer. The two
/// arms map onto the §16.3 closed blocking states: an unverifiable
/// external witness receipt is `witness_unavailable`, everything else
/// (missing record, wrong chain, non-`complete` retention, duplicate,
/// cycle, branch, gap) is `lineage_incomplete`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineageFault {
    Incomplete(String),
    WitnessUnavailable(String),
}

impl LineageFault {
    pub fn blocking_state(&self) -> &'static str {
        match self {
            LineageFault::Incomplete(_) => "lineage_incomplete",
            LineageFault::WitnessUnavailable(_) => "witness_unavailable",
        }
    }

    pub fn detail(&self) -> &str {
        match self {
            LineageFault::Incomplete(d) | LineageFault::WitnessUnavailable(d) => d,
        }
    }
}

impl RestoreLineageProof {
    pub fn validate(&self) -> Result<(), String> {
        if self.ordered_hops.is_empty() || self.ordered_hops.len() > LINEAGE_HOP_MAX {
            return Err("ordered_hops is out of the negotiated lineage limit".to_owned());
        }
        if self.hop_count as usize != self.ordered_hops.len() {
            return Err("hop_count MUST equal the ordered_hops length (§16.3)".to_owned());
        }
        class(
            "target_idempotency_domain_digest",
            &self.target_idempotency_domain_digest,
            DigestClass::ScopeErasureSafe,
        )?;
        class("digest", &self.digest, DigestClass::PortablePublic)?;
        timestamp("composed_at", &self.composed_at)?;
        Ok(())
    }

    /// The §16.3 hop verifier, verbatim: one endpoint root and Society
    /// throughout; the first predecessor incarnation/epoch equals the
    /// query target; each hop's successor equals the next hop's
    /// predecessor; the final successor equals the current authenticated
    /// endpoint/Society epoch; every hop carries a valid external
    /// witness receipt, `idempotency_retention: complete`,
    /// `predecessor_domain_execution: permanently_fenced`, a contiguous
    /// journal/idempotency checkpoint, and no duplicate, cycle, branch,
    /// or gap. Later complete hops cannot launder an earlier incomplete
    /// one — the walk fails at the first bad hop.
    #[allow(clippy::too_many_arguments)]
    pub fn verify(
        &self,
        resolve: impl Fn(&str) -> Option<RestoreLineage>,
        witness_ok: impl Fn(&RestoreLineage) -> bool,
        target_incarnation: &str,
        target_epoch: u64,
        current_incarnation: &str,
        current_epoch: u64,
        society_ref: &str,
        target_domain_digest: &DigestRef,
    ) -> Result<Vec<RestoreLineage>, LineageFault> {
        let bad = |m: &str| LineageFault::Incomplete(m.to_owned());
        self.validate().map_err(|e| bad(&e))?;
        if self.society_ref != society_ref {
            return Err(bad("proof names another Society"));
        }
        if self.target_endpoint_incarnation != target_incarnation
            || self.target_society_recovery_epoch != target_epoch
        {
            return Err(bad("proof target is not the queried target"));
        }
        if self.current_endpoint_incarnation != current_incarnation
            || self.current_society_recovery_epoch != current_epoch
        {
            return Err(bad("proof current end is not this endpoint"));
        }
        if &self.target_idempotency_domain_digest != target_domain_digest {
            return Err(bad("proof does not pin the queried idempotency domain"));
        }
        let mut seen_refs = std::collections::BTreeSet::new();
        let mut seen_nodes = std::collections::BTreeSet::new();
        let mut hops = Vec::with_capacity(self.ordered_hops.len());
        let mut cursor = (target_incarnation.to_owned(), target_epoch);
        seen_nodes.insert(cursor.clone());
        for (i, hop) in self.ordered_hops.iter().enumerate() {
            if !seen_refs.insert(hop.lineage_ref.clone()) {
                return Err(bad("duplicate lineage hop"));
            }
            let Some(record) = resolve(&hop.lineage_ref) else {
                return Err(bad("cited RestoreLineage record is missing"));
            };
            record.validate().map_err(|e| bad(&e))?;
            if record.digest != hop.lineage_digest {
                return Err(bad("hop digest does not pin the cited record"));
            }
            if record.endpoint_root_id != self.endpoint_root_id
                || record.society_ref != self.society_ref
            {
                return Err(bad("hop leaves the one endpoint root / Society"));
            }
            if record.predecessor_endpoint_incarnation != cursor.0
                || record.predecessor_society_recovery_epoch != cursor.1
            {
                return Err(bad("hop predecessor does not continue the chain (gap)"));
            }
            if record.idempotency_retention != "complete" {
                return Err(bad(
                    "hop retention is not complete (a later complete hop cannot launder it)",
                ));
            }
            if record.predecessor_authority_journal_head.is_empty()
                || record.predecessor_idempotency_checkpoint_ref.is_empty()
            {
                return Err(bad("hop has no contiguous journal/idempotency checkpoint"));
            }
            if !witness_ok(&record) {
                return Err(LineageFault::WitnessUnavailable(format!(
                    "hop {i} external witness receipt is unavailable or unverified"
                )));
            }
            cursor = (
                record.successor_endpoint_incarnation.clone(),
                record.successor_society_recovery_epoch,
            );
            if !seen_nodes.insert(cursor.clone()) {
                return Err(bad("hop revisits an incarnation/epoch (cycle or branch)"));
            }
            hops.push(record);
        }
        if cursor.0 != current_incarnation || cursor.1 != current_epoch {
            return Err(bad("final successor is not the current endpoint/epoch"));
        }
        Ok(hops)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn the_command_digest_covers_only_the_command_bytes() {
        let command = serde_json::json!({"society_ref": "soc-1", "n": 1});
        let a = command_digest(&command).unwrap();
        let b = command_digest(&serde_json::json!({"n": 1, "society_ref": "soc-1"})).unwrap();
        assert_eq!(a, b, "member order is canonicalized away");
        assert_eq!(a.class, "portable_public");
        assert_ne!(a, command_digest(&serde_json::json!({"n": 2})).unwrap());
    }

    #[test]
    fn a_fresh_nonce_changes_the_attempt_proof_but_not_the_command() {
        let d = DigestRef::portable_public("a".repeat(64));
        let dom = DigestRef::scope_erasure_safe("k", "b".repeat(64));
        let one = attempt_proof(&d, &dom, "nonce-1", &d, &d).unwrap();
        let two = attempt_proof(&d, &dom, "nonce-2", &d, &d).unwrap();
        assert_ne!(one, two);
        assert!(one.starts_with("ap1."));
    }
}
