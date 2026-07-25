------------------------- MODULE GreenfieldEnablement -------------------------
(***************************************************************************)
(* The D10 greenfield enablement saga (C2 slice 1): kovee amendment A2 and *)
(* the frozen governance_enable KCP authority row of the family contract   *)
(* section 2.A, exactly as committed in                                    *)
(* spec/descriptors/greenfield-enablement.json and specified in            *)
(* spec/governed-work/greenfield-saga.md.                                  *)
(*                                                                         *)
(* Projection: per governed scope, the saga phase of its                   *)
(* KoveeGovernanceOwnerBinding row plus the enablement (binding) epoch     *)
(* and the owner arm.  Step 1 (create KoveeRealmByomBinding +              *)
(* KoveeSocietyMapping durably, not yet authoritative) and step 2 (CAS     *)
(* the owner none -> byom at the expected revision, atomic with            *)
(* activation) are the two mutating actions; the record bytes, subject     *)
(* digests, principal identity, and step-up assurance are abstracted --    *)
(* the frozen authority row fixes them and a CAS proves concurrency, not   *)
(* authority.  Overlap is a constant symmetric relation over scopes        *)
(* (selector semantics are Kovee-owned); the guard on both steps is the    *)
(* "no overlapping active owner selectors" rule of byom section 16.6       *)
(* item 1.  Retries are guarded no-ops returning the stored identical      *)
(* binding -- the descriptor-level idempotency claim.  All variables are   *)
(* durable; a daemon crash between any two transitions is stuttering.     *)
(* Restore honesty: a Kovee store restore re-enters at the durable saga    *)
(* state and an unknown CAS outcome is resolved by the recovery-only       *)
(* query before retry or rollback (greenfield-saga.md section 5); the      *)
(* rewind of durable state itself is out of model scope and recorded in    *)
(* PROPERTIES.md.                                                          *)
(***************************************************************************)
EXTENDS Naturals

CONSTANTS Scopes,    \* governed scopes, e.g. {s1, s2}
          Overlaps,  \* set of unordered scope pairs whose selectors overlap
          MaxEpoch   \* bound on enablement (binding) epochs per scope

VARIABLES
  phase,          \* scope -> saga phase ("absent" + the 4 descriptor states)
  epoch,          \* scope -> current binding epoch (0 = never enabled)
  owner,          \* scope -> KoveeGovernanceOwnerBinding.governance_owner arm
  createCount,    \* history: scope -> epoch -> step-1 executions
  activateCount,  \* history: scope -> epoch -> owner-CAS executions
  activated,      \* history: scope -> set of epochs that activated
  rolledBack      \* history: scope -> set of epochs rolled back

vars == <<phase, epoch, owner, createCount, activateCount, activated,
          rolledBack>>

Phases == {"absent", "bindings_created", "active", "rolled_back", "disabled"}
Epochs == 1..MaxEpoch

Overlapping(s, t) == {s, t} \in Overlaps

\* A scope whose saga holds its owner selector: pending bindings hold the
\* uniqueness slot exactly like an active owner row (section 16.6 item 1).
HoldsSelector(s) == phase[s] \in {"bindings_created", "active"}

TypeOK ==
  /\ phase \in [Scopes -> Phases]
  /\ epoch \in [Scopes -> 0..MaxEpoch]
  /\ owner \in [Scopes -> {"sage", "byom", "none"}]
  /\ createCount \in [Scopes -> [Epochs -> 0..2]]
  /\ activateCount \in [Scopes -> [Epochs -> 0..2]]
  /\ activated \in [Scopes -> SUBSET Epochs]
  /\ rolledBack \in [Scopes -> SUBSET Epochs]

Init ==
  /\ phase = [s \in Scopes |-> "absent"]
  /\ epoch = [s \in Scopes |-> 0]
  /\ owner = [s \in Scopes |-> "none"]
  /\ createCount = [s \in Scopes |-> [e \in Epochs |-> 0]]
  /\ activateCount = [s \in Scopes |-> [e \in Epochs |-> 0]]
  /\ activated = [s \in Scopes |-> {}]
  /\ rolledBack = [s \in Scopes |-> {}]

-----------------------------------------------------------------------------
(* governance_enable step 1: durably create KoveeRealmByomBinding +        *)
(* KoveeSocietyMapping (pending, not yet authoritative); the owner binding *)
(* stays none.  From absent, or from rolled_back under a NEW binding       *)
(* epoch -- the rolled-back epoch's bindings are never resurrected.  An    *)
(* overlapping held selector rejects the enable (no transition).          *)
EnableCreate(s) ==
  /\ phase[s] \in {"absent", "rolled_back"}
  /\ epoch[s] < MaxEpoch
  /\ \A t \in Scopes : (t # s /\ Overlapping(s, t)) => ~HoldsSelector(t)
  /\ phase' = [phase EXCEPT ![s] = "bindings_created"]
  /\ epoch' = [epoch EXCEPT ![s] = @ + 1]
  /\ createCount' = [createCount EXCEPT ![s][epoch[s] + 1] = @ + 1]
  /\ UNCHANGED <<owner, activateCount, activated, rolledBack>>

(* governance_enable retry (before or after activation): returns the       *)
(* stored identical pending bindings or the identical active binding --    *)
(* never a second creation, CAS, or epoch advance.  A guarded no-op: the   *)
(* descriptor self-rows' idempotency claim.                                *)
EnableRetry(s) ==
  /\ phase[s] \in {"bindings_created", "active"}
  /\ UNCHANGED vars

(* owner_cas_none_to_byom, saga step 2: CAS the                            *)
(* KoveeGovernanceOwnerBinding none -> byom at the expected revision,      *)
(* atomic with binding/mapping activation; re-checks that no overlapping   *)
(* owner selector is active.  A CAS proves concurrency, not authority.    *)
Activate(s) ==
  /\ phase[s] = "bindings_created"
  /\ owner[s] = "none"
  /\ \A t \in Scopes : (t # s /\ Overlapping(s, t)) => phase[t] # "active"
  /\ phase' = [phase EXCEPT ![s] = "active"]
  /\ owner' = [owner EXCEPT ![s] = "byom"]
  /\ activateCount' = [activateCount EXCEPT ![s][epoch[s]] = @ + 1]
  /\ activated' = [activated EXCEPT ![s] = @ \cup {epoch[s]}]
  /\ UNCHANGED <<epoch, createCount, rolledBack>>

(* governance_enable_rollback: strictly before activation.  Voids the      *)
(* pending bindings, the owner stays none, and the binding epoch is spent  *)
(* -- the rolled-back epoch can never activate.                            *)
Rollback(s) ==
  /\ phase[s] = "bindings_created"
  /\ phase' = [phase EXCEPT ![s] = "rolled_back"]
  /\ rolledBack' = [rolledBack EXCEPT ![s] = @ \cup {epoch[s]}]
  /\ UNCHANGED <<epoch, owner, createCount, activateCount, activated>>

(* governance_disable (always step-up): freezes the owner row (status      *)
(* active -> frozen); the owner arm is retained for audit and the frozen   *)
(* row holds no active selector.  Terminal in this machine: re-enablement  *)
(* after a governed disable is a fresh saga row under a new binding epoch. *)
Disable(s) ==
  /\ phase[s] = "active"
  /\ phase' = [phase EXCEPT ![s] = "disabled"]
  /\ UNCHANGED <<epoch, owner, createCount, activateCount, activated,
                 rolledBack>>

Next ==
  \E s \in Scopes :
    EnableCreate(s) \/ EnableRetry(s) \/ Activate(s) \/ Rollback(s)
    \/ Disable(s)

Spec == Init /\ [][Next]_vars

-----------------------------------------------------------------------------
(* Machine-checked invariants                                              *)

(* Section 16.6 item 1 / frozen authority row: no overlapping ACTIVE owner *)
(* selectors -- no scope ends up under two governance owners.              *)
NoOverlappingActiveOwners ==
  \A s, t \in Scopes :
    (s # t /\ Overlapping(s, t)) =>
      ~(phase[s] = "active" /\ phase[t] = "active")

(* Stronger slot honesty: a pending enablement already holds its selector, *)
(* so two overlapping sagas can never both be past step 1.                 *)
NoOverlappingEnablementSlots ==
  \A s, t \in Scopes :
    (s # t /\ Overlapping(s, t)) => ~(HoldsSelector(s) /\ HoldsSelector(t))

(* Frozen-row fence: exact-CAS at the expected revision -- per scope and   *)
(* binding epoch, step 1 executes at most once and the owner CAS wins at   *)
(* most once; every retry returns the stored identical binding.            *)
RetryIdempotent ==
  \A s \in Scopes, e \in Epochs :
    createCount[s][e] <= 1 /\ activateCount[s][e] <= 1

(* D10 rollback fence: an epoch that rolled back never activates and an    *)
(* activated epoch never rolls back -- activation after a rollback exists  *)
(* only under a new binding epoch.                                         *)
NoActivationAfterRollback ==
  \A s \in Scopes : activated[s] \cap rolledBack[s] = {}

(* The currently active/frozen enablement never sits on a rolled-back      *)
(* epoch.                                                                  *)
ActiveEpochNeverRolledBack ==
  \A s \in Scopes :
    phase[s] \in {"active", "disabled"} => epoch[s] \notin rolledBack[s]

(* Kovee amendment A1: the sage arm exists for spec fidelity and is never  *)
(* exercised in this stack.                                                *)
SageNeverExercised == \A s \in Scopes : owner[s] # "sage"

(* The owner arm flips exactly at the CAS and survives the freeze: byom    *)
(* iff the saga activated (and was not rolled back before activation).     *)
OwnerMatchesPhase ==
  \A s \in Scopes :
    (owner[s] = "byom") <=> (phase[s] \in {"active", "disabled"})

=============================================================================
\* Descriptor-model parity annotations (proof/check-descriptors.py).
\* @parity module: GreenfieldEnablement
\* @parity descriptor: greenfield-enablement.json
\* @parity state: bindings_created
\* @parity state: active
\* @parity state: rolled_back
\* @parity state: disabled
\* @parity transition: absent -> bindings_created via governance_enable
\* @parity transition: bindings_created -> bindings_created via governance_enable
\* @parity transition: bindings_created -> active via owner_cas_none_to_byom
\* @parity transition: active -> active via governance_enable
\* @parity transition: bindings_created -> rolled_back via governance_enable_rollback
\* @parity transition: rolled_back -> bindings_created via governance_enable
\* @parity transition: active -> disabled via governance_disable
