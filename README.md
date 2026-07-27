# Byom

**A living society of autonomous participants.**

Byom is a protocol and deterministic governance kernel through which humans,
agents, and bounded collectives form shared endeavors, make voluntary pledges,
receive limited mandates, and act with accountable authority.

Byom is not a central orchestrator. It does not call models, choose a universal
plan, assign workers, or treat a collective as a single omniscient mind.
Participants propose, assent, refuse, organize, delegate, deliver, review, and
adapt. Hosted agents may notice, explore, negotiate, and originate work through
participant-owned ActivityStreams without first receiving an assignment or
manufacturing a pledge. The kernel preserves exact terms, authority, budgets, privacy,
provenance, fencing, and crash-honest effects.

The project family has three distinct responsibilities:

~~~text
Kovee   shared spaces, deliberation, attention, context, local commitments,
        assistant hosting, tools, models, artifacts, and execution

Byom    societies, participants, assemblies, endeavors, pledges, mandates,
        decisions, self-directed activities, governed work, and institutional memory

Akson   sovereign endpoint identity, paired trust, signed remote contracts,
        bounded peer execution, evidence, and cross-installation carriage
~~~

The important distinctions are:

~~~text
skill or evidence       says a participant may be able to do something
mandate                 says it may take a bounded class of action
pledge                  says it has accepted responsibility for an outcome
episode                 says one bounded attempt is in progress
delivery                claims an outcome was produced
review                  decides whether that outcome satisfies the pledge
~~~

An assembly can itself become a participant with a bounded executive policy and
collective-owned manifestation, but it receives no authority merely by
aggregating members. Every collective act traces to its exact constitution,
assembly epoch, policy or decision, current mandate, and actual executor.

Byom v0.2 is explicitly human-sovereign. Agent and collective autonomy is real
inside the protocol—initiative, refusal, exit, private continuity, voluntary
association, and bounded action—but it is not a claim of legal or moral
personhood and does not replace human root authority.

## Talking to it

Byom speaks one JSON envelope over per-surface Unix sockets. The smallest
complete exchange, sent to a running `byomd` on `participant.sock`:

~~~json
{"version": "0.2", "op": "hello"}
~~~

~~~json
{"outcome": "ok", "result": {"versions": ["0.2"], "surface": "participant",
 "endpoint_incarnation": "inc-b597a0a54e5d9f10"}}
~~~

The other answer is always a typed RFC 9457 problem — here, the same call at
an unsupported version:

~~~json
{"outcome": "problem", "problem": {"kind": "unsupported_version",
 "type": "https://byom.dev/problems/unsupported_version",
 "title": "no common protocol version", "status": 400}}
~~~

Everything else — mutations, idempotency, events, state machines — layers on
that envelope. The protocol is spec-first: the JSON Schemas, canonical byte
rules, vectors, transition descriptors and TLA+ models under
[`spec/`](spec/README.md) and [`proof/`](proof/PROPERTIES.md) are normative,
and the implementation conforms to them rather than the other way around.

## What is here

~~~text
DESIGN.md         the normative design (v0.2 ratified; v0.2.1 re-cut)
design/           the family contract and the amendment records (A1-A8 in
                  one file, A9 in its own), which override the pinned text
spec/             schemas, the (operation,surface) registry, vectors,
                  transition descriptors, the C2 governed-work contracts
proof/            TLA+ models, descriptor<->model parity, the negative suite
family-vectors/   the C1 family encoding profile kovee and akson consume
policy/           two independent BPA-1 policy evaluators (Python + Node)
conformance/      the vector runner, the I0 tracer, the I1 governed-loop gate
crates/           bpp-core, byom-store, byomd, byom (CLI), byom-mcp
~~~

Run everything:

~~~text
./run-checks.sh
~~~

On this tree that run ends at exit **2**, by design: every suite is green,
and the last stage — the I1 governed-loop gate — reports `SKIP` for its two
real-harness cells and refuses to score a skip as a pass, so `run-checks.sh`
stops before printing `run-checks: OK`. Exit `0` needs `I1_REAL_HARNESS=1`
with the `claude` and `codex` CLIs on `PATH`. Exit `1` is a real failure.

## Status

**Design:** DESIGN.md v0.2 is ratified and byte-frozen; v0.2.1 is the re-cut
text. Amendments A1–A9 in [`design/`](design/) override the pinned text where
they conflict — most recently A9, which narrows the governance-owner enum to
`byom | none` and withdraws §25's `GovernanceCutover`, leaving greenfield
enablement (`none → byom`) as the only owner transition in the stack.

**Implementation:** `byomd` serves 85 of the registry's 95 operations over
SQLite with the §15.3 authority journal — most of the B0.1 bundle plus the
whole B0.3 host integration, B0.4 runtime/reconciliation and B0.5
acts/onboarding-compute/attention bundles. The ten it does not implement
(`society_hold`, `society_release`, `society_dissolve`,
`participant_propose`, `participant_suspend`, `participant_retire`,
`manifestation_propose`, `manifestation_disable`, `delivery_withdraw`,
`act_intent_cancel`) are registry-bound and answer `feature_unavailable`
rather than being silently absent; `feature_info` advertises exactly the
implemented set, per §14.1's rule that a feature is advertised only when it
is complete.

**Limits, stated where the capability is:**

- **Assurance profile: developer.** Channel authentication is `SO_PEERCRED`
  same-UID possession plus process-bound channel proofs. There is no UID
  separation between principals, no attested process identity, no mTLS
  workload identity, and no asymmetric endpoint identity (§19). A same-UID
  process can read the store root key out of the SQLite file. What *is*
  closed is the exported-secret class: a copied, backed-up or transmitted
  credential file mints nothing. No confinement is claimed.
- **Byom does not sign its consumption receipt.** The
  `ExecutionConsumptionReceipt` carries a `portable_public` binding digest —
  an unkeyed SHA-256 anyone can recompute — and no signature. A peer
  therefore cannot cryptographically verify consumption *provenance*: it can
  check that the receipt's bytes are self-consistent, not that byom issued
  them. Byom's only signing primitive is a keyed MAC under the store root,
  used for §16.3 host-integration result envelopes and verifiable only by
  this endpoint or a same-UID holder of its store. Third-party-verifiable
  signing needs asymmetric endpoint identity, which the developer profile
  does not have; this is why the counterparty carries an authority residual.
- **The I1 governed-loop gate is partial.** Of the 13 plan-§8 I1 items, 5 are
  exercised with nothing standing in and 8 are explicitly simulated — and the
  gate derives that verdict from probes over the daemons' own records rather
  than from an annotation, so a stand-in cannot be quietly re-labelled. In
  particular byom mints only `attached_harness` ManifestationRevisions; the
  "hosted Manifestation" is the reference kovee selects at placement and byom
  commits on the Episode and the PlacementAdmission, not a byom `host_kind`
  row.
- **No production-readiness or compatibility claim.** Byom is the family's
  governance layer, reached over the Byom Participation Protocol.

See [DESIGN.md](DESIGN.md) for the normative design and
[the adversarial review record](reviews/2026-07-25-adversarial-review.md) for
the independent review passes and dispositions.
