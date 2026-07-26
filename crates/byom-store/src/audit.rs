//! Body-free hash-chained audit records (§15.4 developer subset): each
//! record chains over `(seq, ts, event, detail, prev_hash)` so tampering
//! or truncation is detectable locally. The kovee audit pattern.

use rusqlite::{params, Connection, OptionalExtension as _};
use sha2::{Digest as _, Sha256};

#[derive(Debug, thiserror::Error)]
pub enum AuditError {
    #[error(transparent)]
    Db(#[from] rusqlite::Error),
    #[error("audit chain broken at seq {0}")]
    ChainBroken(i64),
}

fn record_hash(seq: i64, ts: i64, event: &str, detail: &str, prev: &[u8]) -> Vec<u8> {
    let mut h = Sha256::new();
    h.update(seq.to_be_bytes());
    h.update(ts.to_be_bytes());
    h.update((event.len() as u64).to_be_bytes());
    h.update(event.as_bytes());
    h.update((detail.len() as u64).to_be_bytes());
    h.update(detail.as_bytes());
    h.update(prev);
    h.finalize().to_vec()
}

/// The §15.4 audit chain table.
pub const AUDIT: &str = "audit";
/// The erasure journal: the append-only record of per-object secret
/// destruction (D-R1-2), chained exactly like the audit ledger and
/// checkpointed beside the witness (BY-J3).
pub const ERASURE: &str = "erasure_journal";

/// Appends one audit record inside the caller's open transaction.
pub fn append(conn: &Connection, ts: i64, event: &str, detail: &str) -> Result<(), AuditError> {
    append_to(conn, AUDIT, ts, event, detail)
}

/// Appends one record to a named hash-chained ledger.
pub fn append_to(
    conn: &Connection,
    table: &str,
    ts: i64,
    event: &str,
    detail: &str,
) -> Result<(), AuditError> {
    debug_assert!(table == AUDIT || table == ERASURE);
    let prev: Option<(i64, Vec<u8>)> = conn
        .query_row(
            &format!("SELECT seq, hash FROM {table} ORDER BY seq DESC LIMIT 1"),
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let (prev_seq, prev_hash) = prev.unwrap_or((0, Vec::new()));
    let seq = prev_seq + 1;
    let hash = record_hash(seq, ts, event, detail, &prev_hash);
    conn.execute(
        &format!(
            "INSERT INTO {table} (seq, ts, event, detail, prev_hash, hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
        ),
        params![seq, ts, event, detail, prev_hash, hash],
    )?;
    Ok(())
}

/// Verifies the whole audit chain; returns the record count.
pub fn verify_chain(conn: &Connection) -> Result<u64, AuditError> {
    Ok(head_of(conn, AUDIT)?.0)
}

/// Verifies a named chain completely and returns `(count, head hash)` —
/// the value the terminal checkpoint pins.
pub fn head_of(conn: &Connection, table: &str) -> Result<(u64, Vec<u8>), AuditError> {
    debug_assert!(table == AUDIT || table == ERASURE);
    let mut stmt = conn.prepare(&format!(
        "SELECT seq, ts, event, detail, prev_hash, hash FROM {table} ORDER BY seq"
    ))?;
    let mut rows = stmt.query([])?;
    let mut prev: Vec<u8> = Vec::new();
    let mut count = 0u64;
    while let Some(row) = rows.next()? {
        let seq: i64 = row.get(0)?;
        let ts: i64 = row.get(1)?;
        let event: String = row.get(2)?;
        let detail: String = row.get(3)?;
        let prev_hash: Vec<u8> = row.get(4)?;
        let hash: Vec<u8> = row.get(5)?;
        if seq != count as i64 + 1
            || prev_hash != prev
            || hash != record_hash(seq, ts, &event, &detail, &prev)
        {
            return Err(AuditError::ChainBroken(seq));
        }
        prev = hash;
        count += 1;
    }
    Ok((count, prev))
}

/// The hash of a named chain at an exact sequence (the checkpoint
/// comparison: a DIFFERENT hash at the checkpointed sequence is an
/// interior alteration, not a legitimate extension).
pub fn hash_at(conn: &Connection, table: &str, seq: u64) -> Result<Option<Vec<u8>>, AuditError> {
    debug_assert!(table == AUDIT || table == ERASURE);
    Ok(conn
        .query_row(
            &format!("SELECT hash FROM {table} WHERE seq = ?1"),
            params![seq as i64],
            |r| r.get(0),
        )
        .optional()?)
}
