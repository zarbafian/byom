//! The developer-recovery authority witness (DESIGN.md §15.3; B1 sheet):
//! a local append-only monotonic journal file standing in for the
//! external non-rollbackable facility. It is HONESTLY LABELED as the
//! `developer-recovery` profile (§15.3: "a deployment without an
//! independent monotonic journal may advertise developer recovery only,
//! never production rollback resistance") — the B-ADR-3 production
//! backend is not required at B1.
//!
//! Semantics mirror `proof/specs/AuthorityJournal.tla`: the file is
//! append-only and survives database rollback (it lives beside, not
//! inside, the SQLite file); the CAS compares `(incarnation, prior
//! generation, prior entry digest)` and dedups by transaction id, so a
//! re-sent request for an already-journaled transaction returns the
//! existing entry, never a second one; a query by transaction id either
//! finds the exact entry or proves its absence.
//!
//! The compare/append/fsync is INTER-PROCESS atomic: the whole
//! read-check-append-fsync-verify runs under an exclusive `flock` on the
//! journal file, and the winner re-reads the file afterwards to confirm
//! its own entry landed at the exact proposed generation. Two processes
//! sharing one data directory therefore cannot both witness generation
//! N+1 (a process-local mutex could not say that). Each entry is signed
//! under a witness key held beside the journal, never in the database,
//! so a database rollback cannot forge one.
//!
//! What you write:
//! ```
//! use byom_store::witness::{Witness, CasOutcome, WitnessFault};
//! let dir = std::env::temp_dir().join(format!("wit-{}", std::process::id()));
//! std::fs::create_dir_all(&dir).unwrap();
//! let witness = Witness::open(&dir.join("authority-witness.jsonl")).unwrap();
//! let out = witness.cas("inc-1", 0, "", "txn-1", &"d".repeat(64), &"r".repeat(64),
//!                       WitnessFault::None).unwrap();
//! assert!(matches!(out, CasOutcome::Witnessed(e) if e.generation == 1));
//! assert!(witness.query("txn-1").unwrap().is_some());
//! ```

use std::io::{Read as _, Write as _};
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};

use bpp_core::canonical::{hex, hmac_sha256, jcs, sha256_hex};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::flock::{open_lock_file, FileLock};

/// The advertised witness profile label (honest, §15.3).
pub const WITNESS_PROFILE: &str = "developer-recovery";

/// Test-only deterministic contention window: milliseconds to hold the
/// journal lock between the read and the append, so a two-process
/// competing-CAS test is a proof rather than a race. Unset in production.
const CAS_DELAY_ENV: &str = "BYOM_WITNESS_CAS_DELAY_MS";

/// One witnessed AuthorityJournalEntry (§15.3): the exact receipt the
/// finalize step verifies member by member.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalEntry {
    pub generation: u64,
    pub transaction_id: String,
    pub transition_digest: String,
    /// The digest of the exact reply bytes this transition will return
    /// (§15.3: the result is witnessed, not re-derived at finalize).
    pub result_digest: String,
    pub endpoint_incarnation: String,
    pub prior_entry_digest: String,
    pub entry_digest: String,
    /// The witness key this entry is signed under.
    pub witness_key_id: String,
    /// `hmac-sha-256(witness key, entry_digest)`.
    pub signature: String,
}

/// Injected witness faults for the b1_journal matrix. Test-only; the
/// production path passes `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WitnessFault {
    None,
    /// The entry is written but the reply is lost in flight
    /// (`WitnessLostAfterWrite` in the model).
    LoseReplyAfterWrite,
    /// The request never reaches the witness (`WitnessLostNoWrite`).
    LoseRequest,
}

/// The CAS answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CasOutcome {
    Witnessed(JournalEntry),
    /// Reply lost — the caller must query by transaction id, never guess.
    Unknown,
    /// A competing CAS advanced the head: complete dependency
    /// revalidation under a new proposed generation is required.
    HeadConflict {
        head: u64,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum WitnessError {
    #[error("witness io: {0}")]
    Io(#[from] std::io::Error),
    #[error("witness journal corrupt: {0}")]
    Corrupt(String),
}

/// The witness file handle. All reads re-scan the file so an external
/// writer (a competing endpoint) is always observed.
pub struct Witness {
    path: PathBuf,
    key: [u8; 32],
    key_id: String,
}

fn entry_digest(entry: &Value) -> Result<String, WitnessError> {
    let bytes = jcs(entry).map_err(|e| WitnessError::Corrupt(e.to_string()))?;
    Ok(sha256_hex(&bytes))
}

fn key_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".key");
    path.with_file_name(name)
}

fn random_key() -> Result<[u8; 32], WitnessError> {
    let mut out = [0u8; 32];
    std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut out))?;
    Ok(out)
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

impl Witness {
    pub fn open(path: &Path) -> Result<Witness, WitnessError> {
        if !path.exists() {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)?;
        }
        // The signing key lives beside the journal, never in the
        // database: a database rollback cannot mint a forged entry.
        let kp = key_path(path);
        let key = match std::fs::read_to_string(&kp) {
            Ok(text) => unhex32(text.trim()).ok_or_else(|| {
                WitnessError::Corrupt("witness key file is not 32 hex bytes".into())
            })?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let key = random_key()?;
                let mut f = std::fs::OpenOptions::new()
                    .create(true)
                    .write(true)
                    .truncate(true)
                    .open(&kp)?;
                f.write_all(hex(&key).as_bytes())?;
                f.sync_all()?;
                let _ = std::fs::set_permissions(&kp, std::fs::Permissions::from_mode(0o600));
                key
            }
            Err(e) => return Err(WitnessError::Io(e)),
        };
        let key_id = sha256_hex(&key)[..16].to_owned();
        Ok(Witness {
            path: path.to_owned(),
            key,
            key_id,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    /// Signs one digest under the witness key (the journal entries and
    /// the terminal audit/erasure checkpoints beside them).
    pub fn sign(&self, entry_digest_hex: &str) -> String {
        hex(&hmac_sha256(&self.key, entry_digest_hex.as_bytes()))
    }

    /// Every entry, in generation order; verifies the chain (append-only
    /// continuity, per-entry digests, witness key id and signature).
    pub fn entries(&self) -> Result<Vec<JournalEntry>, WitnessError> {
        let text = std::fs::read_to_string(&self.path)?;
        let mut out = Vec::new();
        let mut prior_digest = String::new();
        for (i, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let entry: JournalEntry = serde_json::from_str(line)
                .map_err(|e| WitnessError::Corrupt(format!("line {}: {e}", i + 1)))?;
            if entry.generation != out.len() as u64 + 1 {
                return Err(WitnessError::Corrupt(format!(
                    "generation {} at position {}",
                    entry.generation,
                    out.len() + 1
                )));
            }
            if entry.prior_entry_digest != prior_digest {
                return Err(WitnessError::Corrupt("broken digest chain".to_owned()));
            }
            if entry_digest(&unsigned_value(&entry)?)? != entry.entry_digest {
                return Err(WitnessError::Corrupt("entry digest mismatch".to_owned()));
            }
            if entry.witness_key_id != self.key_id {
                return Err(WitnessError::Corrupt(format!(
                    "entry {} is signed under witness key {}, not {}",
                    entry.generation, entry.witness_key_id, self.key_id
                )));
            }
            if entry.signature != self.sign(&entry.entry_digest) {
                return Err(WitnessError::Corrupt(format!(
                    "entry {} signature does not verify",
                    entry.generation
                )));
            }
            prior_digest = entry.entry_digest.clone();
            out.push(entry);
        }
        Ok(out)
    }

    /// The current head generation (0 when empty).
    pub fn head(&self) -> Result<u64, WitnessError> {
        Ok(self.entries()?.len() as u64)
    }

    /// The current head entry digest (empty when the journal is empty) —
    /// the prior-digest half of the CAS.
    pub fn head_digest(&self) -> Result<String, WitnessError> {
        Ok(self
            .entries()?
            .last()
            .map(|e| e.entry_digest.clone())
            .unwrap_or_default())
    }

    /// Query by transaction id (§15.3: a witness timeout is queried,
    /// never guessed).
    pub fn query(&self, transaction_id: &str) -> Result<Option<JournalEntry>, WitnessError> {
        Ok(self
            .entries()?
            .into_iter()
            .find(|e| e.transaction_id == transaction_id))
    }

    /// The §15.3 step-2 compare-and-swap over `(incarnation, prior
    /// generation, prior entry digest)`, deduped by transaction id.
    ///
    /// The whole read-check-append-fsync-verify holds an exclusive
    /// `flock` on the journal file, so two PROCESSES serialize here;
    /// after the append the winner re-reads the file and confirms its
    /// own entry sits at the exact proposed generation (a loser is
    /// detected, never assumed).
    #[allow(clippy::too_many_arguments)]
    pub fn cas(
        &self,
        incarnation: &str,
        prior_generation: u64,
        prior_entry_digest: &str,
        transaction_id: &str,
        transition_digest: &str,
        result_digest: &str,
        fault: WitnessFault,
    ) -> Result<CasOutcome, WitnessError> {
        if fault == WitnessFault::LoseRequest {
            // The request never reached the witness.
            return Ok(CasOutcome::Unknown);
        }
        // ---- inter-process critical section ----
        let guard = FileLock::exclusive(open_lock_file(&self.path)?)?;
        let entries = self.entries()?;
        // Retry/query-safety: an already-journaled transaction returns
        // the existing entry, never a second one.
        if let Some(existing) = entries.iter().find(|e| e.transaction_id == transaction_id) {
            return Ok(CasOutcome::Witnessed(existing.clone()));
        }
        let head = entries.len() as u64;
        let head_digest = entries
            .last()
            .map(|e| e.entry_digest.clone())
            .unwrap_or_default();
        if head != prior_generation || head_digest != prior_entry_digest {
            return Ok(CasOutcome::HeadConflict { head });
        }
        let mut value = serde_json::json!({
            "generation": head + 1,
            "transaction_id": transaction_id,
            "transition_digest": transition_digest,
            "result_digest": result_digest,
            "endpoint_incarnation": incarnation,
            "prior_entry_digest": head_digest,
            "witness_key_id": self.key_id,
        });
        let digest = entry_digest(&value)?;
        if let Some(map) = value.as_object_mut() {
            map.insert("entry_digest".into(), Value::String(digest.clone()));
            map.insert("signature".into(), Value::String(self.sign(&digest)));
        }
        if let Ok(ms) = std::env::var(CAS_DELAY_ENV) {
            // Test hook: widen the window between the head read and the
            // append — the exact window an unlocked CAS loses. Held
            // INSIDE the lock, so a competing process is serialized
            // behind it and the competing-CAS test is a proof rather
            // than a timing coincidence.
            if let Ok(ms) = ms.parse::<u64>() {
                std::thread::sleep(std::time::Duration::from_millis(ms));
            }
        }
        {
            let mut file = std::fs::OpenOptions::new().append(true).open(&self.path)?;
            let mut line =
                serde_json::to_string(&value).map_err(|e| WitnessError::Corrupt(e.to_string()))?;
            line.push('\n');
            file.write_all(line.as_bytes())?;
            file.sync_all()?;
        }
        // Verified after write, still under the lock: our entry must be
        // exactly the new head at the proposed generation.
        let after = self.entries()?;
        let landed = match after.iter().find(|e| e.transaction_id == transaction_id) {
            Some(entry) if entry.generation == head + 1 && entry.entry_digest == digest => {
                entry.clone()
            }
            _ => {
                return Err(WitnessError::Corrupt(format!(
                    "witnessed entry for {transaction_id} did not land at generation {}",
                    head + 1
                )))
            }
        };
        drop(guard);
        if fault == WitnessFault::LoseReplyAfterWrite {
            // The entry is durable but the receipt vanished in flight.
            return Ok(CasOutcome::Unknown);
        }
        Ok(CasOutcome::Witnessed(landed))
    }
}

fn unsigned_value(entry: &JournalEntry) -> Result<Value, WitnessError> {
    let mut value =
        serde_json::to_value(entry).map_err(|e| WitnessError::Corrupt(e.to_string()))?;
    if let Some(map) = value.as_object_mut() {
        map.remove("entry_digest");
        map.remove("signature");
    }
    Ok(value)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn temp_witness(name: &str) -> Witness {
        let dir = std::env::temp_dir().join(format!("byom-wit-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("authority-witness.jsonl");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(dir.join("authority-witness.jsonl.key"));
        Witness::open(&path).unwrap()
    }

    fn cas(
        w: &Witness,
        prior: u64,
        prior_digest: &str,
        txn: &str,
        fault: WitnessFault,
    ) -> CasOutcome {
        w.cas(
            "inc",
            prior,
            prior_digest,
            txn,
            &"a".repeat(64),
            &"b".repeat(64),
            fault,
        )
        .unwrap()
    }

    #[test]
    fn cas_dedups_by_transaction_id() {
        let w = temp_witness("dedup");
        let a = cas(&w, 0, "", "t1", WitnessFault::None);
        let CasOutcome::Witnessed(first) = a else {
            panic!("{a:?}")
        };
        // Re-sent request: the existing entry, never a second one.
        let b = cas(&w, 1, &first.entry_digest, "t1", WitnessFault::None);
        assert_eq!(b, CasOutcome::Witnessed(first));
        assert_eq!(w.head().unwrap(), 1);
    }

    #[test]
    fn stale_prior_generation_conflicts() {
        let w = temp_witness("conflict");
        cas(&w, 0, "", "t1", WitnessFault::None);
        let out = cas(&w, 0, "", "t2", WitnessFault::None);
        assert_eq!(out, CasOutcome::HeadConflict { head: 1 });
    }

    #[test]
    fn a_wrong_prior_digest_conflicts_even_at_the_right_generation() {
        let w = temp_witness("priordigest");
        cas(&w, 0, "", "t1", WitnessFault::None);
        let out = cas(&w, 1, &"f".repeat(64), "t2", WitnessFault::None);
        assert_eq!(out, CasOutcome::HeadConflict { head: 1 });
        assert_eq!(w.head().unwrap(), 1, "the loser appended nothing");
    }

    #[test]
    fn lost_reply_leaves_a_durable_entry_found_by_query() {
        let w = temp_witness("lost");
        let out = cas(&w, 0, "", "t1", WitnessFault::LoseReplyAfterWrite);
        assert_eq!(out, CasOutcome::Unknown);
        assert!(w.query("t1").unwrap().is_some(), "queried, never guessed");
        // A lost request writes nothing.
        let head_digest = w.head_digest().unwrap();
        let out = cas(&w, 1, &head_digest, "t2", WitnessFault::LoseRequest);
        assert_eq!(out, CasOutcome::Unknown);
        assert!(w.query("t2").unwrap().is_none());
    }

    #[test]
    fn an_entry_signed_under_another_key_is_corrupt() {
        let w = temp_witness("sig");
        cas(&w, 0, "", "t1", WitnessFault::None);
        // Rotating the key beneath the journal invalidates every entry:
        // a rollback that reinstates an old journal cannot forge one.
        let kp = key_path(w.path());
        std::fs::write(&kp, hex(&[7u8; 32])).unwrap();
        let reopened = Witness::open(w.path()).unwrap();
        assert!(reopened.entries().is_err());
    }
}
