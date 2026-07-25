# Byom adversarial design review

Date: 2026-07-25

Reviewed target: `DESIGN.md` and `README.md`, design specification v0.2.

This record summarizes independent review passes and the resulting design
changes. It is not a production security certification. Byom is a
pre-implementation specification, and Kovee/Akson compatibility remains gated
on the named protocol bundles and conformance suites.

## Method

Three independent first-pass reviews used different failure lenses:

1. **Agentic nativity:** look for a classical coordinator, compulsory task
   ontology, simulated consent, committee façades, shared-mind assumptions, or
   inability of episodic agents to originate activity.
2. **Kovee/Akson consistency:** compare semantic owners, current schemas,
   cross-database transactions, runtime fences, context, effects, migration, and
   cross-sovereign sequencing against the authoritative sibling designs.
3. **Safety, security, and privacy:** assume malicious participants, controllers,
   insiders, runtimes, content, peers, network inputs, crashes, replay, rollback,
   operator access, and incomplete provenance.

After revision, the same reviewers received the complete new text and were asked
to report only unresolved P0/P1 blockers. Local structural checks separately
covered Markdown fences, links, canonical spelling, whitespace, and stale
terminology.

Severity in this record means:

- **P0:** the design could not truthfully provide a core guarantee if implemented
  as written.
- **P1:** a protocol or integration blocker before the affected feature can be
  advertised.
- **P2:** useful hardening, clarity, or later-phase resilience.

## Pass 1 — agentic nativity

The first draft already separated Pledge, Mandate, Episode, Delivery,
verification, and Review; removed coordinator privilege; treated a plan as a
non-authoritative lens; preserved dissent; and kept participant identity outside
the runtime. The review nevertheless found three architectural reversions.

| Finding | Severity | Disposition in v0.2 |
|---|---:|---|
| A Charter could install an agent's automatic assent. | P0 | Added participant-owned `ParticipantAssentPolicyRevision` and exact `DerivedAssentReceipt`; infrastructure roles can only restrict/suspend it. Controller-operated Participant channels are permanently labelled controller-mediated through every descendant receipt. |
| A hosted episodic agent could run only while fulfilling a Pledge. | P0 | Replaced the Pledge-only Workstream with participant-owned `ActivityStream`; bounded exploration, deliberation, monitoring, learning, relationships, and negotiation need Mandate/budget but no obligation. Added participant-owned activation and four-stage wake/admission/allocation/placement. |
| A collective required a member vote for every act and had no durable manifestation. | P0 | Added collective-owned Manifestations, ContinuityRoot, and decision-derived bounded executive policy. Low-risk activity remains mandate-bound and fully attributed; reform fences it. |
| Admission and exit were not fully voluntary. | P1 | Added membership offer/acceptance, unconditional `participation_cease`, `assembly_withdraw`, non-attribution receipts, and separate obligation disposition. |
| Assembly formation was proposer-designed. | P1 | Added revisioned FormationProcess with open seats, nominations, counterproposals, charter amendments, and proposer succession. |
| Recursive containment was inconsistent. | P1 | Defined an authoritative bipartite graph, overlapping/diamond membership, transitive cycle checks, depth/edge limits, and independence closure. |
| Hard-coded procedures fixed the society's institutional imagination. | P1 | Added bounded deterministic BDPL procedures while keeping consent, human authority, Mandate, classification, and independence invariants non-extensible. |
| Adaptation was mostly runtime replacement. | P1 | Added participant-owned private ContinuityRoot plus assent, activation, interest/profile, manifestation compatibility, and continuity revisions. |
| Server preparation could hide cognition or defaults. | P1 | Defined field-complete deterministic preparation and a source-by-field PreparationTrace. |

## Pass 2 — Kovee and Akson consistency

The ownership model was strong, but the first draft described a future adapter
without enumerating the incompatible current Kovee records and placed Byom
admission after Akson had already issued executable local authority.

| Finding | Severity | Disposition in v0.2 |
|---|---:|---|
| Current Kovee is Sage-shaped and cannot represent Byom. | P1 | Defined all-or-nothing `byom_governed_work_v1`, including multi-Society bindings, `owner_protocol: byom`, `ByomEpisodeBinding`, `byom_subordinate` budgets, provider-context source fields, formation recovery, external consumption, credentials, projections, and workspace binding. |
| Akson inbound work was sequenced after work-order authority existed; a foreign peer was called a remote pledgor. | P1 | v0.2 supports only an independently consented Akson-owned confined worker followed by local admission. A foreign contract is never a Byom Pledge. Future Byom/Kovee inbound execution needs a new dual-authority worker profile and is excluded. |
| One atomic budget reservation was claimed across two databases. | P1 | Added typed Byom accounts, reservation sets, child conservation, trusted settlement, and an idempotent fail-closed Kovee subordinate-reservation saga before queueing. |
| Generic governance decisions and exact formation snapshots were missing. | P1 | Added immutable GovernanceDecision, eligibility/slot/independence/dependency snapshots, and decision refs on formed records. |
| Episode retries and fencing were not representable. | P1 | Added immutable EpisodeAttempt, one CAS EpisodeLeaseHead, attempt events, exact fence fields, and distinct Kovee fences. |
| BPP had no operation authority matrix. | P1 | Added a deny-by-absence `(operation, surface)` registry and closed transition specification. |
| Byom duplicated Kovee artifact/effect ownership. | P1 | Kovee owns artifact bytes/versions/grants/scans and host Effects/receipts. Byom stores immutable refs and `EffectOutcomeAdmission` only. Standalone artifact providers use a separate owner namespace. |
| Sage migration could manufacture native authority or allow dual governance. | P1 | All authority-shaped Sage state imports as inert LegacyEvidence/proposal seeds. Added fenced `governance_owner: sage|none|byom` cutover and native reauthorization. |
| Cross-sovereign classification mapping was invoked but undefined. | P1 | Added exact Society-owned ClassificationMappingRevision, distinct from Kovee mappings, with epochs, direction, transformations, expiry, human decision, dependencies, and revocation. |

## Pass 3 — safety, security, and privacy

The first draft had good one-shot effects, actor-from-channel, honest confinement
profiles, context/disclosure manifests, and crash ambiguity. The review focused
on places where prose could be implemented inconsistently or where rollback and
opaque private state defeated the claim.

| Finding | Severity | Disposition in v0.2 |
|---|---:|---|
| Selector subset and transition semantics were not mechanically complete. | P0 | Added closed BPA-1 types and subset/intersection rules, deny precedence, canonical quantities/currencies, exhaustive transition rows, machine-readable descriptors, and model-check requirements. |
| Authorization dependency closure could omit a revocable input. | P0 | Defined a server-computed closure across endpoint, Society, actor, Assembly, decision, Pledge/activity, Mandate, budget, data, host, Kovee, and Akson dependencies; callers cannot supply it. |
| Database rollback could resurrect spent authority or expose an unwitnessed authority tail. | P0 | Added a synchronous external authority-journal CAS before any mutation becomes visible, sealed-diagnostic startup on SQL/witness mismatch, new keys/epochs on restore, direct incarnation binding, and witnessed erasure journal. |
| Private/unobservable inputs made complete taint inheritance impossible. | P0 | Distinguished declared from enforced-complete provenance; opaque output receives the Society top label or quarantine unless approved attestation proves the complete readable-source ceiling. |
| Independence domains were self-asserted. | P0 | Added evidence-backed governance-issued ControlDomainRevision, conservative unknown correlation, distinct-principal human counting, merge invalidation, and an explicit no-collusion-proof limitation. |
| Actor grants lacked sender constraint and runtime assurance could be self-asserted. | P1 | Added mTLS/DPoP/channel binding, epochs/nonces/subject scope, live revocation, fresh phishing-resistant challenges, and approved nonce-bound workload attestation. |
| Budget settlement lacked typed units and trusted meters. | P1 | Added fixed-scale units/currencies/pricing revisions, trusted broker/provider meters, conservative unknown settlement, finite recurring liabilities, and per-dimension feature negotiation. |
| A participant could bypass Byom through parallel Kovee authority. | P1 | Every causally governed Kovee action carries `governance_owner=byom` and cannot fall back. Managed Participants have no parallel ambient credential for governed resources; outside authority is explicitly outside the claim. |
| Prompt injection could deceive the human approval view. | P1 | Added trusted chrome, quoted/source-labelled untrusted text, bidi/control/homoglyph handling, inert links, canonical high-impact fields, shown-representation digest, fresh challenge, and typed/threshold confirmation. |
| Audit, cursor privacy, erasure, and resource limits were incomplete. | P1 | Added signed/witnessed audit checkpoints, opaque audience cursors, pseudonymous telemetry, field-level retention rules, keyed digests/per-object keys, restore-safe erasure, graph/evaluator/upload quotas, isolated scanners, and Society-scoped storage. |
| Akson security compatibility lacked exact feature gates. | P1 | Added `akson_byom_exchange_v1` field bindings and fail-closed capability matrix for rollback, key expiry, confinement, budget dimensions, processor visibility, evidence, and audit anchoring. |

## Pass 4 — protocol closure

The blocker-only rereads traced every previously repaired guarantee through its
record schema, callable operation, authority-registry row, closed transition,
recovery behavior, and conformance vector. They found several places where the
intent was correct but the protocol surface was not yet self-contained.

| Finding | Severity | Disposition in v0.2 |
|---|---:|---|
| Candidate refusal and Continuation writes were catalogued but not closed state machines; accepted membership could become stale irrevocable assent before admission. | P1 | Added typed MembershipOffer/Onboarding states, immutable MembershipRefusal, accepted-to-refused/expired CAS against admission, terminal candidate/onboarding fencing, and exact retry behavior. Added one revisioned ContinuationHead with predecessor-bound CAS, one concurrent winner, and no implicit merge. |
| Lottery procedures did not pin enough seed inputs to prevent source selection, last-revealer, abort, or retry bias. | P1 | Added a pre-adopted ProcedureSeedPolicyRevision, one unique subject slot, exact VRF input/key, beacon-round/finality rule, committer snapshot, deadlines, threshold, missing-reveal sentinel, post-reveal beacon, terminal unavailability, and named non-callable seed transitions. |
| Kovee formation named one atomic external command that BPP did not expose, and an attempted genesis mode lacked a Society actor/idempotency namespace. | P1 | Added exact existing-Society-only `kovee_endeavor_form` schemas, registry entry, closed all-or-none machine, and sole-source-human seat constraint; native bootstrap remains a direct Byom ceremony. Stable semantic command bytes are separate from fresh authentication attempts, and all recovery rows pin the Society epoch, source actor, canonical IdempotencyDomain, and Realm/Byom binding lineage. Multi-participant formation uses ordinary participant operations. |
| Formation recovery could neither distinguish absence from cancellation nor recover a linkable result safely across binding rotation or repeated restore. | P1 | Query results distinguish live `absent`, signed `historically_fenced_absent`, committed envelope, tombstone, and unknown. A bounded ordered RestoreLineageProof validates every witnessed hop and never revives an old domain. Source-human terminalization has an exact committed/terminalized/not-terminalizable union, races delayed execution on the idempotency/journal lock, and has explicit no-op behavior. Paired intent/slot/attempt tables close every result, ambiguity, link, and release transition. |
| Akson flow made a remote Byom Society optional while its bundle required remote-Society mappings; later authority could be predicted/spliced, and the host receipt represented only verified results. | P1 | Split closed Society-mapped and Akson-neutral profiles into signed request, acceptance, result, and local-admission phases. Exact equality includes phase digests, peer epochs, ActIntent, MandateUse, consumption receipt, keys, result, Kovee receipt, EOA head, Society, and mapping. The Kovee receipt has one revision head and closed pre-result-failed/ambiguous/verification-rejected/verified-result branches; only verified result creates classification admission. |
| Ambiguous effect resolution conflated a later host fact with a local governance judgment, implied one CAS across owners, and initially left both axes sharing one terminal Byom head. | P1 | Split runtime `effect_outcome_admit` source reconciliation from governance `effect_reconcile`, then separated their records and heads. Kovee commits its immutable receipt successor first; Byom always admits that source fact independently. A local disposition cannot close or block the EOA/ActIntent source axis; a late verified result is classified but quarantined until a fresh decision resolves its use. Neither path mutates Kovee or claims a shared CAS. |
| Rate, filesystem, private-read, and erasable-digest controls still admitted implementation choices that could weaken the guarantee. | P1 | Added the canonical hierarchical token-bucket algorithm, handle-relative filesystem access, independent post-close seed mechanics, a synchronous privacy-access journal before sensitive result release, and typed digest classes that reject public plaintext hashes for erasable low-entropy data. |

## Cross-cutting decisions retained

- Byom remains model-free and effect-free. Intelligence stays with Participants.
- The system is explicitly human-sovereign while preserving operational and
  associational autonomy for agents and collectives.
- Kovee, Byom, and Akson each have one semantic write owner; projections never
  authorize.
- Arrival, admission, attention, activation, allocation, placement, execution,
  delivery, verification, and review remain different facts.
- Security and privacy claims are attached to negotiated feature profiles. A
  missing enforcement dimension disables the workflow.
- One home writer is retained for v0.2, with signed portability checkpoints,
  lossless re-home, and non-authority-carrying evidentiary fork semantics.

## Verification status

Final blocker-only rereads checked `DESIGN.md` at exact SHA-256
`ccea384ff931bcf45d30df680b86835ac682006072a07ef2f34f565eba5fa501`.

- **Agentic nativity:** no remaining P0/P1. The source-fact/local-disposition
  split does not centralize intelligence or manufacture assent; participant
  initiative, activation, continuity, refusal, exit, dissent, plural plans, and
  collective autonomy remain intact.
- **Kovee/Akson integration:** no remaining P0/P1. Semantic ownership,
  Kovee-first source commits, independent Byom admission, Akson phase/epoch
  anti-splice bindings, the receipt union/head, and late-source
  classification/quarantine now close under both race orders.
- **Safety, security, and privacy:** no remaining P0/P1. Source truth and local
  judgment use separate heads; late verified bytes are classified but
  quarantined until a fresh exact decision; no-result branches stay
  unavailable; ambiguous budget remains conservative; and result consumers
  bind both heads plus current authorization and classification dependencies.

Local structural checks found 154 catalog operations and 154 deny-by-absence
registry operations with no missing or extra entry, balanced Markdown fences,
no trailing whitespace, resolvable local links, and canonical Akson spelling.

This is design-level closure, not implementation or security certification.
Generated schemas, machine descriptors, model checks, protocol vectors, and
the Kovee/Akson conformance suites remain delivery gates before any production
or compatibility claim.

## Change scope

This design session created and modified only files inside `byom/`. Kovee,
Akson, Sage, and other sibling project files were read for comparison and were
not edited.
