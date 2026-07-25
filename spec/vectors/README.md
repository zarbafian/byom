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
  runner asserts only the verdict.
- **`input.raw`** or **`input.synthetic`** — strict I-JSON + limit
  acceptance of exact bytes, *before* any schema: duplicate keys, unsafe
  integers, NaN/Infinity, and the §14.9 256 KiB request ceiling fail
  closed. `raw` carries the bytes as a JSON string; `synthetic`
  (`oversized_request` + `target_bytes`) has the runner build a valid JSON
  text of exactly `target_bytes` bytes so the repository does not store a
  quarter-megabyte literal.
- **`input.domain`** + **`input.value`** — digest derivation:
  `expected.canonical` is `JCS([domain, value])` and `expected.sha256_hex`
  its SHA-256 (type-tag construction, `../README.md`). The runner
  re-derives both.

`envelope/` covers the §14.2 request/success/failure envelope, MutationMeta,
and the `bpp-idempotency-domain-v1` digest domain.
