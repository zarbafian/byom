---------------------------- MODULE EpisodeLease ----------------------------
(***************************************************************************)
(* Byom B0.1 ActivityStream / Episode lease machine (byom DESIGN.md        *)
(* sections 11.1-11.2, 14.8), folding the three committed descriptors:     *)
(*   spec/descriptors/activity-stream.json (ActivityStream),               *)
(*   spec/descriptors/wake-intent.json (WakeIntent/ActivationAdmission/    *)
(*     ResourceAllocation, the named kernel transitions activation_admit   *)
(*     and resource_allocate), and                                         *)
(*   spec/descriptors/episode.json (Episode + folded EpisodeLeaseHead,     *)
(*     gap note G29).                                                      *)
(*                                                                         *)
(* Projection: one ActivityStream, one WakeIntent revision, one            *)
(* ActivationAdmission, one ResourceAllocation, one Episode, one           *)
(* EpisodeLeaseHead, and a set of workers competing for the lease.  The    *)
(* lease-head CAS is modeled exactly: every successful claim increments    *)
(* the Byom fence and mints an immutable attempt; a worker whose recorded  *)
(* fence is stale satisfies no guard, so it can neither start, yield,      *)
(* complete, nor fail the Episode (stale-claim rejection).  Worker crash/  *)
(* silence needs no explicit action: an expired head is re-claimable at    *)
(* any time (ReClaim), which is the crash story for workers.  Daemon state *)
(* is all durable in this projection, so a byomd crash is stuttering.      *)
(***************************************************************************)
EXTENDS Naturals

CONSTANTS Workers,   \* competing workload identities, e.g. {w1, w2}
          MaxFence   \* bound on Byom fence epochs (= claim attempts)

None == "none"

VARIABLES
  act,        \* ActivityStream state
  wi,         \* WakeIntent state
  adm,        \* ActivationAdmission state (kernel, activation_admit)
  alloc,      \* ResourceAllocation state (kernel, resource_allocate)
  ep,         \* Episode state
  lease,      \* EpisodeLeaseHead state (folded, G29)
  fence,      \* current Byom fence epoch on the one lease head
  holder,     \* worker holding the current lease, or "none"
  wfence,     \* [Workers -> 0..MaxFence]: fence each worker last claimed
  attempts,   \* history: immutable EpisodeAttempts ever created
  expiries,   \* history: authoritative lease expiries (D-RT-6)
  yields,     \* history: voluntary lease yields (D-RT-6)
  completedFence, \* history: fence under which completion committed (0 = none)
  everSubmitted, everAdmitted, everBridged  \* pipeline history flags

vars == <<act, wi, adm, alloc, ep, lease, fence, holder, wfence, attempts,
          expiries, yields, completedFence, everSubmitted, everAdmitted,
          everBridged>>

ActStates == {"absent", "ready", "active", "waiting", "reviewing", "held",
              "completed", "failed", "canceled"}
WiStates == {"absent", "submitted", "withdrawn", "expired"}
AdmStates == {"absent", "admission_admitted", "admission_denied",
              "admission_revoked", "admission_expired"}
AllocStates == {"absent", "allocation_prepared", "allocation_reserved",
                "allocation_bridged", "allocation_released",
                "allocation_uncertain", "allocation_revoked"}
EpStates == {"absent", "prepared", "eligible", "queued", "running",
             "yielded", "waiting", "completed", "interrupted", "failed",
             "canceled", "ambiguous"}
LeaseStates == {"absent", "lease_leased", "lease_expired",
                "lease_running", "lease_yielding", "lease_completing",
                "lease_terminal"}

TypeOK ==
  /\ act \in ActStates
  /\ wi \in WiStates
  /\ adm \in AdmStates
  /\ alloc \in AllocStates
  /\ ep \in EpStates
  /\ lease \in LeaseStates
  /\ fence \in 0..MaxFence
  /\ holder \in Workers \cup {None}
  /\ wfence \in [Workers -> 0..MaxFence]
  /\ attempts \in 0..MaxFence
  /\ expiries \in 0..MaxFence
  /\ yields \in 0..MaxFence
  /\ completedFence \in 0..MaxFence
  /\ everSubmitted \in BOOLEAN
  /\ everAdmitted \in BOOLEAN
  /\ everBridged \in BOOLEAN

Init ==
  /\ act = "absent" /\ wi = "absent" /\ adm = "absent" /\ alloc = "absent"
  /\ ep = "absent" /\ lease = "absent"
  /\ fence = 0 /\ holder = None /\ wfence = [w \in Workers |-> 0]
  /\ attempts = 0 /\ expiries = 0 /\ yields = 0 /\ completedFence = 0
  /\ everSubmitted = FALSE /\ everAdmitted = FALSE /\ everBridged = FALSE

\* The current lease holder with a current (non-stale) fence.
CurrentHolder(w) == holder = w /\ wfence[w] = fence

-----------------------------------------------------------------------------
(* -------------------------- ActivityStream ----------------------------- *)

(* activity_open (R29): the stream belongs to the Participant.             *)
ActivityOpen ==
  /\ act = "absent"
  /\ act' = "ready"
  /\ UNCHANGED <<wi, adm, alloc, ep, lease, fence, holder, wfence, attempts,
                 completedFence, everSubmitted, everAdmitted, everBridged, expiries, yields>>

(* activity_hold (R29): any nonterminal -> held; hold fences a running     *)
(* Episode (episode row running -> interrupted, one transaction).          *)
Hold ==
  /\ act \in {"ready", "active", "waiting", "reviewing"}
  /\ act' = "held"
  /\ ep' = IF ep = "running" THEN "interrupted" ELSE ep
  /\ UNCHANGED <<wi, adm, alloc, lease, fence, holder, wfence, attempts,
                 completedFence, everSubmitted, everAdmitted, everBridged, expiries, yields>>

(* activity_close (R29, target_state-discriminated, G25): cancel fences a  *)
(* running Episode (episode row running -> canceled); in-flight work       *)
(* settles conservatively.                                                 *)
Close ==
  /\ act \in {"ready", "active", "waiting", "reviewing", "held"}
  /\ \E target \in {"completed", "failed", "canceled"} : act' = target
  /\ ep' = IF ep = "running" THEN "canceled" ELSE ep
  /\ UNCHANGED <<wi, adm, alloc, lease, fence, holder, wfence, attempts,
                 completedFence, everSubmitted, everAdmitted, everBridged, expiries, yields>>

(* delivery_submit cascade: the PledgeWorkstream enters review.            *)
DeliverySubmit ==
  /\ act = "active"
  /\ act' = "reviewing"
  /\ UNCHANGED <<wi, adm, alloc, ep, lease, fence, holder, wfence, attempts,
                 completedFence, everSubmitted, everAdmitted, everBridged, expiries, yields>>

(* pledge_resume cascade: a new Activity generation under unchanged terms. *)
PledgeResume ==
  /\ act = "reviewing"
  /\ act' = "ready"
  /\ UNCHANGED <<wi, adm, alloc, ep, lease, fence, holder, wfence, attempts,
                 completedFence, everSubmitted, everAdmitted, everBridged, expiries, yields>>

(* ------------------ WakeIntent / admission / allocation ---------------- *)

(* wake_intent_submit (R29): authored only by the Participant channel - no *)
(* event, cron, host, attention ranking, or model score (section 11.1).    *)
WakeSubmit ==
  /\ wi = "absent"
  /\ act # "absent"
  /\ wi' = "submitted"
  /\ everSubmitted' = TRUE
  /\ UNCHANGED <<act, adm, alloc, ep, lease, fence, holder, wfence, attempts,
                 completedFence, everAdmitted, everBridged, expiries, yields>>

WakeWithdraw ==
  /\ wi = "submitted"
  /\ wi' = "withdrawn"
  /\ UNCHANGED <<act, adm, alloc, ep, lease, fence, holder, wfence, attempts,
                 completedFence, everSubmitted, everAdmitted, everBridged, expiries, yields>>

WakeExpire ==
  /\ wi = "submitted"
  /\ wi' = "expired"
  /\ UNCHANGED <<act, adm, alloc, ep, lease, fence, holder, wfence, attempts,
                 completedFence, everSubmitted, everAdmitted, everBridged, expiries, yields>>

(* activation_admit (kernel, non-callable): deterministic evaluation of a  *)
(* COMMITTED WakeIntent - the kernel may deny but cannot invent an         *)
(* interest; one decision per WakeIntent revision.                         *)
ActivationAdmit ==
  /\ adm = "absent"
  /\ wi = "submitted"
  /\ \E d \in {"admission_admitted", "admission_denied"} : adm' = d
  /\ everAdmitted' = (adm' = "admission_admitted")
  /\ UNCHANGED <<act, wi, alloc, ep, lease, fence, holder, wfence, attempts,
                 completedFence, everSubmitted, everBridged, expiries, yields>>

(* activation_policy_revoke cascade (R13): queued admissions fence on      *)
(* revoke without erasing executed Episodes.                               *)
AdmissionRevoke ==
  /\ adm = "admission_admitted"
  /\ adm' = "admission_revoked"
  /\ UNCHANGED <<act, wi, alloc, ep, lease, fence, holder, wfence, attempts,
                 completedFence, everSubmitted, everAdmitted, everBridged, expiries, yields>>

AdmissionExpire ==
  /\ adm = "admission_admitted"
  /\ adm' = "admission_expired"
  /\ UNCHANGED <<act, wi, alloc, ep, lease, fence, holder, wfence, attempts,
                 completedFence, everSubmitted, everAdmitted, everBridged, expiries, yields>>

(* resource_allocate (kernel, non-callable): can only reserve an ADMITTED  *)
(* WakeIntent (section 11.1); prepared -> reserved -> bridged, with the    *)
(* released/uncertain/revoked fan-out from any of the three.               *)
AllocPrepare ==
  /\ alloc = "absent"
  /\ adm = "admission_admitted"
  /\ alloc' = "allocation_prepared"
  /\ UNCHANGED <<act, wi, adm, ep, lease, fence, holder, wfence, attempts,
                 completedFence, everSubmitted, everAdmitted, everBridged, expiries, yields>>

AllocReserve ==
  /\ alloc = "allocation_prepared"
  /\ alloc' = "allocation_reserved"
  /\ UNCHANGED <<act, wi, adm, ep, lease, fence, holder, wfence, attempts,
                 completedFence, everSubmitted, everAdmitted, everBridged, expiries, yields>>

AllocBridge ==
  /\ alloc = "allocation_reserved"
  /\ alloc' = "allocation_bridged"
  /\ everBridged' = TRUE
  /\ UNCHANGED <<act, wi, adm, ep, lease, fence, holder, wfence, attempts,
                 completedFence, everSubmitted, everAdmitted, expiries, yields>>

(* An unknown Kovee bridge stays uncertain; releases/revocations settle    *)
(* conservatively (section 14.8).                                          *)
AllocSettle ==
  /\ alloc \in {"allocation_prepared", "allocation_reserved",
                "allocation_bridged"}
  /\ \E target \in {"allocation_released", "allocation_uncertain",
                    "allocation_revoked"} :
       alloc' = target
  /\ UNCHANGED <<act, wi, adm, ep, lease, fence, holder, wfence, attempts,
                 completedFence, everSubmitted, everAdmitted, everBridged, expiries, yields>>

(* ------------------------------ Episode -------------------------------- *)

(* episode_request (R29): creation (absent -> prepared, G27) opens the     *)
(* stream's generation (activity rows ready/waiting -> active).            *)
EpisodeCreate ==
  /\ ep = "absent"
  /\ act \in {"ready", "waiting"}
  /\ ep' = "prepared"
  /\ act' = "active"
  /\ UNCHANGED <<wi, adm, alloc, lease, fence, holder, wfence, attempts,
                 completedFence, everSubmitted, everAdmitted, everBridged, expiries, yields>>

(* episode_request: deterministic eligibility - no raw message or Kovee    *)
(* attention candidate starts an Episode (section 11.2).                   *)
EpisodeEligible ==
  /\ ep = "prepared"
  /\ ep' = "eligible"
  /\ UNCHANGED <<act, wi, adm, alloc, lease, fence, holder, wfence, attempts,
                 completedFence, everSubmitted, everAdmitted, everBridged, expiries, yields>>

(* resource_allocate: queue ONLY after both Byom and Kovee reservations    *)
(* (the bridged allocation); an uncertain bridge stays unqueued.           *)
Queue ==
  /\ ep = "eligible"
  /\ alloc = "allocation_bridged"
  /\ ep' = "queued"
  /\ UNCHANGED <<act, wi, adm, alloc, lease, fence, holder, wfence, attempts,
                 completedFence, everSubmitted, everAdmitted, everBridged, expiries, yields>>

(* episode_claim (R30): compare-and-swap on the ONE EpisodeLeaseHead -     *)
(* increments the Byom fence and creates an immutable EpisodeAttempt.      *)
Claim(w) ==
  /\ ep = "queued"
  /\ lease = "absent"
  /\ fence < MaxFence
  /\ lease' = "lease_leased"
  /\ holder' = w
  /\ fence' = fence + 1
  /\ wfence' = [wfence EXCEPT ![w] = fence + 1]
  /\ attempts' = attempts + 1
  /\ UNCHANGED <<act, wi, adm, alloc, ep, completedFence, everSubmitted,
                 everAdmitted, everBridged, expiries, yields>>

(* server_time (D-RT-6, RT-10): authoritative expiry - the server clock    *)
(* passes the lease deadline and moves the head to lease_expired.  This    *)
(* is the ONLY thing that makes a leased head re-claimable: worker crash   *)
(* or silence is stuttering and enables nothing.  Expiry never deletes     *)
(* the head or reuses a fence (section 11.2).                              *)
LeaseExpire ==
  /\ lease = "lease_leased"
  /\ expiries < MaxFence
  /\ expiries' = expiries + 1
  /\ lease' = "lease_expired"
  /\ UNCHANGED <<act, wi, adm, alloc, ep, fence, holder, wfence, attempts,
                 completedFence, everSubmitted, everAdmitted, everBridged,
                 yields>>

(* episode_claim: expired-head re-claim (lease_expired -> lease_leased)    *)
(* under a FRESH fence and a new immutable attempt - enabled ONLY from     *)
(* the authoritative lease_expired state (D-RT-6): a live leased head is   *)
(* not stealable.  The old worker is stale (section 11.2).                 *)
ReClaim(w) ==
  /\ lease = "lease_expired"
  /\ fence < MaxFence
  /\ lease' = "lease_leased"
  /\ holder' = w
  /\ fence' = fence + 1
  /\ wfence' = [wfence EXCEPT ![w] = fence + 1]
  /\ attempts' = attempts + 1
  /\ UNCHANGED <<act, wi, adm, alloc, ep, completedFence, everSubmitted,
                 everAdmitted, everBridged, expiries, yields>>

(* episode_claim: re-claim after yield (lease_yielding -> lease_leased);   *)
(* prior attempts remain historical.                                       *)
YieldReClaim(w) ==
  /\ lease = "lease_yielding"
  /\ fence < MaxFence
  /\ lease' = "lease_leased"
  /\ holder' = w
  /\ fence' = fence + 1
  /\ wfence' = [wfence EXCEPT ![w] = fence + 1]
  /\ attempts' = attempts + 1
  /\ UNCHANGED <<act, wi, adm, alloc, ep, completedFence, everSubmitted,
                 everAdmitted, everBridged, expiries, yields>>

(* episode_start (R30): only the current holder under the current fence.   *)
Start(w) ==
  /\ ep = "queued"
  /\ lease = "lease_leased"
  /\ CurrentHolder(w)
  /\ ep' = "running"
  /\ lease' = "lease_running"
  /\ UNCHANGED <<act, wi, adm, alloc, fence, holder, wfence, attempts,
                 completedFence, everSubmitted, everAdmitted, everBridged, expiries, yields>>

(* episode_yield (R30): yield to yielded, or to waiting on an admitted     *)
(* event/dependency; the stream returns to waiting (activity cascade).     *)
Yield(w) ==
  /\ ep = "running"
  /\ lease = "lease_running"
  /\ CurrentHolder(w)
  /\ yields < MaxFence
  /\ yields' = yields + 1
  /\ \E target \in {"yielded", "waiting"} : ep' = target
  /\ lease' = "lease_yielding"
  /\ act' = IF act = "active" THEN "waiting" ELSE act
  /\ UNCHANGED <<wi, adm, alloc, fence, holder, wfence, attempts,
                 completedFence, everSubmitted, everAdmitted, everBridged,
                 expiries>>

(* episode_complete (R30): completion is evidence only - Delivery remains  *)
(* separate and pledgor-authored; conservative settlement; the stream      *)
(* returns to ready (activity cascade).                                    *)
Complete(w) ==
  /\ ep = "running"
  /\ lease = "lease_running"
  /\ CurrentHolder(w)
  /\ ep' = "completed"
  /\ lease' = "lease_completing"
  /\ act' = IF act = "active" THEN "ready" ELSE act
  /\ completedFence' = fence
  /\ UNCHANGED <<wi, adm, alloc, fence, holder, wfence, attempts,
                 everSubmitted, everAdmitted, everBridged, expiries, yields>>

(* episode_complete: the completing head settles terminal.                 *)
CompleteSettle ==
  /\ lease = "lease_completing"
  /\ lease' = "lease_terminal"
  /\ UNCHANGED <<act, wi, adm, alloc, ep, fence, holder, wfence, attempts,
                 completedFence, everSubmitted, everAdmitted, everBridged, expiries, yields>>

(* episode_fail (R30): only the current holder under the current fence.    *)
Fail(w) ==
  /\ ep = "running"
  /\ lease = "lease_running"
  /\ CurrentHolder(w)
  /\ ep' = "failed"
  /\ lease' = "lease_terminal"
  /\ UNCHANGED <<act, wi, adm, alloc, fence, holder, wfence, attempts,
                 completedFence, everSubmitted, everAdmitted, everBridged, expiries, yields>>

(* server_time: lease/deadline expiry with unknown external use is         *)
(* ambiguous - never blindly repeated (section 14.8).                      *)
AmbiguousTimeout ==
  /\ ep = "running"
  /\ lease = "lease_running"
  /\ ep' = "ambiguous"
  /\ UNCHANGED <<act, wi, adm, alloc, lease, fence, holder, wfence, attempts,
                 completedFence, everSubmitted, everAdmitted, everBridged, expiries, yields>>

Next ==
  \/ ActivityOpen \/ Hold \/ Close \/ DeliverySubmit \/ PledgeResume
  \/ WakeSubmit \/ WakeWithdraw \/ WakeExpire
  \/ ActivationAdmit \/ AdmissionRevoke \/ AdmissionExpire
  \/ AllocPrepare \/ AllocReserve \/ AllocBridge \/ AllocSettle
  \/ EpisodeCreate \/ EpisodeEligible \/ Queue
  \/ CompleteSettle \/ AmbiguousTimeout \/ LeaseExpire
  \/ \E w \in Workers :
       \/ Claim(w) \/ ReClaim(w) \/ YieldReClaim(w)
       \/ Start(w) \/ Yield(w) \/ Complete(w) \/ Fail(w)

Spec == Init /\ [][Next]_vars

-----------------------------------------------------------------------------
(* Machine-checked invariants                                              *)

(* Section 11.1: no stage of the activation pipeline is skippable -        *)
(* admission needs a committed WakeIntent, allocation needs an admission,  *)
(* and nothing queues (or runs) without the bridged double reservation.    *)
PipelineNoSkip ==
  /\ adm # "absent" => everSubmitted
  /\ alloc # "absent" => everAdmitted
  \* every post-eligible Episode state (including interrupted/canceled,
  \* reachable only from running) sits behind the bridged reservation:
  /\ ep \notin {"absent", "prepared", "eligible"} => everBridged

(* Section 11.2 lease CAS: every claim minted exactly one fresh fence and  *)
(* one immutable attempt - a fence is never reused.                        *)
FencePerAttempt == attempts = fence

(* UNIQUE(episode_id, generation): no two workers ever share a fence.      *)
FenceUnique ==
  \A v, w \in Workers :
    (v # w /\ wfence[v] = wfence[w]) => wfence[v] = 0

(* A live (non-terminal) lease is held by a worker whose recorded fence IS *)
(* the current fence - there is no window in which a stale claim holds the *)
(* head.                                                                   *)
HolderIsCurrent ==
  lease \in {"lease_leased", "lease_running", "lease_yielding",
             "lease_completing"}
    => holder # None /\ wfence[holder] = fence

(* Stale-claim rejection: the completion that committed was made under the *)
(* CURRENT fence - a superseded worker's completion can never land.        *)
CompletionUnderCurrentFence ==
  ep = "completed" => completedFence = fence /\ completedFence > 0

(* Crash honesty: an ambiguous Episode (unknown external use) was never    *)
(* reported complete.                                                      *)
AmbiguousNeverCompleted == ep = "ambiguous" => completedFence = 0

(* The Episode runs only under a running lease head.                       *)
RunningHasRunningLease == ep = "running" => lease = "lease_running"

(* D-RT-6 (RT-10): crash alone never enables reclaim.  Every attempt       *)
(* beyond the first consumed either an authoritative-time expiry           *)
(* (LeaseExpire) or a voluntary yield - crash/stuttering mints nothing.    *)
ReclaimNeedsExpiryOrYield == attempts <= 1 + expiries + yields

(* An expired head still exists: expiry never deletes the head or its      *)
(* fence history (section 11.2).                                           *)
ExpiryKeepsHead ==
  lease = "lease_expired" => holder # None /\ fence > 0

=============================================================================
\* Descriptor-model parity annotations (proof/check-descriptors.py).
\* @parity module: EpisodeLease
\* @parity descriptor: episode.json
\* @parity state: prepared
\* @parity state: eligible
\* @parity state: queued
\* @parity state: running
\* @parity state: yielded
\* @parity state: completed
\* @parity state: waiting
\* @parity state: interrupted
\* @parity state: failed
\* @parity state: canceled
\* @parity state: ambiguous
\* @parity state: lease_leased
\* @parity state: lease_expired
\* @parity state: lease_running
\* @parity state: lease_yielding
\* @parity state: lease_completing
\* @parity state: lease_terminal
\* @parity transition: absent -> prepared via episode_request
\* @parity transition: prepared -> eligible via episode_request
\* @parity transition: eligible -> queued via resource_allocate
\* @parity transition: queued -> running via episode_start
\* @parity transition: running -> yielded via episode_yield
\* @parity transition: running -> waiting via episode_yield
\* @parity transition: running -> completed via episode_complete
\* @parity transition: running -> failed via episode_fail
\* @parity transition: running -> interrupted via activity_hold
\* @parity transition: running -> canceled via activity_close
\* @parity transition: running -> ambiguous via server_time
\* @parity transition: absent -> lease_leased via episode_claim
\* @parity transition: lease_leased -> lease_expired via server_time
\* @parity transition: lease_expired -> lease_leased via episode_claim
\* @parity transition: lease_leased -> lease_running via episode_start
\* @parity transition: lease_running -> lease_yielding via episode_yield
\* @parity transition: lease_yielding -> lease_leased via episode_claim
\* @parity transition: lease_running -> lease_completing via episode_complete
\* @parity transition: lease_completing -> lease_terminal via episode_complete
\* @parity transition: lease_running -> lease_terminal via episode_fail
\* @parity descriptor: activity-stream.json
\* @parity state: ready
\* @parity state: active
\* @parity state: waiting
\* @parity state: reviewing
\* @parity state: held
\* @parity state: completed
\* @parity state: failed
\* @parity state: canceled
\* @parity transition: absent -> ready via activity_open
\* @parity transition: ready -> active via episode_request
\* @parity transition: waiting -> active via episode_request
\* @parity transition: active -> ready via episode_complete
\* @parity transition: active -> waiting via episode_yield
\* @parity transition: active -> reviewing via delivery_submit
\* @parity transition: reviewing -> ready via pledge_resume
\* @parity transition: ready -> held via activity_hold
\* @parity transition: active -> held via activity_hold
\* @parity transition: waiting -> held via activity_hold
\* @parity transition: reviewing -> held via activity_hold
\* @parity transition: ready -> completed via activity_close
\* @parity transition: ready -> failed via activity_close
\* @parity transition: ready -> canceled via activity_close
\* @parity transition: active -> completed via activity_close
\* @parity transition: active -> failed via activity_close
\* @parity transition: active -> canceled via activity_close
\* @parity transition: waiting -> completed via activity_close
\* @parity transition: waiting -> failed via activity_close
\* @parity transition: waiting -> canceled via activity_close
\* @parity transition: reviewing -> completed via activity_close
\* @parity transition: reviewing -> failed via activity_close
\* @parity transition: reviewing -> canceled via activity_close
\* @parity transition: held -> completed via activity_close
\* @parity transition: held -> failed via activity_close
\* @parity transition: held -> canceled via activity_close
\* @parity descriptor: wake-intent.json
\* @parity state: submitted
\* @parity state: withdrawn
\* @parity state: expired
\* @parity state: admission_admitted
\* @parity state: admission_denied
\* @parity state: admission_revoked
\* @parity state: admission_expired
\* @parity state: allocation_prepared
\* @parity state: allocation_reserved
\* @parity state: allocation_bridged
\* @parity state: allocation_released
\* @parity state: allocation_uncertain
\* @parity state: allocation_revoked
\* @parity transition: absent -> submitted via wake_intent_submit
\* @parity transition: submitted -> withdrawn via wake_intent_withdraw
\* @parity transition: submitted -> expired via server_time
\* @parity transition: absent -> admission_admitted via activation_admit
\* @parity transition: absent -> admission_denied via activation_admit
\* @parity transition: admission_admitted -> admission_revoked via activation_policy_revoke
\* @parity transition: admission_admitted -> admission_expired via server_time
\* @parity transition: absent -> allocation_prepared via resource_allocate
\* @parity transition: allocation_prepared -> allocation_reserved via resource_allocate
\* @parity transition: allocation_reserved -> allocation_bridged via resource_allocate
\* @parity transition: allocation_prepared -> allocation_released via resource_allocate
\* @parity transition: allocation_prepared -> allocation_uncertain via resource_allocate
\* @parity transition: allocation_prepared -> allocation_revoked via resource_allocate
\* @parity transition: allocation_reserved -> allocation_released via resource_allocate
\* @parity transition: allocation_reserved -> allocation_uncertain via resource_allocate
\* @parity transition: allocation_reserved -> allocation_revoked via resource_allocate
\* @parity transition: allocation_bridged -> allocation_released via resource_allocate
\* @parity transition: allocation_bridged -> allocation_uncertain via resource_allocate
\* @parity transition: allocation_bridged -> allocation_revoked via resource_allocate
