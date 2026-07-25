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

/// Appends one audit record inside the caller's open transaction.
pub fn append(conn: &Connection, ts: i64, event: &str, detail: &str) -> Result<(), AuditError> {
    let prev: Option<(i64, Vec<u8>)> = conn
        .query_row(
            "SELECT seq, hash FROM audit ORDER BY seq DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let (prev_seq, prev_hash) = prev.unwrap_or((0, Vec::new()));
    let seq = prev_seq + 1;
    let hash = record_hash(seq, ts, event, detail, &prev_hash);
    conn.execute(
        "INSERT INTO audit (seq, ts, event, detail, prev_hash, hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![seq, ts, event, detail, prev_hash, hash],
    )?;
    Ok(())
}

/// Verifies the whole chain; returns the record count.
pub fn verify_chain(conn: &Connection) -> Result<u64, AuditError> {
    let mut stmt =
        conn.prepare("SELECT seq, ts, event, detail, prev_hash, hash FROM audit ORDER BY seq")?;
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
        if prev_hash != prev || hash != record_hash(seq, ts, &event, &detail, &prev) {
            return Err(AuditError::ChainBroken(seq));
        }
        prev = hash;
        count += 1;
    }
    Ok(count)
}
