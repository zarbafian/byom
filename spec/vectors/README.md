# Golden vectors

Implementation-independent vectors for BPP: schema acceptance, strict I-JSON
and limit enforcement, and canonical-bytes/digest derivation. Layout mirrors
`akson/spec/vectors/`: one directory per family, one JSON file per case, and
a vector file is immutable once merged — fixes are new cases.

Every file is `{"name", "description", "input", "expected"}` where `name`
equals `<family>/<file-stem>`. Three input kinds, dispatched by key:

- **`input.schema`** (+ optional `input.ref`, a JSON pointer into that
  schema's `$defs`) — validate `input.value` against
  `../schemas/<schema>.schema.json`; `expected.valid` says whether it
  passes. Negative cases carry `expected.reason` for the human reader; the
  runner asserts the verdict, plus one convention check JSON Schema cannot
  express: for `bpp-failure`, problem `type` must equal exactly
  `https://byom.dev/problems/<kind>` (R0/BYOM-02).
- **`input.raw`**, **`input.raw_base64`**, or **`input.json_synth`** — C1
  family acceptance of exact bytes, *before* any schema
  (`../../family-vectors/PROFILE.md` §1, normative — R0/BYOM-03):
  token-order first-error reporting, the 256 KiB request cap (1 MiB with
  `input.context: "response"`), inclusive depth-64 and 65 536-node caps, the
  `$domain` reservation at every depth, unpaired surrogates, unsafe
  integers/floats, NaN/Infinity. `raw` carries the bytes as a JSON string,
  `raw_base64` as base64 (non-UTF-8 cases); `json_synth`
  (`{prefix, repeat, count, suffix}`, the family convention) has the runner
  build large bytes so the repository does not store megabyte literals.
  Negative cases carry the asserted profile `expected.error` class.
- **`input.domain`** + **`input.value`** (+ `input.index_secret_hex`, a test
  fixture in shape only, and `input.key_ref`) — the ratified
  idempotency-domain digest derivation (PROFILE.md §5, D-R0-1 — R0/BYOM-01):
  `expected.canonical` is `JCS(value ∪ {"$domain": domain})` and
  `expected.digest_ref` the typed `scope_erasure_safe` DigestRef whose
  `value_hex` is `HMAC-SHA-256(index key, canonical)`. The runner re-derives
  both.
- **`input.policy_op`** — a BPA-1 algebra case (ADR-0001 accepted, DESIGN.md
  §10.5): `well_formed`/`canonical` over `input.policy`, `intersect` over
  `input.a`/`input.b`, `is_subset` over `input.child`/`input.parent`,
  `decide` over `input.policy`/`input.request`. `expected.result` is the
  exact total-function result — `{"ok": true, ...}` or the typed rejection
  `{"ok": false, "error": {"kind", "where"}}` — and both independent
  evaluators (`../../policy/eval.py`, `../../policy/eval.mjs`) must
  re-derive it byte-for-byte under JCS (the B0.1 two-evaluator gate); the
  runner replays every case through both.

`envelope/` covers the §14.2 request/success/failure envelope, MutationMeta,
and the `bpp-idempotency-domain-v1` digest domain. `ops/` covers the B0.1
society + participants/candidates, work-lifecycle (endeavors, calls/pledges,
activities), and governance/acts/events (charter, mandates, acts, events +
recovery core) operation schemas
(`../schemas/ops/`): golden request shapes per op group plus negatives for
wrong-surface args (fields naming the channel-bound actor/surface — a
position filling another actor's seat, a non-pledgor delivery, a finalize
supplying a seat), missing required fields (a derivation without its exact
parent pin), candidate operations without offer scope, a read carrying
meta, a caller-shaped cursor naming its own audience, an events page over
the 512 cap, and problem shapes: the continuation-head conflict, the
mandate-derivation `authority_widening` rejection, and the spent one-shot
execution decision. `policy/` covers BPA-1 (`../schemas/bpa1-policy.
schema.json`, ADR-0001): schema acceptance for every §10.5 atom domain,
canonical bytes + `bpa1-policy-v1` digests, per-domain is_subset
positive/negative pairs (the §10.2 never-widening shapes MandateChain
models abstractly), deny preservation, intersect meets, deny-wins decide
cases, incomparable-reject cases, and malformed/overflow fail-closed
rejections with exact first-error pointers; the seeded differential
harness (`../../policy/differential.py`, pinned in `../../run-checks.sh`)
extends the same two-evaluator agreement over structured random cases.
