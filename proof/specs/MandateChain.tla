---------------------------- MODULE MandateChain ----------------------------
(***************************************************************************)
(* Byom B0.1 Mandate chain (byom DESIGN.md sections 10.1-10.2, 14.8),      *)
(* exactly as committed in spec/descriptors/mandate.json, over a bounded   *)
(* derivation chain root -> c1 -> c2.                                      *)
(*                                                                         *)
(* Projection: authority is a finite capability set (BPA-1 subjects,       *)
(* resources, data classes, destinations ... are all folded into abstract  *)
(* capabilities); budgets are in the BudgetConservation model; use slots   *)
(* are MaxUses = 1 so the last-slot consumption IS the exhaustion row.     *)
(* Derivation is the section 10.2 mechanical subset; a widening attempt is *)
(* modeled explicitly (AttemptWiden) and must fail authority_widening      *)
(* (G33) - it is a rejected no-op, so never-widening is checked against    *)
(* live widening attempts, not by their absence.  All variables are        *)
(* durable; daemon crash is stuttering; replays are disabled by guards.    *)
(***************************************************************************)
EXTENDS Naturals

CONSTANTS Caps,          \* the abstract capability universe
          NonDelegable   \* human non-delegable powers (subset of Caps)

ASSUME NonDelegable \subseteq Caps

M == {"root", "c1", "c2"}
Children == {"c1", "c2"}
Parent(m) == CASE m = "c1" -> "root" [] m = "c2" -> "c1" [] OTHER -> "none"
MaxUses == 1

VARIABLES
  mst,           \* [M -> mandate state]
  auth,          \* [M -> SUBSET Caps]: granted authority (empty until prepared)
  uses,          \* [M -> 0..MaxUses]: MandateUse ordinal slots consumed
  widenRejected  \* history: widening derivation attempts rejected (bounded)

vars == <<mst, auth, uses, widenRejected>>

States == {"absent", "prepared", "active", "held", "exhausted", "revoked",
           "expired", "superseded"}

\* Every ancestor of m (proper, up to root).
Ancestors(m) == CASE m = "root" -> {}
                  [] m = "c1"   -> {"root"}
                  [] OTHER      -> {"root", "c1"}

ChainActive(m) == \A a \in Ancestors(m) : mst[a] = "active"

TypeOK ==
  /\ mst \in [M -> States]
  /\ auth \in [M -> SUBSET Caps]
  /\ uses \in [M -> 0..MaxUses]
  /\ widenRejected \in 0..1

Init ==
  /\ mst = [m \in M |-> "absent"]
  /\ auth = [m \in M |-> {}]
  /\ uses = [m \in M |-> 0]
  /\ widenRejected = 0

-----------------------------------------------------------------------------
(* mandate_prepare (R15): server-prepared root proposal - the exact        *)
(* subject is fixed at preparation ("prepared" is the folded pre-issue     *)
(* state, G30).                                                            *)
PrepareRoot ==
  /\ mst["root"] = "absent"
  /\ \E a \in (SUBSET Caps) \ {{}} :
       auth' = [auth EXCEPT !["root"] = a]
  /\ mst' = [mst EXCEPT !["root"] = "prepared"]
  /\ UNCHANGED <<uses, widenRejected>>

(* mandate_derive (R15): a child Mandate is a MECHANICAL SUBSET of every   *)
(* parent (section 10.2) - no new operation/resource/class/destination,    *)
(* and human non-delegable powers never appear in a child.                 *)
Derive(m) ==
  /\ m \in Children
  /\ mst[m] = "absent"
  /\ mst[Parent(m)] = "active"
  /\ \E a \in (SUBSET (auth[Parent(m)] \ NonDelegable)) \ {{}} :
       auth' = [auth EXCEPT ![m] = a]
  /\ mst' = [mst EXCEPT ![m] = "prepared"]
  /\ UNCHANGED <<uses, widenRejected>>

(* A widening derivation attempt fails authority_widening (G33): the       *)
(* request is rejected, no Mandate is created.  Modeled as a no-op with a  *)
(* bounded history flag so TLC explores traces containing the attempt.     *)
AttemptWiden(m) ==
  /\ m \in Children
  /\ mst[m] = "absent"
  /\ mst[Parent(m)] = "active"
  /\ \E a \in SUBSET Caps :
       a \ (auth[Parent(m)] \ NonDelegable) # {}
  /\ widenRejected = 0
  /\ widenRejected' = 1
  /\ UNCHANGED <<mst, auth, uses>>

(* mandate_position (R16/R17): seat Position lifecycle folded as a         *)
(* proposal-stage self-transition (G19).                                   *)
Position(m) ==
  /\ mst[m] = "prepared"
  /\ mst' = [mst EXCEPT ![m] = "prepared"]
  /\ UNCHANGED <<auth, uses, widenRejected>>

(* mandate_issue (R18): locks the exact decision and the COMPLETE parent   *)
(* closure - a child cannot be issued under a non-active parent.           *)
Issue(m) ==
  /\ mst[m] = "prepared"
  /\ m \in Children => mst[Parent(m)] = "active"
  /\ mst' = [mst EXCEPT ![m] = "active"]
  /\ UNCHANGED <<auth, uses, widenRejected>>

(* mandate_issue: successor issuance atomically supersedes the prior       *)
(* active revision (G32).                                                  *)
Supersede(m) ==
  /\ mst[m] = "active"
  /\ mst' = [mst EXCEPT ![m] = "superseded"]
  /\ UNCHANGED <<auth, uses, widenRejected>>

(* mandate_hold (R18): fences new uses; no un-hold exists in the catalog   *)
(* and none is derived (G32).                                              *)
HoldM(m) ==
  /\ mst[m] = "active"
  /\ mst' = [mst EXCEPT ![m] = "held"]
  /\ UNCHANGED <<auth, uses, widenRejected>>

(* execution_permit_consume cascade (R34): MandateUse slots are created on *)
(* consumption; the use consuming the LAST slot moves the Mandate to       *)
(* exhausted (G32).  A use requires the whole chain active: revocation,    *)
(* exhaustion, hold, expiry, or supersession of any ancestor fences every  *)
(* descendant's new uses (the exhaustion cascade).                         *)
Consume(m) ==
  /\ mst[m] = "active"
  /\ ChainActive(m)
  /\ uses[m] < MaxUses
  /\ uses' = [uses EXCEPT ![m] = @ + 1]
  /\ mst' = [mst EXCEPT ![m] = "exhausted"]  \* MaxUses = 1: last slot
  /\ UNCHANGED <<auth, widenRejected>>

(* mandate_revoke (R18): blocks new uses and cannot un-send prior effects. *)
RevokeM(m) ==
  /\ mst[m] = "active"
  /\ mst' = [mst EXCEPT ![m] = "revoked"]
  /\ UNCHANGED <<auth, uses, widenRejected>>

(* server_time: deadlines and expiry use authoritative server time.        *)
ExpireM(m) ==
  /\ mst[m] = "active"
  /\ mst' = [mst EXCEPT ![m] = "expired"]
  /\ UNCHANGED <<auth, uses, widenRejected>>

Next ==
  \/ PrepareRoot
  \/ \E m \in M :
       \/ Position(m) \/ Issue(m) \/ Supersede(m) \/ HoldM(m)
       \/ Consume(m) \/ RevokeM(m) \/ ExpireM(m)
  \/ \E m \in Children : Derive(m) \/ AttemptWiden(m)

Spec == Init /\ [][Next]_vars

-----------------------------------------------------------------------------
(* Machine-checked invariants                                              *)

(* Section 10.2 never-widening: every existing child's authority is a      *)
(* subset of its parent's - under live widening attempts.                  *)
NeverWiden ==
  \A m \in Children :
    mst[m] # "absent" => auth[m] \subseteq auth[Parent(m)]

(* Transitive closure witness: the grandchild never exceeds the root.      *)
RootClosure == mst["c2"] # "absent" => auth["c2"] \subseteq auth["root"]

(* Human non-delegable powers never appear in any child.                   *)
NonDelegableNeverDerived ==
  \A m \in Children : auth[m] \cap NonDelegable = {}

(* Use slots are bounded and exhaustion is exactly slot exhaustion.        *)
UseCap == \A m \in M : uses[m] <= MaxUses
ExhaustedIsSpent == \A m \in M : (mst[m] = "exhausted") <=> (uses[m] = MaxUses)

-----------------------------------------------------------------------------
(* Machine-checked action properties (TLC PROPERTY)                        *)

(* Revocation cannot un-send prior effects: uses never decrease.           *)
UsesMonotonic ==
  [][\A m \in M : uses'[m] >= uses[m]]_vars

(* The exhaustion/revocation cascade: a NEW use happens only on an active  *)
(* Mandate whose complete ancestor chain is active at that moment.         *)
NoUseUnderInactiveChain ==
  [][\A m \in M :
       uses'[m] > uses[m] => (mst[m] = "active" /\ ChainActive(m))]_vars

=============================================================================
\* Descriptor-model parity annotations (proof/check-descriptors.py).
\* @parity module: MandateChain
\* @parity descriptor: mandate.json
\* @parity state: prepared
\* @parity state: active
\* @parity state: held
\* @parity state: exhausted
\* @parity state: revoked
\* @parity state: expired
\* @parity state: superseded
\* @parity transition: absent -> prepared via mandate_prepare
\* @parity transition: absent -> prepared via mandate_derive
\* @parity transition: prepared -> prepared via mandate_position
\* @parity transition: prepared -> active via mandate_issue
\* @parity transition: active -> superseded via mandate_issue
\* @parity transition: active -> held via mandate_hold
\* @parity transition: active -> exhausted via execution_permit_consume
\* @parity transition: active -> revoked via mandate_revoke
\* @parity transition: active -> expired via server_time
\* @parity crash: absent -> prepared via mandate_prepare = no bearer authority escapes commit (§14.8 Mandate row)
\* @parity fences: absent -> prepared via mandate_prepare = (none)
\* @parity crash: absent -> prepared via mandate_derive = child delegated quantity conserved (§14.8 Mandate row)
\* @parity fences: absent -> prepared via mandate_derive = budget reserved from the parent atomically
\* @parity crash: prepared -> prepared via mandate_position = prior Position inputs remain (§14.8 Position/Decision row)
\* @parity fences: prepared -> prepared via mandate_position = one current seat head (§14.8 Position/Decision row)
\* @parity crash: prepared -> active via mandate_issue = no bearer authority escapes commit; child delegated quantity conserved (§14.8 Mandate row)
\* @parity fences: prepared -> active via mandate_issue = (none)
\* @parity crash: active -> superseded via mandate_issue = no bearer authority escapes commit (§14.8 Mandate row)
\* @parity fences: active -> superseded via mandate_issue = successor issuance supersedes the prior active revision atomically
\* @parity crash: active -> held via mandate_hold = no bearer authority escapes commit (§14.8 Mandate row)
\* @parity fences: active -> held via mandate_hold = hold fences new uses
\* @parity crash: active -> exhausted via execution_permit_consume = child delegated quantity conserved (§14.8 Mandate row)
\* @parity fences: active -> exhausted via execution_permit_consume = MandateUse ordinal slots created on consumption
\* @parity crash: active -> revoked via mandate_revoke = child delegated quantity conserved (§14.8 Mandate row)
\* @parity fences: active -> revoked via mandate_revoke = revocation fences new uses
\* @parity crash: active -> expired via server_time = no bearer authority escapes commit (§14.8 Mandate row)
\* @parity fences: active -> expired via server_time = (none)
