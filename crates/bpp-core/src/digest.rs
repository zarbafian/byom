//! The closed family `DigestRef` wire (family-vectors/PROFILE.md §6.1,
//! normative; D-R0-1): exactly the members `{class, algorithm, key_ref?,
//! value_hex}`, closed class/algorithm pairing, `key_ref` required
//! exactly for the keyed erasure classes. Every digest field is typed —
//! never an unlabelled hash.
//!
//! What you write:
//! ```
//! use bpp_core::digest::{DigestRef, DigestClass};
//! let d = DigestRef::local_erasure_safe("society-key:soc-1/object:x",
//!     "a".repeat(64));
//! d.validate_wire().unwrap();
//! assert_eq!(d.class_enum().unwrap(), DigestClass::LocalErasureSafe);
//! ```

use serde::{Deserialize, Serialize};

/// The six digest classes (PROFILE §6, D-R0-1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DigestClass {
    StructuralPublic,
    PortablePublic,
    LocalErasureSafe,
    ScopeErasureSafe,
    DisclosedParty,
    CiphertextPublic,
}

impl DigestClass {
    pub fn as_str(self) -> &'static str {
        match self {
            DigestClass::StructuralPublic => "structural_public",
            DigestClass::PortablePublic => "portable_public",
            DigestClass::LocalErasureSafe => "local_erasure_safe",
            DigestClass::ScopeErasureSafe => "scope_erasure_safe",
            DigestClass::DisclosedParty => "disclosed_party",
            DigestClass::CiphertextPublic => "ciphertext_public",
        }
    }

    pub fn parse(s: &str) -> Option<DigestClass> {
        Some(match s {
            "structural_public" => DigestClass::StructuralPublic,
            "portable_public" => DigestClass::PortablePublic,
            "local_erasure_safe" => DigestClass::LocalErasureSafe,
            "scope_erasure_safe" => DigestClass::ScopeErasureSafe,
            "disclosed_party" => DigestClass::DisclosedParty,
            "ciphertext_public" => DigestClass::CiphertextPublic,
            _ => return None,
        })
    }

    /// The keyed erasure classes take `hmac-sha-256` and require
    /// `key_ref`; the public classes take `sha-256` and forbid it.
    pub fn keyed(self) -> bool {
        matches!(
            self,
            DigestClass::LocalErasureSafe | DigestClass::ScopeErasureSafe
        )
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DigestWireError {
    #[error("unknown_digest_class")]
    UnknownClass,
    #[error("digest_ref_algorithm_class_mismatch")]
    AlgorithmClassMismatch,
    #[error("digest_ref_key_ref_missing")]
    KeyRefMissing,
    #[error("digest_ref_key_ref_forbidden")]
    KeyRefForbidden,
    #[error("digest_ref_value_not_64_hex")]
    ValueNot64Hex,
    #[error("digest_class_mismatch")]
    ClassMismatch,
}

/// A typed family digest on the closed wire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DigestRef {
    pub class: String,
    pub algorithm: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub key_ref: Option<String>,
    pub value_hex: String,
}

impl DigestRef {
    /// A `scope_erasure_safe` ref (per-scope HMAC key; PROFILE §6.1).
    pub fn scope_erasure_safe(key_ref: &str, value_hex: String) -> DigestRef {
        DigestRef {
            class: "scope_erasure_safe".to_owned(),
            algorithm: "hmac-sha-256".to_owned(),
            key_ref: Some(key_ref.to_owned()),
            value_hex,
        }
    }

    /// A `portable_public` ref (unkeyed SHA-256 over content that
    /// crosses an authority boundary — the C2 host-integration digests
    /// both sides recompute; PROFILE §6.1).
    pub fn portable_public(value_hex: String) -> DigestRef {
        DigestRef {
            class: "portable_public".to_owned(),
            algorithm: "sha-256".to_owned(),
            key_ref: None,
            value_hex,
        }
    }

    /// A `local_erasure_safe` ref (random per-object secret; PROFILE §6.1).
    pub fn local_erasure_safe(key_ref: &str, value_hex: String) -> DigestRef {
        DigestRef {
            class: "local_erasure_safe".to_owned(),
            algorithm: "hmac-sha-256".to_owned(),
            key_ref: Some(key_ref.to_owned()),
            value_hex,
        }
    }

    pub fn class_enum(&self) -> Result<DigestClass, DigestWireError> {
        DigestClass::parse(&self.class).ok_or(DigestWireError::UnknownClass)
    }

    /// PROFILE §6.3 steps 4–7: class known, class/algorithm pairing,
    /// `key_ref` presence, `value_hex` exactly 64 lowercase hex. (Steps
    /// 1–3, wire typing and closed members, are the serde shape.)
    pub fn validate_wire(&self) -> Result<DigestClass, DigestWireError> {
        let class = self.class_enum()?;
        let expected_algorithm = if class.keyed() {
            "hmac-sha-256"
        } else {
            "sha-256"
        };
        if self.algorithm != expected_algorithm {
            return Err(DigestWireError::AlgorithmClassMismatch);
        }
        match (&self.key_ref, class.keyed()) {
            (None, true) => return Err(DigestWireError::KeyRefMissing),
            (Some(k), true) if k.is_empty() => return Err(DigestWireError::KeyRefMissing),
            (Some(_), false) => return Err(DigestWireError::KeyRefForbidden),
            _ => {}
        }
        if self.value_hex.len() != 64
            || !self
                .value_hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(DigestWireError::ValueNot64Hex);
        }
        Ok(class)
    }

    /// PROFILE §6.3 step 9: a well-constructed digest of the wrong class
    /// is `digest_class_mismatch`, never a silent substitution (RT-02).
    pub fn require_class(&self, required: DigestClass) -> Result<(), DigestWireError> {
        let class = self.validate_wire()?;
        if class != required {
            return Err(DigestWireError::ClassMismatch);
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn key_ref_is_forbidden_for_public_classes() {
        let d = DigestRef {
            class: "structural_public".into(),
            algorithm: "sha-256".into(),
            key_ref: Some("k".into()),
            value_hex: "a".repeat(64),
        };
        assert_eq!(d.validate_wire(), Err(DigestWireError::KeyRefForbidden));
    }

    #[test]
    fn erasure_classes_are_mutually_non_substitutable() {
        let d = DigestRef::scope_erasure_safe("scope-k", "a".repeat(64));
        assert_eq!(
            d.require_class(DigestClass::LocalErasureSafe),
            Err(DigestWireError::ClassMismatch)
        );
    }
}
