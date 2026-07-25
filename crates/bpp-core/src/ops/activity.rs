//! ActivityStream request shapes (§11; registry R29–R31): streams with
//! enforced mandate binding, wake intents (submitted-and-pending at the
//! attached slice), and the continuation head CAS.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    check_create_meta, check_id_array, check_identifier, check_local_erasure_safe, check_op,
    check_opt_identifier, check_opt_local_erasure_safe, check_timestamp, check_update_meta,
    check_version, parse_closed,
};
use crate::digest::DigestRef;
use crate::envelope::MutationMeta;

pub const ACTIVITY_KINDS: [&str; 7] = [
    "pledge_work",
    "exploration",
    "deliberation",
    "monitoring",
    "relationship",
    "learning",
    "negotiation",
];

/// The exact committed obligation a `pledge_work` stream binds.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PledgeBinding {
    pub pledge_id: String,
    pub pledge_revision: u64,
    pub terms_digest: DigestRef,
}

/// activity_open (participant, create).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivityOpenRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub kind: String,
    pub purpose_ref: String,
    pub purpose_digest: DigestRef,
    #[serde(default)]
    pub pledge_binding: Option<PledgeBinding>,
    #[serde(default)]
    pub activation_policy_ref: Option<String>,
    #[serde(default)]
    pub activation_policy_digest: Option<DigestRef>,
    pub mandate_refs: Vec<String>,
    pub budget_account_set_ref: String,
}

impl ActivityOpenRequest {
    pub fn parse(body: &Value) -> Result<ActivityOpenRequest, String> {
        let req: ActivityOpenRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "activity_open")?;
        check_create_meta(&req.meta)?;
        if !ACTIVITY_KINDS.contains(&req.kind.as_str()) {
            return Err("kind is not a closed activity kind".to_owned());
        }
        check_identifier("purpose_ref", &req.purpose_ref)?;
        check_local_erasure_safe("purpose_digest", &req.purpose_digest)?;
        if let Some(binding) = &req.pledge_binding {
            check_identifier("pledge_binding.pledge_id", &binding.pledge_id)?;
            if binding.pledge_revision > crate::canonical::SAFE_MAX {
                return Err("pledge_binding.pledge_revision exceeds the safe range".to_owned());
            }
            check_local_erasure_safe("pledge_binding.terms_digest", &binding.terms_digest)?;
        }
        check_opt_identifier("activation_policy_ref", &req.activation_policy_ref)?;
        check_opt_local_erasure_safe("activation_policy_digest", &req.activation_policy_digest)?;
        check_id_array("mandate_refs", &req.mandate_refs, 0, 256)?;
        check_identifier("budget_account_set_ref", &req.budget_account_set_ref)?;
        Ok(req)
    }
}

/// activity_show (projection, read).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivityShowRequest {
    pub version: String,
    pub op: String,
    pub activity_stream_ref: String,
}

impl ActivityShowRequest {
    pub fn parse(body: &Value) -> Result<ActivityShowRequest, String> {
        let req: ActivityShowRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "activity_show")?;
        check_identifier("activity_stream_ref", &req.activity_stream_ref)?;
        Ok(req)
    }
}

/// activity_hold (participant, update; generation-fenced).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivityHoldRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub activity_stream_ref: String,
    pub generation: u64,
    #[serde(default)]
    pub hold_reason_ref: Option<String>,
}

impl ActivityHoldRequest {
    pub fn parse(body: &Value) -> Result<ActivityHoldRequest, String> {
        let req: ActivityHoldRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "activity_hold")?;
        check_update_meta(&req.meta)?;
        check_identifier("activity_stream_ref", &req.activity_stream_ref)?;
        if req.generation > crate::canonical::SAFE_MAX {
            return Err("generation exceeds the safe range".to_owned());
        }
        check_opt_identifier("hold_reason_ref", &req.hold_reason_ref)?;
        Ok(req)
    }
}

/// activity_close (participant, update; generation-fenced, discriminated).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivityCloseRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub activity_stream_ref: String,
    pub generation: u64,
    pub target_state: String,
    #[serde(default)]
    pub reason_ref: Option<String>,
}

impl ActivityCloseRequest {
    pub fn parse(body: &Value) -> Result<ActivityCloseRequest, String> {
        let req: ActivityCloseRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "activity_close")?;
        check_update_meta(&req.meta)?;
        check_identifier("activity_stream_ref", &req.activity_stream_ref)?;
        if req.generation > crate::canonical::SAFE_MAX {
            return Err("generation exceeds the safe range".to_owned());
        }
        if !matches!(
            req.target_state.as_str(),
            "completed" | "failed" | "canceled"
        ) {
            return Err("target_state is not a closed activity terminal".to_owned());
        }
        check_opt_identifier("reason_ref", &req.reason_ref)?;
        Ok(req)
    }
}

/// wake_intent_submit (participant, create; I0: accepted and left
/// pending — no activation machinery in this slice).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WakeIntentSubmitRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub activity_stream_ref: String,
    pub generation: u64,
    pub origin: String,
    #[serde(default)]
    pub activation_policy_ref: Option<String>,
    #[serde(default)]
    pub activation_policy_digest: Option<DigestRef>,
    pub exact_cause_ref: String,
    pub exact_cause_digest: DigestRef,
    pub purpose_ref: String,
    pub stable_wake_key: String,
    pub expires_at: String,
}

impl WakeIntentSubmitRequest {
    pub fn parse(body: &Value) -> Result<WakeIntentSubmitRequest, String> {
        let req: WakeIntentSubmitRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "wake_intent_submit")?;
        check_create_meta(&req.meta)?;
        check_identifier("activity_stream_ref", &req.activity_stream_ref)?;
        if req.generation > crate::canonical::SAFE_MAX {
            return Err("generation exceeds the safe range".to_owned());
        }
        if !matches!(
            req.origin.as_str(),
            "direct_participant" | "participant_activation_policy"
        ) {
            return Err("origin is not a closed wake origin".to_owned());
        }
        check_opt_identifier("activation_policy_ref", &req.activation_policy_ref)?;
        check_opt_local_erasure_safe("activation_policy_digest", &req.activation_policy_digest)?;
        check_identifier("exact_cause_ref", &req.exact_cause_ref)?;
        check_local_erasure_safe("exact_cause_digest", &req.exact_cause_digest)?;
        check_identifier("purpose_ref", &req.purpose_ref)?;
        check_identifier("stable_wake_key", &req.stable_wake_key)?;
        check_timestamp("expires_at", &req.expires_at)?;
        Ok(req)
    }
}

/// wake_intent_withdraw (participant, update).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WakeIntentWithdrawRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub wake_intent_ref: String,
}

impl WakeIntentWithdrawRequest {
    pub fn parse(body: &Value) -> Result<WakeIntentWithdrawRequest, String> {
        let req: WakeIntentWithdrawRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "wake_intent_withdraw")?;
        check_update_meta(&req.meta)?;
        check_identifier("wake_intent_ref", &req.wake_intent_ref)?;
        Ok(req)
    }
}

/// continuation_write (participant, create; §11.3 head CAS through
/// `expected_head_revision` — create-classed, the CAS is a body field).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContinuationWriteRequest {
    pub version: String,
    pub op: String,
    pub meta: MutationMeta,
    pub activity_stream_ref: String,
    pub generation: u64,
    pub summary_ref: String,
    pub unresolved_refs: Vec<String>,
    pub exact_state_refs: Vec<String>,
    pub source_event_cursor: String,
    #[serde(default)]
    pub prior_continuation_ref: Option<String>,
    #[serde(default)]
    pub prior_continuation_digest: Option<DigestRef>,
    pub expected_head_revision: u64,
    pub classification_ref: String,
    #[serde(default)]
    pub episode_ref: Option<String>,
    #[serde(default)]
    pub byom_fence_epoch: Option<u64>,
}

impl ContinuationWriteRequest {
    pub fn parse(body: &Value) -> Result<ContinuationWriteRequest, String> {
        let req: ContinuationWriteRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "continuation_write")?;
        check_create_meta(&req.meta)?;
        check_identifier("activity_stream_ref", &req.activity_stream_ref)?;
        if req.generation > crate::canonical::SAFE_MAX
            || req.expected_head_revision > crate::canonical::SAFE_MAX
        {
            return Err("generation/expected_head_revision exceeds the safe range".to_owned());
        }
        check_identifier("summary_ref", &req.summary_ref)?;
        check_id_array("unresolved_refs", &req.unresolved_refs, 0, 256)?;
        check_id_array("exact_state_refs", &req.exact_state_refs, 0, 256)?;
        check_identifier("source_event_cursor", &req.source_event_cursor)?;
        check_opt_identifier("prior_continuation_ref", &req.prior_continuation_ref)?;
        check_opt_local_erasure_safe("prior_continuation_digest", &req.prior_continuation_digest)?;
        check_identifier("classification_ref", &req.classification_ref)?;
        check_opt_identifier("episode_ref", &req.episode_ref)?;
        Ok(req)
    }
}
