------------------------------- MODULE Pledge -------------------------------
(***************************************************************************)
(* Byom B0.1 Pledge lifecycle: the 13 post-proposal states of byom         *)
(* DESIGN.md section 9.3-9.5 / section 14.8 plus the folded proposal stage *)
(* (gap note G20), exactly as committed in spec/descriptors/pledge.json.   *)
(*                                                                         *)
(* Projection: one Pledge, its required seat set, and the one-successor    *)
(* amendment CAS slot.  Seat Position payloads, terms digests, budgets,    *)
(* Activity generations, and Delivery/Review bodies are abstracted; the    *)
(* pledgor/terms seat receipts are modeled as a set so finalization        *)
(* determinism ("authors no missing seat", R9) is a guard, not a promise.  *)
(* Every operation may be replayed at any time: guards make the exact      *)
(* retry a no-op, which is the descriptor-level idempotency claim.         *)
(* All variables are durable; a daemon crash between any two transitions   *)
(* is stuttering, so crash-honesty here is exactly the closed-transition   *)
(* property (nothing leaves a terminal state, nothing skips a stage).      *)
(***************************************************************************)
EXTENDS Naturals

CONSTANTS Seats,       \* required Pledge seats, e.g. {pledgor, terms}
          MaxResumes   \* bound on revision_requested -> underway generations

VARIABLES
  st,            \* pledge state: "absent" + the 14 descriptor states
  positions,     \* seats holding a separately attributable Position receipt
  successors,    \* history: accepted amendment successors (one CAS slot, G22)
  resumes,       \* history: pledge_resume generations taken
  firstTerminal  \* history: first terminal state entered, or "none"

vars == <<st, positions, successors, resumes, firstTerminal>>

States == {"absent", "proposed", "active", "waiting", "underway",
           "submitted", "revision_requested", "fulfilled", "rejected",
           "relinquished", "canceled", "failed", "superseded", "expired",
           "disputed"}

\* Section 14.8: fulfilled/rejected/relinquished/canceled/failed/superseded/
\* expired are terminal; revision_requested and disputed are not.
Terminal == {"fulfilled", "rejected", "relinquished", "canceled", "failed",
             "superseded", "expired"}

\* States carrying an open obligation: amend/relinquish/disposition/expiry
\* all fan out from exactly these six (descriptor rows).
Open == {"active", "waiting", "underway", "submitted",
         "revision_requested", "disputed"}

TypeOK ==
  /\ st \in States
  /\ positions \subseteq Seats
  /\ successors \in 0..1
  /\ resumes \in 0..MaxResumes
  /\ firstTerminal \in Terminal \cup {"none"}

Init ==
  /\ st = "absent"
  /\ positions = {}
  /\ successors = 0
  /\ resumes = 0
  /\ firstTerminal = "none"

\* Records the first terminal entry; a second entry would be a model bug
\* caught by TerminalIsFinal.
Enter(target) ==
  /\ st' = target
  /\ firstTerminal' = IF target \in Terminal /\ firstTerminal = "none"
                      THEN target ELSE firstTerminal

-----------------------------------------------------------------------------
(* pledge_propose (R5): new PledgeProposal; "proposed" is the folded       *)
(* pre-formation state (G20).                                              *)
Propose ==
  /\ st = "absent"
  /\ Enter("proposed")
  /\ UNCHANGED <<positions, successors, resumes>>

(* pledge_position (R7): a Participant fills exactly its own eligible seat *)
(* (section 9.3: no coordinator, model, or finalizer fills the pledgor     *)
(* seat).  Folded proposal-stage self-transition (G19).  Re-positioning    *)
(* the same seat is the seat-head CAS supersede - state-idempotent here.   *)
Position ==
  /\ st = "proposed"
  /\ \E s \in Seats : positions' = positions \cup {s}
  /\ Enter("proposed")
  /\ UNCHANGED <<successors, resumes>>

(* pledge_finalize (R9): deterministic - it authors NO missing seat, so it *)
(* is enabled only when every required slot already holds a receipt.       *)
Finalize ==
  /\ st = "proposed"
  /\ positions = Seats
  /\ \E target \in {"active", "waiting"} : Enter(target)
  /\ UNCHANGED <<positions, successors, resumes>>

(* activity_open cascade (R29): the PledgeWorkstream begins work.          *)
ActivityOpen ==
  /\ st \in {"active", "waiting"}
  /\ Enter("underway")
  /\ UNCHANGED <<positions, successors, resumes>>

(* delivery_submit cascade: pledgor-only Delivery (section 9.5).           *)
DeliverySubmit ==
  /\ st = "underway"
  /\ Enter("submitted")
  /\ UNCHANGED <<positions, successors, resumes>>

(* review_record cascade (R14): the immutable Review outcome drives the    *)
(* Pledge transition; no runtime or verifier authors acceptance.           *)
ReviewRecord ==
  /\ st = "submitted"
  /\ \E target \in {"fulfilled", "revision_requested", "rejected",
                    "disputed"} :
       Enter(target)
  /\ UNCHANGED <<positions, successors, resumes>>

(* pledge_resume (R14): each resume starts a new Activity generation under *)
(* unchanged terms.                                                        *)
Resume ==
  /\ st = "revision_requested"
  /\ resumes < MaxResumes
  /\ resumes' = resumes + 1
  /\ Enter("underway")
  /\ UNCHANGED <<positions, successors>>

(* pledge_amend (R5): ONE compare-and-swap successor slot (G22).  The CAS  *)
(* head is st itself: acceptance atomically supersedes the revision, so a  *)
(* competing amendment finds no Open state and fails.                      *)
Amend ==
  /\ st \in Open
  /\ successors' = successors + 1
  /\ Enter("superseded")
  /\ UNCHANGED <<positions, resumes>>

(* pledge_relinquish (R14): recorded as relinquishment, never rewritten as *)
(* success; reservations settle conservatively (section 9.5).              *)
Relinquish ==
  /\ st \in Open
  /\ Enter("relinquished")
  /\ UNCHANGED <<positions, successors, resumes>>

(* pledge_disposition_decision (named transition, G22): the remaining      *)
(* obligation becomes canceled or failed under its own procedure.          *)
Disposition ==
  /\ st \in Open
  /\ \E target \in {"canceled", "failed"} : Enter(target)
  /\ UNCHANGED <<positions, successors, resumes>>

(* server_time: deterministic expiry at the record's deadline; the         *)
(* descriptor has NO expiry row from proposed (proposal stage is folded).  *)
Expire ==
  /\ st \in Open
  /\ Enter("expired")
  /\ UNCHANGED <<positions, successors, resumes>>

Next ==
  \/ Propose \/ Position \/ Finalize \/ ActivityOpen \/ DeliverySubmit
  \/ ReviewRecord \/ Resume \/ Amend \/ Relinquish \/ Disposition \/ Expire

Spec == Init /\ [][Next]_vars

-----------------------------------------------------------------------------
(* Machine-checked invariants                                              *)

(* R9 determinism: nothing past the proposal stage exists without every    *)
(* required seat's separately attributable receipt - finalization authored *)
(* no missing seat, and no later transition invented one.                  *)
FinalizedHasAllSeats ==
  st \notin {"absent", "proposed"} => positions = Seats

(* G22: one current successor only - the amendment CAS can win at most     *)
(* once across every interleaving and replay.                              *)
AtMostOneSuccessor == successors <= 1

(* The successor slot is spent exactly when the Pledge is superseded.      *)
SupersededIffSuccessor == (st = "superseded") <=> (successors = 1)

(* Section 14.8 closed-machine rule: a terminal state is final.  Any       *)
(* future edit adding an exit from a terminal state fails this invariant.  *)
TerminalIsFinal == firstTerminal # "none" => st = firstTerminal

(* Resume generations stay within the modeled bound (finite check).       *)
ResumesBounded == resumes <= MaxResumes

=============================================================================
\* Descriptor-model parity annotations (proof/check-descriptors.py).
\* @parity module: Pledge
\* @parity descriptor: pledge.json
\* @parity state: proposed
\* @parity state: active
\* @parity state: waiting
\* @parity state: underway
\* @parity state: submitted
\* @parity state: revision_requested
\* @parity state: fulfilled
\* @parity state: rejected
\* @parity state: relinquished
\* @parity state: canceled
\* @parity state: failed
\* @parity state: superseded
\* @parity state: expired
\* @parity state: disputed
\* @parity transition: absent -> proposed via pledge_propose
\* @parity transition: proposed -> proposed via pledge_position
\* @parity transition: proposed -> active via pledge_finalize
\* @parity transition: proposed -> waiting via pledge_finalize
\* @parity transition: active -> underway via activity_open
\* @parity transition: waiting -> underway via activity_open
\* @parity transition: underway -> submitted via delivery_submit
\* @parity transition: submitted -> fulfilled via review_record
\* @parity transition: submitted -> revision_requested via review_record
\* @parity transition: submitted -> rejected via review_record
\* @parity transition: submitted -> disputed via review_record
\* @parity transition: revision_requested -> underway via pledge_resume
\* @parity transition: active -> superseded via pledge_amend
\* @parity transition: waiting -> superseded via pledge_amend
\* @parity transition: underway -> superseded via pledge_amend
\* @parity transition: submitted -> superseded via pledge_amend
\* @parity transition: revision_requested -> superseded via pledge_amend
\* @parity transition: disputed -> superseded via pledge_amend
\* @parity transition: active -> relinquished via pledge_relinquish
\* @parity transition: waiting -> relinquished via pledge_relinquish
\* @parity transition: underway -> relinquished via pledge_relinquish
\* @parity transition: submitted -> relinquished via pledge_relinquish
\* @parity transition: revision_requested -> relinquished via pledge_relinquish
\* @parity transition: disputed -> relinquished via pledge_relinquish
\* @parity transition: active -> canceled via pledge_disposition_decision
\* @parity transition: waiting -> canceled via pledge_disposition_decision
\* @parity transition: underway -> canceled via pledge_disposition_decision
\* @parity transition: submitted -> canceled via pledge_disposition_decision
\* @parity transition: revision_requested -> canceled via pledge_disposition_decision
\* @parity transition: disputed -> canceled via pledge_disposition_decision
\* @parity transition: active -> failed via pledge_disposition_decision
\* @parity transition: waiting -> failed via pledge_disposition_decision
\* @parity transition: underway -> failed via pledge_disposition_decision
\* @parity transition: submitted -> failed via pledge_disposition_decision
\* @parity transition: revision_requested -> failed via pledge_disposition_decision
\* @parity transition: disputed -> failed via pledge_disposition_decision
\* @parity transition: active -> expired via server_time
\* @parity transition: waiting -> expired via server_time
\* @parity transition: underway -> expired via server_time
\* @parity transition: submitted -> expired via server_time
\* @parity transition: revision_requested -> expired via server_time
\* @parity transition: disputed -> expired via server_time
