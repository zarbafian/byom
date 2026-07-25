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
//! generation)` and dedups by transaction id, so a re-sent request for an
//! already-journaled transaction returns the existing entry, never a
//! second one; a query by transaction id either finds the exact entry or
//! proves its absence.
//!
//! What you write:
//! ```
//! use byom_store::witness::{Witness, CasOutcome, WitnessFault};
//! let dir = std::env::temp_dir().join(format!("wit-{}", std::process::id()));
//! std::fs::create_dir_all(&dir).unwrap();
//! let witness = Witness::open(&dir.join("authority-witness.jsonl")).unwrap();
//! let out = witness.cas("inc-1", 0, "txn-1", &"d".repeat(64),
//!                       WitnessFault::None).unwrap();
//! assert!(matches!(out, CasOutcome::Witnessed(e) if e.generation == 1));
//! assert!(witness.query("txn-1").unwrap().is_some());
//! ```

use std::io::Write as _;
use std::path::{Path, PathBuf};

use bpp_core::canonical::{jcs, sha256_hex};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The advertised witness profile label (honest, §15.3).
pub const WITNESS_PROFILE: &str = "developer-recovery";

/// One witnessed AuthorityJournalEntry (§15.3 projection: digests folded
/// into generation numbers plus transaction identity).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalEntry {
    pub generation: u64,
    pub transaction_id: String,
    pub transition_digest: String,
    pub endpoint_incarnation: String,
    pub prior_entry_digest: String,
    pub entry_digest: String,
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
/// writer (a competing endpoint in tests) is always observed.
pub struct Witness {
    path: PathBuf,
}

fn entry_digest(entry: &Value) -> Result<String, WitnessError> {
    let bytes = jcs(entry).map_err(|e| WitnessError::Corrupt(e.to_string()))?;
    Ok(sha256_hex(&bytes))
}

impl Witness {
    pub fn open(path: &Path) -> Result<Witness, WitnessError> {
        if !path.exists() {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)?;
        }
        Ok(Witness {
            path: path.to_owned(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Every entry, in generation order; verifies the chain (append-only
    /// continuity and per-entry digests).
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
            let mut value =
                serde_json::to_value(&entry).map_err(|e| WitnessError::Corrupt(e.to_string()))?;
            if let Some(map) = value.as_object_mut() {
                map.remove("entry_digest");
            }
            if entry_digest(&value)? != entry.entry_digest {
                return Err(WitnessError::Corrupt("entry digest mismatch".to_owned()));
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

    /// Query by transaction id (§15.3: a witness timeout is queried,
    /// never guessed).
    pub fn query(&self, transaction_id: &str) -> Result<Option<JournalEntry>, WitnessError> {
        Ok(self
            .entries()?
            .into_iter()
            .find(|e| e.transaction_id == transaction_id))
    }

    /// The §15.3 step-2 compare-and-swap: `(incarnation, prior
    /// generation)` to the exact next entry, deduped by transaction id.
    pub fn cas(
        &self,
        incarnation: &str,
        prior_generation: u64,
        transaction_id: &str,
        transition_digest: &str,
        fault: WitnessFault,
    ) -> Result<CasOutcome, WitnessError> {
        if fault == WitnessFault::LoseRequest {
            // The request never reached the witness.
            return Ok(CasOutcome::Unknown);
        }
        let entries = self.entries()?;
        // Retry/query-safety: an already-journaled transaction returns
        // the existing entry, never a second one.
        if let Some(existing) = entries.iter().find(|e| e.transaction_id == transaction_id) {
            return Ok(CasOutcome::Witnessed(existing.clone()));
        }
        let head = entries.len() as u64;
        if head != prior_generation {
            return Ok(CasOutcome::HeadConflict { head });
        }
        let prior_entry_digest = entries
            .last()
            .map(|e| e.entry_digest.clone())
            .unwrap_or_default();
        let mut value = serde_json::json!({
            "generation": head + 1,
            "transaction_id": transaction_id,
            "transition_digest": transition_digest,
            "endpoint_incarnation": incarnation,
            "prior_entry_digest": prior_entry_digest,
        });
        let digest = entry_digest(&value)?;
        if let Some(map) = value.as_object_mut() {
            map.insert("entry_digest".into(), Value::String(digest.clone()));
        }
        let mut file = std::fs::OpenOptions::new().append(true).open(&self.path)?;
        let mut line =
            serde_json::to_string(&value).map_err(|e| WitnessError::Corrupt(e.to_string()))?;
        line.push('\n');
        file.write_all(line.as_bytes())?;
        file.sync_all()?;
        let entry: JournalEntry =
            serde_json::from_value(value).map_err(|e| WitnessError::Corrupt(e.to_string()))?;
        if fault == WitnessFault::LoseReplyAfterWrite {
            // The entry is durable but the receipt vanished in flight.
            return Ok(CasOutcome::Unknown);
        }
        Ok(CasOutcome::Witnessed(entry))
    }
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
        Witness::open(&path).unwrap()
    }

    #[test]
    fn cas_dedups_by_transaction_id() {
        let w = temp_witness("dedup");
        let a = w
            .cas("inc", 0, "t1", &"a".repeat(64), WitnessFault::None)
            .unwrap();
        let CasOutcome::Witnessed(first) = a else {
            panic!("{a:?}")
        };
        // Re-sent request: the existing entry, never a second one.
        let b = w
            .cas("inc", 1, "t1", &"a".repeat(64), WitnessFault::None)
            .unwrap();
        assert_eq!(b, CasOutcome::Witnessed(first));
        assert_eq!(w.head().unwrap(), 1);
    }

    #[test]
    fn stale_prior_generation_conflicts() {
        let w = temp_witness("conflict");
        w.cas("inc", 0, "t1", &"a".repeat(64), WitnessFault::None)
            .unwrap();
        let out = w
            .cas("inc", 0, "t2", &"b".repeat(64), WitnessFault::None)
            .unwrap();
        assert_eq!(out, CasOutcome::HeadConflict { head: 1 });
    }

    #[test]
    fn lost_reply_leaves_a_durable_entry_found_by_query() {
        let w = temp_witness("lost");
        let out = w
            .cas(
                "inc",
                0,
                "t1",
                &"a".repeat(64),
                WitnessFault::LoseReplyAfterWrite,
            )
            .unwrap();
        assert_eq!(out, CasOutcome::Unknown);
        assert!(w.query("t1").unwrap().is_some(), "queried, never guessed");
        // A lost request writes nothing.
        let out = w
            .cas("inc", 1, "t2", &"b".repeat(64), WitnessFault::LoseRequest)
            .unwrap();
        assert_eq!(out, CasOutcome::Unknown);
        assert!(w.query("t2").unwrap().is_none());
    }
}
