-------------------------- MODULE BudgetConservation -------------------------
(***************************************************************************)
(* Byom B0.1 budget conservation (byom DESIGN.md section 11.4): for each   *)
(* dimension and account revision,                                         *)
(*                                                                         *)
(*   ceiling = remaining + reserved + committed + uncertain                *)
(*             + delegated_to_children                                     *)
(*                                                                         *)
(* held as an invariant under reserve / commit / measured settlement /     *)
(* release / ambiguous (uncertain) marking / conservative resolution /     *)
(* child delegation / unused-delegation return.                            *)
(*                                                                         *)
(* Projection: one dimension, one parent BudgetAccount and one delegated   *)
(* child account; quantities are small naturals.  BudgetReservationSet,    *)
(* ExternalBudgetBridge, and UsageSettlement records are folded into the   *)
(* bucket moves they cause; the Kovee bridge's deny/unknown outcomes are   *)
(* the Release / MarkUncertain moves.  released_lifetime is the monotonic  *)
(* audit counter, bounded by MaxReleased for finite checking only.  This   *)
(* model has no descriptor machine: BudgetAccount is a ledger, not a       *)
(* section 14.8 transition machine - the parity checker is told so.        *)
(* All variables are durable; daemon crash is stuttering.                  *)
(***************************************************************************)
EXTENDS Naturals

CONSTANTS Cap,         \* parent ceiling (constant across the run)
          MaxReleased  \* exploration bound on the released_lifetime counter

A == {"parent", "child"}
Amounts == 1..2

VARIABLES
  ceiling,     \* [A -> Nat] ceiling per account (child 0 until delegation)
  remaining,   \* [A -> Nat]
  reserved,    \* [A -> Nat]
  committed,   \* [A -> Nat]
  uncertain,   \* [A -> Nat]
  delegated,   \* [A -> Nat] delegated_to_children
  released,    \* released_lifetime: monotonic audit counter (never a bucket)
  childExists  \* the child account has been created

vars == <<ceiling, remaining, reserved, committed, uncertain, delegated,
          released, childExists>>

TypeOK ==
  /\ ceiling \in [A -> 0..Cap]
  /\ remaining \in [A -> 0..Cap]
  /\ reserved \in [A -> 0..Cap]
  /\ committed \in [A -> 0..Cap]
  /\ uncertain \in [A -> 0..Cap]
  /\ delegated \in [A -> 0..Cap]
  /\ released \in 0..MaxReleased
  /\ childExists \in BOOLEAN

Init ==
  /\ ceiling = [a \in A |-> IF a = "parent" THEN Cap ELSE 0]
  /\ remaining = [a \in A |-> IF a = "parent" THEN Cap ELSE 0]
  /\ reserved = [a \in A |-> 0]
  /\ committed = [a \in A |-> 0]
  /\ uncertain = [a \in A |-> 0]
  /\ delegated = [a \in A |-> 0]
  /\ released = 0
  /\ childExists = FALSE

Usable(a) == a = "parent" \/ childExists

-----------------------------------------------------------------------------
(* Reservation: worst-case amount moves remaining -> reserved in one Byom  *)
(* transaction (section 11.4).                                             *)
Reserve(a, q) ==
  /\ Usable(a)
  /\ remaining[a] >= q
  /\ remaining' = [remaining EXCEPT ![a] = @ - q]
  /\ reserved' = [reserved EXCEPT ![a] = @ + q]
  /\ UNCHANGED <<ceiling, committed, uncertain, delegated, released,
                 childExists>>

(* Measured settlement: a reservation of q settles to actual r <= q; the   *)
(* unspent remainder returns to remaining and counts toward the audit      *)
(* counter.  r = q is a full commit; r = 0 is a pure release (a bridge     *)
(* denial releasing only demonstrably unspent reservations).               *)
Settle(a, q, r) ==
  /\ reserved[a] >= q
  /\ released + (q - r) <= MaxReleased
  /\ reserved' = [reserved EXCEPT ![a] = @ - q]
  /\ committed' = [committed EXCEPT ![a] = @ + r]
  /\ remaining' = [remaining EXCEPT ![a] = @ + (q - r)]
  /\ released' = released + (q - r)
  /\ UNCHANGED <<ceiling, uncertain, delegated, childExists>>

(* An unknown external result: the reservation is neither spent nor        *)
(* releasable - it moves to uncertain, never silently back to remaining.   *)
MarkUncertain(a, q) ==
  /\ reserved[a] >= q
  /\ reserved' = [reserved EXCEPT ![a] = @ - q]
  /\ uncertain' = [uncertain EXCEPT ![a] = @ + q]
  /\ UNCHANGED <<ceiling, remaining, committed, delegated, released,
                 childExists>>

(* Reconciliation of an uncertain quantity: settle to the conservative     *)
(* maximum (commit) or prove non-use (release).                            *)
ResolveUncertainCommit(a, q) ==
  /\ uncertain[a] >= q
  /\ uncertain' = [uncertain EXCEPT ![a] = @ - q]
  /\ committed' = [committed EXCEPT ![a] = @ + q]
  /\ UNCHANGED <<ceiling, remaining, reserved, delegated, released,
                 childExists>>

ResolveUncertainRelease(a, q) ==
  /\ uncertain[a] >= q
  /\ released + q <= MaxReleased
  /\ uncertain' = [uncertain EXCEPT ![a] = @ - q]
  /\ remaining' = [remaining EXCEPT ![a] = @ + q]
  /\ released' = released + q
  /\ UNCHANGED <<ceiling, reserved, committed, delegated, childExists>>

(* Child delegation (section 11.4): moves quantity from the parent's       *)
(* remaining bucket into delegated_to_children and creates the child       *)
(* ceiling ATOMICALLY - settlement cannot spend it in both places.         *)
Delegate(q) ==
  /\ ~childExists
  /\ remaining["parent"] >= q
  /\ remaining' = [remaining EXCEPT !["parent"] = @ - q,
                                    !["child"] = q]
  /\ delegated' = [delegated EXCEPT !["parent"] = @ + q]
  /\ ceiling' = [ceiling EXCEPT !["child"] = q]
  /\ childExists' = TRUE
  /\ UNCHANGED <<reserved, committed, uncertain, released>>

(* Returning provably unheld child headroom shrinks the child ceiling and  *)
(* the parent's delegated bucket in the same transaction.                  *)
ReturnUnused(q) ==
  /\ childExists
  /\ remaining["child"] >= q
  /\ remaining' = [remaining EXCEPT !["child"] = @ - q,
                                    !["parent"] = @ + q]
  /\ ceiling' = [ceiling EXCEPT !["child"] = @ - q]
  /\ delegated' = [delegated EXCEPT !["parent"] = @ - q]
  /\ UNCHANGED <<reserved, committed, uncertain, released, childExists>>

Next ==
  \/ \E a \in A, q \in Amounts :
       \/ Reserve(a, q)
       \/ MarkUncertain(a, q)
       \/ ResolveUncertainCommit(a, q)
       \/ ResolveUncertainRelease(a, q)
       \/ \E r \in 0..q : Settle(a, q, r)
  \/ \E q \in Amounts : Delegate(q) \/ ReturnUnused(q)

Spec == Init /\ [][Next]_vars

-----------------------------------------------------------------------------
(* Machine-checked invariants                                              *)

(* THE conservation identity, per account, in every reachable state.       *)
Conservation ==
  \A a \in A :
    ceiling[a] = remaining[a] + reserved[a] + committed[a] + uncertain[a]
                 + delegated[a]

(* Delegated quantity exists exactly once: the parent's                    *)
(* delegated_to_children equals the child ceiling it created.              *)
DelegatedMatchesChild == delegated["parent"] = ceiling["child"]

(* The model's depth bound: the child delegates nothing further.           *)
ChildDelegatesNothing == delegated["child"] = 0

(* The parent ceiling never moves; expanding the ledger to the child       *)
(* buckets re-derives it exactly - "settlement cannot spend it in both     *)
(* places" (section 11.4), Kovee bridging cannot double-charge.            *)
GrandConservation ==
  /\ ceiling["parent"] = Cap
  /\ Cap = remaining["parent"] + reserved["parent"] + committed["parent"]
           + uncertain["parent"]
           + remaining["child"] + reserved["child"] + committed["child"]
           + uncertain["child"]

-----------------------------------------------------------------------------
(* Machine-checked action properties (TLC PROPERTY)                        *)

(* released_lifetime is a monotonic audit counter, not an available        *)
(* bucket.                                                                 *)
ReleasedMonotonic == [][released' >= released]_vars

(* Settled spend is never silently un-spent.                               *)
CommittedMonotonic ==
  [][\A a \in A : committed'[a] >= committed[a]]_vars

=============================================================================
\* Descriptor-model parity annotations (proof/check-descriptors.py).
\* @parity module: BudgetConservation
\* @parity none: BudgetAccount is the section 11.4 ledger, not a section
\*   14.8 transition machine; B0.1 commits no budget descriptor.  The
\*   BudgetReservationSet / ExternalBudgetBridge state enums land with the
\*   runtime slice; this model checks the conservation identity only.
