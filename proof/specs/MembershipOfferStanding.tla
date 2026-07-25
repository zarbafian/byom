--------------------- MODULE MembershipOfferStanding -----------------------
(***************************************************************************)
(* Byom B0.1 MembershipOffer/Standing plus the coupled                     *)
(* OnboardingActivationOffer (byom DESIGN.md section 7.4, section 14.8),   *)
(* exactly as committed in spec/descriptors/membership-offer-standing.json *)
(* and spec/descriptors/onboarding-activation-offer.json.                  *)
(*                                                                         *)
(* Projection: one offer, one candidate, one onboarding offer, one         *)
(* Standing.  Offer revisions, subject digests, decisions, channels, and   *)
(* fences are abstracted to the state enums; the admit/refuse/revoke/      *)
(* expiry race on "the same offer revision" (section 7.4's CAS) is         *)
(* faithful because all four guard on the same state variable.  Replays    *)
(* are free: every guard makes the exact retry a no-op, which is the       *)
(* "exact retry returns the same receipt" claim at descriptor level.       *)
(* All variables are durable; daemon crash is stuttering.                  *)
(***************************************************************************)
EXTENDS Naturals

VARIABLES
  offer,           \* MembershipOffer: absent/offered/onboarding/accepted/
                   \*   admitted/refused/revoked/expired
  onb,             \* OnboardingActivationOffer: absent/offered/active/
                   \*   completed/refused/revoked/expired
  standing,        \* Standing: absent/standing_active/standing_suspended/
                   \*   standing_ceased/standing_expired/standing_superseded
  everAccepted,    \* history: the candidate itself authored an acceptance
  standingCreated, \* history: StandingRevisions ever created
  computeUses      \* history: one-shot onboarding compute permit uses

vars == <<offer, onb, standing, everAccepted, standingCreated, computeUses>>

OfferStates == {"absent", "offered", "onboarding", "accepted", "admitted",
                "refused", "revoked", "expired"}
OnbStates == {"absent", "offered", "active", "completed", "refused",
              "revoked", "expired"}
StandingStates == {"absent", "standing_active", "standing_suspended",
                   "standing_ceased", "standing_expired",
                   "standing_superseded"}

\* Offer states still holding an open (non-decided) offer revision: the
\* refuse/revoke/expire/admit CAS fans out from these.
OfferOpen == {"offered", "onboarding", "accepted"}
\* Onboarding-offer states the refusal/revocation cascade fences.
OnbOpen == {"offered", "active", "completed"}

TypeOK ==
  /\ offer \in OfferStates
  /\ onb \in OnbStates
  /\ standing \in StandingStates
  /\ everAccepted \in BOOLEAN
  /\ standingCreated \in 0..1
  /\ computeUses \in 0..1

Init ==
  /\ offer = "absent"
  /\ onb = "absent"
  /\ standing = "absent"
  /\ everAccepted = FALSE
  /\ standingCreated = 0
  /\ computeUses = 0

-----------------------------------------------------------------------------
(* membership_offer (R10): a decided MembershipOffer.                      *)
Offer ==
  /\ offer = "absent"
  /\ offer' = "offered"
  /\ UNCHANGED <<onb, standing, everAccepted, standingCreated, computeUses>>

(* onboarding_offer (R10): the same governance decision creates the        *)
(* OnboardingActivationOffer (owning row) and moves the MembershipOffer to *)
(* onboarding (cascade row) - one atomic transaction.                      *)
OnboardingOffer ==
  /\ offer = "offered"
  /\ onb = "absent"
  /\ offer' = "onboarding"
  /\ onb' = "offered"
  /\ UNCHANGED <<standing, everAccepted, standingCreated, computeUses>>

(* membership_accept (R11): ONLY the candidate authors acceptance, and     *)
(* acceptance is not Standing (section 7.4).                               *)
Accept ==
  /\ offer \in {"offered", "onboarding"}
  /\ offer' = "accepted"
  /\ everAccepted' = TRUE
  /\ UNCHANGED <<onb, standing, standingCreated, computeUses>>

(* participant_admit (R8): deterministic finalization - admits the exact   *)
(* current acceptance and creates the active StandingRevision atomically   *)
(* (two descriptor rows, one transaction).  The guard offer = "accepted"   *)
(* is the revision CAS: refusal, revocation, or expiry that won first      *)
(* leaves nothing to admit.                                                *)
Admit ==
  /\ offer = "accepted"
  /\ standing = "absent"
  /\ offer' = "admitted"
  /\ standing' = "standing_active"
  /\ standingCreated' = standingCreated + 1
  /\ UNCHANGED <<onb, everAccepted, computeUses>>

(* membership_refuse (R11): one revision-CAS transaction - supersedes any  *)
(* prior acceptance, advances the onboarding fence, closes the candidate   *)
(* channel (owning rows) and fences the OnboardingActivationOffer          *)
(* (cascade rows).                                                         *)
Refuse ==
  /\ offer \in OfferOpen
  /\ offer' = "refused"
  /\ onb' = IF onb \in OnbOpen THEN "refused" ELSE onb
  /\ UNCHANGED <<standing, everAccepted, standingCreated, computeUses>>

(* membership_offer_revoke (R10): same fencing as refusal without          *)
(* attributing a refusal to the candidate.                                 *)
Revoke ==
  /\ offer \in OfferOpen
  /\ offer' = "revoked"
  /\ onb' = IF onb \in OnbOpen THEN "revoked" ELSE onb
  /\ UNCHANGED <<standing, everAccepted, standingCreated, computeUses>>

(* server_time: silence and stale acceptance expire; expiry races          *)
(* admission through the same CAS (section 7.4).                           *)
OfferExpire ==
  /\ offer \in OfferOpen
  /\ offer' = "expired"
  /\ UNCHANGED <<onb, standing, everAccepted, standingCreated, computeUses>>

(* onboarding_episode_claim (R31): the candidate workload claims the one   *)
(* onboarding Episode.                                                     *)
OnbClaim ==
  /\ onb = "offered"
  /\ onb' = "active"
  /\ UNCHANGED <<offer, standing, everAccepted, standingCreated, computeUses>>

(* onboarding_compute_permit_consume (R32): at most one compute use.       *)
OnbCompute ==
  /\ onb = "offered"
  /\ computeUses = 0
  /\ onb' = "active"
  /\ computeUses' = 1
  /\ UNCHANGED <<offer, standing, everAccepted, standingCreated>>

(* onboarding_episode_complete (R31): completion is evidence only and is   *)
(* never reinterpreted as acceptance (section 14.3).                       *)
OnbComplete ==
  /\ onb \in {"offered", "active"}
  /\ onb' = "completed"
  /\ UNCHANGED <<offer, standing, everAccepted, standingCreated, computeUses>>

(* server_time on the onboarding offer's own expiry.                       *)
OnbExpire ==
  /\ onb \in OnbOpen
  /\ onb' = "expired"
  /\ UNCHANGED <<offer, standing, everAccepted, standingCreated, computeUses>>

(* participant_suspend cascade (R10): minimum revocation set - no partial  *)
(* revocation.                                                             *)
Suspend ==
  /\ standing = "standing_active"
  /\ standing' = "standing_suspended"
  /\ UNCHANGED <<offer, onb, everAccepted, standingCreated, computeUses>>

(* participation_cease cascade (R12): affected Participant only.           *)
Cease ==
  /\ standing = "standing_active"
  /\ standing' = "standing_ceased"
  /\ UNCHANGED <<offer, onb, everAccepted, standingCreated, computeUses>>

(* server_time: StandingRevision expires_at.                               *)
StandingExpire ==
  /\ standing = "standing_active"
  /\ standing' = "standing_expired"
  /\ UNCHANGED <<offer, onb, everAccepted, standingCreated, computeUses>>

(* standing_replacement (named transition, G12): a governance decision     *)
(* adopts a successor StandingRevision.                                    *)
StandingReplace ==
  /\ standing = "standing_active"
  /\ standing' = "standing_superseded"
  /\ UNCHANGED <<offer, onb, everAccepted, standingCreated, computeUses>>

Next ==
  \/ Offer \/ OnboardingOffer \/ Accept \/ Admit \/ Refuse \/ Revoke
  \/ OfferExpire \/ OnbClaim \/ OnbCompute \/ OnbComplete \/ OnbExpire
  \/ Suspend \/ Cease \/ StandingExpire \/ StandingReplace

Spec == Init /\ [][Next]_vars

-----------------------------------------------------------------------------
(* Machine-checked invariants                                              *)

(* Section 7.4: Standing exists only through admission - no onboarding     *)
(* activity, completion, or replay manufactures it.                        *)
StandingRequiresAdmission == standing # "absent" => offer = "admitted"

(* Section 7.4: only the candidate's own acceptance can precede admission. *)
AdmissionRequiresAcceptance == offer = "admitted" => everAccepted

(* Section 7.4 / 14.8: silence never becomes acceptance - an expired offer *)
(* never yields Standing.                                                  *)
SilenceNeverAccepts == offer = "expired" => standing = "absent"

(* The admit/refuse/revoke/expiry CAS admits at most one StandingRevision  *)
(* across every interleaving and replay.                                   *)
AtMostOneStanding == standingCreated <= 1

(* A refused or revoked offer left no live onboarding authority behind     *)
(* (the cascade fenced it; only an earlier independent expiry remains).    *)
RefusalFencesOnboarding ==
  offer = "refused" => onb \in {"absent", "refused", "expired"}
RevocationFencesOnboarding ==
  offer = "revoked" => onb \in {"absent", "revoked", "expired"}

(* One zero-general-effect onboarding compute permit (section 7.4).        *)
OneComputeUse == computeUses <= 1

(* Completion is evidence only: even a completed onboarding Episode gives  *)
(* no Standing without candidate acceptance AND governance admission.      *)
CompletionIsNotAcceptance ==
  (onb = "completed" /\ ~everAccepted) => standing = "absent"

=============================================================================
\* Descriptor-model parity annotations (proof/check-descriptors.py).
\* @parity module: MembershipOfferStanding
\* @parity descriptor: membership-offer-standing.json
\* @parity state: offered
\* @parity state: onboarding
\* @parity state: accepted
\* @parity state: admitted
\* @parity state: refused
\* @parity state: revoked
\* @parity state: expired
\* @parity state: standing_active
\* @parity state: standing_suspended
\* @parity state: standing_ceased
\* @parity state: standing_expired
\* @parity state: standing_superseded
\* @parity transition: absent -> offered via membership_offer
\* @parity transition: offered -> onboarding via onboarding_offer
\* @parity transition: offered -> accepted via membership_accept
\* @parity transition: onboarding -> accepted via membership_accept
\* @parity transition: accepted -> admitted via participant_admit
\* @parity transition: absent -> standing_active via participant_admit
\* @parity transition: offered -> refused via membership_refuse
\* @parity transition: onboarding -> refused via membership_refuse
\* @parity transition: accepted -> refused via membership_refuse
\* @parity transition: offered -> revoked via membership_offer_revoke
\* @parity transition: onboarding -> revoked via membership_offer_revoke
\* @parity transition: accepted -> revoked via membership_offer_revoke
\* @parity transition: offered -> expired via server_time
\* @parity transition: onboarding -> expired via server_time
\* @parity transition: accepted -> expired via server_time
\* @parity transition: standing_active -> standing_suspended via participant_suspend
\* @parity transition: standing_active -> standing_ceased via participation_cease
\* @parity transition: standing_active -> standing_expired via server_time
\* @parity transition: standing_active -> standing_superseded via standing_replacement
\* @parity descriptor: onboarding-activation-offer.json
\* @parity state: offered
\* @parity state: active
\* @parity state: completed
\* @parity state: refused
\* @parity state: revoked
\* @parity state: expired
\* @parity transition: absent -> offered via onboarding_offer
\* @parity transition: offered -> active via onboarding_episode_claim
\* @parity transition: offered -> active via onboarding_compute_permit_consume
\* @parity transition: offered -> completed via onboarding_episode_complete
\* @parity transition: active -> completed via onboarding_episode_complete
\* @parity transition: offered -> refused via membership_refuse
\* @parity transition: active -> refused via membership_refuse
\* @parity transition: completed -> refused via membership_refuse
\* @parity transition: offered -> revoked via membership_offer_revoke
\* @parity transition: active -> revoked via membership_offer_revoke
\* @parity transition: completed -> revoked via membership_offer_revoke
\* @parity transition: offered -> expired via server_time
\* @parity transition: active -> expired via server_time
\* @parity transition: completed -> expired via server_time
