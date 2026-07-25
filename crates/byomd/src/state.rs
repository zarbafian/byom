//! Shared problem constructors and small helpers: the daemon's §14.9
//! problem-kind pins for conditions DESIGN.md names without fixing a
//! kind. Each pin is recorded here once so tests and handlers agree.

use bpp_core::problem::{Problem, ProblemKind};

/// An operation absent from the frozen registry bundle at the
/// negotiated version (§14.6): not callable anywhere.
pub fn unknown_op() -> Problem {
    Problem::new(
        ProblemKind::FeatureUnavailable,
        "operation absent from the negotiated bundle",
    )
    .with_status(501)
}

/// An operation whose registry rows bind other surfaces only: the
/// deny-by-absence answer decided by the (operation,surface) rows (G35;
/// B1 sheet: self-policy adoption on governance/runtime is
/// forbidden-surface).
pub fn forbidden_surface() -> Problem {
    Problem::new(
        ProblemKind::ForbiddenSurface,
        "operation is not bound to this surface",
    )
    .with_status(403)
}

/// Non-enumerating §14.9 not-found: never discloses hidden existence.
pub fn not_found() -> Problem {
    Problem::new(ProblemKind::NotFound, "no such record").with_status(404)
}

/// Non-enumerating authorization refusal (invalid/closed credentials).
pub fn forbidden() -> Problem {
    Problem::new(ProblemKind::Forbidden, "forbidden").with_status(403)
}

/// An authorization refusal that may safely name its reason (the caller
/// already sees the record it is insufficiently authorized for).
pub fn forbidden_detail(detail: &str) -> Problem {
    Problem::new(ProblemKind::Forbidden, "forbidden")
        .with_status(403)
        .with_detail(detail.to_owned())
}

/// The §15.3 sealed endpoint: every non-diagnostic surface refuses.
pub fn endpoint_sealed() -> Problem {
    Problem::new(
        ProblemKind::EndpointSealed,
        "endpoint is sealed_diagnostic; authority surfaces are closed",
    )
    .with_status(503)
}

pub fn stale_revision() -> Problem {
    Problem::new(
        ProblemKind::StaleRevision,
        "expected revision is no longer current",
    )
    .with_status(409)
}

/// A request bound to a superseded binding: an old incarnation or
/// recovery epoch, a superseded acceptance, or a terminally fenced offer
/// (§7.4: no terminal offer can later admit).
pub fn stale_binding(detail: &str) -> Problem {
    Problem::new(ProblemKind::StaleBinding, "binding is no longer current")
        .with_status(409)
        .with_detail(detail.to_owned())
}

pub fn invalid(detail: &str) -> Problem {
    Problem::new(ProblemKind::Invalid, "invalid request")
        .with_status(400)
        .with_detail(detail.to_owned())
}

pub fn internal(detail: &str) -> Problem {
    Problem::new(ProblemKind::Internal, "internal fault")
        .with_status(500)
        .with_detail(detail.to_owned())
}
