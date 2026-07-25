# BPP specification

The Byom Participation Protocol (BPP, DESIGN.md §14) is transport-independent
and spec-first: the JSON Schemas, canonical byte rules, problems, limits,
vectors, transition descriptors, and models in this tree are normative;
implementations conform to them, never the other way around.

A request and its two possible answers, end to end:

~~~json
{"version": "0.2", "op": "hello"}
~~~

~~~json
{"outcome": "ok", "result": {"versions": ["0.1", "0.2"],
 "surface": "participant", "endpoint_incarnation": "inc-0001"}}
~~~

~~~json
{"outcome": "problem", "problem": {"kind": "unsupported_version",
 "type": "https://byom.dev/problems/unsupported_version",
 "title": "no common protocol minor version", "status": 400}}
~~~

Everything else — mutations, idempotency, events, state machines — is layered
on that envelope (`schemas/`), pinned by golden vectors (`vectors/`), and
checked by `../conformance/run.py`.

## Layout

| Directory | Contents | Status (B0.1 slices 1–3) |
|---|---|---|
| `adr/` | architecture decision records (format below) | B-ADR-1 accepted (BPA-1 encoding); B-ADR-2/4 proposed |
| `schemas/` | JSON Schema 2020-12, one file per schema version | envelope + negotiation; `ops/` covers every B0.1 sheet family — society, charter, participants, candidates, endeavors, calls/pledges, mandates, acts, activities, events + recovery core (73 ops, request/result pairs); `bpa1-policy.schema.json` — the closed BPA-1 policy AST (ADR-0001 accepted, §10.5), dual-evaluated by `../policy/eval.py` + `../policy/eval.mjs` |
| `vectors/` | golden and negative vectors, one JSON file per case | `envelope/`, `ops/`, `policy/` (BPA-1: schema, canonical/digest, is_subset, intersect, deny-wins decide, malformed/overflow — both policy evaluators must agree on every case) |
| `registry/` | machine-readable operation registry: one row per `(operation, surface)` with family, mutating flag, closure categories — the freeze source for every bundle | planned (later B0.1 slice); interim §14.6 catalog + B0.1 sheet transcriptions live in `../conformance/run.py` |
| `descriptors/` | machine-readable transition descriptors, one-to-one with every mutating operation in the bundle plus the named internal kernel transitions (§14.8) | society + participant/candidate + work-lifecycle + mandate/act-intent/charter machines (20 files); parity checked by the runner. C2 adds five Kovee-owned executor machines (`owner: "kovee (C2)"` — outside the BPP one-to-one rule): greenfield-enablement, endeavor-formation, byom-episode-binding, subordinate-reservation, byom-akson-dispatch-outcome-head |
| `governed-work/` | C2 `byom_governed_work_v1`: slice 1 — the byom-normative binding/enablement/formation record schemas (§16.3/§16.6 field lists verbatim) plus the normative D10 greenfield enablement saga (`greenfield-saga.md`); slice 2 — the episode/effect/driver contracts (`episode-budget-dispatch.md`): ByomEpisodeBinding, the byom_subordinate budget bridge, ProviderContextManifest byom fields (Δ5), onboarding-compute one-shot, the byom_akson_dispatch_v1 driver + outcome receipt/head, worker/candidate sender-constrained credential profiles, the Δ4 act-class subject taxonomy (BPA-1 cross-validated) | 27 closed record schemas — bundle contracts complete; vectors under `vectors/governed-work/`; `proof/specs/GreenfieldEnablement.tla` + `proof/specs/SubordinateReservation.tla` |
| `models/` | TLA+ models with crash/replay vectors and per-model proof READMEs (ADR-0003) | planned (later B0.1 slice) |
| `../conformance/` | runner: schemas compile, every vector validates, digests re-derive | `run.py` |

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
  (`activation_admit`, `resource_allocate`, the journal mutation protocol);
  descriptor parity is a CI gate (`cargo test -p bpp-spec --test
  descriptor_parity`).

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

Current digest domains:

| Domain string | Value and construction | Vectors |
|---|---|---|
| `bpp-idempotency-domain-v1` | `IdempotencyDomain` (§14.2): `HMAC-SHA-256(per-Society index key, tagged(domain, value))`, emitted as a `scope_erasure_safe` DigestRef — the index key is a scope key, so destroying it erases offline verifiability of the whole index, never one entry (PROFILE.md §5, D-R0-1) | `vectors/envelope/digest-*` |
| `bpa1-policy-v1` | BPA-1 policy AST (ADR-0001) | with the BPA-1 slice |

## ADR format

`adr/NNNN-short-title.md`, next free number, sections **Context / Decision
(proposed) / Criteria / Consequences**; an ADR is `proposed` until merged
with maintainer approval, then `accepted`; superseding links both ways. The
index in `adr/README.md` maps file numbers to the B0 plan's B-ADR ids.
