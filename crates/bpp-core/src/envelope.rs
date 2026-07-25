//! The §14.2 request/result envelopes: `Request {version, op, meta?}`
//! with operation arguments at the top level, `MutationMeta`, and the
//! `Success` envelope. Concrete operations restate these fields in their
//! own closed schemas (`crate::ops`); this module owns the generic shape
//! and the registry meta-class rule (read: no meta; create: meta without
//! `expected_revision`; update: meta with it — RT-01).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::limits::IDENTIFIER_MAX_BYTES;
use crate::problem::{Problem, ProblemKind};
use crate::registry::OpClass;

/// An opaque visible-ASCII identifier, at most 128 bytes (§14.9).
pub fn is_identifier(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= IDENTIFIER_MAX_BYTES
        && s.bytes().all(|b| (0x21..=0x7e).contains(&b))
}

/// The §14.6 operation-id shape; membership is decided by the registry.
pub fn is_operation_id(s: &str) -> bool {
    let bytes = s.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 128
        && bytes[0].is_ascii_lowercase()
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

/// The protocol minor version shape, e.g. `0.2`.
pub fn is_protocol_version(s: &str) -> bool {
    let Some((major, minor)) = s.split_once('.') else {
        return false;
    };
    let num = |p: &str| {
        !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) && (p == "0" || !p.starts_with('0'))
    };
    num(major) && num(minor)
}

/// MutationMeta (§14.2). Required on every mutation; updates additionally
/// supply `expected_revision` (the closed update meta requires it, RT-01).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MutationMeta {
    pub request_id: String,
    pub idempotency_key: String,
    pub expected_endpoint_incarnation: String,
    pub expected_recovery_epoch: u64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub expected_revision: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub causation_event_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub correlation_ref: Option<String>,
}

impl MutationMeta {
    /// Field-level validation past the serde shape: identifier byte
    /// bounds and the safe-integer epoch.
    pub fn validate(&self) -> Result<(), String> {
        for (name, v) in [
            ("request_id", &self.request_id),
            ("idempotency_key", &self.idempotency_key),
            (
                "expected_endpoint_incarnation",
                &self.expected_endpoint_incarnation,
            ),
        ] {
            if !is_identifier(v) {
                return Err(format!("{name} is not a valid identifier"));
            }
        }
        if self.expected_recovery_epoch > crate::canonical::SAFE_MAX {
            return Err("expected_recovery_epoch exceeds the safe range".to_owned());
        }
        for (name, v) in [
            ("causation_event_ref", &self.causation_event_ref),
            ("correlation_ref", &self.correlation_ref),
        ] {
            if let Some(v) = v {
                if !is_identifier(v) {
                    return Err(format!("{name} is not a valid identifier"));
                }
            }
        }
        if let Some(rev) = self.expected_revision {
            if rev > crate::canonical::SAFE_MAX {
                return Err("expected_revision exceeds the safe range".to_owned());
            }
        }
        Ok(())
    }
}

/// The generic envelope extracted from one accepted request body: the
/// operation tag, optional meta, and the full body for per-op parsing.
#[derive(Debug, Clone)]
pub struct RawRequest {
    pub version: String,
    pub op: String,
    pub meta: Option<MutationMeta>,
    pub body: Value,
}

impl RawRequest {
    /// Extracts version/op/meta. Unknown fields stay in `body` for the
    /// closed per-operation schema; only the envelope fields are shaped
    /// here.
    pub fn from_value(value: &Value) -> Result<RawRequest, Problem> {
        let invalid = |detail: &str| {
            Problem::new(ProblemKind::Invalid, "invalid request envelope")
                .with_status(400)
                .with_detail(detail)
        };
        let Value::Object(map) = value else {
            return Err(invalid("request is not an object"));
        };
        let version = map
            .get("version")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("version required"))?;
        if !is_protocol_version(version) {
            return Err(invalid("version does not match the minor-version shape"));
        }
        let op = map
            .get("op")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("op required"))?;
        if !is_operation_id(op) {
            return Err(invalid("op does not match ^[a-z][a-z0-9_]{0,127}$"));
        }
        let meta = match map.get("meta") {
            None => None,
            Some(v) => {
                let meta: MutationMeta = serde_json::from_value(v.clone())
                    .map_err(|e| invalid(&format!("meta: {e}")))?;
                meta.validate().map_err(|e| invalid(&e))?;
                Some(meta)
            }
        };
        Ok(RawRequest {
            version: version.to_owned(),
            op: op.to_owned(),
            meta,
            body: value.clone(),
        })
    }

    /// The registry meta-class rule (§14.2, RT-01): reads never carry
    /// meta; creates require meta WITHOUT `expected_revision`; updates
    /// require meta WITH it.
    pub fn check_class(&self, class: OpClass) -> Result<(), Problem> {
        let invalid = |detail: &str| {
            Problem::new(
                ProblemKind::Invalid,
                "envelope does not match the operation class",
            )
            .with_status(400)
            .with_detail(detail)
        };
        match (class, &self.meta) {
            (OpClass::Read, None) => Ok(()),
            (OpClass::Read, Some(_)) => Err(invalid("closed read schema declares no meta")),
            (OpClass::Create | OpClass::Update, None) => {
                Err(invalid("required meta absent on a mutation"))
            }
            (OpClass::Create, Some(meta)) => {
                if meta.expected_revision.is_some() {
                    Err(invalid("create meta carries no expected_revision member"))
                } else {
                    Ok(())
                }
            }
            (OpClass::Update, Some(meta)) => {
                if meta.expected_revision.is_none() {
                    Err(invalid("update meta requires expected_revision"))
                } else {
                    Ok(())
                }
            }
        }
    }
}

/// The §14.2 success envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Success {
    /// Always the literal `"ok"`.
    pub outcome: String,
    pub result: Value,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub revision: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub source_cursor: Option<String>,
}

impl Success {
    pub fn new(result: Value) -> Success {
        Success {
            outcome: "ok".to_owned(),
            result,
            revision: None,
            source_cursor: None,
        }
    }

    /// Shape validation past serde: outcome const, cursor bounds.
    pub fn validate(&self) -> Result<(), String> {
        if self.outcome != "ok" {
            return Err("outcome must be \"ok\"".to_owned());
        }
        if let Some(rev) = self.revision {
            if rev > crate::canonical::SAFE_MAX {
                return Err("revision exceeds the safe range".to_owned());
            }
        }
        if let Some(cursor) = &self.source_cursor {
            if cursor.is_empty() || cursor.len() > 4096 {
                return Err("source_cursor out of bounds".to_owned());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn reads_reject_meta_and_updates_require_expected_revision() {
        let read = RawRequest::from_value(&serde_json::json!({
            "version": "0.2", "op": "hello"}))
        .unwrap();
        read.check_class(OpClass::Read).unwrap();
        assert!(read.check_class(OpClass::Update).is_err());

        let create = RawRequest::from_value(&serde_json::json!({
            "version": "0.2", "op": "membership_offer",
            "meta": {"request_id": "r", "idempotency_key": "k",
                     "expected_endpoint_incarnation": "inc",
                     "expected_recovery_epoch": 0}}))
        .unwrap();
        create.check_class(OpClass::Create).unwrap();
        assert!(
            create.check_class(OpClass::Update).is_err(),
            "update needs expected_revision"
        );
        assert!(create.check_class(OpClass::Read).is_err());
    }
}
