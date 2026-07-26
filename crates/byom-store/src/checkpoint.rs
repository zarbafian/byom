//! Terminal audit/erasure checkpoints (BY-J3): the witness records not
//! only the authority journal but the HEADS of the two local
//! hash-chained ledgers — the §15.4 audit chain and the erasure journal.
//!
//! The checkpoints live beside the journal in their own append-only,
//! chained, witness-signed file, so they survive a database rollback the
//! same way journal entries do. At startup the store verifies BOTH
//! complete chains and compares them with the last checkpoint before any
//! non-diagnostic surface opens: a chain that is missing, shorter than
//! the checkpoint (rollback), conflicting at the checkpointed sequence
//! (alteration), or unreadable seals the endpoint.
//!
//! What you write:
//! ```
//! use byom_store::witness::Witness;
//! use byom_store::checkpoint::{Checkpoints, ChainHead};
//! let dir = std::env::temp_dir().join(format!("cp-{}", std::process::id()));
//! std::fs::create_dir_all(&dir).unwrap();
//! let w = Witness::open(&dir.join("w.jsonl")).unwrap();
//! let cps = Checkpoints::open(&dir.join("cp.jsonl")).unwrap();
//! cps.append(&w, 1, ChainHead { seq: 3, hash_hex: "aa".into() },
//!                    ChainHead { seq: 0, hash_hex: String::new() }).unwrap();
//! assert_eq!(cps.latest(&w).unwrap().unwrap().audit.seq, 3);
//! ```

use std::io::Write as _;
use std::path::{Path, PathBuf};

use bpp_core::canonical::{jcs, sha256_hex};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::flock::{open_lock_file, FileLock};
use crate::witness::{Witness, WitnessError};

/// One hash-chained ledger head: how many records and the head hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChainHead {
    pub seq: u64,
    pub hash_hex: String,
}

/// One terminal checkpoint record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkpoint {
    pub sequence: u64,
    pub journal_generation: u64,
    pub audit: ChainHead,
    pub erasure: ChainHead,
    pub prior_checkpoint_digest: String,
    pub checkpoint_digest: String,
    pub witness_key_id: String,
    pub signature: String,
}

pub struct Checkpoints {
    path: PathBuf,
}

fn digest_of(value: &Value) -> Result<String, WitnessError> {
    let bytes = jcs(value).map_err(|e| WitnessError::Corrupt(e.to_string()))?;
    Ok(sha256_hex(&bytes))
}

fn unsigned(record: &Checkpoint) -> Result<Value, WitnessError> {
    let mut value =
        serde_json::to_value(record).map_err(|e| WitnessError::Corrupt(e.to_string()))?;
    if let Some(map) = value.as_object_mut() {
        map.remove("checkpoint_digest");
        map.remove("signature");
    }
    Ok(value)
}

impl Checkpoints {
    pub fn open(path: &Path) -> Result<Checkpoints, WitnessError> {
        if !path.exists() {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)?;
        }
        Ok(Checkpoints {
            path: path.to_owned(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Every checkpoint, verifying the chain, digests, key id and
    /// signature. An unreadable or broken file is `Corrupt` — the
    /// startup comparison seals on it.
    pub fn records(&self, witness: &Witness) -> Result<Vec<Checkpoint>, WitnessError> {
        let text = std::fs::read_to_string(&self.path)?;
        let mut out: Vec<Checkpoint> = Vec::new();
        let mut prior = String::new();
        for (i, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let record: Checkpoint = serde_json::from_str(line)
                .map_err(|e| WitnessError::Corrupt(format!("checkpoint line {}: {e}", i + 1)))?;
            if record.sequence != out.len() as u64 + 1 {
                return Err(WitnessError::Corrupt(format!(
                    "checkpoint sequence {} at position {}",
                    record.sequence,
                    out.len() + 1
                )));
            }
            if record.prior_checkpoint_digest != prior {
                return Err(WitnessError::Corrupt("broken checkpoint chain".to_owned()));
            }
            if digest_of(&unsigned(&record)?)? != record.checkpoint_digest {
                return Err(WitnessError::Corrupt(
                    "checkpoint digest mismatch".to_owned(),
                ));
            }
            if record.witness_key_id != witness.key_id()
                || record.signature != witness.sign(&record.checkpoint_digest)
            {
                return Err(WitnessError::Corrupt(format!(
                    "checkpoint {} does not verify under the witness key",
                    record.sequence
                )));
            }
            prior = record.checkpoint_digest.clone();
            out.push(record);
        }
        Ok(out)
    }

    /// The last checkpoint, if any.
    pub fn latest(&self, witness: &Witness) -> Result<Option<Checkpoint>, WitnessError> {
        Ok(self.records(witness)?.pop())
    }

    /// Appends one checkpoint under an exclusive lock (inter-process
    /// safe, like the journal itself).
    pub fn append(
        &self,
        witness: &Witness,
        journal_generation: u64,
        audit: ChainHead,
        erasure: ChainHead,
    ) -> Result<Checkpoint, WitnessError> {
        let guard = FileLock::exclusive(open_lock_file(&self.path)?)?;
        let existing = self.records(witness)?;
        let prior = existing
            .last()
            .map(|c| c.checkpoint_digest.clone())
            .unwrap_or_default();
        let mut value = serde_json::json!({
            "sequence": existing.len() as u64 + 1,
            "journal_generation": journal_generation,
            "audit": audit,
            "erasure": erasure,
            "prior_checkpoint_digest": prior,
            "witness_key_id": witness.key_id(),
        });
        let digest = digest_of(&value)?;
        if let Some(map) = value.as_object_mut() {
            map.insert("checkpoint_digest".into(), Value::String(digest.clone()));
            map.insert("signature".into(), Value::String(witness.sign(&digest)));
        }
        let mut file = std::fs::OpenOptions::new().append(true).open(&self.path)?;
        let mut line =
            serde_json::to_string(&value).map_err(|e| WitnessError::Corrupt(e.to_string()))?;
        line.push('\n');
        file.write_all(line.as_bytes())?;
        file.sync_all()?;
        drop(guard);
        serde_json::from_value(value).map_err(|e| WitnessError::Corrupt(e.to_string()))
    }
}
