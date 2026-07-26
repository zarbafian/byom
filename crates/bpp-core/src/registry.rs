//! The frozen B0.1 (operation,surface) registry (spec/registry.json,
//! embedded verbatim at build time): exactly one row per pair, the
//! dispatch truth (§14.6/§14.7). An operation absent from the registry is
//! not callable; an operation whose rows name other surfaces answers the
//! deny-by-absence problem on this one — the registry rows, not daemon
//! code, decide.
//!
//! What you write:
//! ```
//! use bpp_core::registry::{lookup, OpClass, Surface};
//! let row = lookup("membership_accept", Surface::Candidate).unwrap();
//! assert_eq!(row.class, OpClass::Update);
//! assert!(lookup("membership_accept", Surface::Governance).is_none(),
//!     "nothing but the candidate accepts");
//! ```

use std::sync::OnceLock;

use serde_json::Value;

/// The registry file this build is frozen to.
pub const REGISTRY_JSON: &str = include_str!("../../../spec/registry.json");

/// The §14.5 authority surfaces named by registry rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    PreAuth,
    Governance,
    Candidate,
    Participant,
    Runtime,
    Projection,
    Originating,
}

impl Surface {
    pub fn as_str(self) -> &'static str {
        match self {
            Surface::PreAuth => "pre_auth",
            Surface::Governance => "governance",
            Surface::Candidate => "candidate",
            Surface::Participant => "participant",
            Surface::Runtime => "runtime",
            Surface::Projection => "projection",
            Surface::Originating => "originating",
        }
    }

    pub fn parse(s: &str) -> Option<Surface> {
        Some(match s {
            "pre_auth" => Surface::PreAuth,
            "governance" => Surface::Governance,
            "candidate" => Surface::Candidate,
            "participant" => Surface::Participant,
            "runtime" => Surface::Runtime,
            "projection" => Surface::Projection,
            "originating" => Surface::Originating,
            _ => return None,
        })
    }
}

/// The registry meta class of one row (RT-01).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpClass {
    Read,
    Create,
    Update,
}

/// One frozen (operation,surface) row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpRow {
    pub operation: String,
    pub surface: Surface,
    pub class: OpClass,
    pub request_schema: String,
    pub result_schema: String,
}

fn rows() -> &'static Vec<OpRow> {
    static ROWS: OnceLock<Vec<OpRow>> = OnceLock::new();
    ROWS.get_or_init(|| {
        // The registry is committed, frozen input; a malformed row is a
        // build-input defect, surfaced as an empty registry (every op
        // then answers deny-by-absence rather than panicking).
        let Ok(value) = serde_json::from_str::<Value>(REGISTRY_JSON) else {
            return Vec::new();
        };
        let Some(ops) = value.get("operations").and_then(Value::as_array) else {
            return Vec::new();
        };
        let mut out = Vec::with_capacity(ops.len());
        for row in ops {
            let (Some(operation), Some(surface), Some(class)) = (
                row.get("operation").and_then(Value::as_str),
                row.get("surface")
                    .and_then(Value::as_str)
                    .and_then(Surface::parse),
                row.get("class").and_then(Value::as_str),
            ) else {
                return Vec::new();
            };
            let class = match class {
                "read" => OpClass::Read,
                "create" => OpClass::Create,
                "update" => OpClass::Update,
                _ => return Vec::new(),
            };
            out.push(OpRow {
                operation: operation.to_owned(),
                surface,
                class,
                request_schema: row
                    .get("request_schema")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                result_schema: row
                    .get("result_schema")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
            });
        }
        out
    })
}

/// Every frozen row.
pub fn all_rows() -> &'static [OpRow] {
    rows()
}

/// Is the operation known to the bundle at all (on any surface)?
pub fn op_exists(op: &str) -> bool {
    rows().iter().any(|r| r.operation == op)
}

/// The one row for (operation, surface), if the registry binds it.
pub fn lookup(op: &str, surface: Surface) -> Option<&'static OpRow> {
    rows()
        .iter()
        .find(|r| r.operation == op && r.surface == surface)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn the_registry_freezes_the_two_bundles() {
        // 80 B0.1 rows + the 3 B0.3 host-integration rows (R39/R40/R42).
        assert_eq!(all_rows().len(), 83);
        for (op, surface, class) in [
            ("kovee_endeavor_form", Surface::Governance, OpClass::Create),
            (
                "external_command_terminalize",
                Surface::Governance,
                OpClass::Create,
            ),
            (
                "external_command_result_query",
                Surface::Projection,
                OpClass::Read,
            ),
        ] {
            let row = lookup(op, surface).unwrap_or_else(|| panic!("{op}"));
            assert_eq!(row.class, class, "{op}");
        }
        // The delegated-principal rows exist on governance ONLY; the
        // read-only recovery query never reaches a mutation surface.
        assert!(lookup("kovee_endeavor_form", Surface::Participant).is_none());
        assert!(lookup("external_command_result_query", Surface::Governance).is_none());
    }

    #[test]
    fn dual_surface_ops_carry_exactly_two_rows() {
        for op in [
            "mandate_position",
            "act_intent_position",
            "act_intent_finalize",
            "act_intent_cancel",
        ] {
            let n = all_rows().iter().filter(|r| r.operation == op).count();
            assert_eq!(n, 2, "{op}");
            assert!(lookup(op, Surface::Participant).is_some());
            assert!(lookup(op, Surface::Governance).is_some());
        }
    }

    #[test]
    fn onboarding_authority_split_is_registry_truth() {
        // C3a negative core, decided by rows alone.
        assert!(lookup("membership_accept", Surface::Candidate).is_some());
        assert!(lookup("membership_accept", Surface::Governance).is_none());
        assert!(lookup("participant_admit", Surface::Governance).is_some());
        assert!(lookup("participant_admit", Surface::Candidate).is_none());
        assert!(lookup("assent_policy_adopt", Surface::Participant).is_some());
        assert!(lookup("assent_policy_adopt", Surface::Governance).is_none());
        assert!(lookup("activation_policy_adopt", Surface::Runtime).is_none());
    }
}
