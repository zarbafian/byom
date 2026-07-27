# Golden vectors

Implementation-independent vectors for BPP: schema acceptance, strict I-JSON
and limit enforcement, and canonical-bytes/digest derivation. Layout mirrors
`akson/spec/vectors/`: one directory per family, one JSON file per case, and
a vector file is immutable once merged — fixes are new cases.

Every file is `{"name", "description", "input", "expected"}` where `name`
equals `<family>/<file-stem>`. `python3 ../../conformance/run.py` replays all
462 cases and prints the per-kind tally. Seven input kinds, dispatched by key:

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
- **`input.machine`** + **`input.steps`** — an executable state walk over the
  committed descriptor JSON, the §14.8 closed-machine rule as an oracle
  independent of both TLC and the parity checker. Walks start at `absent`;
  an `accepted` step must be an exact descriptor row, a `rejected` step must
  be an absent row (an unlisted transition is invalid), a `replay` step
  retries the preceding mutation and must be state-idempotent, and
  `{"crash": true}` restarts the daemon between steps — every
  descriptor-level variable is durable, so the walk resumes unchanged.
  `expected.final_state` pins where it lands.
- **`input.tool_call`** — a C3a MCP tool invocation (`../schemas/mcp-tools.
  schema.json`, `../../mcp/byom-mcp.tools.json` v0.1.1): the runner checks
  the tool exists in the right profile, that its arguments render the exact
  BPP envelope for the bound operation, and that no channel-derived member
  (actor, participant, surface) can be supplied by the caller.
- **`input.permit_probe`** — a §13.1 step-6 consumption oracle: a stored
  one-shot decision plus a schema-valid `execution_permit_consume`, with
  `expected` naming the problem kind the oracle must answer. Each probe also
  proves the positive half (the byte-identical canonical request replays to
  the retained receipt), so a rejection is never vacuous.

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

Three families the layout above does not cover:

- `governed-work/` — the C2 `byom_governed_work_v1` record schemas
  (`../governed-work/*.schema.json`), including the amendment-A9
  `kovee-governance-owner-binding-v2` narrowing, whose
  `-withdrawn-owner-arm-invalid` case feeds the exact value v1 accepted so
  the narrowing is proven rather than asserted — plus 12 executable saga
  walks. Six are C2 slice 1 (greenfield retry-identical and
  rollback-then-new-epoch; the four endeavor-formation paths: happy,
  awaiting-principal resubmit, crash → remote-unknown → query-committed, and
  terminalize-released) and six are slice 2 (subordinate
  reserve→commit→settle and uncertain→query→resolve, the ByomEpisodeBinding
  fence walk, dispatch happy and ambiguous-then-disposition, and the
  onboarding-compute one-shot).
- `machines/` — 16 §14.8 crash/replay state walks over the B0.1 machines
  (5 Pledge, 4 Episode/lease, 4 ActIntent, 3 AuthorityJournal), covering
  one-shot permit consumption under replay, the expired-lease re-claim as a
  real fence-minting transition rather than a replay, ambiguous-never-
  completed, terminal-is-final for every modeled terminal, and the
  witness-unknown query/abandon-after-proof recovery paths.
- `mcp/` — the C3a tool bindings: six `tool_call` renderings plus the
  `attached_harness` Manifestation profile and its wrong-`host_kind`
  negative, and a governance operation proved absent from the tool document.
