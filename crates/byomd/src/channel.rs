//! Non-exportable, peer-bound channel proofs (BY-C1, DESIGN.md
//! §2342-2358).
//!
//! # What you write (the whole client side)
//!
//! ```no_run
//! # use byomd::channel;
//! # fn f(run_dir: &std::path::Path) -> Result<(), String> {
//! let credential = std::fs::read_to_string(
//!     "…/channels/candidate-offer-1.token").map_err(|e| e.to_string())?;
//! // ONE claim per process: byomd observes this connection's SO_PEERCRED
//! // and hands back a proof key that is useless in any other process.
//! let key = channel::claim(run_dir, &credential)?;
//! // then one fresh proof per call
//! let proof = channel::mint_proof(&credential, &key, "membership_accept",
//!                                 channel::Peer::current(),
//!                                 bpp_core::time::unix_now());
//! # let _ = proof; Ok(()) }
//! ```
//!
//! # Why (the R1 confirmation's finding)
//!
//! The first fix made the wire credential a per-call PROOF instead of a
//! bearer token, but the credential FILE still carried the raw HMAC key.
//! A same-UID process that copied the file simply minted a proof naming
//! its OWN pid, and the daemon — which derives the same key from the
//! channel id alone — verified it. The pid binding was decoration.
//!
//! Now the file carries NO key material at all:
//!
//! ```text
//! bpk1.<hex JSON {channel_id, audience, scope_ref, binding_ref, fence_epoch}>
//! ```
//!
//! The minting secret stays endpoint-side. A client CLAIMS its channel
//! over the surface socket (`bpb1.<channel_id>` as the preamble line,
//! nothing else on the connection) and byomd answers with a proof key
//! derived from the connection's kernel-observed peer:
//!
//! ```text
//! proof key = hmac-sha-256(store "channel-proof-key" scope key,
//!                          "<channel_id>|<peer pid>|<peer start time>")
//! ```
//!
//! A channel is held by exactly ONE LIVE process: a claim from another
//! peer while the holder is alive is refused, and only a dead holder's
//! binding is taken over (the one-shot CLI case). Verification re-derives
//! the key from the peer byomd OBSERVES, so a key that leaked to another
//! process mints nothing there either.
//!
//! The presented proof is still the same MAC over the exact call:
//!
//! ```text
//! bpx1.<channel_id>.<nonce hex>.<issued_at>.<mac hex>
//! mac = hmac-sha-256(proof key, JCS {
//!     $domain: "bpp-channel-proof-v0",
//!     audience, channel_id, scope_ref, operation, binding_ref,
//!     fence_epoch, peer_pid, peer_process_start, nonce, issued_at })
//! ```
//!
//! so it is additionally bound to the exact offer/participant scope, the
//! Manifestation/control binding, the fence epoch, the audience, the
//! exact operation and a short expiry, with each nonce spent once.
//!
//! HONEST RESIDUAL (developer profile, §19). There is no UID separation,
//! no attested process identity and no asymmetric endpoint identity here,
//! so the ceiling is "one live claimant at a time":
//!
//! - a same-UID process that claims a channel BEFORE the legitimate
//!   client does, or AFTER it exits, becomes the holder — the file names
//!   the channel, it does not authenticate who may hold it;
//! - a same-UID process can read the store root key out of the SQLite
//!   file, or `ptrace` the holder, and derive keys directly.
//!
//! What is genuinely closed is the exported-secret class: a copied,
//! backed-up, logged or transmitted credential file mints nothing, and a
//! proof key issued to one process is worthless in another.

use std::io::{BufRead as _, BufReader, Write as _};
use std::os::unix::net::UnixStream;
use std::path::Path;

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
        // B3 slice 2: `episode_request` is the participant entry point of
        // the four-stage activation, so a participant credential
        // authorizes it. The runtime-surface commands are NOT here —
        // a runtime identity never crosses to participant (§14.7).
        .chain(crate::reads::RUNTIME_OPS.iter())
        // B3 slice 3: the §13.1 act chain's participant seats
        // (act_intent_prepare/position/finalize). The runtime one-shot
        // consumptions and the narrow onboarding/attention adapters carry
        // their own workload channels and are filtered out below by their
        // surface, exactly like the runtime episode commands.
        .chain(crate::reads::SLICE3_OPS.iter())
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

/// The wire tag of a channel CLAIM line (the whole claim protocol).
pub const CLAIM_PREFIX: &str = "bpb1.";

/// The PEER-BOUND per-channel proof key: derived endpoint-side from a
/// store scope key AND the kernel-observed peer, so the same channel
/// yields a different key in every process. Nothing derivable from the
/// credential file alone.
pub fn proof_key(store: &Store, channel_id: &str, peer: Peer) -> Result<[u8; 32], Problem> {
    let root = store
        .scope_key("channel-proof-key")
        .map_err(|e| state::internal(&e.to_string()))?;
    Ok(hmac_sha256(
        &root,
        format!("{channel_id}|{}|{}", peer.pid, peer.process_start).as_bytes(),
    ))
}

/// Is this exact process still running? A recycled pid has a different
/// kernel start time, so a stale binding never covers a new process.
fn peer_alive(peer: Peer) -> bool {
    std::path::Path::new(&format!("/proc/{}", peer.pid)).exists()
        && process_start(peer.pid) == peer.process_start
}

/// The server side of a claim: binds the channel to THIS connection's
/// peer and returns its proof key. Refused while a DIFFERENT live
/// process holds the channel; a dead holder's binding is taken over.
pub fn claim_for_peer(
    store: &Store,
    channel_id: &str,
    peer: Peer,
    now: i64,
) -> Result<[u8; 32], Problem> {
    let credential = rows::get_row(
        store.conn(),
        "channel_credentials",
        "channel_id",
        channel_id,
    )
    .map_err(|e| state::internal(&e.to_string()))?
    .ok_or_else(state::forbidden)?;
    if rows::str_of(&credential, "state") != "open" {
        return Err(state::forbidden());
    }
    let held: Option<(i64, i64)> = store
        .conn()
        .query_row(
            "SELECT peer_pid, peer_start FROM channel_bindings WHERE channel_id = ?1",
            [channel_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok();
    if let Some((pid, start)) = held {
        let holder = Peer {
            pid: pid as i32,
            process_start: start.max(0) as u64,
        };
        if holder != peer && peer_alive(holder) {
            // One live holder per channel: a copier of the credential
            // file cannot take the channel from a running client.
            return Err(state::forbidden());
        }
    }
    store
        .conn()
        .execute(
            "INSERT INTO channel_bindings (channel_id, peer_pid, peer_start, bound_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(channel_id) DO UPDATE SET
                 peer_pid = excluded.peer_pid,
                 peer_start = excluded.peer_start,
                 bound_at = excluded.bound_at",
            rusqlite::params![channel_id, peer.pid, peer.process_start as i64, now],
        )
        .map_err(|e| state::internal(&e.to_string()))?;
    proof_key(store, channel_id, peer)
}

/// The client side of a claim: one connection carrying only
/// `bpb1.<channel_id>`, answered with this process's proof key.
/// `socket_dir` is byomd's runtime directory.
pub fn claim(socket_dir: &Path, credential_line: &str) -> Result<[u8; 32], String> {
    let credential =
        parse_credential(credential_line).ok_or("not a byom channel credential file")?;
    let surface = match credential.audience.as_str() {
        AUDIENCE_CANDIDATE => "candidate",
        _ => "participant",
    };
    let path = socket_dir.join(format!("{surface}.sock"));
    let mut stream = UnixStream::connect(&path)
        .map_err(|e| format!("claim {}: {e} (is byomd running?)", path.display()))?;
    stream
        .write_all(format!("{CLAIM_PREFIX}{}\n", credential.channel_id).as_bytes())
        .map_err(|e| format!("claim write: {e}"))?;
    let mut reply = String::new();
    BufReader::new(stream)
        .read_line(&mut reply)
        .map_err(|e| format!("claim read: {e}"))?;
    let parsed: serde_json::Value =
        serde_json::from_str(reply.trim_end()).map_err(|e| format!("claim reply: {e}"))?;
    if parsed["outcome"] != "ok" {
        return Err(format!("channel claim refused: {parsed}"));
    }
    parsed["result"]["proof_key"]
        .as_str()
        .and_then(unhex32)
        .ok_or_else(|| format!("claim reply carries no proof key: {parsed}"))
}

/// The VERIFIER reference the store keeps for a channel: a digest of the
/// channel's key root, NOT of any per-peer proof key (those are minted
/// per claiming process and never stored).
pub fn channel_key_id(store: &Store, channel_id: &str) -> Result<String, Problem> {
    let root = store
        .scope_key("channel-proof-key")
        .map_err(|e| state::internal(&e.to_string()))?;
    Ok(sha256_hex(&hmac_sha256(
        &root,
        format!("verifier|{channel_id}").as_bytes(),
    ))[..32]
        .to_owned())
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
/// frames the participant preamble by exactly that rule). It carries ONLY
/// the PUBLIC binding a proof must commit to: NO key material, so the
/// file's bytes are not a credential and a copy of them mints nothing
/// (BY-C1). The proof key is claimed over the socket, per process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credential {
    pub channel_id: String,
    pub audience: String,
    pub scope_ref: String,
    pub binding_ref: String,
    pub fence_epoch: u64,
}

pub fn credential_line(
    channel_id: &str,
    audience: &str,
    scope_ref: &str,
    binding_ref: &str,
    fence_epoch: u64,
) -> String {
    let body = json!({
        "channel_id": channel_id,
        "audience": audience,
        "scope_ref": scope_ref,
        "binding_ref": binding_ref,
        "fence_epoch": fence_epoch,
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
    })
}

/// Mints one proof for the exact call (the CLIENT side; exported so the
/// CLI, the MCP bridge and the tests all speak one construction). `key`
/// is the peer-bound key this process CLAIMED — the file alone cannot
/// mint.
pub fn mint_proof(
    credential_line: &str,
    key: &[u8; 32],
    operation: &str,
    peer: Peer,
    now: i64,
) -> Option<String> {
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
        hex(&hmac_sha256(key, &bytes))
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
    // The verifying key is re-derived from the peer byomd OBSERVES on
    // THIS connection (BY-C1): a key issued to another process — or a
    // copied credential file, which carries none — cannot produce this
    // MAC. The channel must also still be held by that exact peer.
    let held: Option<(i64, i64)> = store
        .conn()
        .query_row(
            "SELECT peer_pid, peer_start FROM channel_bindings WHERE channel_id = ?1",
            [channel_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok();
    match held {
        Some((pid, start)) if pid == peer.pid as i64 && start == peer.process_start as i64 => {}
        _ => return Err(state::forbidden()),
    }
    if channel_key_id(store, channel_id)? != rows::str_of(&credential, "proof_key_id") {
        return Err(state::forbidden());
    }
    let key = proof_key(store, channel_id, peer)?;
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
        let line = credential_line("chan-1", AUDIENCE_CANDIDATE, "offer-1", "manif-1", 1);
        let credential = parse_credential(&line).unwrap();
        assert_eq!(credential.channel_id, "chan-1");
        assert!(
            !line.contains(&hex(&key))
                && !parse_body(&line).as_object().unwrap().contains_key("key"),
            "the credential FILE carries no key material (BY-C1): {line}"
        );
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

    fn parse_body(line: &str) -> serde_json::Value {
        let body = line.trim().strip_prefix("bpk1.").unwrap();
        let bytes: Vec<u8> = (0..body.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&body[i..i + 2], 16).unwrap())
            .collect();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[test]
    fn the_current_peer_has_a_start_time() {
        let peer = Peer::current();
        assert_eq!(peer.pid, std::process::id() as i32);
        assert!(peer.process_start > 0, "/proc start time is readable");
    }
}
