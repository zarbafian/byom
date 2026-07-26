//! Random per-object `local_erasure_safe` secrets (D-R1-2).
//!
//! `local_erasure_safe` means a **random per-object secret wrapped under
//! the Society key, individually destroyable**. A per-object key derived
//! deterministically from one store root is the forbidden scope-key
//! substitution: erasing one object cannot destroy that object's
//! verification, and destroying the root destroys every object.
//!
//! The wrap is a keystream XOR under the Society key
//! (`hmac-sha-256(society wrap key, key_ref)`), so the database never
//! holds a bare object secret and one row can be zeroed without touching
//! any other. What you write:
//!
//! ```no_run
//! # use rusqlite::Connection;
//! # fn f(conn: &Connection, wrap: &[u8; 32]) -> Result<(), Box<dyn std::error::Error>> {
//! let secret = byom_store::object_secrets::mint(
//!     conn, wrap, "society-key:soc-1/object:offer-1", "soc-1", "bpp-membership-offer-v0")?;
//! assert_eq!(secret.len(), 32);
//! # Ok(()) }
//! ```

use bpp_core::canonical::{hex, hmac_sha256};
use rusqlite::{params, Connection, OptionalExtension as _};

use crate::{random_bytes, StoreError};

/// The Society a `society-key:<society>/…` key ref belongs to.
pub fn society_of(key_ref: &str) -> String {
    key_ref
        .strip_prefix("society-key:")
        .and_then(|rest| rest.split('/').next())
        .unwrap_or("genesis")
        .to_owned()
}

fn keystream(wrap_key: &[u8; 32], key_ref: &str) -> [u8; 32] {
    hmac_sha256(wrap_key, key_ref.as_bytes())
}

fn wrap(wrap_key: &[u8; 32], key_ref: &str, secret: &[u8; 32]) -> String {
    let stream = keystream(wrap_key, key_ref);
    let mut out = [0u8; 32];
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = secret[i] ^ stream[i];
    }
    hex(&out)
}

fn unwrap_secret(wrap_key: &[u8; 32], key_ref: &str, wrapped_hex: &str) -> Option<[u8; 32]> {
    if wrapped_hex.len() != 64 {
        return None;
    }
    let stream = keystream(wrap_key, key_ref);
    let mut out = [0u8; 32];
    for (i, slot) in out.iter_mut().enumerate() {
        let byte = u8::from_str_radix(wrapped_hex.get(i * 2..i * 2 + 2)?, 16).ok()?;
        *slot = byte ^ stream[i];
    }
    Some(out)
}

/// Mints a fresh random secret for `key_ref` and retains it wrapped.
/// Minting twice for one key ref keeps the FIRST secret (an object's
/// digest never silently changes underneath its record).
pub fn mint(
    conn: &Connection,
    wrap_key: &[u8; 32],
    key_ref: &str,
    society_id: &str,
    tag: &str,
) -> Result<[u8; 32], StoreError> {
    ensure(conn, wrap_key, key_ref, society_id, tag)
}

/// Loads the retained secret, minting one when this key ref is new.
pub fn ensure(
    conn: &Connection,
    wrap_key: &[u8; 32],
    key_ref: &str,
    society_id: &str,
    tag: &str,
) -> Result<[u8; 32], StoreError> {
    if let Some(secret) = load(conn, wrap_key, key_ref)? {
        return Ok(secret);
    }
    let existing: Option<String> = conn
        .query_row(
            "SELECT state FROM object_secrets WHERE key_ref = ?1",
            [key_ref],
            |r| r.get(0),
        )
        .optional()?;
    if existing.as_deref() == Some("destroyed") {
        return Err(StoreError::Corrupt(format!(
            "object secret {key_ref} was destroyed and cannot be re-minted"
        )));
    }
    let secret = random_bytes::<32>()?;
    conn.execute(
        "INSERT INTO object_secrets
            (key_ref, society_id, tag, wrapped, state, created_at, destroyed_at)
         VALUES (?1, ?2, ?3, ?4, 'live', ?5, NULL)",
        params![
            key_ref,
            society_id,
            tag,
            wrap(wrap_key, key_ref, &secret),
            bpp_core::time::rfc3339_utc(bpp_core::time::unix_now()),
        ],
    )?;
    Ok(secret)
}

/// The retained secret, or `None` when absent or destroyed.
pub fn load(
    conn: &Connection,
    wrap_key: &[u8; 32],
    key_ref: &str,
) -> Result<Option<[u8; 32]>, StoreError> {
    let row: Option<(String, String)> = conn
        .query_row(
            "SELECT wrapped, state FROM object_secrets WHERE key_ref = ?1",
            [key_ref],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    match row {
        Some((wrapped, state)) if state == "live" => Ok(unwrap_secret(wrap_key, key_ref, &wrapped)),
        _ => Ok(None),
    }
}
