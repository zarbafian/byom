//! SQLite storage plus the §15.3 authority mutation journal for byomd
//! (B1, personal profile): plain WAL SQLite, numbered migrations, and
//! every authoritative mutation driven through the TLC-checked
//! three-step protocol (proof/specs/AuthorityJournal.tla):
//!
//! 1. `journal_sql_prepare` — one serializable SQL transaction
//!    revalidates dependencies and writes the FULL transition (result,
//!    effects, events) invisibly into `authority_pending`; no reply, no
//!    visible row.
//! 2. `journal_witness_cas` — the developer-recovery witness (an
//!    append-only monotonic file, honestly labeled; B-ADR-3 backend not
//!    required at B1) CASes `(incarnation, prior generation)` to the
//!    exact next entry, deduped by transaction id.
//! 3. `journal_sql_finalize` — a second SQL transaction verifies the
//!    exact entry, materializes the pending set (dense per-Society event
//!    sequences, idempotency record, outbox), advances the local mirror,
//!    and only then may byomd reply.
//!
//! On startup the store compares mirror and witness: a witnessed entry
//! the database no longer knows is a journal/database mismatch and the
//! endpoint starts `sealed_diagnostic` — every non-diagnostic surface
//! refuses (§15.3).
//!
//! What one mutation looks like (the daemon's side):
//! ```
//! use byom_store::{Store, MutationScope, Prepared, CursorMint, CrashHooks};
//! use bpp_core::envelope::MutationMeta;
//! let dir = std::env::temp_dir().join(format!("byom-doc-{}", std::process::id()));
//! std::fs::create_dir_all(&dir).unwrap();
//! let mut store = Store::open(&dir).unwrap();
//! let scope = MutationScope {
//!     society_id: "genesis".into(), operation: "society_prepare".into(),
//!     actor: "governance:sovereign".into(),
//!     meta: MutationMeta { request_id: "r-1".into(), idempotency_key: "k-1".into(),
//!         expected_endpoint_incarnation: store.incarnation().unwrap(),
//!         expected_recovery_epoch: 0, expected_revision: None,
//!         causation_event_ref: None, correlation_ref: None },
//!     body: serde_json::json!({"op": "society_prepare"}),
//! };
//! let bytes = store.authority_mutation(&scope, 0, CrashHooks::NONE, |_conn, _scope| {
//!     Ok(Prepared { result: serde_json::json!({"ok": true}), revision: Some(1),
//!                   cursor: CursorMint::None, effects: vec![], events: vec![] })
//! }).unwrap();
//! // An exact replay returns byte-identical bytes without re-executing.
//! let replay = store.authority_mutation(&scope, 0, CrashHooks::NONE,
//!     |_, _| unreachable!("a replay never re-executes")).unwrap();
//! assert_eq!(bytes, replay);
//! ```

pub mod audit;
pub mod checkpoint;
pub mod effects;
pub mod flock;
pub mod object_secrets;
pub mod privacy;
pub mod rows;
pub mod schema;
pub mod witness;

use std::io::Read as _;
use std::path::{Path, PathBuf};

use bpp_core::canonical::{hex, hmac_sha256, jcs, sha256_hex, tagged_canonical};
use bpp_core::digest::DigestRef;
use bpp_core::envelope::{MutationMeta, Success};
use bpp_core::idempotency::IdempotencyDomain;
use bpp_core::problem::{Problem, ProblemKind};
use bpp_core::time::rfc3339_utc;
use checkpoint::{ChainHead, Checkpoints};
use effects::{Effect, NewEvent};
use flock::{open_lock_file, FileLock};
use rusqlite::{params, Connection, OptionalExtension as _};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use witness::{CasOutcome, Witness, WitnessFault};

const META_ENDPOINT_INCARNATION: &str = "endpoint_incarnation";
const META_INDEX_ROOT_KEY: &str = "index_root_key";
const META_CURSOR_SECRET: &str = "cursor_secret";
const META_MIRROR_GEN: &str = "journal_mirror_gen";
const META_MIRROR_DIGEST: &str = "journal_mirror_digest";
const META_ENDPOINT_STATUS: &str = "endpoint_status";
const META_SEAL_REASON: &str = "seal_reason";

/// The society scope of pre-genesis mutations (`society_prepare`): the
/// IdempotencyDomain requires a society id before any Society exists, so
/// the genesis scope is pinned to this sentinel at epoch 0.
pub const GENESIS_SCOPE: &str = "genesis";

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error(transparent)]
    Db(#[from] rusqlite::Error),
    #[error(transparent)]
    Schema(#[from] schema::SchemaError),
    #[error(transparent)]
    Witness(#[from] witness::WitnessError),
    #[error(transparent)]
    Audit(#[from] audit::AuditError),
    #[error(transparent)]
    Effect(#[from] effects::EffectError),
    #[error("serialization: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("canonicalization: {0}")]
    Canonical(#[from] bpp_core::canonical::CanonicalError),
    #[error("entropy: {0}")]
    Entropy(std::io::Error),
    #[error("corrupt store state: {0}")]
    Corrupt(String),
    /// Another process already owns this data directory (BY-J1): a
    /// second daemon on one data directory could witness a competing
    /// generation, so exclusive ownership is refused, never shared.
    #[error("data directory {0} is already owned by another byom endpoint")]
    DataDirLocked(String),
}

/// A mutation either fails with a §14.9 problem (nothing committed) or a
/// store fault.
#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("problem: {0:?}")]
    Problem(Problem),
    #[error(transparent)]
    Store(#[from] StoreError),
}

impl From<rusqlite::Error> for CommandError {
    fn from(e: rusqlite::Error) -> CommandError {
        CommandError::Store(StoreError::Db(e))
    }
}

/// Crash-honesty hooks (the b1_journal / crash-matrix points): abort the
/// process at a named §15.3 boundary, or inject a witness fault.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CrashHooks {
    /// After the prepare transaction commits, before the witness CAS.
    pub abort_before_witness: bool,
    /// After the witness CAS succeeded, before SQL finalize.
    pub abort_after_witness: bool,
    /// Inside the finalize transaction, before its commit.
    pub abort_before_finalize: bool,
    /// After finalize committed, before the reply is written.
    pub abort_after_finalize: bool,
    pub witness_fault: WitnessFault,
}

impl CrashHooks {
    pub const NONE: CrashHooks = CrashHooks {
        abort_before_witness: false,
        abort_after_witness: false,
        abort_before_finalize: false,
        abort_after_finalize: false,
        witness_fault: WitnessFault::None,
    };
}

/// The channel-derived scope of one mutation (§14.2): the authenticated
/// actor, the operation, the Society, and the client MutationMeta.
#[derive(Debug, Clone)]
pub struct MutationScope {
    pub society_id: String,
    pub operation: String,
    /// The channel-derived actor binding string (never caller-selected).
    pub actor: String,
    pub meta: MutationMeta,
    /// The accepted request body (the idempotency request digest covers
    /// it minus the volatile meta members).
    pub body: Value,
}

/// Where the success `source_cursor` points after finalize.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CursorMint {
    None,
    /// From the beginning of the Society's ledger (genesis replay).
    FromStart {
        society_id: String,
    },
    /// After the events this mutation appended.
    AfterEvents {
        society_id: String,
    },
}

/// What one prepared mutation hands to the journal driver: the §15.3
/// "full transition", written invisibly at prepare and materialized
/// exactly at finalize.
pub struct Prepared {
    pub result: Value,
    pub revision: Option<u64>,
    pub cursor: CursorMint,
    pub effects: Vec<Effect>,
    pub events: Vec<NewEvent>,
}

/// One event, fully materialized at PREPARE time (BY-J2): sequence,
/// timestamp, payload secret and payload digest are all fixed before the
/// witness sees the transition, so finalize (live or at recovery) writes
/// byte-identical rows.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct FinalEvent {
    event_id: String,
    society_id: String,
    sequence: u64,
    kind: String,
    object_ref: String,
    object_revision: u64,
    participant_ref: Option<String>,
    actor_ref: String,
    causation_ref: String,
    correlation_ref: String,
    payload: Value,
    payload_digest: Value,
    visibility_scope_ref: String,
    occurred_at: String,
}

/// One outbox delivery, also fixed at prepare time.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct FinalOutbox {
    delivery_id: String,
    kind: String,
    payload: String,
    created_at: String,
}

/// The §15.3 "full transition": every final state, event, outbox and
/// result byte, hashed into `transition_digest`/`result_digest` BEFORE
/// witnessing.
#[derive(Debug, Serialize, Deserialize)]
struct PendingPayload {
    society_id: String,
    operation: String,
    request_digest: String,
    result: Value,
    revision: Option<u64>,
    source_cursor: Option<String>,
    effects: Vec<Effect>,
    events: Vec<FinalEvent>,
    outbox: Vec<FinalOutbox>,
    /// The final `next_event_sequence` per Society this transition sets.
    society_sequence_heads: Vec<(String, u64)>,
    occurred_at: String,
}

impl PendingPayload {
    /// The exact reply bytes this transition returns — recomputed
    /// identically by the live path and by crash recovery.
    fn result_bytes(&self) -> Result<Vec<u8>, StoreError> {
        let success = Success {
            outcome: "ok".to_owned(),
            result: self.result.clone(),
            revision: self.revision,
            source_cursor: self.source_cursor.clone(),
        };
        Ok(serde_json::to_vec(&success)?)
    }
}

/// The verified §15.3 receipt persisted with the finalized transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalReceipt {
    pub transaction_id: String,
    pub endpoint_incarnation: String,
    pub prior_generation: u64,
    pub prior_entry_digest: String,
    pub generation: u64,
    pub transition_digest: String,
    pub result_digest: String,
    pub entry_digest: String,
    pub witness_key_id: String,
    pub signature: String,
}

/// One personal-profile authoritative database plus its witness. Opening
/// takes EXCLUSIVE ownership of the data directory for the process's
/// life (BY-J1).
pub struct Store {
    conn: Connection,
    witness: Witness,
    checkpoints: Checkpoints,
    data_dir: PathBuf,
    /// Held for the store's whole life; released by the kernel on exit.
    _dir_lock: FileLock,
}

fn internal(detail: &str) -> Problem {
    Problem::new(ProblemKind::Internal, "internal fault")
        .with_status(500)
        .with_detail(detail.to_owned())
}

impl Store {
    /// Opens (creating if absent) the store under `data_dir`, bootstraps
    /// endpoint identity, and runs the §15.3 startup comparison and
    /// crash recovery. A mismatch does NOT fail the open: the endpoint
    /// comes up `sealed_diagnostic` and every authority surface refuses.
    pub fn open(data_dir: &Path) -> Result<Store, StoreError> {
        std::fs::create_dir_all(data_dir).map_err(StoreError::Entropy)?;
        // §15.3 exclusive data-directory ownership (BY-J1): the witness
        // CAS is only meaningful if exactly one endpoint writes this
        // directory. A second daemon is refused here, before it can
        // observe a generation.
        let lock_file = open_lock_file(&data_dir.join("byom.lock")).map_err(StoreError::Entropy)?;
        let dir_lock = FileLock::try_exclusive(lock_file)
            .map_err(|_| StoreError::DataDirLocked(data_dir.display().to_string()))?;
        let mut conn = Connection::open(data_dir.join("byom.db"))?;
        // Prepare reads (heads, idempotency) then writes; IMMEDIATE takes
        // the write lock at BEGIN so check-and-act is a genuine CAS.
        conn.set_transaction_behavior(rusqlite::TransactionBehavior::Immediate);
        let journal_mode = schema::open_and_migrate(&conn)?;
        if journal_mode != "wal" && journal_mode != "memory" {
            return Err(StoreError::Corrupt(format!(
                "journal_mode is {journal_mode:?}, expected wal"
            )));
        }
        let witness = Witness::open(&data_dir.join("authority-witness.jsonl"))?;
        let checkpoints = Checkpoints::open(&data_dir.join("authority-checkpoints.jsonl"))?;
        let store = Store {
            conn,
            witness,
            checkpoints,
            data_dir: data_dir.to_owned(),
            _dir_lock: dir_lock,
        };
        store.bootstrap_meta()?;
        store.startup_recover()?;
        Ok(store)
    }

    fn bootstrap_meta(&self) -> Result<(), StoreError> {
        if schema::meta_get(&self.conn, META_ENDPOINT_INCARNATION)?.is_some() {
            return Ok(());
        }
        let incarnation = format!("inc-{}", hex(&random_bytes::<8>()?));
        let tx = self.conn.unchecked_transaction()?;
        schema::meta_set(&tx, META_ENDPOINT_INCARNATION, incarnation.as_bytes())?;
        schema::meta_set(&tx, META_INDEX_ROOT_KEY, &random_bytes::<32>()?)?;
        schema::meta_set(&tx, META_CURSOR_SECRET, &random_bytes::<32>()?)?;
        schema::meta_set(&tx, META_MIRROR_GEN, b"0")?;
        schema::meta_set(&tx, META_MIRROR_DIGEST, b"")?;
        schema::meta_set(&tx, META_ENDPOINT_STATUS, b"active")?;
        audit::append(
            &tx,
            bpp_core::time::unix_now(),
            "endpoint.bootstrapped",
            &format!(
                "incarnation {incarnation}; witness profile {}",
                witness::WITNESS_PROFILE
            ),
        )?;
        // The first terminal checkpoint: from now on a missing one is a
        // seal condition, never a fresh store.
        self.write_checkpoint()?;
        tx.commit()?;
        Ok(())
    }

    /// Records the audit/erasure chain heads THIS OPEN TRANSACTION will
    /// leave, as a terminal checkpoint beside the witness (BY-J3).
    ///
    /// Called INSIDE every audit-appending transaction, immediately
    /// before its commit. That ordering is what makes the ledgers
    /// completely covered: a committed database is always EXACTLY at
    /// some checkpoint, so there is no "tail beyond the checkpoint" for
    /// an adversary to rewrite — a re-chained append is detected because
    /// the chain is longer than the checkpoint pins, not merely because
    /// its interior hashes broke. The one window it opens (checkpoint
    /// appended, commit lost) is the single PROVISIONAL checkpoint the
    /// startup comparison is allowed to skip.
    fn write_checkpoint(&self) -> Result<(), StoreError> {
        let (audit_seq, audit_hash) = audit::head_of(&self.conn, audit::AUDIT)?;
        let (erasure_seq, erasure_hash) = audit::head_of(&self.conn, audit::ERASURE)?;
        self.checkpoints.append(
            &self.witness,
            self.mirror_gen()?,
            ChainHead {
                seq: audit_seq,
                hash_hex: hex(&audit_hash),
            },
            ChainHead {
                seq: erasure_seq,
                hash_hex: hex(&erasure_hash),
            },
        )?;
        Ok(())
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn incarnation(&self) -> Result<String, StoreError> {
        schema::meta_get_text(&self.conn, META_ENDPOINT_INCARNATION)?
            .ok_or_else(|| StoreError::Corrupt("store is not bootstrapped".to_owned()))
    }

    /// Is the endpoint sealed (§15.3 startup comparison)?
    pub fn sealed(&self) -> bool {
        matches!(
            schema::meta_get_text(&self.conn, META_ENDPOINT_STATUS),
            Ok(Some(status)) if status == "sealed_diagnostic"
        )
    }

    pub fn seal_reason(&self) -> Option<String> {
        schema::meta_get_text(&self.conn, META_SEAL_REASON)
            .ok()
            .flatten()
    }

    fn seal(&self, reason: &str) -> Result<(), StoreError> {
        let tx = self.conn.unchecked_transaction()?;
        schema::meta_set(&tx, META_ENDPOINT_STATUS, b"sealed_diagnostic")?;
        schema::meta_set(&tx, META_SEAL_REASON, reason.as_bytes())?;
        audit::append(
            &tx,
            bpp_core::time::unix_now(),
            "endpoint.sealed_diagnostic",
            reason,
        )?;
        // The seal record is checkpointed like every other append, so a
        // sealed endpoint's ledger stays completely covered. Best effort:
        // the reason for sealing is often that the ledger no longer
        // verifies, and a failure to checkpoint must never stop the seal.
        let _ = self.write_checkpoint();
        tx.commit()?;
        Ok(())
    }

    fn mirror_gen(&self) -> Result<u64, StoreError> {
        Ok(schema::meta_get_text(&self.conn, META_MIRROR_GEN)?
            .and_then(|s| s.parse().ok())
            .unwrap_or(0))
    }

    /// The local mirror's head ENTRY DIGEST — the prior-digest half of
    /// the §15.3 CAS (BY-J1/BY-J2): a generation number alone does not
    /// identify a journal head.
    fn mirror_digest(&self) -> Result<String, StoreError> {
        Ok(schema::meta_get_text(&self.conn, META_MIRROR_DIGEST)?.unwrap_or_default())
    }

    pub fn verify_audit(&self) -> Result<u64, StoreError> {
        Ok(audit::verify_chain(&self.conn)?)
    }

    /// The local journal mirror generation (diagnostic surface, §15.3).
    pub fn journal_mirror_generation(&self) -> Result<u64, StoreError> {
        self.mirror_gen()
    }

    /// The witness head generation (diagnostic surface, §15.3).
    pub fn witness_head(&self) -> Result<u64, StoreError> {
        Ok(self.witness.head()?)
    }

    // ------------------------------------------------------- key material ----

    fn index_root_key(&self) -> Result<Vec<u8>, StoreError> {
        schema::meta_get(&self.conn, META_INDEX_ROOT_KEY)?
            .ok_or_else(|| StoreError::Corrupt("store is not bootstrapped".to_owned()))
    }

    /// A derived per-scope key under the store root (PROFILE §5/§7 scope
    /// keys: idempotency indexes, privacy chains).
    pub fn scope_key(&self, label: &str) -> Result<[u8; 32], StoreError> {
        let root = self.index_root_key()?;
        Ok(hmac_sha256(&root, label.as_bytes()))
    }

    /// The per-Society idempotency-index key — a SCOPE key (PROFILE §5):
    /// destroying it erases offline verifiability of the entire index.
    pub fn society_index_key(&self, society_id: &str) -> Result<[u8; 32], StoreError> {
        self.scope_key(&format!("idempotency-index:{society_id}"))
    }

    /// The per-Society key that WRAPS every per-object secret
    /// (D-R1-2): the object secrets are random, and what the database
    /// stores is each secret wrapped under this Society key.
    pub fn society_wrap_key(&self, society_id: &str) -> Result<[u8; 32], StoreError> {
        self.scope_key(&format!("object-secret-wrap:{society_id}"))
    }

    /// The channel-derived actor binding digest (§14.2): a typed
    /// `local_erasure_safe` commitment over the authenticated actor
    /// string under a RANDOM per-object secret (D-R1-2), never accepted
    /// from a request body. The secret is minted once and retained, so
    /// the idempotency domain stays stable while remaining individually
    /// destroyable.
    pub fn actor_binding_digest(
        &self,
        society_id: &str,
        actor: &str,
    ) -> Result<DigestRef, StoreError> {
        let key_ref = format!(
            "society-key:{society_id}/actor:{}",
            &sha256_hex(actor.as_bytes())[..16]
        );
        let secret = object_secrets::ensure(
            &self.conn,
            &self.society_wrap_key(society_id)?,
            &key_ref,
            society_id,
            "bpp-actor-binding-v0",
        )?;
        let preimage = tagged_canonical(
            "bpp-actor-binding-v0",
            &serde_json::json!({ "actor": actor }),
        )?;
        Ok(DigestRef::local_erasure_safe(
            &key_ref,
            hex(&hmac_sha256(&secret, &preimage)),
        ))
    }

    /// Mints a `local_erasure_safe` digest over an object under a FRESH
    /// RANDOM per-object secret (D-R1-2), retained wrapped under the
    /// Society key so the object stays verifiable until ITS secret is
    /// destroyed.
    ///
    /// The secret is NOT returned (BY-D2): the only retained copy is the
    /// wrapped `object_secrets` row, so `destroy_object_secret` really
    /// destroys it. Handing the raw bytes back is how they ended up
    /// copied into `societies.preparation` and pending payloads, where
    /// destruction never reached them. Re-derive through
    /// `verify_object_digest` instead.
    pub fn mint_object_digest(
        &self,
        key_ref: &str,
        tag: &str,
        object: &Value,
    ) -> Result<DigestRef, StoreError> {
        let society_id = object_secrets::society_of(key_ref);
        let secret = object_secrets::mint(
            &self.conn,
            &self.society_wrap_key(&society_id)?,
            key_ref,
            &society_id,
            tag,
        )?;
        let preimage = tagged_canonical(tag, object)?;
        let mac = hmac_sha256(&secret, &preimage);
        Ok(DigestRef::local_erasure_safe(key_ref, hex(&mac)))
    }

    /// A `local_erasure_safe` record digest under a RANDOM per-object
    /// secret (D-R1-2). The old root-derived per-object derivation was
    /// the forbidden scope-key substitution: erasing one object could
    /// not destroy that object's verification, and destroying the root
    /// destroyed every object. Now each record carries its own secret,
    /// individually destroyable through `destroy_object_secret`.
    pub fn record_digest(
        &self,
        society_id: &str,
        object_id: &str,
        tag: &str,
        object: &Value,
    ) -> Result<DigestRef, StoreError> {
        let key_ref = format!("society-key:{society_id}/object:{object_id}");
        let secret = object_secrets::mint(
            &self.conn,
            &self.society_wrap_key(society_id)?,
            &key_ref,
            society_id,
            tag,
        )?;
        let preimage = tagged_canonical(tag, object)?;
        Ok(DigestRef::local_erasure_safe(
            &key_ref,
            hex(&hmac_sha256(&secret, &preimage)),
        ))
    }

    /// Destroys exactly ONE object's secret: that object stops being
    /// verifiable, every other object keeps its own secret. The
    /// destruction is appended to the erasure journal, whose head the
    /// terminal checkpoint pins.
    ///
    /// BY-D2: destruction is COMPLETE. Zeroing the wrapped row while a
    /// raw copy of the same bytes sits in another column erases nothing
    /// — the confirmation found the secret still readable from
    /// `societies.preparation`, pending payloads and `events`. So the
    /// secret is unwrapped once, every TEXT/BLOB column of every table is
    /// swept for those bytes, and the database file and its WAL are
    /// rewritten so no free page keeps them.
    pub fn destroy_object_secret(&self, key_ref: &str, now: i64) -> Result<bool, StoreError> {
        let society_id = object_secrets::society_of(key_ref);
        let secret =
            object_secrets::load(&self.conn, &self.society_wrap_key(&society_id)?, key_ref)?;
        let tx = self.conn.unchecked_transaction()?;
        let changed = tx.execute(
            "UPDATE object_secrets SET wrapped = '', state = 'destroyed', destroyed_at = ?2
             WHERE key_ref = ?1 AND state = 'live'",
            params![key_ref, rfc3339_utc(now)],
        )?;
        if changed == 0 {
            tx.rollback()?;
            return Ok(false);
        }
        let swept = match &secret {
            Some(secret) => scrub_everywhere(&tx, &hex(secret))?,
            None => 0,
        };
        audit::append_to(&tx, audit::ERASURE, now, "object-secret.destroyed", key_ref)?;
        audit::append(
            &tx,
            now,
            "erasure.object_secret_destroyed",
            &format!("{key_ref}; raw copies scrubbed: {swept}"),
        )?;
        self.write_checkpoint()?;
        tx.commit()?;
        // The rows are gone; now the FILE must not keep them. A
        // checkpoint folds the WAL into the database and truncates it, a
        // VACUUM rewrites the database without its free pages, and a
        // second checkpoint truncates the WAL the VACUUM itself wrote.
        self.conn.execute_batch(
            "PRAGMA wal_checkpoint(TRUNCATE); VACUUM; PRAGMA wal_checkpoint(TRUNCATE);",
        )?;
        Ok(true)
    }

    /// Re-derives a `local_erasure_safe` digest from the retained
    /// per-object secret. `None` once that object's secret is destroyed
    /// — the point of the class.
    pub fn verify_object_digest(
        &self,
        key_ref: &str,
        tag: &str,
        object: &Value,
    ) -> Result<Option<String>, StoreError> {
        let society_id = object_secrets::society_of(key_ref);
        let Some(secret) =
            object_secrets::load(&self.conn, &self.society_wrap_key(&society_id)?, key_ref)?
        else {
            return Ok(None);
        };
        let preimage = tagged_canonical(tag, object)?;
        Ok(Some(hex(&hmac_sha256(&secret, &preimage))))
    }

    /// The Society recovery epoch (0 for the genesis scope).
    pub fn recovery_epoch(&self, society_id: &str) -> Result<u64, StoreError> {
        Ok(rows::get_society(&self.conn, society_id)?
            .map(|s| s.recovery_epoch)
            .unwrap_or(0))
    }

    // ------------------------------------------------- idempotency scope ----

    /// The ratified idempotency-domain digest of one mutation scope
    /// (PROFILE §5): recomputed server-side for every mutation.
    pub fn domain_digest(&self, scope: &MutationScope) -> Result<DigestRef, StoreError> {
        let domain = IdempotencyDomain {
            actor_binding_digest: self.actor_binding_digest(&scope.society_id, &scope.actor)?,
            operation: scope.operation.clone(),
            endpoint_incarnation: self.incarnation()?,
            society_id: scope.society_id.clone(),
            society_recovery_epoch: self.recovery_epoch(&scope.society_id)?,
            idempotency_key: scope.meta.idempotency_key.clone(),
        };
        let key = self.society_index_key(&scope.society_id)?;
        let key_ref = format!("society-key:{}/idempotency-index", scope.society_id);
        domain
            .digest(&key, &key_ref)
            .map_err(|e| StoreError::Corrupt(e.to_string()))
    }

    /// The request digest covered by the idempotency record: the body
    /// minus the volatile meta members (`request_id`,
    /// `causation_event_ref`, `correlation_ref`) and minus the DERIVED
    /// CAS token (`expected_revision`) — reusing a key with a changed
    /// covered value is `idempotency_mismatch`.
    ///
    /// MCP-1: `expected_revision` is a precondition, not an argument.
    /// A client that recomputes it from observable state — every MCP
    /// bridge does — necessarily reads a DIFFERENT value after the first
    /// attempt committed, so covering it turned the retry of an
    /// ambiguous (committed but unanswered) call into
    /// `idempotency_mismatch`: the one case the retained receipt exists
    /// for. The CAS itself is still enforced, per operation, against the
    /// live revision; it just no longer changes what the logical call IS.
    pub fn request_digest(body: &Value) -> Result<String, StoreError> {
        let mut projected = body.clone();
        if let Some(meta) = projected.get_mut("meta").and_then(Value::as_object_mut) {
            meta.remove("request_id");
            meta.remove("causation_event_ref");
            meta.remove("correlation_ref");
            meta.remove("expected_revision");
        }
        let bytes = match &projected {
            Value::Object(_) => tagged_canonical("bpp-request-idempotency-v0", &projected)?,
            other => jcs(other)?,
        };
        Ok(sha256_hex(&bytes))
    }

    /// Looks up a stored idempotency record by domain digest.
    pub fn lookup_idempotency(
        &self,
        domain_digest_hex: &str,
    ) -> Result<Option<(String, Vec<u8>)>, StoreError> {
        Ok(self
            .conn
            .query_row(
                "SELECT request_digest, result FROM idempotency_records
                 WHERE domain_digest = ?1",
                [domain_digest_hex],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?)
    }

    // ------------------------------------------------- the §15.3 driver ----

    /// Drives one authoritative mutation through prepare → witness CAS →
    /// finalize. The apply closure revalidates dependencies against the
    /// open prepare transaction and returns the full transition; it may
    /// run again if a competing CAS advanced the head (complete
    /// dependency revalidation under a new proposed generation).
    pub fn authority_mutation(
        &mut self,
        scope: &MutationScope,
        now: i64,
        hooks: CrashHooks,
        apply: impl Fn(&Connection, &MutationScope) -> Result<Prepared, Problem>,
    ) -> Result<Vec<u8>, CommandError> {
        if self.sealed() {
            return Err(CommandError::Problem(
                Problem::new(ProblemKind::EndpointSealed, "endpoint is sealed_diagnostic")
                    .with_status(503),
            ));
        }
        let incarnation = self.incarnation()?;
        let domain_digest = self.domain_digest(scope)?;
        let request_digest = Store::request_digest(&scope.body)?;

        // Idempotency: an exact replay returns the retained result
        // without re-executing; a changed covered value is refused.
        if let Some((stored_digest, stored_result)) =
            self.lookup_idempotency(&domain_digest.value_hex)?
        {
            if stored_digest == request_digest {
                return Ok(stored_result);
            }
            return Err(CommandError::Problem(
                Problem::new(
                    ProblemKind::IdempotencyMismatch,
                    "same idempotency domain, different canonical request",
                )
                .with_status(409)
                .with_detail("reusing an idempotency key with changed arguments is refused"),
            ));
        }

        let mut attempts = 0;
        loop {
            attempts += 1;
            if attempts > 4 {
                return Err(CommandError::Problem(internal(
                    "witness head kept moving during revalidation",
                )));
            }

            // Step 1 — journal_sql_prepare: the full transition,
            // invisible AND fully materialized. Every final state,
            // event, outbox and result byte is fixed here, so the
            // witness commits to exactly what finalize (live or at
            // recovery) will write (BY-J2).
            let observed_gen = self.mirror_gen()?;
            let observed_digest = self.mirror_digest()?;
            let txn_id = format!("txn-{}", hex(&random_bytes::<12>()?));
            let tx = self.conn.unchecked_transaction()?;
            let prepared = match apply(&tx, scope) {
                Ok(p) => p,
                Err(problem) => {
                    tx.rollback()?;
                    return Err(CommandError::Problem(problem));
                }
            };
            let payload = match self.materialize(&tx, scope, &request_digest, prepared, now) {
                Ok(payload) => payload,
                Err(e) => {
                    tx.rollback()?;
                    return Err(CommandError::Store(e));
                }
            };
            let payload_json = serde_json::to_value(&payload).map_err(StoreError::from)?;
            let transition_digest = sha256_hex(&jcs(&payload_json).map_err(StoreError::from)?);
            let result_digest = sha256_hex(&payload.result_bytes()?);
            tx.execute(
                "INSERT INTO authority_pending
                    (transaction_id, endpoint_incarnation, society_id, operation,
                     actor_binding_digest, idempotency_domain_digest,
                     prior_journal_generation, proposed_generation, transition_digest,
                     state, payload, created_at, prior_journal_digest, result_digest)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'prepared',?10,?11,?12,?13)",
                params![
                    txn_id,
                    incarnation,
                    scope.society_id,
                    scope.operation,
                    serde_json::to_string(
                        &self.actor_binding_digest(&scope.society_id, &scope.actor)?
                    )
                    .map_err(StoreError::from)?,
                    domain_digest.value_hex,
                    observed_gen as i64,
                    (observed_gen + 1) as i64,
                    transition_digest,
                    payload_json.to_string(),
                    rfc3339_utc(now),
                    observed_digest,
                    result_digest,
                ],
            )?;
            tx.commit()?;

            if hooks.abort_before_witness {
                // Crash before the witness CAS: inert pending state,
                // recovered (abandoned after proof) at next startup.
                std::process::abort();
            }

            // Step 2 — journal_witness_cas over (incarnation, prior
            // generation, prior entry digest), inter-process atomic.
            let entry = match self.witness.cas(
                &incarnation,
                observed_gen,
                &observed_digest,
                &txn_id,
                &transition_digest,
                &result_digest,
                hooks.witness_fault,
            ) {
                Ok(CasOutcome::Witnessed(entry)) => entry,
                Ok(CasOutcome::Unknown) => {
                    // A witness timeout is queried by transaction id and
                    // never guessed (§15.3).
                    match self.witness.query(&txn_id).map_err(StoreError::from)? {
                        Some(entry) => entry,
                        None => {
                            // Proven absent: abandon this pending
                            // transition; the caller may retry freshly.
                            self.abandon_pending(&txn_id, now)?;
                            return Err(CommandError::Problem(
                                Problem::new(
                                    ProblemKind::Unavailable,
                                    "authority witness unavailable; mutation not committed",
                                )
                                .with_status(503),
                            ));
                        }
                    }
                }
                Ok(CasOutcome::HeadConflict { .. }) => {
                    // A competing CAS advanced the head: the old pending
                    // transition stays inert (abandoned after proof) and
                    // the exact transaction is revalidated afresh.
                    self.abandon_pending(&txn_id, now)?;
                    continue;
                }
                Err(e) => {
                    return Err(CommandError::Store(StoreError::Witness(e)));
                }
            };
            // The EXACT receipt is verified member by member before
            // anything this transition wrote becomes visible.
            if let Err(reason) = verify_receipt(
                &entry,
                &txn_id,
                &incarnation,
                observed_gen,
                &observed_digest,
                &transition_digest,
                &result_digest,
                self.witness.key_id(),
            ) {
                self.seal(&reason)?;
                return Err(CommandError::Problem(
                    Problem::new(ProblemKind::EndpointSealed, "endpoint is sealed_diagnostic")
                        .with_status(503),
                ));
            }

            if hooks.abort_after_witness {
                // Crash between witness success and SQL finalize: the
                // exact receipt finalizes once at recovery.
                std::process::abort();
            }

            // Step 3 — journal_sql_finalize.
            let bytes = self.finalize_pending(&txn_id, &entry, now, &hooks)?;

            if hooks.abort_after_finalize {
                // Crash after commit, before the reply: the retry finds
                // the stored byte-identical result.
                std::process::abort();
            }
            return Ok(bytes);
        }
    }

    /// Turns a `Prepared` transition into the FULL §15.3 pending set:
    /// dense per-Society event sequences, the transition timestamp, the
    /// per-event `local_erasure_safe` secret and payload digest, the
    /// outbox rows, the minted source cursor and the exact result bytes
    /// — all fixed BEFORE witnessing so recovery reproduces them
    /// byte-identically (BY-J2).
    fn materialize(
        &self,
        tx: &Connection,
        scope: &MutationScope,
        request_digest: &str,
        prepared: Prepared,
        now: i64,
    ) -> Result<PendingPayload, StoreError> {
        let occurred_at = rfc3339_utc(now);
        // Where each Society's sequence allocation starts: the value the
        // effects will leave on the row, else the current row.
        let mut heads: Vec<(String, u64)> = Vec::new();

        let mut events = Vec::with_capacity(prepared.events.len());
        let mut outbox = Vec::with_capacity(prepared.events.len());
        let mut last_seq: Option<(String, u64)> = None;
        for event in &prepared.events {
            let seq = next_sequence(tx, &mut heads, &event.society_id, &prepared.effects)?;
            if let Some(slot) = heads.iter_mut().find(|(s, _)| *s == event.society_id) {
                slot.1 = seq + 1;
            }
            let key_ref = format!("society-key:{}/event:{}", event.society_id, event.event_id);
            let secret = object_secrets::ensure(
                tx,
                &self.society_wrap_key(&event.society_id)?,
                &key_ref,
                &event.society_id,
                "bpp-event-payload-v0",
            )?;
            let preimage = tagged_canonical("bpp-event-payload-v0", &event.payload)?;
            let digest =
                DigestRef::local_erasure_safe(&key_ref, hex(&hmac_sha256(&secret, &preimage)));
            events.push(FinalEvent {
                event_id: event.event_id.clone(),
                society_id: event.society_id.clone(),
                sequence: seq,
                kind: event.kind.clone(),
                object_ref: event.object_ref.clone(),
                object_revision: event.object_revision,
                participant_ref: event.participant_ref.clone(),
                actor_ref: event.actor_ref.clone(),
                causation_ref: event.causation_ref.clone(),
                correlation_ref: event.correlation_ref.clone(),
                payload: event.payload.clone(),
                payload_digest: serde_json::to_value(&digest)?,
                visibility_scope_ref: event.visibility_scope_ref.clone(),
                occurred_at: occurred_at.clone(),
            });
            outbox.push(FinalOutbox {
                delivery_id: event.event_id.clone(),
                kind: "event".to_owned(),
                payload: serde_json::json!({
                    "event_id": event.event_id,
                    "society_id": event.society_id,
                    "sequence": seq,
                    "kind": event.kind,
                })
                .to_string(),
                created_at: occurred_at.clone(),
            });
            last_seq = Some((event.society_id.clone(), seq));
        }

        let source_cursor = match &prepared.cursor {
            CursorMint::None => None,
            CursorMint::FromStart { society_id } => Some(self.mint_events_cursor(society_id, 0)?),
            CursorMint::AfterEvents { society_id } => {
                let seq = match &last_seq {
                    Some((sid, seq)) if sid == society_id => *seq,
                    _ => next_sequence(tx, &mut heads, society_id, &prepared.effects)?
                        .saturating_sub(1),
                };
                Some(self.mint_events_cursor(society_id, seq)?)
            }
        };

        Ok(PendingPayload {
            society_id: scope.society_id.clone(),
            operation: scope.operation.clone(),
            request_digest: request_digest.to_owned(),
            result: prepared.result,
            revision: prepared.revision,
            source_cursor,
            effects: prepared.effects,
            events,
            outbox,
            society_sequence_heads: heads,
            occurred_at,
        })
    }

    fn abandon_pending(&self, txn_id: &str, now: i64) -> Result<(), StoreError> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE authority_pending SET state = 'abandoned' WHERE transaction_id = ?1",
            [txn_id],
        )?;
        audit::append(&tx, now, "authority-journal.abandoned", txn_id)?;
        self.write_checkpoint()?;
        tx.commit()?;
        Ok(())
    }

    /// The §15.3 step-3 finalize: materializes the exact pending set —
    /// used by both the live path and startup recovery, so a recovered
    /// transaction finalizes identically.
    fn finalize_pending(
        &self,
        txn_id: &str,
        entry: &witness::JournalEntry,
        now: i64,
        hooks: &CrashHooks,
    ) -> Result<Vec<u8>, StoreError> {
        let tx = self.conn.unchecked_transaction()?;
        let (
            payload_text,
            state,
            prior_gen,
            prior_digest,
            stored_transition,
            stored_result_digest,
            incarnation,
        ): (String, String, i64, String, String, String, String) = tx.query_row(
            "SELECT payload, state, prior_journal_generation, prior_journal_digest,
                    transition_digest, result_digest, endpoint_incarnation
             FROM authority_pending WHERE transaction_id = ?1",
            [txn_id],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                ))
            },
        )?;
        if state == "finalized" {
            return Err(StoreError::Corrupt(format!(
                "transaction {txn_id} already finalized"
            )));
        }
        // BY-J2: the transition digest is RECOMPUTED from the stored
        // payload — never read out of the digest column beside it. The
        // witness committed to `sha256(jcs(payload))`, so a payload that
        // no longer hashes to the witnessed value is exactly the state
        // finalize must refuse: comparing a stored column to the witness
        // proves only that two copies of a number agree, and would then
        // apply UNCHECKED effects, events and outbox rows.
        let payload_value: Value = serde_json::from_str(&payload_text)?;
        let recomputed = sha256_hex(&jcs(&payload_value)?);
        if recomputed != entry.transition_digest || recomputed != stored_transition {
            tx.rollback()?;
            let reason = format!(
                "pending payload of {txn_id} hashes to {recomputed}, not the witnessed \
                 transition digest {}",
                entry.transition_digest
            );
            self.seal(&reason)?;
            return Err(StoreError::Corrupt(reason));
        }
        let payload: PendingPayload = serde_json::from_value(payload_value)?;
        // The exact reply bytes, recomputed from the witnessed
        // transition — not re-derived from live state.
        let bytes = payload.result_bytes()?;
        // Finalization verifies the EXACT receipt (§15.3): witness key,
        // signature, incarnation, prior head, generation, transaction,
        // transition and result digests. A naked (transaction id,
        // generation) pair is not a receipt.
        if let Err(reason) = verify_receipt(
            entry,
            txn_id,
            &incarnation,
            prior_gen.max(0) as u64,
            &prior_digest,
            &stored_transition,
            &stored_result_digest,
            self.witness.key_id(),
        ) {
            tx.rollback()?;
            self.seal(&reason)?;
            return Err(StoreError::Corrupt(reason));
        }
        if sha256_hex(&bytes) != stored_result_digest {
            tx.rollback()?;
            let reason = format!("result bytes of {txn_id} do not match the witnessed digest");
            self.seal(&reason)?;
            return Err(StoreError::Corrupt(reason));
        }

        // Materialize state effects.
        for effect in &payload.effects {
            effects::apply(&tx, effect)?;
        }

        // Append the exact witnessed events, outbox rows and sequence
        // heads — all fixed at prepare time.
        for event in &payload.events {
            tx.execute(
                "INSERT INTO events (event_id, society_id, sequence, kind, object_ref,
                     object_revision, participant_ref, actor_ref, causation_ref,
                     correlation_ref, payload, payload_digest, payload_secret,
                     visibility_scope_ref, occurred_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
                params![
                    event.event_id,
                    event.society_id,
                    event.sequence as i64,
                    event.kind,
                    event.object_ref,
                    event.object_revision as i64,
                    event.participant_ref,
                    event.actor_ref,
                    event.causation_ref,
                    event.correlation_ref,
                    event.payload.to_string(),
                    serde_json::to_string(&event.payload_digest)?,
                    // BY-D2: the ONLY retained copy of a per-event
                    // secret is its wrapped `object_secrets` row, so
                    // destroying that row destroys the secret. A raw
                    // copy here made every "destroyed" event secret
                    // still readable.
                    "",
                    event.visibility_scope_ref,
                    event.occurred_at,
                ],
            )?;
        }
        for delivery in &payload.outbox {
            tx.execute(
                "INSERT INTO outbox (delivery_id, kind, payload, created_at)
                 VALUES (?1,?2,?3,?4)",
                params![
                    delivery.delivery_id,
                    delivery.kind,
                    delivery.payload,
                    delivery.created_at
                ],
            )?;
        }
        for (society_id, next_sequence) in &payload.society_sequence_heads {
            tx.execute(
                "UPDATE societies SET next_event_sequence = ?2 WHERE society_id = ?1",
                params![society_id, *next_sequence as i64],
            )?;
        }

        // Retain the exact result bytes for idempotent replay.
        let (domain_digest, society_id, operation): (String, String, String) = tx.query_row(
            "SELECT idempotency_domain_digest, society_id, operation
             FROM authority_pending WHERE transaction_id = ?1",
            [txn_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        tx.execute(
            "INSERT INTO idempotency_records
                (domain_digest, society_id, operation, request_digest, result, created_at)
             VALUES (?1,?2,?3,?4,?5,?6)",
            params![
                domain_digest,
                society_id,
                operation,
                payload.request_digest,
                bytes,
                payload.occurred_at,
            ],
        )?;

        // Mark the exact pending set finalized, persist the verified
        // receipt, and advance the mirror generation AND head digest.
        let receipt = JournalReceipt {
            transaction_id: entry.transaction_id.clone(),
            endpoint_incarnation: entry.endpoint_incarnation.clone(),
            prior_generation: prior_gen.max(0) as u64,
            prior_entry_digest: entry.prior_entry_digest.clone(),
            generation: entry.generation,
            transition_digest: entry.transition_digest.clone(),
            result_digest: entry.result_digest.clone(),
            entry_digest: entry.entry_digest.clone(),
            witness_key_id: entry.witness_key_id.clone(),
            signature: entry.signature.clone(),
        };
        tx.execute(
            "UPDATE authority_pending SET state = 'finalized', receipt = ?2
             WHERE transaction_id = ?1",
            params![txn_id, serde_json::to_string(&receipt)?],
        )?;
        let mirror = self.mirror_gen()?;
        if entry.generation >= mirror {
            schema::meta_set(
                &tx,
                META_MIRROR_GEN,
                entry.generation.to_string().as_bytes(),
            )?;
            schema::meta_set(&tx, META_MIRROR_DIGEST, entry.entry_digest.as_bytes())?;
        }
        audit::append(
            &tx,
            now,
            "authority-journal.finalized",
            &format!(
                "{txn_id} generation {} op {operation} entry {}",
                entry.generation, entry.entry_digest
            ),
        )?;

        // The terminal checkpoint pins the audit and erasure heads this
        // transition leaves behind, written INSIDE the transaction so
        // the committed ledger is always exactly at a checkpoint (BY-J3).
        self.write_checkpoint()?;

        if hooks.abort_before_finalize {
            // Crash inside finalize before commit: witnessed entry with
            // an unfinalized pending row — recovered by the exact
            // receipt at startup, finalized once. The checkpoint this
            // transaction appended is the single PROVISIONAL one the
            // startup comparison may skip.
            std::process::abort();
        }
        tx.commit()?;
        Ok(bytes)
    }

    // -------------------------------------------- startup comparison ----

    /// The §15.3 startup comparison and closed crash-state recovery:
    /// - a witnessed entry the database no longer knows → sealed;
    /// - the mirror ahead of the witness (witness loss/truncation) → sealed;
    /// - an abandoned pending row with a witness entry → sealed;
    /// - in-flight pending rows are the closed crash states: finalized
    ///   via the exact entry when one exists, abandoned after proof of
    ///   absence otherwise.
    fn startup_recover(&self) -> Result<(), StoreError> {
        if self.sealed() {
            return Ok(());
        }
        let entries = match self.witness.entries() {
            Ok(entries) => entries,
            Err(e) => {
                self.seal(&format!("authority witness unreadable: {e}"))?;
                return Ok(());
            }
        };
        let incarnation = self.incarnation()?;
        let mirror = self.mirror_gen()?;
        if mirror > entries.len() as u64 {
            self.seal(&format!(
                "local journal mirror {mirror} ahead of witness head {}",
                entries.len()
            ))?;
            return Ok(());
        }
        // §15.3: the complete audit and erasure chains are verified and
        // bound to a terminal checkpoint BEFORE anything is recovered and
        // before any non-diagnostic surface opens (BY-J3). Verified here,
        // not at the end, so recovery's own appends cannot mask a tail an
        // adversary wrote.
        if !self.verify_chains_against_checkpoints()? {
            return Ok(());
        }
        for entry in &entries {
            if entry.endpoint_incarnation != incarnation {
                self.seal(&format!(
                    "witness entry {} belongs to foreign incarnation {}",
                    entry.generation, entry.endpoint_incarnation
                ))?;
                return Ok(());
            }
            let known: Option<String> = self
                .conn
                .query_row(
                    "SELECT state FROM authority_pending WHERE transaction_id = ?1",
                    [&entry.transaction_id],
                    |r| r.get(0),
                )
                .optional()?;
            match known.as_deref() {
                None => {
                    // The model's Mismatch: extLog[g] known, pend absent.
                    self.seal(&format!(
                        "journal/database mismatch: witnessed transaction {} (generation {}) is unknown to the database",
                        entry.transaction_id, entry.generation
                    ))?;
                    return Ok(());
                }
                Some("abandoned") => {
                    // AbandonedHasNoEntry violated.
                    self.seal(&format!(
                        "abandoned transaction {} has a witness entry",
                        entry.transaction_id
                    ))?;
                    return Ok(());
                }
                Some(_) => {}
            }
        }

        // Recover in-flight pending rows.
        let now = bpp_core::time::unix_now();
        let pending: Vec<(String, String)> = {
            let mut stmt = self.conn.prepare(
                "SELECT transaction_id, transition_digest FROM authority_pending
                 WHERE state IN ('prepared', 'witness_unknown') ORDER BY created_at",
            )?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.collect::<Result<_, _>>()?
        };
        for (txn_id, transition_digest) in pending {
            match entries.iter().find(|e| e.transaction_id == txn_id) {
                Some(entry) => {
                    if entry.transition_digest != transition_digest {
                        self.seal(&format!(
                            "witnessed digest differs from pending transition {txn_id}"
                        ))?;
                        return Ok(());
                    }
                    // Witness success before SQL finalize: recovered by
                    // the exact receipt and finalized once. A receipt
                    // that does not verify seals instead of committing.
                    match self.finalize_pending(&txn_id, entry, now, &CrashHooks::NONE) {
                        Ok(_) => {}
                        Err(StoreError::Corrupt(reason)) => {
                            if !self.sealed() {
                                self.seal(&reason)?;
                            }
                            return Ok(());
                        }
                        Err(e) => return Err(e),
                    }
                    let tx = self.conn.unchecked_transaction()?;
                    audit::append(&tx, now, "authority-journal.recovered", &txn_id)?;
                    self.write_checkpoint()?;
                    tx.commit()?;
                }
                None => {
                    // Proven no entry: abandoned, never guessed.
                    self.abandon_pending(&txn_id, now)?;
                }
            }
        }

        // The local mirror must name the exact witness head entry — the
        // generation AND the entry digest. After recovery every
        // witnessed transition is finalized, so a mirror short of the
        // head is a database that lost committed authority state.
        let head_digest = entries
            .last()
            .map(|e| e.entry_digest.clone())
            .unwrap_or_default();
        let mirror_gen = self.mirror_gen()?;
        if mirror_gen != entries.len() as u64 {
            self.seal(&format!(
                "after recovery the local journal mirror is at generation {mirror_gen}, \
                 not the witness head {}",
                entries.len()
            ))?;
            return Ok(());
        }
        if self.mirror_digest()? != head_digest {
            self.seal("local journal mirror names a different head entry than the witness")?;
            return Ok(());
        }
        Ok(())
    }

    /// Verifies both hash-chained ledgers completely and binds them to a
    /// terminal checkpoint. Returns `false` when it sealed.
    ///
    /// BY-J3. Two things the first fix left open:
    ///
    /// 1. **The tail beyond the checkpoint was unchecked.** The audit
    ///    chain is unkeyed SHA-256, so anything appended after the
    ///    checkpointed sequence could be rewritten and RE-CHAINED, and
    ///    both the internal chain check and the "matches at the pinned
    ///    sequence" check still passed. Now a checkpoint is appended
    ///    inside every audit-appending transaction, so a committed
    ///    database sits EXACTLY at a checkpoint: the comparison is
    ///    equality, and any extra record — however well re-chained — is
    ///    a chain the endpoint never witnessed.
    /// 2. **`journal_generation` was recorded and never read.** The
    ///    checkpoint now has to name the generation the mirror is at, so
    ///    a truncated/rolled-back checkpoint file (an older but genuinely
    ///    signed record) no longer certifies a newer journal.
    ///
    /// Exactly ONE checkpoint may be skipped: the latest, when its
    /// transaction never committed (the checkpoint is appended just
    /// before the commit). Anything further back is a rollback.
    fn verify_chains_against_checkpoints(&self) -> Result<bool, StoreError> {
        let records = match self.checkpoints.records(&self.witness) {
            Ok(records) => records,
            Err(e) => {
                self.seal(&format!("terminal checkpoints unreadable: {e}"))?;
                return Ok(false);
            }
        };
        if records.is_empty() {
            self.seal("no terminal audit/erasure checkpoint beside the witness")?;
            return Ok(false);
        }
        let mut heads = Vec::new();
        for table in [audit::AUDIT, audit::ERASURE] {
            match audit::head_of(&self.conn, table) {
                Ok((count, hash)) => heads.push((count, hex(&hash))),
                Err(e) => {
                    self.seal(&format!("{table} chain does not verify: {e}"))?;
                    return Ok(false);
                }
            }
        }
        let mirror = self.mirror_gen()?;
        let matches = |c: &checkpoint::Checkpoint| {
            c.journal_generation == mirror
                && heads[0] == (c.audit.seq, c.audit.hash_hex.clone())
                && heads[1] == (c.erasure.seq, c.erasure.hash_hex.clone())
        };
        // A LOST COMMIT is the only reason the database may sit one
        // checkpoint back: the skipped record must then be strictly
        // ahead of the database in both ledgers and name this generation
        // or its immediate successor. Anything else — an older
        // checkpoint reinstated, a generation the endpoint never reached
        // — is not a crash window.
        let provisional = |c: &checkpoint::Checkpoint| {
            c.audit.seq >= heads[0].0
                && c.erasure.seq >= heads[1].0
                && (c.journal_generation == mirror || c.journal_generation == mirror + 1)
        };
        let last = records.len() - 1;
        if matches(&records[last])
            || (provisional(&records[last]) && last > 0 && matches(&records[last - 1]))
        {
            return Ok(true);
        }
        let latest = records.last().map(|c| {
            format!(
                "checkpoint {} pins audit {}/{} erasure {}/{} at generation {}",
                c.sequence,
                c.audit.seq,
                &c.audit.hash_hex[..c.audit.hash_hex.len().min(12)],
                c.erasure.seq,
                &c.erasure.hash_hex[..c.erasure.hash_hex.len().min(12)],
                c.journal_generation
            )
        });
        self.seal(&format!(
            "audit/erasure ledgers do not match any terminal checkpoint: database holds \
             audit {}/{} erasure {}/{} at journal generation {mirror}; {}",
            heads[0].0,
            &heads[0].1[..heads[0].1.len().min(12)],
            heads[1].0,
            &heads[1].1[..heads[1].1.len().min(12)],
            latest.unwrap_or_default()
        ))?;
        Ok(false)
    }

    // ------------------------------------------------------- cursors ----

    fn cursor_secret(&self) -> Result<Vec<u8>, StoreError> {
        schema::meta_get(&self.conn, META_CURSOR_SECRET)?
            .ok_or_else(|| StoreError::Corrupt("store is not bootstrapped".to_owned()))
    }

    /// The SHORT authenticated events continuation: the same
    /// audience/scope binding as `mint_events_cursor`, inside the
    /// §14.9 128-byte identifier bound so it fits the C2
    /// `KoveeEndeavorFormResult.source_cursor` field (which is typed
    /// `identifier`, not an unbounded token). Both forms verify through
    /// the same secret and are accepted wherever a continuation is.
    pub fn mint_short_cursor(&self, society_id: &str, seq: u64) -> Result<String, StoreError> {
        let bound = format!("bs1|projection|events:{society_id}|{seq}");
        let tag = hmac_sha256(&self.cursor_secret()?, bound.as_bytes());
        Ok(format!("bs1.{society_id}.{seq:x}.{}", hex(&tag[..16])))
    }

    fn parse_short_cursor(&self, raw: &str) -> Option<(String, u64)> {
        let mut parts = raw.split('.');
        let (Some("bs1"), Some(society), Some(seq_hex), Some(tag), None) = (
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
        ) else {
            return None;
        };
        let seq = u64::from_str_radix(seq_hex, 16).ok()?;
        let expected = self.mint_short_cursor(society, seq).ok()?;
        // Constant-shape comparison against the freshly minted token.
        if expected.split('.').nth(3)? != tag {
            return None;
        }
        Some((society.to_owned(), seq))
    }

    /// The endpoint's detached signature envelope over one result
    /// (§16.3 `server_signature`). HONEST PROFILE LABEL: the personal
    /// developer profile has no asymmetric endpoint identity material
    /// (§19), so this is a keyed MAC under a store-root scope key —
    /// verifiable by this endpoint and by a same-UID holder of the
    /// store, and NOT an offline third-party-verifiable signature.
    pub fn endpoint_sign(&self, payload: &Value) -> Result<String, StoreError> {
        let key = self.scope_key("endpoint-result-signature")?;
        let preimage = jcs(payload)?;
        Ok(format!("sig1.{}", hex(&hmac_sha256(&key, &preimage))))
    }

    /// Mints the opaque authenticated events continuation (§14.4):
    /// audience- and scope-bound, never a raw sequence on the wire.
    pub fn mint_events_cursor(&self, society_id: &str, seq: u64) -> Result<String, StoreError> {
        let payload = serde_json::json!({
            "v": 1,
            "source": format!("events:{society_id}"),
            "aud": "projection",
            "seq": seq,
        });
        let bytes = serde_json::to_vec(&payload)?;
        let tag = hmac_sha256(&self.cursor_secret()?, &bytes);
        Ok(format!("bc1.{}.{}", hex(&bytes), hex(&tag)))
    }

    /// Verifies and decodes an events continuation, returning the bound
    /// `(society_id, sequence)`. A token this endpoint did not mint for
    /// the projection audience is indistinguishably invalid.
    pub fn parse_events_cursor_any(&self, raw: &str) -> Result<(String, u64), Problem> {
        let fail = || {
            Problem::new(ProblemKind::Invalid, "invalid continuation")
                .with_status(400)
                .with_detail("not a continuation this endpoint minted for this source")
        };
        if raw.starts_with("bs1.") {
            return self.parse_short_cursor(raw).ok_or_else(fail);
        }
        let mut parts = raw.split('.');
        let (Some("bc1"), Some(body), Some(tag), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(fail());
        };
        let bytes = unhex(body).ok_or_else(fail)?;
        let tag = unhex(tag).ok_or_else(fail)?;
        let secret = self.cursor_secret().map_err(|_| fail())?;
        if hmac_sha256(&secret, &bytes).as_slice() != tag.as_slice() {
            return Err(fail());
        }
        let payload: Value = serde_json::from_slice(&bytes).map_err(|_| fail())?;
        if payload["aud"].as_str() != Some("projection") {
            return Err(fail());
        }
        let source = payload["source"].as_str().ok_or_else(fail)?;
        let society = source.strip_prefix("events:").ok_or_else(fail)?;
        let seq = payload["seq"].as_u64().ok_or_else(fail)?;
        Ok((society.to_owned(), seq))
    }

    /// Verifies and decodes an events continuation for the exact
    /// society. A token minted for another source or audience is
    /// indistinguishably invalid.
    pub fn parse_events_cursor(&self, raw: &str, society_id: &str) -> Result<u64, Problem> {
        let fail = || {
            Problem::new(ProblemKind::Invalid, "invalid continuation")
                .with_status(400)
                .with_detail("not a continuation this endpoint minted for this source")
        };
        if raw.starts_with("bs1.") {
            return match self.parse_short_cursor(raw) {
                Some((bound, seq)) if bound == society_id => Ok(seq),
                _ => Err(fail()),
            };
        }
        let mut parts = raw.split('.');
        let (Some("bc1"), Some(body), Some(tag), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(fail());
        };
        let bytes = unhex(body).ok_or_else(fail)?;
        let tag = unhex(tag).ok_or_else(fail)?;
        let secret = self.cursor_secret().map_err(|_| fail())?;
        if hmac_sha256(&secret, &bytes).as_slice() != tag.as_slice() {
            return Err(fail());
        }
        let payload: Value = serde_json::from_slice(&bytes).map_err(|_| fail())?;
        if payload["source"].as_str() != Some(&format!("events:{society_id}"))
            || payload["aud"].as_str() != Some("projection")
        {
            return Err(fail());
        }
        payload["seq"].as_u64().ok_or_else(fail)
    }

    // ------------------------------------------------------ id minting ----

    /// A fresh prefixed id: OS entropy as hex.
    pub fn new_id(&self, prefix: &str) -> Result<String, StoreError> {
        Ok(format!("{prefix}-{}", hex(&random_bytes::<12>()?)))
    }
}

/// The next per-Society event sequence to allocate, memoized in `heads`:
/// the value the transition's effects will leave on the Society row,
/// else the row as it stands.
fn next_sequence(
    conn: &Connection,
    heads: &mut Vec<(String, u64)>,
    society_id: &str,
    effects: &[Effect],
) -> Result<u64, StoreError> {
    if let Some((_, seq)) = heads.iter().find(|(s, _)| s == society_id) {
        return Ok(*seq);
    }
    let staged = effects.iter().rev().find_map(|e| match e {
        Effect::Upsert { table, row }
            if table == "societies"
                && row.get("society_id").and_then(Value::as_str) == Some(society_id) =>
        {
            row.get("next_event_sequence").and_then(Value::as_u64)
        }
        _ => None,
    });
    let seq = match staged {
        Some(seq) => seq,
        None => conn
            .query_row(
                "SELECT next_event_sequence FROM societies WHERE society_id = ?1",
                [society_id],
                |r| r.get::<_, i64>(0),
            )
            .optional()?
            .unwrap_or(1) as u64,
    };
    heads.push((society_id.to_owned(), seq));
    Ok(seq)
}

/// Verifies one witness receipt member by member (§15.3): witness key,
/// signature, incarnation, prior head, generation, transaction id,
/// transition digest and result digest. Returns the seal reason on the
/// first mismatch.
#[allow(clippy::too_many_arguments)]
fn verify_receipt(
    entry: &witness::JournalEntry,
    txn_id: &str,
    incarnation: &str,
    prior_generation: u64,
    prior_entry_digest: &str,
    transition_digest: &str,
    result_digest: &str,
    witness_key_id: &str,
) -> Result<(), String> {
    let fail = |what: &str| Err(format!("witness receipt for {txn_id}: {what}"));
    if entry.transaction_id != txn_id {
        return fail("transaction id mismatch");
    }
    if entry.endpoint_incarnation != incarnation {
        return fail("endpoint incarnation mismatch");
    }
    if entry.generation != prior_generation + 1 {
        return fail("generation is not the proposed successor");
    }
    if entry.prior_entry_digest != prior_entry_digest {
        return fail("prior journal entry digest mismatch");
    }
    if entry.transition_digest != transition_digest {
        return fail("transition digest mismatch");
    }
    if entry.result_digest != result_digest {
        return fail("result digest mismatch");
    }
    if entry.witness_key_id != witness_key_id {
        return fail("signed under a foreign witness key");
    }
    Ok(())
}

/// Removes one secret's hex bytes from EVERY text-bearing column of
/// EVERY table (BY-D2), returning how many values it rewrote.
///
/// The sweep is deliberately blind to meaning. If a raw copy ever
/// reappeared inside an unfinalized `authority_pending.payload`, the
/// sweep would change that payload and its recomputed transition digest
/// would no longer match the witness — finalize would then SEAL rather
/// than commit, which is the right way round: erasure wins, and the
/// endpoint says so instead of quietly diverging.
///
/// Enumerating the columns from `sqlite_master`/`table_info` rather than
/// naming them is the point: the confirmation found the secret in three
/// columns the fix had not thought of, and a hand-written list would
/// silently miss the fourth. Case-insensitive, since hex is written both
/// ways across the codebase.
fn scrub_everywhere(conn: &Connection, secret_hex: &str) -> Result<usize, StoreError> {
    let tables: Vec<String> = {
        let mut stmt = conn.prepare(
            "SELECT name FROM sqlite_master WHERE type = 'table'
               AND name NOT LIKE 'sqlite_%'",
        )?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.collect::<Result<_, _>>()?
    };
    let mut swept = 0usize;
    for table in tables {
        let columns: Vec<(String, String)> = {
            let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
            let rows =
                stmt.query_map([], |r| Ok((r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?;
            rows.collect::<Result<_, _>>()?
        };
        for (column, kind) in columns {
            let kind = kind.to_ascii_uppercase();
            if !(kind.contains("TEXT") || kind.contains("BLOB") || kind.is_empty() || kind == "ANY")
            {
                continue;
            }
            swept += conn.execute(
                &format!(
                    "UPDATE \"{table}\" SET \"{column}\" =
                         replace(replace(CAST(\"{column}\" AS TEXT), ?1, ''), ?2, '')
                     WHERE instr(CAST(\"{column}\" AS TEXT), ?1) > 0
                        OR instr(CAST(\"{column}\" AS TEXT), ?2) > 0"
                ),
                params![secret_hex, secret_hex.to_ascii_uppercase()],
            )?;
        }
    }
    Ok(swept)
}

pub(crate) fn random_bytes<const N: usize>() -> Result<[u8; N], StoreError> {
    let mut out = [0u8; N];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut out))
        .map_err(StoreError::Entropy)?;
    Ok(out)
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn temp_store(name: &str) -> Store {
        let dir = std::env::temp_dir().join(format!(
            "byom-store-{}-{name}-{}",
            std::process::id(),
            bpp_core::time::unix_now()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        Store::open(&dir).unwrap()
    }

    fn scope(store: &Store, op: &str, key: &str) -> MutationScope {
        MutationScope {
            society_id: GENESIS_SCOPE.into(),
            operation: op.into(),
            actor: "governance:sovereign".into(),
            meta: MutationMeta {
                request_id: format!("req-{key}"),
                idempotency_key: key.into(),
                expected_endpoint_incarnation: store.incarnation().unwrap(),
                expected_recovery_epoch: 0,
                expected_revision: None,
                causation_event_ref: None,
                correlation_ref: None,
            },
            body: serde_json::json!({"op": op, "k": key}),
        }
    }

    #[test]
    fn replay_is_byte_identical_and_never_reexecutes() {
        let mut store = temp_store("replay");
        let s = scope(&store, "society_prepare", "k1");
        let bytes = store
            .authority_mutation(&s, 0, CrashHooks::NONE, |_, _| {
                Ok(Prepared {
                    result: serde_json::json!({"n": 1}),
                    revision: Some(1),
                    cursor: CursorMint::None,
                    effects: vec![],
                    events: vec![],
                })
            })
            .unwrap();
        let replay = store
            .authority_mutation(&s, 0, CrashHooks::NONE, |_, _| {
                panic!("a replay never re-executes")
            })
            .unwrap();
        assert_eq!(bytes, replay);
        assert_eq!(
            store.witness.head().unwrap(),
            1,
            "exactly one journal entry"
        );
    }

    #[test]
    fn changed_request_under_same_key_is_idempotency_mismatch() {
        let mut store = temp_store("mismatch");
        let s = scope(&store, "society_prepare", "k1");
        store
            .authority_mutation(&s, 0, CrashHooks::NONE, |_, _| {
                Ok(Prepared {
                    result: serde_json::json!({}),
                    revision: None,
                    cursor: CursorMint::None,
                    effects: vec![],
                    events: vec![],
                })
            })
            .unwrap();
        let mut changed = s.clone();
        changed.body = serde_json::json!({"op": "society_prepare", "k": "DIFFERENT"});
        let err = store
            .authority_mutation(&changed, 0, CrashHooks::NONE, |_, _| {
                panic!("a mismatch never re-executes")
            })
            .unwrap_err();
        match err {
            CommandError::Problem(p) => assert_eq!(p.kind, ProblemKind::IdempotencyMismatch),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn failed_apply_commits_nothing_and_witnesses_nothing() {
        let mut store = temp_store("failed");
        let s = scope(&store, "society_prepare", "k1");
        let err = store
            .authority_mutation(&s, 0, CrashHooks::NONE, |_, _| {
                Err(Problem::new(ProblemKind::StaleRevision, "stale"))
            })
            .unwrap_err();
        assert!(matches!(err, CommandError::Problem(_)));
        assert_eq!(store.witness.head().unwrap(), 0, "no journal entry");
        let pending: i64 = store
            .conn()
            .query_row("SELECT COUNT(*) FROM authority_pending", [], |r| r.get(0))
            .unwrap();
        assert_eq!(pending, 0, "nothing prepared");
    }

    #[test]
    fn lost_witness_reply_recovers_by_query_and_finalizes_once() {
        let mut store = temp_store("lostreply");
        let s = scope(&store, "society_prepare", "k1");
        let hooks = CrashHooks {
            witness_fault: WitnessFault::LoseReplyAfterWrite,
            ..CrashHooks::NONE
        };
        let bytes = store
            .authority_mutation(&s, 0, hooks, |_, _| {
                Ok(Prepared {
                    result: serde_json::json!({"ok": true}),
                    revision: Some(1),
                    cursor: CursorMint::None,
                    effects: vec![],
                    events: vec![],
                })
            })
            .unwrap();
        assert!(!bytes.is_empty());
        assert_eq!(
            store.witness.head().unwrap(),
            1,
            "one entry despite lost reply"
        );
    }

    #[test]
    fn lost_witness_request_abandons_after_proof() {
        let mut store = temp_store("lostreq");
        let s = scope(&store, "society_prepare", "k1");
        let hooks = CrashHooks {
            witness_fault: WitnessFault::LoseRequest,
            ..CrashHooks::NONE
        };
        let err = store
            .authority_mutation(&s, 0, hooks, |_, _| {
                Ok(Prepared {
                    result: serde_json::json!({}),
                    revision: None,
                    cursor: CursorMint::None,
                    effects: vec![],
                    events: vec![],
                })
            })
            .unwrap_err();
        match err {
            CommandError::Problem(p) => assert_eq!(p.kind, ProblemKind::Unavailable),
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(store.witness.head().unwrap(), 0);
        let state: String = store
            .conn()
            .query_row("SELECT state FROM authority_pending", [], |r| r.get(0))
            .unwrap();
        assert_eq!(state, "abandoned", "abandoned only after proving no entry");
    }
}
