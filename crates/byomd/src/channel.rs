//! Sender-constrained channel proofs (BY-C1, DESIGN.md §2342-2358).
//!
//! The candidate and participant credentials used to be a PLAINTEXT
//! REUSABLE BEARER TOKEN kept in SQLite and in a `0600` file: possession
//! alone resolved the actor, so a copied token worked from any other
//! same-UID process, for any operation, forever.
//!
//! What the client now presents is a PROOF, not a token — a MAC over the
//! exact binding of THIS call, computed under a channel proof key the
//! client holds:
//!
//! ```text
//! bpx1.<channel_id>.<nonce hex>.<issued_at>.<mac hex>
//! ```
//!
//! ```text
//! mac = hmac-sha-256(proof key, JCS {
//!     $domain: "bpp-channel-proof-v0",
//!     audience, channel_id, scope_ref, operation, binding_ref,
//!     fence_epoch, peer_pid, peer_process_start, nonce, issued_at })
//! ```
//!
//! The proof is bound to the CONNECTION (the peer process's pid and its
//! kernel start time, read from `SO_PEERCRED` and `/proc`), the exact
//! offer/participant scope, the Manifestation/control binding, the
//! current onboarding fence epoch, the audience, the exact operation and
//! a short expiry, and each nonce is spent once. A proof copied out of
//! one process therefore fails in another, and a proof for one operation
//! does not authorize a different one.
//!
//! What the store keeps is a VERIFIER reference — `proof_key_id` plus
//! the binding a presented proof must commit to — never a credential a
//! holder could replay. HONEST PROFILE LABEL: the developer profile has
//! no asymmetric endpoint identity (§19), so the proof key is derived
//! from a store scope key rather than being a public key; a reader of
//! the same-UID database is inside the trust boundary either way.

use bpp_core::canonical::{hex, hmac_sha256, sha256_hex, tagged_canonical};
use bpp_core::problem::Problem;
use byom_store::rows::{self, ChannelRow};
use byom_store::Store;
use serde_json::json;

use crate::state;

/// The proof wire tag.
pub const PROOF_PREFIX: &str = "bpx1.";
/// A presented proof is accepted within this many seconds of issue.
pub const PROOF_MAX_AGE_SECS: i64 = 120;
/// The proof domain tag.
const PROOF_TAG: &str = "bpp-channel-proof-v0";

/// The observed peer of one connection: what the proof is bound to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Peer {
    pub pid: i32,
    /// The peer process's kernel start time (`/proc/<pid>/stat` field
    /// 22): pins the exact process, not a recycled pid.
    pub process_start: u64,
}

impl Peer {
    /// The peer of the CURRENT process (what a client binds its own
    /// proof to).
    pub fn current() -> Peer {
        let pid = std::process::id() as i32;
        Peer {
            pid,
            process_start: process_start(pid),
        }
    }
}

/// Reads a process's kernel start time; 0 when unreadable (the binding
/// then rests on the pid alone, still per-process).
pub fn process_start(pid: i32) -> u64 {
    let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
        return 0;
    };
    // Field 2 (comm) may contain spaces inside parentheses; fields are
    // counted after the closing one.
    let Some((_, rest)) = stat.rsplit_once(')') else {
        return 0;
    };
    rest.split_whitespace()
        .nth(19)
        .and_then(|f| f.parse().ok())
        .unwrap_or(0)
}

/// The audience of a channel credential.
pub const AUDIENCE_CANDIDATE: &str = "candidate";
pub const AUDIENCE_PARTICIPANT: &str = "participant";

/// The exact operations a candidate credential authorizes (§7.4): the
/// candidate surface plus the originating-surface recovery reads.
pub fn candidate_operations() -> Vec<String> {
    operations_of(bpp_core::registry::Surface::Candidate)
}

/// The exact operations a participant credential authorizes.
pub fn participant_operations() -> Vec<String> {
    operations_of(bpp_core::registry::Surface::Participant)
}

fn operations_of(surface: bpp_core::registry::Surface) -> Vec<String> {
    let mut out: Vec<String> = crate::reads::SLICE1_OPS
        .iter()
        .chain(crate::reads::SLICE2_OPS.iter())
        .chain(crate::reads::SLICE2_RECOVERY_OPS.iter())
        .filter(|op| {
            bpp_core::registry::lookup(op, surface).is_some()
                || bpp_core::registry::lookup(op, bpp_core::registry::Surface::Originating)
                    .is_some()
        })
        .map(|op| (*op).to_owned())
        .collect();
    out.sort();
    out.dedup();
    out
}

/// The per-channel proof key. Derived from a store scope key so no
/// bearer credential is retained; the client holds the key in its
/// `0600` channel file.
pub fn proof_key(store: &Store, channel_id: &str) -> Result<[u8; 32], Problem> {
    let root = store
        .scope_key("channel-proof-key")
        .map_err(|e| state::internal(&e.to_string()))?;
    Ok(hmac_sha256(&root, channel_id.as_bytes()))
}

/// The VERIFIER reference the store keeps for a proof key.
pub fn key_id(key: &[u8; 32]) -> String {
    sha256_hex(key)[..32].to_owned()
}

/// The canonical proof preimage — identical on both sides.
#[allow(clippy::too_many_arguments)]
fn preimage(
    audience: &str,
    channel_id: &str,
    scope_ref: &str,
    operation: &str,
    binding_ref: &str,
    fence_epoch: u64,
    peer: Peer,
    nonce: &str,
    issued_at: i64,
) -> Result<Vec<u8>, Problem> {
    tagged_canonical(
        PROOF_TAG,
        &json!({
            "audience": audience,
            "channel_id": channel_id,
            "scope_ref": scope_ref,
            "operation": operation,
            "binding_ref": binding_ref,
            "fence_epoch": fence_epoch,
            "peer_pid": peer.pid,
            "peer_process_start": peer.process_start,
            "nonce": nonce,
            "issued_at": issued_at,
        }),
    )
    .map_err(|e| state::internal(&e.to_string()))
}

/// The credential file a client reads: `bpk1.<hex JSON>` — one
/// visible-ASCII line, `0600`, that never opens a JSON object (byomd
/// frames the participant preamble by exactly that rule). It carries the
/// PUBLIC binding a proof must commit to plus the client's proof key, so
/// the client can mint a proof without a second round trip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credential {
    pub channel_id: String,
    pub audience: String,
    pub scope_ref: String,
    pub binding_ref: String,
    pub fence_epoch: u64,
    pub key: [u8; 32],
}

pub fn credential_line(
    channel_id: &str,
    audience: &str,
    scope_ref: &str,
    binding_ref: &str,
    fence_epoch: u64,
    key: &[u8; 32],
) -> String {
    let body = json!({
        "channel_id": channel_id,
        "audience": audience,
        "scope_ref": scope_ref,
        "binding_ref": binding_ref,
        "fence_epoch": fence_epoch,
        "key": hex(key),
    })
    .to_string();
    format!("bpk1.{}", hex(body.as_bytes()))
}

/// Parses a credential file line.
pub fn parse_credential(line: &str) -> Option<Credential> {
    let body = line.trim().strip_prefix("bpk1.")?;
    if body.len() % 2 != 0 {
        return None;
    }
    let bytes: Option<Vec<u8>> = (0..body.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(body.get(i..i + 2)?, 16).ok())
        .collect();
    let value: serde_json::Value = serde_json::from_slice(&bytes?).ok()?;
    Some(Credential {
        channel_id: value["channel_id"].as_str()?.to_owned(),
        audience: value["audience"].as_str()?.to_owned(),
        scope_ref: value["scope_ref"].as_str()?.to_owned(),
        binding_ref: value["binding_ref"].as_str()?.to_owned(),
        fence_epoch: value["fence_epoch"].as_u64()?,
        key: unhex32(value["key"].as_str()?)?,
    })
}

/// Mints one proof for the exact call (the CLIENT side; exported so the
/// CLI, the MCP bridge and the tests all speak one construction).
pub fn mint_proof(credential_line: &str, operation: &str, peer: Peer, now: i64) -> Option<String> {
    let credential = parse_credential(credential_line)?;
    let nonce = nonce();
    let bytes = preimage(
        &credential.audience,
        &credential.channel_id,
        &credential.scope_ref,
        operation,
        &credential.binding_ref,
        credential.fence_epoch,
        peer,
        &nonce,
        now,
    )
    .ok()?;
    Some(format!(
        "{PROOF_PREFIX}{}.{nonce}.{now}.{}",
        credential.channel_id,
        hex(&hmac_sha256(&credential.key, &bytes))
    ))
}

/// A resolved channel plus the credential row it verified against.
pub struct Verified {
    pub channel: ChannelRow,
    pub credential: serde_json::Map<String, serde_json::Value>,
}

/// Verifies a presented proof against the stored VERIFIER and the
/// observed connection. Every refusal is non-enumerating `forbidden`.
pub fn verify(
    store: &Store,
    audience: &str,
    operation: &str,
    presented: &str,
    peer: Peer,
    now: i64,
) -> Result<Verified, Problem> {
    let rest = presented
        .trim()
        .strip_prefix(PROOF_PREFIX)
        .ok_or_else(state::forbidden)?;
    let mut parts = rest.split('.');
    let (Some(channel_id), Some(nonce), Some(issued_at), Some(mac), None) = (
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
    ) else {
        return Err(state::forbidden());
    };
    let issued_at: i64 = issued_at.parse().map_err(|_| state::forbidden())?;
    if nonce.len() < 16 || nonce.len() > 64 || !nonce.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(state::forbidden());
    }
    let credential = rows::get_row(
        store.conn(),
        "channel_credentials",
        "channel_id",
        channel_id,
    )
    .map_err(|e| state::internal(&e.to_string()))?
    .ok_or_else(state::forbidden)?;
    if rows::str_of(&credential, "audience") != audience {
        return Err(state::forbidden());
    }
    // Expiry and freshness: a proof is short-lived and the credential
    // itself expires with its offer.
    if now - issued_at > PROOF_MAX_AGE_SECS || issued_at - now > PROOF_MAX_AGE_SECS {
        return Err(state::forbidden());
    }
    let expires_at = rows::str_of(&credential, "expires_at");
    if bpp_core::time::parse_rfc3339_utc(expires_at).is_some_and(|t| t <= now) {
        return Err(state::forbidden());
    }
    // Operation binding: the credential authorizes an exact operation
    // set, and the proof commits to the operation being called.
    let operations: Vec<String> =
        serde_json::from_str(rows::str_of(&credential, "operations")).unwrap_or_default();
    if !operations.iter().any(|o| o == operation) {
        return Err(state::forbidden());
    }
    let key = proof_key(store, channel_id)?;
    if key_id(&key) != rows::str_of(&credential, "proof_key_id") {
        return Err(state::forbidden());
    }
    let bytes = preimage(
        audience,
        channel_id,
        rows::str_of(&credential, "scope_ref"),
        operation,
        rows::str_of(&credential, "binding_ref"),
        rows::u64_of(&credential, "fence_epoch"),
        peer,
        nonce,
        issued_at,
    )?;
    if hex(&hmac_sha256(&key, &bytes)) != mac {
        return Err(state::forbidden());
    }
    // One nonce, one use.
    let spent = store
        .conn()
        .execute(
            "INSERT OR IGNORE INTO channel_proof_nonces (channel_id, nonce, seen_at)
             VALUES (?1, ?2, ?3)",
            rusqlite::params![channel_id, nonce, now],
        )
        .map_err(|e| state::internal(&e.to_string()))?;
    if spent == 0 {
        return Err(state::forbidden());
    }
    let channel = match audience {
        AUDIENCE_CANDIDATE => rows::candidate_channel_by_id(store.conn(), channel_id),
        _ => rows::participant_channel_by_id(store.conn(), channel_id),
    }
    .map_err(|e| state::internal(&e.to_string()))?
    .ok_or_else(state::forbidden)?;
    Ok(Verified {
        channel,
        credential,
    })
}

fn unhex32(s: &str) -> Option<[u8; 32]> {
    if s.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = u8::from_str_radix(s.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(out)
}

/// A fresh proof nonce (hex over OS entropy; time-derived only if
/// `/dev/urandom` is unavailable).
pub fn nonce() -> String {
    let mut bytes = [0u8; 16];
    let read = std::fs::File::open("/dev/urandom")
        .and_then(|mut f| std::io::Read::read_exact(&mut f, &mut bytes));
    if read.is_err() {
        bytes.copy_from_slice(&(bpp_core::time::unix_now() as u128).to_be_bytes());
    }
    hex(&bytes)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn a_proof_is_bound_to_its_peer_and_operation() {
        let key = [3u8; 32];
        let line = credential_line("chan-1", AUDIENCE_CANDIDATE, "offer-1", "manif-1", 1, &key);
        let credential = parse_credential(&line).unwrap();
        assert_eq!(credential.channel_id, "chan-1");
        assert_eq!(credential.key, key);
        assert!(!line.starts_with('{'), "never frames as a request");
        let peer = Peer {
            pid: 4242,
            process_start: 99,
        };
        let nonce = "ab".repeat(8);
        let mac = |peer: Peer, operation: &str| {
            hex(&hmac_sha256(
                &key,
                &preimage(
                    AUDIENCE_CANDIDATE,
                    "chan-1",
                    "offer-1",
                    operation,
                    "manif-1",
                    1,
                    peer,
                    &nonce,
                    1_700_000_000,
                )
                .unwrap(),
            ))
        };
        let mine = mac(peer, "membership_accept");
        // Another same-UID process replaying the SAME line is a
        // different peer: the daemon recomputes a different MAC.
        let other_peer = Peer {
            pid: 4243,
            process_start: 99,
        };
        assert_ne!(mine, mac(other_peer, "membership_accept"));
        // A different operation is a different proof.
        assert_ne!(mine, mac(peer, "membership_refuse"));
    }

    #[test]
    fn the_current_peer_has_a_start_time() {
        let peer = Peer::current();
        assert_eq!(peer.pid, std::process::id() as i32);
        assert!(peer.process_start > 0, "/proc start time is readable");
    }
}
