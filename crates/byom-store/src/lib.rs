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
pub mod effects;
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
use effects::{Effect, NewEvent};
use rusqlite::{params, Connection, OptionalExtension as _};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use witness::{CasOutcome, Witness, WitnessFault};

const META_ENDPOINT_INCARNATION: &str = "endpoint_incarnation";
const META_INDEX_ROOT_KEY: &str = "index_root_key";
const META_CURSOR_SECRET: &str = "cursor_secret";
const META_MIRROR_GEN: &str = "journal_mirror_gen";
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

#[derive(Debug, Serialize, Deserialize)]
struct PendingPayload {
    society_id: String,
    operation: String,
    request_digest: String,
    result: Value,
    revision: Option<u64>,
    cursor: CursorMint,
    effects: Vec<Effect>,
    events: Vec<NewEvent>,
}

/// One personal-profile authoritative database plus its witness.
pub struct Store {
    conn: Connection,
    witness: Witness,
    data_dir: PathBuf,
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
        let store = Store {
            conn,
            witness,
            data_dir: data_dir.to_owned(),
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
        tx.commit()?;
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
        tx.commit()?;
        Ok(())
    }

    fn mirror_gen(&self) -> Result<u64, StoreError> {
        Ok(schema::meta_get_text(&self.conn, META_MIRROR_GEN)?
            .and_then(|s| s.parse().ok())
            .unwrap_or(0))
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
    pub(crate) fn scope_key(&self, label: &str) -> Result<[u8; 32], StoreError> {
        let root = self.index_root_key()?;
        Ok(hmac_sha256(&root, label.as_bytes()))
    }

    /// The per-Society idempotency-index key — a SCOPE key (PROFILE §5):
    /// destroying it erases offline verifiability of the entire index.
    pub fn society_index_key(&self, society_id: &str) -> Result<[u8; 32], StoreError> {
        self.scope_key(&format!("idempotency-index:{society_id}"))
    }

    /// The channel-derived actor binding digest (§14.2): a typed
    /// `local_erasure_safe` commitment over the authenticated actor
    /// string; never accepted from a request body.
    pub fn actor_binding_digest(
        &self,
        society_id: &str,
        actor: &str,
    ) -> Result<DigestRef, StoreError> {
        let root = self.index_root_key()?;
        let object_key = hmac_sha256(
            &root,
            format!("actor-binding:{society_id}:{actor}").as_bytes(),
        );
        let preimage = tagged_canonical(
            "bpp-actor-binding-v0",
            &serde_json::json!({ "actor": actor }),
        )?;
        let mac = hmac_sha256(&object_key, &preimage);
        let key_ref = format!(
            "society-key:{society_id}/actor:{}",
            &sha256_hex(actor.as_bytes())[..16]
        );
        Ok(DigestRef::local_erasure_safe(&key_ref, hex(&mac)))
    }

    /// Mints a `local_erasure_safe` digest over an object under a fresh
    /// per-object secret; returns (digest, secret_hex).
    pub fn mint_object_digest(
        &self,
        key_ref: &str,
        tag: &str,
        object: &Value,
    ) -> Result<(DigestRef, String), StoreError> {
        let secret = random_bytes::<32>()?;
        let preimage = tagged_canonical(tag, object)?;
        let mac = hmac_sha256(&secret, &preimage);
        Ok((
            DigestRef::local_erasure_safe(key_ref, hex(&mac)),
            hex(&secret),
        ))
    }

    /// A deterministic `local_erasure_safe` record digest: the
    /// per-object key is derived from the store root for the exact
    /// object id (developer profile: derived per-object keys keep
    /// digests recomputable without a secrets table; erasure is by root
    /// destruction — honestly narrower than the hosted profile).
    pub fn record_digest(
        &self,
        society_id: &str,
        object_id: &str,
        tag: &str,
        object: &Value,
    ) -> Result<DigestRef, StoreError> {
        let root = self.index_root_key()?;
        let object_key = hmac_sha256(&root, format!("record:{society_id}:{object_id}").as_bytes());
        let preimage = tagged_canonical(tag, object)?;
        Ok(DigestRef::local_erasure_safe(
            &format!("society-key:{society_id}/object:{object_id}"),
            hex(&hmac_sha256(&object_key, &preimage)),
        ))
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
    /// `causation_event_ref`, `correlation_ref`) — reusing a key with a
    /// changed covered value is `idempotency_mismatch`.
    pub fn request_digest(body: &Value) -> Result<String, StoreError> {
        let mut projected = body.clone();
        if let Some(meta) = projected.get_mut("meta").and_then(Value::as_object_mut) {
            meta.remove("request_id");
            meta.remove("causation_event_ref");
            meta.remove("correlation_ref");
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

            // Step 1 — journal_sql_prepare: the full transition, invisible.
            let observed_gen = self.mirror_gen()?;
            let txn_id = format!("txn-{}", hex(&random_bytes::<12>()?));
            let tx = self.conn.unchecked_transaction()?;
            let prepared = match apply(&tx, scope) {
                Ok(p) => p,
                Err(problem) => {
                    tx.rollback()?;
                    return Err(CommandError::Problem(problem));
                }
            };
            let payload = PendingPayload {
                society_id: scope.society_id.clone(),
                operation: scope.operation.clone(),
                request_digest: request_digest.clone(),
                result: prepared.result,
                revision: prepared.revision,
                cursor: prepared.cursor,
                effects: prepared.effects,
                events: prepared.events,
            };
            let payload_json = serde_json::to_value(&payload).map_err(StoreError::from)?;
            let transition_digest = sha256_hex(&jcs(&payload_json).map_err(StoreError::from)?);
            tx.execute(
                "INSERT INTO authority_pending
                    (transaction_id, endpoint_incarnation, society_id, operation,
                     actor_binding_digest, idempotency_domain_digest,
                     prior_journal_generation, proposed_generation, transition_digest,
                     state, payload, created_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'prepared',?10,?11)",
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
                ],
            )?;
            tx.commit()?;

            if hooks.abort_before_witness {
                // Crash before the witness CAS: inert pending state,
                // recovered (abandoned after proof) at next startup.
                std::process::abort();
            }

            // Step 2 — journal_witness_cas.
            let entry = match self.witness.cas(
                &incarnation,
                observed_gen,
                &txn_id,
                &transition_digest,
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
            if entry.transition_digest != transition_digest {
                // Same transaction id, different digest: impossible under
                // honest operation — refuse to finalize.
                self.seal("witness entry digest mismatch for own transaction")?;
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
            let bytes = self.finalize_pending(&txn_id, entry.generation, now, &hooks)?;

            if hooks.abort_after_finalize {
                // Crash after commit, before the reply: the retry finds
                // the stored byte-identical result.
                std::process::abort();
            }
            return Ok(bytes);
        }
    }

    fn abandon_pending(&self, txn_id: &str, now: i64) -> Result<(), StoreError> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE authority_pending SET state = 'abandoned' WHERE transaction_id = ?1",
            [txn_id],
        )?;
        audit::append(&tx, now, "authority-journal.abandoned", txn_id)?;
        tx.commit()?;
        Ok(())
    }

    /// The §15.3 step-3 finalize: materializes the exact pending set —
    /// used by both the live path and startup recovery, so a recovered
    /// transaction finalizes identically.
    fn finalize_pending(
        &self,
        txn_id: &str,
        generation: u64,
        now: i64,
        hooks: &CrashHooks,
    ) -> Result<Vec<u8>, StoreError> {
        let tx = self.conn.unchecked_transaction()?;
        let (payload_text, state): (String, String) = tx.query_row(
            "SELECT payload, state FROM authority_pending WHERE transaction_id = ?1",
            [txn_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if state == "finalized" {
            return Err(StoreError::Corrupt(format!(
                "transaction {txn_id} already finalized"
            )));
        }
        let payload: PendingPayload = serde_json::from_str(&payload_text)?;

        // Materialize state effects.
        for effect in &payload.effects {
            effects::apply(&tx, effect)?;
        }

        // Append events with dense per-Society sequences.
        let occurred_at = rfc3339_utc(now);
        let mut last_seq: Option<(String, u64)> = None;
        for event in &payload.events {
            let seq: i64 = tx.query_row(
                "UPDATE societies SET next_event_sequence = next_event_sequence + 1
                 WHERE society_id = ?1 RETURNING next_event_sequence - 1",
                [&event.society_id],
                |r| r.get(0),
            )?;
            let secret = random_bytes::<32>()?;
            let key_ref = format!("society-key:{}/event:{}", event.society_id, event.event_id);
            let preimage = tagged_canonical("bpp-event-payload-v0", &event.payload)?;
            let digest =
                DigestRef::local_erasure_safe(&key_ref, hex(&hmac_sha256(&secret, &preimage)));
            tx.execute(
                "INSERT INTO events (event_id, society_id, sequence, kind, object_ref,
                     object_revision, participant_ref, actor_ref, causation_ref,
                     correlation_ref, payload, payload_digest, payload_secret,
                     visibility_scope_ref, occurred_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
                params![
                    event.event_id,
                    event.society_id,
                    seq,
                    event.kind,
                    event.object_ref,
                    event.object_revision as i64,
                    event.participant_ref,
                    event.actor_ref,
                    event.causation_ref,
                    event.correlation_ref,
                    event.payload.to_string(),
                    serde_json::to_string(&digest)?,
                    hex(&secret),
                    event.visibility_scope_ref,
                    occurred_at,
                ],
            )?;
            tx.execute(
                "INSERT INTO outbox (delivery_id, kind, payload, created_at)
                 VALUES (?1, 'event', ?2, ?3)",
                params![
                    event.event_id,
                    serde_json::json!({
                        "event_id": event.event_id,
                        "society_id": event.society_id,
                        "sequence": seq,
                        "kind": event.kind,
                    })
                    .to_string(),
                    occurred_at,
                ],
            )?;
            last_seq = Some((event.society_id.clone(), seq as u64));
        }

        // Mint the source cursor now that sequences exist.
        let source_cursor = match &payload.cursor {
            CursorMint::None => None,
            CursorMint::FromStart { society_id } => {
                Some(self.mint_events_cursor_with(&tx, society_id, 0)?)
            }
            CursorMint::AfterEvents { society_id } => {
                let seq = match &last_seq {
                    Some((sid, seq)) if sid == society_id => *seq,
                    _ => {
                        // No events appended for that society: cursor at
                        // the current head.
                        let head: i64 = tx.query_row(
                            "SELECT next_event_sequence - 1 FROM societies WHERE society_id = ?1",
                            [society_id],
                            |r| r.get(0),
                        )?;
                        head.max(0) as u64
                    }
                };
                Some(self.mint_events_cursor_with(&tx, society_id, seq)?)
            }
        };

        // Retain the exact result bytes for idempotent replay.
        let success = Success {
            outcome: "ok".to_owned(),
            result: payload.result.clone(),
            revision: payload.revision,
            source_cursor,
        };
        let bytes = serde_json::to_vec(&success)?;
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
                occurred_at,
            ],
        )?;

        // Mark the exact pending set finalized and advance the mirror.
        tx.execute(
            "UPDATE authority_pending SET state = 'finalized' WHERE transaction_id = ?1",
            [txn_id],
        )?;
        let mirror = self.mirror_gen()?.max(generation);
        schema::meta_set(&tx, META_MIRROR_GEN, mirror.to_string().as_bytes())?;
        audit::append(
            &tx,
            now,
            "authority-journal.finalized",
            &format!("{txn_id} generation {generation} op {operation}"),
        )?;

        if hooks.abort_before_finalize {
            // Crash inside finalize before commit: witnessed entry with
            // an unfinalized pending row — recovered by the exact
            // receipt at startup, finalized once.
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
                    // the exact receipt and finalized once.
                    self.finalize_pending(&txn_id, entry.generation, now, &CrashHooks::NONE)?;
                    let tx = self.conn.unchecked_transaction()?;
                    audit::append(&tx, now, "authority-journal.recovered", &txn_id)?;
                    tx.commit()?;
                }
                None => {
                    // Proven no entry: abandoned, never guessed.
                    self.abandon_pending(&txn_id, now)?;
                }
            }
        }
        Ok(())
    }

    // ------------------------------------------------------- cursors ----

    fn cursor_secret(&self) -> Result<Vec<u8>, StoreError> {
        schema::meta_get(&self.conn, META_CURSOR_SECRET)?
            .ok_or_else(|| StoreError::Corrupt("store is not bootstrapped".to_owned()))
    }

    fn mint_events_cursor_with(
        &self,
        _conn: &Connection,
        society_id: &str,
        seq: u64,
    ) -> Result<String, StoreError> {
        self.mint_events_cursor(society_id, seq)
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

fn random_bytes<const N: usize>() -> Result<[u8; N], StoreError> {
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
