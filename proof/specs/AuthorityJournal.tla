--------------------------- MODULE AuthorityJournal ---------------------------
(***************************************************************************)
(* Byom B0.1 authority mutation journal (byom DESIGN.md section 15.3, the  *)
(* R0/B1 journal semantics): every authoritative mutation is (1) a         *)
(* serializable SQL transaction writing the full transition and an         *)
(* AuthorityMutationPending row, INVISIBLE and unusable; (2) an external   *)
(* non-rollbackable witness compare-and-swap of (incarnation, prior        *)
(* generation, prior digest) to the exact next AuthorityJournalEntry; (3)  *)
(* a second SQL transaction that verifies the receipt, finalizes, and only *)
(* then makes the retained result visible.                                 *)
(*                                                                         *)
(* Projection: two competing transactions, one endpoint incarnation, an    *)
(* external journal modeled as a generation counter plus an entry log      *)
(* (append-only: the adversary cannot roll it back), and a DB snapshot/    *)
(* restore adversary (one snapshot, one restore) standing in for the       *)
(* section 15.3 rollback threat.  Digests are folded into generation       *)
(* numbers plus transaction identity; the witness dedups by transaction    *)
(* id, which is what makes the CAS retry/query-safe.  The daemon crashes   *)
(* at every commit boundary (all protocol steps are separate actions and   *)
(* Crash can interleave anywhere); startup runs the section 15.3           *)
(* comparison and a journal/database mismatch starts sealed_diagnostic,    *)
(* closing every authority surface.                                        *)
(***************************************************************************)
EXTENDS Naturals, FiniteSets

CONSTANTS T,       \* transaction ids, e.g. {t1, t2}
          MaxGen   \* bound on external journal generations

None == "none"

VARIABLES
  up,        \* daemon is up
  endpoint,  \* incarnation status: active | sealed_diagnostic
  pend,      \* [T -> AuthorityMutationPending state] (DB, rollbackable)
  obs,       \* [T -> 0..MaxGen] journal head observed at (re)prepare (DB)
  extGen,    \* external witness generation (NON-rollbackable)
  extLog,    \* [1..MaxGen -> T + none] entry per generation (NON-rollbackable)
  mirrorGen, \* local journal mirror (DB, rollbackable)
  visible,   \* [T -> BOOLEAN] retained result readable / permit released (DB)
  snapTaken, restored,            \* adversary bookkeeping (one shot each)
  snapPend, snapObs, snapMirror, snapVisible  \* the DB snapshot

vars == <<up, endpoint, pend, obs, extGen, extLog, mirrorGen, visible,
          snapTaken, restored, snapPend, snapObs, snapMirror, snapVisible>>

PendStates == {"absent", "prepared", "witness_unknown", "witnessed",
               "finalized", "abandoned"}

HasEntry(t) == \E g \in 1..extGen : extLog[g] = t
GenOf(t) == CHOOSE g \in 1..extGen : extLog[g] = t

TypeOK ==
  /\ up \in BOOLEAN
  /\ endpoint \in {"active", "sealed_diagnostic"}
  /\ pend \in [T -> PendStates]
  /\ obs \in [T -> 0..MaxGen]
  /\ extGen \in 0..MaxGen
  /\ extLog \in [1..MaxGen -> T \cup {None}]
  /\ mirrorGen \in 0..MaxGen
  /\ visible \in [T -> BOOLEAN]
  /\ snapTaken \in BOOLEAN
  /\ restored \in BOOLEAN
  /\ snapPend \in [T -> PendStates]
  /\ snapObs \in [T -> 0..MaxGen]
  /\ snapMirror \in 0..MaxGen
  /\ snapVisible \in [T -> BOOLEAN]

Init ==
  /\ up = TRUE
  /\ endpoint = "active"
  /\ pend = [t \in T |-> "absent"]
  /\ obs = [t \in T |-> 0]
  /\ extGen = 0
  /\ extLog = [g \in 1..MaxGen |-> None]
  /\ mirrorGen = 0
  /\ visible = [t \in T |-> FALSE]
  /\ snapTaken = FALSE /\ restored = FALSE
  /\ snapPend = [t \in T |-> "absent"]
  /\ snapObs = [t \in T |-> 0]
  /\ snapMirror = 0
  /\ snapVisible = [t \in T |-> FALSE]

Snap == <<snapTaken, restored, snapPend, snapObs, snapMirror, snapVisible>>

-----------------------------------------------------------------------------
(* Step 1 - journal_sql_prepare: the serializable SQL transaction writes   *)
(* the full transition and pending set, invisible, against the OBSERVED    *)
(* journal head (the local mirror).  Commits no reply or permit.           *)
Prepare(t) ==
  /\ up /\ endpoint = "active"
  /\ pend[t] = "absent"
  /\ pend' = [pend EXCEPT ![t] = "prepared"]
  /\ obs' = [obs EXCEPT ![t] = mirrorGen]
  /\ UNCHANGED <<up, endpoint, extGen, extLog, mirrorGen, visible>>
  /\ UNCHANGED Snap

(* journal_sql_prepare: a competing CAS advanced the head - complete       *)
(* dependency revalidation under a NEW proposed generation; the old        *)
(* pending transition stays inert (section 15.3).                          *)
RePrepare(t) ==
  /\ up /\ endpoint = "active"
  /\ pend[t] = "prepared"
  /\ obs[t] # mirrorGen
  /\ obs' = [obs EXCEPT ![t] = mirrorGen]
  /\ pend' = [pend EXCEPT ![t] = "prepared"]
  /\ UNCHANGED <<up, endpoint, extGen, extLog, mirrorGen, visible>>
  /\ UNCHANGED Snap

(* Step 2 - journal_witness_cas: the witness atomically CASes (prior       *)
(* generation, prior digest) to the exact next entry and returns a signed  *)
(* receipt.                                                                *)
WitnessOK(t) ==
  /\ up /\ endpoint = "active"
  /\ pend[t] = "prepared"
  /\ ~HasEntry(t)
  /\ obs[t] = extGen
  /\ extGen < MaxGen
  /\ extGen' = extGen + 1
  /\ extLog' = [extLog EXCEPT ![extGen + 1] = t]
  /\ pend' = [pend EXCEPT ![t] = "witnessed"]
  /\ UNCHANGED <<up, endpoint, obs, mirrorGen, visible>>
  /\ UNCHANGED Snap

(* journal_witness_cas: the transaction id makes the CAS retry-safe - a    *)
(* re-sent request for an already-journaled transaction returns the        *)
(* existing receipt, never a second entry.                                 *)
WitnessDedup(t) ==
  /\ up /\ endpoint = "active"
  /\ pend[t] = "prepared"
  /\ HasEntry(t)
  /\ pend' = [pend EXCEPT ![t] = "witnessed"]
  /\ UNCHANGED <<up, endpoint, obs, extGen, extLog, mirrorGen, visible>>
  /\ UNCHANGED Snap

(* journal_witness_cas: the receipt is lost in flight AFTER the entry was  *)
(* written - a witness timeout is queried by transaction id, never         *)
(* guessed.                                                                *)
WitnessLostAfterWrite(t) ==
  /\ up /\ endpoint = "active"
  /\ pend[t] = "prepared"
  /\ ~HasEntry(t)
  /\ obs[t] = extGen
  /\ extGen < MaxGen
  /\ extGen' = extGen + 1
  /\ extLog' = [extLog EXCEPT ![extGen + 1] = t]
  /\ pend' = [pend EXCEPT ![t] = "witness_unknown"]
  /\ UNCHANGED <<up, endpoint, obs, mirrorGen, visible>>
  /\ UNCHANGED Snap

(* journal_witness_cas: the request never reached the witness (or the CAS  *)
(* lost and the response vanished) - unknown, no entry.                    *)
WitnessLostNoWrite(t) ==
  /\ up /\ endpoint = "active"
  /\ pend[t] = "prepared"
  /\ pend' = [pend EXCEPT ![t] = "witness_unknown"]
  /\ UNCHANGED <<up, endpoint, obs, extGen, extLog, mirrorGen, visible>>
  /\ UNCHANGED Snap

(* journal_witness_cas: the query by transaction id finds the exact entry. *)
QueryFound(t) ==
  /\ up /\ endpoint = "active"
  /\ pend[t] = "witness_unknown"
  /\ HasEntry(t)
  /\ pend' = [pend EXCEPT ![t] = "witnessed"]
  /\ UNCHANGED <<up, endpoint, obs, extGen, extLog, mirrorGen, visible>>
  /\ UNCHANGED Snap

(* journal_sql_prepare: the query PROVED no entry - the exact transaction  *)
(* is retried against the current head.                                    *)
QueryRetry(t) ==
  /\ up /\ endpoint = "active"
  /\ pend[t] = "witness_unknown"
  /\ ~HasEntry(t)
  /\ pend' = [pend EXCEPT ![t] = "prepared"]
  /\ obs' = [obs EXCEPT ![t] = mirrorGen]
  /\ UNCHANGED <<up, endpoint, extGen, extLog, mirrorGen, visible>>
  /\ UNCHANGED Snap

(* journal_abandon: inert pending state is abandoned only after PROVING no *)
(* journal entry exists for it.                                            *)
Abandon(t) ==
  /\ up /\ endpoint = "active"
  /\ pend[t] \in {"prepared", "witness_unknown"}
  /\ ~HasEntry(t)
  /\ pend' = [pend EXCEPT ![t] = "abandoned"]
  /\ UNCHANGED <<up, endpoint, obs, extGen, extLog, mirrorGen, visible>>
  /\ UNCHANGED Snap

(* Step 3 - journal_sql_finalize: verifies the receipt, marks the exact    *)
(* pending set finalized/visible, advances the local mirror.  ONLY NOW may *)
(* Byom return success, release a permit, or publish an event.             *)
Finalize(t) ==
  /\ up /\ endpoint = "active"
  /\ pend[t] = "witnessed"
  /\ pend' = [pend EXCEPT ![t] = "finalized"]
  /\ visible' = [visible EXCEPT ![t] = TRUE]
  /\ mirrorGen' = IF GenOf(t) > mirrorGen THEN GenOf(t) ELSE mirrorGen
  /\ UNCHANGED <<up, endpoint, obs, extGen, extLog>>
  /\ UNCHANGED Snap

(* The daemon dies at an arbitrary commit boundary; everything modeled is  *)
(* durable, so crashing loses nothing - recovery decides what it means.    *)
Crash ==
  /\ up
  /\ up' = FALSE
  /\ UNCHANGED <<endpoint, pend, obs, extGen, extLog, mirrorGen, visible>>
  /\ UNCHANGED Snap

(* Startup comparison (section 15.3): a witnessed generation whose         *)
(* transaction the database no longer knows is a journal/database mismatch *)
(* - it cannot be skipped or re-created under a new transaction id; the    *)
(* endpoint starts sealed_diagnostic and every authority surface stays     *)
(* closed.  (In-flight pending rows are NOT a mismatch: they are the       *)
(* closed crash states, recovered by query/receipt above.)                 *)
Mismatch == \E g \in 1..extGen :
              extLog[g] # None /\ pend[extLog[g]] = "absent"

Recover ==
  /\ ~up
  /\ up' = TRUE
  /\ endpoint' = IF Mismatch THEN "sealed_diagnostic" ELSE endpoint
  /\ UNCHANGED <<pend, obs, extGen, extLog, mirrorGen, visible>>
  /\ UNCHANGED Snap

(* The rollback adversary: takes one backup of the restorable database ... *)
SnapshotTake ==
  /\ up /\ ~snapTaken
  /\ snapTaken' = TRUE
  /\ snapPend' = pend
  /\ snapObs' = obs
  /\ snapMirror' = mirrorGen
  /\ snapVisible' = visible
  /\ UNCHANGED <<up, endpoint, pend, obs, extGen, extLog, mirrorGen, visible,
                 restored>>

(* ... and later restores it in place of the live database.  The external  *)
(* journal does NOT roll back.  Restore implies a restart, so the startup  *)
(* comparison always runs before any surface reopens.                      *)
RollbackRestore ==
  /\ snapTaken /\ ~restored
  /\ pend' = snapPend
  /\ obs' = snapObs
  /\ mirrorGen' = snapMirror
  /\ visible' = snapVisible
  /\ restored' = TRUE
  /\ up' = FALSE
  /\ UNCHANGED <<endpoint, extGen, extLog, snapTaken, snapPend, snapObs,
                 snapMirror, snapVisible>>

Next ==
  \/ \E t \in T :
       \/ Prepare(t) \/ RePrepare(t)
       \/ WitnessOK(t) \/ WitnessDedup(t)
       \/ WitnessLostAfterWrite(t) \/ WitnessLostNoWrite(t)
       \/ QueryFound(t) \/ QueryRetry(t) \/ Abandon(t) \/ Finalize(t)
  \/ Crash \/ Recover \/ SnapshotTake \/ RollbackRestore

Spec == Init /\ [][Next]_vars

-----------------------------------------------------------------------------
(* Machine-checked invariants                                              *)

(* Section 15.3 step 3: NO visible mutation (readable result, released     *)
(* permit, published event) without its witnessed journal entry - across   *)
(* crashes, competing CASes, lost receipts, and database rollback.         *)
NoVisibleWithoutEntry == \A t \in T : visible[t] => HasEntry(t)

(* Visibility is exactly finalization: a rolled-back database may hide a   *)
(* finalized result again (mismatch -> sealed), but nothing is ever        *)
(* visible in a state the pending protocol did not finalize.               *)
VisibleIsFinalized == \A t \in T : visible[t] => pend[t] = "finalized"

(* The local mirror never runs ahead of the non-rollbackable external      *)
(* journal.                                                                *)
MirrorNeverAhead == mirrorGen <= extGen

(* A transaction is abandoned only when the external journal provably      *)
(* holds no entry for it - abandonment never orphans a witnessed entry.    *)
AbandonedHasNoEntry ==
  \A t \in T : pend[t] = "abandoned" => ~HasEntry(t)

(* Witnessed/finalized pending state always has its exact entry.           *)
WitnessedHasEntry ==
  \A t \in T : pend[t] \in {"witnessed", "finalized"} => HasEntry(t)

(* The witness dedups by transaction id: one entry per transaction, ever.  *)
EntryUnique ==
  \A t \in T : Cardinality({g \in 1..extGen : extLog[g] = t}) <= 1

(* Rollback detection: an endpoint that is up and serving (active) has no  *)
(* journal/database mismatch - a restore that erased witnessed state can   *)
(* only reopen as sealed_diagnostic (section 15.3 startup comparison).     *)
ActiveHasNoMismatch == (up /\ endpoint = "active") => ~Mismatch

-----------------------------------------------------------------------------
(* Machine-checked action properties (TLC PROPERTY)                        *)

(* A sealed endpoint mints no new journal entries and widens no            *)
(* visibility: recovery diagnostics only (section 15.3).                   *)
SealedNoNewAuthority ==
  [][endpoint = "sealed_diagnostic" =>
       (extGen' = extGen /\ \A t \in T : visible'[t] => visible[t])]_vars

(* The external journal is append-only and monotonic (the rollback         *)
(* adversary never touches it).                                            *)
ExternalMonotonic ==
  [][extGen' >= extGen
     /\ \A g \in 1..MaxGen :
          (g <= extGen /\ extLog[g] # None) => extLog'[g] = extLog[g]]_vars

=============================================================================
\* Descriptor-model parity annotations (proof/check-descriptors.py).
\* @parity module: AuthorityJournal
\* @parity descriptor: authority-journal.json
\* @parity state: prepared
\* @parity state: witness_unknown
\* @parity state: witnessed
\* @parity state: finalized
\* @parity state: abandoned
\* @parity transition: absent -> prepared via journal_sql_prepare
\* @parity transition: prepared -> prepared via journal_sql_prepare
\* @parity transition: prepared -> witnessed via journal_witness_cas
\* @parity transition: prepared -> witness_unknown via journal_witness_cas
\* @parity transition: witness_unknown -> witnessed via journal_witness_cas
\* @parity transition: witness_unknown -> prepared via journal_sql_prepare
\* @parity transition: prepared -> abandoned via journal_abandon
\* @parity transition: witness_unknown -> abandoned via journal_abandon
\* @parity transition: witnessed -> finalized via journal_sql_finalize
