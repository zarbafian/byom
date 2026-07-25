# Family encoding profile (C1)

The shared substrate every akson, kovee, and byom implementation must accept,
canonicalize, and derive identically. Byom hosts this directory; kovee and
akson consume it via the lock manifest (plan D3, sheet C1).

One directory per family, one JSON file per case (akson's convention). A
complete vector — `jcs/basic-ordering.json`:

~~~json
{
  "name": "jcs/basic-ordering",
  "description": "object keys serialize in sorted order regardless of input order",
  "input": { "value": { "b": 2, "a": 1, "c": [3, "x"] } },
  "expected": {
    "canonical": "{\"a\":1,\"b\":2,\"c\":[3,\"x\"]}",
    "sha256_hex": "7f26f35fd8404944dd9a2441528c810b6c55fb2ae7b569b0438d309d81806acf"
  }
}
~~~

Re-derive everything:

~~~text
python3 family-vectors/xcheck.py     # exit 0 = every `expected` re-derived
~~~

`xcheck.py` is the independent Python-stdlib rederiver (nonzero on any
mismatch, and on an empty vector tree). Per the sheet, each project's CI also
re-derives the same files: akson `python3 xcheck/run.py family-vectors`;
kovee/byom `cargo test -p family-vectors` and `npm test --prefix tscheck`.

Families: `ijson/`, `jcs/`, `problem/`, `idempotency/`, `digest-class/`,
`privacy/`. Sections below define what each family pins.

## 1. Strict I-JSON acceptance (`ijson/`)

A request body is a single UTF-8 I-JSON text within hard caps. Checks run in
this order; the first failure names the vector error class:

| Order | Check | Error class |
|---|---|---|
| 1 | at most 262 144 bytes (256 KiB request cap) | `oversize` |
| 2 | valid UTF-8 | `invalid-utf8` |
| 3 | parses as exactly one JSON text | `syntax`, `trailing-data` |
| 3 | no duplicate member names, at any depth (RFC 7493) | `duplicate` |
| 3 | integers within ±(2^53 − 1) | `unsafe-integer` |
| 3 | no `NaN`/`Infinity` literals | `non-finite` |
| 3 | floats finite; integer-valued floats within ±(2^53 − 1) | `unsafe-number` |
| 4 | no unpaired surrogates after escape decoding | `unpaired-surrogate` |
| 5 | container nesting depth at most 64 | `over-depth` |
| 6 | at most 65 536 JSON values per document | `over-nodes` |

The order-3 classes surface in token order during the single parse. Responses
follow the same rules under a 1 MiB cap. Per-design list caps (kovee §11.8: at
most 256 list items per request, 512 events per page; byom §14.9 identifier,
title, and prose byte caps) remain owned by each design and are not re-pinned
here. The concrete depth-64 / 65 536-node numbers are profile-pinned because
both designs say only "bounded" (see Profile-pinned decisions).

## 2. Canonical bytes: RFC 8785 JCS and type tags (`jcs/`)

Canonical bytes are RFC 8785 JCS: object keys sorted by UTF-16 code units
(an astral key such as U+10000 sorts as its surrogate pair D800 DC00 — before
U+FF61, unlike UTF-8 byte order), numbers in ECMAScript `Number::toString(10)`
minimal form (`10.0` → `10`, `-0.0` → `0`, `1e-07` → `1e-7`, `1e21` →
`1e+21`), short escapes plus `\u00xx` for remaining C0 controls, everything
else literal UTF-8.

Byom type-tagged canonical bytes inject the reserved `$domain` member at the
top level, then apply JCS:

~~~text
tagged(tag, obj) = JCS(obj ∪ {"$domain": tag})   # obj must not carry $domain
~~~

`$domain` (0x24) sorts before every letter, so the tag always leads the
canonical bytes. An object that already carries `$domain` fails closed. This
mirrors kovee's `$domain` envelope member so the tag namespace is shared while
the byte layouts stay distinct.

Scope limit (documented per the sheet): the profile's JCS covers exactly the
value space section 1 admits — finite numbers, ints within ±(2^53 − 1),
string keys, no unpaired surrogates. That is the entire space the family ever
canonicalizes; non-I-JSON input is rejected in section 1, never canonicalized.

## 3. RFC 9457 problem shape (`problem/`)

Byom's failure envelope (byom §14.2) wraps an RFC 9457 problem object:

~~~json
{
  "outcome": "problem",
  "problem": {
    "type": "https://byom.dev/problems/forbidden",
    "title": "Forbidden",
    "kind": "forbidden",
    "status": 403
  }
}
~~~

- `type`, `title`, `kind` are required; `title` is a string.
- `kind` is the closed 29-kind enum of byom §14.9 (`invalid` …
  `internal`); unknown kinds fail closed.
- `type` equals `https://byom.dev/problems/` + `kind`, exactly.
- `status`, when present, is a JSON integer in 400–599 (never a bool or
  string).
- RFC 9457 extension members (`detail`, `instance`, domain extensions) are
  allowed and carry no authority.
- Problems never disclose hidden object, participant, peer, path, policy, or
  membership existence (byom §14.9); vectors test shape, not wording.

## 4. Digest domains side by side — divergence is intentional

| Project | Domain / discriminator | Construction | Output |
|---|---|---|---|
| kovee | `dev.kovee.canonical-object-digest.v1` | `SHA-256(JCS({"$domain": …, "protocol_major": 0, "object_kind", "schema_ref", "projection"}))` (kovee §11.8) | bare 32-byte digest, typed by its schema field |
| kovee | `kcp-command-idempotency` | the canonical-object digest with `object_kind: "kcp-command-idempotency"`; projection `{version, authority_surface, op, realm_id, project_id?, expected_revision?, args, ext}` — excludes `request_id`, `traceparent`, transport headers, causation telemetry (kovee §11.6) | bare 32-byte digest |
| kovee | `dev.kovee.typed-bytes-digest.v1` | `SHA-256(frame(domain-const) ‖ frame(domain) ‖ frame("0") ‖ frame(media_or_schema_ref) ‖ frame(bytes))`, `frame(x) = uint64_be(len(x)) ‖ x` (kovee §11.8) | bare 32-byte digest |
| byom | `bpp-idempotency-domain-v1` | `HMAC-SHA-256(index secret, tagged("bpp-idempotency-domain-v1", IdempotencyDomain))` (byom §14.2) | `DigestRef` class `local_erasure_safe` |
| akson | DSSE payload types (`application/vnd.akson.*`) | PAE: `"DSSEv1 " len(type) " " type " " len(payload) " " payload` | signature input / SHA-256 over PAE |

Intentional divergences — none of these is a defect, and no value from one
row ever satisfies a field of another, even byte-equal (kovee §11.8, byom
§14.2; pinned by `idempotency/cross-domain-separation`):

- kovee digests are bare values whose type lives in the schema field; byom
  digests are self-describing `DigestRef` objects (section 6).
- kovee separates domains by a framed length-prefixed preamble
  (typed-bytes) or a `$domain` member (canonical-object); akson separates by
  DSSE payload type in the PAE, not by JSON at all.
- byom's idempotency domain digest is keyed (HMAC, erasable); kovee's command
  idempotency digest is a public structural SHA-256 of non-sensitive command
  material. Both are correct for their store.

## 5. Idempotency-domain derivations (`idempotency/`)

`input.derivations[].kind` selects the construction; `expected.results` holds
the re-derived canonical bytes and digests, and optional
`expected.relation: "equal" | "distinct"` pins cross-derivation claims.

| kind | derives |
|---|---|
| `bpp-idempotency-domain-v1` | `canonical` (tagged JCS of the IdempotencyDomain) + `digest_ref` (HMAC, `local_erasure_safe`) |
| `kcp-command-idempotency` | `canonical` + `sha256_hex` (projection given directly or as `projection_fields` over `raw_command`) |
| `dev.kovee.canonical-object-digest.v1` | `canonical` + `sha256_hex` |
| `dev.kovee.typed-bytes-digest.v1` | `digest_hex` |
| `akson-dsse-pae` | `pae_utf8` + `sha256_hex` |
| `byom-tagged-structural` | `canonical` + `digest_ref` (`structural_public`) |

The byom `IdempotencyDomain` is `{actor_binding_digest, operation,
endpoint_incarnation, society_id, society_recovery_epoch, idempotency_key}`
(byom §14.2). Its digest is keyed because the domain embeds an actor binding
and a caller-chosen key: erasable, low-entropy material that must never get a
public hash (section 6). `society_recovery_epoch` and the endpoint
incarnation are digest material, so an old-domain request can never resolve
in a new domain (`bpp-domain-epoch-separation`).

## 6. Digest classes (byom §14.2, `digest-class/`)

### 6.1 DigestRef and construction

Every digest field is a typed `DigestRef`, never an unlabelled hash:

~~~json
{ "class": "local_erasure_safe", "algorithm": "hmac-sha-256",
  "key_ref": "society-key:soc-garden/object:engram-42", "value_hex": "…" }
~~~

Key id, class, algorithm, and value are part of the ref; raw HMAC keys and
per-object salts never are. The five classes:

| Class | Construction | Allowed use |
|---|---|---|
| `structural_public` | SHA-256 over type-tagged canonical bytes | knowingly non-sensitive, non-erasable protocol/schema bytes only |
| `portable_public` | SHA-256 | content whose owner explicitly accepted a durable, publicly dictionary-testable identifier; required for truly portable content |
| `local_erasure_safe` | HMAC-SHA-256 over type-tagged canonical bytes, random per-object secret protected by a Society key | ordinary erasable local content and authority subjects; destroying the object secret destroys offline verification |
| `disclosed_party` | SHA-256 over exact bytes already disclosed to named recipients | visible only to those parties; always accompanied by the external-copy obligation |
| `ciphertext_public` | SHA-256 over encrypted blob bytes | sealed blobs only; never a commitment to low-entropy plaintext |

### 6.2 Content addressing and forbidden substitutions

- Public SHA-256 over ordinary erasable low-entropy content is forbidden
  (`structural_public` there, or `portable_public` without disclosure, is
  rejected).
- Content addressing is typed: `local_erasure_safe` for erasable plaintext,
  `ciphertext_public` for sealed blobs, `portable_public` only after the
  explicit durable-identifier disclosure — never a silent upgrade.
- Authority subjects take `local_erasure_safe` commitments, never a public
  hash.
- Kovee's retained plaintext `raw_sha256` (kovee §10.10/§11.8) is amended by
  kovee amendment A5: as a family class it is unknown and fails closed. K0/K1
  artifact stores must pass these vectors.
- Classes are never interchangeable: a well-constructed digest of the wrong
  class is `digest_class_mismatch` even when the 32-byte value spaces
  coincide.

### 6.3 Acceptance rule

A digest offered where a schema field requires class `R` is checked in this
order; construction violations are reported before the generic mismatch:

1. typed at all → else `untyped_digest_forbidden`
2. carries no key material → else `digest_ref_carries_key_material`
3. per-class construction rule → else one of
   `authority_subject_requires_local_erasure_safe`,
   `public_hash_over_erasable_content_forbidden`,
   `structural_public_requires_protocol_bytes`,
   `portable_requires_durable_identifier_disclosure`,
   `sealed_blob_requires_ciphertext_public`,
   `disclosed_party_requires_named_recipients`,
   `ciphertext_public_requires_ciphertext`,
   `unknown_digest_class`
4. class equals `R` → else `digest_class_mismatch`

Every forbidden substitution is a negative vector whose offered value is
arithmetically correct (the vector embeds the HMAC test secret where needed —
test-only material, akson's convention for signature keys), so each rejection
is proven to be typing-only, never a wrong-bytes accident.

## 7. PrivacyAccessRecord before sensitive release (`privacy/`)

Allowed and denied sensitive reads both append to the privacy-access chain
(byom §15.4). Vector construction:

~~~text
record.previous_access_digest = DigestRef(local_erasure_safe,
                                          value_hex of the previous record's
                                          digest; null at genesis)
record_digest = HMAC-SHA-256(chain secret,
                             tagged("bpp-privacy-access-record-v1", record))
~~~

Release rule: sensitive plaintext or search results are released only when
the covering record's `outcome` is `allowed` **and** the record has committed
to the separate non-rollbackable access journal (receipt stored) before
release. Otherwise:

- `outcome` ≠ `allowed` → no release, reason `access_denied` — the denied
  read still chains a record;
- journal commit failed → no release, reason
  `privacy_access_record_commit_failed` — unlogged bytes are never served.

Records carry actor, purpose, canonical query/scope digest, result
cardinality and bytes, and outcome — never result plaintext.

## 8. Vector conventions and freezing

- `name` equals `family/filename-stem`; `xcheck.py` enforces it.
- Vectors are frozen once merged; fixes are new cases (akson's rule).
- Inputs are given as parsed values (`value`), UTF-8 text (`json_utf8`),
  base64 (`json_base64` / `bytes_base64`), or synthesized repetition
  (`json_synth`) for cap cases.
- Embedded secrets (`*_secret_hex`) are test fixtures in shape only.

## Profile-pinned decisions

Choices this profile fixes because the pinned designs state the requirement
without the exact value or encoding. Divergence from these is a profile
change, not an implementation choice:

1. JSON depth cap 64 and node cap 65 536 (designs: "bounded").
2. The I-JSON error-class taxonomy and check order of section 1.
3. Problem `type` URI prefix `https://byom.dev/problems/` and the rule
   `type = prefix + kind`.
4. Byom type-tag encoding: reserved top-level `$domain` member injected
   before JCS; fail-closed on collision (byom writes
   `JCS(type_tag(…) ‖ Object)` without fixing the byte encoding).
5. `bpp-idempotency-domain-v1` digests are class `local_erasure_safe` with a
   per-Society index secret (byom writes `DigestRef(…)` without naming the
   class; §14.2's erasable-content rule forces a keyed class).
6. `DigestRef` wire shape `{class, algorithm, key_ref?, value_hex}` with
   lowercase algorithm names `sha-256` / `hmac-sha-256`.
7. The rejection-reason identifiers of section 6.3.
8. Privacy-chain construction of section 7 (genesis link `null`, tag
   `bpp-privacy-access-record-v1`, chain-keyed HMAC record digests).
9. Content-kind vocabulary for digest-class vectors: `protocol_bytes`,
   `erasable_plaintext_object`, `erasable_plaintext_bytes`, `sealed_blob`,
   `authority_subject`.
10. `disclosed_party` acceptance always asserts the external-copy obligation.
