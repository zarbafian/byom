# Amendment: family-contract alignment

Status: proposed (C0 revision work; becomes normative on C0 ratification)

Date: 2026-07-25

Amends: `byom/DESIGN.md` at sha256
`ccea384ff931bcf45d30df680b86835ac682006072a07ef2f34f565eba5fa501`.
Authority: the family contract (`byom/design/2026-07-25-family-contract.md`)
and plan decisions D1, D9, D10 (`2026-07-25-kovee-byom-implementation-plan.md`,
sha256 `d5e73952ac90a67a4e4a060052ca66d4be729dec500436b86623b89c54afb2d2`).

## A1 — Naming normalization (plan D9)

README/DESIGN references to the gateway via the historical `../axon` path read
`../akson`; the Rust gateway is **Akson** (`aksond`, `akson-*`). The external
`akson-ai` Python project remains authoring inspiration only.

## A2 — Kovee integration is greenfield-first (plan D1, D10)

§16's Kovee prerequisites are being implemented against a kovee that has
**byom as its governance owner from day one** (never Sage). Consequences:

- §25's Sage migration (`GovernanceCutover`, `sage → none → byom`) remains
  specified but **unbuilt**; no milestone in the current program exercises the
  `sage` arm of `KoveeGovernanceOwnerBinding`.
- A **greenfield enablement saga** (`none → byom` without a Sage
  predecessor) is added to the `byom_governed_work_v1` bundle (C2): create
  `KoveeRealmByomBinding` + `KoveeSocietyMapping`, then CAS the owner binding
  `none → byom`, with exact-CAS, retry, overlapping-scope rejection,
  rollback-before-activation, and restore behavior specified and
  model-checked. Kovee is never the genesis governance actor (§16.3's
  `society_prepare`/`society_bootstrap` rule is unchanged and controlling).
- §26's open dependency "Kovee must adopt the Byom adapter and BPP
  prerequisites" is tracked as C-track milestones C2/C3 with kovee K2/B3
  consuming them; the Akson least-privilege surface dependency is tracked as
  A0/C4.

## A3 — Review-cadence completion

The program adds **RT** (tracer-contracts review: makes B0.1 and C3a
implementation-ready before B1/I0) and **RC4** (contract review of the frozen
`akson_byom_exchange_v1` surface: makes C4 implementation-ready for K6/B5) —
the plan's §9 diagram and review list carry both; `plan/dag.json` encodes the
grants.

## A4 — Candidate MCP binding (normative amendment to §14.10)

§14.10 defines MCP as a participant/harness binding. This amendment adds a
**candidate profile**: a sender-constrained MCP binding scoped to exactly one
`MembershipOffer`, its proposed Manifestation, its control-domain binding, and
its onboarding fence, exposing exactly `membership_refuse`,
`membership_accept`, and `candidate_self_policy_propose` (registry row for the
candidate surface, unchanged closure). Lifecycle: minted with the offer;
closed server-side on admission, refusal, revocation, or expiry (terminal
offer fencing per §7.4 — while an exact refusal retry still returns its
retained receipt); it never converts in place — the participant channel is a
new credential minted at admission. Elicitation through the binding is never
assent; no human-authority, governance, or admin tool exists on it.

## A5 — Directory evidence operations (tracked obligation, B0.4)

Δ6 routing currently maps to `participant_show` + `engram_search` evidence.
Typed **ProfileClaim/evidence publish, read, and search operations** (with
§14.7 registry rows, closures, and conformance vectors) are a tracked design
obligation frozen with the B0.4 bundle; until they exist, no ranked-routing
claim is advertised beyond what R4/R37 reads support.

## A6 — Manual developer profile for sovereign exchange (normative amendment to §17/B5)

v0.2 specifies the akson-confined remote worker as the B5 execution path.
This amendment adds a second, weaker, honestly-labeled profile for the I2
gate: **manual developer profile** — the remote performer forms its own
finalized local Pledge, executes via its own locally attached harness
(developer profile), discloses through its own outbound
ActIntent/`execution_permit_consume` chain, and returns the result through
akson **manual fulfillment** (signed manifest over exact output bytes;
**no execution evidence claimed** — evidence slots empty unless genuinely
supplied). No inbound object authors the performer-side Standing, Pledge,
WakeIntent, or execution authority. The confined-worker path remains the
default for any stronger claim; the capability matrix records which profile a
peer exchange used.

## A7 — B1/B3 re-slice and B0.2 scope (normative amendment to §24)

- **B1** is delivered in two slices with independent exit criteria:
  *attached slice* (I0): offer→acceptance→`participant_admit`+
  `manifestation_admit`→Standing, mandate chain, exploration, endeavor
  formation, call/pledge full seat sequence, deterministic delivery, review,
  pending `wake_intent_submit`, kill/restart honesty. *Hosted slice* (I1,
  with K2/C3b): Kovee-hosted episodic participant, full activation pipeline,
  dual fences, cross-Manifestation Continuation resume. I0 confirms only the
  attached slice; §24 B1's full exit is met when both slices pass.
- **B3** hosts the complete B1 flow at I1; hosting the B2 flow is accepted at
  B2's own exit, not I1.
- **B0.2** covers the complete B2 operation families: assemblies, procedures,
  seeds, ControlDomain revisions, disputes/appeals, and StandingMandates.

## Follow-through

Folded into DESIGN.md at the next design revision (v0.3); until then this
record rides alongside the pinned v0.2 text. The C0 three-lens review covers
it; the blocker-only confirmation pass verifies A4–A7.

## A8 — The cross-boundary digest-class rule, and the achievable activation order

Both items come from the live kovee↔byomd integration
(`reviews/2026-07-26-seam-findings.md`, S-1..S-3) — defects a frozen contract
could not surface and two independent implementations meeting at it did.

**The cross-boundary class rule** (now normative in `family-vectors/PROFILE.md`
§6.2 and profile-pinned decision 14; **both repos mirror that section**):

- A digest one protocol **demands from the other** MUST be `portable_public`,
  taken over a **frozen cross-boundary fragment** whose members both sides
  hold. Crossing the boundary *is* the durable-identifier disclosure that
  class requires; because the fragment is never the owner's whole erasable
  record, `public_hash_over_erasable_content_forbidden` is untouched.
- Converse, equally normative: a digest the owner **recomputes from its own
  state** keeps `local_erasure_safe` and is **never a request member** — the
  owner computes it rather than asking the peer to echo an opaque blob.

Applied: `resource_allocation_digest` → `portable_public` **and published** in
the `episode_request` result (byom now derives a second, fragment-scoped
binding digest); `context_manifest_digest` and `checkpoint_digest` →
`portable_public` (byom holds no bytes to re-derive them);
`claim_subject_digest` → removed from the request entirely (byom's own
authority subject, recomputed internally). That last one had forced Kovee to
add per-object erasure secrets for nothing — a storage change driven purely by
a class choice, now reverted.

**The achievable activation order** (L25, corrected above): `episode_request`
comes **before** placement, because `placement_admit` needs the
`ResourceAllocation` that `episode_request` creates. The prior transcription
(`PlacementBinding → placement_admit → episode_request`) is not executable.

Two follow-ons recorded, not silently changed: `kovee_context_assembly_digest`
and `provider_context_manifest_digest` name Kovee objects byom cannot
recompute, so the rule would move them to `portable_public` as well — outside
the ratified four, so left for an owner decision.
