//! The remaining read shapes (§14.4/§15; registry R4/R37–R41):
//! snapshots, the events long-poll, sensitive payload reads, and the
//! originating-surface recovery reads.

use serde::Deserialize;
use serde_json::Value;

use super::{
    check_id_array, check_identifier, check_op, check_opt_identifier, check_version, parse_closed,
};
use crate::digest::{DigestClass, DigestRef};
use crate::envelope::is_operation_id;
use crate::limits::EVENTS_PAGE_ITEMS_MAX;

fn check_continuation(token: &str) -> Result<(), String> {
    if token.is_empty() || token.len() > 4096 || !token.bytes().all(|b| (0x21..=0x7e).contains(&b))
    {
        return Err("continuation is not an opaque visible-ASCII token".to_owned());
    }
    Ok(())
}

/// snapshot_get (projection, read).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotGetRequest {
    pub version: String,
    pub op: String,
    pub society_id: String,
    #[serde(default)]
    pub kinds: Option<Vec<String>>,
    #[serde(default)]
    pub endeavor_ref: Option<String>,
}

impl SnapshotGetRequest {
    pub fn parse(body: &Value) -> Result<SnapshotGetRequest, String> {
        let req: SnapshotGetRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "snapshot_get")?;
        check_identifier("society_id", &req.society_id)?;
        if let Some(kinds) = &req.kinds {
            check_id_array("kinds", kinds, 0, 256)?;
        }
        check_opt_identifier("endeavor_ref", &req.endeavor_ref)?;
        Ok(req)
    }
}

/// events_wait (projection, read; bounded long-poll, 1..=60000 ms).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventsWaitRequest {
    pub version: String,
    pub op: String,
    pub continuation: String,
    pub page_size: u64,
    pub max_wait_milliseconds: u64,
}

impl EventsWaitRequest {
    pub fn parse(body: &Value) -> Result<EventsWaitRequest, String> {
        let req: EventsWaitRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "events_wait")?;
        check_continuation(&req.continuation)?;
        if req.page_size < 1 || req.page_size > EVENTS_PAGE_ITEMS_MAX {
            return Err("maximum 512 events per page (§14.9)".to_owned());
        }
        if req.max_wait_milliseconds < 1 || req.max_wait_milliseconds > 60_000 {
            return Err("max_wait_milliseconds is out of bounds (1..=60000)".to_owned());
        }
        Ok(req)
    }
}

/// event_payload (projection, read; a SENSITIVE read — released only
/// behind a committed PrivacyAccessRecord, §15.4).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventPayloadRequest {
    pub version: String,
    pub op: String,
    pub event_id: String,
    #[serde(default)]
    pub payload_digest: Option<DigestRef>,
}

impl EventPayloadRequest {
    pub fn parse(body: &Value) -> Result<EventPayloadRequest, String> {
        let req: EventPayloadRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "event_payload")?;
        check_identifier("event_id", &req.event_id)?;
        if let Some(d) = &req.payload_digest {
            d.require_class(DigestClass::LocalErasureSafe)
                .map_err(|e| format!("payload_digest: {e}"))?;
        }
        Ok(req)
    }
}

/// idempotency_result (originating surface, read; never re-executes).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdempotencyResultRequest {
    pub version: String,
    pub op: String,
    pub operation: String,
    pub idempotency_key: String,
}

impl IdempotencyResultRequest {
    pub fn parse(body: &Value) -> Result<IdempotencyResultRequest, String> {
        let req: IdempotencyResultRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "idempotency_result")?;
        if !is_operation_id(&req.operation) {
            return Err("operation is not an operation id".to_owned());
        }
        check_identifier("idempotency_key", &req.idempotency_key)?;
        Ok(req)
    }
}

/// cursor_recover (originating surface, read).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CursorRecoverRequest {
    pub version: String,
    pub op: String,
    pub continuation: String,
}

impl CursorRecoverRequest {
    pub fn parse(body: &Value) -> Result<CursorRecoverRequest, String> {
        let req: CursorRecoverRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "cursor_recover")?;
        check_continuation(&req.continuation)?;
        Ok(req)
    }
}

/// recovery_checkpoint_show (projection, read; part of the diagnostic
/// remainder — it still answers on a sealed endpoint).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryCheckpointShowRequest {
    pub version: String,
    pub op: String,
    #[serde(default)]
    pub society_id: Option<String>,
}

impl RecoveryCheckpointShowRequest {
    pub fn parse(body: &Value) -> Result<RecoveryCheckpointShowRequest, String> {
        let req: RecoveryCheckpointShowRequest = parse_closed(body)?;
        check_version(&req.version)?;
        check_op(&req.op, "recovery_checkpoint_show")?;
        check_opt_identifier("society_id", &req.society_id)?;
        Ok(req)
    }
}
