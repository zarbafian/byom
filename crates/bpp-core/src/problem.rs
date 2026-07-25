//! The BPP failure convention (DESIGN.md §14.2/§14.9; PROFILE.md §3,
//! normative): RFC 9457 problem objects with the closed 29-kind enum,
//! `type` exactly `https://byom.dev/problems/<kind>`, optional integer
//! `status` in 400–599, and extension members solely under reverse-domain
//! names. Unknown kinds fail closed; the convention is non-substitutable
//! with kovee's `urn:kovee:error:<kind>` (PROFILE §3.1).
//!
//! What you write:
//! ```
//! use bpp_core::problem::{Problem, ProblemKind};
//! let p = Problem::new(ProblemKind::StaleRevision, "expected revision is old");
//! let json = serde_json::to_value(&p.into_failure()).unwrap();
//! assert_eq!(json["problem"]["type"],
//!     "https://byom.dev/problems/stale_revision");
//! ```

use serde::ser::SerializeMap as _;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The problems namespace prefix (PROFILE §3, pinned decision 3).
pub const PROBLEM_TYPE_PREFIX: &str = "https://byom.dev/problems/";

/// The closed §14.9 problem kinds, stable for this bundle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProblemKind {
    Invalid,
    UnsupportedVersion,
    FeatureUnavailable,
    ForbiddenSurface,
    Forbidden,
    NotFound,
    StaleRevision,
    StaleBinding,
    StaleAssemblyEpoch,
    StaleLease,
    IdempotencyMismatch,
    PositionIneligible,
    DecisionIncomplete,
    IndependenceConflict,
    AuthorityWidening,
    MandateHeld,
    AdmissionRequired,
    ClassificationUnmapped,
    PolicyConflict,
    PolicyOverflow,
    BudgetExceeded,
    EffectAmbiguous,
    AuthorityWitnessUnknown,
    EndpointSealed,
    CursorExpired,
    Unavailable,
    FormationRequiresParticipation,
    ExternalCommandNotTerminalizable,
    Internal,
}

/// Every kind, in the §14.9 order (29 kinds).
pub const ALL_KINDS: [ProblemKind; 29] = [
    ProblemKind::Invalid,
    ProblemKind::UnsupportedVersion,
    ProblemKind::FeatureUnavailable,
    ProblemKind::ForbiddenSurface,
    ProblemKind::Forbidden,
    ProblemKind::NotFound,
    ProblemKind::StaleRevision,
    ProblemKind::StaleBinding,
    ProblemKind::StaleAssemblyEpoch,
    ProblemKind::StaleLease,
    ProblemKind::IdempotencyMismatch,
    ProblemKind::PositionIneligible,
    ProblemKind::DecisionIncomplete,
    ProblemKind::IndependenceConflict,
    ProblemKind::AuthorityWidening,
    ProblemKind::MandateHeld,
    ProblemKind::AdmissionRequired,
    ProblemKind::ClassificationUnmapped,
    ProblemKind::PolicyConflict,
    ProblemKind::PolicyOverflow,
    ProblemKind::BudgetExceeded,
    ProblemKind::EffectAmbiguous,
    ProblemKind::AuthorityWitnessUnknown,
    ProblemKind::EndpointSealed,
    ProblemKind::CursorExpired,
    ProblemKind::Unavailable,
    ProblemKind::FormationRequiresParticipation,
    ProblemKind::ExternalCommandNotTerminalizable,
    ProblemKind::Internal,
];

impl ProblemKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ProblemKind::Invalid => "invalid",
            ProblemKind::UnsupportedVersion => "unsupported_version",
            ProblemKind::FeatureUnavailable => "feature_unavailable",
            ProblemKind::ForbiddenSurface => "forbidden_surface",
            ProblemKind::Forbidden => "forbidden",
            ProblemKind::NotFound => "not_found",
            ProblemKind::StaleRevision => "stale_revision",
            ProblemKind::StaleBinding => "stale_binding",
            ProblemKind::StaleAssemblyEpoch => "stale_assembly_epoch",
            ProblemKind::StaleLease => "stale_lease",
            ProblemKind::IdempotencyMismatch => "idempotency_mismatch",
            ProblemKind::PositionIneligible => "position_ineligible",
            ProblemKind::DecisionIncomplete => "decision_incomplete",
            ProblemKind::IndependenceConflict => "independence_conflict",
            ProblemKind::AuthorityWidening => "authority_widening",
            ProblemKind::MandateHeld => "mandate_held",
            ProblemKind::AdmissionRequired => "admission_required",
            ProblemKind::ClassificationUnmapped => "classification_unmapped",
            ProblemKind::PolicyConflict => "policy_conflict",
            ProblemKind::PolicyOverflow => "policy_overflow",
            ProblemKind::BudgetExceeded => "budget_exceeded",
            ProblemKind::EffectAmbiguous => "effect_ambiguous",
            ProblemKind::AuthorityWitnessUnknown => "authority_witness_unknown",
            ProblemKind::EndpointSealed => "endpoint_sealed",
            ProblemKind::CursorExpired => "cursor_expired",
            ProblemKind::Unavailable => "unavailable",
            ProblemKind::FormationRequiresParticipation => "formation_requires_participation",
            ProblemKind::ExternalCommandNotTerminalizable => "external_command_not_terminalizable",
            ProblemKind::Internal => "internal",
        }
    }

    /// Unknown kinds fail closed (PROFILE §3).
    pub fn parse(s: &str) -> Option<ProblemKind> {
        ALL_KINDS.into_iter().find(|k| k.as_str() == s)
    }

    /// The `type` URI: exactly prefix + kind (pinned decision 3).
    pub fn type_uri(self) -> String {
        format!("{PROBLEM_TYPE_PREFIX}{}", self.as_str())
    }
}

/// One RFC 9457 problem under the byom convention.
#[derive(Debug, Clone, PartialEq)]
pub struct Problem {
    pub kind: ProblemKind,
    pub title: String,
    pub status: Option<u16>,
    pub detail: Option<String>,
    pub instance: Option<String>,
    /// Reverse-domain extension members (carry no authority).
    pub extensions: Vec<(String, Value)>,
}

impl Problem {
    pub fn new(kind: ProblemKind, title: &str) -> Problem {
        Problem {
            kind,
            title: title.to_owned(),
            status: None,
            detail: None,
            instance: None,
            extensions: Vec::new(),
        }
    }

    pub fn with_status(mut self, status: u16) -> Problem {
        self.status = Some(status);
        self
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Problem {
        self.detail = Some(detail.into());
        self
    }

    pub fn into_failure(self) -> Failure {
        Failure { problem: self }
    }
}

/// The §14.2 failure envelope: `{"outcome":"problem","problem":{...}}`.
#[derive(Debug, Clone, PartialEq)]
pub struct Failure {
    pub problem: Problem,
}

/// An extension member name is acceptable only under a reverse-domain
/// name: at least three dot-separated lowercase segments (PROFILE §3).
pub fn is_reverse_domain_member(name: &str) -> bool {
    let segments: Vec<&str> = name.split('.').collect();
    segments.len() >= 3
        && segments.iter().all(|seg| {
            let bytes = seg.as_bytes();
            !bytes.is_empty()
                && bytes[0].is_ascii_lowercase()
                && bytes[1..].iter().all(|b| {
                    b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-' || *b == b'_'
                })
        })
}

impl Serialize for Failure {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("outcome", "problem")?;
        let p = &self.problem;
        let mut problem = serde_json::Map::new();
        problem.insert("type".into(), Value::String(p.kind.type_uri()));
        problem.insert("title".into(), Value::String(p.title.clone()));
        problem.insert("kind".into(), Value::String(p.kind.as_str().to_owned()));
        if let Some(status) = p.status {
            problem.insert("status".into(), Value::from(status));
        }
        if let Some(detail) = &p.detail {
            problem.insert("detail".into(), Value::String(detail.clone()));
        }
        if let Some(instance) = &p.instance {
            problem.insert("instance".into(), Value::String(instance.clone()));
        }
        for (name, value) in &p.extensions {
            if is_reverse_domain_member(name) {
                problem.insert(name.clone(), value.clone());
            }
        }
        map.serialize_entry("problem", &Value::Object(problem))?;
        map.end()
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FailureParseError {
    #[error("failure envelope shape: {0}")]
    Shape(String),
    #[error("unknown problem kind fails closed")]
    UnknownKind,
    #[error("type is not exactly the problems namespace plus kind")]
    TypeKindMismatch,
    #[error("status must be a JSON integer in 400-599")]
    StatusRange,
    #[error("extension member name is not reverse-domain")]
    ExtensionName,
}

impl<'de> Deserialize<'de> for Failure {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Failure, D::Error> {
        use serde::de::Error as _;
        let value = Value::deserialize(deserializer)?;
        parse_failure(&value).map_err(D::Error::custom)
    }
}

/// Validates and parses one failure envelope value (PROFILE §3 rules).
pub fn parse_failure(value: &Value) -> Result<Failure, FailureParseError> {
    let shape = |m: &str| FailureParseError::Shape(m.to_owned());
    let Value::Object(map) = value else {
        return Err(shape("not an object"));
    };
    if map.len() != 2 {
        return Err(shape("exactly outcome and problem"));
    }
    if map.get("outcome").and_then(Value::as_str) != Some("problem") {
        return Err(shape("outcome must be \"problem\""));
    }
    let Some(Value::Object(p)) = map.get("problem") else {
        return Err(shape("problem must be an object"));
    };
    let kind_s = p
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| shape("kind required"))?;
    let kind = ProblemKind::parse(kind_s).ok_or(FailureParseError::UnknownKind)?;
    let type_s = p
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| shape("type required"))?;
    if type_s != kind.type_uri() {
        return Err(FailureParseError::TypeKindMismatch);
    }
    let title = p
        .get("title")
        .and_then(Value::as_str)
        .ok_or_else(|| shape("title required"))?;
    let status = match p.get("status") {
        None => None,
        Some(Value::Number(n)) => {
            // A JSON integer only: floats, bools, and strings fail.
            let v = n.as_u64().ok_or(FailureParseError::StatusRange)?;
            if !(400..=599).contains(&v) {
                return Err(FailureParseError::StatusRange);
            }
            Some(v as u16)
        }
        Some(_) => return Err(FailureParseError::StatusRange),
    };
    let mut detail = None;
    let mut instance = None;
    let mut extensions = Vec::new();
    for (name, member) in p {
        match name.as_str() {
            "type" | "title" | "kind" | "status" => {}
            "detail" => {
                detail = Some(
                    member
                        .as_str()
                        .ok_or_else(|| shape("detail must be a string"))?
                        .to_owned(),
                );
            }
            "instance" => {
                instance = Some(
                    member
                        .as_str()
                        .ok_or_else(|| shape("instance must be a string"))?
                        .to_owned(),
                );
            }
            other => {
                if !is_reverse_domain_member(other) {
                    return Err(FailureParseError::ExtensionName);
                }
                extensions.push((other.to_owned(), member.clone()));
            }
        }
    }
    Ok(Failure {
        problem: Problem {
            kind,
            title: title.to_owned(),
            status,
            detail,
            instance,
            extensions,
        },
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn the_kind_enum_is_closed_at_29() {
        assert_eq!(ALL_KINDS.len(), 29);
        assert!(
            ProblemKind::parse("stale-revision").is_none(),
            "kovee spelling never satisfies byom"
        );
        assert!(ProblemKind::parse("stale_revision").is_some());
    }

    #[test]
    fn round_trips_with_extensions() {
        let mut p = Problem::new(ProblemKind::StaleRevision, "old").with_status(409);
        p.extensions
            .push(("dev.byom.expected_revision".into(), Value::from(17)));
        let json = serde_json::to_value(p.clone().into_failure()).unwrap();
        let back = parse_failure(&json).unwrap();
        assert_eq!(back.problem, p);
    }

    #[test]
    fn a_bare_extension_name_fails_closed() {
        let v = serde_json::json!({"outcome":"problem","problem":{
            "type":"https://byom.dev/problems/budget_exceeded",
            "title":"t","kind":"budget_exceeded","budget_ref":"b"}});
        assert_eq!(parse_failure(&v), Err(FailureParseError::ExtensionName));
    }
}
