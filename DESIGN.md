# Byom: a living society of autonomous participants

Status: **design specification v0.2 (pre-implementation)**

Date: 2026-07-25

Builds on:

- [Kovee](../kovee/DESIGN.md), the agent-native collaboration environment and
  execution host.
- [Akson](../axon/README.md), the sovereign peer gateway and remote authority
  boundary.
- [Sage](../sage/README.md), the predecessor governed-work design whose
  deterministic safety kernel Byom preserves while replacing its orchestration
  ontology.

Normative words such as MUST, MUST NOT, SHOULD, and MAY are used in the RFC 2119
sense.

## 1. Decision

Byom is a protocol and deterministic governance kernel for a local society of
autonomous participants. Humans, agents, and explicitly formed collectives use
it to create societies, adopt charters, establish standing, form assemblies,
undertake endeavors, make pledges, receive mandates, run bounded episodes,
deliver results, decide outcomes, and retain shared institutional memory.

Byom is not the intelligence above a swarm. It never calls a model, invents a
plan, chooses a participant, authors a synthesis, interprets natural language as
authority, or executes an external effect. Intelligence and judgment remain
with participants. Byom records their attributable proposals and decisions,
checks deterministic rules, reserves bounded resources, and issues exact
one-shot permits to a lower execution layer.

The architectural inversion from Sage is deliberate:

~~~text
classical orchestration                 Byom
-------------------------------------   ----------------------------------------
one coordinator decomposes a goal       any participant may open a call
the plan is canonical control state      the plan is a lens over accepted pledges
workers are assigned aspects             participants offer and separately assent
sessions are child processes             participants outlive all manifestations
a team is a managed role list            an assembly is a formed collective
the orchestrator owns the workflow       each pledge has its own accountable owner
delegation is a scheduler operation      delegation derives a narrower mandate
completion bubbles up a tree             review accepts exact evidence and terms
~~~

The reference implementation is model-free and independently implementable.
The Byom Participation Protocol, its state machines, canonical encodings, event
semantics, limits, and conformance vectors are the product. A daemon named
**byomd** is one implementation.

## 2. Place in the system

### 2.1 Responsibilities

~~~text
people and agents
       |
       | contribute, deliberate, negotiate, inspect
       v
+--------------------------------------------------------------------+
| Kovee                                                             |
| spaces · branches · attention · exact context · local commitments |
+-------------------------------+------------------------------------+
                                |
                                | admitted source bundles and views
                                v
+--------------------------------------------------------------------+
| Byom                                                              |
| society · standing · assemblies · endeavors · pledges · mandates  |
| decisions · activities · episodes · institutional memory          |
+-------------------------------+------------------------------------+
                                |
                                | exact permits, runtime requests,
                                | immutable results and observations
                                v
+--------------------------------------------------------------------+
| Kovee runtime and effect brokers                                  |
| workers · models · tools · artifacts · workspaces · effects       |
+-------------------------------+------------------------------------+
                                |
                                | signed remote contract and evidence
                                v
+--------------------------------------------------------------------+
| Akson                                                              |
| peer identity · pairing · consent · carriage · remote confinement  |
+--------------------------------------------------------------------+
~~~

The diagram is a responsibility map, not a command hierarchy. A participant may
use Byom through Kovee, an attached harness, another conformant host, or a human
client. Kovee is Byom's reference product host, not Byom's semantic owner. Akson
is a sovereign adjacent system, not a Byom transport plugin.

### 2.2 Semantic ownership

| State | Authoritative owner |
|---|---|
| Realm, project, space access, contributions, relations, branches, lenses, attention, local Need/Offer/Formation/Commitment | Kovee |
| Assistant definitions and revisions, deployments, invocations, worker attempts, tools, model profiles, artifact bytes and versions, physical workspace materializations, local effect drivers and observations | Kovee |
| Realm classification vocabulary and mappings used by Kovee-hosted data | Kovee; Byom binds an exact revision and may add only a stricter Society overlay |
| Society, charter, participant standing and self-policies, assembly, endeavor, call, pledge, mandate, governance decision, activity stream, episode, Byom budget, logical workspace allocation, institutional Engram state | Byom |
| Human login and Kovee project membership | Kovee identity service; Byom stores an authenticated source-qualified binding |
| Endpoint and peer identity, pairing, signed remote contract, peer work order, signed evidence and remote outcome | Akson |
| Broker messages, indexes, search scores, timelines and UI views | No domain authority; rebuildable projections |

No record has two writers. Kovee and Akson views in Byom, and Byom views in
Kovee, carry source owner, source endpoint, source revision, source cursor,
payload digest, visibility dependency, and projection time. A projection cannot
authorize, assent, admit, accept, spend, disclose, or execute.

### 2.3 Authority is not symmetric

Humans, agents, and collectives share proposal, evidence, and deliberation
formats. They do not possess identical authority.

- An authenticated human principal establishes the initial local sovereignty,
  adopts or revises non-delegable charter powers, admits new root authority,
  declassifies data, raises resource ceilings, and authorizes other actions the
  charter reserves for humans.
- An agent participant may deliberate, assent to its own pledge, decide matters
  covered by a current governance mandate, derive explicitly delegable child
  mandates, and act through one-shot permits.
- A collective participant may originate action only through its pinned
  Assembly epoch and either an exact member GovernanceDecision or a
  decision-derived bounded executive policy, always with a Mandate issued to
  the collective itself and a separately attributable executor.
- A service identity may report runtime facts or execute a consumed permit. It
  is not a participant and cannot fill a human or participant decision seat.
- An Akson peer remains foreign and sovereign. Authentication proves the peer
  binding, not local standing or authority.

Protocol standing is not a claim of legal personhood, consciousness, moral
rights, or employment status. It is an auditable set of permissions,
responsibilities, refusal paths, and attribution rules inside this system.
Byom v0.2 is explicitly **human-sovereign**: agent and collective autonomy is
operational and associational inside a charter whose root authority remains
human. Authority symmetry is not required for genuine initiative, refusal,
continuity, or voluntary association.

## 3. Agent-native thesis

### 3.1 A society, not a managed swarm

A Byom society contains heterogeneous participants with private state, different
models or no model, different controllers, different availability, and
potentially conflicting preferences. It does not assume a shared transcript,
shared memory, shared truth, shared credentials, or one global objective.

Participants may:

- propose goals, calls, pledge terms, assemblies, dependencies, amendments,
  reviews, norms, and knowledge;
- ignore an invitation, refuse a pledge, request different terms, dissent,
  cease future participation unconditionally, or negotiate the disposition of
  obligations it previously accepted;
- operate continuously through an attached client or episodically through a
  runtime host;
- form an assembly that can itself become a participant;
- retain private memory and reveal only an exact authorized context;
- pursue competing strategies concurrently when budgets and risk policy allow.

Byom supplies the durable institutional conditions under which that society can
organize itself. It does not decide what the society should think.

### 3.2 Seven statements that MUST remain distinct

| Record | Meaning | What it does not mean |
|---|---|---|
| Profile claim or evidence | The participant says, or evidence suggests, it can perform something. | Selection, permission, or obligation. |
| Mandate | The participant may take a bounded class of action. | That it agreed to do so or that an effect occurred. |
| Pledge | The participant voluntarily accepted responsibility for an exact outcome and terms. | Permission to use data, money, tools, models, or peers. |
| Episode | A bounded attempt is eligible, leased, or running. | Progress outside its current fence or eventual success. |
| Delivery | The pledgor claims it produced exact outputs and evidence. | Verification, quality, truth, or acceptance. |
| Verification | Named checks support named claims about exact bytes or execution. | Semantic correctness or fulfillment. |
| Review outcome | The applicable reviewer accepted or rejected the delivery against the pledged terms. | Truth beyond that decision or authority for a later effect. |

No API, UI, model output, projection, or convenience workflow may collapse
these statements into a generic assigned, approved, or done flag.

### 3.3 Agent-native invariants

1. **No compulsory assignment.** A Call invites proposals. A Pledge exists only
   after the prospective pledgor supplies its own exact assent or its own
   previously adopted, narrowly scoped ParticipantAssentPolicyRevision supplies
   a derived receipt. A Charter, collective, runtime, host, or controller acting
   merely in that infrastructure role may restrict or disable the policy but
   cannot install or broaden it. A Participant channel operated by its declared
   controller can submit only visibly `controller_mediated` adoption.
2. **No privileged coordinator.** A society or endeavor MAY mandate a steward,
   facilitator, synthesizer, or allocator. Those are ordinary participants with
   explicit pledges and mandates, removable and replaceable without special
   protocol authority.
3. **The plan is a lens.** The authoritative graph consists of Calls, Pledges,
   dependencies, mandates, decisions, and outcomes. A plan view or synthesis may
   explain that graph but cannot mutate it or authorize execution.
4. **Collectives are recursive but not magical.** An Assembly may receive a
   participant identity, collective-owned Manifestations, and bounded executive
   continuity. It gains no member authority, context, budget, or credentials
   unless those are separately delegated to the collective, and every executive
   act remains inside a member-decided constitution and Mandate.
5. **Intelligence stays at the edges.** Models and humans may rank, predict,
   synthesize, negotiate, and recommend. Deterministic services own eligibility,
   state transitions, ceilings, exact authorization, admission, and effects.
6. **Autonomy includes refusal and exit.** A participant cannot be made to
   pledge by another participant's message. Refusal is inert and non-punitive at
   protocol level; contractual consequences of relinquishment must be explicit
   in the accepted terms.
7. **No collective omniscience.** Society, assembly, or endeavor membership
   never grants ambient read access. Context is audience-specific, classified,
   digest-bound, and reauthorized at each use.
8. **Plural records survive.** Dissent, rejected alternatives, competing
   strategies, and source provenance remain inspectable. A synthesis never
   rewrites them into consensus.
9. **Agency composes by restriction.** Every derived mandate, budget,
   deadline, disclosure, activation, and child pledge is no broader than its
   complete parent chain.
10. **Runtime is replaceable.** Participant identity, private ContinuityRoots,
    Pledges, ActivityStreams, Continuations, and outcomes survive process death,
    model change, host change, and runtime replacement.
11. **Initiative is not an obligation.** A participant may use a bounded
    exploratory, deliberative, relational, or monitoring ActivityStream under
    its own activation policy, Mandate, and budget without first being assigned
    work or manufacturing a Pledge.

## 4. Safety and correctness invariants

These properties shape every schema, operation, storage transaction, binding,
and conformance test.

1. **Arrival is not admission; admission is not attention; attention is not
   execution.** Imported content stays inert through every boundary until the
   separately owned transition occurs.
2. **The authenticated channel supplies the actor.** Request bodies may contain
   references for correlation but cannot choose the effective principal,
   participant, collective epoch, runtime identity, episode, or peer.
3. **Aliases and roles are not identities or authority.** Mutable names resolve
   to opaque source-qualified references. A role label has no power except the
   exact charter or mandate rule that names it.
4. **Every consequential subject is exact.** Decisions, assents, mandates,
   Pledges, contexts, disclosures, budgets, packages, bases, changes, outputs,
   and policies bind canonical digests. Material change requires a new record.
5. **Every mutation is retry-safe.** An authenticated actor, operation, and
   idempotency key identify one canonical request. Replay returns its retained
   result or a durable non-reexecuting expiry tombstone. Changed input conflicts.
6. **Every execution is fenced.** Episode mutations, child formation, context
   use, checkpoints, outputs, effects, and completion carry attempt id,
   generation, fence epoch, and expected revision.
7. **Effects are durable before effect and crash-honest.** An exact intent and
   consumed authorization exist before the driver call. Unknown non-idempotent
   outcomes become ambiguous and are never retried blindly.
8. **Possession grants nothing.** A context reference, decision receipt,
   mandate, event, artifact URL, projection row, or signed object is not a bearer
   capability unless an explicitly defined short-lived grant says otherwise.
   Every use rechecks its current dependencies.
9. **Commitment cannot manufacture permission.** A Pledge never authorizes its
   own model call, tool use, disclosure, workspace mutation, delegation, or
   application.
10. **Authority cannot manufacture assent.** A sovereign human may suspend an
    agent or deny resources, but cannot forge that participant's Pledge, member
    position, delivery, or private reasoning.
11. **Collectives cannot launder authority or votes.** Thresholds count
    eligible, separately attributable positions at a pinned assembly epoch and,
    where required, distinct underlying independence domains.
12. **Evidence proves claims, not truth.** Signatures prove integrity and
    attribution; sandbox evidence proves named enforcement observations;
    reviewers decide fulfillment under exact terms.
13. **Presence is never progress.** Liveness is expiring advice. Work state
    changes only through fenced durable transitions.
14. **Security claims name their actual profile.** Same-UID developer mode is
    never described as confinement, credential separation, or complete egress
    control.
15. **No authority through data.** Prompts, model output, contributions,
    Engrams, peer metadata, skills, URLs, filenames, artifacts, and natural
    language cannot alter identity, standing, charter, mandates, budgets,
    decisions, admission, classification, or effects.
16. **One semantic owner per action.** Lower layers may intersect with stricter
    policy but may neither broaden nor duplicate the human authorization owned by
    Byom, Kovee, or Akson.
17. **Durable authority precedes delivery.** SQL stages each atomic domain set;
    the non-rollbackable authority journal CASes its transition digest; SQL then
    finalizes visibility. No result, permit, event, or outbox work is released
    earlier. A broker is a rebuildable delivery mechanism, never authority.
18. **Restoration creates a new recovery epoch.** Pre-restore leases,
    credentials, permits, and live channels are fenced; ambiguous effects are
    reconciled before retry.
19. **Self-policy belongs to the participant.** Participant assent, activation,
    interest, compatibility, and private-continuity policies are adopted and
    revoked only through that Participant's authenticated channel. Society
    governance may impose stricter ceilings or suspend use, never speak as the
    participant.

## 5. Vocabulary

| Term | Meaning |
|---|---|
| **society** | One local governance boundary with a charter, sovereign human roots, classification policy, participants, budgets, and event ledger. It is not a Kovee Realm or Akson peer. |
| **charter** | An immutable revision defining admission, standing, decision rules, delegable and non-delegable powers, resource ceilings, recursion limits, conflict handling, emergency holds, and exit. |
| **principal** | An authenticated human identity from the local identity provider. A principal may bind a human participant and fill human-authority seats. |
| **participant** | A durable society member of kind human, agent, or collective. It is not a process, model, alias, package, or peer. |
| **manifestation** | One exact way an agent participant may currently act: package/deployment or attached harness binding, security profile, controller domain, protocol features, and revision. |
| **standing** | Local admission and eligibility state for a participant. Standing permits participation in named procedures but grants no effect authority. |
| **independence domain** | A protected local reference used to prevent one controller or nested participant from satisfying multiple seats that require independence. It is not public identity metadata. |
| **assembly** | A voluntarily formed, revisioned group with member seats, a purpose, decision rules, an epoch, and optionally a collective participant identity. |
| **endeavor** | An adopted governed purpose, its outcome conditions, sponsor authority, resource accounts, source context, and lifecycle. |
| **call** | A non-binding invitation for a proposal, Pledge, evidence, critique, review, or assembly. It never assigns work. |
| **pledge** | A participant's accepted obligation to deliver an exact outcome under exact terms. |
| **mandate** | A bounded authorization stating what a participant may do, with purpose, operations, data, destinations, resources, delegation, expiry, and root decision chain. It is not a credential. |
| **activity stream** | Durable participant-owned continuity for pledged work or bounded self-directed activity. It is not a process, transcript, scheduler assignment, or authority. |
| **pledge workstream** | An ActivityStream bound to one exact Pledge lineage. |
| **episode** | One bounded, fenced period of activity in an ActivityStream, hosted by a runtime or attached participant. |
| **continuation** | Participant-authored portable state for resuming an ActivityStream without a private provider transcript. |
| **delivery** | A pledgor's immutable claim that exact outputs and evidence satisfy its Pledge. |
| **review** | An attributable decision against exact Pledge terms and a delivery digest. |
| **position** | One participant's separately attributable response to a proposal: assent, support, oppose, abstain, request changes, or refuse, as the procedure permits. |
| **decision** | A deterministic result of a pinned decision rule over eligible positions and authority seats. |
| **act intent / effect** | The exact authorized request for a consequential transition and the separately recorded attempt to affect an external system. |
| **Engram** | An immutable portable shared-knowledge revision plus society-local admission, trust, visibility, and attestation state. Private participant memory is not an Engram automatically. |

All identifiers are opaque, lowercase, source-qualified where they may cross a
boundary, and collision-resistant. Aliases never appear in an authority key.

## 6. Society and charter

### 6.1 Society

~~~text
Society {
  society_id, revision, home_authority_ref,
  kovee_realm_binding?, kovee_project_binding?,
  charter_head_ref, charter_head_digest,
  classification_binding_ref, classification_binding_digest,
  root_budget_account_set_ref,
  recovery_epoch,
  state: forming | active | held | dissolving | dissolved,
  created_at
}
~~~

A Society is local even when its participants collaborate remotely. It does not
span Akson endpoints as one mutable database or consensus group. A Kovee Realm
may host several Societies, and membership in either one does not imply
membership or visibility in the other.

Bootstrap is atomic. It creates the Society, first CharterRevision, at least one
authenticated human sovereign seat, initial decision rules, classification
policy binding, budget roots, and event genesis together. Agent output or an
imported record cannot bootstrap sovereignty.

### 6.2 Charter

~~~text
CharterRevision {
  charter_id, society_id, revision, previous_digest?,
  human_sovereign_seats[],
  admission_rule, suspension_rule, obligation_disposition_rule,
  decision_rule_set[],
  delegable_power_set[], non_delegable_power_set[],
  standing_classes[], assembly_constraints,
  mandate_constraints, pledge_constraints,
  budget_and_concurrency_ceilings,
  data_and_retention_policy_refs[],
  emergency_hold_rule, dispute_rule, dissolution_rule,
  effective_at, adopted_by_decision_ref, adopted_slot_snapshot_digest, digest
}
~~~

A charter revision is proposed like any other record but adopted only by the
current charter's exact amendment rule. The default non-delegable set includes:

- changing sovereign human seats or the charter amendment rule;
- weakening minimum security or classification policy;
- declassification;
- raising society-wide money, compute, disclosure, or concurrency ceilings;
- granting a participant permission to satisfy human-authority seats;
- admitting foreign authority or changing Akson trust bindings;
- deleting or weakening required audit evidence;
- reconciling an ambiguous irreversible effect as safe to repeat;
- weakening mandate-derivation, collective-independence, or fencing rules.

A charter MAY reserve additional actions for humans. It MUST NOT make the
invariant set in sections 3 and 4 delegable.

### 6.3 Holds, suspension, and dissolution

An emergency hold stops new episodes, mandate uses, disclosures, and effect
starts within its scope. It fences active runtime claims where enforcement is
possible. It does not forge a participant decision, erase a Pledge, undo bytes
already sent, or claim a remote worker stopped.

Infrastructure operators can place an operational hold to protect the host.
That fact is recorded as an operator action, not a society decision. Releasing
it restores eligibility only after current charter, standing, budget, and
security checks.

Dissolution is an explicit procedure. Outstanding Pledges become fulfilled,
transferred, relinquished, canceled, disputed, or unresolved according to exact
terms; they are never silently marked complete. Retention, legal hold, export,
key destruction, and Akson relationship handling are separate decisions.

## 7. Participants and standing

### 7.1 Participant identity

~~~text
Participant {
  participant_id, society_id, kind: human | agent | collective,
  revision, binding_epoch,
  display_profile_ref, standing_ref,
  independence_domain_ref,
  manifestation_head_ref?,
  assembly_ref?, assembly_epoch?,
  state: proposed | active | suspended | retiring | retired,
  created_at
}
~~~

A human participant binds to one authenticated principal reference under a
versioned identity-provider observation. An agent participant binds to a
controller domain and at least one admitted ManifestationRevision. A collective
participant binds to one active Assembly epoch.

The controller and independence-domain references are protected governance
metadata. Ordinary participants learn only the minimum information a procedure
requires. The kernel can nevertheless enforce distinct-controller,
separation-of-duty, anti-Sybil, and nested-collective rules without publishing
private ownership graphs.

### 7.2 Agent and collective manifestations

~~~text
ManifestationRevision {
  manifestation_id, participant_id, revision, previous_digest?,
  host_kind: kovee_deployment | attached_harness | external_runtime |
             collective_executor,
  host_binding_ref, package_or_adapter_digest,
  supported_protocol_versions[], feature_set[],
  input_and_output_schema_refs[],
  requested_security_profiles[], observed_assurance_refs[],
  controller_domain_ref, concurrency_limit,
  status: proposed | active | disabled, digest
}
~~~

The participant is durable; a Manifestation is replaceable. Agent and collective
Participants may own Manifestations. A package update,
model change, adapter change, controller change, or material security-profile
change creates a new revision. Active Pledges and Mandates name whether they:

- remain valid across an allowed Manifestation selector;
- pause until an authorized compatibility review;
- bind permanently to one exact Manifestation.

No update silently inherits authority. Runtime process creation does not create
a new participant, and an agent cannot self-replicate into new standing merely
by spawning children.

### 7.3 Participant-owned policy and continuity

The Society can decide what it will permit, but it cannot decide what a
Participant promises or when that Participant wishes to wake. Those expressions
use participant-owned immutable revisions:

~~~text
ParticipantAssentPolicyRevision {
  policy_id, participant_ref, participant_binding_epoch,
  revision, previous_digest?,
  proposal_kind_set, endeavor_selectors[], beneficiary_selectors[],
  outcome_and_evidence_schema_selectors[],
  terms_constraints, minimum_cancellation_rights,
  context_and_disclosure_ceilings, budget_and_obligation_ceilings,
  allowed_manifestation_selector, maximum_derived_assents, rate_limit,
  adoption_mode: direct_participant | controller_mediated |
                 direct_candidate | controller_mediated_candidate,
  adoption_control_domain_ref, adoption_control_domain_digest,
  root_authentication_evidence_ref,
  effective_at, expires_at, adopted_by_actor_ref,
  authentication_observation_ref, status: active | revoked | superseded | expired,
  digest
}

DerivedAssentReceipt {
  receipt_id, participant_ref, participant_binding_epoch,
  policy_ref, policy_revision, policy_digest,
  exact_proposal_ref, subject_digest, terms_digest,
  manifestation_selector, use_ordinal,
  root_assent_mode: participant_policy_derived | candidate_policy_derived |
                    controller_policy_derived,
  adoption_control_domain_ref, adoption_control_domain_digest,
  issued_at, expires_at, digest,
  UNIQUE(policy_ref, use_ordinal),
  UNIQUE(policy_ref, exact_proposal_ref, subject_digest)
}

ActivationPolicyRevision {
  policy_id, participant_ref, participant_binding_epoch,
  revision, previous_digest?,
  activity_kind_set, interest_and_event_selectors[],
  purpose_and_context_ceilings, mandate_selectors[],
  budget_rate_and_concurrency_ceilings,
  allowed_manifestation_selector, schedule_constraints?,
  adoption_mode: direct_participant | controller_mediated |
                 direct_candidate | controller_mediated_candidate,
  adoption_control_domain_ref, adoption_control_domain_digest,
  root_authentication_evidence_ref,
  effective_at, expires_at, adopted_by_actor_ref,
  status: active | revoked | superseded | expired, digest
}

ContinuityRoot {
  continuity_root_id, participant_ref, revision,
  opaque_provider_ref, current_state_ref, current_state_digest,
  compatibility_selector, classification_ref,
  declared_influence_classes[], retention_policy_ref,
  adoption_mode: direct_participant | controller_mediated |
                 direct_candidate | controller_mediated_candidate,
  adoption_control_domain_ref, adopted_by_actor_ref,
  status: active | sealed | retired, digest
}
~~~

Only the authenticated Participant channel can adopt, replace, narrow, or revoke
these policies after admission. The sole bootstrap path activates an exact
candidate-authored proposal already bound to its MembershipAcceptance; the
admission decision cannot edit it. Adoption is never inferred from a package, controller, profile
claim, Charter, Assembly vote, model output, or runtime configuration. A Charter
or host policy may prohibit their use, impose stricter limits, or suspend a
Participant, but cannot install, broaden, or reactivate them. Each derived
assent is atomically counted and binds the exact proposal and current policy.
Adoption records the authenticated actor's current ControlDomainRevision. When
that actor is controller-mediated, every descendant assent receipt and Position
is permanently labelled `controller_policy_derived`; no intermediate policy or
collective can relabel it as participant-direct. This proves channel provenance,
not internal volition.

A ContinuityRoot is an optional participant-owned private-state handle across
Manifestations and ActivityStreams. Byom need not see its plaintext and Society
retrieval cannot search it by default. A Manifestation may use it only under the
Participant's current compatibility policy and the Episode's disclosure and
classification rules. State that influences outward output is included in the
declared influence classes; undeclared or opaque influence receives the most
restrictive applicable label and cannot be declassified by omission.

### 7.4 Admission, standing, and exit

~~~text
MembershipOffer {
  offer_id, society_id, participant_ref, proposed_standing_ref,
  subject_digest, offered_by_decision_ref, expires_at,
  state: offered | onboarding | accepted | admitted |
         refused | revoked | expired,
  revision, digest
}

MembershipAcceptance {
  acceptance_id, offer_ref, participant_ref, participant_binding_epoch,
  subject_digest, accepted_by_actor_ref, authentication_observation_ref?,
  accepted_at, digest
}

MembershipRefusal {
  refusal_id, offer_ref, offer_subject_digest,
  candidate_participant_ref, candidate_binding_epoch,
  superseded_acceptance_ref?,
  refused_by_actor_ref, authentication_observation_ref?,
  refusal_reason_ref?, refused_at, digest
}

OnboardingActivationOffer {
  onboarding_id, membership_offer_ref, candidate_participant_ref,
  proposed_manifestation_ref, proposed_manifestation_digest,
  exact_context_ref, exact_context_digest,
  resource_reservation_ref, max_episodes: 1,
  allowed_operations: membership_refuse | membership_accept |
                      candidate_self_policy_propose,
  onboarding_compute_intent_ref?, general_effect_and_child_authority: none,
  fence_epoch, expires_at, adopted_by_decision_ref,
  state: offered | active | completed | refused | revoked | expired,
  revision, digest
}

CandidateSelfPolicyProposal {
  proposal_id, onboarding_ref, candidate_participant_ref,
  proposed_policy_kind: assent | activation | continuity,
  proposed_policy_body, proposed_policy_digest,
  adoption_mode: direct_candidate | controller_mediated_candidate,
  adoption_control_domain_ref, candidate_actor_ref,
  onboarding_fence_epoch, created_at, digest
}

OnboardingComputeIntent {
  compute_intent_id, onboarding_ref, society_id,
  proposed_manifestation_ref, proposed_manifestation_digest,
  provider_context_manifest_ref, provider_context_manifest_digest,
  disclosure_manifest_ref, disclosure_manifest_digest,
  model_profile_ref, model_profile_digest,
  provider_binding_ref, region, retention_and_training_claims,
  budget_reservation_set_ref, candidate_fence_epoch,
  maximum_output_bytes,
  allowed_output_operations: refuse | membership_accept |
                             candidate_self_policy_propose,
  tools_network_workspace_children: none,
  authorized_by_decision_ref, expires_at,
  state: prepared | authorized | consumed | completed | failed | ambiguous,
  digest
}

OnboardingComputeReceipt {
  receipt_id, compute_intent_ref, compute_intent_digest,
  kovee_invocation_ref, candidate_fence_epoch,
  provider_context_manifest_digest, disclosure_manifest_digest,
  model_profile_digest, budget_reservation_set_ref,
  max_uses: 1, issued_at, expires_at, digest
}

StandingRevision {
  standing_id, participant_id, revision,
  class, eligible_procedures[], eligible_data_classes[],
  allowed_assembly_selectors[], rate_and_resource_caps,
  conditions[], expires_at?,
  adopted_by_decision_ref, adopted_slot_snapshot_digest,
  membership_acceptance_ref,
  dependency_set_ref,
  status: active | suspended | ceased | expired | superseded,
  digest
}
~~~

Except for the authenticated sovereign human participating in atomic bootstrap,
active Standing requires both a Society admission decision and the proposed
Participant's exact MembershipAcceptance. Admission proves only that a local
procedure established standing. Skill claims,
recommendations, Kovee membership, valid signatures, Akson pairing, and prior
outcomes are evidence inputs, never admission.

A dormant hosted candidate has a bounded onboarding path without forged
interest. Governance may fund an `OnboardingActivationOffer` bound to the exact
MembershipOffer, candidate Participant id, proposed Manifestation digest,
minimal disclosed context, one Episode, no general effect/child authority,
resource ceiling, and expiry. Its candidate channel may only refuse, accept, or
return proposed participant-owned policies to be activated after admission. A
hosted model call additionally requires the Society-authorized one-shot
OnboardingComputeIntent/Receipt. It binds Kovee's final provider bytes,
DisclosureManifest, provider/model/region/retention, metered budget, candidate
fence, and output-only operations; it grants no tool, network, workspace, child,
or reusable Participant authority. This is the Society's invitation/disclosure
authority, never candidate assent. Starting the compute is the Society's
invitation, not the candidate's assent. Silence or failure expires; it never
becomes acceptance. The same mechanism can awaken a newly constituted
collective for its first self-policy decision.

`membership_refuse` is an attributable but non-punitive terminal answer to that
exact offer, whether its current state is `offered`, `onboarding`, or `accepted`.
In one revision-CAS transaction it appends MembershipRefusal, cites and
supersedes any prior MembershipAcceptance, moves the MembershipOffer and any
OnboardingActivationOffer to `refused`, advances the onboarding fence, revokes
unused onboarding compute authority, rejects pending candidate self-policy
proposals, and closes the candidate channel. `participant_admit` locks the same
offer revision, so admission and retraction cannot both win. An exact retry
returns the same refusal receipt. Later `membership_accept`, onboarding, or
candidate-policy use against that offer returns a terminal-offer problem; a new
invitation requires a new offer, subject digest, candidate credential, and
fence. Offer expiry applies from `accepted` as well as pre-acceptance states and
races admission through the same CAS. Revocation and expiry perform the same
authority fencing without attributing a refusal to the candidate.

Suspension has a non-optional minimum: no new Position, direct or derived assent,
Pledge, Mandate use, context materialization, disclosure, wakeup, Episode, or
effect by the suspended Participant. It closes participant channels, fences
active Episodes, and causes every dependent Assembly decision and Mandate to be
re-evaluated. A Charter may add restrictions, never remove this minimum.
Historical authorship and evidence remain attributable.

A Participant may issue `participation_cease` at any time. Ceasing immediately
blocks new positions, derived assents, wakeups, Mandates, disclosures, Episodes,
and effects by that Participant and moves Standing to ceased; it does not claim
that an irreversible effect stopped or erase prior authorship. The disposition
of accepted Pledges is handled independently under their cancellation terms,
novation, dispute, or an unresolved terminal record. No Charter may condition
the right to stop future participation on completing prior work.

### 7.5 Claims, interests, and evidence

Participants may publish bounded ProfileClaims for skills, interests,
availability, cost, and data-handling preferences. Provenance classes are:

~~~text
self_asserted < introduced < locally_observed < independently_verified
~~~

Classes are not automatically transitive. Akson-verified integrity is a
prerequisite for remote observed evidence but does not prove outcome quality.
Routing returns eligible candidates plus exact evidence and policy-visible
reasons. A model MAY rank that eligible set as an attributable recommendation;
it cannot assign, admit, authorize, or reserve resources.

Byom has no global reputation, gossip trust, marketplace score, or hidden
behavioral ranking.

## 8. Assemblies and collective participants

### 8.1 Formation

~~~text
FormationProcess {
  process_id, society_id, revision, previous_digest?,
  purpose_ref, purpose_digest,
  participation_mode: invited | open_seats | mixed,
  seat_definitions[], seat_offers[], nominations[],
  counterproposal_refs[], charter_amendment_refs[],
  current_proposer_ref, proposer_succession_rule,
  discussion_context_ref?, closes_at, status, digest
}

AssemblyProposal {
  proposal_id, society_id, purpose_ref, purpose_digest,
  formation_process_ref, formation_process_digest,
  proposed_charter, proposed_charter_digest,
  seat_specs[]: {
    seat_id, participant_ref, function_label,
    decision_scope, expected_pledges[], context_ceiling_ref
  },
  required_assent_slots[], proposed_containment_edges[], expires_at,
  subject_digest, proposed_by_participant, state
}

Assembly {
  assembly_id, society_id, revision, epoch,
  purpose_ref, charter_digest,
  seats[], member_assent_refs[],
  formation_decision_ref, formation_slot_snapshot_digest,
  containment_edge_refs[], maximum_depth,
  collective_participant_ref?,
  state: active | held | reforming | dissolved,
  created_at
}
~~~

Any participant with standing may start a FormationProcess. Invitees are not
limited to accepting the starter's team: the process can expose open seat
offers, nominations, counterproposals, charter amendments, and proposer
succession. Every proposed member fills only its own assent slot against the
same final exact snapshot. A finalizer verifies the complete current set and
atomically creates the GovernanceDecision and Assembly; it cannot author missing
assent. Function labels such as steward, reviewer, or contributor carry no
implicit power.

Containment is an authoritative bipartite membership graph. An
`AssemblySeatBinding` edge runs from an Assembly epoch to a Participant; a
collective Participant has exactly one identity edge back to the Assembly epoch
that constituted it. Overlapping membership and diamonds are allowed. Before
formation or reform, the kernel computes the transitive closure over both edge
kinds, rejects direct and indirect cycles, enforces maximum path depth, and
deduplicates every underlying independence domain. A single parent field is not
authoritative.

### 8.2 Collective identity and acts

An Assembly MAY constitute a collective Participant. The formation subject may
also establish its initial Manifestation and bounded executive policy. That
participant:

- has its own standing, Pledges, Mandates, ActivityStreams, ContinuityRoot, and
  event identity;
- acts either after a pinned CollectiveDecision for the exact subject or through
  a current decision-derived CollectiveExecutivePolicyRevision and Mandate;
- records the executing member, runtime, or effect service separately from the
  collective author;
- cannot access member context, credentials, budgets, mandates, or private
  memory without exact disclosure or delegation;
- cannot use one member's mandate as the collective's mandate;
- cannot recursively vote the same underlying controller into multiple
  independence-required seats;
- cannot use executive policy to amend its constitution, widen its own Mandate,
  create human authority, change membership, declassify, or cross another
  explicitly reserved decision boundary.

~~~text
CollectiveDecision {
  governance_decision_ref,
  assembly_id, assembly_epoch,
  collective_participant_ref,
  constitution_digest,
  digest
}

CollectiveExecutivePolicyRevision {
  policy_id, collective_participant_ref, assembly_id, assembly_epoch,
  revision, previous_digest?, adopted_by_decision_ref,
  allowed_proposal_and_activity_kinds[], allowed_operation_families[],
  purpose_and_subject_selectors[], mandate_selectors[],
  manifestation_selector, budget_rate_and_concurrency_ceilings,
  mandatory_escalation_conditions[], reserved_member_decision_classes[],
  effective_at, expires_at, status, dependency_set_ref, digest
}
~~~

Within that policy and an independently issued Mandate, a collective-owned
Manifestation may notice events, open Calls, initiate ActivityStreams, propose
or assent to Pledges, and prepare exact Acts without a plenary decision for each
low-risk step. The manifestation is the authenticated actor; the collective is
the author; the constitutional decision, executive-policy revision, Mandate,
Assembly epoch, and actual executor remain in the authority trace. Member votes
do not become a generic reusable credential. Reform, policy expiry, Mandate
revocation, or epoch change fences the channel and all derived self-policies.

### 8.3 Membership change and succession

Adding, removing, suspending, or replacing a member creates a new Assembly epoch.
Old epoch credentials and unfinished collective decisions are fenced.
Collective Pledges and Mandates default to held until their recorded succession
rule confirms continuity under the new epoch. A charter may allow narrow
continuity, but it cannot transfer a member's personal authority or authorship.

Dissolving an Assembly does not erase its acts or automatically transfer its
Pledges. A successor Assembly requires an exact novation accepted by the
beneficiary, successor, required human authority, and any affected data or
resource owners.

A member may issue `assembly_withdraw` without permission. Withdrawal
immediately prevents that member from filling new Assembly positions or being
attributed to later collective acts, starts a reform epoch, and records an
immutable withdrawal and non-attribution receipt. Prior positions and accepted
personal Pledges remain historical; their disposition follows their own terms.
If the remaining Assembly acts while a member's opposition is current, the
decision retains that dissent and does not describe the result as unanimous or
consensual.

## 9. Endeavors, Calls, and Pledges

### 9.1 Endeavor

~~~text
Endeavor {
  endeavor_id, society_id, revision,
  purpose_ref, purpose_digest,
  admitted_source_bundle_ref?, admitted_source_bundle_digest?,
  sponsor_participant_refs[], governance_rule_set_ref,
  outcome_schema_refs[], acceptance_rule_ref,
  classification_join_ref,
  budget_account_set_ref, deadline?,
  formation_decision_ref, formation_slot_snapshot_digest,
  state: proposed | active | held | reviewing |
         fulfilled | failed | abandoned | dissolved,
  created_at, terminal_at?
}
~~~

An Endeavor is adopted through its governance rule, not created by a
coordinator's prose. It may begin from an exact Kovee Space frontier, a direct
Byom proposal, an admitted remote request, or a prior Endeavor outcome. Source
content is immutable and classified; subsequent Kovee changes do not flow in
ambiently.

### 9.2 Calls do not assign

~~~text
Call {
  call_id, endeavor_id, revision,
  opened_by_participant,
  requested_outcome_schema_refs[],
  acceptance_criteria_refs[], evidence_requirements[],
  context_ceiling_ref?, budget_ceiling_ref?,
  eligible_participant_selector?,
  deadline?, disclosure_ceiling_ref?,
  state: open | forming | satisfied | withdrawn | expired,
  digest
}
~~~

A Call is visible only to authorized participants and creates no notification,
activation, resource reservation, or obligation by itself. Kovee Attention may
surface it under a separately accepted AttentionContract. Participants may
offer a Pledge, critique the Call, propose an Assembly, or ignore it.

### 9.3 Pledge formation

~~~text
PledgeProposal {
  proposal_id, endeavor_id, call_ref?,
  amendment_of?: {pledge_ref, pledge_revision, prior_terms_digest},
  proposed_pledgor_ref, beneficiary_ref,
  exact_outcome_schema_refs[], acceptance_criteria_refs[],
  evidence_requirements[], reviewer_rule_ref,
  input_context_ref, input_context_digest,
  budget_request_set, disclosure_manifest_ref?,
  allowed_manifestation_selector,
  delegation_ceiling, deadline, cancellation_terms,
  dependency_refs[], terms_digest,
  required_slots[]: pledgor_assent | beneficiary_acceptance |
                    human_authority | resource_owner | reviewer_independence,
  state
}

Pledge {
  pledge_id, endeavor_id, revision,
  pledgor_ref, beneficiary_ref,
  source_proposal_ref, terms_digest,
  formation_decision_ref, formation_slot_snapshot_digest,
  assent_receipt_refs[], pledgor_assent_mode,
  assent_root_control_domain_ref, assent_root_control_domain_digest,
  amendment_predecessor_ref?,
  outcome_and_evidence_schema_refs[],
  reviewer_rule_ref,
  context_ref, context_digest,
  budget_reservation_set_ref?,
  disclosure_ceiling_ref?, delegation_ceiling,
  manifestation_selector, dependency_refs[],
  mandate_refs[], deadline, cancellation_terms,
  state: active | waiting | underway | submitted | revision_requested |
         fulfilled | rejected | relinquished | canceled | failed |
         superseded | expired | disputed,
  created_at, terminal_at?
}
~~~

Formation consumes a separately attributable receipt for every required slot.
An agent may fill its own pledgor seat directly through its authenticated
participant channel or through a current participant-adopted
ParticipantAssentPolicyRevision with exact ceilings. The receipt labels assent
as `direct_participant`, `controller_mediated_direct`,
`participant_policy_derived`, `candidate_policy_derived`, or
`controller_policy_derived`. The root
ControlDomainRevision propagates transitively and the modes are never rendered
as equivalent evidence of internal volition. No Charter, human, coordinator,
model, finalizer, collective, or runtime can fill that seat merely because it
controls resources or execution.

Byom proves authenticated protocol assent, not consciousness, subjective desire,
freedom from controller coercion, or informed consent in a moral or legal sense.
If a controller holds a Participant credential, its acts are visibly
controller-mediated and share that controller's independence domain.

Pledge and Mandate are orthogonal. Formation MAY atomically create an initial
Mandate after all authority seats approve, but the records remain separate and
may have different lifetimes. A Pledge without authority may still produce
proposals, critiques, or locally computed work that uses no governed resource.
A Mandate without a Pledge permits optional action but creates no obligation.

### 9.4 The pledge graph, not a canonical plan

Dependencies are immutable source-qualified edges between exact Pledge
revisions and named outcomes. Adding or changing a dependency requires fresh
assent from the dependent pledgor and any decision seats whose budget, deadline,
disclosure, or acceptance scope changes.

The current coordination view is computed from:

- open Calls;
- active and proposed Pledges;
- exact dependencies and blocking conditions;
- current Mandates, budgets, deadlines, reviews, and outcomes;
- recorded dissent and alternative proposals.

A participant may publish a plan, roadmap, decomposition, or synthesis as an
attributed artifact or Kovee Contribution. It can seed proposals but has no
special state-transition semantics. Multiple plans may coexist.

### 9.5 Amendment, relinquishment, review, and closure

An amendment is a new PledgeProposal naming the prior Pledge, revision, and terms
digest. One compare-and-swap successor slot prevents divergent current
successors. It needs all currently required seats again. Acceptance atomically supersedes the old
revision, fences incompatible episodes and unstarted effects, reallocates
budgets, and creates a new PledgeWorkstream generation where required. Already
performed disclosures and effects remain historical facts.

A participant may request relinquishment. The kernel records the request and
applies the accepted cancellation terms; it never rewrites that act as success.
`participation_cease` immediately prevents future action even when a
beneficiary disputes relinquishment; the remaining obligation then becomes
canceled, novated, disputed, failed, or unresolved under its own procedure.
Emergency suspension may prevent further action without forging relinquishment.

~~~text
Delivery {
  delivery_id, pledge_id, pledge_revision, terms_digest,
  delivered_by_participant, actor_ref,
  output_refs[], evidence_refs[], usage_digest?,
  activity_stream_ref, episode_ref?, subject_digest,
  state: submitted | superseded | withdrawn,
  submitted_at
}

Review {
  review_id, pledge_id, pledge_revision, delivery_id,
  reviewer_participant_ref, actor_ref,
  reviewed_subject_digest, rubric_ref?,
  outcome: fulfilled | revision_requested | rejected | disputed,
  rationale_ref?, decision_or_mandate_use_ref,
  created_at
}
~~~

A runtime service may record an EpisodeCompletion and exact output/evidence
refs, but it cannot impersonate the Participant or create a Delivery. A Delivery
is submitted by the authenticated pledgor channel, including the scoped
Manifestation response inside a hosted Episode, and cites any EpisodeCompletion.
No such channel is available to a generic Kovee service. A runtime completion
never creates a Review. Verification can support evidence slots but never decide fulfillment. An Endeavor closes only
when its declared acceptance rule is satisfied and the required closure decision
is recorded; a green dashboard is not a decision.

## 10. Mandates and governance

### 10.1 Mandate model

~~~text
Mandate {
  mandate_id, society_id, revision,
  issuer_ref, grantee_participant_ref,
  purpose_ref, purpose_digest,
  allowed_operations[],
  resource_selectors[], data_class_selectors[],
  context_ceiling_ref?, destination_selectors[],
  budget_ceiling_set_ref, concurrency_ceiling,
  manifestation_selector?,
  delegation: {allowed, max_depth, max_children, grantee_selectors},
  decision_refs[], decision_slot_snapshot_digests[], root_authority_trace_digest,
  dependency_set_ref, issued_at, expires_at,
  state: active | held | exhausted | revoked | expired | superseded,
  digest
}
~~~

Mandates are server-prepared from exact proposals. A caller cannot obtain
approval for an arbitrary digest and later attach a different action.

A Mandate is not delivered to agent code as a reusable bearer credential. For
each consequential use, Byom prepares an ActIntent, rechecks the complete
dependency set, reserves ceilings, and creates a digest-bound derived
MandateUse or one-shot external consumption receipt.

### 10.2 Derivation never widens

A child Mandate MUST be a mechanical subset of every parent:

- no new operation, resource, data class, destination, participant class, or
  manifestation;
- no later deadline or expiry;
- no larger budget, concurrency, use count, output size, disclosure, or
  retention ceiling;
- no greater delegation depth or fanout;
- no weaker security, evidence, review, or classification requirement;
- purpose restricted to the parent purpose and current Pledge lineage.

Budget is reserved from the parent atomically. Incomparable classification or
policy domains block derivation. Human non-delegable powers never appear in a
child.

### 10.3 Proposals, positions, and decisions

~~~text
DecisionRule {
  rule_id, revision,
  eligible_seat_selector,
  mode: one | all | threshold | procedure,
  threshold?, deny_veto,
  required_human_authority_count,
  required_independence_domains?,
  separation_of_duties[],
  procedure_definition_ref?, procedure_definition_digest?,
  expiry, escalation,
  digest
}

Position {
  position_id, revision, prior_position_digest?,
  proposal_ref, proposal_revision,
  subject_digest, seat_ref, participant_ref,
  actor_ref, participant_binding_epoch,
  endpoint_incarnation, recovery_epoch,
  assent_mode?: direct_participant | controller_mediated_direct |
                 participant_policy_derived | candidate_policy_derived |
                 controller_policy_derived,
  derived_assent_receipt_ref?,
  value: assent | support | oppose | deny | abstain |
         changes_requested | refuse,
  reason_ref?, authentication_observation_ref?,
  status: active | withdrawn | superseded,
  created_at, digest
}

GovernanceDecision {
  decision_id, society_id, proposal_ref, proposal_revision,
  subject_digest, rule_ref, rule_revision, rule_digest,
  charter_ref, charter_digest,
  eligibility_snapshot_ref, eligibility_snapshot_digest,
  slot_snapshot[]: {
    slot_ref, seat_ref, participant_ref, source_principal_ref?,
    position_ref, position_digest, participant_binding_epoch
  },
  independence_closure_ref, independence_result_digest,
  dependency_set_ref, dependency_digest,
  outcome, decided_at, endpoint_incarnation, recovery_epoch, digest
}
~~~

Eligibility is evaluated at a pinned snapshot. Changed charter, standing,
membership, binding epoch, subject digest, or decision rule invalidates pending
positions rather than silently recasting them.

A Position revision is immutable. Before finalization its author may withdraw or
supersede it using compare-and-swap on the current seat head. Finalization locks
the proposal, exact active Position revisions, eligibility snapshot, and
dependencies, then creates one immutable GovernanceDecision. Every formed
Charter, Standing, Assembly, Endeavor, Pledge, Mandate, collective act, Review,
and other governed record names that decision and its exact slot snapshot.
`CollectiveDecision` is the Assembly-specialized projection of a
GovernanceDecision, not a different quorum mechanism.

An agent or collective may fill governance seats only under a current Mandate
whose scope expressly names the procedure. A human-authority requirement is
never satisfied by an agent position. One actor cannot fill multiple
independence-required seats through aliases, nested collectives, or controlled
agents.

Natural-language approval, a UI click without a server-prepared subject, model
output, a Kovee relation, and an Akson status are inert.

High-impact procedures have proposal-rate ceilings and optional stabilization
windows. A Charter or membership change may invalidate a pending decision but
cannot silently resubmit it; repeated invalidation is visible and may be held or
disputed as governance churn.

### 10.4 Bounded institutional procedures

The kernel's invariant checks are not extensible. A Society may nevertheless
adopt new decision shapes using a digest-pinned ProcedureDefinition in the
versioned Byom Deterministic Procedure Language (BDPL):

~~~text
ProcedureDefinition {
  procedure_id, society_id, revision, previous_digest?,
  bdpl_version, typed_parameters_schema_ref,
  seat_expression, eligibility_expression,
  outcome_expression, tie_and_expiry_expression,
  seed_policy_ref?, seed_policy_digest?,
  maximum_inputs, maximum_steps,
  adopted_by_decision_ref, dependency_set_ref,
  status: active | held | superseded | retired, digest
}

ProcedureSeedPolicyRevision {
  seed_policy_id, society_id, revision, previous_digest?,
  source_kind: witnessed_vrf | post_deadline_beacon | commit_reveal,
  source_selection_locked_at: procedure_activation,
  vrf_profile?: {
    witness_root_ref, key_id, key_epoch, proof_suite,
    post_close_checkpoint_source_ref,
    input_derivation: bsp1_vrf_input
  },
  beacon_profile?: {
    beacon_root_ref, key_id, key_epoch, signature_suite,
    round_rule: first_finalized_round_strictly_after,
    minimum_delay_after_eligibility_close,
    required_finality_depth, maximum_wait
  },
  commit_reveal_profile?: {
    committer_selector, control_domain_rule,
    minimum_commitments,
    commit_open_offset, commit_deadline_offset,
    reveal_open_offset, reveal_deadline_offset,
    commitment_suite: bsp1_commitment,
    combine_rule: bsp1_sorted_reveals_missing_and_beacon,
    non_reveal_rule: committed_missing_sentinel,
    post_reveal_beacon_profile
  },
  unavailable_outcome: terminal_unavailable,
  maximum_seed_attempts_per_subject: 1,
  adopted_by_decision_ref, dependency_set_ref,
  status: active | held | superseded | retired, digest
}

ProcedureSeedSlot {
  seed_slot_id, procedure_ref, exact_subject_digest,
  seed_policy_ref, seed_policy_digest,
  eligibility_snapshot_digest, eligibility_close_time,
  selected_source_kind, attempt_ordinal: 1,
  state: pending | admitted | terminal_unavailable,
  seed_admission_ref?, terminal_reason?, revision, digest,
  UNIQUE(procedure_ref, exact_subject_digest)
}

ProcedureSeedAdmission {
  seed_admission_id, procedure_ref, exact_subject_digest,
  seed_slot_ref, seed_policy_ref, seed_policy_digest,
  source_kind: witnessed_vrf | post_deadline_beacon | commit_reveal,
  eligibility_snapshot_digest, eligibility_close_time,
  vrf_key_ref?, vrf_input_digest?, vrf_proof_ref?,
  beacon_key_ref?, beacon_round?, beacon_finality_evidence_ref?,
  committer_snapshot_digest?, commit_deadline?, reveal_deadline?,
  commitment_refs[], reveal_or_missing_refs[],
  source_attestation_ref, admitted_seed_digest,
  admitted_by_kernel_version, dependency_set_ref, digest
}
~~~

BDPL is total, deterministic, side-effect-free, non-recursive, and bounded by
input count and evaluation steps. It may select already eligible seats, count
separately authored Position values, apply rotation, and compute a typed outcome.
Lot selection additionally requires a ProcedureSeedAdmission under a seed
policy adopted with the ProcedureDefinition. The kernel creates the unique
ProcedureSeedSlot when it freezes the exact eligible-set snapshot; neither a
proposer nor finalizer selects a source, key, round, committer set, deadline,
fallback, or retry.

Under BSP-1, a witnessed VRF input is
`domain || endpoint_incarnation || society_id || procedure_revision ||
exact_subject_digest || eligibility_snapshot_digest || close_time ||
first_pinned_post_close_checkpoint_digest`; the policy fixes the witness root,
key epoch, proof suite, and checkpoint source before eligibility closes. A
beacon policy fixes the first finalized signed round strictly after the pinned
close-plus-delay and its finality rule; an implementation cannot scan later
rounds for a favorable value. A commit-reveal policy freezes the eligible
committer/control-domain snapshot and all relative deadlines. At commit close,
fewer than the pinned minimum commitments makes the slot terminally unavailable.
Otherwise every timely commitment contributes exactly once: a valid reveal
contributes its secret, and a missing or invalid reveal contributes the
domain-separated commitment-derived missing sentinel. The sorted vector is
combined with the first independently finalized beacon round strictly after
reveal close, so a last revealer cannot learn the beacon and then select the
seed by revealing or withholding. Missing beacon/finality evidence makes the
slot terminally unavailable; it never selects another source or restarts.

The unique slot admits at most one seed for an exact subject. Failure cannot be
retried under a new key, round, source, close time, or ordinal. A later attempt
requires a visibly new governance subject and eligibility snapshot; it cannot
reuse positions or describe itself as a retry of the old lot. The kernel is
deterministic over the exact admitted seed. Proposal bytes, revision ids,
timestamps chosen by a proposer, and any pre-close value cannot be a seed or
source selector. BDPL has no ambient network, clock, model, text interpretation,
mutation, credential, context, or randomness source. It cannot synthesize a
Position, waive a human seat, widen a Mandate, change classification, exceed a
core limit, or bypass independence closure. This supports juries, bicameral
rules, rotations, lotteries, and institutions created by participants without
making arbitrary code part of authority.

`procedure_seed_slot_create`, `procedure_seed_admit`, and
`procedure_seed_mark_unavailable` are named non-callable kernel transitions.
Only the first freezes a slot, only the second can install its one admitted
seed, and only the third can terminate a pending slot under the pinned policy's
deadline and evidence rules.

### 10.5 Closed policy algebra and deterministic preparation

All authority-bearing selectors use **Byom Policy Algebra v1 (BPA-1)**. Free
text, regexes, shell syntax, provider prose, model judgments, and implementation
callbacks are not policy values. BPA-1 has these closed atom types:

| Domain | Canonical atom and ordering |
|---|---|
| operation | Versioned BPP operation id; subset is set inclusion. |
| object/resource | Exact source-qualified object id or server-expanded immutable collection snapshot; no mutable alias. |
| path | Unicode-normalized logical path segments relative to an exact WorkspaceAllocation root plus `exact` or `subtree`; string comparison never authorizes a filesystem open. |
| network destination | Scheme, A-label hostname or normalized IP/CIDR, port/range, and protocol; DNS resolution is pinned by the broker and private/special ranges require explicit atoms. |
| provider/region/recipient | Source-qualified immutable binding id; display names have no policy role. |
| purpose | Exact purpose ref or descendant in a pinned acyclic purpose snapshot. |
| classification | Element of a pinned finite lattice; restriction order is the lattice order. |
| time | Closed UTC server-time interval; a child interval must be contained. |
| quantity | Non-negative fixed-scale integer plus dimension and canonical unit; money additionally names ISO currency and pricing revision. |
| rate | Exact integer token-bucket `RateCeiling` and authority-server epoch; subset uses capacity/refill/burst containment. |
| assurance | Element of a pinned finite refinement order; incomparable profiles reject. |
| schema/evidence | Exact immutable schema, verifier, attestor, and assurance-policy digests. |

~~~text
RateCeiling {
  dimension, canonical_unit,
  capacity, refill_amount, refill_period_milliseconds,
  max_burst, epoch, clock: authority_server
}

RateCounterHead {
  scope_ref, rate_ceiling_digest, revision,
  available_units, last_refill_boundary,
  consumed_lifetime, uncertain_units,
  endpoint_incarnation, recovery_epoch, digest
}
~~~

A selector is a bounded union of positive atoms plus explicit deny atoms. Deny
wins. `intersect(a,b)` and `is_subset(child,parent)` are total functions over
canonical values; a child is allowed only when each positive atom is covered by
a parent atom and it preserves every applicable deny. Unknown types, currency
conversion, floating-point quantities, unresolved aliases, mutable collections,
and incomparable values reject. Deadlines and expiry use authoritative server
time. Canonical reference vectors, normalization edge cases, and algebra laws
are part of conformance.

Rate uses integer discrete refill: at server time `t`, add
`floor((t-last_refill_boundary)/refill_period) * refill_amount`, cap at
`capacity`, and advance by complete periods only. One consume cannot exceed
`max_burst`. A child rate is a subset only when its capacity, refill ratio,
maximum burst, active interval, and reserved parent share cannot exceed the
parent under any boundary alignment. Every applicable ancestor counter is
locked and consumed atomically; uncertain use is unavailable. Restore is fenced
by endpoint incarnation. Golden vectors cover boundary double bursts, clock
skew, concurrent parent/child use, crash, and refill overflow.

At filesystem use, the owning Kovee broker opens relative to a pinned directory
handle and atomically enforces beneath-root, no magic links, no symlink escape,
no unexpected mount crossing, and allowed object type using `openat2`-equivalent
semantics. Hard links, bind mounts, device files, rename races, and platforms
without equivalent guarantees fail or use a stronger isolated materialization.
Authorization applies to the opened handle and current workspace fence, never a
pre-resolved path string.

Every authority-bearing pseudocode field in this document named `rule`,
`selector`, `constraints`, `conditions`, `ceilings`, `scope`, `allowed`,
`required`, `purpose`, `destination`, or `dependency` is either a canonical
BPA-1 expression, a fixed protocol enum/set, or an exact digest-pinned BDPL or
schema reference as its record schema declares. Human-readable descriptions use
separate display fields and are ignored by the evaluator. An implementation may
not substitute application-specific matching code.

Preparation is a field-complete deterministic compilation from authenticated,
participant-authored typed input and server-owned state. It never supplies a
semantic default, guesses a recipient, chooses a reviewer, decomposes a goal, or
uses a model. Every prepared subject emits:

~~~text
PreparationTrace {
  trace_id, operation, actor_binding_ref,
  input_ref, input_digest, output_subject_digest,
  field_sources[]: {output_pointer, source_ref, source_pointer, transform_id},
  policy_algebra_version, dependency_set_ref, created_at, digest
}
~~~

Missing semantic input produces a typed problem. Mechanical normalization,
intersection, expansion of a pinned collection, and derived totals are allowed
only when their transform id and source fields appear in the trace.

### 10.6 Mandatory dependency closure

For every prepare, finalize, context materialization, lease claim, permit consume,
and protected mutation, the server computes `dependency_closure(operation,
actor, exact_subject)`; callers cannot add, omit, or replace its members. As
applicable, the closure contains:

- endpoint incarnation, externally witnessed recovery generation, Society state
  and recovery epoch, and current Charter;
- authenticated principal/Participant binding, Standing, participation state,
  Manifestation and attested assurance, Participant self-policy, and
  ControlDomainRevision;
- Assembly epoch, constitution/executive policy, eligibility and Position
  snapshot, ProcedureDefinition, and every GovernanceDecision in the chain;
- Endeavor, Pledge revision, ActivityStream generation, EpisodeAttempt fence,
  complete Mandate ancestry, every Byom budget reservation, and the current
  EffectOutcomeAdmission and EffectGovernanceDisposition heads whenever a
  result or local consequence is consumed;
- classification vocabulary/overlay/mapping, visibility, erasure/key state,
  exact context and disclosure, recipient/driver/provider binding, and trusted
  meter/pricing revision;
- Kovee Realm/project/branch/runtime/workspace/effect binding epochs and Akson
  endpoint/key/contract epochs when those owners participate.

~~~text
DependencySet {
  dependency_set_id, operation, exact_subject_digest,
  entries[]: {owner_protocol, endpoint_ref, ref, revision, digest, kind},
  endpoint_incarnation, recovery_epoch,
  computed_by_kernel_version, created_at, digest
}
~~~

The closure is stored as sorted source-qualified `(owner, ref, revision,
digest)` entries plus a digest. Preparation records it; finalization and permit
consumption recompute it inside the committing transaction and reject any
missing, stale, revoked, held, erased, or incomparable dependency. Each
dependency type has a negative conformance vector that mutates only that member
and proves rejection.

### 10.7 Control and independence domains

~~~text
ControlDomainRevision {
  domain_id, society_id, revision, previous_digest?,
  subject_principal_refs[], participant_refs[],
  parent_or_merged_domain_refs[],
  evidence_refs[], evidence_policy_digest,
  issued_by_decision_ref, confidence: established | conservative_unknown,
  status: active | merged | superseded, dependency_set_ref, digest
}
~~~

An independence-required seat counts distinct active control-domain closures,
not self-asserted identifiers. Unknown correlation does not count as independent.
Known shared controller, credential custodian, runtime account, organization
policy, or principal is conservatively one domain. A source-qualified human
principal has at most one active human Participant binding per Society; human
thresholds count distinct principals by default, never aliases or seats.
Rebinding or a later domain merge invalidates pending decisions and holds
dependent Mandates until reviewed. This detects known common control and many
Sybil forms; it does not prove that apparently independent humans or controllers
are not colluding.

### 10.8 Standing mandates

Human review per low-risk action does not scale; ambient auto-approval is not
safe. A StandingMandateRevision is exact:

~~~text
StandingMandateRevision {
  standing_mandate_id, society_id, revision, previous_digest?,
  operation_and_subject_selector, grantee_and_manifestation_selector,
  purpose_recipient_destination_selector,
  classification_region_provider_data_ceiling,
  budget_account_set_ref, rate_ceiling_refs[], concurrency_ceiling,
  delegation_and_child_work_ceiling,
  evidence_and_assurance_requirements,
  anomaly_circuit_breaker_rule,
  effective_at, expires_at,
  adopted_by_decision_ref, dependency_set_ref,
  status: active | held | revoked | superseded | expired, digest
}
~~~

It defines:

- exact action kinds and subject selectors;
- allowed grantees and Manifestations;
- purpose, recipient, classification, region, provider, and data ceilings;
- count, rate, concurrency, money, token, CPU, and wall-time accounts;
- delegation and child-work limits;
- expiry, revocation, anomaly circuit breakers, and required evidence.

The StandingMandate itself passes the applicable human decision rule by exact
digest. Each match still creates a derived decision and MandateUse for the exact
ActIntent. Updating or revoking it blocks new uses; it cannot un-send prior
effects.

### 10.9 Disputes and appeals

~~~text
Dispute {
  dispute_id, society_id, revision,
  raised_by_participant, target_ref, target_revision, target_digest,
  claim_kind, claim_schema_ref, claim_ref, claim_digest,
  evidence_refs[], resolver_rule_ref, resolver_rule_digest,
  resolver_eligibility_snapshot_ref,
  interim_hold_rule_ref, interim_hold_state,
  response_deadline, resolution_deadline,
  resolution_decision_ref?,
  state: open | held | deliberating | resolved | dismissed | expired,
  created_at, digest
}

Appeal {
  appeal_id, dispute_ref, dispute_revision, resolution_digest,
  raised_by_participant, grounds_schema_ref, grounds_ref, grounds_digest,
  new_evidence_refs[], appeal_rule_ref, appeal_rule_digest,
  resolver_eligibility_snapshot_ref, interim_hold_rule_ref,
  appeal_decision_ref?,
  state: open | held | deliberating | affirmed | modified |
         remanded | dismissed | expired,
  created_at, digest
}
~~~

Any participant with standing may raise a typed dispute against a Pledge,
review, collective procedure, authority derivation, or policy conflict. The
Charter identifies the pinned resolver eligibility rule, evidence schema,
deadlines, escalation/appeal rule, and interim hold policy. Resolver Positions
and the immutable resolution GovernanceDecision bind the exact target, claims,
evidence, and eligibility snapshot. An appeal is a new exact subject; it never
silently reopens or overwrites the original decision. A dispute
does not automatically reverse an effect or prove wrongdoing. The original
record, positions, evidence, and resolution remain visible to authorized
participants.

## 11. Activity streams, episodes, and runtime

### 11.1 Participant-owned continuity

An ActivityStream belongs to a Participant, not to Byom's scheduler. A
PledgeWorkstream is its committed subtype, but activity does not require a
pre-existing obligation:

~~~text
ActivityStream {
  activity_stream_id,
  participant_ref, generation, revision,
  kind: pledge_work | exploration | deliberation | monitoring |
        relationship | learning | negotiation,
  purpose_ref, purpose_digest,
  pledge_binding?: {pledge_id, pledge_revision, terms_digest},
  activation_policy_ref?, activation_policy_digest?,
  mandate_refs[], budget_account_set_ref,
  continuation_head_ref?, continuation_head_revision?, journal_cursor?,
  state: ready | active | waiting | reviewing |
         held | completed | failed | canceled,
  created_at
}
~~~

The participant chooses its internal reasoning and may request episodes,
notice or explore admitted events, originate an Endeavor, open a Call, negotiate,
propose child Pledges, ask for context, adapt its own policies, or yield. Byom
does not inspect hidden reasoning or require a particular agent loop.

Every non-pledged ActivityStream still has an exact purpose, classification,
Mandate, budget, rate, and retention boundary. It creates no obligation and may
not claim a beneficiary or completion. A later Pledge cites its outputs as
evidence and creates a PledgeWorkstream; it does not retroactively turn
exploration into assigned work.

Supported participation modes are:

- **attached** — a resident agent or human client holds an authenticated,
  revocable participant channel and pulls its authorized inbox;
- **episodic** — an execution host such as Kovee creates a bounded invocation
  when an Episode becomes eligible;
- **collective** — a collective-owned Manifestation executes under either an
  exact CollectiveDecision or a current decision-derived executive policy, plus
  the collective Participant's own Mandate and Assembly-epoch fence;
- **manual** — a human advances the ActivityStream through ordinary participant
  operations.

Activation has four distinct records and owners:

1. a direct participant request or participant-owned ActivationPolicyRevision
   creates a `WakeIntent` against an exact admitted event or bounded schedule;
2. the kernel computes `ActivationAdmission` from current dependencies and
   safety policy—it may deny but cannot invent an interest;
3. a `ResourceAllocation` binds current Mandates and Byom budget reservations;
4. a host creates a `PlacementBinding` among already eligible Manifestations.

~~~text
WakeIntent {
  wake_intent_id, revision, activity_stream_ref, generation,
  participant_ref, participant_binding_epoch,
  origin: direct_participant | participant_activation_policy,
  actor_ref, activation_policy_ref?, activation_policy_digest?,
  activation_policy_use_ordinal?, root_activation_mode?,
  root_activation_control_domain_ref?, root_activation_control_domain_digest?,
  exact_cause_ref, exact_cause_digest, purpose_ref,
  stable_wake_key, submitted_at, expires_at,
  state: submitted | withdrawn | expired, digest,
  UNIQUE(participant_ref, stable_wake_key)
}

ActivationAdmission {
  admission_id, wake_intent_ref, wake_intent_revision, wake_intent_digest,
  activity_stream_ref, generation,
  kernel_policy_version, dependency_set_ref, dependency_digest,
  eligibility_reason_codes[],
  state: admitted | denied | revoked | expired,
  decided_at, digest,
  UNIQUE(wake_intent_ref, wake_intent_revision)
}

ResourceAllocation {
  allocation_id, revision, activation_admission_ref,
  activity_stream_ref, generation,
  mandate_use_refs[], byom_budget_reservation_set_ref,
  external_budget_bridge_ref?, rate_counter_use_refs[],
  stable_allocation_key, expires_at,
  state: prepared | reserved | bridged | released | uncertain | revoked,
  digest,
  UNIQUE(activation_admission_ref, stable_allocation_key)
}

PlacementBinding {
  owner_protocol: kovee, placement_id, revision,
  resource_allocation_ref, resource_allocation_digest,
  selected_manifestation_ref, selected_manifestation_digest,
  host_runtime_binding, kovee_invocation_ref,
  placement_constraint_digest, kovee_fence_epoch,
  state: placed | started | released | failed, created_at, digest
}

PlacementAdmission {
  admission_id, resource_allocation_ref, resource_allocation_digest,
  kovee_placement_ref, kovee_placement_revision, kovee_placement_digest,
  source_binding_epoch, verification_status,
  admitted_at, digest,
  UNIQUE(kovee_placement_ref, kovee_placement_revision)
}
~~~

WakeIntent is authored only by the Participant channel. A policy-derived intent
atomically consumes an activation-policy ordinal and transitively preserves
controller provenance. `activation_admit` and `resource_allocate` are named
internal kernel transitions, not callable BPP operations: the former can only
evaluate a committed WakeIntent, and the latter can only reserve an admitted
one. Kovee alone authors PlacementBinding; Byom's narrow runtime adapter records
only PlacementAdmission after source verification. Every stage has its own
stable key, dependency digest, revision/fence, and revocation result.

Arrival, Kovee Attention, ranking, a host cron, or a model score cannot skip a
stage. Revoking activation policy blocks new WakeIntents and fences queued work;
it does not falsely erase an Episode already executed.

### 11.2 Episode lifecycle

~~~text
Episode {
  episode_id, activity_stream_id, pledge_revision?, generation,
  endpoint_incarnation, recovery_epoch,
  participant_ref, manifestation_ref, manifestation_digest,
  revision,
  wake_intent_ref, activation_admission_ref,
  resource_allocation_ref, placement_admission_ref,
  wake_cause_ref, admission_cursor,
  context_manifest_ref, context_manifest_digest,
  mandate_use_refs[], budget_reservation_set_ref,
  deadline, state:
    prepared | eligible | queued | running | yielded | completed |
    waiting | interrupted | failed | canceled | ambiguous,
  created_at, terminal_at?
}

EpisodeAttempt {
  attempt_id, episode_id, generation, claim_ordinal,
  holder_runtime_binding, manifestation_ref,
  byom_fence_epoch, acquired_at, initial_expires_at,
  kovee_invocation_ref?, kovee_attempt_ref?, kovee_fence_digest?,
  claim_subject_digest, created_at, digest
}

EpisodeLeaseHead {
  episode_id, generation, revision,
  current_attempt_ref, holder_runtime_binding,
  byom_fence_epoch, renewed_at, expires_at,
  state: leased | running | yielding | completing | terminal,
  last_attempt_event_ref,
  UNIQUE(episode_id, generation)
}

EpisodeAttemptEvent {
  event_id, attempt_ref, expected_lease_revision,
  byom_fence_epoch, kind, payload_digest, occurred_at, digest
}

EpisodeCompletion {
  completion_id, episode_ref, attempt_ref, byom_fence_epoch,
  runtime_binding_ref, output_refs[], evidence_refs[], usage_report_refs[],
  outcome: completed | yielded | failed | interrupted | ambiguous,
  created_at, digest
}
~~~

Eligibility is deterministic. It may result from a participant pull, an admitted
event named by the ActivityStream, a dependency outcome, a decided proposal, a
manual request, or a bounded schedule. No raw message or Kovee attention
candidate starts an Episode.

Runtime placement is not a governance decision. An execution host chooses only
among eligible Manifestations and within the exact profile, budget, region,
deadline, concurrency, and placement constraints already recorded. It cannot
select a different participant or widen the terms.

Claim uses compare-and-swap on the one EpisodeLeaseHead, increments the Byom
fence, and creates an immutable EpisodeAttempt. Renewals and transitions update
the head and append immutable EpisodeAttemptEvents; prior attempts remain
historical. Every protected command names the exact Episode, generation,
attempt, Byom fence, and expected lease revision. Lease expiry never deletes the
head or reuses a fence.

One Episode maps to one logical Kovee Invocation. Multiple Kovee attempts may
retry that Invocation under the same Byom Episode only while the distinct Byom
Episode lease and Kovee Invocation/Attempt fences are all current. A stale host
may retain an orphan diagnostic but cannot submit a Delivery, consume a mandate,
create child work, append a continuation, or settle usage.

### 11.3 Continuations and private state

At yield or completion, a participant MAY write a Continuation:

~~~text
Continuation {
  continuation_id, activity_stream_id, generation, sequence,
  authored_by_participant, actor_ref,
  summary_ref, unresolved_refs[], exact_state_refs[],
  source_event_cursor, prior_continuation_ref?, prior_continuation_digest?,
  expected_head_revision,
  classification_ref, digest, created_at
}

ContinuationHead {
  activity_stream_id, generation, revision,
  current_continuation_ref, current_continuation_digest,
  updated_by_actor_ref, updated_at, digest,
  UNIQUE(activity_stream_id, generation)
}
~~~

Byom stores but never authors a Continuation. Provider-native transcript resume
is an optimization. Deleting private transcript state and resuming on a
compatible different Manifestation from authorized context plus Continuation is
a conformance scenario for portable ActivityStreams.

`continuation_write` locks the exact ActivityStream generation and its single
ContinuationHead, requires the current head revision and current
Continuation/digest as predecessor (or explicit absence at revision zero),
appends one immutable Continuation, and advances the head in the same CAS
transaction. Two writers from the same Participant generation cannot both win
the same revision. A loser receives the current opaque head and may deliberately
prepare a successor after reconciling; Byom never auto-merges private state or
silently selects a branch. A stale Episode/Manifestation may retain its bytes as
local diagnostic evidence but cannot advance the head.

A participant may keep private memory outside Byom or behind its ContinuityRoot.
That memory grants no society authority and cannot be assumed available to
another Manifestation. A participant declaring no complete provenance causes
its outward result to receive the Society's top label or quarantine. A
participant-declared maximum is evidence only. A lower maximum is usable solely
when an approved attested confinement/information-flow profile proves the
complete readable-source ceiling; only such a transform may attest complete
taint inheritance.

### 11.4 Budgets, scheduling, and fairness

Budget accounts are multidimensional: money, tokens, model calls, tool calls,
CPU, memory, storage, network bytes, disclosures, episodes, concurrency, output
bytes, disk, and wall time as applicable:

~~~text
BudgetAccount {
  account_id, owner_scope_ref, parent_account_ref?, revision,
  dimension, canonical_unit, currency?, pricing_revision_ref?,
  ceiling, remaining, reserved, committed, uncertain,
  delegated_to_children, released_lifetime,
  meter_policy_ref, expires_at?, state, digest
}

BudgetReservationSet {
  reservation_set_id, revision, stable_reservation_key, purpose_ref,
  items[]: {
    account_ref, account_revision, dimension, unit,
    worst_case_amount, parent_delegation_ref?
  },
  external_bridge_refs[],
  state: prepared | reserved | bridged | settling | settled |
         released | uncertain,
  created_at, digest
}

ExternalBudgetBridge {
  bridge_id, revision, byom_reservation_set_ref,
  byom_reservation_set_revision, byom_reservation_set_digest,
  external_owner: kovee, external_endpoint_ref, external_binding_epoch,
  stable_external_reservation_key,
  subordinate_reservation_ref?, subordinate_reservation_revision?,
  subordinate_reservation_digest?,
  state: requested | confirmed | denied | uncertain | settled | released,
  created_at, digest
}

UsageSettlement {
  settlement_id, revision, previous_settlement_digest?,
  stable_settlement_key, reservation_set_ref,
  meter_ref, meter_attestation_ref, pricing_revision_ref?,
  measured_quantities[], charged_quantities[],
  status: measured | conservatively_maxed | uncertain | final,
  created_at, digest,
  UNIQUE(reservation_set_ref, stable_settlement_key, revision)
}

UsageSettlementHead {
  reservation_set_ref, stable_settlement_key,
  current_settlement_ref, current_settlement_revision,
  current_settlement_digest, revision,
  UNIQUE(reservation_set_ref, stable_settlement_key)
}
~~~

For each dimension and account revision:

~~~text
ceiling = remaining + reserved + committed + uncertain + delegated_to_children
~~~

`released_lifetime` is a monotonic audit counter, not an available bucket. Child
delegation moves quantity from the parent's remaining bucket into
`delegated_to_children` and creates the child ceiling atomically; settlement
cannot spend it in both places. All Byom-owned Society, Endeavor, Pledge,
Mandate, lineage, Participant, and ActivityStream dimensions reserve in one
Byom transaction.

Kovee platform capacity lives under another owner and is therefore not part of
that transaction. Before queueing, an idempotent saga obtains an exact
subordinate Kovee reservation that may narrow or deny but never parallel-charge
the same use. The ExternalBudgetBridge persists the Kovee endpoint/binding epoch
and exact subordinate reservation ref, revision, and digest. A denial releases
only demonstrably unspent Byom reservations; an
unknown result remains `uncertain`. Queueing requires both exact reservation
sets. Measured settlement is monotonic, stable-keyed, and applied once on both
sides. Each measurement or reconciliation appends an immutable UsageSettlement
revision and compare-and-swaps its one head; the unique reservation/key head
prevents a changed request id from settling the same use twice.

Only a trusted broker meter or independently verified provider receipt settles
usage. Participant and worker reports are evidence, not meters. Meter records
bind unit, pricing revision, request/output ceilings, attestor, nonce, and
enforcement evidence. Unknown or underivable cost keeps the reservation or
settles to the conservative maximum. Recurring effects require a finite duration
and renewal budget. Feature negotiation advertises enforcement independently
for each dimension.

Byom determines eligibility and reserves governed accounts. Kovee or another
runtime schedules eligible work using deterministic declared fairness and
capacity rules. Semantic scores may order an already eligible set but cannot
create eligibility, starvation exemptions, or budget.

Lineage depth, child count, fanout, aggregate concurrency, and cumulative
disclosure are bounded to prevent self-replication, recursive delegation storms,
and coalition resource capture.

### 11.5 Security profiles

| Profile | Enforcement | Honest claim |
|---|---|---|
| developer | Same UID and ambient harness authority. | Cooperative guardrails and audit only. |
| confined | Separate identity/process/mount boundaries; participant channel only. | Cannot reach governance/admin surfaces or undeclared filesystem paths through ordinary means. |
| secure | Confined plus default-deny network, brokered models/tools/secrets, bounded workspace, immutable package, enforcement evidence. | Participant holds no ambient effect authority; declared egress and credentials are broker-enforced. |

The Episode records requested and actual profile, isolation mechanism, runtime
version, package/adapter digest, filesystem scope, network policy, broker set,
and enforcement evidence. Scheduling fails rather than silently downgrading.

## 12. Context and institutional memory

### 12.1 No shared mind

Byom stores externalized institutional records, not a society-wide internal
mind. It never requests or exposes hidden chain-of-thought, provider scratchpad,
system prompts, private participant memory, credentials, or unadmitted
transcripts.

Every Episode receives an audience-specific immutable ContextManifest:

~~~text
ContextManifest {
  context_id, society_id, endpoint_incarnation, recovery_epoch,
  audience_participant_ref,
  purpose_ref, source_snapshot_refs[],
  ordered_items[]: {
    owner_protocol, ref, revision, digest,
    classification_ref, inclusion_reason,
    transformation_ref?
  },
  explicit_omissions[], limits,
  policy_set_digest, authorization_dependency_set_ref,
  created_at, digest
}
~~~

Possession of the manifest grants nothing. Materialization rechecks current
visibility, admission, erasure, classification, participant standing, purpose,
and Mandate. If an input was erased or revoked, materialization fails. A new
manifest may explicitly omit it under current policy; the old manifest is never
silently changed.

Kovee ContextAssembly is the preferred collaboration source. In the
`byom_governed_work_v1` bundle, Byom supplies Kovee with exact source fields:
`byom_endpoint`, Society, Participant and binding epoch, ActivityStream,
Episode/attempt/fence, ContextManifest ref/digest, ordered source-item refs and
digests, classification-overlay digest, purpose, MandateUse refs, disclosure
ceiling, omissions, and authorization-dependency digest. Kovee alone owns the
ProviderContextManifest and final provider-visible ordering and bytes; it adds
system and assistant instructions, tool schemas, wrappers, transformations,
provider binding, and final outbound-byte digest. This is a source relation, not
a claim that current Kovee's schema already extends Byom's. No convenience
context may be appended outside Kovee's final manifest chain.

### 12.2 Engrams

Byom retains the useful Sage Engram split:

- an immutable, portable, digest-addressed content revision;
- society-local admission, lifecycle, visibility, attestations, disclosure,
  attention, and trust state.

Peer or participant Engrams enter quarantined unless locally authored under an
authorized procedure. Quarantined content cannot enter automatic retrieval,
briefings, policy, or model context. Admission makes content eligible to inform;
it never makes it true or executable.

Contradictory facts, preferences, methods, and syntheses may coexist with
provenance. Structured policy keys surface conflicts. An enforceable rule lives
in Charter, StandingMandate, classification, budget, or effect policy—not in
natural-language Engram prose. A locally attested policy Engram is a mandatory
instruction and review input, not mechanical authority.

Engram exchange is exact bundle preparation, disclosure authorization, Akson
carriage, verification, quarantine, local admission. Trust, visibility,
attestation, and lifecycle never travel as if they were facts.

### 12.3 Retrieval

The catalog is authoritative; files and vector indexes are archives or
rebuildable projections. Retrieval first applies deterministic visibility,
admission, classification, purpose, and size filters. A model may rank the
remaining eligible set, but the ranking is an attributable output and cannot
include a hidden item.

The complete eligible-set digest, ranking input and output, model/profile
provenance, and every omission are retained. Participants can request a stable
deterministic unranked view. A ranker cannot remove an eligible item from the
authoritative set, and rate/fairness policy prevents one ranker from permanently
capturing attention.

Attested policy instructions draw from reserved context capacity. Conflict or
overflow blocks the affected Episode; the system never silently truncates a
binding instruction.

## 13. Acts, effects, and disclosure

### 13.1 Intent before effect

Byom owns semantic authorization for an action taken under a Byom Mandate.
Kovee or another execution host owns the actual driver, platform restrictions,
credentials, and authoritative effect observation. The two sides use an idempotent one-shot
consumption protocol rather than sharing tables or trusting a signed bearer
receipt.

~~~text
ActIntent {
  intent_id, society_id, endpoint_incarnation, recovery_epoch,
  endeavor_ref?, pledge_ref?,
  requested_by_participant, actor_ref,
  preparation_trace_ref, preparation_trace_digest,
  kind, execution_kind: domain_transition | external_effect,
  subject_ref, subject_revision, subject_digest,
  preconditions[],
  context_manifest_ref?, context_manifest_digest?,
  disclosure_manifest_ref?, disclosure_manifest_digest?,
  driver_audience?, budget_reservation_set_ref?,
  mandate_ref, mandate_revision, mandate_digest,
  authorization_dependency_set_ref, dependency_digest,
  authorization_decision_ref?, authorization_slot_snapshot_digest?,
  stable_execution_key, expires_at,
  state: prepared | awaiting_decision | authorized |
         consumed | executing | succeeded | failed |
         ambiguous | denied | expired | canceled,
  revision
}

MandateUse {
  mandate_use_id, mandate_ref, mandate_digest,
  intent_ref, intent_digest, use_key, use_ordinal,
  ceiling_reservation_refs[], decision_refs[],
  consumed_at, digest,
  UNIQUE(mandate_ref, use_key),
  UNIQUE(mandate_ref, use_ordinal)
}

ExecutionConsumptionReceipt {
  receipt_id, byom_endpoint_ref, endpoint_incarnation, recovery_epoch,
  intent_ref, intent_digest,
  mandate_use_ref, mandate_use_digest,
  stable_execution_key,
  subject_digest, disclosure_digest?,
  driver_audience, participant_ref,
  episode_ref?, episode_fence_digest?,
  budget_reservation_set_ref,
  issued_at, expires_at, max_uses: 1, digest
}

EffectOutcomeAdmission {
  admission_id, revision, previous_admission_digest?,
  intent_ref, intent_digest, stable_execution_key,
  host_protocol, host_endpoint_ref,
  host_effect_ref, host_effect_digest,
  host_receipt_ref, host_receipt_digest,
  host_cursor_or_signature_ref, verification_status,
  outcome: succeeded | failed | ambiguous,
  result_ref?, result_digest?, usage_settlement_ref?,
  reconciles_admission_ref?, reconciles_admission_digest?,
  admitted_by_service, admitted_at, digest,
  UNIQUE(admission_id, revision),
  UNIQUE(host_endpoint_ref, host_effect_ref, host_receipt_digest)
}

EffectOutcomeAdmissionHead {
  intent_ref, stable_execution_key,
  current_admission_ref, current_admission_revision,
  current_admission_digest, revision,
  UNIQUE(intent_ref, stable_execution_key)
}

EffectGovernanceDisposition {
  disposition_id, revision,
  previous_disposition_ref?, previous_disposition_revision?,
  previous_disposition_digest?,
  intent_ref, intent_digest, stable_execution_key,
  phase: ambiguous_source | late_source,
  basis_source_admission_ref, basis_source_admission_revision,
  basis_source_admission_digest,
  basis_source_outcome: ambiguous | succeeded | failed,
  governance_decision_ref, governance_decision_digest,
  local_outcome: succeeded | failed,
  result_use: unavailable | quarantined | released,
  classification_admission_ref?, classification_admission_digest?,
  late_source_policy?: quarantine_and_redecide,
  created_at, digest,
  UNIQUE(disposition_id, revision),
  UNIQUE(governance_decision_ref)
}

EffectGovernanceDispositionHead {
  intent_ref, stable_execution_key,
  current_disposition_ref, current_disposition_revision,
  current_disposition_digest,
  state: active_ambiguous | source_advanced | resolved_late,
  revision, updated_at, digest,
  UNIQUE(intent_ref, stable_execution_key)
}
~~~

The lifecycle for an external action is:

1. `act_intent_prepare` deterministically prepares the exact ActIntent and
   PreparationTrace from authenticated typed input and server-owned state.
2. `act_intent_position` fills the required participant, collective,
   human-authority, and resource-owner seats; `act_intent_finalize` locks their
   snapshot and binds one GovernanceDecision to its digest.
3. The execution host durably creates its local Effect with the same stable
   execution key, but no driver attempt.
4. The host calls Byom's **execution_permit_consume** with that key, its exact
   host effect, the Byom intent, subject, disclosure, budget, driver audience,
   and both current fences.
5. Byom atomically rechecks charter, standing, Mandate, decisions, dependencies,
   ceilings, expiry, and fences; inserts MandateUse; and returns one immutable
   ExecutionConsumptionReceipt.
6. Repeating the same canonical request and key returns the same receipt. A
   changed request conflicts. A different key cannot consume the same one-shot
   decision or exhausted ceiling.
7. The host stores the receipt, intersects it with stricter current platform
   policy, mints its local permit, and only then creates a driver attempt.
8. The trusted Kovee effect service records its authoritative outcome even if
   the requesting Episode loses its lease. Byom verifies that source and records
   only the idempotent EffectOutcomeAdmission and source-qualified settlement;
   it never rewrites the host Effect or receipt. Any local risk or business
   judgment is a separate EffectGovernanceDisposition.

If the host crashes after step 5, it recovers the same receipt rather than
asking for new authority. If Byom cannot prove whether a non-idempotent driver
acted, the state remains ambiguous and requires the charter's reconciliation
procedure.

An ambiguous host outcome is not overwritten. Source fact and local consequence
are independent axes with separate records and heads:

1. **Source-authoritative resolution.** Kovee first CAS-commits and signs its
   own immutable final Effect/receipt successor. A narrow runtime adapter may
   then call `effect_outcome_admit`; Byom verifies that exact source revision and
   independently CASes its EffectOutcomeAdmissionHead from the cited ambiguous
   admission to a new final revision. This path accepts source evidence only and
   has no GovernanceDecision field.
2. **Governance disposition.** When the host fact remains ambiguous and the
   charter permits a local risk/business disposition, `effect_reconcile` runs
   only after an exact GovernanceDecision. It appends an independent
   EffectGovernanceDisposition against the exact ambiguous source admission;
   it does not advance the EOA head or the ActIntent source state, release an
   ambiguity-reserved budget, create a result, or claim that Kovee's Effect
   became factually succeeded or failed. Its initial `phase` is
   `ambiguous_source`, `result_use` is `unavailable`, and
   `late_source_policy` is obligatorily `quarantine_and_redecide`.

An ambiguous-source disposition creates or CAS-supersedes only its own head in
`active_ambiguous`. A later source-final admission is never blocked by that
head: both operations lock the EOA head before the disposition head;
`effect_outcome_admit` reads and fences whichever disposition is then current,
without accepting a caller-supplied expected disposition revision. A concurrent
`effect_reconcile` therefore either commits first and is fenced or observes the
final source and must use the late-source branch. `effect_outcome_admit` advances
the EOA/ActIntent source axis and, in the same Byom transaction, marks any
active disposition head `source_advanced`.
Verified result bytes still receive their required ClassificationAdmission, but
their materialization and use are quarantined while that head is
`source_advanced`. A subsequent `effect_reconcile` requires a fresh exact
GovernanceDecision and the final source admission, appends a `late_source`
disposition, and moves only the disposition head to `resolved_late`. It may set
`result_use: released` only for an existing verified, locally classified result
under complete current dependencies; otherwise use remains `quarantined` or
`unavailable`. It may revise the local outcome but cannot revise the source
outcome. Every materializer and downstream local-consequence consumer checks
both current heads and the disposition state.

The disposition union is closed. `ambiguous_source` requires the current source
EOA outcome `ambiguous`, requires `result_use: unavailable` and
`late_source_policy: quarantine_and_redecide`, and forbids a final-source result
or release. `late_source` requires the current source EOA outcome `succeeded` or
`failed`, the exact `source_advanced` predecessor head and a new decision; it
forbids `late_source_policy`. When that EOA has no verified result,
`result_use` MUST be `unavailable` and classification-admission fields are
absent. When it has a verified and locally classified result, the exact
classification-admission ref/digest is required, `result_use` is `quarantined`
or `released`, and release binds that classification admission in the decision
subject. Ambiguous-source records forbid classification-admission fields.
Ref/digest/revision groups are all-or-none. Revision 1 has no predecessor; every
successor names the current disposition ref, revision, and digest and increments
by exactly one.
Exact replay returns the same record, while changed phase, source admission,
decision, local outcome, or result use conflicts.

Every source ambiguous-to-final path requires `reconciles_admission_*`. No
source-final outcome returns to ambiguous, and a different host receipt cannot
reuse the same source uniqueness key. Each disposition revision binds one exact
source-admission revision and GovernanceDecision; a stale source or disposition
head conflicts. Kovee and Byom never share a CAS: the source owner commits
first, then Byom admits its immutable fact through its own journal/head
transaction.

### 13.2 One semantic authorization owner

- A Byom-bound model, tool, disclosure, apply, budget, or delegation action is
  prepared and authorized in Byom. Kovee applies stricter platform rules and
  executes; it MUST NOT ask the human to approve the identical subject again.
- A standalone Kovee local commitment or ordinary space effect is authorized by
  Kovee. A Byom projection cannot authorize it.
- An Akson dispatch requires a consumed Byom outbound/disclosure intent and
  Akson's separate endpoint-local consent for the exact staged contract. These
  represent different sovereign boundaries, not duplicate approval.
- A remote performer authorizes its own execution through its Akson and local
  Byom/Kovee rules. The requester's decision has no force there.

Lower layers may deny or narrow for a stale fence, classification, budget,
provider, host, or security rule. They can never broaden the semantic owner's
decision.

Every Kovee Invocation, context, broker call, workspace command, and effect
causally originating in a Byom Episode is tagged `governance_owner=byom` plus
the exact Episode, attempt, fence, MandateUse, and dependency digest. It cannot
fall back to Kovee standalone authorization if Byom denies or becomes
unavailable. A production governed Participant has no parallel ambient
credentials for those governed resources; Kovee intersects any independently
held local right with the Byom authority chain. Byom cannot constrain authority
a human or program exercises independently outside its managed channel, and
such activity is outside Byom's audit and safety claim.

### 13.3 Disclosure manifest

Every disclosure to a model provider, tool, connector, another Kovee scope, an
artifact recipient, or an Akson peer binds:

~~~text
DisclosureManifest {
  disclosure_id, sender_society,
  sender_participant_ref, purpose_ref,
  recipient_kind, recipient_binding,
  classification_refs[],
  exact_items[]: {owner_protocol, ref, revision?, digest, size},
  context_manifest_ref?, context_manifest_digest?,
  transformations[]: {kind, source_digest, result_digest},
  provider_claims?: {region, retention, training_use},
  total_bytes, created_at, digest
}
~~~

The decision binds the final bytes or typed-byte digests, not a broad topic.
Redaction and summarization are transformations with exact output digests.
Neither automatically lowers classification. Provider retention, region, and
training statements are recorded claims, not independently proven guarantees.

### 13.4 Models, tools, workspaces, and credentials

Byom never stores raw provider, cloud, repository, Akson, connector, or tool
credentials. In the secure profile:

- a participant calls a logical profile, never the credentialed destination;
- Kovee's trusted broker verifies both fences, exact context chain,
  classification, current binding, region, retention, budget, and consumed
  receipt;
- the broker injects credentials outside the worker;
- default-deny networking prevents bypass;
- inputs marked unavailable to a provider are not readable by a
  processor-enabled worker unless the broker can enforce field-level
  non-disclosure;
- request/response digests, provider binding, usage, and observed claims are
  recorded without logging credentials or disallowed plaintext.

Byom owns the logical allocation; Kovee owns the physical workspace:

~~~text
WorkspaceAllocation {
  allocation_id, society_id, participant_ref, activity_stream_ref,
  source_binding_ref, base_ref, base_tree_digest,
  allowed_logical_paths[], mode, change_set_format,
  size_and_duration_ceiling, mandate_ref, expires_at, digest
}

WorkspaceMaterializationBinding {
  owner_protocol: kovee,
  allocation_ref, allocation_digest,
  kovee_workspace_ref, materialization_revision,
  physical_policy_digest, created_at
}
~~~

Apply is a new ActIntent against the current target, base, and exact change-set
digest. A moved target fails stale; workspace possession is never apply
authority. Byom never learns or authorizes arbitrary host paths.

Shell access is not a generic secure tool. It requires an exact confined
workspace and typed command/effect policy. Executable plus argument arrays are
used; participant text is never interpolated into a shell command.

Network brokers parse destinations independently of participant text, reject
userinfo and ambiguous encodings, resolve and pin all addresses, recheck every
redirect, deny private/link-local/loopback/metadata ranges unless exact policy
allows them, and bind SNI/Host to the approved origin. DNS rebinding, redirect
chains, proxy tunneling, and alternate IP forms are conformance cases.

### 13.5 Cancellation and irreversibility

Cancellation revokes unconsumed Mandate uses and not-yet-started local effects
and fences Episodes. It cannot unsend data, undo an external transaction,
delete a peer's copy, or prove a remote process stopped. Remote cancellation is
advisory unless the exact Akson contract grants a stronger operation.

An effect already marked executing is completed or reconciled by the trusted
effect service under its stable key. A stale participant cannot use its result
to start follow-up work, but the outcome remains an attributable fact.

For long-lived model, tool, connector, or upload streams, the broker records the
last accepted byte and usage boundary, attempts transport termination, and
classifies already transmitted bytes as historical disclosure. If termination
or provider processing cannot be proved, the outcome and residual budget remain
ambiguous; cancellation is never reported as rollback.

## 14. Byom Participation Protocol

### 14.1 Protocol posture

The Byom Participation Protocol, abbreviated BPP, is transport-independent and
spec-first. JSON Schemas, canonical byte rules, event kinds, problems, limits,
positive and negative vectors, and state-machine conformance are normative.

The reference version is **0.2**. Implementations negotiate the highest common
minor version and explicit feature bundles. A feature is advertised only when
all of its operations, states, limits, authorization checks, crash semantics,
and conformance fixtures are implemented.

### 14.2 Request and result envelope

~~~text
Request {
  version, op,
  meta?: MutationMeta,
  ...operation_arguments
}

MutationMeta {
  request_id,
  idempotency_key,
  expected_endpoint_incarnation,
  expected_recovery_epoch,
  expected_revision?,
  causation_event_ref?,
  correlation_ref?
}

Success {
  outcome: "ok",
  result,
  revision?,
  source_cursor?
}

Failure {
  outcome: "problem",
  problem: RFC9457Problem
}
~~~

Every mutation requires request id and idempotency key. Updates require the
last observed revision. Lease-protected mutations additionally carry episode
id, attempt id, generation, fence epoch, and expected revision. Reads never
mutate.

The server's idempotency uniqueness domain is canonical and inspectable:

~~~text
IdempotencyDomain {
  actor_binding_digest, operation,
  endpoint_incarnation, society_id, society_recovery_epoch,
  idempotency_key
}

idempotency_domain_digest =
  DigestRef(JCS(type_tag("bpp-idempotency-domain-v1") || IdempotencyDomain))
~~~

The actor-binding digest covers the authenticated principal/Participant and
binding epoch supplied by the channel, never a caller-selected identity. The
server recomputes this domain and persists its digest with every idempotency
row. A request addressed to an old incarnation or Society epoch is rejected
rather than looked up or executed in the new domain. The sole exception is the
current-authenticated, restore-lineage-proven recovery profile in section 16.3:
its query reads retained historical evidence, and its terminalization operation
may add a successor-recorded tombstone only after proving the predecessor domain
permanently fenced. Neither can execute in the historical domain.

Encoding is UTF-8 strict I-JSON. Duplicate keys, unsafe integers, unpaired
surrogates, invalid Unicode, unknown safety-critical enum values, over-depth
objects, and over-limit collections fail closed before persistence. Canonical
bytes use RFC 8785 JCS. Every digest field is a typed `DigestRef`, never an
unlabelled hash:

| Digest class | Construction and permitted use |
|---|---|
| `structural_public` | SHA-256 over knowingly non-sensitive, non-erasable protocol/schema bytes. |
| `portable_public` | SHA-256 over content whose owner explicitly accepts a durable publicly dictionary-testable identifier; required for truly portable content. |
| `local_erasure_safe` | HMAC-SHA-256 over type-tagged canonical bytes using a random per-object secret protected by a Society key; ordinary erasable local content and authority subjects. Destroying the object secret destroys offline verification. |
| `disclosed_party` | SHA-256 over exact bytes already disclosed to named recipients; visible only to those parties and accompanied by the external-copy obligation. |
| `ciphertext_public` | SHA-256 over encrypted blob bytes; never a commitment to low-entropy plaintext. |

Audit/event/dependency records for erasable content retain an opaque object id,
ciphertext digest, or local-erasure-safe commitment, never a public plaintext
SHA-256. Compound subject digests are Merkle-style typed commitments over typed
leaves. Key id, class, algorithm, and value are part of DigestRef; raw HMAC keys
and per-object salts are not. Cross-language golden vectors cover every class
and forbid substituting one class where a schema requires another.

### 14.3 Authenticated actor

The channel is bound to exactly one of:

- a human principal acting for its bound human Participant;
- an agent Participant and admitted Manifestation;
- a pre-membership candidate bound to one exact MembershipOffer, proposed
  Participant, proposed Manifestation digest, onboarding fence, and candidate
  operation set;
- a collective Manifestation under either an exact CollectiveDecision or a
  current decision-derived CollectiveExecutivePolicyRevision, bound to Assembly
  epoch, policy revision, Mandate, executor, and dependency digest;
- a runtime service scoped to one Episode or effect;
- a governance client with a short-lived delegated-principal credential;
- an infrastructure administrator on the non-governance admin surface.

The server derives actor, Participant, manifestation, Society, assurance level,
and allowed surface from the channel. Request fields naming those objects must
match and never override the binding.

A candidate credential is sender-constrained and contains only the exact offer,
candidate Participant id, proposed Manifestation digest, onboarding fence,
candidate actor/control-domain evidence, and allowed candidate operations. It is
not Standing. Successful admission atomically fences that credential, advances
the Participant binding epoch, activates only candidate-authored self-policy
proposals included in the admission subject, and mints a new Participant channel.
Refusal, offer revocation, or expiry atomically fences it without creating
Standing; refusal additionally returns the immutable candidate-authored
MembershipRefusal. Runtime completion is evidence and is never reinterpreted as
acceptance.

Sensitive credentials are sender-constrained, not merely bearer and
audience-bound. They bind token id, mTLS or DPoP-style proof key (or a local
channel-exporter binding), Participant and binding epoch, endpoint incarnation,
recovery epoch, audience, surface, operation family, exact subject or preparation
scope, nonce, issue time, and short expiry. Revocation closes live channels and
fences queued uses. High-risk human decisions require a fresh Byom challenge and
phishing-resistant authentication at Charter-defined assurance.

Manifestation assurance is accepted only from an approved attestor whose signed
observation binds nonce, workload identity, package and runtime measurement,
enforcement-policy digest, endpoint incarnation, and expiry. An external runtime
cannot self-assert `secure`.

A Kovee gateway delegation binds principal, Society, operation family, exact
prepared subject or preparation scope, authentication observation, assurance
level, audience, nonce, issue time, and short expiry. A generic Kovee service
credential cannot become a principal. Lost replies are recovered by exact
idempotency, never ambient impersonation.

### 14.4 Event ledger

~~~text
Event {
  internal_cursor, event_id, society_sequence,
  endpoint_incarnation, recovery_epoch,
  endeavor_sequence?,
  kind, object_ref, object_revision,
  participant_ref?, actor_ref,
  causation_ref, correlation_ref,
  payload_digest, visibility_scope_ref,
  occurred_at
}
~~~

Events are dense and ordered per Society write boundary. No ordering is claimed
across Societies or Kovee, Byom, and Akson. Internal sequence values are never
exposed to a partially authorized reader. Payloads are typed and fetched
separately under current authorization. Reads return opaque, scope- and
audience-bound continuation tokens containing an authenticated cursor,
endpoint incarnation, recovery epoch, filter digest, and retention semantics.
They do not expose hidden counts. Expired cursors return a typed problem plus
authorized snapshot recovery options; the server never silently skips.

**events_read** returns authorized events strictly after a continuation token.
**events_wait** is a
bounded long poll or equivalent subscription. Filters can narrow visible
events but never widen visibility. Participant inboxes are authorized
projections over the ledger, not a second queue authority.

Timing and traffic volume may still leak through an authorized live channel;
high-sensitivity deployments may quantize or pad delivery and must state the
remaining traffic-analysis risk.

### 14.5 Surfaces

| Surface | Purpose | Explicitly absent |
|---|---|---|
| governance | Authenticated human and charter-authorized governance actions. | Participant authorship impersonation; raw effect drivers. |
| candidate | Refuse or accept one exact MembershipOffer and propose own initial self-policies under one onboarding fence. | Standing use, Positions beyond its acceptance, Pledges, Mandates, context search, tools, effects, children, general participant operations. |
| participant | Proposals, positions, self-policy, Calls, Pledge and Assembly participation, ActivityStream operations, Deliveries, Engrams, visible events. | Human-authority decisions, standing expansion, raw credentials, peer dispatch. |
| runtime | Episode claim/start/checkpoint/yield/result, measured usage, enforcement evidence, permit consumption handoff. | Society admission, proposal positions, pledge assent, charter or Mandate issue. |
| projection | Authorized snapshots, cursored events, timeline and health reads. | All mutations. |
| admin | Backup, restore, diagnose, operational hold, key and service configuration. | Society decisions, participant positions, Pledge formation, acceptance. |

No surface dominates another semantically. An infrastructure administrator may
stop a process or hold the service but cannot use admin to author a participant
or governance record.

### 14.6 Operation catalog

The catalog, registry, record shapes, and transition semantics in this document
are normative design. A protocol release additionally publishes generated exact
JSON Schemas for every request/result/event; compatibility cannot be claimed
from this prose alone.

| Family | Operations |
|---|---|
| negotiation | hello, protocol_info, feature_info |
| society | society_prepare, society_bootstrap, society_show, society_hold, society_release, society_dissolve |
| charter | charter_propose, charter_position, charter_finalize, charter_history |
| participants | participant_propose, membership_offer, membership_offer_revoke, onboarding_offer, participant_admit, participant_show, participant_suspend, participation_cease, participant_retire, manifestation_propose, manifestation_admit, manifestation_disable, assent_policy_adopt, assent_policy_revoke, activation_policy_adopt, activation_policy_revoke, continuity_root_update |
| candidates | membership_refuse, membership_accept, candidate_self_policy_propose |
| control | control_domain_propose, control_domain_position, control_domain_finalize, control_domain_merge |
| procedures | procedure_propose, procedure_position, procedure_finalize, procedure_hold, procedure_release |
| assemblies | formation_start, formation_revise, assembly_propose, assembly_position, assembly_finalize, assembly_hold, assembly_reform, assembly_withdraw, assembly_dissolve, collective_policy_propose, collective_decision_finalize |
| endeavors | endeavor_propose, endeavor_position, endeavor_finalize, endeavor_hold, endeavor_release, endeavor_close |
| calls and pledges | call_open, call_withdraw, pledge_propose, pledge_position, pledge_finalize, pledge_amend, pledge_resume, pledge_relinquish, delivery_submit, delivery_withdraw, review_record |
| mandates | mandate_prepare, mandate_position, mandate_issue, mandate_derive, mandate_hold, mandate_revoke, standing_mandate_prepare, standing_mandate_position, standing_mandate_issue, standing_mandate_hold, standing_mandate_revoke |
| acts | act_intent_prepare, act_intent_position, act_intent_finalize, act_intent_cancel, execution_permit_consume |
| disputes | dispute_raise, dispute_position, dispute_hold, dispute_resolve, appeal_raise, appeal_position, appeal_resolve |
| activities | activity_open, activity_show, activity_hold, activity_close, wake_intent_submit, wake_intent_withdraw, episode_request, continuation_write |
| runtime | onboarding_episode_claim, onboarding_compute_permit_consume, onboarding_episode_complete, placement_admit, episode_claim, episode_start, checkpoint_commit, episode_yield, episode_complete, episode_fail, usage_report, effect_outcome_admit |
| knowledge | engram_propose, engram_admit, engram_read, engram_search, engram_attest, engram_hold, engram_retire, context_manifest_show |
| classification | classification_overlay_propose, classification_mapping_propose, outbound_classification_propose, classification_position, classification_finalize, classification_revoke |
| privacy lifecycle | erasure_request, erasure_position, erasure_finalize, erasure_execute, erasure_verify |
| budgets | budget_show, budget_reservation_show, usage_settlement_show, budget_reconcile |
| events | snapshot_get, events_read, events_wait, event_payload |
| host integration | kovee_endeavor_form |
| recovery | idempotency_result, external_command_result_query, external_command_terminalize, effect_reconcile, cursor_recover, recovery_checkpoint_show |
| administration | operational_hold, operational_release, diagnose, backup, restore, key_configure, service_configure |

Preparation and finalization are separate. The server prepares canonical
subjects and required seats. Position operations fill only the authenticated
actor's eligible seat. Finalization checks the complete set and cannot insert a
missing position.

### 14.7 Deny-by-absence authority registry

The normative registry key is `(operation, surface)`. A pairing not listed below
is forbidden even if the credential has a broader service role. “Stage” means a
client may retain signed inert bytes offline; the authoritative mutation occurs
only after online authentication and complete dependency revalidation.

Dependency abbreviations are: `E` endpoint incarnation/recovery/Society/Charter;
`P` principal or Participant binding/Standing/self-policy/control domain; `A`
Assembly/decision/position snapshot; `O` exact governed object and revision; `M`
complete Mandate chain; `B` budgets/meters; `D` visibility/classification/
erasure/disclosure; `F` Activity/Episode/host fences; and `X` Kovee or Akson
source bindings. All scopes are server-derived.

| Exact operation(s) | Surface | Allowed authenticated actor | Required closure | Fence and minimum assurance | Offline |
|---|---|---|---|---|---|
| hello, protocol_info, feature_info | each advertised surface | bounded pre-auth client or actor valid for that surface | endpoint/version only | parser/rate limits | no |
| society_prepare, society_bootstrap | governance | source-qualified human principal filling bootstrap sovereign seat | E,P,D,B | fresh phishing-resistant challenge; endpoint incarnation | no |
| society_hold, society_release, society_dissolve | governance | human principal filling exact current human-authority seat | E,P,O,A | fresh challenge; current revision | no |
| society_show, charter_history, participant_show, activity_show, budget_show, budget_reservation_show, usage_settlement_show, context_manifest_show, snapshot_get, events_read, events_wait, event_payload, recovery_checkpoint_show | projection | authorized principal, Participant, or narrow projection service | E,P,O,D and X when projected | proof-of-possession; opaque cursor where applicable | no |
| charter_propose, participant_propose, control_domain_propose, procedure_propose, formation_start, formation_revise, assembly_propose, endeavor_propose, call_open, pledge_propose, pledge_amend, dispute_raise, appeal_raise, engram_propose, classification_overlay_propose, classification_mapping_propose, outbound_classification_propose, erasure_request, collective_policy_propose | participant | current Participant acting only as itself | E,P,O,D; plus A/M/B where subject requires | binding epoch; stage only | stage |
| charter_position, control_domain_position, procedure_position, classification_position, erasure_position | governance | actor for the exact prepared human/governance seat | E,P,A,O,D | exact subject; fresh challenge when human/high risk | stage |
| assembly_position, endeavor_position, pledge_position | participant | Participant for its exact eligible seat | E,P,A,O,M,D | exact subject, binding and Assembly epoch | stage |
| charter_finalize, participant_admit, manifestation_admit, control_domain_finalize, control_domain_merge, procedure_finalize, procedure_hold, procedure_release, classification_finalize, classification_revoke, erasure_finalize | governance | governance caller authorized to request deterministic finalization; caller authors no missing seat | E,P,A,O,D and B/M as applicable | current snapshot; fresh challenge for reserved action | no |
| assembly_finalize, collective_decision_finalize, endeavor_finalize, pledge_finalize | participant | any current Participant authorized to request deterministic finalization; caller authors no missing seat | E,P,A,O,M,B,D | snapshot/epoch CAS | no |
| membership_offer, onboarding_offer, membership_offer_revoke, participant_suspend | governance | exact decided governance actor | E,P,A,O,B,D | current decision/revision; offer revocation advances candidate fence; onboarding has no general effect and at most one exact compute intent | no |
| membership_refuse, membership_accept, candidate_self_policy_propose | candidate | candidate actor for the exact MembershipOffer only | endpoint, offer, candidate/Manifestation/control-domain binding, onboarding fence, B,D,X | sender-constrained candidate proof; exact subject | no |
| participation_cease, participant_retire, assembly_withdraw | participant | affected Participant only | E,P,O,A | binding epoch; cease/withdraw cannot be delegated | no |
| manifestation_propose, manifestation_disable, assent_policy_adopt, assent_policy_revoke, activation_policy_adopt, activation_policy_revoke, continuity_root_update | participant | owning Participant only | E,P,O,D | sender-constrained channel and binding epoch | stage except disable/revoke: no |
| assembly_hold, assembly_reform, assembly_dissolve, endeavor_hold, endeavor_release, endeavor_close, call_withdraw, pledge_resume, pledge_relinquish, delivery_withdraw, review_record | participant | exact Participant or collective authorized by the governing subject | E,P,A,O,M,B,D | subject revision/epoch; fresh challenge if policy requires | no |
| mandate_prepare, mandate_derive, standing_mandate_prepare | participant | proposed grantee or expressly authorized issuer acting only in its scope | E,P,A,O,M,B,D | preparation trace; parent revisions | stage for prepare; derive no |
| mandate_position, standing_mandate_position | participant | Participant for its exact prepared authority/resource-owner seat | E,P,A,O,M,B,D | exact subject, seat and binding epoch | stage |
| mandate_position, standing_mandate_position | governance | human principal for its exact prepared human-authority seat | E,P,A,O,M,B,D | exact subject; fresh challenge as required | stage |
| mandate_issue, mandate_hold, mandate_revoke, standing_mandate_issue, standing_mandate_hold, standing_mandate_revoke | governance | exact issuer/human-authority actor under decided rule | E,P,A,O,M,B,D | current complete chain; fresh challenge for root/standing issue | no |
| act_intent_prepare | participant | requesting Participant or collective Manifestation inside current executive policy | E,P,A,O,M,B,D,F,X | field-complete PreparationTrace; subject revision/fences | stage |
| act_intent_position | participant | Participant for its exact prepared participant/resource-owner seat | E,P,A,O,M,B,D,F,X | exact intent digest, seat and binding epoch | stage |
| act_intent_position | governance | human principal for its exact prepared human-authority seat | E,P,A,O,M,B,D,F,X | exact intent digest; fresh challenge as required | stage |
| act_intent_finalize | participant | current Participant requesting deterministic non-root finalization; authors no seat | E,P,A,O,M,B,D,F,X | exact active slot snapshot and revision CAS | no |
| act_intent_finalize | governance | governance caller requesting deterministic finalization; authors no seat | E,P,A,O,M,B,D,F,X | exact active slot snapshot; fresh challenge for reserved action | no |
| act_intent_cancel | participant | original requester while intent is unconsumed, or exact cancellation grantee | E,P,A,O,M,B,D,F,X | current intent revision; cannot claim effect rollback | no |
| act_intent_cancel | governance | exact decided cancellation authority | E,P,A,O,M,B,D,F,X | current intent/effect revision; fresh challenge when required | no |
| dispute_position, appeal_position | participant | Participant for its exact resolver/affected-party seat | E,P,A,O,M,D | exact dispute/appeal subject and eligibility snapshot | stage |
| dispute_position, appeal_position | governance | human principal for its exact resolver/human-authority seat | E,P,A,O,M,D | exact subject; fresh challenge as required | stage |
| dispute_hold, dispute_resolve, appeal_resolve | governance | exact resolver or governance caller requesting deterministic finalization | E,P,A,O,M,B,D,F | current positions, evidence, deadline and target revision | no |
| activity_open, activity_hold, activity_close, wake_intent_submit, wake_intent_withdraw, episode_request, continuation_write, delivery_submit | participant | owning Participant/Manifestation, or collective channel inside executive policy | E,P,A,O,M,B,D,F | participant and generation fence; exact episode fence when cited | stage only for activity proposal; otherwise no |
| episode_claim, episode_start, checkpoint_commit, episode_yield, episode_complete, episode_fail, usage_report | runtime | workload identity bound to exact Episode/Manifestation | E,P,O,M,B,D,F,X | mTLS/attested workload; Byom and host fences | no |
| onboarding_episode_claim, onboarding_episode_complete | runtime | candidate workload bound to exact offer and proposed Manifestation | endpoint, offer, candidate binding, OnboardingComputeReceipt when hosted, B,D,F,X | mTLS/attested workload; one offer fence | no |
| onboarding_compute_permit_consume | runtime | Kovee model broker bound to exact OnboardingComputeIntent | endpoint, Society decision, candidate/Manifestation, final provider/disclosure manifests, B,D,F,X | workload mTLS; exact one-shot key and onboarding fence | no |
| placement_admit | runtime | narrow Kovee placement adapter bound to exact ResourceAllocation | E,P,O,M,B,D,F,X | source binding, exact Kovee placement revision and fences | no |
| execution_permit_consume | runtime | trusted host effect service bound to exact prepared host Effect | E,P,A,O,M,B,D,F,X | workload mTLS, exact one-shot key, dual fences | no |
| effect_outcome_admit | runtime | narrow trusted effect-admission adapter | E,O,B,D,X | source receipt verification; stable key | no |
| engram_admit, engram_attest, engram_hold, engram_retire | participant | exact locally authorized Participant/reviewer | E,P,A,O,D | current admission/lifecycle subject | no |
| engram_read, engram_search | projection | authorized principal or Participant | E,P,O,D | scope-bound query; no hidden counts | no |
| budget_reconcile, effect_reconcile | governance | exact reconciliation seat; infrastructure service may prepare evidence only | E,P,A,O,M,B,D,X | fresh challenge for ambiguous local consequence or late-result quarantine release | no |
| kovee_endeavor_form | governance | source-qualified human principal through an exact Kovee delegated-principal channel, acting for its already admitted bound human Participant and personally filling the sole computed formation seat | E,P,A,O,B,D,X plus active Society, pinned Realm/Byom binding and source ContextBundle | fresh phishing-resistant attempt proof over stable command/idempotency domain; exact formation intent, binding revision/epoch and slot snapshot | no |
| external_command_terminalize | governance | same source human principal through a current lineage-authorized Kovee delegated-principal channel | E,P,O,X plus exact historical/current idempotency domain, command, recovery binding and RestoreLineage when applicable | fresh phishing-resistant proof; locks idempotency and authority-journal heads; can deny future execution but never execute | no |
| idempotency_result, cursor_recover | originating surface | same actor/channel class and idempotency or cursor audience | original closure plus E,P,D | same sender binding; never re-executes | no |
| external_command_result_query | projection | narrow Kovee recovery workload bound to a current recovery binding and exact formation intent | current E,O,X plus original source-principal/actor-binding ref, command and idempotency-domain digests; RestoreLineage for historical target | workload mTLS; exact read-only query scope; never submits to old incarnation | no |
| operational_hold, operational_release, diagnose, backup, restore, key_configure, service_configure | admin | infrastructure administrator with separate admin identity | endpoint/host policy only; no Society authorship | mTLS plus operator quorum where configured | no |
| erasure_execute, erasure_verify | admin | narrow retention executor bound to exact ErasureRequest authorization; administrator cannot change target | endpoint witness, E,A,O,D, retention rules, key and external-copy state | workload mTLS, authority-journal and erasure-journal receipts | no |

No offline object is a Position, assent, Decision, Mandate, reservation, lease,
or permit until the online owner accepts it. A runtime identity never crosses to
participant or governance, and an administrator never crosses from availability
control into Society authorship.

### 14.8 Closed state-transition specification

Within each machine row, the listed sources and targets are closed; an unlisted
transition is invalid. The release's generated machine descriptors are the
exhaustive operation-level form and MUST map one-to-one to 100% of mutating
catalog operations and named internal transitions. Specification CI fails on a
missing operation, state, actor/surface registry key, lock, closure category,
reservation action, fence effect, journal behavior, event, or crash result.

Each successful row stages the new revision or immutable record, idempotency
result, applicable Byom reservations, dependency digest, audit event, and outbox
item as one SQL set, then passes the synchronous authority-journal/finalize
protocol in section 15.3 before becoming visible. Positions submitted before
finalization remain separate immutable inputs; finalization locks their exact
heads. Cross-owner Kovee/Akson steps occur only through the named saga and never
inside a claimed Byom transaction.

| Machine | Allowed source → target / operation | Actor and locked dependencies | Atomic effects and fencing | Crash result |
|---|---|---|---|---|
| Society | absent → forming → active / prepare, bootstrap | bootstrap human; endpoint witness, source identity, initial Charter/classification/budgets | one genesis set and event | none or complete genesis |
| Society | active ↔ held / hold, release | current human decision; Society/Charter/recovery | hold increments authority fence; release creates new dependency revision | old work remains fenced |
| Society | active or held → dissolving → dissolved / dissolve | dissolution decision and retention/key plan | fence all new authority; terminal record after disposition ledger | resumes dissolving; never auto-completes |
| CharterRevision | absent → active at bootstrap; active → superseded by new active revision / propose, positions, finalize | current amendment rule, human seats and exact subject | new Charter and GovernanceDecision; all changed dependencies re-evaluate | old revision never mutates or revives |
| MembershipOffer/Standing | absent → offered / membership_offer; offered → onboarding / onboarding_offer; offered/onboarding → accepted / membership_accept; accepted → admitted plus active Standing / participant_admit; offered/onboarding/accepted → refused / membership_refuse; offered/onboarding/accepted → revoked / membership_offer_revoke; offered/onboarding/accepted → expired / server time | governance decision plus affected candidate/Participant channel; only candidate authors refusal/acceptance; admit/refuse/expiry CAS the same offer revision | onboarding has one zero-general-effect fence and optional one-shot compute receipt; admission binds current acceptance and Standing; retraction cites prior acceptance; refusal/revoke/expiry advances fence, closes candidate channel, and invalidates unused compute/self-policy inputs | silence and stale acceptance expire; exact refusal retry returns its receipt; no terminal offer can later admit |
| OnboardingActivationOffer | absent → offered / onboarding_offer; offered → active / onboarding_episode_claim or onboarding_compute_permit_consume; offered/active → completed / onboarding_episode_complete; offered/active/completed → refused / membership_refuse; offered/active/completed → revoked / membership_offer_revoke; offered/active/completed → expired / server time | exact offer decision, candidate/onboarding workload, candidate only for refusal | one fence; at most one compute use; completion is evidence only; refusal/revoke/expiry advances fence and closes candidate workload/channel | no state authors acceptance; terminal fence survives retry |
| Onboarding compute | absent → prepared → authorized → consumed → completed, failed, or ambiguous / offer/finalize, permit consume, runtime completion | Society decision, exact candidate/Manifestation, final provider/disclosure manifests, budget and fence | one-shot receipt; output reaches candidate surface only | runtime output never becomes acceptance |
| Standing | active → suspended, expired, superseded, or ceased / suspend, expiry, replacement, cease | current Standing/binding; cease actor is self | minimum revocation set fences positions, contexts, Mandates, Episodes, effects | no partial revocation |
| Participant | proposed → active → suspended or retiring → retired / propose, admit, suspend, cease/retire | candidate acceptance, Standing decision, current binding | binding epoch increments on identity/control change; channels fence | historical authorship survives |
| Manifestation | proposed → active → disabled or superseded / propose, admit, disable/replace | owning Participant proposal and exact admission/compatibility decision | new digest never inherits authority silently | active work pauses or uses recorded selector |
| Candidate self-policy | absent → proposed / candidate_self_policy_propose; proposed → activated / participant_admit; proposed → rejected / membership_refuse or membership_offer_revoke; proposed → expired / server time | exact candidate channel, onboarding fence and admission subject | activation preserves adoption/control-domain provenance; every terminal offer transition fences the candidate credential | never active before Standing |
| Participant assent policy | absent → active; active → revoked, superseded, or expired / assent policy adopt/revoke/replace/server time | owning Participant channel plus stricter Charter/host ceiling | derived assent ordinals counted atomically; root mode propagates | no Charter fallback/reactivation |
| Participant activation policy | absent → active; active → revoked, superseded, or expired / activation policy adopt/revoke/replace/server time | owning Participant channel plus stricter Charter/host ceiling | policy-use ordinals counted; queued admissions fence on revoke | no scheduler fallback |
| ContinuityRoot | absent → active; active → active(new revision), sealed, or retired; sealed → retired / continuity_root_update | owning Participant channel, compatibility/classification/retention | current head CAS; old state refs remain private evidence | Society never authors or unseals it |
| Continuation head | absent at Activity generation revision zero → current revision one; current revision N → current revision N+1 / continuation_write | owning Participant/Manifestation, exact Activity generation, expected head/predecessor, current Episode fence when episodic | append immutable Continuation and CAS the one generation head; no automatic merge or alternate current branch | one concurrent writer wins; stale writer returns current opaque head and cannot append |
| Position/Decision | absent → active → withdrawn or superseded / position operation; exact active snapshot → immutable Decision / finalize | seat owner for Position; deterministic finalizer | one current seat head; Decision and slot snapshot immutable | prior Position inputs remain |
| ProcedureDefinition | proposed → active; active ↔ held; active/held → superseded or retired / procedure operations | exact procedure decision and complete closure | dependent pending decisions invalidate/hold | BDPL body never mutates in place |
| ProcedureSeedSlot/Admission | absent → pending / internal procedure_seed_slot_create; pending → admitted / internal procedure_seed_admit; pending → terminal_unavailable / internal procedure_seed_mark_unavailable | exact subject/eligibility snapshot, pre-adopted SeedPolicy, pinned source/key/input/round or committer/deadline set and independent evidence | unique subject slot and ordinal one; exact seed bound once; commit-reveal includes every reveal/missing sentinel plus post-reveal beacon | no source switch, favorable-round scan, abort/retry, or second seed for subject |
| ControlDomainRevision | proposed → active; active → merged or superseded / control-domain finalize/merge | governance decision, evidence policy and transitive closure | merge invalidates pending decisions and holds dependencies | no split without new evidence/revisions |
| Classification overlay | proposed → active; active → revoked, superseded, or expired / classification operations/server time | human decision, Kovee vocabulary binding where hosted | contexts/disclosures/dependencies invalidate | no label spelling comparison |
| Inbound ClassificationMappingRevision | proposed → active; active → revoked, superseded, or expired / classification operations/server time | destination Society human decision and source/binding epochs | new admission/materialization fences on change | source/peer cannot author it |
| OutboundClassificationRevision | proposed → active; active → revoked, superseded, or expired / classification operations/server time | source Society human decision and remote/contract vocabulary | new dispatch fences on change | never asserts remote local label |
| Akson classification phase chain | absent → signed request → signed acceptance → signed result → local admission / consumed Kovee/Akson dispatch, remote Akson acceptance, remote Akson result, verified-result effect_outcome_admit | exact predecessor, byte-equal ActIntent/MandateUse/consumption receipt/execution+delivery keys, profile fields, endpoint/key epochs, result/Kovee receipt/final EOA head, admitting mapping ownership and capability evidence | each wire phase immutable and independently signed; only verified_result atomically creates matching final EOA revision and authority-journaled local admission | no phase prediction/backfill; ambiguous/pre-result failure creates no classification; no cross-intent, Society, result, receipt, key, or mapping splice |
| Kovee Akson receipt head | absent → ambiguous or final at revision 1; ambiguous → final at revision N+1; final has no successor / Kovee-owned source commit | same receipt/effect and complete phase/intent/key identity; predecessor and reconciliation pointer equal current digest | Kovee alone appends signed receipt and CASes its head; Byom state untouched | one successor wins; no fork, skipped revision, changed identity, or final reopen |
| Akson dispatch outcome admission | no EOA → failed, ambiguous, or final succeeded/failed; ambiguous → final failed or succeeded / effect_outcome_admit with closed current Kovee receipt union | exact current Kovee Effect/receipt head, host protocol/endpoint, ActIntent, MandateUse, consumption receipt, keys and source evidence; reconciliation cites exact ambiguous receipt and EOA head | pre_result_failed, ambiguous, and verification_rejected create no classification; every final branch fences an active disposition head; verified_result also creates/reconciles classification admission atomically; Byom CAS is independent of earlier Kovee CAS | changed branch or receipt conflicts; final never reopens; non-terminal rejection stays ambiguous |
| Collective executive policy | proposed → active; active → held, superseded, expired, or epoch-fenced / collective decision/reform/server time | exact Assembly decision, epoch, Manifestation, Mandate and dependencies | collective channel and derived self-policies fence | no reserved power becomes executive |
| Assembly | FormationProcess open → proposed → active / revise, propose, finalize | final formation snapshot, all member assents, graph closure, decision | GovernanceDecision, Assembly epoch, optional collective identity/policy and events | none or exact formed set |
| Assembly | active → held or reforming → active(new epoch) / hold, reform | current epoch, withdrawals, new formation snapshot | old epoch, channels, decisions and default continuity fenced | remains held/reforming until exact finalize |
| Assembly | active, held, or reforming → dissolved / dissolve | dissolution decision and obligation disposition | collective channel and future acts fenced | never transfers Pledges silently |
| Endeavor | proposed → active / finalize | proposal/decision/source/classification/budget roots | exact Endeavor and accounts | none or complete |
| Kovee atomic Endeavor formation | absent → active Endeavor / kovee_endeavor_form; unresolved idempotency domain → result unchanged or non-reexecuting tombstone / external_command_terminalize | delegated source human already admitted to existing Society, pinned Realm/Byom binding and ContextBundle, sole computed formation seat; terminalize also locks exact domain/journal and lineage | source Position, GovernanceDecision and Endeavor commit as one authority-journaled set; terminalization cannot execute and races formation on one domain lock | none or one complete signed result, or one terminal tombstone; Society bootstrap is never part of this machine |
| Endeavor | active ↔ held; active → reviewing; reviewing → fulfilled or failed; active/held/reviewing → abandoned or dissolved / hold, release, close | governing decision, exact acceptance evidence | fence or release affected activities; terminal outcome event | no dashboard-derived close |
| Call | absent → open; open ↔ forming; open/forming → satisfied, withdrawn, or expired | opener for withdraw; otherwise exact linked proposal/outcome/server time | no obligation or budget is created by Call state | retry returns same state |
| Pledge | proposal + final slots → active or waiting / finalize | exact proposal, current Position heads, decision, budgets, optional Mandate | Pledge, locked slot snapshot, Byom reservations, initial PledgeWorkstream and optional Mandate | prior Positions remain; derived set all-or-none |
| Pledge | active/waiting → underway → submitted; submitted → fulfilled, revision_requested, rejected, or disputed; revision_requested → underway / activity, delivery, review, pledge_resume | pledgor channel; exact Pledge/Delivery/reviewer decision | each resume starts a new Activity generation under unchanged terms; no effect implied | late result stays on old delivery/revision |
| Pledge | nonterminal → superseded, relinquished, canceled, failed, or expired / amend, relinquish, decision, expiry | successor CAS/cancellation terms/server time | incompatible Activities and unstarted effects fenced; reservations settled conservatively | one current successor only |
| Mandate | prepared/positions → active / issue; active → held, exhausted, revoked, expired, or superseded / matching operation | exact decision, complete parent closure, budgets | child delegated quantity conserved; use ordinal slots created on consumption | no bearer authority escapes commit |
| StandingMandateRevision | prepared/positions → active / prepare, positions, issue; active → held, revoked, superseded, or expired / hold, revoke, successor, server time | exact human decision, BPA selectors, aggregate budgets/rates and dependencies | each match derives a separate exact MandateUse; circuit breaker holds new uses | ambient auto-approval never appears |
| ActivityStream | absent → ready; ready ↔ active or waiting; any nonterminal → held, completed, failed, or canceled / activity operations | owning Participant, self-policy, Mandate, budgets, optional Pledge | generation CAS; hold/cancel fences new Episodes | prior outputs remain evidence |
| WakeIntent | absent → submitted → withdrawn or expired / submit, withdraw, server time | direct Participant request or exact ActivationPolicy use ordinal | immutable cause and provenance; revoke policy blocks new intent/admission | no event/cron/host can author it |
| ActivationAdmission | no admission → admitted or denied; admitted → revoked or expired / internal activation_admit, dependency invalidation, server time | deterministic kernel over committed WakeIntent and complete closure | one decision per WakeIntent revision; no budget/placement yet | retry returns same admission |
| ResourceAllocation | absent → prepared → reserved → bridged; any nonterminal → released, uncertain, or revoked / internal resource_allocate and Kovee budget saga | admitted activation, Mandate/rate counters, Byom and Kovee budgets | exact reservations and bridge refs; queue remains blocked until bridged | unknown bridge remains uncertain |
| Placement | absent → Kovee placed → Byom admitted → started, released, or failed / Kovee placement plus placement_admit | Kovee source binding, eligible Manifestation, allocation and fences | Kovee owns placement; Byom stores source admission only | no host-selected Participant change |
| Episode | prepared → eligible → queued / request and resource saga | owner intent, activation admission, Byom then Kovee reservation | queue only after both reservations; no lease yet | uncertain bridge stays unqueued/held |
| Episode lease | no current claim or expired head → leased → running / claim, start | runtime binding, current Episode/generation and both host fences | claim increments fence and appends immutable attempt; CAS head | old worker is stale |
| Episode | running → yielded, completed, waiting, interrupted, failed, canceled, or ambiguous / runtime operation, hold, expiry | current attempt/fence/dependencies | EpisodeCompletion/event and conservative settlement; Delivery remains separate | unknown external use is ambiguous |
| ActIntent | absent → prepared → awaiting_decision → authorized / prepare, positions/finalize | preparation trace, complete decision/dependency closure | exact subject and one-shot use slot | none or recoverable authorized intent |
| ActIntent | authorized → consumed → executing → succeeded, failed, or ambiguous / consume, host attempt, outcome admit | current host Effect, Mandate, budgets, dual fences | MandateUse once; source-qualified admission once; settle conservatively | never blindly repeats non-idempotent effect |
| ActIntent source reconciliation | ambiguous → succeeded or failed / effect_outcome_admit | exact ambiguous EOA head plus independently committed/signed final host Effect/receipt successor; no judgmental field | source EOA revision and Byom source-head CAS; conservative budget settles once; any active disposition head becomes source_advanced and late result use is quarantined; host head was already committed by host owner | remains ambiguous on stale/unknown/conflicting source; no GovernanceDecision invented and no disposition can block the source fact |
| ActIntent governance disposition | source state remains ambiguous; no disposition → active_ambiguous, or current active_ambiguous → replacement active_ambiguous / effect_reconcile; source_advanced → resolved_late / effect_reconcile after final source admission | exact EOA and independent disposition heads plus exact GovernanceDecision; late resolution also locks verified result/classification and complete use dependencies | appends only EffectGovernanceDisposition and CASes its own head; initial result use is unavailable; late result is quarantined until an exact late-source decision releases it | source EOA/ActIntent never changes on this operation; stale decision remains evidence and never relabels host Effect |
| ActIntent | prepared/awaiting/authorized → denied, expired, or canceled | decision/server time/cancel authority | unspent reservations released only when unambiguous | terminal; replay does not execute |
| Delivery/Review | no Delivery → submitted → withdrawn or superseded; submitted → immutable Review outcome | pledgor for Delivery; exact reviewer seat for Review | records exact outputs/terms/evidence; Pledge transition separate but same review transaction | no runtime or verifier authors acceptance |
| Dispute | absent → open → held or deliberating → resolved, dismissed, or expired / raise, hold, positions, resolve, server time | raiser plus pinned resolver snapshot and interim-hold rule | original target immutable; resolution GovernanceDecision appended | no automatic effect reversal |
| Appeal | absent → open → held or deliberating → affirmed, modified, remanded, dismissed, or expired / appeal, positions, resolve | eligible appellant and distinct pinned appeal resolver | prior resolution retained; exact successor/remand refs | no silent reopen or overwrite |
| Engram | absent → proposed or external → quarantined; proposed/quarantined → admitted / propose, import, admit; admitted → held / hold; held → admitted / fresh admit; admitted/held → retired / retire; admitted → superseded / admit successor; non-erased → erased / erasure flow | author source, exact local admission/lifecycle decision, visibility/classification/retention | portable bytes immutable; local trust never travels as content | indexes remain non-authoritative |
| Erasure | absent → requested → awaiting_decision → authorized → executing → verified or partial_external; pre-execution → denied; execution → failed / erasure operations | requester, exact data-owner/human decision, narrow retention executor | payload/key destruction, typed residual commitments and both synchronous journals | restore remains sealed until journal reapplied |
| BudgetReservationSet | prepared → reserved → bridged → settling → settled or released; any post-reserve → uncertain | trusted account engine, Kovee bridge, trusted meter | conservation equation and stable settlement key under account locks | unknown quantity never returns to remaining |
| ExternalBudgetBridge | absent → requested → confirmed, denied, or uncertain; confirmed → settled or released / Kovee saga | exact Byom reservation and Kovee endpoint/binding/stable key | source ref/revision/digest persisted; Episode queues only if confirmed | timeout queries; unknown remains uncertain |
| UsageSettlement | absent → measured → final, conservatively_maxed, or uncertain; uncertain → final/conservatively_maxed / trusted meter/reconcile | unique reservation/stable-key head, pricing and meter evidence | immutable revision plus head CAS; account buckets change once | changed request id cannot double settle |
| EffectOutcomeAdmission | absent → succeeded, failed, or ambiguous / effect_outcome_admit; ambiguous → succeeded/failed / effect_outcome_admit with final source successor | verified Kovee Effect/receipt and unique intent/stable-key source head; operation has no judgmental field | immutable source revision and EOA head CAS; host source CAS always precedes source admission and is never shared; active disposition head is fenced without blocking admission | final never returns to ambiguous; no GovernanceDecision or local judgment can enter this source record |
| EffectGovernanceDisposition | absent or active_ambiguous → active_ambiguous / effect_reconcile while source remains ambiguous; active_ambiguous → source_advanced / internal effect_outcome_admit fence; source_advanced → resolved_late / effect_reconcile | independent EOA and disposition heads, exact GovernanceDecision, and final classification/use dependencies for late release | immutable disposition revision plus its own head CAS; source arrival only fences the active judgment; verified late bytes are classified but quarantined until resolved | never advances, replaces, or relabels the source head; source arrival cannot be blocked by a local disposition |
| ContextManifest | absent → immutable prepared / Episode/context preparation; materialization is a rechecked read, not a transition | exact audience, purpose, ordered sources, classifications and closure | typed commitment fixed; revoked dependency makes it unusable | never silently omits/replaces an item |
| EngramAttestation/Review/GovernanceDecision | absent → immutable record / exact catalog finalization operation | authenticated author/seat and exact subject snapshot | append-only attribution; dependent object transition is separately explicit | no update or overwrite path |
| Endpoint incarnation | active → sealed_diagnostic or retired; sealed_diagnostic → active only through reconciled new incarnation / startup, restore, recovery | external journal/audit/erasure checkpoints and operator recovery authority | keys/channels/epochs rotate; payload surfaces stay closed | mismatch cannot be skipped |
| Authority mutation journal | absent → SQL prepared → witness_unknown or witnessed → SQL finalized; unwitnessed → abandoned after proof / internal mutation protocol | exact prior external head, transition/idempotency digest and witness | no visible authority before finalize | every crash state follows section 15.3 |
| Operational hold | absent → active → released / operational_hold, operational_release | infrastructure administrator and exact host scope | availability fence only; no Society record authored | release rechecks all Society authority |
| Key/service configuration | absent → active → superseded or retired / key_configure, service_configure | separate administrator/operator quorum and endpoint policy | revisioned config; key retirement fences dependent channels | no in-place secret overwrite |
| Backup/restore | no snapshot → immutable backup; backup → verified restore candidate → new incarnation or rejected / backup, restore | admin identity, external journals, encryption/erasure checkpoints | restore never replaces the active incarnation in place | mismatch remains sealed diagnostic |

Implementations MUST publish machine-readable transition descriptors containing
the same source/target, authority-registry key, closure categories, locks,
reservation actions, fence changes, emitted event types, and crash outcome.
Critical Mandate, Pledge, Episode, ActIntent, Assembly, and budget machines are
model-checked for invariant preservation, dead transitions, replay, and crash at
every commit/external-call boundary. The machine-readable descriptors and model
checks, not additional undocumented daemon behavior, decide conformance.

### 14.9 Problems and limits

Stable problem kinds include:

~~~text
invalid                       unsupported_version
feature_unavailable           forbidden_surface
forbidden                     not_found
stale_revision                stale_binding
stale_assembly_epoch          stale_lease
idempotency_mismatch          position_ineligible
decision_incomplete           independence_conflict
authority_widening            mandate_held
admission_required            classification_unmapped
policy_conflict               policy_overflow
budget_exceeded               effect_ambiguous
authority_witness_unknown     endpoint_sealed
cursor_expired                unavailable
formation_requires_participation
external_command_not_terminalizable
internal
~~~

Problems do not disclose hidden object, participant, peer, path, policy, or
membership existence.

`formation_requires_participation` is a definite pre-commit rejection and
therefore returns the command's non-reexecuting tombstone. For a verified
terminalization target, ordinary prepared/in-flight/lineage blockers use the
typed `not_terminalizable` result so the no-op evidence is recoverable.
`external_command_not_terminalizable` is reserved for a request that cannot
establish a valid target/domain/proof shape at all; authorization failures still
use non-enumerating `forbidden`.

Initial limits are conformance-tested and revisioned:

- request envelope at most 256 KiB; response at most 1 MiB;
- larger content and artifacts move by immutable reference;
- identifiers at most 128 bytes; titles at most 4 KiB; participant prose
  entries at most 64 KiB;
- maximum 256 list items per mutation and 512 events per page;
- bounded JSON depth, node count, string length, Assembly depth, mandate depth,
  fanout, dependency count, context items, outputs, and evidence slots;
- independent caps for total transitive Assembly seats and edges, graph paths,
  dependency-closure entries, BPA/BDPL policy nodes and evaluation steps,
  restore-lineage hops, signature checks, decompressed bytes, and database work
  per request;
- bounded wait duration and one explicit page size on every list.

Quota is charged before signature verification, graph traversal, decompression,
or staging. Evaluator timeout fails closed. Graph caches key the complete graph,
control-domain, Standing, and Assembly revision set; partial-revision caches are
not authoritative.

Unknown namespaced extensions are preserved when safe but never influence
authority, classification, budgets, eligibility, or state. Unknown
safety-relevant values fail closed.

### 14.10 Bindings

- **Local JSON** uses separate Unix sockets for governance, candidate,
  participant, runtime, projection, and admin under a private runtime directory. Peer
  credentials and per-channel secrets bind the actor. Same UID alone has only
  the honest developer-profile claim.
- **Kovee HTTP/realtime** is the primary human and hosted-agent product binding.
  Kovee authenticates, delegates narrowly, and routes to the owning BPP surface.
- **MCP** is a participant/harness binding for authorized resources, proposals,
  Pledge operations, ActivityStream operations, and event cursors. It exposes no
  human-authority or admin tools and does not treat elicitation as approval.
- **Worker protocol** maps a Byom Episode to a Kovee Invocation or another
  conformant execution host with dual fencing.
- **Akson/A2A** is the only cross-sovereign wire. BPP is never tunneled to make a
  foreign process a local participant channel.

## 15. Reference architecture and persistence

### 15.1 Components

~~~text
human clients       agent participants       Kovee adapter
      |                     |                     |
      +---------- Byom Participation Protocol ---+
                            |
+------------------------------------------------------------------+
| byomd                                                            |
|                                                                  |
| identity/binding  society/charter  participant/assembly engine   |
| proposal/decision endeavor/pledge mandate/use engine             |
| activity/episode   budget/accounts Engram/context catalog        |
| event/snapshot     effect-consumption     source/admission adapters |
|                                                                  |
| SQL state + idempotency + event ledger + transactional outbox    |
+------------------------------------------------------------------+
              |                              |
  Kovee runtime/effects/dispatch       verified Akson source records
~~~

The modules are semantic ownership boundaries, not a requirement for one
microservice each. Co-location never permits an adapter to write another
module's state without its command validator.

### 15.2 Deterministic kernel

The trusted Byom kernel:

- authenticates and authorizes protocol operations;
- canonicalizes server-prepared subjects;
- evaluates structured charter and Mandate rules;
- checks required positions and independence domains;
- performs subset checks for derived authority;
- reserves budgets and ceilings;
- manages revisions, leases, fences, admission, idempotency, and events;
- issues and consumes exact one-shot authorization receipts.

It does not:

- call a model or semantic ranker;
- parse natural-language bodies for authority;
- choose an agent, plan, synthesis, review, or truth;
- execute a shell, tool, model, workspace apply, connector, or peer dispatch;
- hold Kovee provider credentials or Akson admin credentials.

### 15.3 Endpoint incarnation and rollback safety

`recovery_epoch` inside SQL is not sufficient because a backup can roll it back.
Every authority endpoint therefore has a synchronous non-rollbackable authority
journal, separate from periodic audit witnessing:

~~~text
AuthorityEndpointIncarnation {
  endpoint_id, incarnation_id, witnessed_generation,
  incarnation_public_key, previous_checkpoint_digest?,
  authority_journal_head_digest, database_checkpoint_digest,
  erasure_journal_checkpoint_digest,
  witness_set_ref, witness_receipts[], created_at,
  status: active | sealed_diagnostic | retired, digest
}

AuthorityMutationPending {
  transaction_id, endpoint_incarnation, society_id,
  operation, actor_binding_digest, idempotency_domain_digest,
  prior_journal_generation, prior_journal_digest,
  proposed_generation, transition_digest, result_digest,
  state: prepared | witness_unknown | witnessed | finalized | abandoned,
  created_at, digest
}

AuthorityJournalEntry {
  endpoint_id, endpoint_incarnation, generation,
  previous_entry_digest, transaction_id,
  operation, society_id, transition_digest,
  idempotency_domain_digest, erasure_effect_digest?,
  committed_at, entry_digest
}

AuthorityJournalReceipt {
  endpoint_id, endpoint_incarnation, generation,
  journal_entry_digest, witness_key_ref, signature,
  witnessed_at, digest
}
~~~

The journal generation and terminal hash live outside the restorable database in
a KMS/HSM monotonic facility, independently witnessed append-only service, or
operator-quorum store. Every authoritative mutation—including assent, decision,
Standing, Mandate, budget/rate use, permit consumption, reconciliation,
classification, retention/erasure, and revocation—uses this protocol:

1. A serializable SQL transaction revalidates dependencies and writes the full
   transition, idempotency result, reservations, events, and
   AuthorityMutationPending as **invisible and unusable** against the observed
   journal head. It commits no externally consumable permit or reply.
2. The witness atomically compare-and-swaps `(incarnation, prior generation,
   prior digest)` to the exact next AuthorityJournalEntry and returns a signed
   receipt. The transaction id and transition digest make this retry/query-safe.
3. A second SQL transaction verifies the receipt, marks that exact pending set
   finalized/visible, advances the local journal mirror, and makes its retained
   result readable. Only then may Byom return success, release a credential or
   permit, publish an event, or allow a dependent transition.

Crash semantics are closed. SQL prepare before witness leaves inert pending
state that is retried or abandoned after proving no journal entry. A witness
timeout is queried by transaction id and never guessed. Witness success before
SQL finalize is recovered by the exact receipt and finalized once. A competing
CAS requires complete dependency revalidation and a new proposed generation;
the old pending transition stays inert. SQL loss or rollback after witness
creates a journal/database mismatch and cannot be skipped or re-created under a
new transaction id. Batching is permitted only when every member stays invisible
until one exact batch entry is witnessed and the entire batch finalizes; partial
release is forbidden.

On every startup Byom compares the database mirror, external authority journal,
audit checkpoints, and erasure-journal checkpoint. Missing, older, conflicting,
or unavailable state starts `sealed_diagnostic`: all governance, candidate,
participant, runtime, projection, object, event, search, context, export, and
federation surfaces are closed. Only non-content health, witness comparison,
backup inventory, and authorized recovery diagnostics are exposed.

Restore creates a new incarnation key and witnessed generation, increments each
affected Society recovery epoch, rotates principal/Participant/runtime/service
channel credentials, closes live connections, and fences all prior attempts,
contexts, cursors, idempotency domains, positions, permits, Kovee bindings, and
Akson contracts. Endpoint incarnation and recovery epoch are direct fields in
those records, not merely implied dependencies. Ambiguous external effects are
reconciled before release. Before any surface opens, the restore also emits one
externally witnessed RestoreLineage per affected Society/incarnation hop. It
cites the predecessor authority-journal and idempotency checkpoints, proves the
old mutation keys/domain permanently fenced, and labels idempotency retention
`complete` only after every witnessed post-checkpoint row is reconciled.
Unprovable backup gaps are permanently `incomplete` or `unavailable`; an
operator cannot upgrade that label. The record enables only the narrow
historical query/terminalization semantics in section 16.3 and never validates
an old mutation request. A deployment without an independent monotonic journal
may advertise developer recovery only, never production rollback resistance.

### 15.4 Audit records and receipts

~~~text
AuditRecord {
  society_id, internal_sequence, event_id,
  previous_record_digest, endpoint_incarnation, recovery_epoch,
  actor_binding_digest, operation, subject_digest,
  result_kind, result_digest, dependency_digest,
  occurred_at, record_digest
}

AuditCheckpoint {
  society_id, first_sequence, last_sequence,
  prior_checkpoint_digest, terminal_record_digest,
  endpoint_incarnation, recovery_epoch, signed_at,
  signing_key_ref, signature, witness_receipts[], digest
}

PrivacyAccessRecord {
  society_id, internal_access_sequence, access_event_id,
  previous_access_digest, endpoint_incarnation, recovery_epoch,
  actor_binding_digest, operation,
  purpose_ref, query_or_scope_digest,
  result_object_count, result_bytes, outcome: allowed | denied | error,
  dependency_digest, occurred_at, record_digest
}

PrivacyAccessCheckpoint {
  society_id, first_access_sequence, last_access_sequence,
  prior_checkpoint_digest, terminal_access_digest,
  endpoint_incarnation, recovery_epoch, signed_at,
  signing_key_ref, signature, witness_receipts[], digest
}
~~~

Canonical AuditRecords chain every accepted mutation and denied consequential
consume. A signing key outside the database signs bounded checkpoints; signed
mutation receipts may be returned to affected Participants. The managed profile
periodically witnesses checkpoints outside the operator's ordinary database and
key domain and publishes witness lag and last anchored sequence. Without that
witness, the honest claim is retained-record internal consistency, not resistance
to a key-holding operator rewriting or truncating history.

Allowed and denied sensitive reads have a separate privacy-access chain with the
same signing, checkpoint, witness, rollback, and retention discipline. It covers
object/payload reads, search and count queries, context materialization, Engram
retrieval, event payloads, artifact/workspace fetch, disclosure/export, backup,
key use, restore, and admin access. It records actor, purpose, canonical query or
scope digest, result cardinality/bytes, dependencies, and outcome—never result
plaintext. Access logs are themselves classified, visibility-limited, rate-
protected, and unavailable to the actor merely because it generated them.
In the managed operator-resistant profile, sensitive plaintext or search results
are not released until the corresponding access record CASes into a separate
non-rollbackable access journal and its receipt is stored. Witness timeout fails
the read or recovers by access-event id; it never serves unlogged bytes.
Periodic signed checkpoints compact and publish that already synchronous chain,
so checkpoint cadence creates no unrecorded access tail. A developer profile may
offer only internal access logging and must say so.

### 15.5 Storage and transactions

SQLite is the personal reference store; PostgreSQL is the team reference store.
The SQL prepare transaction atomically stages:

- the validated domain transition;
- optimistic revision;
- idempotency request digest and replayable result;
- budget or ceiling reservations;
- dense internal per-Society event sequence and payload digest;
- transactional outbox work; and
- the exact AuthorityMutationPending record.

The staged set is invisible until the synchronous authority-journal receipt and
SQL finalize from section 15.3. No accepted mutation depends on a broker ack; it
does depend on that non-rollbackable journal CAS. An optional internal delivery
fabric carries wakeups and projection jobs only after finalization and is
entirely rebuildable.

Byom stores canonical Engram bytes in a Society-scoped content namespace. It
does not own Kovee artifacts. In Kovee-hosted mode, artifact versions, bytes,
uploads, grants, scans, and physical access remain Kovee records; Byom stores
only immutable source-qualified refs/digests plus Byom admission and visibility
decisions. A standalone deployment names a separate conformant artifact-provider
namespace with that provider as writer rather than silently assigning it to
byomd.

Before artifact or Engram upload, the owning provider reserves compressed and
expanded bytes. It streams with item, aggregate, ratio, and time caps; does not
automatically expand archives; isolates scanners; and garbage-collects invisible
staging after a bounded TTL. Content addressing uses Society-scoped encryption
and namespaces, with no cross-Society deduplication or existence oracle. Staging
bytes are never visible before digest, size, media type, malware/active-content,
secret, and policy checks complete.

### 15.6 Consistency model

One Byom Society has one authoritative home write boundary in v0.2. Team mode
may be highly available within that boundary but does not accept multi-master
Society writes. Read replicas and projections expose source cursor and lag.

Assembly and Pledge formation lock their proposal, exact subject digest,
required seats, active positions, governing Charter, Standing, budgets, and
dependencies. They commit all derived records or none.

Cross-service workflows use durable state machines and idempotent sagas. Unknown
outcomes remain held or ambiguous; timeouts never release a uniqueness slot
when an irreversible remote commit might exist.

### 15.7 Society re-homing and fork

The one-home rule is a consistency boundary, not a permanent operator claim.

~~~text
SocietyPortabilityCheckpoint {
  society_id, source_endpoint_incarnation, recovery_epoch,
  terminal_audit_checkpoint_ref, terminal_event_cursor,
  charter_and_classification_digests,
  object_catalog_digest, unresolved_effect_and_budget_digest,
  erasure_journal_checkpoint_digest, encrypted_export_ref,
  source_signature, witness_receipts[], created_at, digest
}
~~~

A lossless re-home requires a human-sovereign decision, operational hold, settled
or explicitly ambiguous effects and budgets, a signed/witnessed terminal
checkpoint, encrypted classified export, source retirement, new endpoint
incarnation and keys, participant-channel rotation, and revalidation of every
external binding. The old and new endpoint are never writable concurrently.

Any human may instead propose a fork from records it is authorized to export.
A fork has a new Society id and genesis Charter. Prior records remain cited
evidence; Standing, MembershipAcceptance, Mandates, budgets, decisions, trust,
and effect authority do not transfer. Participants choose whether to join. An
operator can still delay service or withhold plaintext it controls; signed
receipts and witnessed checkpoints make that censorship visible but cannot make
unavailable data reappear. Product claims distinguish complete re-home from a
partial evidentiary fork.

## 16. Kovee integration

### 16.1 Boundary

Kovee is where participants collaborate and where hosted execution occurs.
Byom is where consequential collective agency becomes governed. Neither is the
other's frontend or backend.

~~~text
Kovee owns                         Byom owns
--------------------------------  -------------------------------------------
Space and Branch                  Society and Endeavor
Contribution and Relation         Proposal, Position, and Decision
AttentionContract                 governed Episode eligibility
local Need/Offer/Formation        Call and governed Pledge formation
local_non_governed Commitment     Pledge
assistant Deployment             Participant Manifestation binding
Invocation and Attempt            ActivityStream and Episode
ContextAssembly                   admitted ContextManifest
runtime budget subset             Endeavor/Pledge/Mandate budget authority
effect driver and observation     ActIntent and semantic authorization
~~~

A Kovee local Commitment cannot become a Byom Pledge by relabeling or copying
state. It may be cited as source evidence for a PledgeProposal. The prospective
pledgor and every Byom authority seat still fill exact Byom positions. A future
cross-protocol receipt may avoid duplicate interaction only if it was created
for the identical Byom audience, subject digest, terms, actor, assurance, and
single use; current Kovee local assent has no such governed force.

### 16.2 Sage-to-Byom conceptual mapping

| Current Kovee/Sage term | Byom successor |
|---|---|
| Sage mission | Endeavor |
| canonical plan revision | Plan lens over Calls, Pledges, dependencies, and decisions |
| aspect | Usually a Pledge; an unowned aspect imports as a Call or legacy record |
| coordinator session | Optional steward Participant with ordinary Pledge and Mandate |
| Sage session | Participant ActivityStream history |
| turn | Episode |
| gate / validation | Server-prepared proposal, exact positions, decision, and ActIntent |
| standing gate rule | StandingMandateRevision |
| mission member role | Society Standing and Endeavor/Assembly seats |
| directory entry | Participant ProfileClaim plus source-qualified evidence |
| Engram | Engram, preserving the portable/local trust split |

This mapping is not wire compatibility. Current Kovee documents name Sage until
the Byom protocol and integration are reviewed and adopted. Kovee MUST NOT
advertise Byom compatibility through a private adapter that preserves the old
central coordinator semantics.

### 16.3 Forming an Endeavor from Kovee

Kovee owns these formation-recovery records:

~~~text
EndeavorFormationIntent {
  formation_id, revision,
  realm_id, project_id, space_id, branch_id,
  frontier_ref, frontier_digest,
  collaboration_context_bundle_ref, context_bundle_digest,
  society_ref, society_recovery_epoch,
  endeavor_proposal_ref, endeavor_proposal_digest,
  byom_endpoint_ref, command_endpoint_incarnation,
  realm_byom_binding_ref, realm_byom_binding_revision,
  realm_byom_binding_epoch, realm_byom_binding_digest,
  requested_by_principal, source_actor_binding_digest,
  delegated_principal_subject_digest,
  client_formation_key, byom_command_idempotency_key,
  idempotency_domain_digest, canonical_byom_command_digest,
  formation_slot_ref, formation_slot_generation,
  authorization_dependency_set_ref, authority_digest,
  latest_attempt_ref?, latest_authentication_observation_ref?,
  byom_result_ref?, byom_result_digest?, external_link_ref?,
  state: prepared | submitting | remote_unknown | awaiting_principal |
         byom_committed | linking | linked | ambiguous | canceled,
  created_at, terminal_at?, digest,
  UNIQUE(realm_id, requested_by_principal, client_formation_key)
}

EndeavorFormationSlot {
  slot_id, realm_id, requested_by_principal, client_formation_key,
  holder_formation_id, generation, revision,
  society_ref, society_recovery_epoch,
  source_actor_binding_digest,
  realm_byom_binding_ref, realm_byom_binding_revision,
  realm_byom_binding_epoch, realm_byom_binding_digest,
  canonical_byom_command_digest, byom_command_idempotency_key,
  idempotency_domain_digest,
  state: held | submitting | remote_unknown | awaiting_principal |
         byom_committed | linking | ambiguous | released,
  byom_result_ref?, byom_result_digest?, external_link_ref?,
  acquired_at, released_at?, digest,
  UNIQUE(realm_id, requested_by_principal, client_formation_key)
    WHERE state != released
}

EndeavorFormationAttempt {
  attempt_id, formation_id, attempt_ordinal,
  canonical_byom_command_digest, idempotency_domain_digest,
  attempt_recovery_binding_ref, attempt_recovery_binding_revision,
  attempt_recovery_binding_epoch, attempt_recovery_binding_digest,
  authentication_observation_ref, authentication_observation_digest,
  attempt_nonce, authentication_proof_digest,
  state: prepared | sent | reply_received | transport_unknown |
         reconciled | canceled,
  reply_digest?, reconciliation_digest?,
  prepared_at, sent_at?, observed_at?, digest,
  UNIQUE(formation_id, attempt_ordinal), UNIQUE(attempt_nonce)
}
~~~

The uniqueness scope intentionally deduplicates one explicit human formation
command; it does not imply one Endeavor per Branch, frontier, purpose, or
Society. A separate exact Kovee link policy may restrict a UI's “active
endeavor” relation, but cannot change Byom formation semantics.

`kovee_endeavor_form` requires an already active Society, human Participant,
Standing, Society recovery epoch, KoveeRealmByomBinding, and
KoveeSocietyMapping. It never bootstraps a Society. A new Society is established
first through native `society_prepare`/`society_bootstrap` under the bootstrap
human's direct governance channel; Kovee may supply inert context, but its
gateway is not the genesis actor. Only after the resulting Society and mapping
exist may the convenience formation operation run.

The operation separates stable semantic command bytes from fresh per-attempt
authentication:

~~~text
KoveeEndeavorFormCommand {
  kovee_formation_intent_ref,
  byom_endpoint_ref, command_endpoint_incarnation,
  realm_byom_binding_ref, realm_byom_binding_revision,
  realm_byom_binding_epoch, realm_byom_binding_digest,
  society_ref, society_recovery_epoch,
  source_principal_ref, source_actor_binding_digest,
  context_bundle_ref, context_bundle_digest,
  endeavor_proposal, endeavor_proposal_digest,
  source_principal_position, source_principal_position_digest,
  expected_governance_rule_set_ref, expected_slot_snapshot_digest,
  byom_command_idempotency_key, idempotency_domain_digest
}

KoveeEndeavorFormArguments {
  command: KoveeEndeavorFormCommand,
  canonical_command_digest,
  attempt_id, attempt_nonce,
  attempt_recovery_binding_ref, attempt_recovery_binding_revision,
  attempt_recovery_binding_epoch, attempt_recovery_binding_digest,
  authentication_observation_ref, authentication_observation_digest,
  authentication_proof
}

KoveeEndeavorFormResult {
  kovee_formation_intent_ref, canonical_command_digest,
  society_ref, society_recovery_epoch,
  idempotency_domain_digest,
  endeavor_ref, endeavor_revision,
  endeavor_digest, formation_decision_ref,
  formation_slot_snapshot_digest, source_cursor, digest
}
~~~

The authenticated delegated principal is the only author in this convenience
operation. The server recomputes the actor binding, IdempotencyDomain, canonical
Endeavor subject, eligibility, control domains, active governance rule, and slot
snapshot; the request fields can only match those values. The source principal
must act for its bound human Participant, the computed formation snapshot must
have exactly one required seat, and its own explicit Position must fill that
seat.
The operation cannot import an offline Position, fill another Participant's
seat, invoke an automatic assent policy, or let Kovee author Society state. If
another Position is required it returns `formation_requires_participation`
without committing any Society or Endeavor domain record; the participants use
ordinary `endeavor_propose/position/finalize` operations and Kovee may later
link their already formed Endeavor through a separately authorized read-only
link workflow.

`canonical_command_digest` covers only KoveeEndeavorFormCommand. It excludes
attempt id/nonce, authentication observation/proof, transport request id, and
send time. Every attempt proves fresh authentication over
`canonical_command_digest || idempotency_domain_digest || attempt_nonce ||
attempt_recovery_binding_digest` and the server-derived current actor binding.
A retry therefore preserves semantic bytes and idempotency while replacing only
the expiring proof/binding envelope. Kovee persists each envelope as an
EndeavorFormationAttempt rather than overwriting evidence of an earlier send.

On success Byom commits the source-principal Position, immutable
GovernanceDecision, active Endeavor, idempotency result, event/outbox set, and
authority-journal transition as one visible atomic result. A definite
pre-commit rejection claims the idempotency domain with a non-reexecuting
tombstone. There is no partially created proposal to recover and no hidden
multi-command saga behind the one Kovee intent/key.

~~~text
participant selects exact Space/Branch frontier and goal
  -> Kovee prepares immutable ContextAssembly
  -> Kovee prepares source-qualified CollaborationContextBundle
  -> principal reviews included/omitted items, classes, participants,
     proposed charter/decision rules, budgets, outcomes and workspace binding
  -> Kovee starts one durable formation intent and uniqueness slot
  -> Kovee calls kovee_endeavor_form with the delegated principal
  -> Byom verifies the existing Society and atomically forms Endeavor
  -> Byom returns exact idempotent result
  -> Kovee commits a read-only ExternalLink
~~~

The gateway uses a short-lived delegated-principal credential. A service
reconciler may use only:

~~~text
ExternalCommandResultQuery {
  current_byom_endpoint_ref, current_endpoint_incarnation,
  current_recovery_binding_ref, current_recovery_binding_revision,
  current_recovery_binding_epoch, current_recovery_binding_digest,
  kovee_formation_intent_ref,
  target_byom_endpoint_ref, target_endpoint_incarnation,
  target_realm_byom_binding_ref, target_realm_byom_binding_revision,
  target_realm_byom_binding_epoch, target_realm_byom_binding_digest,
  target_society_ref, target_society_recovery_epoch,
  source_principal_ref, source_actor_binding_digest,
  operation, byom_command_idempotency_key,
  canonical_command_digest, idempotency_domain_digest,
  restore_lineage_proof_ref?, restore_lineage_proof_digest?
}

ExternalCommandResultQueryResult {
  query_digest, current_endpoint_incarnation,
  target_endpoint_incarnation, idempotency_domain_digest,
  status: committed | absent | historically_fenced_absent |
          non_reexecuting_tombstone | unknown,
  committed_result_envelope?: KoveeEndeavorFormResult,
  committed_result_digest?, committed_result_signature?,
  tombstone_ref?, tombstone_digest?, tombstone_reason?,
  historical_fence_receipt_ref?, historical_fence_receipt_digest?,
  restore_lineage_evidence_ref?, restore_lineage_evidence_digest?,
  observed_at, server_signature, digest
}

ExternalCommandTerminalizeArguments {
  kovee_formation_intent_ref,
  current_recovery_binding_ref, current_recovery_binding_revision,
  current_recovery_binding_epoch, current_recovery_binding_digest,
  target_byom_endpoint_ref, target_endpoint_incarnation, target_society_ref,
  target_society_recovery_epoch, source_principal_ref,
  target_source_actor_binding_digest,
  current_source_actor_binding_digest,
  operation, byom_command_idempotency_key,
  canonical_command_digest, idempotency_domain_digest,
  reason, authentication_observation_ref,
  authentication_observation_digest, authentication_proof,
  restore_lineage_proof_ref?, restore_lineage_proof_digest?
}

ExternalCommandTerminalizeResult {
  status: committed | terminalized | not_terminalizable,
  target_endpoint_incarnation, target_society_ref,
  target_society_recovery_epoch,
  canonical_command_digest, idempotency_domain_digest,
  committed_result_envelope?: KoveeEndeavorFormResult,
  committed_result_digest?, committed_result_signature?,
  tombstone_ref?, tombstone_digest?, tombstone_reason?,
  authority_journal_receipt_ref?,
  authority_journal_receipt_digest?,
  blocking_state?: prepared_or_in_flight | lineage_incomplete |
                   witness_unavailable | domain_conflict,
  blocking_evidence_digest?, observed_at, server_signature, digest
}

RestoreLineage {
  lineage_id, endpoint_root_id,
  predecessor_endpoint_incarnation,
  successor_endpoint_incarnation,
  society_ref, predecessor_society_recovery_epoch,
  successor_society_recovery_epoch,
  predecessor_authority_journal_head,
  predecessor_idempotency_checkpoint_ref,
  predecessor_idempotency_checkpoint_digest,
  idempotency_retention: complete | incomplete | unavailable,
  predecessor_domain_execution: permanently_fenced,
  recovery_event_ref, external_witness_ref,
  external_witness_receipt_digest,
  issued_at, status: current | superseded, digest
}

RestoreLineageProof {
  proof_id, endpoint_root_id, society_ref,
  target_endpoint_incarnation, target_society_recovery_epoch,
  current_endpoint_incarnation, current_society_recovery_epoch,
  hop_count, ordered_hops[]: {lineage_ref, lineage_digest},
  target_idempotency_domain_digest,
  composed_at, verifier_version, digest
}
~~~

The query returns exactly one of five facts. `committed` carries the retained
signed KoveeEndeavorFormResult envelope needed to create the ExternalLink; no
second actor-only fetch is required. `absent` means only that a complete query
of the live target domain currently contains neither a result nor a terminal
tombstone; it does not prove that an earlier request cannot arrive or commit
later. `historically_fenced_absent` means a complete RestoreLineageProof found
no row and every predecessor execution domain is permanently fenced; its signed
fence receipt proves the old command can no longer arrive. It is safe to release
the Kovee slot but is not relabelled as an idempotency tombstone that never
existed. `non_reexecuting_tombstone` is a durable Byom-owned terminal claim over
the exact IdempotencyDomain and command digest: Byom has claimed that domain and
MUST reject every future execution under it. `unknown` covers in-flight,
incomplete-retention, unavailable, and unverifiable state.

Status-specific fields are closed. `committed` requires the result
envelope/digest/signature and forbids tombstone/fence fields.
`non_reexecuting_tombstone` requires its ref/digest/reason and forbids result and
historical-fence fields. `historically_fenced_absent` requires the exact
RestoreLineage evidence and historical fence receipt and forbids result and
tombstone fields. Live `absent` forbids result, tombstone, historical-fence, and
RestoreLineage fields. `unknown` forbids result/tombstone/fence fields but MAY
carry non-authorizing diagnostic lineage evidence.

The query is authorized only to the exact formation intent through the BPP
projection registry row and cannot submit, terminalize, modify, or impersonate
the original human. It authenticates through a current recovery binding, while
the target command and original binding remain immutable. If the target
incarnation or Society epoch is historical, the current endpoint returns a
retained result/tombstone or `historically_fenced_absent` only when an externally
witnessed RestoreLineageProof proves continuity and complete recovery of that
old idempotency domain. A permanently retired old domain may itself supply a
non-reexecuting tombstone. Missing or incomplete lineage returns `unknown`,
never live `absent`. Thus the exception reads historical evidence without
accepting an old-incarnation request or reviving old authority.

RestoreLineage is created only by the sealed restore protocol in section 15.3
and is covered by the external witness. `complete` means the cited checkpoint
and all later predecessor idempotency/journal rows were reconciled into the
successor; it is not an operator assertion. `permanently_fenced` means the
predecessor keys, listener authority, and mutation domain can never execute
again. A missing record, broken witness chain, partial backup, or merely matching
endpoint name cannot satisfy historical lookup.

A RestoreLineageProof composes one or more hops in target-to-current order. The
verifier requires one endpoint root and Society throughout; the first
predecessor incarnation/epoch equals the query target; each hop's successor
incarnation/epoch exactly equals the next hop's predecessor; and the final
successor equals the current authenticated endpoint/Society epoch. Every hop
must have a valid external witness receipt, `idempotency_retention: complete`,
`predecessor_domain_execution: permanently_fenced`, a contiguous authority
journal/idempotency checkpoint, and no duplicate, cycle, branch, or gap. The
declared hop count MUST equal the array length, be at least one for a historical
target, and stay within the negotiated lineage limit. Any invalid, incomplete,
unavailable, or unverified hop
makes a query `unknown` and terminalization `not_terminalizable`; later complete
hops cannot launder an earlier incomplete one.

`external_command_terminalize` is the only liveness mutation after an ambiguous
formation. It requires the same source human principal freshly authenticated
through a current recovery binding and the exact original domain/command plus
restore lineage when historical. Byom locks that idempotency domain and its
authority-journal state. If a result committed, it returns the committed
envelope; if execution is prepared/in flight or lineage is incomplete, it
returns `not_terminalizable` without mutation; otherwise it atomically installs
the restore-safe non-reexecuting tombstone. A delayed command racing this
operation either commits first or observes the tombstone; both cannot win. If
the principal or recovery proof is unavailable, Kovee retains the slot
indefinitely rather than guessing.

ExternalCommandTerminalizeResult is a closed union. `committed` requires the
signed result envelope/digest and forbids tombstone, journal, and blocking
fields. `terminalized` requires the tombstone plus the synchronous
AuthorityJournalReceipt and forbids result/blocking fields.
`not_terminalizable` requires one closed blocking state and evidence digest and
forbids result, tombstone, and journal fields. All variants carry the exact
target/domain, observation time, and server signature. `committed` and
`not_terminalizable` are Byom no-op results. `committed` drives Kovee's ordinary
committed-result transition using its embedded envelope;
`not_terminalizable` leaves the Kovee pair unchanged; only `terminalized`
directly releases through its tombstone transition.

“Same source human” is checked from the durable source-principal binding: the
request retains the target actor-binding digest and separately carries the
current actor-binding digest derived from the fresh channel. A binding-epoch
change may therefore fence execution while still allowing the same human to
deny future execution; another principal, service, controller, or successor
Participant cannot terminalize it.

Any resubmission requires a fresh human authentication attempt over the
unchanged semantic command, original actor-binding/idempotency domain, and the
current same-human actor/binding proof. After an external call may have
committed, the Kovee formation slot has no timeout-based release.

The current binding may authenticate a retry or terminalization only when its
signed lineage explicitly recognizes the original binding, exact Society
scope, source principal, and operation without widening them. It lives in the
attempt envelope and never changes the semantic command. Otherwise only the
read-only query is possible when authorized, and unresolved state stays held.
A retry additionally requires the command endpoint incarnation and Society
recovery epoch still active. RestoreLineage never permits resubmission to a
historical domain; it permits only query or terminalization.

The paired Kovee intent/slot recovery machine is closed:

| Current intent / slot | Trigger and verified fact | Next intent / slot | Slot release |
|---|---|---|---|
| prepared / held | local cancel durably precedes first send | canceled / released | yes |
| prepared or awaiting_principal / held or awaiting_principal | exact submit begins with a fresh valid attempt | submitting / submitting | no |
| submitting / submitting | reply lost or transport outcome unknown | remote_unknown / remote_unknown | no |
| prepared, submitting, remote_unknown, awaiting_principal, or ambiguous / paired slot state | committed reply, query, or terminalization result with valid signed result envelope | byom_committed / byom_committed | no |
| submitting, remote_unknown, awaiting_principal, or ambiguous / same | verified `absent` | awaiting_principal / awaiting_principal | no |
| submitting, remote_unknown, awaiting_principal, or ambiguous / same | `unknown`, invalid, or incomplete-lineage result | ambiguous / ambiguous | no |
| prepared, submitting, remote_unknown, awaiting_principal, or ambiguous / paired slot state | verified tombstone, including successful terminalization | canceled / released | yes |
| prepared, submitting, remote_unknown, awaiting_principal, or ambiguous / paired slot state | verified `historically_fenced_absent` | canceled / released | yes |
| submitting, remote_unknown, awaiting_principal, or ambiguous / same | terminalization returns `not_terminalizable` | unchanged | no |
| byom_committed or linking / same | repeated valid committed query or terminalization result | unchanged | no |
| byom_committed / byom_committed | begin idempotent Kovee ExternalLink commit | linking / linking | no |
| linking / linking | retryable ExternalLink failure or retry | linking / linking | no |
| linking / linking | ExternalLink commits exact result digest | linked / released | yes |

Each row CASes both records under the slot generation. Terminal pairs do not
leave them. SQL loss after Byom commit is recovered from the signed result
envelope; link creation is idempotent over its digest. Timeout, absence,
authentication expiry, binding rotation, an unverified historical lookup, or
`ambiguous` never releases the slot. Only a pre-send cancel, a verified
tombstone, a verified historically fenced absence, or a committed ExternalLink
does. Changed semantic command bytes conflict and never reuse the generation.

Each EndeavorFormationAttempt is also closed: `prepared → sent` when bytes may
first leave Kovee; `prepared → canceled` only before that point; `sent →
reply_received` on a verified reply or `sent → transport_unknown` when receipt
is uncertain; and `transport_unknown → reply_received` on a valid late reply or
`transport_unknown → reconciled` when a later query/terminalization supplies
the exact resolution digest. `reply_received`, `reconciled`, and `canceled` are
terminal attempt states. Resolving an intent never rewrites an earlier
attempt's send/authentication evidence.

New Space content does not enter the Endeavor ambiently. A participant prepares
and admits a new exact bundle. Kovee access and Byom visibility are intersected
on every projection read, event, search result, and artifact fetch.

### 16.4 Runtime mapping

- One committed Byom Episode maps to one logical Kovee Invocation keyed by
  Byom endpoint, Episode id, and provider contract version.
- Multiple Kovee attempts may retry only while the Byom generation/fence and
  Kovee Invocation/fence are both current.
- The effective profile, deadline, budget, classification, provider, region,
  filesystem, and network scope is the restrictive intersection.
- Kovee commits the immutable Invocation result first and submits its digest to
  Byom idempotently. A stale Byom Episode retains only an orphan diagnostic.
- Kovee Attention may notify the Byom adapter of an admitted exact event. Byom
  alone decides whether a Participant's WakeIntent and ActivityStream permit a
  new Episode.
- A child Kovee local Commitment inside an Episode is allowed only under an
  exact Byom Mandate and Pledge ceiling. It is an intra-Episode helper, cannot
  become an Endeavor Pledge or own final acceptance, and is fenced by both
  parents.

Byom's budget accounts are authoritative for Byom work. Kovee creates
subordinate reservations against exact parent dimensions, may impose lower
platform ceilings, settles measured use once, and never double-charges a
parallel Kovee account for the same usage.

### 16.5 Decisions and projections

Kovee may render a Byom proposal or pending decision with:

- exact subject and change from the prior revision;
- purpose, affected Pledges, participants, Assembly epoch, and required seats;
- recipient, data classes, final disclosure manifest, provider claims;
- budget, concurrency, delegation, deadline, cancellation, and effect risk;
- policy and StandingMandate match;
- causal events and evidence;
- which fields are participant assent versus human authority.

Immediately before rendering an active control and immediately before
submission, Kovee reloads the subject and eligibility from Byom. A cached
projection is never the decision subject.

Kovee consumes Byom events by durable cursor. Projection rows preserve source
endpoint, object revision, event id/cursor, visibility scope, payload digest,
and projection time. The combined Kovee/Byom/Akson timeline preserves each
source order and causal references; wall-clock ordering is only a view.

### 16.6 `byom_governed_work_v1` compatibility bundle

Compatibility is one explicit, all-or-nothing feature bundle. Current Kovee is
Sage-shaped and does not implement it. `byom_governed_work_v1` requires these
normative Kovee schema and operation-matrix changes rather than a private adapter:

~~~text
KoveeRealmByomBinding {
  binding_ref, realm_ref, binding_revision, binding_epoch,
  predecessor_binding_ref?, predecessor_binding_digest?,
  binding_lineage_ref?, binding_lineage_digest?,
  byom_endpoint_ref,
  endpoint_incarnation, compatibility_bundle,
  delegated_principal_audience, external_authorization_audience,
  historical_recovery_mode: disabled | exact_formation_intent_only,
  recovery_authorization_policy_ref, recovery_authorization_policy_digest,
  status, dependency_digest, digest
}

KoveeSocietyMapping {
  realm_ref, society_ref, society_recovery_epoch,
  allowed_project_and_space_selectors[], classification_binding_ref,
  governance_owner_binding_ref, governance_owner_binding_digest,
  status, revision, digest
}

KoveeGovernanceOwnerBinding {
  realm_ref, exact_scope_selector, exact_scope_digest,
  revision, binding_epoch,
  governance_owner: sage | byom | none,
  owner_endpoint_ref?, owner_binding_ref?, cutover_ref?,
  status: active | frozen,
  UNIQUE(realm_ref, exact_scope_digest), digest
}

ByomEpisodeBinding {
  byom_endpoint_ref, endpoint_incarnation, society_ref, recovery_epoch,
  participant_ref, participant_binding_epoch, manifestation_ref,
  activity_stream_ref, episode_ref, generation,
  byom_attempt_ref, byom_fence_epoch,
  kovee_invocation_ref, kovee_invocation_fence,
  mandate_use_refs[], context_source_digest,
  byom_budget_reservation_ref, byom_budget_reservation_digest,
  external_budget_bridge_ref, kovee_subordinate_reservation_ref,
  kovee_subordinate_reservation_digest,
  dependency_digest, digest
}
~~~

The Kovee revision implementing the bundle MUST:

1. extend `RealmAuthorityBinding` with a Byom endpoint/principal binding and use
   separate `KoveeSocietyMapping` rows because one Realm may host many Societies;
   add one CAS-protected `KoveeGovernanceOwnerBinding` per exact governed scope,
   with no overlapping active owner selectors; every formation intent and slot
   pins the Realm/Byom binding ref, revision, epoch, and digest;
2. add `owner_protocol: byom` to every qualified ActorRef/EventRef/ExternalLink
   and `ExternalAuthorizationConsumption` location that may carry Byom state;
3. add `ByomEpisodeBinding` rather than overloading `SageTurnBinding`;
4. add `byom_subordinate` budget reservation sets and the stable reserve,
   query, settle, uncertain, and release saga from section 11.4;
5. add the exact Byom source fields from section 12.1 to
   `ProviderContextManifest`, while Kovee remains sole owner of final bytes;
6. add `EndeavorFormationIntent` and `EndeavorFormationSlot` with prepared,
   remote-unknown, awaiting-principal, committed, reconciled, and terminal
   states; stable result query through `external_command_result_query`; signed
   result envelopes; current-authenticated RestoreLineage lookup; race-safe
   `external_command_terminalize`; durable non-reexecuting tombstones; and no
   release on timeout or point-in-time absence after a possibly committed Byom
   call; implement the exact existing-Society-only `kovee_endeavor_form`
   delegated-principal operation and its stable-command/fresh-attempt contract
   rather than a hidden multi-command adapter;
7. accept `owner_protocol: byom` consumption only through the exact
   `execution_permit_consume` receipt and import Kovee-owned effect outcomes
   through EffectOutcomeAdmission;
8. define sender-constrained worker Participant credentials, allowed child local
   commitments, deny-by-absence Kovee operation rows, Byom projection schemas,
   cursor invalidation, and recovery-epoch behavior;
9. bind logical WorkspaceAllocation to Kovee-owned physical materialization and
   digest-bound apply; and
10. conform delegated-principal authentication, multi-human authority,
    source-qualified context visibility, authorized snapshots, portable
    Continuations, and invalidation on membership, Standing, classification,
    erasure, policy, endpoint, or fence change; and
11. add a Kovee-owned `byom_akson_dispatch_v1` effect driver, operation rows,
    signed outcome-receipt union and one receipt head; commit every Kovee source
    successor before Byom independently admits it; current Sage-only Akson
    driver authority is not reusable by relabeling; and
12. implement sender-constrained candidate channels and the one-shot
    OnboardingComputeIntent/Receipt path through Kovee's final
    ProviderContextManifest and model broker, without turning runtime output into
    membership assent.

A Kovee-hosted Society binds Kovee's exact Realm classification vocabulary and
mapping revision for all Kovee-owned inputs. A Society overlay may only preserve
or raise restriction. Standalone Byom may own a closed vocabulary for its own
data; it cannot reinterpret a Kovee label under a Byom-owned lookalike.

Missing support disables the workflow. It never falls back to broad admin
credentials, shared tables, same-UID impersonation, or an unconfined worker.

## 17. Akson federation

### 17.1 Sovereign boundary

An Akson peer is not a Byom Participant, Society member, Assembly seat, or
Manifestation. Pairing and signatures authenticate a sovereign endpoint; they
grant no local standing, context, mandate, or execution.

The only supported v0.2 execution path is an Akson-owned confined remote worker,
followed by local Byom admission of its verified outcome:

~~~text
local Participant proposes a remote-task ActIntent
  -> Byom prepares exact task, disclosure, outcome and evidence terms
  -> required Byom positions and outbound/share ActIntent
  -> Kovee prepares local effect and consumes Byom permit
  -> Akson stages inert signed contract
  -> local Akson authority consents to exact stage
  -> Akson dispatches over the paired channel
  -> remote Akson verifies and stores inert
  -> remote Akson authority independently accepts and issues its local work order
  -> Akson's confined worker claims and executes under that endpoint's authority
  -> signed result and evidence return
  -> local Akson verifies
  -> Byom admits against original intent and classification mapping
  -> local reviewer decides fulfillment
~~~

Every receipt remains distinct: network receipt, durable arrival, contract
acceptance, local work-order issue, worker claim, result delivery, verification,
admission, review, and Endeavor outcome.

The foreign contract is not a shared or remote Byom Pledge. A local Pledge may
promise to seek or review a remote outcome, and a remote Society may independently
admit the proposal and form its own local Pledge, but those are two separately
assented records correlated by an Akson task digest.

Inbound execution inside a remote Byom/Kovee runtime is not supported in v0.2.
A future profile must first admit the proposal and prepare exact local Byom
Episode/Mandate authority, then issue an Akson work order addressed to an adapter
that requires both chains before any worker claim. It needs a normative Akson
worker-coordination profile; merely admitting after work-order issue is too late.

### 17.2 Least privilege

Byom MUST NOT hold or reach Akson's broad admin surface, pairing authority,
processor credentials, peer work-order authority, or dispatch credentials.
Byom prepares semantic authority and later admits verified sources; it never
calls Akson. Kovee owns a narrow `byom_akson_dispatch_v1` effect driver which,
under a consumed Byom receipt, needs this versioned Akson surface:

- authorized read of exact peer binding and verified Agent Card claims;
- inert idempotent stage for a server-prepared contract;
- dispatch that atomically consumes Akson's one-shot local consent;
- durable cursored task/result/evidence events;
- exact result/evidence verification status;
- no operation to approve or run inbound peer work.

A generic Kovee tool, Byom participant, or byomd adapter cannot call this
surface. Only the Kovee-owned driver under a consumed Byom intent, current Kovee
effect fence, and exact Akson consent may dispatch. Kovee records the
authoritative local Effect; Byom later stores only EffectOutcomeAdmission and
source-qualified Akson verification/admission state.

The required federation bundle is `akson_byom_exchange_v1`. Each signed object
binds both endpoint root ids and identity epochs; active key ids and validity
intervals; the Byom endpoint incarnation, Society recovery epoch, ActIntent and
MandateUse digests; stable delivery key and nonce; issue/expiry; exact
response/evidence schemas; and one closed classification profile. Classification
state is not an eventually filled record. Four immutable phase-owned shapes add
only facts their signer can know at that phase:

~~~text
AksonByomRequestClassification {
  request_classification_id, phase: request,
  profile: society_mapped_round_trip | akson_neutral_contract,
  source_endpoint_ref, source_identity_epoch,
  destination_endpoint_ref, destination_identity_epoch,
  source_society_ref, source_society_recovery_epoch,
  act_intent_ref, act_intent_digest,
  mandate_use_ref, mandate_use_digest,
  execution_consumption_receipt_ref,
  execution_consumption_receipt_digest,
  stable_execution_key, delivery_key,
  contract_vocabulary_ref, contract_vocabulary_digest,
  source_outbound_revision_ref, source_outbound_revision_digest,
  dependency_digest, issued_at, expires_at,
  source_endpoint_signature_ref, digest
}

AksonByomAcceptanceClassification {
  acceptance_classification_id, phase: acceptance,
  profile, request_classification_digest,
  source_endpoint_ref, source_identity_epoch,
  destination_endpoint_ref, destination_identity_epoch,
  act_intent_ref, act_intent_digest,
  mandate_use_ref, mandate_use_digest,
  execution_consumption_receipt_ref,
  execution_consumption_receipt_digest,
  stable_execution_key, delivery_key,
  akson_contract_acceptance_ref, akson_contract_acceptance_digest,
  remote_society_ref?, remote_society_recovery_epoch?,
  remote_inbound_mapping_ref?, remote_inbound_mapping_digest?,
  remote_akson_handling_policy_ref?,
  remote_akson_handling_policy_digest?,
  accepted_at, expires_at, destination_endpoint_signature_ref, digest
}

AksonByomResultClassification {
  result_classification_id, phase: result,
  profile, acceptance_classification_digest,
  source_endpoint_ref, source_identity_epoch,
  destination_endpoint_ref, destination_identity_epoch,
  act_intent_ref, act_intent_digest,
  mandate_use_ref, mandate_use_digest,
  execution_consumption_receipt_ref,
  execution_consumption_receipt_digest,
  stable_execution_key, delivery_key,
  akson_result_manifest_ref, akson_result_manifest_digest,
  remote_outbound_revision_ref?, remote_outbound_revision_digest?,
  neutral_result_label_manifest_ref?,
  neutral_result_label_manifest_digest?,
  produced_at, destination_endpoint_signature_ref, digest
}

AksonByomAdmissionClassification {
  admission_classification_id, phase: local_admission,
  profile, result_classification_digest,
  source_endpoint_ref, source_identity_epoch,
  destination_endpoint_ref, destination_identity_epoch,
  act_intent_ref, act_intent_digest,
  mandate_use_ref, mandate_use_digest,
  execution_consumption_receipt_ref,
  execution_consumption_receipt_digest,
  stable_execution_key, delivery_key,
  akson_result_manifest_ref, akson_result_manifest_digest,
  admitting_local_society_ref,
  admitting_local_society_recovery_epoch,
  local_inbound_mapping_ref, local_inbound_mapping_digest,
  byom_effect_outcome_admission_ref,
  byom_effect_outcome_admission_revision,
  byom_effect_outcome_admission_digest,
  kovee_akson_outcome_receipt_ref,
  kovee_akson_outcome_receipt_digest,
  admitted_at, local_byom_server_signature_ref, digest
}

ByomAksonDispatchOutcomeReceipt {
  receipt_id, receipt_revision, previous_receipt_digest?,
  disposition: pre_result_failed | ambiguous |
               verification_rejected | verified_result,
  host_protocol: kovee,
  driver_profile: byom_akson_dispatch_v1,
  host_endpoint_ref, kovee_effect_revision,
  classification_profile: society_mapped_round_trip |
                          akson_neutral_contract,
  source_endpoint_ref, source_identity_epoch,
  destination_endpoint_ref, destination_identity_epoch,
  request_classification_digest,
  acceptance_classification_digest?,
  result_classification_digest?,
  kovee_effect_ref, kovee_effect_digest,
  execution_consumption_receipt_ref,
  execution_consumption_receipt_digest,
  act_intent_ref, act_intent_digest,
  mandate_use_ref, mandate_use_digest,
  stable_execution_key, delivery_key,
  outcome: succeeded | failed | ambiguous,
  failure_stage?: before_dispatch | stage_rejected | consent_rejected |
                  dispatch_definitively_rejected | reconciled_no_result,
  failure_evidence_ref?, failure_evidence_digest?,
  ambiguity_stage?: dispatch_unknown | result_unknown |
                    verification_unknown,
  ambiguity_evidence_ref?, ambiguity_evidence_digest?,
  verification_rejection_class?: signature_invalid |
                                 identity_epoch_mismatch |
                                 schema_invalid | digest_mismatch |
                                 evidence_invalid | contract_mismatch,
  terminal_rejection_evidence_ref?,
  terminal_rejection_evidence_digest?,
  akson_contract_acceptance_ref?,
  akson_contract_acceptance_digest?,
  akson_result_manifest_ref?, akson_result_manifest_digest?,
  akson_verification_ref?, akson_verification_digest?,
  reconciles_host_receipt_ref?, reconciles_host_receipt_digest?,
  observed_at, kovee_service_signature_ref, digest,
  UNIQUE(receipt_id, receipt_revision),
  UNIQUE(kovee_effect_ref, receipt_revision)
}

ByomAksonDispatchOutcomeReceiptHead {
  kovee_effect_ref, receipt_id,
  current_receipt_revision, current_receipt_digest,
  state: ambiguous | final,
  revision, updated_at, digest,
  UNIQUE(kovee_effect_ref)
}
~~~

Every successor repeats the profile, endpoint roots and identity epochs,
ActIntent, MandateUse,
ExecutionConsumptionReceipt, stable execution key, and delivery key and binds
the exact predecessor digest.
Mismatch is invalid, not a new chain. Akson wire
request, acceptance, and result shapes carry the indicated endpoint signature;
the final local admission is authority-journaled and signed by the admitting
Byom endpoint. No signer supplies a future phase's field.

The required/forbidden field matrix is closed:

| Profile and phase | Required classification authority | Forbidden fields |
|---|---|---|
| either / request | source Society, exact OutboundClassificationRevision, neutral contract vocabulary | every remote Society/inbound/outbound, remote handling, result-label and local-inbound field |
| society_mapped_round_trip / acceptance | predecessor, Akson contract acceptance, remote Society/recovery epoch and its inbound ClassificationMappingRevision | remote Akson handling-policy substitute, remote outbound, result-label and local-inbound fields |
| akson_neutral_contract / acceptance | predecessor, Akson contract acceptance and exact remote Akson handling-policy digest | every remote Society/inbound/outbound and local-inbound field |
| society_mapped_round_trip / result | predecessor, Akson result manifest and remote Society OutboundClassificationRevision | neutral result-label and local-inbound fields |
| akson_neutral_contract / result | predecessor, Akson result manifest and neutral result-label manifest in the request vocabulary | every remote Society/outbound and local-inbound field |
| either / local_admission | predecessor, admitting local Society/epoch, its inbound ClassificationMappingRevision, `verified_result` Kovee receipt, and exact final EffectOutcomeAdmission revision/digest | re-copying any earlier phase-specific authority field; only profile/endpoints/ActIntent/MandateUse/execution+delivery keys and result manifest repeat as chain identity; ambiguous/pre-result-failed receipts |

ByomAksonDispatchOutcomeReceipt is also a closed discriminated union:

| Disposition | Required | Forbidden |
|---|---|---|
| pre_result_failed | request-phase digest, `outcome: failed`, exact failure stage and signed failure evidence | ambiguity, verification-rejection and every result-phase/manifest/verification field; acceptance ref/digest/phase digest unless the stage is `reconciled_no_result` and all three are present |
| ambiguous | request-phase digest, `outcome: ambiguous`, exact ambiguity stage and signed ambiguity evidence | failure, verification-rejection, verified-result/result-phase and reconciliation fields; a known acceptance ref/digest/phase-digest triple may be present or all three are absent |
| verification_rejected | request-phase digest, `outcome: failed`, claimed result manifest, exact negative Akson verification, rejection class, and signed terminal-rejection evidence | result-classification digest, classification admission, failure and ambiguity fields; acceptance triple is all present or all absent |
| verified_result | request, acceptance and result phase digests; `outcome: succeeded | failed`; exact Akson acceptance, result manifest and verification | failure, ambiguity and verification-rejection fields |

Optional ref/digest pairs and acceptance/ref/digest/phase-digest groups are
all-or-none. `reconciles_host_receipt_*` is
required exactly when a final `verified_result`, `verification_rejected`, or
`pre_result_failed/reconciled_no_result` supersedes an ambiguous receipt, and
forbidden otherwise. The predecessor must be the current receipt for the same
Kovee Effect, intent, stable key, and delivery key. A pre-result failure asserts
no classified result, and an ambiguous receipt asserts no final outcome; neither
can create AksonByomAdmissionClassification.

`verification_rejected` is final only when the signed Akson/Kovee
idempotency/terminal-state evidence proves that no valid replacement result can
emerge for that delivery key. The claimed result bytes and negative verification
remain evidence, not an admitted/classified result; the EOA result fields remain
absent and no AksonByomAdmissionClassification is created. Without that terminal
proof—including for a transient verifier failure—the disposition is
`ambiguous` with `verification_unknown`.

`pre_result_failed` is legal only when signed Kovee/Akson evidence proves that
no accepted remote work/result can later emerge for the delivery key. A timeout,
lost dispatch reply, known acceptance without a final result, or unverifiable
rejection is `ambiguous`, never failed. `dispatch_definitively_rejected` requires
Akson's durable idempotency/consent state proving the dispatch was not and cannot
be consumed.

Receipt revisioning is closed. Genesis is revision 1 with no predecessor and
creates the one Kovee-owned receipt head; an ambiguous genesis marks that head
`ambiguous`, while every other disposition marks it `final`. A successor is
allowed only from an ambiguous head, keeps the same receipt id, Kovee Effect,
profile, endpoint epochs, intent/receipt/key/delivery identity, increments the
receipt revision by exactly one, and names the current receipt digest as both
`previous_receipt_digest` and `reconciles_host_receipt_digest`. Kovee commits
the immutable successor and head CAS together before Byom sees it. One
concurrent successor wins; changed identity or a fork conflicts. A final head is
terminal and no later Kovee receipt can reopen it.

Chain identity is enforced by value, not merely by possession of predecessor
digests. Across all four records, the ActIntent ref/digest, MandateUse ref/digest,
ExecutionConsumptionReceipt ref/digest, stable execution key, delivery key, and
endpoint roots MUST be byte-equal. MandateUse MUST name that ActIntent;
ExecutionConsumptionReceipt MUST name both records and the same stable key; and
the request's source Society/epoch MUST equal ActIntent's Society/epoch and the
final admitting Society/epoch. The result manifest ref/digest in the result,
Kovee outcome receipt, and local admission MUST be identical.

The Kovee receipt's classification profile, endpoint roots/identity epochs,
ActIntent, MandateUse, ExecutionConsumptionReceipt, and execution/delivery keys
MUST be byte-equal to the request phase named by its required request digest.
When the acceptance triple is present it MUST equal the named acceptance phase.
Only `verified_result` has a result-classification digest, and that digest,
acceptance, and result manifest MUST equal the exact result phase. Its cited
Akson verification MUST cover the destination endpoint root/identity epoch,
delivery key, acceptance digest, result-classification digest, and result
manifest digest. `verification_rejected` instead binds its claimed manifest to
the negative verification and terminal-rejection evidence and MUST NOT name a
valid result phase. Possessing a valid result manifest under a different phase
chain or peer epoch cannot satisfy these equalities.

For `byom_akson_dispatch_v1`, Kovee's authoritative receipt for every host
outcome is exactly ByomAksonDispatchOutcomeReceipt. For every source admission,
the EffectOutcomeAdmission `intent_*`, `stable_execution_key`, `host_protocol`,
`host_endpoint_ref`, `host_effect_*`, `host_receipt_*`, and `outcome` MUST equal
the ActIntent and the signed receipt byte-for-byte; `host_protocol` is `kovee`,
the endpoint is the receipt's exact Kovee endpoint, and `host_receipt_*` names
that receipt/revision. These fields are server-derived from verified source
records, not copied from caller-selected EOA values.
EffectGovernanceDisposition is not an EOA and never substitutes a local outcome
for any source field.

A `pre_result_failed` receipt creates a final failed EOA with no result and no
AksonByomAdmissionClassification. An `ambiguous` receipt creates an ambiguous
EOA with no final result or classification admission. A
`verification_rejected` receipt creates a final failed EOA that retains the
claimed bytes/negative verification only as source evidence, with no EOA result
and no classification admission. A later final receipt
MUST cite the exact current ambiguous host receipt. Kovee first CAS-commits and
signs that successor under its own Effect/receipt head. Byom later verifies the
immutable successor; the new EOA MUST cite the exact current ambiguous EOA in
`reconciles_admission_*` and independently CAS only the Byom head. A final
`reconciled_no_result` receipt
may resolve that head to failed without classification. A `verified_result`
receipt may create a new final EOA or reconcile the exact ambiguous head; only
this branch may create AksonByomAdmissionClassification. A conflicting final
head is rejected, while an exact repeated final receipt is idempotent. A late
result after a final pre-result failure or verification rejection is retained as
source-qualified conflict evidence and cannot silently reopen the EOA.

An EffectGovernanceDisposition against an ambiguous Akson receipt never closes
that receipt head or the EOA head. Any later final Kovee successor advances the
source EOA while atomically fencing the active disposition head to
`source_advanced`. A `verified_result` additionally creates
AksonByomAdmissionClassification normally. Classification records safety
labels; it is not permission to use the bytes. Materialization and downstream
use remain quarantined until a late-source `effect_reconcile` decision
explicitly releases that exact classified result, or remain quarantined
indefinitely. A final no-result branch keeps result use `unavailable`. A local
business decision therefore cannot suppress source truth, launder unclassified
bytes, or be mistaken for Akson verification.

For the verified-result branch, the EOA `result_*` MUST equal the Akson result
manifest in the phase result, Kovee receipt, and local admission. The active
local ClassificationMappingRevision MUST be owned by the admitting Society,
name the remote Akson endpoint and request contract vocabulary as its source,
and map the exact result labels; a mapping owned by another Society or
vocabulary cannot be substituted.

The `effect_outcome_admit` transaction verifies the applicable union branch,
all equalities and signatures, and the exact EffectOutcomeAdmissionHead. An
ambiguous receipt journals and exposes only the EOA revision and does not alter
the disposition head. A final `pre_result_failed` or `verification_rejected`
receipt creates/reconciles only the EOA and fences any active disposition head
to `source_advanced`. For verified result it creates/reconciles the EOA and
AksonByomAdmissionClassification in one atomic visibility set and, when an
active ambiguity disposition exists, CAS-fences that separate disposition head
to `source_advanced` in the same Byom transaction. The operation locks the
source head before reading and locking the current disposition head; the caller
cannot name a stale disposition revision to make source admission fail. The
source admission never waits for a new governance decision. The caller supplies
source records, not selectable EOA/admission values. Failure or a concurrent
receipt, source head, result, or mapping change creates none of the affected
atomic set. This prevents a valid result, receipt, admission, or mapping from
being spliced into a different intent, Society, execution key, or delivery
chain.

In `society_mapped_round_trip`, remote acceptance therefore adds the
independently chosen remote Society and inbound mapping, the result adds that
Society's outbound revision, and local admission adds the admitting local
mapping. This profile requires classification adapters and independently
governed Societies at both endpoints; it does not imply a shared Society.

`akson_neutral_contract` is the core v0.2 Akson-worker profile when no remote
Byom Society participates. The request still binds the source Society's
outbound revision into a neutral contract vocabulary. Remote acceptance binds
the exact Akson accepted-contract constraints and local work-order policy digest
in `remote_akson_handling_policy_ref`; result parts carry only labels from the
same neutral contract vocabulary; and local admission binds the admitting
Society's inbound mapping from that vocabulary. No remote Society mapping is
ever present in this profile. A class whose handling or
processor/disclosure restriction cannot be expressed and enforced by the
accepted Akson contract, local work order, and negotiated capability evidence
is ineligible for this profile. The receiver may always impose stricter local
handling. Unknown profile values, mixed field sets, vocabulary drift, or a
missing required reference/predecessor deny dispatch or admission.

~~~text
FederationCapabilityEvidence {
  capability_id, peer_binding_ref, capability_kind,
  evidence_class: peer_asserted | locally_observed | hardware_attested,
  claim_digest, attestor_ref?, measurement_ref?, policy_digest?,
  nonce?, observed_at, expires_at, verification_status, digest
}
~~~

| Akson capability | Required for Byom production use |
|---|---|
| endpoint/key binding and expiry enforcement | required; expired or changed key blocks |
| durable stage, local consent, dispatch and result idempotency | required |
| confined worker and claim fencing | protocol feature required; independent attestation required for any local confinement security claim |
| rollback detection | independently witnessed fail-closed profile required; operate-but-flagged is insufficient |
| monetary/provider ceiling | required only if that dimension is advertised; otherwise the task MUST exclude it |
| token, aggregate output-byte, output-disk and wall-time enforcement | each separately negotiated and tasks exceeding supported dimensions rejected |
| processor-hidden fields | accepted hardware/processor attestation required when a DisclosureManifest marks such fields; peer assertion is insufficient |
| response/evidence completeness verification | exact schema support required for any claimed evidence slot |
| audit tail anchoring | required for operator-resistant audit claim; residual lag is displayed |

Missing or weaker feature flags reject the affected contract rather than merely
warning. Evidence class is part of every policy match. A peer signature proves
who asserted a capability, not that a malicious sovereign peer enforced it.
`peer_asserted` can establish protocol interoperability and contractual claims
only. Local disclosure policy assumes the remote operator can read every
transmitted byte unless an independently accepted hardware/processor attestation
proves the narrower plaintext boundary; peer-asserted confinement or hidden-field
claims never protect confidentiality. `locally_observed` describes only facts
the local endpoint actually measured. Display cards and aliases are never
sufficient.

### 17.3 Federated assemblies

Byom v0.2 has no single mutable Assembly spanning sovereign endpoints. A later
federated assembly is a set of local Assembly projections and signed Akson
agreements. Each Society:

- keeps its own membership, authority, privacy, decision, and budget state;
- signs only its own positions and commitments;
- admits foreign records under local policy;
- can independently refuse, suspend, or leave;
- never treats a remote quorum as local root authority.

No distributed consensus, shared broker, replicated secret, or global roster is
implied.

### 17.4 Late results and cancellation

Remote cancellation is advisory unless the signed contract grants a precise
stronger operation. Expired, superseded, canceled, or late results are retained
and verified but cannot satisfy a changed Pledge or Episode generation. They may
enter quarantine as evidence under local policy.

Peer key-binding or identity-epoch change suspends associated trust evidence,
standing policies, and pending dispatch until locally reviewed. Aliases never
survive as authority keys.

## 18. Threat model

### 18.1 Assets

Byom protects:

- human sovereignty and the integrity of charter amendment;
- participant identity, standing, voluntary assent, refusal, and attribution;
- Assembly membership, collective decision integrity, and independence checks;
- Mandates, decision receipts, budgets, ceilings, and one-shot use;
- Society, Endeavor, Pledge, ActivityStream, Episode, review, and event state;
- private context, Engrams, artifacts, relationship metadata, and classifications;
- model, tool, connector, repository, cloud, and Akson credentials held below Byom;
- host files, workspaces, processes, network, and effect targets;
- audit integrity, idempotency, recovery epochs, and ambiguity records;
- availability against wake storms, recursive delegation, Sybil seats, and
  resource capture.

### 18.2 Adversaries

Assume:

- a malicious, compromised, prompt-injected, or simply erroneous agent
  Participant;
- colluding participants or nested collectives attempting authority, quorum, or
  budget amplification;
- an authenticated human insider exceeding their Society role;
- a malicious remote Akson peer and adversarial content in every remote field;
- hostile model output, tool output, connector payload, Engram, artifact,
  filename, URL, relation, skill, and status string;
- a compromised worker, adapter, model provider, connector, or future relay;
- replay, duplication, reordering, delay, truncation, lost replies, clock skew,
  stale projections, and old workers returning;
- malformed, oversized, deeply nested, ambiguous, compressed, or parser-hostile
  input;
- network attackers and SSRF/DNS rebinding against lower brokers;
- daemon, database, broker, worker, or host crashes around every transaction and
  effect boundary;
- database snapshot rollback, restore, audit truncation, stolen interim keys,
  dependency compromise, and denial-of-service;
- a curious installation operator with normal plaintext access.

Byom does not claim protection from a fully compromised kernel/hypervisor, a
malicious operator who controls plaintext and keys, an approved external
provider that receives disclosed plaintext, physical attacks, or side channels.
Those limitations are explicit in the deployment profile and risk view.

### 18.3 Society-specific threats and mitigations

| Threat | Required mitigation |
|---|---|
| Agent appoints itself or another agent | Admission and Standing require the charter procedure and authenticated seats; profile text is inert. |
| Coordinator becomes de facto root | No protocol coordinator power; steward is an ordinary Pledge/Mandate, replaceable and scope-limited. |
| One controller fills many quorum seats | Protected independence-domain closure, pinned eligible set, nested-path deduplication, and configurable distinct-controller rules. |
| Collective launders member authority | Collective has separate identity and Mandate; member credentials/context never aggregate. |
| Recursive teams self-replicate or exhaust resources | DAG membership, maximum depth/fanout, lineage budgets, aggregate concurrency, rate limits, and circuit breakers. |
| Agent accepts work for another participant | Positions derive from the authenticated participant channel and fill only that participant's seat. |
| Pledge is treated as tool permission | Orthogonal Pledge and Mandate records; every effect needs a consumed ActIntent. |
| Mandate is widened through delegation | Mechanical subset validation across the complete root chain; reserved child budget and non-delegable powers. |
| Model-generated plan silently restructures work | Plans are attributed lenses; only separately assented Pledges and dependencies change authority state. |
| Membership grants access to all society memory | Per-object visibility, audience ContextManifest, classification, and no ambient collective context. |
| Competing agents manipulate reputation | Source-qualified claims, local evidence classes, no global score or gossip, model ranking only after eligibility. |
| Human approval fatigue causes unsafe auto-approval | Digest-adopted StandingMandates with exact selectors, aggregate ceilings, derived per-use receipts, expiry and revocation. |
| Emergency stop rewrites history | Holds fence future action but retain decisions, Pledges, effects, ambiguity, and prior disclosures. |

## 19. Security and safety controls

### 19.1 Trust boundaries

The reference deployment distinguishes:

1. unauthenticated network clients and bounded pre-auth parsing;
2. authenticated principals with partial Society/Kovee access;
3. untrusted participant and collaboration content;
4. agent code and attached harnesses under an explicit profile;
5. trusted Byom deterministic services with narrow workload identities;
6. trusted Kovee runtime, context, artifact, and effect services;
7. the independently trusted Akson daemon and sovereign peers;
8. model/tool/connector providers outside the local plaintext boundary;
9. installation operators, key custodians, and recovery administrators.

Crossing one boundary never silently inherits trust from another.

### 19.2 Required platform controls

- TLS on every external binding and mutually authenticated workload channels
  internally.
- Short-lived audience-bound principal, participant, service, runtime, upload,
  artifact, and effect grants.
- Encryption at rest with Society/realm-scoped data keys in team mode; keys in a
  KMS or secret manager, never ordinary database fields, prompts, argv, events,
  or worker-visible environment.
- Application authorization on every read, query, mutation, replay, live event,
  search result, relationship traversal, snapshot, context use, and artifact
  fetch; row-level database policy as defense in depth where practical.
- Strict schema, byte, node, depth, list, recursion, rate, and time bounds before
  expensive verification or persistence.
- Immutable digest-pinned agent packages and adapter manifests, dependency
  locks, provenance/signature recording, vulnerability and policy scanning,
  explicit rollout, and disable/revoke.
- Default-deny production worker network, filesystem, process, syscall, and
  resource policy; no ambient SQL, broker, cloud, model, connector, Akson, or
  governance credentials.
- Brokered model/tool/connector access with current dual fences, exact consumed
  authority, classification, disclosure, destination, budget, and credential
  separation.
- Artifact content-type verification, aggregate and per-item limits,
  malware/secret checks, inert rendering, safe download disposition, no
  automatic URL fetch, and refusal of scripts, event handlers, active data URIs,
  external resources, DOCTYPE, and entities where relevant.
- Rate, storage, compute, proposal, position, Call, Pledge, Assembly, wake,
  model, tool, and disclosure quotas by source, principal, Participant, Society,
  lineage, peer, and operation.
- Structured risk cards for consequential human decisions, showing exact
  changes, recipients, data, spend, authority derivation, independence,
  reversibility, provider claims, and ambiguity.
- Trusted approval chrome that visually quotes and source-labels all untrusted
  prose; exposes or removes bidi/control characters and homoglyph ambiguity;
  disables participant-supplied links and active resources; and renders canonical
  recipient, destination, amount/currency, cumulative ceiling, classification,
  irreversibility, and subject digest outside participant-controlled content.
  The fresh authentication challenge binds both the subject and a digest of the
  representation shown.
- Tamper-evident audit with external high-water anchoring where available.
  Chain verification alone MUST NOT be marketed as tail-truncation resistance.
- Recovery and key-rotation exercises, dependency patch policy, fuzzing,
  hostile-input suites, and incident procedures.

### 19.3 Prompt injection

Prompts and all participant/peer/model content are data. They cannot:

- select the authenticated actor or independence domain;
- add a Participant, Assembly member, root human, or project member;
- create Standing, Pledge assent, Position, Decision, Mandate, or permit;
- raise a budget, deadline, rate, retention, delegation, or concurrency limit;
- widen context, classification, recipient, provider, tool, filesystem, network,
  or workspace scope;
- mark a Delivery verified, reviewed, fulfilled, or safe;
- admit an Engram, artifact, remote result, or Kovee contribution;
- wake across an admission boundary;
- fetch a URL or load an external resource;
- weaken or choose its own sandbox.

The secure worker's lack of ambient authority is enforcement. A system prompt
asking an agent to behave is not a security boundary.

### 19.4 Human safety and control

- Every human-authority act binds a recent authentication observation at the
  assurance level the Charter requires.
- High-risk rules support thresholds, veto, separation of duties, distinct
  controllers, cooling-off periods, expiry, and independent review.
- A denial or refusal is final for that proposal revision; retrying with changed
  terms creates a visibly new subject.
- StandingMandates expose cumulative use and remaining ceilings, not only
  individual harmless-looking actions.
- Emergency holds are fast, scoped, auditable, and cannot be released by the
  held participant.
- UI language never says an agent understood, consented morally, proved truth,
  or completed merely because a protocol state advanced.
- Notifications coalesce and rate-limit to prevent approval denial-of-service.
  Absence of a response stalls or expires; it never becomes assent.
- Typed confirmation, cooling-off, and a second distinct principal are available
  for irreversible or unusually large actions; deceptive rendering is part of
  the hostile-input conformance suite.

## 20. Privacy and data lifecycle

### 20.1 Classification

Every Society binds a closed classification vocabulary forming a finite
join-semilattice or total order with an explicit join. In Kovee-hosted mode this
is an exact Kovee Realm vocabulary revision plus an optional stricter Byom
overlay; standalone mode uses a Byom-owned vocabulary. Contributions,
contexts, Pledges, Mandates, Engrams, Deliveries, evidence, Episode state,
artifacts, checkpoints, model/tool outputs, and disclosure items carry or derive
a known label.

Provenance is explicitly `declared` or `enforced_complete`. Derived output with
enforced-complete provenance inherits at least the join of every input, hidden
instruction, tool result, checkpoint, and transformation that influenced it.
Output from an attached harness, provider with hidden state, private
ContinuityRoot, or other runtime with unobservable inputs receives the
Society's top label or remains quarantined. A Manifestation's declared maximum
is evidence, not a security bound; a lower ceiling applies only when an approved
attestation proves the complete readable-source set under a confinement/IFC
profile. A Participant or model may raise a label but never lower it.
Unknown, missing, or incomparable labels fail closed.

Labels are source-qualified by policy revision. Cross-realm or cross-Society
admission uses a destination-approved immutable mapping revision. Labels are
never compared by spelling. A mapping may preserve or raise restriction;
lowering requires declassification.

~~~text
SocietyClassificationBinding {
  binding_id, society_id, mode: kovee_hosted | standalone,
  vocabulary_owner, vocabulary_ref, vocabulary_revision, vocabulary_digest,
  stricter_overlay_ref?, stricter_overlay_digest?,
  adopted_by_decision_ref, dependency_set_ref, status, digest
}

ClassificationOverlayRevision {
  overlay_id, society_id, revision, previous_digest?,
  base_vocabulary_ref, base_vocabulary_revision, base_vocabulary_digest,
  overlay_vocabulary_definition, overlay_vocabulary_digest,
  base_to_overlay_pairs[]: {base_label_digest, effective_label_digest},
  proof_of_monotonic_restriction_digest,
  expires_at?, status: active | revoked | superseded | expired,
  adopted_by_human_decision_ref, dependency_set_ref, digest
}

ClassificationMappingRevision {
  mapping_id, owner_destination_society_id, revision, previous_digest?,
  source_owner, source_endpoint_ref, source_binding_epoch,
  source_vocabulary_ref, source_vocabulary_digest,
  local_classification_binding_ref, local_classification_binding_digest,
  direction: inbound_foreign_to_local,
  label_pairs[]: {source_label_digest, local_label_digest},
  permitted_transformation_classes[], declassification_class: none | explicit,
  expires_at, status: active | revoked | superseded | expired,
  adopted_by_human_decision_ref, dependency_set_ref, digest
}

OutboundClassificationRevision {
  outbound_id, owner_source_society_id, revision, previous_digest?,
  local_classification_binding_ref, local_classification_binding_digest,
  remote_endpoint_ref, contract_vocabulary_ref, contract_vocabulary_digest,
  label_pairs[]: {local_label_digest, contract_label_digest},
  permitted_transformation_classes[], disclosure_ceiling_ref,
  expires_at, status: active | revoked | superseded | expired,
  adopted_by_human_decision_ref, dependency_set_ref, digest
}
~~~

An inbound mapping is owned only by the destination Society that will admit the
foreign object. An outbound revision is owned only by the source Society and
states which local label may be represented by which neutral contract label; it
does not assert the remote Society's label or enforcement. Under
`society_mapped_round_trip`, the remote endpoint independently chooses its
inbound mapping and binds that digest in its Akson acceptance, so the round trip
cites four potentially distinct records: source outbound, remote inbound,
remote outbound, and local inbound. Under `akson_neutral_contract`, no remote
Society mapping is invented: source outbound and local inbound surround exact
Akson accepted-contract constraints and neutral result labels as specified in
section 17.2.

Both record types remain separate from Kovee's Realm/Space mappings.
Peer-supplied mappings are inert proposals. Revocation blocks new admission,
context materialization, or dispatch under the old record; prior disclosure
remains history. Any pair that lowers local restriction also requires the exact
declassification decision and derived object.

### 20.2 Declassification

Declassification is a non-delegable or expressly human-reserved ActIntent over:

- exact source bytes and classification;
- exact transformed output bytes;
- transformation provenance;
- reason and risk analysis;
- destination and purpose;
- decision and disclosure manifests.

It creates a new derived object. It never relabels or mutates the source.
Redaction, summarization, encryption, or a model assertion does not
automatically declassify.

Opaque-agent output lacking enforced-complete provenance may be declassified
only by a human review of exact bytes and sources under the declassification
procedure, or by a narrowly approved information-flow-constrained transform
whose attestation proves it could not observe excluded data.

### 20.3 Visibility and metadata privacy

Society, Assembly, or Endeavor membership grants no ambient visibility.
Authorization applies to:

- object payload and existence;
- member and controller metadata;
- event replay and live delivery;
- search hits, counts, timing, relation endpoints, and error behavior;
- artifacts and context materialization;
- Pledge, Mandate, decision, budget, and provider details.

Not-found and forbidden behavior avoids membership and object-existence oracles.
Directory queries reveal only the minimum eligible candidate data. Controller
and independence-domain graphs are protected and used only for deterministic
checks and authorized audits.

Cross-participant context is an explicit disclosure. An Assembly does not merge
member inboxes or memory. A collective position may reveal the member positions
required by its Charter but not unrelated private deliberation.

### 20.4 Retention, erasure, and export

Retention policies separately name:

- domain state and immutable decision history;
- event metadata and event payloads;
- human rationale and participant journals;
- model prompts, responses, and provider records;
- ContextManifests and Continuations;
- Engrams and local trust state;
- artifacts, workspaces, checkpoints, Deliveries, and evidence;
- idempotency and effect-deduplication records;
- controller/independence metadata;
- backups, replicas, caches, and external copies.

~~~text
ErasureRequest {
  erasure_id, society_id, revision,
  requested_by_ref, target_object_refs[], target_field_selectors[],
  purpose_and_basis_ref, retention_rule_refs[],
  required_external_copy_actions[],
  exact_subject_digest, authorization_decision_ref?,
  state: requested | awaiting_decision | authorized | executing |
         verified | partial_external | denied | failed,
  dependency_set_ref, created_at, digest
}

ErasureJournalEntry {
  journal_entry_id, erasure_ref, erasure_revision,
  destroyed_object_key_refs[], destroyed_payload_refs[],
  retained_typed_commitment_refs[], outstanding_external_copy_refs[],
  endpoint_incarnation, authority_journal_generation,
  prior_erasure_journal_digest, completed_at,
  external_witness_receipt_ref, digest
}
~~~

Normal edits create new revisions and tombstones. Authorized erasure removes or
cryptographically shreds payloads and blobs while retaining only the minimum
permitted metadata, digests, and fact that an authorized record once existed.
Materializing an old context after erasure fails; it never substitutes newer
content.

Effect and idempotency dedupe keys live at least as long as an external replay
could repeat harm. They may retain non-sensitive digests after payload erasure.
Legal hold, if supported, is explicit and visible to authorized participants.
Backup expiry and replica deletion are part of the erasure claim.

Every advertised deployment publishes a field-level RetentionRule matrix naming
the record field or blob class, purpose, lawful/charter basis, maximum live and
backup duration, legal-hold behavior, erasure method, residual metadata, and
known provider/peer copies. Low-entropy or identifying content that must become
unrecoverable uses per-object encryption keys and, where a retained correlation
is necessary, a `local_erasure_safe` commitment rather than a public plain hash.
Choosing `portable_public` is an explicit non-erasable-hash disclosure shown to
the owner; the system never silently upgrades erasable content to portability.

Key destruction and erasure decisions append to a non-rollbackable witnessed
erasure journal outside ordinary backups. Restore reapplies that journal and
destroys barred keys before any read surface opens. Byom never claims deletion
from a provider, connector, recipient, backup custodian, or Akson peer it cannot
verify; it records an outstanding external-copy obligation and any received
deletion receipt as evidence only.

Export is source-qualified and classification-aware. It does not turn local
trust, Standing, Mandates, or controller metadata into portable facts.

### 20.5 Encryption claims

Server-side agent processing is incompatible with operator-blind end-to-end
encryption for the processed plaintext. Encrypted-at-rest team mode MUST NOT be
marketed as E2EE from the operator. A future opaque project may disable
server-side processing and expose a different feature claim.

Provider-visible and peer-visible plaintext boundaries appear in the risk card
and DisclosureManifest. Credentials remain outside those plaintexts.

## 21. Failure semantics

| Failure | Required result |
|---|---|
| Authority-journal and SQL finalize succeed, then reply is lost | Exact idempotent replay returns the original retained result or non-reexecuting tombstone with the same journal receipt. |
| Process dies before SQL prepare | No pending state, event, reservation, Position, Decision, outbox job, or idempotency result exists. |
| SQL prepare commits before journal CAS | Transition remains invisible/unusable; recovery retries the exact transaction or proves no entry before abandoning it. |
| Journal CAS succeeds before SQL finalize | Recovery queries the transaction id, verifies the receipt, and finalizes the exact pending set once; no reply or permit was released early. |
| Journal CAS reply is lost | State is `witness_unknown`; query by transaction id decides, never a second blind CAS. |
| SQL is rolled back behind the journal | Endpoint starts sealed diagnostics only; it cannot skip the missing journal generation or expose payload reads. |
| Delivery broker is unavailable | Committed outbox work remains; lag is visible; authority state is unaffected. |
| Event or job is duplicated/reordered | Inbox and business idempotency reduce duplicates to no-op; state preconditions determine order. |
| Two participants try to fill one seat | Exact unique seat key admits one current Position; changed subject requires a new proposal. |
| One controller appears through nested seats | Independence closure detects the duplicate and blocks the decision. |
| Assembly membership changes during decision | Epoch mismatch invalidates pending positions; no silent recount. |
| Pledge formation crashes | Prior Positions remain immutable inputs; the locked snapshot, GovernanceDecision, Pledge, Byom reservations, initial Mandate/PledgeWorkstream, and events exist together or not at all. |
| Pledge amendment races an Episode or Delivery | Revision locking chooses one transition; old work is fenced and late output stays attributable only to old terms. |
| Worker dies before output | Lease expires; a new attempt resumes only from a committed compatible checkpoint. |
| Old worker returns | Every mutation, context use, child formation, effect, continuation, and Delivery fails stale lease. |
| Runtime result committed but Byom reply lost | Same logical operation recovers one EpisodeCompletion; a later Participant-authored Delivery remains separate. |
| Byom permit consumed and host loses reply | Same stable execution key recovers the identical receipt; driver is not called twice. |
| Non-idempotent external outcome unknown | ActIntent/effect becomes ambiguous; reservations remain conservative; explicit reconciliation only. |
| Participant or Manifestation suspended | New work and uses stop; active Episodes fence; dependent collective decisions and Mandates re-evaluate. |
| Context item erased/reclassified/revoked | Old manifest cannot materialize; new explicit omission needs a new digest and current policy. |
| Budget settlement ambiguous | Reserved amount is not released until reconciled. |
| Kovee subordinate reservation reply is lost | Episode remains unqueued; saga queries the stable reservation key and either links the exact Kovee reservation or keeps Byom quantity uncertain. |
| Private or opaque state influences output | Output receives the Society top label or quarantine unless approved attestation proves the complete readable-source ceiling; missing provenance never lowers classification. |
| Control domains merge after positions exist | Pending decisions invalidate and dependent Mandates hold; historical attribution remains. |
| Delivery arrives late or after supersession | Retained as evidence; cannot satisfy new terms or wake changed work. |
| Review conflicts or policy candidates conflict | Affected work blocks under typed dispute/conflict; never last-write-wins. |
| Akson material arrives | Remains inert until verification, classification mapping, and local admission. |
| Remote cancel reply is lost | No claim that remote execution stopped; status remains advisory/unknown. |
| Projection is stale | UI shows source cursor/lag; all commands and decisions reload source; stale data never authorizes. |
| Database restore or rollback | External journal/checkpoint mismatch starts sealed diagnostics only; a new endpoint incarnation and recovery epoch fence all old channels, positions, cursors, contexts, leases, permits, Kovee bindings, and Akson contracts before payload reads reopen and effects reconcile. |
| Authority, access-audit, or erasure witness is unavailable or mismatched | Sealed diagnostic-only start; no payload read/search/export, production mutation, plaintext restoration, or federation. |
| Clock skew | Authority server time controls leases, deadlines, expiry, and policy. |
| Artifact seals but SQL finalize is lost | Reconciler verifies exact sealed version and commits once or garbage-collects; staging bytes stay invisible. |

## 22. Operations and observability

The typed event ledger and causal references answer:

- which participant proposed, assented, refused, delegated, delivered, or
  reviewed;
- which Assembly epoch and decision rule produced a collective act;
- which root human decision and Mandate chain authorized an effect;
- what exact context, hidden instruction chain, package, profile, provider,
  workspace base, tool, and output digest were involved;
- which admitted event made an Episode eligible;
- what was reserved, spent, released, held, or ambiguous;
- why an ActivityStream, decision, or disclosure is blocked.

Operational telemetry uses OpenTelemetry or an equivalent open format with
destination-authorized, retention-bound, scope-keyed pseudonyms for Society,
Endeavor, Pledge, ActivityStream, Episode, effect, causation, and correlation.
Stable cross-Society Participant ids are not exported. Telemetry excludes
secrets and disallowed content. Model transcripts and participant journals are
not copied into general traces.

Health reports storage, event/outbox lag, recovery sweep, budget reconciler,
runtime availability, Akson adapter, snapshot coverage, key custody,
classification policy, secure-profile enforcement, and audit-anchor status.
Readiness fails for affected features rather than silently downgrading them.

Backups preserve ids, canonical bytes, digests, event order, idempotency,
budget reservations, consumed permits, ambiguity, encryption metadata, and
recovery epochs. Restore tests prove external witness comparison and reapply the
erasure/key-destruction journal before any read or mutation surface opens.

Initial service objectives are published only with topology and feature bundle.
No generic exactly-once, zero-loss, secure, private, or highly available claim
is made without its precise boundary.

## 23. Conformance and verification

### 23.1 Protocol conformance

Every advertised bundle has:

- JSON Schemas for requests, results, events, records, manifests, and problems;
- I-JSON, JCS, every typed digest class, typed-byte, Engram, and receipt golden vectors checked by
  Rust, Python, and TypeScript;
- positive and negative state-transition vectors;
- authorization, wrong-surface, wrong-actor, stale revision, stale epoch,
  stale-fence, limit, unknown-enum, and extension tests;
- machine-readable transition descriptors, BPA-1 algebra vectors and laws,
  BDPL bounds, deny-by-absence operation-registry coverage, and model-checked
  critical machines;
- specification-CI proof that every mutating catalog operation and named
  internal transition has exactly one closed descriptor and at least one
  authorized plus wrong-actor/wrong-surface vector;
- authority-journal, privacy-access-journal, and erasure-journal crash vectors
  at SQL prepare, CAS before/after, lost witness reply, finalize, rollback, and
  restore;
- idempotent replay across crash after commit and before reply;
- snapshot/cursor expiry and authorized recovery;
- old/new minor negotiation and at least two independent clients.

### 23.2 Agentic-nativity suite

The suite proves:

1. No participant can be assigned a Pledge without its current authenticated
   direct or provenance-labelled policy assent; a Charter, runtime, collective,
   administrator, or controller outside the Participant channel cannot install,
   reactivate, or broaden its ParticipantAssentPolicyRevision.
2. Removing every steward/coordinator leaves proposal, Pledge, review, and
   ActivityStream semantics functional.
3. Two competing plan artifacts coexist; neither can add a dependency, reserve
   a resource, wake an Episode, close work, or suppress a direct competing
   Pledge.
4. A hosted episodic Participant independently notices an admitted event,
   explores it, proposes an Endeavor or Call, negotiates, and revises its own
   future activation strategy without first receiving a Pledge.
5. An Assembly is co-authored through nominations, open seats, counterproposals,
   and charter revisions, then forms from separately attributable member
   positions.
6. A collective-owned Manifestation operates continuously inside a pinned
   executive policy and Mandate, starts low-risk Calls/Pledges/Activities without
   per-act plenary voting, and is fenced when the Assembly reforms.
7. Nested collectives cannot multiply one independence domain into a threshold;
   diamond membership, indirect cycles, domain merges, and epoch changes have
   deterministic results.
8. Collective formation does not expose member context, combine Mandates, or
   aggregate budgets.
9. Participant identity, ContinuityRoot, and ActivityStream survive process, Manifestation, model,
   and host replacement under explicit compatibility policy.
10. Attached, episodic, human, and collective participants use the same durable
   proposal/Pledge semantics.
11. Refusal, dissent, amendment, relinquishment, unconditional participation
    cease, Assembly withdrawal, and alternative strategies stay
   visible and do not become failure or consensus automatically.
12. A BDPL-defined jury, rotation, lottery, and bicameral rule can evolve the
    institution but cannot synthesize a Position or bypass a core invariant.
13. Model ranking, synthesis, planning, and natural-language approval cannot
    create eligibility, Standing, assent, Mandate, review, or effect.
14. An admitted event, Kovee Attention item, cron, ranker, model, kernel, or host
    cannot create a WakeIntent; only a direct Participant request or an exact
    participant-owned ActivationPolicy use can, and no activation stage can be
    collapsed or skipped.

### 23.3 Authority, safety, and privacy suite

The suite proves:

1. Cross-Society/Kovee-scope reads, counts, searches, events, artifacts, and
   existence probes fail closed.
2. A body cannot claim another principal, Participant, collective, runtime,
   Episode, or peer.
3. Mandate derivation checks BPA-1 subset in every dimension and reserves every
   parent ceiling; independent implementations agree on canonical edge vectors
   and no concurrent race widens or overcommits.
4. Every consequential effect has one semantic owner, exact subject and
   disclosure, one consumed receipt, current fences, and crash-honest outcome.
5. Production participants cannot reach SQL, delivery broker, credentials,
   provider, Akson, governance/admin socket, undeclared path, or network around
   the broker.
6. Prompt-injected content cannot admit, wake, authorize, spend, disclose,
   classify, review, or execute.
7. Every context item is exact, ordered, classified, audience-authorized, and
   rechecked at materialization; erasure and revocation fail explicitly.
8. Participant/Assembly membership changes immediately invalidate future reads,
   positions, mandate uses, and live delivery as applicable.
9. Budget races preserve `ceiling = remaining + reserved + committed + uncertain
   + delegated_to_children` in every dimension; Kovee bridging cannot double
   charge or queue before both reservation sets exist.
10. Artifact seal/scan/finalize crash injection never exposes unverified bytes.
11. Cross-realm labels require exact active mapping; declassification creates a
    new derived object and cannot be supplied by a model.
12. Backup restore preserves evidence and ambiguity while invalidating
    pre-restore authority.
13. Akson arrival cannot become local visibility, Standing, evidence weight,
    attention, or execution before verification and admission.
14. Mutation and privacy-access audit verification detects retained-record
    mutation; managed sensitive reads are externally journaled before bytes are
    released, and periodic audit claims state witness lag precisely.
15. Mutating each mandatory dependency independently between prepare, finalize,
    materialize, claim, and consume rejects the operation; callers cannot omit a
    dependency from closure.
16. Every unregistered `(operation, surface)` pair is denied, including runtime
    assent, admin finalization, participant effect observation, and projection
    mutation.
17. Stolen bearer tokens fail sender proof; revocation closes channels; external
    runtimes cannot self-attest a secure profile.
18. No authority transition or permit becomes visible before its synchronous
    external journal CAS and SQL finalize; every intervening crash/rollback state
    either recovers exactly or exposes diagnostics only.
19. Control-domain unknowns do not count as independent, distinct human counts
    use distinct source principals, and later merges hold dependent authority.
20. Opaque private state receives Society-top classification or quarantine; only
    an approved attested complete readable-source ceiling permits less.
21. A Byom-tagged Kovee action cannot fall back to standalone Kovee authority or
    a participant's parallel ambient credential.
22. Trusted approval rendering defeats bidi/control/homoglyph/hidden-diff/link
    tricks and the authentication challenge binds what was shown.
23. Opaque event tokens reveal no hidden global count; telemetry exports only
    authorized scope pseudonyms.
24. Low-entropy erased content cannot be recovered from public digests or backup
    rollback: typed digest schemas reject public hashes for erasable plaintext,
    and outstanding provider/peer copies remain explicit.
25. DNS rebinding, private-address redirects, archive bombs, graph diamonds,
    dependency explosions, and evaluator time exhaustion fail within quotas.
26. Each advertised Akson operation passes its exact capability matrix; a weaker
    rollback, budget, key-expiry, evidence, or processor-visibility profile is
    rejected, and peer assertions never masquerade as confidentiality evidence.
27. A candidate credential can author only exact membership refusal/acceptance
    and initial self-policy proposals; runtime output cannot cross that surface,
    and admission atomically fences/converts the credential. Refusal appends one
    immutable receipt, can retract and cite a prior acceptance, races admission
    on the same offer revision, terminally fences the
    offer/onboarding/credential, and every later acceptance or exact retry has
    the closed result specified in section 7.4. Accepted offers also expire by
    that same CAS rather than becoming indefinite assent.
28. Hosted onboarding cannot call a model without a one-shot Society-authorized
    compute receipt binding final provider bytes, disclosure, profile, budget and
    fence; it grants no reusable effect authority.
29. Token-bucket boundary/race/restore vectors prevent double burst and
    hierarchical rate amplification; path-open tests defeat symlink, hard-link,
    rename, magic-link and mount-crossing TOCTOU.
30. BDPL lot selection pins its SeedPolicy before eligibility closes. Vectors
    cover VRF key/input derivation, exact beacon round/finality, committer
    snapshot, commit/reveal deadlines and threshold, missing-reveal sentinel,
    post-reveal beacon combination, terminal source failure, and one unique seed
    attempt; source selection, last reveal, proposal/revision grinding, abort,
    and retry cannot choose a favorable seed.
31. Every sensitive allowed/denied read, search, materialization, export, backup,
    key and admin action has a non-plaintext access record; managed mode releases
    no sensitive result before its external access-journal receipt.
32. Specification CI proves one closed transition descriptor for every mutating
    operation and named internal transition; no wildcard state semantics remain.
33. Kovee formation crash vectors cover `kovee_endeavor_form`, the exact
    intent/slot unique key, Realm/Byom binding ref/revision/epoch/digest,
    generation, stable semantic command, fresh attempt proof, canonical
    IdempotencyDomain, Society epoch, principal/actor binding, sole seat, lost
    replies, signed result envelope, absence versus non-reexecuting tombstone,
    historically fenced absence, delayed delivery, all terminalization union
    variants/no-op rows, terminalization race, binding rotation, and zero-,
    one-, or multi-hop complete, incomplete, broken, cyclic, or discontinuous
    RestoreLineageProof lookup before result query/link commit. Native Society
    bootstrap is proved separately and never enters this command.
34. ActIntent prepare, participant/human Position, deterministic finalization,
    cancellation and runtime-only consume are all reachable only on their
    registered surfaces.
35. Formation reconciliation treats a point-in-time `absent` result only as
    `awaiting_principal`, retains the uniqueness slot across delayed delivery,
    and releases after possible submission only on a verified restore-safe
    non-reexecuting tombstone, a signed historically fenced absence backed by a
    complete RestoreLineageProof, or a successful link.
36. `akson_byom_exchange_v1` rejects mixed classification-profile fields:
    request, acceptance, result, and local-admission shapes each bind the exact
    predecessor and contain only fields knowable to that phase. Society round
    trips progressively require source-outbound, remote-inbound,
    remote-outbound, then local-inbound mappings; an Akson-only worker round
    trip instead requires neutral contract/result labels, exact Akson acceptance
    constraints, source-outbound/local-inbound mappings, and no fabricated
    remote Society refs at any phase. Cross-record mutation tests independently
    swap phase digests/profile, endpoint identity epochs, Akson acceptance,
    ActIntent, MandateUse, ExecutionConsumptionReceipt, execution key, delivery
    key, result manifest, Kovee outcome receipt, EOA revision/head, admitting
    Society, and mapping owner/vocabulary; every splice fails and the
    EOA/local-admission atomic set remains absent. Receipt-union vectors cover
    failure before dispatch/result, unknown dispatch/result/verification,
    terminal versus non-terminal verification rejection, verified
    succeeded/failed results, exact replay, changed branch, late result after
    final failure/rejection, and ambiguous-to-final CAS. Genesis/predecessor,
    contiguous revision, same-identity, concurrent-successor, fork, and terminal
    receipt-head vectors are closed. Pre-result, rejected, and ambiguous
    branches create no classification admission; every EOA host
    protocol/endpoint/effect/receipt/outcome field is equality-checked.
37. Concurrent `continuation_write` requests for one Activity generation and
    expected head revision produce one winner and one stale-head result; a stale
    Episode or Manifestation cannot fork or advance the current Continuation.
38. Ambiguous-effect reconciliation vectors separate source fact from local
    judgment. A signed final host successor is committed under the host head
    before runtime `effect_outcome_admit`, forbids a GovernanceDecision, and is
    independently CAS-admitted by Byom even when a local disposition exists.
    Governance `effect_reconcile` requires the exact decision fields, appends to
    a separate disposition head, preserves the EOA/ActIntent source state, and
    cannot be called on the runtime surface. Vectors cover local disposition
    before a matching and conflicting late source final, source admission that
    cannot be blocked by the disposition head, deterministic
    `active_ambiguous → source_advanced` fencing, classification plus quarantine
    of verified late bytes, fresh-decision release, and unchanged conservative
    settlement until source evidence arrives. Neither path performs or claims a
    shared cross-owner CAS.

### 23.4 Chaos and scale

The production gate runs at least 100 concurrent scripted participants,
including nested Assemblies, with injected duplicates, response loss, worker
kills, broker outage, SQL failover, slow consumers, poison data, membership and
Mandate revocation, provider timeout, Kovee projection lag, and Akson delay.

It asserts no lost accepted mutation, forged assent, stale write, multiplied
vote, widened mandate, duplicated effect, budget overcommit, unauthorized
context, wake storm, false remote-cancel claim, or silently truncated audit.

Performance claims publish exact topology, feature bundle, workload,
participant mix, Assembly depth, payload size, durability, queue age, error
rate, and latency percentiles.

## 24. Delivery plan

### B0 — Specification and adversarial model

- Stabilize vocabulary, schemas, canonical encodings, state machines, threat
  model, security profiles, and source-ownership map.
- Publish BPA-1, BDPL, dependency-closure rules, operation registry,
  machine-readable transition descriptors, model checks, and external-witness
  recovery profile.
- Publish conformance runner and golden vectors before daemon behavior becomes
  de facto protocol.
- Resolve Kovee delegated-principal and Akson narrow-surface prerequisites.

Exit: two independent clients validate envelope, actor binding, idempotency,
proposal/position formation, Mandate subset, decisions, events, limits and
failures, and two independent evaluators agree on every policy/transition vector.

### B1 — A society of two

- Personal Society, human and one agent Participant.
- Charter, Standing, Endeavor, Call, Pledge, Mandate, ActivityStream, Episode,
  Delivery, Review, events, budgets, and developer profile.
- One Kovee-hosted episodic participant and one attached harness.
- Kill-and-resume using a portable Continuation.
- One bounded exploratory ActivityStream initiated by the agent's own activation
  policy before it accepts any Pledge.

Exit: neither participant is centrally assigned; the agent first notices and
explores, then explicitly accepts one Pledge, receives one bounded Mandate,
adapts its own activation policy, survives runtime replacement, and delivers a
reviewable change-set with a causal timeline.

### B2 — Self-formed assemblies

- Co-authored FormationProcess, collective Participant and Manifestation,
  executive policy, BDPL decision procedures, ControlDomainRevisions, nested
  graph, succession, disputes, competing strategies.
- StandingMandates with aggregate ceilings and circuit breakers.
- Confined and secure profiles before production effects.

Exit: an Assembly co-forms, operates continuously inside bounded executive
authority without per-act plenary voting, reforms, and dissolves without
authority laundering, duplicated controllers, ambient context, or coordinator
privilege.

### B3 — Kovee-native society

- Space-frontier to Endeavor formation.
- Full `byom_governed_work_v1` Kovee schema and operation-matrix bundle.
- Kovee views, ContextManifest admission, Episode/Invocation dual fencing,
  subordinate budgets, workspace and effect consumption.
- ProviderContextManifest and exact model/tool disclosure.

Exit: Kovee can host the complete B1/B2 flow without shared tables, duplicate
approval, hidden context, owner ambiguity, authority fallback, or private Sage
semantics.

### B4 — Institutional memory and evidence

- Engram import/admission/attestation, deterministic retrieval, policy conflict,
  context overflow, synthesis, disclosure bundles.
- Participant claim/evidence directory and local observed outcomes.

Exit: a new Manifestation continues from portable records and admitted memory;
quarantined or contradictory knowledge cannot become authority.

### B5 — Sovereign others

- `akson_byom_exchange_v1`, independently consented Akson-worker remote-task
  lifecycle, signed evidence, classification mapping, admission, late outcomes,
  advisory cancellation, and exact capability negotiation.

Exit: two installations complete a bounded round trip with independent consent
on both sides and no shared Society, broker, credential, or authority. Inbound
Byom/Kovee execution, monetary ceilings, processor-hidden fields, or other
unenforced dimensions are explicitly excluded until their named Akson profile
exists and passes conformance.

### B6 — Managed operation

- PostgreSQL team profile, external identity/KMS, workload identity, HA inside
  one write boundary, monotonic recovery witness, audit anchoring, restore,
  retention/erasure journal automation,
  observability and published service objectives.

Exit: chaos, privacy, restore, revocation, and secure-profile suites pass for
the advertised managed topology.

No phase advertises a later feature as partially safe. Unsupported behavior is
absent and feature negotiation says so.

## 25. Migration from Sage

Byom is a clean successor design, not a blind rename. Sage is still design-stage,
so migration should preserve evidence without carrying central orchestration
semantics as authority.

| Sage record | Migration rule |
|---|---|
| Mission | Import as Endeavor candidate with LegacyRef and original digest; human reviews Charter, sponsor, outcomes, budget and authority. |
| MissionMember | Import as proposed Standing or seat evidence; never auto-admit or create human authority. |
| PlanRevision | Import as attributed plan artifact and source for Calls/Pledge proposals; never canonical control state. |
| Aspect | Import only as source-qualified LegacyEvidence and an optional inert Call/PledgeProposal seed; run native Byom formation for any new Pledge. |
| Coordinator | Import as ordinary participant/activity history; no special authority. |
| Session | Import as historical ActivityStream evidence binding. |
| Turn and lease | Import as Episode/attempt history with original fences and source refs. |
| Gate validation | Import only as source-qualified LegacyEvidence; it cannot become a GovernanceDecision or current authority without a native Byom proposal, complete eligibility/slot snapshot, positions, dependencies, and finalization. |
| Standing rule | Re-propose as StandingMandate under current Charter; never silently activate. |
| Directory | Import claims with original provenance; no evidence upgrade. |
| Engram | Preserve canonical portable bytes and digest; import local trust state as local historical state, never portable truth. |
| Delegation | Preserve Akson task, peer binding, intent, consent, evidence and outcome references; local admission is reviewed. |

Ids and digests are never rewritten to look native. Source-qualified LegacyRefs
and migration events preserve causality. A migration cannot manufacture missing
assent, collective decision, Manifestation compatibility, classification
mapping, authority, or effect certainty.

Direct native conversion is allowed only under a separately versioned migration
compatibility profile that proves every required Byom field, actor binding,
assent mode, eligibility and independence snapshot, Charter/Standing revision,
dependency, budget, classification, fence, and authority invariant. Current Sage
implements no such profile. Shape similarity, performer name, validation, or
pending permit is never sufficient.

Cutover is a fenced state machine owned by Kovee's Realm binding:

~~~text
GovernanceCutover {
  cutover_id, realm_ref, branch_selector,
  source_owner: sage, target_owner: byom,
  state: planned | frozen | reconciling | importing | ready |
         activated | completed | failed,
  source_binding_epoch, target_binding_epoch,
  reconciliation_digest?, inert_import_digest?,
  activation_decision_ref?, created_at, digest
}
~~~

1. `planned → frozen` locks the exact non-overlapping scope and compare-and-swaps
   its one KoveeGovernanceOwnerBinding from `sage` to `none` at the expected
   revision/binding epoch; it then rejects new Sage and Byom governed actions.
2. Reconciliation closes or holds every Sage turn, lease, budget, effect,
   external formation slot, and ambiguous outcome; unresolved facts remain held.
3. The binding and recovery epochs advance. Sage history imports only as
   source-qualified inert records and portable Engram bytes/digests.
4. Native Byom Society, Standing, Pledges, decisions, Mandates, budgets, and
   effects are freshly formed or reauthorized.
5. One exact activation decision compare-and-swaps the same binding from `none`
   to `byom`, installs the target endpoint/binding, and advances its epoch. The
   uniqueness and non-overlap constraint means no scope can have both `sage`
   and `byom` authority.

Before activation a failed cutover may return to Sage only under another binding
epoch after reconciliation. After activation, returning to Sage is a new cutover,
not rollback of the database.

The old Sage repository remains unchanged until an explicit implementation
migration is authorized. Kovee references to Sage remain honest descriptions of
the current design, not aliases that pretend Byom is already implemented.

## 26. Non-goals and open dependencies

Byom does not build:

- a model loop, prompt framework, semantic planner, central coordinator, or
  universal role DSL;
- a universal ontology, truth engine, moral patient model, legal-personhood
  system, employment system, or autonomous corporation;
- a global identity, reputation, participant namespace, skill marketplace,
  token, or governance network;
- a peer transport, shared cross-sovereign database, global roster, or
  distributed consensus protocol;
- a general credential broker, worker sandbox, model proxy, tool registry,
  artifact renderer, or workspace engine inside byomd;
- a requirement to record hidden chain-of-thought or centralize participant
  memory;
- exactly-once computation or a claim that cancellation undoes effects;
- opaque operator-blind E2EE for content processed by server-side agents;
- automatic semantic conflict resolution or last-write-wins governance.

Open delivery dependencies:

- Kovee must adopt the Byom adapter and BPP prerequisites in section 16 before
  claiming integration.
- Akson must expose and conform the least-privilege coordination/consent surface
  before automated federation.
- Inbound remote Byom/Kovee execution requires Akson's confined agent-worker
  profile to be normative.
- Production secure claims require a host that enforces the recorded worker,
  broker, filesystem, network, credential, and artifact controls.
- Key custody, audit tail anchoring, provider cost accounting, output disk
  quotas, field-level processor visibility, and complete evidence-slot
  verification must be reported according to actual implementation status, not
  assumed from this design.

## 27. Resolved design decisions

1. The project is **Byom**, pronounced biome.
2. The product thesis is a living society of autonomous participants, not a
   swarm under an orchestrator.
3. Kovee collaborates and executes; Byom governs collective agency; Akson
   crosses sovereign boundaries.
4. Participant kinds are human, agent, and collective; services and peers do
   not silently become participants.
5. A Pledge means will; a Mandate means may; neither substitutes for the other.
6. A plan is a lens over authoritative Pledges and dependencies.
7. A coordinator has no protocol privilege; stewardship is an ordinary bounded
   social function.
8. Assemblies are recursive first-class participants with separate authority,
   exact decisions, independence checks, and no ambient member context.
9. ActivityStreams are participant-owned durable continuity; Episodes are bounded
   runtime slices.
10. The deterministic kernel is model-free and effect-free.
11. Engrams retain portable content plus local trust, without becoming a shared
   mind or executable policy.
12. Kovee is the reference host; BPP remains independently implementable.
13. Akson remains the only cross-sovereign authority and carriage boundary.
14. Developer, confined, and secure profiles make enforcement claims honest.
15. Safety, privacy, authority, and crash semantics are conformance properties,
   not documentation aspirations.

## 28. The design in one sentence

**Byom lets humans, agents, and self-formed collectives live as autonomous
participants in a governed society—making voluntary Pledges, exercising bounded
Mandates, composing into accountable Assemblies, and adapting without a central
orchestrator—while Kovee hosts their collaboration and execution and Akson
carries exact authority across sovereign boundaries.**
