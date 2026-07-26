//! The §13.1 act/effect request shapes (B3 slice 3; registry R19-R23 and
//! R34): `act_intent_prepare`, `act_intent_finalize`, and the one-shot
//! `execution_permit_consume`. `act_intent_position` reuses the one shared
//! closed `PositionRequest` (flat assent-mode optionals), exactly as
//! `mandate_position` does.
//!
//! What you write (the one-shot consumption; both fences always). Every
//! digest here is KOVEE's own — byom recomputes its own from committed
//! state and never asks for it (A8, PROFILE.md §6.2):
//! ```
//! use bpp_core::ops::ExecutionPermitConsumeRequest;
//! let p = |v: u8| serde_json::json!({
//!     "class": "portable_public", "algorithm": "sha-256",
//!     "value_hex": format!("{v:02x}").repeat(32)});
//! let body = serde_json::json!({
//!     "version": "0.2", "op": "execution_permit_consume",
//!     "meta": {"request_id": "r", "idempotency_key": "k",
//!              "expected_endpoint_incarnation": "inc",
//!              "expected_recovery_epoch": 0, "expected_revision": 3},
//!     "stable_execution_key": "exec-key-1",
//!     "intent_ref": "intent-1",
//!     "host_effect_ref": "kovee-effect-1", "host_effect_digest": p(0x2a),
//!     "host_effect_credential": "b".repeat(64),
//!     // The two host-owned members of the frozen binding fragment byom
//!     // rebuilds, so `host_effect_digest` is DERIVED here, not asserted:
//!     "host_effect_external_idempotency_key": "kovee-model-exec-key-1-2a2a2a2a2a2a2a2a",
//!     "host_effect_request_byte_digest": p(0x2a),
//!     "driver_audience": "kovee-model-broker",
//!     "budget_reservation_set_ref": "rset-1",
//!     "byom_fence_epoch": 3, "host_fence_epoch": 5});
//! let req = ExecutionPermitConsumeRequest::parse(&body).unwrap();
//! assert_eq!(req.max_uses_is_one(), true);
//! // byom's OWN committed digests are not request members at all (A8):
//! // an act subject echoed back is refused, never quietly ignored.
//! let mut echo = body.clone();
//! echo.as_object_mut().unwrap()
//!     .insert("subject_digest".into(), p(0x3b));
//! assert!(ExecutionPermitConsumeRequest::parse(&echo).is_err());
//! // BOTH host manifests travel as all-or-none ref/digest pairs (the
//! // frozen oneOf arms): context, exactly like disclosure.
//! for half in ["disclosure_manifest_ref", "context_manifest_ref"] {
//!     let mut body = body.clone();
//!     body.as_object_mut().unwrap()
//!         .insert(half.into(), serde_json::json!("m-1"));
//!     assert!(ExecutionPermitConsumeRequest::parse(&body).is_err());
//! }
//! ```

use serde::Deserialize;
use serde_json::Value;

use super::{
    check_create_meta, check_identifier, check_local_erasure_safe, check_op, check_opt_identifier,
    check_update_meta, check_version, parse_closed,
};
use crate::canonical::SAFE_MAX;
use crate::digest::{DigestClass, DigestRef};
use crate::envelope::MutationMeta;

/// The CROSS-BOUNDARY class rule of the family contract (§A8, PROFILE.md
/// §6.2), applied to the act family in BOTH directions:
///
/// - a digest byom **demands from kovee** is `portable_public` over a
///   frozen cross-boundary fragment — a keyed class there is an HMAC under
///   kovee's own per-object secret, which byom could only echo;
/// - a digest byom **recomputes from its own committed state** stays
///   `local_erasure_safe` and is **not a request member at all**
///   (`intent_digest`, `subject_digest` and `episode_fence_digest` are
///   gone from `execution_permit_consume` for exactly that reason).
fn check_portable(name: &str, d: &DigestRef) -> Result<(), String> {
    d.require_class(DigestClass::PortablePublic)
        .map_err(|e| format!("{name}: {e}"))
}

fn check_opt_portable(name: &str, d: &Option<DigestRef>) -> Result<(), String> {
    match d {
        Some(d) => check_portable(name, d),
        None => Ok(()),
    }
}

/// A 32-byte authenticator on the wire: exactly 64 lowercase hex digits,
/// carrying no key material of its own.
fn check_credential(name: &str, v: &str) -> Result<(), String> {
    if v.len() == 64
        && v.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        Ok(())
    } else {
        Err(format!(
            "{name} is not a 64-character lowercase hex authenticator"
        ))
    }
}

/// The closed Δ4 act-class list (family contract §4, verbatim), carried in
/// ActIntent subjects. `kind` stays an open identifier on the frozen wire
/// (gap note G34); a `kind` that IS one of these five compiles the Δ4
/// class subject, and only such an act can reach a class-bound driver.
pub const ACT_CLASSES: [&str; 5] = ["model_egress", "share", "outbound", "apply", "budget"];

/// The mandatory BPA-1 request domains per act class
/// (`spec/governed-work/act-class-subject.schema.json` oneOf arms,
/// transcribed verbatim — the C2 deliverable). Extra domains only narrow
/// further and are always allowed; a missing mandatory domain fails
/// closed.
pub fn mandatory_domains(act_class: &str) -> Option<&'static [&'static str]> {
    Some(match act_class {
        "model_egress" => &[
            "operation",
            "purpose",
            "binding",
            "classification",
            "quantity",
        ],
        "share" => &["operation", "purpose", "object", "classification"],
        "outbound" => &[
            "operation",
            "purpose",
            "network_destination",
            "classification",
        ],
        "apply" => &["operation", "purpose", "object", "path", "schema_evidence"],
        "budget" => &["operation", "purpose", "quantity"],
        _ => return None,
    })
}

fn check_safe(name: &str, v: u64) -> Result<(), String> {
    if v > SAFE_MAX {
        return Err(format!("{name} exceeds the safe range"));
    }
    Ok(())
}

// ------------------------------------------------- act_intent_prepare ----

/// act_intent_prepare (participant, create; R19). `intent_id`,
/// `requested_by_participant`, `actor_ref`, `subject_digest`,
/// `preconditions`, `stable_execution_key`, `budget_reservation_set_ref`,
/// the authorization dependency set, the Δ4 class subject, and
/// `expires_at` are ALL server-derived: subject atoms are compiled by the
/// kernel from the dependency closure (§10.6), never caller-shaped.
///
/// The context and disclosure manifests are the HOST's objects, named here
/// with their exact digests. Both pairs enter the assented act subject and
/// are compared again, member for member, when the permit is consumed
/// (R3-A01), so their class is the cross-boundary `portable_public`: the
/// consuming host has to present the identical value, and a keyed digest
/// under one side's per-object secret could never be compared at all.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActIntentPrepareRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub kind: String,
    pub execution_kind: String,
    pub subject_ref: String,
    pub subject_revision: u64,
    #[serde(default)]
    pub endeavor_ref: Option<String>,
    #[serde(default)]
    pub pledge_ref: Option<String>,
    pub mandate_ref: String,
    pub mandate_revision: u64,
    pub mandate_digest: DigestRef,
    #[serde(default)]
    pub context_manifest_ref: Option<String>,
    #[serde(default)]
    pub context_manifest_digest: Option<DigestRef>,
    #[serde(default)]
    pub disclosure_manifest_ref: Option<String>,
    #[serde(default)]
    pub disclosure_manifest_digest: Option<DigestRef>,
    #[serde(default)]
    pub driver_audience: Option<String>,
}

impl ActIntentPrepareRequest {
    pub fn parse(body: &Value) -> Result<ActIntentPrepareRequest, String> {
        let req: ActIntentPrepareRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "act_intent_prepare")?;
        check_create_meta(&req.meta)?;
        check_identifier("kind", &req.kind)?;
        if !matches!(
            req.execution_kind.as_str(),
            "domain_transition" | "external_effect"
        ) {
            return Err("execution_kind is not domain_transition|external_effect".to_owned());
        }
        check_identifier("subject_ref", &req.subject_ref)?;
        check_safe("subject_revision", req.subject_revision)?;
        check_opt_identifier("endeavor_ref", &req.endeavor_ref)?;
        check_opt_identifier("pledge_ref", &req.pledge_ref)?;
        check_identifier("mandate_ref", &req.mandate_ref)?;
        check_safe("mandate_revision", req.mandate_revision)?;
        check_local_erasure_safe("mandate_digest", &req.mandate_digest)?;
        check_opt_identifier("context_manifest_ref", &req.context_manifest_ref)?;
        check_opt_portable("context_manifest_digest", &req.context_manifest_digest)?;
        check_opt_identifier("disclosure_manifest_ref", &req.disclosure_manifest_ref)?;
        check_opt_portable(
            "disclosure_manifest_digest",
            &req.disclosure_manifest_digest,
        )?;
        check_opt_identifier("driver_audience", &req.driver_audience)?;
        // Each manifest binding is an all-or-none pair: a reference the
        // subject carries without its digest is exactly the disclosure a
        // later consumption could substitute (R3-A01).
        if req.context_manifest_ref.is_some() != req.context_manifest_digest.is_some() {
            return Err("context_manifest ref/digest is an all-or-none pair".to_owned());
        }
        if req.disclosure_manifest_ref.is_some() != req.disclosure_manifest_digest.is_some() {
            return Err("disclosure_manifest ref/digest is an all-or-none pair".to_owned());
        }
        Ok(req)
    }

    /// The Δ4 act class this preparation compiles a class subject for, if
    /// `kind` names one.
    pub fn act_class(&self) -> Option<&str> {
        ACT_CLASSES
            .iter()
            .find(|c| **c == self.kind)
            .map(|c| *c as &str)
    }
}

// ------------------------------------------------ act_intent_finalize ----

/// act_intent_finalize (participant + governance, update; R22/R23):
/// deterministic finalization over the exact prepared intent digest. The
/// caller authors no seat — supplying one fails the closed shape.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActIntentFinalizeRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub intent_id: String,
    pub subject_digest: DigestRef,
}

impl ActIntentFinalizeRequest {
    pub fn parse(body: &Value) -> Result<ActIntentFinalizeRequest, String> {
        let req: ActIntentFinalizeRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "act_intent_finalize")?;
        check_update_meta(&req.meta)?;
        check_identifier("intent_id", &req.intent_id)?;
        check_local_erasure_safe("subject_digest", &req.subject_digest)?;
        Ok(req)
    }
}

// ------------------------------------------- execution_permit_consume ----

/// execution_permit_consume (runtime, update; R34): the trusted host
/// effect service bound to the exact prepared host Effect, presenting the
/// one-shot key and BOTH fences. Byom atomically rechecks charter,
/// standing, Mandate, decisions, dependencies, ceilings, expiry and both
/// fences, inserts the MandateUse once, and returns ONE immutable
/// ExecutionConsumptionReceipt (`max_uses: 1`).
///
/// The member set is A8's, per member (R3-L01):
///
/// - **byom's own** `intent_digest`, `subject_digest` and
///   `episode_fence_digest` are NOT members. byom recomputes each from its
///   committed ActIntent, act subject and ByomEpisodeBinding, and publishes
///   the committed value on the receipt. Echoing them back proved nothing —
///   byom compared its own value against itself — while forcing the host to
///   store per-object keyed digests it can never verify.
/// - **kovee's own** `host_effect_digest`, `context_digest` and
///   `disclosure_digest` are `portable_public` over frozen cross-boundary
///   fragments, so byom holds the same bytes the host does. This is also the
///   class `effect_outcome_admit` already demands for `host_effect_digest`,
///   so the permit and the later outcome admission now name the SAME value.
///   BOTH host manifests the act subject pins — context AND disclosure —
///   are presented here and compared, ref and digest, against the pair the
///   seats assented to (R3-A01). A context the consumption never presents
///   is a context nothing binds: the seat assented to "this act, under
///   that context", and the permit is consumable only under it.
/// - `host_effect_credential` binds the permit to one exact prepared host
///   Effect (R3-A02): the authenticator over
///   {intent_ref, stable_execution_key, host_effect_ref, host_effect_digest}
///   under the permit channel credential byomd itself published. Without it
///   the request merely *stored* a caller-chosen effect ref and digest.
/// - `host_effect_external_idempotency_key` and
///   `host_effect_request_byte_digest` are the two members of the host's
///   FROZEN binding fragment byom does not already hold (R3-L01, D-R3-3).
///   With them byom rebuilds the whole `kovee-host-effect-binding-v1`
///   preimage — every other member is read from its own committed ActIntent —
///   and re-derives `host_effect_digest` instead of storing an assertion.
///   Authentication proved only that the addressed host sent the value; it
///   never tied that value to anything both sides hold.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionPermitConsumeRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub stable_execution_key: String,
    pub intent_ref: String,
    pub host_effect_ref: String,
    pub host_effect_digest: DigestRef,
    pub host_effect_credential: String,
    pub host_effect_external_idempotency_key: String,
    pub host_effect_request_byte_digest: DigestRef,
    #[serde(default)]
    pub context_manifest_ref: Option<String>,
    #[serde(default)]
    pub context_digest: Option<DigestRef>,
    #[serde(default)]
    pub disclosure_manifest_ref: Option<String>,
    #[serde(default)]
    pub disclosure_digest: Option<DigestRef>,
    pub driver_audience: String,
    pub budget_reservation_set_ref: String,
    #[serde(default)]
    pub episode_ref: Option<String>,
    pub byom_fence_epoch: u64,
    pub host_fence_epoch: u64,
}

impl ExecutionPermitConsumeRequest {
    pub fn parse(body: &Value) -> Result<ExecutionPermitConsumeRequest, String> {
        let req: ExecutionPermitConsumeRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "execution_permit_consume")?;
        check_update_meta(&req.meta)?;
        check_identifier("stable_execution_key", &req.stable_execution_key)?;
        check_identifier("intent_ref", &req.intent_ref)?;
        check_identifier("host_effect_ref", &req.host_effect_ref)?;
        check_portable("host_effect_digest", &req.host_effect_digest)?;
        check_credential("host_effect_credential", &req.host_effect_credential)?;
        check_identifier(
            "host_effect_external_idempotency_key",
            &req.host_effect_external_idempotency_key,
        )?;
        check_portable(
            "host_effect_request_byte_digest",
            &req.host_effect_request_byte_digest,
        )?;
        check_opt_identifier("context_manifest_ref", &req.context_manifest_ref)?;
        check_opt_portable("context_digest", &req.context_digest)?;
        check_opt_identifier("disclosure_manifest_ref", &req.disclosure_manifest_ref)?;
        check_opt_portable("disclosure_digest", &req.disclosure_digest)?;
        check_identifier("driver_audience", &req.driver_audience)?;
        check_identifier(
            "budget_reservation_set_ref",
            &req.budget_reservation_set_ref,
        )?;
        check_opt_identifier("episode_ref", &req.episode_ref)?;
        check_safe("byom_fence_epoch", req.byom_fence_epoch)?;
        check_safe("host_fence_epoch", req.host_fence_epoch)?;
        // The frozen oneOf arms: each manifest binding is an all-or-none
        // pair. A reference presented without its digest is exactly the
        // substitution the pair exists to refuse (R3-A01).
        if req.context_manifest_ref.is_some() != req.context_digest.is_some() {
            return Err("context_manifest ref/digest is an all-or-none pair".to_owned());
        }
        if req.disclosure_manifest_ref.is_some() != req.disclosure_digest.is_some() {
            return Err("disclosure_manifest ref/digest is an all-or-none pair".to_owned());
        }
        Ok(req)
    }

    /// `max_uses` is 1 BY CONSTRUCTION — never a request field
    /// (§13.1: one-shot consumption).
    pub fn max_uses_is_one(&self) -> bool {
        true
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// The committed Δ4 taxonomy contract this module claims to implement.
    const ACT_CLASS_SUBJECT: &str =
        include_str!("../../../../spec/governed-work/act-class-subject.schema.json");

    #[test]
    fn the_mandatory_domain_table_is_the_committed_taxonomy() {
        // The C2 deliverable is the per-class mandatory-domain pinning; the
        // kernel's compiler and the frozen schema must agree EXACTLY, in
        // both directions, or a schema-valid subject could be compiled that
        // the driver refuses (or worse, one it accepts).
        let schema: Value = serde_json::from_str(ACT_CLASS_SUBJECT).unwrap();
        let arms = schema["oneOf"].as_array().expect("the closed class arms");
        assert_eq!(arms.len(), ACT_CLASSES.len(), "one arm per act class");
        for arm in arms {
            let class = arm["properties"]["act_class"]["const"]
                .as_str()
                .expect("each arm pins its class");
            assert!(ACT_CLASSES.contains(&class), "{class} is not a Δ4 class");
            let want: Vec<&str> = arm["properties"]["subject_atoms"]["required"]
                .as_array()
                .expect("each arm pins its mandatory domains")
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect();
            assert_eq!(
                mandatory_domains(class).unwrap(),
                want.as_slice(),
                "{class}: the compiler's mandatory domains diverge from the \
                 committed taxonomy"
            );
        }
        // Every class the compiler knows is an arm of the closed union.
        for class in ACT_CLASSES {
            assert!(
                arms.iter()
                    .any(|a| a["properties"]["act_class"]["const"] == class),
                "{class} has no committed arm"
            );
            assert!(mandatory_domains(class).is_some());
        }
        // And nothing else is an act class.
        assert!(mandatory_domains("legacy_tool_call").is_none());
        assert!(mandatory_domains("").is_none());
    }

    fn consume_body() -> Value {
        let portable = |v: u8| {
            serde_json::json!({"class": "portable_public", "algorithm": "sha-256",
                               "value_hex": format!("{v:02x}").repeat(32)})
        };
        serde_json::json!({
            "version": "0.2", "op": "execution_permit_consume",
            "meta": {"request_id": "r", "idempotency_key": "k",
                     "expected_endpoint_incarnation": "inc",
                     "expected_recovery_epoch": 0, "expected_revision": 3},
            "stable_execution_key": "exec-key-1",
            "intent_ref": "intent-1",
            "host_effect_ref": "kovee-effect-1",
            "host_effect_digest": portable(0x2a),
            "host_effect_credential": "b".repeat(64),
            "host_effect_external_idempotency_key": "kovee-model-exec-key-1-2a2a2a2a2a2a2a2a",
            "host_effect_request_byte_digest": portable(0x2a),
            "context_manifest_ref": "kovee-context-1",
            "context_digest": portable(0x3e),
            "disclosure_manifest_ref": "kovee-disclosure-1",
            "disclosure_digest": portable(0x7d),
            "driver_audience": "kovee-model-broker",
            "budget_reservation_set_ref": "rset-1",
            "episode_ref": "ep-1",
            "byom_fence_epoch": 3, "host_fence_epoch": 5})
    }

    #[test]
    fn the_consume_request_applies_a8_per_member() {
        let keyed = serde_json::json!({
            "class": "local_erasure_safe", "algorithm": "hmac-sha-256",
            "key_ref": "kovee-object:1", "value_hex": "9".repeat(64)});
        assert!(ExecutionPermitConsumeRequest::parse(&consume_body()).is_ok());
        // The converse half: byom's OWN recomputed digests are not members.
        for owned in ["intent_digest", "subject_digest", "episode_fence_digest"] {
            let mut body = consume_body();
            body.as_object_mut()
                .unwrap()
                .insert(owned.to_owned(), keyed.clone());
            assert!(
                ExecutionPermitConsumeRequest::parse(&body).is_err(),
                "{owned} is byom's own recomputed digest: the closed shape must refuse it"
            );
        }
        // The demanded half: a host-owned digest byom must verify travels as
        // a frozen portable_public fragment, never a keyed blob.
        for peer in [
            "host_effect_digest",
            "host_effect_request_byte_digest",
            "context_digest",
            "disclosure_digest",
        ] {
            let mut body = consume_body();
            body.as_object_mut()
                .unwrap()
                .insert(peer.to_owned(), keyed.clone());
            assert!(
                ExecutionPermitConsumeRequest::parse(&body).is_err(),
                "{peer} must be portable_public: byom holds no key for the host's secret"
            );
        }
        // The registration credential is a 32-byte hex authenticator, and it
        // is required: an unregistered host Effect never reaches the state.
        for bad in ["", "not-hex", &"A".repeat(64), &"a".repeat(63)] {
            let mut body = consume_body();
            body.as_object_mut()
                .unwrap()
                .insert("host_effect_credential".to_owned(), serde_json::json!(bad));
            assert!(
                ExecutionPermitConsumeRequest::parse(&body).is_err(),
                "{bad:?}"
            );
        }
        let mut body = consume_body();
        body.as_object_mut()
            .unwrap()
            .remove("host_effect_credential");
        assert!(ExecutionPermitConsumeRequest::parse(&body).is_err());
        // The episode reference now travels ALONE: its fence digest is
        // byom's own committed record.
        let mut body = consume_body();
        body.as_object_mut().unwrap().remove("episode_ref");
        assert!(ExecutionPermitConsumeRequest::parse(&body).is_ok());
        // BOTH host manifests are all-or-none pairs, and BOTH are members:
        // a consumption that names a context without pinning its content —
        // or pins content without naming the manifest — is refused here,
        // before any state is read (R3-A01).
        for (present, absent) in [
            ("context_manifest_ref", "context_digest"),
            ("context_digest", "context_manifest_ref"),
            ("disclosure_manifest_ref", "disclosure_digest"),
            ("disclosure_digest", "disclosure_manifest_ref"),
        ] {
            let mut body = consume_body();
            body.as_object_mut().unwrap().remove(absent);
            assert!(
                ExecutionPermitConsumeRequest::parse(&body).is_err(),
                "{present} without {absent} must fail the closed pair"
            );
        }
        // Both pairs dropped together is a shape the parser accepts — the
        // ACT decides whether a consumption without them can proceed.
        let mut body = consume_body();
        let members = body.as_object_mut().unwrap();
        for name in [
            "context_manifest_ref",
            "context_digest",
            "disclosure_manifest_ref",
            "disclosure_digest",
        ] {
            members.remove(name);
        }
        assert!(ExecutionPermitConsumeRequest::parse(&body).is_ok());
        // The context pair is presented under the SAME names the act
        // subject pins, so the two can be compared member for member.
        let req = ExecutionPermitConsumeRequest::parse(&consume_body()).unwrap();
        assert_eq!(req.context_manifest_ref.as_deref(), Some("kovee-context-1"));
        assert_eq!(
            req.context_digest.map(|d| d.value_hex),
            Some("3e".repeat(32))
        );
    }

    #[test]
    fn each_prepared_manifest_binding_is_an_all_or_none_portable_pair() {
        let portable = serde_json::json!({
            "class": "portable_public", "algorithm": "sha-256",
            "value_hex": "e2".repeat(32)});
        let keyed = serde_json::json!({
            "class": "local_erasure_safe", "algorithm": "hmac-sha-256",
            "key_ref": "society-key:soc-1/object:mandate-1", "value_hex": "9".repeat(64)});
        let base = serde_json::json!({
            "version": "0.2", "op": "act_intent_prepare",
            "meta": {"request_id": "r", "idempotency_key": "k",
                     "expected_endpoint_incarnation": "inc",
                     "expected_recovery_epoch": 0},
            "kind": "model_egress", "execution_kind": "external_effect",
            "subject_ref": "subject-1", "subject_revision": 1,
            "mandate_ref": "mandate-1", "mandate_revision": 2,
            "mandate_digest": keyed,
            "driver_audience": "kovee-model-broker"});
        assert!(ActIntentPrepareRequest::parse(&base).is_ok());
        let with = |members: Vec<(&str, Value)>| {
            let mut body = base.clone();
            for (name, value) in members {
                body.as_object_mut().unwrap().insert(name.to_owned(), value);
            }
            body
        };
        assert!(ActIntentPrepareRequest::parse(&with(vec![
            ("disclosure_manifest_ref", serde_json::json!("d-1")),
            ("disclosure_manifest_digest", portable.clone()),
        ]))
        .is_ok());
        // A reference without its digest is the substitution the pair
        // exists to refuse (R3-A01).
        assert!(ActIntentPrepareRequest::parse(&with(vec![(
            "disclosure_manifest_ref",
            serde_json::json!("d-1")
        )]))
        .is_err());
        assert!(ActIntentPrepareRequest::parse(&with(vec![(
            "context_manifest_digest",
            portable.clone()
        )]))
        .is_err());
        // And the manifest digests are the HOST's: portable_public only.
        assert!(ActIntentPrepareRequest::parse(&with(vec![
            ("disclosure_manifest_ref", serde_json::json!("d-1")),
            ("disclosure_manifest_digest", keyed.clone()),
        ]))
        .is_err());
    }
}
