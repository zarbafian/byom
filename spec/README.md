# BPP specification

The Byom Participation Protocol (BPP, DESIGN.md §14) is transport-independent
and spec-first: the JSON Schemas, canonical byte rules, problems, limits,
vectors, transition descriptors, and models in this tree are normative;
implementations conform to them, never the other way around.

A request and its two possible answers, end to end — the bytes a running
`byomd` returns on `participant.sock` today:

~~~json
{"version": "0.2", "op": "hello"}
~~~

~~~json
{"outcome": "ok", "result": {"versions": ["0.2"], "surface": "participant",
 "endpoint_incarnation": "inc-b597a0a54e5d9f10"}}
~~~

~~~json
{"outcome": "problem", "problem": {"kind": "unsupported_version",
 "type": "https://byom.dev/problems/unsupported_version",
 "title": "no common protocol version", "status": 400}}
~~~

(The `hello-result` schema admits a multi-version list; this daemon
advertises exactly one negotiated minor. Problem `title` is free RFC 9457
prose — `kind` and `type` are the normative pair.)

Everything else — mutations, idempotency, events, state machines — is layered
on that envelope (`schemas/`), pinned by golden vectors (`vectors/`), and
checked by `../conformance/run.py`.

## Layout

Counts below are what `python3 conformance/run.py` prints on this tree today;
it is the source, this table is the transcription.

| Directory | Contents | Status |
|---|---|---|
| `adr/` | architecture decision records (format below) | B-ADR-1 accepted (BPA-1 encoding) and B-ADR-4 accepted (model checking); B-ADR-2 (BDPL serialization) still proposed |
| `schemas/` | JSON Schema 2020-12, one file per schema version | 234 schemas compile; envelope + negotiation, and `ops/` covers every B0.1 sheet family — society, charter, participants, candidates, endeavors, calls/pledges, mandates, acts, activities, events + recovery core (76 ops, 62 mutating + 14 reads, request/result pairs) plus the later B0.3/B0.4/B0.5 rows in the same directory; `bpa1-policy.schema.json` — the closed BPA-1 policy AST (ADR-0001 accepted, §10.5), dual-evaluated by `../policy/eval.py` + `../policy/eval.mjs` |
| `vectors/` | golden and negative vectors, one JSON file per case | 462 cases across six families — `envelope/`, `ops/`, `policy/` (BPA-1: schema, canonical/digest, is_subset, intersect, deny-wins decide, malformed/overflow — both policy evaluators must agree on every case), `governed-work/` (C2 record schemas plus 12 executable saga walks), `machines/` (16 §14.8 crash/replay state walks), `mcp/` (C3a tool-call bindings) |
| `registry.json` | machine-readable operation registry: one row per `(operation, surface)` with binding (R-number), family, class (read/create/update), and the frozen request/result schema names (the RT-06 `-v2` successors included) — the freeze source for every bundle | 99 rows over 95 distinct operations: the B0.1 bundle (80 rows; the four G35 dual-surface ops with exactly two rows each), the three B0.3 host-integration rows R39 `kovee_endeavor_form`, R40 `external_command_terminalize` (governance, create) and R42 `external_command_result_query` (projection, read), the 11 B0.4 runtime/reconciliation rows and the 5 B0.5 acts/onboarding-compute/attention rows. Every non-B0.1 row carries an explicit `"bundle"` and the B0.3 rows name the FROZEN `governed-work/` record as their `result_schema`, so the C2 seam cannot fork its own wire; `../conformance/run.py` derives its bundle, meta-class, and MCP checks from these rows and fails on any extra/missing surface binding |
| `descriptors/` | machine-readable transition descriptors, one-to-one with every mutating operation in the bundle plus the named internal kernel transitions (§14.8) | 26 machines (167 states, 312 transitions) plus the frozen v2 column vocabulary in `vocabulary.json`. The runner enforces the one-to-one rule over the **B0.1 sheet's 62 mutating operations** — all 62 owned by exactly one descriptor — so the mutating rows of the later B0.3/B0.4/B0.5 bundles are not yet inside that gate. Five machines are Kovee-owned executors (`owner: "kovee (C2)"` — outside the BPP one-to-one rule): greenfield-enablement, endeavor-formation, byom-episode-binding, subordinate-reservation, byom-akson-dispatch-outcome-head |
| `governed-work/` | C2 `byom_governed_work_v1`: slice 1 — the byom-normative binding/enablement/formation record schemas (§16.3/§16.6 field lists verbatim) plus the normative D10 greenfield enablement saga (`greenfield-saga.md`); slice 2 — the episode/effect/driver contracts (`episode-budget-dispatch.md`): ByomEpisodeBinding, the byom_subordinate budget bridge, ProviderContextManifest byom fields (Δ5), onboarding-compute one-shot, the byom_akson_dispatch_v1 driver + outcome receipt/head, worker/candidate sender-constrained credential profiles, the Δ4 act-class subject taxonomy (BPA-1 cross-validated) | 28 closed record schemas (27 + the amendment-A9 `kovee-governance-owner-binding-v2` successor, its v1 still published) — bundle contracts complete; vectors under `vectors/governed-work/`; `../proof/specs/GreenfieldEnablement.tla` + `../proof/specs/SubordinateReservation.tla` |
| `../proof/specs/` | the TLA+ models this bundle is checked against, with crash/replay walks under `vectors/machines/` (ADR-0003) | 9 modules, TLC-exhaustive at their configured constants; per-model projection, refinement boundary, fairness and coverage in `../proof/PROPERTIES.md`. There is no `spec/models/` directory — the models live with the proof tooling |
| `../conformance/` | runner: schemas compile, every vector validates, digests re-derive | `run.py`; `run.py --live` replays the slice-op request vectors against a spawned `byomd` |

## Bundle-freeze rule

A specification bundle (B0.1, B0.2, …) is the unit of compatibility:

- The exact operation list of a bundle is **frozen from the machine-readable
  registry at bundle freeze**. Counts and membership are registry-derived,
  never prose; CI compares descriptors, schemas, and vectors against the
  registry rows, and a mismatch fails the bundle.
- A feature is advertised over the wire only when all of its operations,
  states, limits, authorization checks, crash semantics, and conformance
  fixtures are implemented (§14.1) — a bundle freeze is the spec-side
  precondition for that advertisement.
- Published schemas are immutable; any change is a new schema version file.
- A vector file is immutable once merged; fixes are new cases.
- Transition descriptors must map one-to-one to 100% of mutating catalog
  operations in the frozen bundle plus the named internal kernel transitions
  (`activation_admit`, `resource_allocate`, the journal mutation protocol).
  Two independent gates enforce this, both in CI and in `../run-checks.sh`:
  `python3 conformance/run.py` proves each of the B0.1 sheet's 62 mutating
  operations is owned by exactly one descriptor, and
  `python3 proof/check-descriptors.py` proves the descriptors and the TLA+
  models agree set-for-set in both directions. There is no `bpp-spec` crate —
  parity is a Python gate, not a `cargo test`. **Honest limit:** the
  one-to-one gate currently ranges over the B0.1 sheet only; the mutating
  rows added by B0.3/B0.4/B0.5 are registry- and schema-checked but are not
  yet inside the descriptor-ownership rule.

## Schema conventions

Mirrors `akson/spec/ext/README.md`:

- JSON Schema draft 2020-12; closed schemas: `additionalProperties: false` on
  every object schema that declares properties (unknown fields fail closed).
  Two recorded exceptions (R0/BYOM-01/02): pure refinement branches (`oneOf`
  arms without `type`, e.g. the DigestRef class/algorithm pairing) constrain
  members of an already-closed parent; and the RFC 9457 problem object admits
  extension members solely under reverse-domain names via a `propertyNames`
  pattern (family profile §3). The conformance runner enforces exactly this
  rule.
- Self-contained: internal `$ref` into the same file's `$defs` only, never a
  remote reference. Shared shapes (identifier, DigestRef, MutationMeta) are
  restated per file.
- Instances must pass strict I-JSON acceptance *before* schema validation:
  the C1 family acceptance rules of `../family-vectors/PROFILE.md` §1
  (normative — R0/BYOM-03): token-order first-error reporting, 256 KiB
  request / 1 MiB response caps, inclusive depth-64 and 65 536-node caps,
  the `$domain` reservation at every depth, unpaired-surrogate rejection,
  unsafe integers and integer-valued floats beyond ±(2^53 − 1) rejected, no
  NaN/Infinity (§14.2, §14.9).
- Validation rules the schemas cannot express (per-operation registry
  membership, mutation/meta pairing by registry row, envelope byte limits)
  are enforced in code and covered by raw vectors.

**Namespace gate (unmet).** Akson's convention requires `$id` under a
project-controlled HTTPS namespace before a stable release. Byom does not yet
control one; every `$id` uses the provisional reserved name
`https://byom.example/bpp/…` and MUST be rewritten in one place when the
namespace gate is met. No stable release may ship on `byom.example`.

**Problem-type namespace (open fact, A0.4-style).** The ratified family
profile pins problem `type` as exactly `https://byom.dev/problems/<kind>`
(`../family-vectors/PROFILE.md` §3, profile-pinned decision 3), and the
schemas, vectors, and runner enforce that prefix (R0/BYOM-02). Control of the
`byom.dev` problems namespace is **not yet established**: this is recorded
honestly as an open fact, tracked to closure (domain control demonstrated, or
the profile amended) before any public advertisement of the protocol. It is
distinct from the `$id` gate above — `$id`s stay on the reserved
`byom.example` name, while problem types already carry the profile-pinned
`byom.dev` prefix on the wire.

## Canonical bytes and digest domains

The ratified family encoding profile — `../family-vectors/PROFILE.md` (C1,
amended by the R0 dispositions of 2026-07-25) — is **normative** for
canonical bytes, the DigestRef wire, digest classes, and the
idempotency-domain construction. This section is a conforming summary, not a
second source (R0/BYOM-01).

Canonical bytes are RFC 8785 JCS over the profile's strict-I-JSON value
space (PROFILE.md §1/§2): the full finite-number space in ES minimal form.
BPP canonical values happen to contain no floats today (§14.2, ADR-0001),
but the canonicalizer implements the profile space. Every digest field is a
typed `DigestRef` with the closed wire shape
`{class, algorithm, key_ref?, value_hex}` (PROFILE.md §6.1), never an
unlabelled hash: closed class/algorithm pairing (`sha-256` for the public
classes, `hmac-sha-256` for the keyed erasure classes), `key_ref` required
exactly for the keyed erasure classes and forbidden otherwise, `value_hex`
exactly 64 lowercase hex characters. The six classes include
`scope_erasure_safe` (D-R0-1) for shared-key index and chain constructions.

Type-tagged canonical bytes inject the reserved top-level `$domain` member,
then apply JCS (PROFILE.md §2; `$domain` is reserved at every depth of wire
bodies and fails closed on collision):

~~~text
tagged(domain, value) = JCS(value ∪ {"$domain": domain})
~~~

This is the ratified concrete reading of DESIGN.md §14.2's
`DigestRef(JCS(type_tag("…") || value))`. The earlier B0.1 proposal —
`SHA-256(JCS([domain, value]))` with the tag as the first array element — is
**superseded** (R0/BYOM-01, profile-pinned decision 4).

**Digest domains.** There are around ninety `$domain` tags in the daemon —
one per record commitment — and they are owned by the code that mints them,
not by this file. Two are pinned by vectors here, and a further six matter at
the specification boundary because a *counterparty* must derive them: those
are the **frozen cross-boundary fragments** of the A8 rule
(`../family-vectors/PROFILE.md` §6.2), each an unkeyed `portable_public`
SHA-256 over a closed member list, and each with its members enumerated in
`schemas/ops/README.md` gap note G48.

| Domain string | Value and construction | Where pinned |
|---|---|---|
| `bpp-idempotency-domain-v1` | `IdempotencyDomain` (§14.2): `HMAC-SHA-256(per-Society index key, tagged(domain, value))`, emitted as a `scope_erasure_safe` DigestRef — the index key is a scope key, so destroying it erases offline verifiability of the whole index, never one entry (PROFILE.md §5, D-R0-1) | `vectors/envelope/digest-*` |
| `bpa1-policy-v1` | BPA-1 policy AST (ADR-0001); both evaluators derive it independently | `vectors/policy/*`, `../policy/eval.py`, `../policy/eval.mjs` |
| `bpp-resource-allocation-binding-v0` | the ResourceAllocation's cross-boundary identity; published on `episode_request`'s result and compared byte-for-byte at `placement_admit` | G48, `vectors/ops/episode-request-result-*` |
| `bpp-parent-budget-fragment-v0` | the parent budget worst case Kovee's subordinate reservation must stay under | G48; cross-repo vector `crates/byomd/tests/vectors/parent-budget-fragment.json` |
| `bpp-provider-context-source-v0` | the §12.1 provider-context source fields byom composes for Kovee's ProviderContextManifest | gap note G47 |
| `bpp-mandate-use-binding-v0` | the MandateUse binding published on the consumption receipt | G48 |
| `bpp-execution-consumption-receipt-binding-v0` | the receipt's own binding digest — the one value the broker relies on, derived by the consumer from the receipt it just received | G48, `vectors/ops/execution-permit-consume-result-*` |
| `kovee-host-effect-binding-v1` | **Kovee's** tag, not byom's: the nine-member host Effect fragment byom rebuilds from its own committed act and refuses if it does not re-derive | G48; cross-repo vector `crates/byomd/tests/vectors/kovee-host-effect-binding.json` |

## ADR format

`adr/NNNN-short-title.md`, next free number, sections **Context / Decision
(proposed) / Criteria / Consequences**; an ADR is `proposed` until merged
with maintainer approval, then `accepted`; superseding links both ways. The
index in `adr/README.md` maps file numbers to the B0 plan's B-ADR ids.
