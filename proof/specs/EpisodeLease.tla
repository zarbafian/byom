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
          MaxFence,  \* bound on Byom fence epochs (= claim attempts)
          LeaseTTL,  \* lease time-to-live: deadline = claim time + LeaseTTL (RT-10)
          MaxTime    \* bound on the authoritative clock for the finite check (RT-10)

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
  everSubmitted, everAdmitted, everBridged,  \* pipeline history flags
  now,        \* authoritative server clock, a monotone natural (RT-10)
  deadline    \* current lease deadline, minted at claim: now + LeaseTTL (RT-10)

vars == <<act, wi, adm, alloc, ep, lease, fence, holder, wfence, attempts,
          expiries, yields, completedFence, everSubmitted, everAdmitted,
          everBridged, now, deadline>>

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
  /\ now \in 0..MaxTime
  /\ deadline \in 0..(MaxTime + LeaseTTL)

Init ==
  /\ act = "absent" /\ wi = "absent" /\ adm = "absent" /\ alloc = "absent"
  /\ ep = "absent" /\ lease = "absent"
  /\ fence = 0 /\ holder = None /\ wfence = [w \in Workers |-> 0]
  /\ attempts = 0 /\ expiries = 0 /\ yields = 0 /\ completedFence = 0
  /\ everSubmitted = FALSE /\ everAdmitted = FALSE /\ everBridged = FALSE
  /\ now = 0 /\ deadline = 0

\* The current lease holder with a current (non-stale) fence.
CurrentHolder(w) == holder = w /\ wfence[w] = fence

-----------------------------------------------------------------------------
(* -------------------------- ActivityStream ----------------------------- *)

(* activity_open (R29): the stream belongs to the Participant.             *)
ActivityOpen ==
  /\ act = "absent"
  /\ act' = "ready"
  /\ UNCHANGED <<wi, adm, alloc, ep, lease, fence, holder, wfence, attempts,
                 completedFence, everSubmitted, everAdmitted, everBridged, expiries, yields, now, deadline>>

(* activity_hold (R29): any nonterminal -> held; hold fences a running     *)
(* Episode (episode row running -> interrupted, one transaction).          *)
Hold ==
  /\ act \in {"ready", "active", "waiting", "reviewing"}
  /\ act' = "held"
  /\ ep' = IF ep = "running" THEN "interrupted" ELSE ep
  /\ UNCHANGED <<wi, adm, alloc, lease, fence, holder, wfence, attempts,
                 completedFence, everSubmitted, everAdmitted, everBridged, expiries, yields, now, deadline>>

(* activity_close (R29, target_state-discriminated, G25): cancel fences a  *)
(* running Episode (episode row running -> canceled); in-flight work       *)
(* settles conservatively.                                                 *)
Close ==
  /\ act \in {"ready", "active", "waiting", "reviewing", "held"}
  /\ \E target \in {"completed", "failed", "canceled"} : act' = target
  /\ ep' = IF ep = "running" THEN "canceled" ELSE ep
  /\ UNCHANGED <<wi, adm, alloc, lease, fence, holder, wfence, attempts,
                 completedFence, everSubmitted, everAdmitted, everBridged, expiries, yields, now, deadline>>

(* delivery_submit cascade: the PledgeWorkstream enters review.            *)
DeliverySubmit ==
  /\ act = "active"
  /\ act' = "reviewing"
  /\ UNCHANGED <<wi, adm, alloc, ep, lease, fence, holder, wfence, attempts,
                 completedFence, everSubmitted, everAdmitted, everBridged, expiries, yields, now, deadline>>

(* pledge_resume cascade: a new Activity generation under unchanged terms. *)
PledgeResume ==
  /\ act = "reviewing"
  /\ act' = "ready"
  /\ UNCHANGED <<wi, adm, alloc, ep, lease, fence, holder, wfence, attempts,
                 completedFence, everSubmitted, everAdmitted, everBridged, expiries, yields, now, deadline>>

(* ------------------ WakeIntent / admission / allocation ---------------- *)

(* wake_intent_submit (R29): authored only by the Participant channel - no *)
(* event, cron, host, attention ranking, or model score (section 11.1).    *)
WakeSubmit ==
  /\ wi = "absent"
  /\ act # "absent"
  /\ wi' = "submitted"
  /\ everSubmitted' = TRUE
  /\ UNCHANGED <<act, adm, alloc, ep, lease, fence, holder, wfence, attempts,
                 completedFence, everAdmitted, everBridged, expiries, yields, now, deadline>>

WakeWithdraw ==
  /\ wi = "submitted"
  /\ wi' = "withdrawn"
  /\ UNCHANGED <<act, adm, alloc, ep, lease, fence, holder, wfence, attempts,
                 completedFence, everSubmitted, everAdmitted, everBridged, expiries, yields, now, deadline>>

WakeExpire ==
  /\ wi = "submitted"
  /\ wi' = "expired"
  /\ UNCHANGED <<act, adm, alloc, ep, lease, fence, holder, wfence, attempts,
                 completedFence, everSubmitted, everAdmitted, everBridged, expiries, yields, now, deadline>>

(* activation_admit (kernel, non-callable): deterministic evaluation of a  *)
(* COMMITTED WakeIntent - the kernel may deny but cannot invent an         *)
(* interest; one decision per WakeIntent revision.                         *)
ActivationAdmit ==
  /\ adm = "absent"
  /\ wi = "submitted"
  /\ \E d \in {"admission_admitted", "admission_denied"} : adm' = d
  /\ everAdmitted' = (adm' = "admission_admitted")
  /\ UNCHANGED <<act, wi, alloc, ep, lease, fence, holder, wfence, attempts,
                 completedFence, everSubmitted, everBridged, expiries, yields, now, deadline>>

(* activation_policy_revoke cascade (R13): queued admissions fence on      *)
(* revoke without erasing executed Episodes.                               *)
AdmissionRevoke ==
  /\ adm = "admission_admitted"
  /\ adm' = "admission_revoked"
  /\ UNCHANGED <<act, wi, alloc, ep, lease, fence, holder, wfence, attempts,
                 completedFence, everSubmitted, everAdmitted, everBridged, expiries, yields, now, deadline>>

AdmissionExpire ==
  /\ adm = "admission_admitted"
  /\ adm' = "admission_expired"
  /\ UNCHANGED <<act, wi, alloc, ep, lease, fence, holder, wfence, attempts,
                 completedFence, everSubmitted, everAdmitted, everBridged, expiries, yields, now, deadline>>

(* resource_allocate (kernel, non-callable): can only reserve an ADMITTED  *)
(* WakeIntent (section 11.1); prepared -> reserved -> bridged, with the    *)
(* released/uncertain/revoked fan-out from any of the three.               *)
AllocPrepare ==
  /\ alloc = "absent"
  /\ adm = "admission_admitted"
  /\ alloc' = "allocation_prepared"
  /\ UNCHANGED <<act, wi, adm, ep, lease, fence, holder, wfence, attempts,
                 completedFence, everSubmitted, everAdmitted, everBridged, expiries, yields, now, deadline>>

AllocReserve ==
  /\ alloc = "allocation_prepared"
  /\ alloc' = "allocation_reserved"
  /\ UNCHANGED <<act, wi, adm, ep, lease, fence, holder, wfence, attempts,
                 completedFence, everSubmitted, everAdmitted, everBridged, expiries, yields, now, deadline>>

AllocBridge ==
  /\ alloc = "allocation_reserved"
  /\ alloc' = "allocation_bridged"
  /\ everBridged' = TRUE
  /\ UNCHANGED <<act, wi, adm, ep, lease, fence, holder, wfence, attempts,
                 completedFence, everSubmitted, everAdmitted, expiries, yields, now, deadline>>

(* An unknown Kovee bridge stays uncertain; releases/revocations settle    *)
(* conservatively (section 14.8).                                          *)
AllocSettle ==
  /\ alloc \in {"allocation_prepared", "allocation_reserved",
                "allocation_bridged"}
  /\ \E target \in {"allocation_released", "allocation_uncertain",
                    "allocation_revoked"} :
       alloc' = target
  /\ UNCHANGED <<act, wi, adm, ep, lease, fence, holder, wfence, attempts,
                 completedFence, everSubmitted, everAdmitted, everBridged, expiries, yields, now, deadline>>

(* ------------------------------ Episode -------------------------------- *)

(* episode_request (R29): creation (absent -> prepared, G27) opens the     *)
(* stream's generation (activity rows ready/waiting -> active).            *)
EpisodeCreate ==
  /\ ep = "absent"
  /\ act \in {"ready", "waiting"}
  /\ ep' = "prepared"
  /\ act' = "active"
  /\ UNCHANGED <<wi, adm, alloc, lease, fence, holder, wfence, attempts,
                 completedFence, everSubmitted, everAdmitted, everBridged, expiries, yields, now, deadline>>

(* episode_request: deterministic eligibility - no raw message or Kovee    *)
(* attention candidate starts an Episode (section 11.2).                   *)
EpisodeEligible ==
  /\ ep = "prepared"
  /\ ep' = "eligible"
  /\ UNCHANGED <<act, wi, adm, alloc, lease, fence, holder, wfence, attempts,
                 completedFence, everSubmitted, everAdmitted, everBridged, expiries, yields, now, deadline>>

(* resource_allocate: queue ONLY after both Byom and Kovee reservations    *)
(* (the bridged allocation); an uncertain bridge stays unqueued.           *)
Queue ==
  /\ ep = "eligible"
  /\ alloc = "allocation_bridged"
  /\ ep' = "queued"
  /\ UNCHANGED <<act, wi, adm, alloc, lease, fence, holder, wfence, attempts,
                 completedFence, everSubmitted, everAdmitted, everBridged, expiries, yields, now, deadline>>

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
  /\ deadline' = now + LeaseTTL
  /\ UNCHANGED <<act, wi, adm, alloc, ep, completedFence, everSubmitted,
                 everAdmitted, everBridged, expiries, yields, now>>

(* server_time (D-RT-6, RT-10): authoritative expiry - enabled ONLY when   *)
(* the server clock has strictly passed the lease deadline minted at       *)
(* claim (now > deadline), and moves the head to lease_expired.  This is   *)
(* the ONLY thing that makes a leased head re-claimable: worker crash or   *)
(* silence is stuttering and enables nothing, and without Tick steps the   *)
(* immediate claim -> expire -> reclaim trace is impossible.  Expiry       *)
(* never deletes the head or reuses a fence (section 11.2).                *)
LeaseExpire ==
  /\ lease = "lease_leased"
  /\ now > deadline
  /\ expiries < MaxFence
  /\ expiries' = expiries + 1
  /\ lease' = "lease_expired"
  /\ UNCHANGED <<act, wi, adm, alloc, ep, fence, holder, wfence, attempts,
                 completedFence, everSubmitted, everAdmitted, everBridged,
                 yields, now, deadline>>

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
  /\ deadline' = now + LeaseTTL
  /\ UNCHANGED <<act, wi, adm, alloc, ep, completedFence, everSubmitted,
                 everAdmitted, everBridged, expiries, yields, now>>

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
  /\ deadline' = now + LeaseTTL
  /\ UNCHANGED <<act, wi, adm, alloc, ep, completedFence, everSubmitted,
                 everAdmitted, everBridged, expiries, yields, now>>

(* episode_start (R30): only the current holder under the current fence.   *)
Start(w) ==
  /\ ep = "queued"
  /\ lease = "lease_leased"
  /\ CurrentHolder(w)
  /\ ep' = "running"
  /\ lease' = "lease_running"
  /\ UNCHANGED <<act, wi, adm, alloc, fence, holder, wfence, attempts,
                 completedFence, everSubmitted, everAdmitted, everBridged, expiries, yields, now, deadline>>

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
                 expiries, now, deadline>>

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
                 everSubmitted, everAdmitted, everBridged, expiries, yields, now, deadline>>

(* episode_complete: the completing head settles terminal.                 *)
CompleteSettle ==
  /\ lease = "lease_completing"
  /\ lease' = "lease_terminal"
  /\ UNCHANGED <<act, wi, adm, alloc, ep, fence, holder, wfence, attempts,
                 completedFence, everSubmitted, everAdmitted, everBridged, expiries, yields, now, deadline>>

(* episode_fail (R30): only the current holder under the current fence.    *)
Fail(w) ==
  /\ ep = "running"
  /\ lease = "lease_running"
  /\ CurrentHolder(w)
  /\ ep' = "failed"
  /\ lease' = "lease_terminal"
  /\ UNCHANGED <<act, wi, adm, alloc, fence, holder, wfence, attempts,
                 completedFence, everSubmitted, everAdmitted, everBridged, expiries, yields, now, deadline>>

(* server_time: lease/deadline expiry with unknown external use is         *)
(* ambiguous - never blindly repeated (section 14.8).                      *)
AmbiguousTimeout ==
  /\ ep = "running"
  /\ lease = "lease_running"
  /\ ep' = "ambiguous"
  /\ UNCHANGED <<act, wi, adm, alloc, lease, fence, holder, wfence, attempts,
                 completedFence, everSubmitted, everAdmitted, everBridged, expiries, yields, now, deadline>>

(* server_time (RT-10): the authoritative clock advances one unit.  Time   *)
(* is bounded by MaxTime for the finite check; nothing else moves.  Tick   *)
(* is what stands between a claim and its expiry: LeaseExpire demands      *)
(* now > deadline = claim time + LeaseTTL, so at least LeaseTTL + 1 Tick   *)
(* steps separate every claim from the expiry that supersedes it.          *)
Tick ==
  /\ now < MaxTime
  /\ now' = now + 1
  /\ UNCHANGED <<act, wi, adm, alloc, ep, lease, fence, holder, wfence,
                 attempts, expiries, yields, completedFence, everSubmitted,
                 everAdmitted, everBridged, deadline>>

Next ==
  \/ ActivityOpen \/ Hold \/ Close \/ DeliverySubmit \/ PledgeResume
  \/ WakeSubmit \/ WakeWithdraw \/ WakeExpire
  \/ ActivationAdmit \/ AdmissionRevoke \/ AdmissionExpire
  \/ AllocPrepare \/ AllocReserve \/ AllocBridge \/ AllocSettle
  \/ EpisodeCreate \/ EpisodeEligible \/ Queue
  \/ CompleteSettle \/ AmbiguousTimeout \/ LeaseExpire \/ Tick
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

(* RT-10: expiry is an authoritative-CLOCK fact.  A head is expired only   *)
(* strictly after the deadline minted at its claim - if LeaseExpire ever   *)
(* fired without the clock guard, an expired head with now <= deadline     *)
(* would be reachable and TLC would report it here.                        *)
NoPrematureExpiry == lease = "lease_expired" => now > deadline

(* RT-10: expiry consumes real time.  Each of the n expiries waited out a  *)
(* full deadline (claim time + LeaseTTL, strictly passed), so the clock    *)
(* has strictly passed n * LeaseTTL - the immediate                        *)
(* claim -> expire -> reclaim trace with no Tick steps is impossible.      *)
ExpiryConsumesTime == expiries = 0 \/ now > expiries * LeaseTTL

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
\* @parity crash: absent -> prepared via episode_request = uncertain bridge stays unqueued/held (§14.8 Episode request row; creation derived, gap note G27)
\* @parity fences: absent -> prepared via episode_request = (none)
\* @parity crash: prepared -> eligible via episode_request = uncertain bridge stays unqueued/held (§14.8 Episode request row)
\* @parity fences: prepared -> eligible via episode_request = (none)
\* @parity crash: eligible -> queued via resource_allocate = uncertain bridge stays unqueued/held (§14.8 Episode request row)
\* @parity fences: eligible -> queued via resource_allocate = (none)
\* @parity crash: queued -> running via episode_start = old worker is stale (§14.8 Episode lease row)
\* @parity fences: queued -> running via episode_start = (none)
\* @parity crash: running -> yielded via episode_yield = unknown external use is ambiguous (§14.8 Episode running row)
\* @parity fences: running -> yielded via episode_yield = EpisodeCompletion/event and conservative settlement; Delivery remains separate
\* @parity crash: running -> waiting via episode_yield = unknown external use is ambiguous (§14.8 Episode running row)
\* @parity fences: running -> waiting via episode_yield = EpisodeCompletion/event and conservative settlement; Delivery remains separate
\* @parity crash: running -> completed via episode_complete = unknown external use is ambiguous (§14.8 Episode running row)
\* @parity fences: running -> completed via episode_complete = EpisodeCompletion/event and conservative settlement; Delivery remains separate
\* @parity crash: running -> failed via episode_fail = unknown external use is ambiguous (§14.8 Episode running row)
\* @parity fences: running -> failed via episode_fail = EpisodeCompletion/event and conservative settlement; Delivery remains separate
\* @parity crash: running -> interrupted via activity_hold = unknown external use is ambiguous (§14.8 Episode running row)
\* @parity fences: running -> interrupted via activity_hold = hold fences the running Episode
\* @parity crash: running -> canceled via activity_close = unknown external use is ambiguous (§14.8 Episode running row)
\* @parity fences: running -> canceled via activity_close = fence advance revokes permits; settlement is conservative
\* @parity crash: running -> ambiguous via server_time = unknown external use is ambiguous (§14.8 Episode running row)
\* @parity fences: running -> ambiguous via server_time = conservative settlement
\* @parity crash: absent -> lease_leased via episode_claim = old worker is stale (§14.8 Episode lease row)
\* @parity fences: absent -> lease_leased via episode_claim = claim increments Byom fence and appends immutable attempt; CAS head
\* @parity crash: lease_leased -> lease_expired via server_time = old worker is stale; lease expiry never deletes the head or reuses a fence (§14.8 Episode lease row, §11.2)
\* @parity fences: lease_leased -> lease_expired via server_time = expiry never deletes the head or reuses a fence (§11.2)
\* @parity crash: lease_expired -> lease_leased via episode_claim = old worker is stale (§14.8 Episode lease row)
\* @parity fences: lease_expired -> lease_leased via episode_claim = claim increments Byom fence and appends new immutable attempt; the old worker is stale and cannot submit a Delivery, consume a mandate, create child work, append a continuation, or settle usage (§11.2)
\* @parity crash: lease_leased -> lease_running via episode_start = old worker is stale (§14.8 Episode lease row)
\* @parity fences: lease_leased -> lease_running via episode_start = head update with immutable EpisodeAttemptEvent
\* @parity crash: lease_running -> lease_yielding via episode_yield = old worker is stale (§14.8 Episode lease row)
\* @parity fences: lease_running -> lease_yielding via episode_yield = head update with immutable EpisodeAttemptEvent
\* @parity crash: lease_yielding -> lease_leased via episode_claim = old worker is stale (§14.8 Episode lease row)
\* @parity fences: lease_yielding -> lease_leased via episode_claim = claim increments Byom fence and appends new immutable attempt; CAS head
\* @parity crash: lease_running -> lease_completing via episode_complete = old worker is stale (§14.8 Episode lease row)
\* @parity fences: lease_running -> lease_completing via episode_complete = head update with immutable EpisodeAttemptEvent
\* @parity crash: lease_completing -> lease_terminal via episode_complete = old worker is stale (§14.8 Episode lease row)
\* @parity fences: lease_completing -> lease_terminal via episode_complete = head update with immutable EpisodeAttemptEvent
\* @parity crash: lease_running -> lease_terminal via episode_fail = old worker is stale (§14.8 Episode lease row)
\* @parity fences: lease_running -> lease_terminal via episode_fail = head update with immutable EpisodeAttemptEvent
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
\* @parity crash: absent -> ready via activity_open = prior outputs remain evidence (§14.8 ActivityStream row)
\* @parity fences: absent -> ready via activity_open = generation CAS
\* @parity crash: ready -> active via episode_request = prior outputs remain evidence (§14.8 ActivityStream row)
\* @parity fences: ready -> active via episode_request = generation CAS
\* @parity crash: waiting -> active via episode_request = prior outputs remain evidence (§14.8 ActivityStream row)
\* @parity fences: waiting -> active via episode_request = generation CAS
\* @parity crash: active -> ready via episode_complete = prior outputs remain evidence (§14.8 ActivityStream row)
\* @parity fences: active -> ready via episode_complete = generation CAS
\* @parity crash: active -> waiting via episode_yield = prior outputs remain evidence (§14.8 ActivityStream row)
\* @parity fences: active -> waiting via episode_yield = generation CAS
\* @parity crash: active -> reviewing via delivery_submit = prior outputs remain evidence (§14.8 ActivityStream row)
\* @parity fences: active -> reviewing via delivery_submit = generation CAS
\* @parity crash: reviewing -> ready via pledge_resume = prior outputs remain evidence (§14.8 ActivityStream row)
\* @parity fences: reviewing -> ready via pledge_resume = new Activity generation under unchanged terms; generation CAS
\* @parity crash: ready -> held via activity_hold = prior outputs remain evidence (§14.8 ActivityStream row)
\* @parity fences: ready -> held via activity_hold = hold fences new Episodes; generation CAS
\* @parity crash: active -> held via activity_hold = prior outputs remain evidence (§14.8 ActivityStream row)
\* @parity fences: active -> held via activity_hold = hold fences new Episodes; generation CAS
\* @parity crash: waiting -> held via activity_hold = prior outputs remain evidence (§14.8 ActivityStream row)
\* @parity fences: waiting -> held via activity_hold = hold fences new Episodes; generation CAS
\* @parity crash: reviewing -> held via activity_hold = prior outputs remain evidence (§14.8 ActivityStream row)
\* @parity fences: reviewing -> held via activity_hold = hold fences new Episodes; generation CAS
\* @parity crash: ready -> completed via activity_close = prior outputs remain evidence (§14.8 ActivityStream row)
\* @parity fences: ready -> completed via activity_close = cancel fences new Episodes; in-flight work settles conservatively; generation CAS
\* @parity crash: ready -> failed via activity_close = prior outputs remain evidence (§14.8 ActivityStream row)
\* @parity fences: ready -> failed via activity_close = cancel fences new Episodes; in-flight work settles conservatively; generation CAS
\* @parity crash: ready -> canceled via activity_close = prior outputs remain evidence (§14.8 ActivityStream row)
\* @parity fences: ready -> canceled via activity_close = cancel fences new Episodes; in-flight work settles conservatively; generation CAS
\* @parity crash: active -> completed via activity_close = prior outputs remain evidence (§14.8 ActivityStream row)
\* @parity fences: active -> completed via activity_close = cancel fences new Episodes; in-flight work settles conservatively; generation CAS
\* @parity crash: active -> failed via activity_close = prior outputs remain evidence (§14.8 ActivityStream row)
\* @parity fences: active -> failed via activity_close = cancel fences new Episodes; in-flight work settles conservatively; generation CAS
\* @parity crash: active -> canceled via activity_close = prior outputs remain evidence (§14.8 ActivityStream row)
\* @parity fences: active -> canceled via activity_close = cancel fences new Episodes; in-flight work settles conservatively; generation CAS
\* @parity crash: waiting -> completed via activity_close = prior outputs remain evidence (§14.8 ActivityStream row)
\* @parity fences: waiting -> completed via activity_close = cancel fences new Episodes; in-flight work settles conservatively; generation CAS
\* @parity crash: waiting -> failed via activity_close = prior outputs remain evidence (§14.8 ActivityStream row)
\* @parity fences: waiting -> failed via activity_close = cancel fences new Episodes; in-flight work settles conservatively; generation CAS
\* @parity crash: waiting -> canceled via activity_close = prior outputs remain evidence (§14.8 ActivityStream row)
\* @parity fences: waiting -> canceled via activity_close = cancel fences new Episodes; in-flight work settles conservatively; generation CAS
\* @parity crash: reviewing -> completed via activity_close = prior outputs remain evidence (§14.8 ActivityStream row)
\* @parity fences: reviewing -> completed via activity_close = cancel fences new Episodes; in-flight work settles conservatively; generation CAS
\* @parity crash: reviewing -> failed via activity_close = prior outputs remain evidence (§14.8 ActivityStream row)
\* @parity fences: reviewing -> failed via activity_close = cancel fences new Episodes; in-flight work settles conservatively; generation CAS
\* @parity crash: reviewing -> canceled via activity_close = prior outputs remain evidence (§14.8 ActivityStream row)
\* @parity fences: reviewing -> canceled via activity_close = cancel fences new Episodes; in-flight work settles conservatively; generation CAS
\* @parity crash: held -> completed via activity_close = prior outputs remain evidence (§14.8 ActivityStream row)
\* @parity fences: held -> completed via activity_close = cancel fences new Episodes; in-flight work settles conservatively; generation CAS
\* @parity crash: held -> failed via activity_close = prior outputs remain evidence (§14.8 ActivityStream row)
\* @parity fences: held -> failed via activity_close = cancel fences new Episodes; in-flight work settles conservatively; generation CAS
\* @parity crash: held -> canceled via activity_close = prior outputs remain evidence (§14.8 ActivityStream row)
\* @parity fences: held -> canceled via activity_close = cancel fences new Episodes; in-flight work settles conservatively; generation CAS
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
\* @parity crash: absent -> submitted via wake_intent_submit = no event/cron/host can author it (§14.8 WakeIntent row)
\* @parity fences: absent -> submitted via wake_intent_submit = immutable cause and provenance
\* @parity crash: submitted -> withdrawn via wake_intent_withdraw = no event/cron/host can author it (§14.8 WakeIntent row)
\* @parity fences: submitted -> withdrawn via wake_intent_withdraw = (none)
\* @parity crash: submitted -> expired via server_time = no event/cron/host can author it (§14.8 WakeIntent row)
\* @parity fences: submitted -> expired via server_time = (none)
\* @parity crash: absent -> admission_admitted via activation_admit = retry returns same admission (§14.8 ActivationAdmission row)
\* @parity fences: absent -> admission_admitted via activation_admit = no budget/placement yet
\* @parity crash: absent -> admission_denied via activation_admit = retry returns same admission (§14.8 ActivationAdmission row)
\* @parity fences: absent -> admission_denied via activation_admit = no budget/placement yet
\* @parity crash: admission_admitted -> admission_revoked via activation_policy_revoke = retry returns same admission (§14.8 ActivationAdmission row)
\* @parity fences: admission_admitted -> admission_revoked via activation_policy_revoke = queued admissions fence on revoke; queued work fenced without erasing executed Episodes
\* @parity crash: admission_admitted -> admission_expired via server_time = retry returns same admission (§14.8 ActivationAdmission row)
\* @parity fences: admission_admitted -> admission_expired via server_time = (none)
\* @parity crash: absent -> allocation_prepared via resource_allocate = unknown bridge remains uncertain (§14.8 ResourceAllocation row)
\* @parity fences: absent -> allocation_prepared via resource_allocate = (none)
\* @parity crash: allocation_prepared -> allocation_reserved via resource_allocate = unknown bridge remains uncertain (§14.8 ResourceAllocation row)
\* @parity fences: allocation_prepared -> allocation_reserved via resource_allocate = exact reservations
\* @parity crash: allocation_reserved -> allocation_bridged via resource_allocate = unknown bridge remains uncertain (§14.8 ResourceAllocation row)
\* @parity fences: allocation_reserved -> allocation_bridged via resource_allocate = bridge refs; queue remains blocked until bridged
\* @parity crash: allocation_prepared -> allocation_released via resource_allocate = unknown bridge remains uncertain (§14.8 ResourceAllocation row)
\* @parity fences: allocation_prepared -> allocation_released via resource_allocate = reservations and bridge refs released
\* @parity crash: allocation_prepared -> allocation_uncertain via resource_allocate = unknown bridge remains uncertain (§14.8 ResourceAllocation row)
\* @parity fences: allocation_prepared -> allocation_uncertain via resource_allocate = (none)
\* @parity crash: allocation_prepared -> allocation_revoked via resource_allocate = unknown bridge remains uncertain (§14.8 ResourceAllocation row)
\* @parity fences: allocation_prepared -> allocation_revoked via resource_allocate = reservations and bridge refs revoked
\* @parity crash: allocation_reserved -> allocation_released via resource_allocate = unknown bridge remains uncertain (§14.8 ResourceAllocation row)
\* @parity fences: allocation_reserved -> allocation_released via resource_allocate = reservations and bridge refs released
\* @parity crash: allocation_reserved -> allocation_uncertain via resource_allocate = unknown bridge remains uncertain (§14.8 ResourceAllocation row)
\* @parity fences: allocation_reserved -> allocation_uncertain via resource_allocate = (none)
\* @parity crash: allocation_reserved -> allocation_revoked via resource_allocate = unknown bridge remains uncertain (§14.8 ResourceAllocation row)
\* @parity fences: allocation_reserved -> allocation_revoked via resource_allocate = reservations and bridge refs revoked
\* @parity crash: allocation_bridged -> allocation_released via resource_allocate = unknown bridge remains uncertain (§14.8 ResourceAllocation row)
\* @parity fences: allocation_bridged -> allocation_released via resource_allocate = reservations and bridge refs released
\* @parity crash: allocation_bridged -> allocation_uncertain via resource_allocate = unknown bridge remains uncertain (§14.8 ResourceAllocation row)
\* @parity fences: allocation_bridged -> allocation_uncertain via resource_allocate = (none)
\* @parity crash: allocation_bridged -> allocation_revoked via resource_allocate = unknown bridge remains uncertain (§14.8 ResourceAllocation row)
\* @parity fences: allocation_bridged -> allocation_revoked via resource_allocate = reservations and bridge refs revoked
