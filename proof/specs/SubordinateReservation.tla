----------------------- MODULE SubordinateReservation -----------------------
(***************************************************************************)
(* The byom_subordinate budget-bridge saga (C2 slice 2): byom section      *)
(* 11.4's idempotent external-reservation saga as required by section      *)
(* 16.6 item 4 (the stable reserve, query, settle, uncertain, and release  *)
(* steps) and family contract L31-L33, exactly as committed in             *)
(* spec/descriptors/subordinate-reservation.json with the record shape in  *)
(* spec/governed-work/byom-subordinate-reservation.schema.json.            *)
(*                                                                         *)
(* Projection: per stable external reservation key, the bridge-visible     *)
(* saga phase (the section 11.4 ExternalBudgetBridge.state list verbatim)  *)
(* plus the quantities that carry the conservation story: the parent       *)
(* item's worst-case delegation (constant WorstCase -- already reserved    *)
(* byom-side before bridging), the subordinate amount Kovee actually       *)
(* commits (it may narrow, never exceed), and the settled charge.  The     *)
(* hidden variable `truth` records what Kovee's durable store actually     *)
(* did once an outcome went unknown -- the recovery query can only         *)
(* surface that truth, never invent one (guessing is not a transition).   *)
(* Record bytes, digests, account topology, and the byom-side              *)
(* BudgetReservationSet transaction are abstracted; the runner's           *)
(* cross-member vector check pins the per-item never-above-parent rule on  *)
(* concrete bytes.  All variables are durable; a daemon crash between any  *)
(* two transitions is stuttering.                                          *)
(***************************************************************************)
EXTENDS Naturals

CONSTANTS Keys,      \* stable external reservation keys, e.g. {k1, k2}
          WorstCase  \* parent item worst_case_amount (per key, uniform)

VARIABLES
  phase,       \* key -> "absent" + the 6 descriptor states
  amount,      \* key -> subordinate amount Kovee committed (0 before)
  charged,     \* key -> settled charge (0 before settlement)
  truth,       \* key -> "none" | "created" | "not_created" (Kovee durable)
  parentHeld,  \* key -> byom parent reservation still held
  createCount, \* history: key -> Kovee reservation-row creations
  settleCount, \* history: key -> settlements applied (both sides at once)
  reconciled,  \* key -> an R38 budget_reconcile decision was recorded
  relUncertain \* history: key -> released directly out of uncertain

vars == <<phase, amount, charged, truth, parentHeld, createCount,
          settleCount, reconciled, relUncertain>>

Phases == {"absent", "requested", "confirmed", "denied", "uncertain",
           "settled", "released"}
Amounts == 0..WorstCase

TypeOK ==
  /\ phase \in [Keys -> Phases]
  /\ amount \in [Keys -> Amounts]
  /\ charged \in [Keys -> Amounts]
  /\ truth \in [Keys -> {"none", "created", "not_created"}]
  /\ parentHeld \in [Keys -> BOOLEAN]
  /\ createCount \in [Keys -> 0..2]
  /\ settleCount \in [Keys -> 0..2]
  /\ reconciled \in [Keys -> BOOLEAN]
  /\ relUncertain \in [Keys -> BOOLEAN]

Init ==
  /\ phase = [k \in Keys |-> "absent"]
  /\ amount = [k \in Keys |-> 0]
  /\ charged = [k \in Keys |-> 0]
  /\ truth = [k \in Keys |-> "none"]
  /\ parentHeld = [k \in Keys |-> FALSE]
  /\ createCount = [k \in Keys |-> 0]
  /\ settleCount = [k \in Keys |-> 0]
  /\ reconciled = [k \in Keys |-> FALSE]
  /\ relUncertain = [k \in Keys |-> FALSE]

-----------------------------------------------------------------------------
(* subordinate_reserve_request: the byom kernel persists the bridge under  *)
(* the stable key at resource_allocate (L31); the byom parent reservation  *)
(* is already held and stays held while this bridge may still charge.      *)
ReserveRequest(k) ==
  /\ phase[k] = "absent"
  /\ phase' = [phase EXCEPT ![k] = "requested"]
  /\ parentHeld' = [parentHeld EXCEPT ![k] = TRUE]
  /\ UNCHANGED <<amount, charged, truth, createCount, settleCount,
                 reconciled, relUncertain>>

(* Exact retry under the same stable key: the identical pending request,  *)
(* never a second row -- a guarded no-op.                                  *)
ReserveRetry(k) ==
  /\ phase[k] = "requested"
  /\ UNCHANGED vars

(* subordinate_reserved: Kovee durably commits the subordinate            *)
(* reservation, possibly narrowed -- never above the parent worst case.   *)
KoveeCommit(k) ==
  /\ phase[k] = "requested"
  /\ \E a \in Amounts :
       /\ amount' = [amount EXCEPT ![k] = a]
       /\ phase' = [phase EXCEPT ![k] = "confirmed"]
       /\ truth' = [truth EXCEPT ![k] = "created"]
       /\ createCount' = [createCount EXCEPT ![k] = @ + 1]
  /\ UNCHANGED <<charged, parentHeld, settleCount, reconciled, relUncertain>>

(* subordinate_denied: Kovee's definite denial -- nothing was created.    *)
Deny(k) ==
  /\ phase[k] = "requested"
  /\ phase' = [phase EXCEPT ![k] = "denied"]
  /\ truth' = [truth EXCEPT ![k] = "not_created"]
  /\ UNCHANGED <<amount, charged, parentHeld, createCount, settleCount,
                 reconciled, relUncertain>>

(* subordinate_outcome_unknown: the reply is lost.  Kovee's durable store *)
(* nondeterministically either committed the reservation (and the create  *)
(* really happened) or did not -- byom cannot know which yet; the byom     *)
(* reservation stays held and spend stays blocked (section 11.4: an       *)
(* unknown result remains uncertain).                                      *)
OutcomeUnknownCreated(k) ==
  /\ phase[k] = "requested"
  /\ \E a \in Amounts :
       /\ amount' = [amount EXCEPT ![k] = a]
       /\ phase' = [phase EXCEPT ![k] = "uncertain"]
       /\ truth' = [truth EXCEPT ![k] = "created"]
       /\ createCount' = [createCount EXCEPT ![k] = @ + 1]
  /\ UNCHANGED <<charged, parentHeld, settleCount, reconciled, relUncertain>>

OutcomeUnknownNotCreated(k) ==
  /\ phase[k] = "requested"
  /\ phase' = [phase EXCEPT ![k] = "uncertain"]
  /\ truth' = [truth EXCEPT ![k] = "not_created"]
  /\ UNCHANGED <<amount, charged, parentHeld, createCount, settleCount,
                 reconciled, relUncertain>>

(* subordinate_query_unknown: the stable query still cannot prove the     *)
(* outcome -- conservative hold, nothing releases.                         *)
QueryUnknown(k) ==
  /\ phase[k] = "uncertain"
  /\ UNCHANGED vars

(* subordinate_query_confirmed / subordinate_query_denied: the query      *)
(* surfaces Kovee's durable truth -- verification, never invention.        *)
QueryConfirmed(k) ==
  /\ phase[k] = "uncertain"
  /\ truth[k] = "created"
  /\ phase' = [phase EXCEPT ![k] = "confirmed"]
  /\ UNCHANGED <<amount, charged, truth, parentHeld, createCount,
                 settleCount, reconciled, relUncertain>>

QueryDenied(k) ==
  /\ phase[k] = "uncertain"
  /\ truth[k] = "not_created"
  /\ phase' = [phase EXCEPT ![k] = "denied"]
  /\ UNCHANGED <<amount, charged, truth, parentHeld, createCount,
                 settleCount, reconciled, relUncertain>>

(* budget_reconcile (R38): the ONLY release out of uncertain -- an exact  *)
(* governance seat with a fresh challenge accepts the residual ambiguity  *)
(* and releases the byom hold; a timeout never does this.                  *)
ReconcileRelease(k) ==
  /\ phase[k] = "uncertain"
  /\ phase' = [phase EXCEPT ![k] = "released"]
  /\ parentHeld' = [parentHeld EXCEPT ![k] = FALSE]
  /\ reconciled' = [reconciled EXCEPT ![k] = TRUE]
  /\ relUncertain' = [relUncertain EXCEPT ![k] = TRUE]
  /\ UNCHANGED <<amount, charged, truth, createCount, settleCount>>

(* subordinate_settle: measured settlement from a trusted meter or        *)
(* verified provider receipt, stable-keyed, applied once on both sides;   *)
(* the charge never exceeds the subordinate amount (and so never the      *)
(* parent worst case).                                                     *)
Settle(k) ==
  /\ phase[k] = "confirmed"
  /\ \E m \in 0..amount[k] :
       /\ charged' = [charged EXCEPT ![k] = m]
       /\ phase' = [phase EXCEPT ![k] = "settled"]
       /\ settleCount' = [settleCount EXCEPT ![k] = @ + 1]
  /\ UNCHANGED <<amount, truth, parentHeld, createCount, reconciled,
                 relUncertain>>

(* subordinate_release: terminal bookkeeping from a RESOLVED phase only   *)
(* (unspent-confirmed, denied, or settled remainder); the byom parent     *)
(* hold returns in the same accounting step.                               *)
Release(k) ==
  /\ phase[k] \in {"confirmed", "denied", "settled"}
  /\ phase' = [phase EXCEPT ![k] = "released"]
  /\ parentHeld' = [parentHeld EXCEPT ![k] = FALSE]
  /\ UNCHANGED <<amount, charged, truth, createCount, settleCount,
                 reconciled, relUncertain>>

Next ==
  \E k \in Keys :
    ReserveRequest(k) \/ ReserveRetry(k) \/ KoveeCommit(k) \/ Deny(k)
    \/ OutcomeUnknownCreated(k) \/ OutcomeUnknownNotCreated(k)
    \/ QueryUnknown(k) \/ QueryConfirmed(k) \/ QueryDenied(k)
    \/ ReconcileRelease(k) \/ Settle(k) \/ Release(k)

Spec == Init /\ [][Next]_vars

-----------------------------------------------------------------------------
(* Machine-checked invariants                                              *)

(* Section 11.4 / L32: the subordinate reservation never exceeds the      *)
(* parent item's worst case -- narrow or deny, never above parent.        *)
NeverAboveParent == \A k \in Keys : amount[k] <= WorstCase

(* No parallel charge: the settled charge never exceeds the subordinate   *)
(* amount actually committed.                                              *)
ChargeWithinReservation == \A k \in Keys : charged[k] <= amount[k]

(* Idempotent create: per stable key, Kovee's reservation row is created  *)
(* at most once -- across direct commit, lost-reply commit, and every     *)
(* retry.                                                                  *)
CreateOnce == \A k \in Keys : createCount[k] <= 1

(* Measured settlement is applied once on both sides (stable settlement   *)
(* key); a retried settle is a no-op returning the stored settlement.     *)
SettleOnce == \A k \in Keys : settleCount[k] <= 1

(* A confirmed or settled bridge reflects a reservation Kovee really      *)
(* committed, and a denial reflects a real absence: the recovery query    *)
(* surfaces truth, it cannot invent an outcome.                            *)
ResolutionIsReal ==
  \A k \in Keys :
    /\ phase[k] \in {"confirmed", "settled"} => truth[k] = "created"
    /\ phase[k] = "denied" => truth[k] = "not_created"

(* Parent conservation of the hold: the byom parent reservation stays     *)
(* held exactly while the bridge is open (may still charge or is not yet  *)
(* released) -- a denial or unknown result never silently unblocks spend.  *)
HeldIffOpen ==
  \A k \in Keys :
    parentHeld[k] <=> phase[k] \in {"requested", "confirmed", "denied",
                                    "uncertain", "settled"}

(* Release out of uncertain happens only under a recorded R38 governance  *)
(* reconcile decision -- guessing is not a transition.                     *)
UncertainReleaseNeedsGovernance ==
  \A k \in Keys : relUncertain[k] => reconciled[k]

(* Nothing ever settles on a bridge that was never confirmed.             *)
NoChargeWithoutCommit ==
  \A k \in Keys : settleCount[k] > 0 => createCount[k] = 1

=============================================================================
\* Descriptor-model parity annotations (proof/check-descriptors.py).
\* @parity module: SubordinateReservation
\* @parity descriptor: subordinate-reservation.json
\* @parity state: requested
\* @parity state: confirmed
\* @parity state: denied
\* @parity state: uncertain
\* @parity state: settled
\* @parity state: released
\* @parity transition: absent -> requested via subordinate_reserve_request
\* @parity transition: requested -> requested via subordinate_reserve_request
\* @parity transition: requested -> confirmed via subordinate_reserved
\* @parity transition: requested -> denied via subordinate_denied
\* @parity transition: requested -> uncertain via subordinate_outcome_unknown
\* @parity transition: uncertain -> uncertain via subordinate_query_unknown
\* @parity transition: uncertain -> confirmed via subordinate_query_confirmed
\* @parity transition: uncertain -> denied via subordinate_query_denied
\* @parity transition: uncertain -> released via budget_reconcile
\* @parity transition: confirmed -> settled via subordinate_settle
\* @parity transition: confirmed -> released via subordinate_release
\* @parity transition: denied -> released via subordinate_release
\* @parity transition: settled -> released via subordinate_release
\* @parity crash: absent -> requested via subordinate_reserve_request = timeout queries; unknown remains uncertain (§14.8 ExternalBudgetBridge row)
\* @parity fences: absent -> requested via subordinate_reserve_request = (none)
\* @parity crash: requested -> requested via subordinate_reserve_request = timeout queries; unknown remains uncertain (§14.8 ExternalBudgetBridge row)
\* @parity fences: requested -> requested via subordinate_reserve_request = (none)
\* @parity crash: requested -> confirmed via subordinate_reserved = source ref/revision/digest persisted; Episode queues only if confirmed (§14.8 ExternalBudgetBridge row)
\* @parity fences: requested -> confirmed via subordinate_reserved = subordinate ref/revision/digest persisted on the bridge; the byom parent stays reserved (no parallel charge, no early release)
\* @parity crash: requested -> denied via subordinate_denied = timeout queries; unknown remains uncertain (§14.8 ExternalBudgetBridge row)
\* @parity fences: requested -> denied via subordinate_denied = releases only demonstrably unspent Byom reservations
\* @parity crash: requested -> uncertain via subordinate_outcome_unknown = unknown remains uncertain; the byom reservation is not released and spend stays blocked until the stable query or the R38 seat resolves it (§11.4, family contract L33)
\* @parity fences: requested -> uncertain via subordinate_outcome_unknown = the byom reservation is NOT released; spend stays blocked
\* @parity crash: uncertain -> uncertain via subordinate_query_unknown = conservative hold; nothing releases (§11.4)
\* @parity fences: uncertain -> uncertain via subordinate_query_unknown = (none)
\* @parity crash: uncertain -> confirmed via subordinate_query_confirmed = the recovery query surfaces Kovee's durable truth, never invents one (ResolutionIsReal; §11.4)
\* @parity fences: uncertain -> confirmed via subordinate_query_confirmed = bridge persists the recovered ref/revision/digest
\* @parity crash: uncertain -> denied via subordinate_query_denied = the recovery query surfaces Kovee's durable truth, never invents one (ResolutionIsReal; §11.4)
\* @parity fences: uncertain -> denied via subordinate_query_denied = released amount is exactly the demonstrably unspent byom reservation
\* @parity crash: uncertain -> released via budget_reconcile = unknown quantity never returns to remaining without the R38 decision (§14.8 BudgetReservationSet row; family contract L33)
\* @parity fences: uncertain -> released via budget_reconcile = release applied under account locks
\* @parity crash: confirmed -> settled via subordinate_settle = changed request id cannot double settle (§14.8 UsageSettlement row)
\* @parity fences: confirmed -> settled via subordinate_settle = settlement applied once on both sides; unknown or underivable cost keeps the reservation or settles to the conservative maximum
\* @parity crash: confirmed -> released via subordinate_release = unknown quantity never returns to remaining (§14.8 BudgetReservationSet row)
\* @parity fences: confirmed -> released via subordinate_release = releases only demonstrably unspent quantity; the parent bucket returns in the same accounting step
\* @parity crash: denied -> released via subordinate_release = unknown quantity never returns to remaining (§14.8 BudgetReservationSet row)
\* @parity fences: denied -> released via subordinate_release = (none)
\* @parity crash: settled -> released via subordinate_release = terminal: a released bridge never revives — new work is a fresh saga row under a fresh stable key (§11.4)
\* @parity fences: settled -> released via subordinate_release = released_lifetime is a monotonic audit counter, not an available bucket (§11.4)
