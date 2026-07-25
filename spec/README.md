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
 "status": 400, "title": "no common protocol minor version"}}
~~~

Everything else — mutations, idempotency, events, state machines — is layered
on that envelope (`schemas/`), pinned by golden vectors (`vectors/`), and
checked by `../conformance/run.py`.

## Layout

| Directory | Contents | Status (B0.1 slices 1–3) |
|---|---|---|
| `adr/` | architecture decision records (format below) | B-ADR-1/2/4 proposed |
| `schemas/` | JSON Schema 2020-12, one file per schema version | envelope + negotiation; `ops/` covers every B0.1 sheet family — society, charter, participants, candidates, endeavors, calls/pledges, mandates, acts, activities, events + recovery core (73 ops, request/result pairs) |
| `vectors/` | golden and negative vectors, one JSON file per case | `envelope/`, `ops/` |
| `registry/` | machine-readable operation registry: one row per `(operation, surface)` with family, mutating flag, closure categories — the freeze source for every bundle | planned (later B0.1 slice); interim §14.6 catalog + B0.1 sheet transcriptions live in `../conformance/run.py` |
| `descriptors/` | machine-readable transition descriptors, one-to-one with every mutating operation in the bundle plus the named internal kernel transitions (§14.8) | society + participant/candidate + work-lifecycle + mandate/act-intent/charter machines (20 files); parity checked by the runner |
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

- JSON Schema draft 2020-12; `additionalProperties: false` on every object
  that declares properties (closed schemas; unknown fields fail closed).
- Self-contained: internal `$ref` into the same file's `$defs` only, never a
  remote reference. Shared shapes (identifier, DigestRef, MutationMeta) are
  restated per file.
- Instances must pass strict I-JSON acceptance *before* schema validation:
  UTF-8, duplicate keys rejected, unsafe integers (>2^53−1 magnitude)
  rejected, no NaN/Infinity, bounded depth and size (§14.2, §14.9).
- Validation rules the schemas cannot express (per-operation registry
  membership, mutation/meta pairing by registry row, envelope byte limits)
  are enforced in code and covered by raw vectors.

**Namespace gate (unmet).** Akson's convention requires `$id` under a
project-controlled HTTPS namespace before a stable release. Byom does not yet
control one; every `$id` uses the provisional reserved name
`https://byom.example/bpp/…` and MUST be rewritten in one place when the
namespace gate is met. No stable release may ship on `byom.example`.

## Canonical bytes and digest domains

Canonical bytes are RFC 8785 JCS over strict I-JSON values (no floats in any
BPP canonical value). Every digest field is a typed `DigestRef`
`{class, algorithm, value, key_id?}`, never an unlabelled hash (§14.2).

Type-tagged digests use one construction for every domain:

~~~text
canonical_bytes = JCS([ "<digest-domain-string>", value ])
digest          = SHA-256(canonical_bytes)        # or HMAC-SHA-256 per class
~~~

i.e. the domain tag is the first element of a two-element JSON array that is
canonicalized as a whole, so the digest input is itself one valid canonical
JSON text and two domains can never collide by byte concatenation. This is
the proposed concrete reading of DESIGN.md §14.2's
`DigestRef(JCS(type_tag("…") || value))`; it freezes with the bundle.

Current digest domains:

| Domain string | Value | Vectors |
|---|---|---|
| `bpp-idempotency-domain-v1` | `IdempotencyDomain` (§14.2) | `vectors/envelope/digest-*` |
| `bpa1-policy-v1` | BPA-1 policy AST (ADR-0001) | with the BPA-1 slice |

## ADR format

`adr/NNNN-short-title.md`, next free number, sections **Context / Decision
(proposed) / Criteria / Consequences**; an ADR is `proposed` until merged
with maintainer approval, then `accepted`; superseding links both ways. The
index in `adr/README.md` maps file numbers to the B0 plan's B-ADR ids.
