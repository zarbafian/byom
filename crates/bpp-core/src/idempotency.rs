//! The canonical, inspectable idempotency uniqueness domain (§14.2;
//! family-vectors/PROFILE.md §5, normative; D-R0-1):
//!
//! ```text
//! idempotency_domain_digest =
//!   HMAC-SHA-256(per-Society idempotency-index key,
//!                tagged("bpp-idempotency-domain-v1", IdempotencyDomain))
//!   → DigestRef class scope_erasure_safe
//! ```
//!
//! The index key is a scope key: destroying it erases offline
//! verifiability of the entire index, never one entry. The digest is
//! byte-pinned by `spec/vectors/envelope/digest-idempotency-domain-*`.
//!
//! What you write:
//! ```
//! use bpp_core::{digest::DigestRef, idempotency::IdempotencyDomain};
//! let domain = IdempotencyDomain {
//!     actor_binding_digest: DigestRef::local_erasure_safe("kb-1", "a".repeat(64)),
//!     operation: "society_prepare".into(),
//!     endpoint_incarnation: "inc-0001".into(),
//!     society_id: "soc-0001".into(),
//!     society_recovery_epoch: 0,
//!     idempotency_key: "idem-0001".into(),
//! };
//! let d = domain.digest(&[0x5b; 32], "society-key:soc-0001/idempotency-index").unwrap();
//! assert_eq!(d.class, "scope_erasure_safe");
//! ```

use serde::{Deserialize, Serialize};

use crate::canonical::{hex, hmac_sha256, tagged_canonical, CanonicalError};
use crate::digest::{DigestClass, DigestRef};
use crate::envelope::is_identifier;

/// The `$domain` tag of the idempotency-domain preimage (PROFILE §5).
pub const IDEMPOTENCY_DOMAIN_TAG: &str = "bpp-idempotency-domain-v1";

/// The server-recomputed uniqueness domain of one mutation (§14.2).
/// `actor_binding_digest` covers the authenticated principal and binding
/// epoch supplied by the channel, never a caller-selected identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdempotencyDomain {
    pub actor_binding_digest: DigestRef,
    pub operation: String,
    pub endpoint_incarnation: String,
    pub society_id: String,
    pub society_recovery_epoch: u64,
    pub idempotency_key: String,
}

#[derive(Debug, thiserror::Error)]
pub enum DomainError {
    #[error("canonicalization: {0}")]
    Canonical(#[from] CanonicalError),
    #[error("serialization: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("{0}")]
    Invalid(String),
}

impl IdempotencyDomain {
    /// Field validation past the serde shape: identifier bounds, the
    /// operation-id shape, and the typed actor binding (which must be
    /// `local_erasure_safe` — an authority-subject commitment).
    pub fn validate(&self) -> Result<(), DomainError> {
        self.actor_binding_digest
            .require_class(DigestClass::LocalErasureSafe)
            .map_err(|e| DomainError::Invalid(e.to_string()))?;
        if !crate::envelope::is_operation_id(&self.operation) {
            return Err(DomainError::Invalid("operation shape".to_owned()));
        }
        for (name, v) in [
            ("endpoint_incarnation", &self.endpoint_incarnation),
            ("society_id", &self.society_id),
            ("idempotency_key", &self.idempotency_key),
        ] {
            if !is_identifier(v) {
                return Err(DomainError::Invalid(format!("{name} is not an identifier")));
            }
        }
        if self.society_recovery_epoch > crate::canonical::SAFE_MAX {
            return Err(DomainError::Invalid(
                "epoch exceeds the safe range".to_owned(),
            ));
        }
        Ok(())
    }

    /// The `$domain`-tagged JCS preimage bytes.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, DomainError> {
        let value = serde_json::to_value(self)?;
        Ok(tagged_canonical(IDEMPOTENCY_DOMAIN_TAG, &value)?)
    }

    /// The ratified domain digest: keyed HMAC under the per-Society
    /// idempotency-index key, emitted as a `scope_erasure_safe` DigestRef
    /// (pinned decision 5).
    pub fn digest(&self, index_key: &[u8], key_ref: &str) -> Result<DigestRef, DomainError> {
        let preimage = self.canonical_bytes()?;
        let mac = hmac_sha256(index_key, &preimage);
        Ok(DigestRef::scope_erasure_safe(key_ref, hex(&mac)))
    }
}
