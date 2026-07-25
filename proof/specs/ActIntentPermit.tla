--------------------------- MODULE ActIntentPermit ---------------------------
(***************************************************************************)
(* Byom B0.1 ActIntent / execution permit machine (byom DESIGN.md section  *)
(* 13.1, section 14.8), exactly as committed in                            *)
(* spec/descriptors/act-intent.json: server-prepared intent, seat          *)
(* positions, deterministic finalization, one-shot                         *)
(* execution_permit_consume, host attempt, and source-qualified effect     *)
(* outcome admission with ambiguous reconciliation.                        *)
(*                                                                         *)
(* Projection: one ActIntent, its required seat set, and a small set of    *)
(* idempotency keys competing to consume the one execution permit.  The    *)
(* PreparationTrace, subject digests, GovernanceDecision payloads, fences, *)
(* and receipts are abstracted; a receipt is a counter so replay can be    *)
(* proven to mint nothing.  ReplayConsume (same canonical request + key)   *)
(* and ConflictConsume (different key against the spent decision) are      *)
(* modeled explicitly as no-effect actions, so the one-shot invariants are *)
(* checked against live replay/conflict traffic, not by their absence.     *)
(* All variables are durable; a daemon crash between transitions is        *)
(* stuttering ("crash result: none or recoverable intent", section 14.8).  *)
(***************************************************************************)
EXTENDS Naturals

CONSTANTS Seats,   \* required decision seats, e.g. {participant, human}
          Keys     \* consumption idempotency keys, e.g. {k1, k2}

None == "none"

VARIABLES
  st,            \* intent state: "absent" + the 11 descriptor states
  seatsPos,      \* seats holding a Position on the exact intent digest
  spent,         \* the key that consumed the permit, or "none"
  consumptions,  \* history: MandateUse insertions (must stay <= 1)
  receipts,      \* history: ExecutionConsumptionReceipts minted
  replays,       \* history: exact-replay consumptions served (bounded)
  conflicts,     \* history: different-key consume attempts rejected (bounded)
  everAuthorized \* history: a finalize locked the full slot snapshot

vars == <<st, seatsPos, spent, consumptions, receipts, replays, conflicts,
          everAuthorized>>

States == {"absent", "prepared", "awaiting_decision", "authorized",
           "consumed", "executing", "succeeded", "failed", "ambiguous",
           "denied", "expired", "canceled"}

\* States from which finalize-deny / expiry / cancel fan out (descriptor).
Deciding == {"prepared", "awaiting_decision", "authorized"}
\* States after the permit was consumed.
PostConsume == {"consumed", "executing", "succeeded", "failed", "ambiguous"}

TypeOK ==
  /\ st \in States
  /\ seatsPos \subseteq Seats
  /\ spent \in Keys \cup {None}
  /\ consumptions \in 0..1
  /\ receipts \in 0..1
  /\ replays \in 0..1
  /\ conflicts \in 0..1
  /\ everAuthorized \in BOOLEAN

Init ==
  /\ st = "absent"
  /\ seatsPos = {}
  /\ spent = None
  /\ consumptions = 0
  /\ receipts = 0
  /\ replays = 0
  /\ conflicts = 0
  /\ everAuthorized = FALSE

-----------------------------------------------------------------------------
(* act_intent_prepare (R19): deterministic field-complete preparation from *)
(* authenticated typed input and server-owned state (section 13.1 step 1). *)
Prepare ==
  /\ st = "absent"
  /\ st' = "prepared"
  /\ UNCHANGED <<seatsPos, spent, consumptions, receipts, replays, conflicts,
                 everAuthorized>>

(* act_intent_position (R20/R21): first seat Position on the exact         *)
(* prepared intent digest.                                                 *)
PositionFirst ==
  /\ st = "prepared"
  /\ \E s \in Seats : seatsPos' = {s}
  /\ st' = "awaiting_decision"
  /\ UNCHANGED <<spent, consumptions, receipts, replays, conflicts,
                 everAuthorized>>

(* act_intent_position: remaining seats via seat-head CAS (G18/G19).       *)
PositionMore ==
  /\ st = "awaiting_decision"
  /\ \E s \in Seats : seatsPos' = seatsPos \cup {s}
  /\ st' = "awaiting_decision"
  /\ UNCHANGED <<spent, consumptions, receipts, replays, conflicts,
                 everAuthorized>>

(* act_intent_finalize (R22/R23): deterministic - locks the exact active   *)
(* slot snapshot and authors NO seat, so it is enabled only when every     *)
(* required seat already holds a Position.                                 *)
FinalizeAuthorize ==
  /\ st = "awaiting_decision"
  /\ seatsPos = Seats
  /\ st' = "authorized"
  /\ everAuthorized' = TRUE
  /\ UNCHANGED <<seatsPos, spent, consumptions, receipts, replays, conflicts>>

(* act_intent_finalize recording a deny outcome; an authorized-but-        *)
(* unconsumed intent may still be denied under changed dependencies, and   *)
(* the one-shot use slot is never consumable afterward.                    *)
FinalizeDeny ==
  /\ st \in Deciding
  /\ st' = "denied"
  /\ UNCHANGED <<seatsPos, spent, consumptions, receipts, replays, conflicts,
                 everAuthorized>>

(* server_time: terminal - replay does not execute.                        *)
Expire ==
  /\ st \in Deciding
  /\ st' = "expired"
  /\ UNCHANGED <<seatsPos, spent, consumptions, receipts, replays, conflicts,
                 everAuthorized>>

(* act_intent_cancel (R24/R25): cancellation cannot claim effect rollback; *)
(* terminal - replay does not execute.                                     *)
Cancel ==
  /\ st \in Deciding
  /\ st' = "canceled"
  /\ UNCHANGED <<seatsPos, spent, consumptions, receipts, replays, conflicts,
                 everAuthorized>>

(* execution_permit_consume (R34): ONE-SHOT (section 13.1 steps 4-6) -     *)
(* atomic recheck, inserts MandateUse once, returns one immutable          *)
(* ExecutionConsumptionReceipt (max_uses 1).                               *)
Consume ==
  /\ st = "authorized"
  /\ spent = None
  /\ \E k \in Keys :
       spent' = k
  /\ st' = "consumed"
  /\ consumptions' = consumptions + 1
  /\ receipts' = receipts + 1
  /\ UNCHANGED <<seatsPos, replays, conflicts, everAuthorized>>

(* Exact replay of the same canonical request and key returns the SAME     *)
(* retained receipt - no new consumption, no new receipt (G37).            *)
ReplayConsume ==
  /\ st \in PostConsume
  /\ spent # None
  /\ replays = 0
  /\ replays' = 1
  /\ UNCHANGED <<st, seatsPos, spent, consumptions, receipts, conflicts,
                 everAuthorized>>

(* A different key cannot consume the spent decision - conflict, not a     *)
(* second consumption (G37).                                               *)
ConflictConsume ==
  /\ spent # None
  /\ \E k \in Keys : k # spent
  /\ conflicts = 0
  /\ conflicts' = 1
  /\ UNCHANGED <<st, seatsPos, spent, consumptions, receipts, replays,
                 everAuthorized>>

(* host_effect_attempt (named transition, G36): the host stores the        *)
(* receipt, mints its local permit, and only then creates a driver         *)
(* attempt - never callable on BPP.                                        *)
HostAttempt ==
  /\ st = "consumed"
  /\ st' = "executing"
  /\ UNCHANGED <<seatsPos, spent, consumptions, receipts, replays, conflicts,
                 everAuthorized>>

(* effect_outcome_admit (R35): source-qualified admission once;            *)
(* if Byom cannot prove whether a non-idempotent driver acted, the state   *)
(* remains ambiguous - never blindly repeated.                             *)
OutcomeAdmit ==
  /\ st = "executing"
  /\ \E target \in {"succeeded", "failed", "ambiguous"} : st' = target
  /\ UNCHANGED <<seatsPos, spent, consumptions, receipts, replays, conflicts,
                 everAuthorized>>

(* effect_outcome_admit: source-authoritative reconciliation of an         *)
(* ambiguous effect - requires the independently committed, signed final   *)
(* host Effect/receipt successor; no GovernanceDecision is invented.       *)
Reconcile ==
  /\ st = "ambiguous"
  /\ \E target \in {"succeeded", "failed"} : st' = target
  /\ UNCHANGED <<seatsPos, spent, consumptions, receipts, replays, conflicts,
                 everAuthorized>>

Next ==
  \/ Prepare \/ PositionFirst \/ PositionMore
  \/ FinalizeAuthorize \/ FinalizeDeny \/ Expire \/ Cancel
  \/ Consume \/ ReplayConsume \/ ConflictConsume
  \/ HostAttempt \/ OutcomeAdmit \/ Reconcile

Spec == Init /\ [][Next]_vars

-----------------------------------------------------------------------------
(* Machine-checked invariants                                              *)

(* Section 13.1: the execution permit is consumed at most once, and every  *)
(* consumption minted exactly one immutable receipt - across replay and    *)
(* conflict traffic.                                                       *)
OneShotConsumption == consumptions <= 1 /\ receipts = consumptions

(* No consumption without full authorization: every post-consume state     *)
(* sits behind a finalize that locked the complete seat snapshot.          *)
ConsumeRequiresAuthorization ==
  st \in PostConsume =>
    everAuthorized /\ seatsPos = Seats /\ consumptions = 1 /\ spent # None

(* A denied, expired, or canceled intent never consumed the permit and     *)
(* never will (the use slot is spent by decision, not by execution).       *)
DecisionFencesConsumption ==
  st \in {"denied", "expired", "canceled"} =>
    consumptions = 0 /\ spent = None

(* Deterministic finalization authored no seat.                            *)
FinalizeAuthorsNoSeat == everAuthorized => seatsPos = Seats

(* The spent key is bound to the one consumption.                          *)
SpentBindsKey == (consumptions = 1) <=> (spent # None)

=============================================================================
\* Descriptor-model parity annotations (proof/check-descriptors.py).
\* @parity module: ActIntentPermit
\* @parity descriptor: act-intent.json
\* @parity state: prepared
\* @parity state: awaiting_decision
\* @parity state: authorized
\* @parity state: consumed
\* @parity state: executing
\* @parity state: succeeded
\* @parity state: failed
\* @parity state: ambiguous
\* @parity state: denied
\* @parity state: expired
\* @parity state: canceled
\* @parity transition: absent -> prepared via act_intent_prepare
\* @parity transition: prepared -> awaiting_decision via act_intent_position
\* @parity transition: awaiting_decision -> awaiting_decision via act_intent_position
\* @parity transition: awaiting_decision -> authorized via act_intent_finalize
\* @parity transition: prepared -> denied via act_intent_finalize
\* @parity transition: awaiting_decision -> denied via act_intent_finalize
\* @parity transition: authorized -> denied via act_intent_finalize
\* @parity transition: prepared -> expired via server_time
\* @parity transition: awaiting_decision -> expired via server_time
\* @parity transition: authorized -> expired via server_time
\* @parity transition: prepared -> canceled via act_intent_cancel
\* @parity transition: awaiting_decision -> canceled via act_intent_cancel
\* @parity transition: authorized -> canceled via act_intent_cancel
\* @parity transition: authorized -> consumed via execution_permit_consume
\* @parity transition: consumed -> executing via host_effect_attempt
\* @parity transition: executing -> succeeded via effect_outcome_admit
\* @parity transition: executing -> failed via effect_outcome_admit
\* @parity transition: executing -> ambiguous via effect_outcome_admit
\* @parity transition: ambiguous -> succeeded via effect_outcome_admit
\* @parity transition: ambiguous -> failed via effect_outcome_admit
\* @parity crash: absent -> prepared via act_intent_prepare = none or recoverable authorized intent (§14.8 ActIntent row 1)
\* @parity fences: absent -> prepared via act_intent_prepare = exact subject and one-shot use slot created
\* @parity crash: prepared -> awaiting_decision via act_intent_position = none or recoverable authorized intent (§14.8 ActIntent row 1); prior Position inputs remain (§14.8 Position/Decision row)
\* @parity fences: prepared -> awaiting_decision via act_intent_position = one current seat head (§14.8 Position/Decision row)
\* @parity crash: awaiting_decision -> awaiting_decision via act_intent_position = none or recoverable authorized intent (§14.8 ActIntent row 1); prior Position inputs remain (§14.8 Position/Decision row)
\* @parity fences: awaiting_decision -> awaiting_decision via act_intent_position = one current seat head (§14.8 Position/Decision row)
\* @parity crash: awaiting_decision -> authorized via act_intent_finalize = none or recoverable authorized intent (§14.8 ActIntent row 1)
\* @parity fences: awaiting_decision -> authorized via act_intent_finalize = one GovernanceDecision bound to the intent digest; exact subject and one-shot use slot locked
\* @parity crash: prepared -> denied via act_intent_finalize = terminal; replay does not execute (§14.8 ActIntent denial row)
\* @parity fences: prepared -> denied via act_intent_finalize = unspent reservations released only when unambiguous
\* @parity crash: awaiting_decision -> denied via act_intent_finalize = terminal; replay does not execute (§14.8 ActIntent denial row)
\* @parity fences: awaiting_decision -> denied via act_intent_finalize = unspent reservations released only when unambiguous
\* @parity crash: authorized -> denied via act_intent_finalize = terminal; replay does not execute (§14.8 ActIntent denial row)
\* @parity fences: authorized -> denied via act_intent_finalize = unspent reservations released only when unambiguous
\* @parity crash: prepared -> expired via server_time = terminal; replay does not execute (§14.8 ActIntent denial row)
\* @parity fences: prepared -> expired via server_time = unspent reservations released only when unambiguous
\* @parity crash: awaiting_decision -> expired via server_time = terminal; replay does not execute (§14.8 ActIntent denial row)
\* @parity fences: awaiting_decision -> expired via server_time = unspent reservations released only when unambiguous
\* @parity crash: authorized -> expired via server_time = terminal; replay does not execute (§14.8 ActIntent denial row)
\* @parity fences: authorized -> expired via server_time = unspent reservations released only when unambiguous
\* @parity crash: prepared -> canceled via act_intent_cancel = terminal; replay does not execute (§14.8 ActIntent denial row)
\* @parity fences: prepared -> canceled via act_intent_cancel = unspent reservations released only when unambiguous
\* @parity crash: awaiting_decision -> canceled via act_intent_cancel = terminal; replay does not execute (§14.8 ActIntent denial row)
\* @parity fences: awaiting_decision -> canceled via act_intent_cancel = unspent reservations released only when unambiguous
\* @parity crash: authorized -> canceled via act_intent_cancel = terminal; replay does not execute (§14.8 ActIntent denial row)
\* @parity fences: authorized -> canceled via act_intent_cancel = unspent reservations released only when unambiguous
\* @parity crash: authorized -> consumed via execution_permit_consume = never blindly repeats non-idempotent effect (§14.8 ActIntent row 2)
\* @parity fences: authorized -> consumed via execution_permit_consume = MandateUse inserted once; one immutable ExecutionConsumptionReceipt
\* @parity crash: consumed -> executing via host_effect_attempt = never blindly repeats non-idempotent effect (§14.8 ActIntent row 2)
\* @parity fences: consumed -> executing via host_effect_attempt = (none)
\* @parity crash: executing -> succeeded via effect_outcome_admit = never blindly repeats non-idempotent effect (§14.8 ActIntent row 2)
\* @parity fences: executing -> succeeded via effect_outcome_admit = source-qualified admission once; conservative settlement
\* @parity crash: executing -> failed via effect_outcome_admit = never blindly repeats non-idempotent effect (§14.8 ActIntent row 2)
\* @parity fences: executing -> failed via effect_outcome_admit = source-qualified admission once; conservative settlement
\* @parity crash: executing -> ambiguous via effect_outcome_admit = never blindly repeats non-idempotent effect (§14.8 ActIntent row 2)
\* @parity fences: executing -> ambiguous via effect_outcome_admit = source-qualified admission once; conservative settlement
\* @parity crash: ambiguous -> succeeded via effect_outcome_admit = remains ambiguous on stale/unknown/conflicting source; no GovernanceDecision invented and no disposition can block the source fact (§14.8 ActIntent source reconciliation row)
\* @parity fences: ambiguous -> succeeded via effect_outcome_admit = source EOA revision and Byom source-head CAS; conservative budget settles once; any active disposition head becomes source_advanced and late result use is quarantined
\* @parity crash: ambiguous -> failed via effect_outcome_admit = remains ambiguous on stale/unknown/conflicting source; no GovernanceDecision invented and no disposition can block the source fact (§14.8 ActIntent source reconciliation row)
\* @parity fences: ambiguous -> failed via effect_outcome_admit = source EOA revision and Byom source-head CAS; conservative budget settles once; any active disposition head becomes source_advanced and late result use is quarantined
