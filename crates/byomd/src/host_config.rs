//! The endpoint's Kovee host binding — **configuration, not Society
//! authorship**.
//!
//! Kovee owns the greenfield enablement saga
//! (`spec/governed-work/greenfield-saga.md`): its KCP-admin
//! `governance_enable` creates the `KoveeRealmByomBinding` +
//! `KoveeSocietyMapping` and CASes the owner binding `none → byom`. On
//! byom's side those rows are what kovee amendment A2 permits — "Kovee
//! may start/configure/bind byomd and supply inert context only" — so
//! they arrive as an endpoint **configuration file**, never as a BPP
//! operation that would let a gateway author Society state. (The
//! admin-surface `service_configure` row of R43 that would formalize the
//! install path is a later bundle; recorded as a deviation.)
//!
//! What you write (one file beside the store):
//! ```text
//! <data-dir>/kovee/host-binding.json
//! {"realm_byom_binding": {...}, "society_mapping": {...},
//!  "delegated_principal_issuers": ["kovee-gateway-1"],
//!  "recovery_binding": {...}, "endpoint_root_id": "root-1",
//!  "external_witness_receipts": [...], "restore_lineages": [...],
//!  "restore_lineage_proofs": [...]}
//! ```
//!
//! Every field is re-validated against the FROZEN C2 shapes on every
//! use; a malformed or absent file simply means this endpoint has no
//! delegated-principal channel, and R39/R40/R42 answer `forbidden`.

use std::os::unix::fs::PermissionsExt as _;
use std::path::PathBuf;

use bpp_core::canonical::{hex, hmac_sha256};
use bpp_core::digest::DigestRef;
use bpp_core::hostint::{
    BindingPin, KoveeRealmByomBinding, KoveeSocietyMapping, RestoreLineage, RestoreLineageProof,
};
use bpp_core::problem::Problem;
use byom_store::Store;
use serde::Deserialize;

use crate::state;

/// One external witness receipt this endpoint can verify. A
/// RestoreLineage hop whose `(external_witness_ref,
/// external_witness_receipt_digest)` pair is not listed is
/// `witness_unavailable` — never silently "verified" (§16.3).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WitnessReceipt {
    pub witness_ref: String,
    pub receipt_digest: DigestRef,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostConfig {
    pub realm_byom_binding: KoveeRealmByomBinding,
    pub society_mapping: KoveeSocietyMapping,
    /// The gateways whose `issuer_ref` this endpoint accepts on a
    /// DelegatedPrincipalCredential (§14.4: a generic Kovee service
    /// credential cannot become a principal).
    pub delegated_principal_issuers: Vec<String>,
    /// The current recovery binding R40/R42 authenticate through
    /// (`current_recovery_binding_*`; the attempt envelope's
    /// `attempt_recovery_binding_*`).
    pub recovery_binding: BindingPin,
    /// The endpoint root every RestoreLineage hop must stay inside.
    pub endpoint_root_id: String,
    #[serde(default)]
    pub external_witness_receipts: Vec<WitnessReceipt>,
    #[serde(default)]
    pub restore_lineages: Vec<RestoreLineage>,
    #[serde(default)]
    pub restore_lineage_proofs: Vec<RestoreLineageProof>,
}

pub fn config_path(store: &Store) -> PathBuf {
    store.data_dir().join("kovee").join("host-binding.json")
}

/// The narrow R42 recovery-workload token: byomd-minted (never
/// caller-chosen), derived from the store root for the exact binding and
/// epoch, and published `0600` beside the candidate/participant channel
/// tokens. A binding-epoch advance therefore invalidates the derived
/// channel (family contract L2) without any revocation list.
pub fn recovery_workload_token(store: &Store, binding: &KoveeRealmByomBinding) -> Option<String> {
    let key = store.scope_key("recovery-workload-channel").ok()?;
    let bound = format!("{}|{}", binding.binding_ref, binding.binding_epoch);
    Some(format!(
        "rwl1.{}",
        hex(&hmac_sha256(&key, bound.as_bytes()))
    ))
}

/// Publishes the recovery-workload token file for an installed binding
/// (idempotent; the same reconcile discipline as the channel tokens).
pub fn ensure_recovery_token_file(store: &Store, binding: &KoveeRealmByomBinding) {
    let Some(token) = recovery_workload_token(store, binding) else {
        return;
    };
    let dir = crate::gov_ops::channels_dir(store);
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    let path = dir.join(format!("recovery-workload-{}.token", binding.binding_ref));
    let current = std::fs::read_to_string(&path).unwrap_or_default();
    if current.trim() != token {
        let _ = std::fs::write(&path, format!("{token}\n"));
    }
    let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
}

/// Startup reconcile: if a host binding is installed, publish its
/// recovery-workload token file. Absence is normal (no Kovee seam).
pub fn publish_recovery_token(store: &Store) {
    if let Ok(cfg) = HostConfig::load(store) {
        ensure_recovery_token_file(store, &cfg.realm_byom_binding);
    }
}

impl HostConfig {
    /// Loads and fully re-validates the installed binding. Absence is
    /// `forbidden`: without an active `KoveeRealmByomBinding` this
    /// endpoint has no delegated-principal channel at all.
    pub fn load(store: &Store) -> Result<HostConfig, Problem> {
        let path = config_path(store);
        let text = std::fs::read_to_string(&path).map_err(|_| {
            state::forbidden_detail("no KoveeRealmByomBinding is installed at this endpoint")
        })?;
        let value = bpp_core::ijson::parse_request(text.as_bytes())
            .map_err(|e| state::internal(&format!("host binding is not strict I-JSON: {e:?}")))?;
        let cfg: HostConfig = serde_json::from_value(value)
            .map_err(|e| state::internal(&format!("host binding shape: {e}")))?;
        cfg.validate()
            .map_err(|e| state::internal(&format!("host binding: {e}")))?;
        if cfg.realm_byom_binding.status != "active" || cfg.society_mapping.status != "active" {
            return Err(state::stale_binding(
                "the installed KoveeRealmByomBinding/KoveeSocietyMapping is not active",
            ));
        }
        ensure_recovery_token_file(store, &cfg.realm_byom_binding);
        Ok(cfg)
    }

    fn validate(&self) -> Result<(), String> {
        self.realm_byom_binding.validate()?;
        self.society_mapping.validate()?;
        self.recovery_binding.validate()?;
        if self.delegated_principal_issuers.is_empty() {
            return Err("delegated_principal_issuers names no gateway".to_owned());
        }
        if self.society_mapping.realm_ref != self.realm_byom_binding.realm_ref {
            return Err("the mapping and the binding name different Realms".to_owned());
        }
        for lineage in &self.restore_lineages {
            lineage.validate()?;
        }
        for proof in &self.restore_lineage_proofs {
            proof.validate()?;
        }
        Ok(())
    }

    pub fn lineage(&self, lineage_ref: &str) -> Option<RestoreLineage> {
        self.restore_lineages
            .iter()
            .find(|l| l.lineage_id == lineage_ref)
            .cloned()
    }

    pub fn proof(&self, proof_ref: &str) -> Option<&RestoreLineageProof> {
        self.restore_lineage_proofs
            .iter()
            .find(|p| p.proof_id == proof_ref)
    }

    /// Can this endpoint verify the hop's external witness receipt?
    pub fn witness_verifies(&self, lineage: &RestoreLineage) -> bool {
        self.external_witness_receipts.iter().any(|w| {
            w.witness_ref == lineage.external_witness_ref
                && w.receipt_digest == lineage.external_witness_receipt_digest
        })
    }
}
