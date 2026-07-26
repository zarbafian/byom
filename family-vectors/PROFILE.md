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
| 3 | no `$domain` member name, at any depth (section 2 reservation) | `reserved-domain-collision` |
| 3 | integers within ±(2^53 − 1) | `unsafe-integer` |
| 3 | no `NaN`/`Infinity` literals | `non-finite` |
| 3 | floats finite; integer-valued floats within ±(2^53 − 1) | `unsafe-number` |
| 4 | no unpaired surrogates after escape decoding | `unpaired-surrogate` |
| 5 | container nesting depth at most 64 | `over-depth` |
| 6 | at most 65 536 JSON values per document | `over-nodes` |

The order-3 classes surface in token order during the single parse: the first
offending token names the error (an early duplicate beats a later unsafe
number — `ijson/mixed-error-order`). Within a single member-name token, the
reserved-name check precedes the duplicate check. Implementations must
process tokens iteratively: nesting bounded only by the size cap (thousands
of levels — `ijson/pathological-depth`) may never crash a rederiver; the cap
is enforced as `over-depth` after the scan. Responses follow the same rules
under a 1 MiB cap; a vector opts into that context with `input.context:
"response"` (`ijson/response-cap-oversize`,
`ijson/response-request-cap-inapplicable`). Per-design list caps (kovee
§11.8: at most 256 list items per request, 512 events per page; byom §14.9
identifier, title, and prose byte caps) remain owned by each design and are
not re-pinned here. The concrete depth-64 / 65 536-node numbers are
profile-pinned because both designs say only "bounded" (see Profile-pinned
decisions).

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

### 3.1 Problem conventions side by side — divergence is intentional

Byom and kovee both emit RFC 9457 problem objects, but their kind and type
conventions diverge deliberately and are **non-substitutable across
protocols** (byom §14.2/§14.9; kovee §11.7):

| | byom (BPP) | kovee (KCP) |
|---|---|---|
| kind casing | snake_case (`stale_revision`) | kebab-case (`stale-revision`) |
| where the kind lives | a separate required `kind` member AND the `type` suffix | only inside the `type` URN — no separate kind member |
| `type` | `https://byom.dev/problems/<kind>` (HTTPS namespace), exactly | `urn:kovee:error:<kind>` (URN, no HTTPS namespace) |
| `status` | optional; when present a JSON integer in 400–599 | per-kind table value of kovee §11.7 |
| kind enum | the closed 29-kind enum of byom §14.9 | the closed kind table of kovee §11.7 |

Non-substitutability rule: a byom problem `type` (or `kind`) never satisfies
a kovee problem field and vice versa, even for semantically matching errors
(`stale_revision` vs `stale-revision`); there is no translation table, and
each protocol validates only its own convention. A validator that accepts
the other project's spelling or namespace fails these vectors' intent.

## 4. Digest domains side by side — divergence is intentional

| Project | Domain / discriminator | Construction | Output |
|---|---|---|---|
| kovee | `dev.kovee.canonical-object-digest.v1` | `SHA-256(JCS({"$domain": …, "protocol_major": 0, "object_kind", "schema_ref", "projection"}))` (kovee §11.8) | bare 32-byte digest, typed by its schema field |
| kovee | `kcp-command-idempotency` | the canonical-object digest with `object_kind: "kcp-command-idempotency"`; projection `{version, authority_surface, op, realm_id, project_id?, expected_revision?, args, ext}` — excludes `request_id`, `traceparent`, transport headers, causation telemetry (kovee §11.6) | bare 32-byte digest |
| kovee | `dev.kovee.typed-bytes-digest.v1` | `SHA-256(frame(domain-const) ‖ frame(domain) ‖ frame("0") ‖ frame(media_or_schema_ref) ‖ frame(bytes))`, `frame(x) = uint64_be(len(x)) ‖ x` (kovee §11.8) | bare 32-byte digest |
| byom | `bpp-idempotency-domain-v1` | `HMAC-SHA-256(per-Society index key, tagged("bpp-idempotency-domain-v1", IdempotencyDomain))` (byom §14.2, D-R0-1) | `DigestRef` class `scope_erasure_safe` |
| akson | DSSE payload types (`application/vnd.akson.*`) | PAE: `"DSSEv1 " len(type) " " type " " len(payload) " " payload` | signature input / SHA-256 over PAE |

Intentional divergences — none of these is a defect, and no value from one
row ever satisfies a field of another, even byte-equal (kovee §11.8, byom
§14.2; pinned by `idempotency/cross-domain-separation`):

- kovee digests are bare values whose type lives in the schema field; byom
  digests are self-describing `DigestRef` objects (section 6).
- kovee separates domains by a framed length-prefixed preamble
  (typed-bytes) or a `$domain` member (canonical-object); akson separates by
  DSSE payload type in the PAE, not by JSON at all.
- byom's idempotency domain digest is keyed (HMAC under the per-Society
  index key, scope-erasable); kovee's command idempotency digest is a public
  structural SHA-256 of non-sensitive command material. Both are correct for
  their store.

## 5. Idempotency-domain derivations (`idempotency/`)

`input.derivations[].kind` selects the construction; `expected.results` holds
the re-derived canonical bytes and digests, and optional
`expected.relation: "equal" | "distinct"` pins cross-derivation claims.

| kind | derives |
|---|---|
| `bpp-idempotency-domain-v1` | `canonical` (tagged JCS of the IdempotencyDomain) + `digest_ref` (HMAC, `scope_erasure_safe`) |
| `kcp-command-idempotency` | `canonical` + `sha256_hex` (projection given directly or as `projection_fields` over `raw_command`) |
| `dev.kovee.canonical-object-digest.v1` | `canonical` + `sha256_hex` |
| `dev.kovee.typed-bytes-digest.v1` | `digest_hex` |
| `akson-dsse-pae` | `pae_utf8` + `sha256_hex` |
| `byom-tagged-structural` | `canonical` + `digest_ref` (`structural_public`) |

The byom `IdempotencyDomain` is `{actor_binding_digest, operation,
endpoint_incarnation, society_id, society_recovery_epoch, idempotency_key}`
(byom §14.2). Its digest is keyed because the domain embeds an actor binding
and a caller-chosen key: erasable, low-entropy material that must never get a
public hash (section 6). The HMAC key is the Society's idempotency-index
key — one shared key across every entry of the index, i.e. a **scope** key,
so the digest class is `scope_erasure_safe` (D-R0-1): destroying that key
erases offline verifiability of the entire index, never one entry.
`society_recovery_epoch` and the endpoint incarnation are digest material,
so an old-domain request can never resolve in a new domain
(`bpp-domain-epoch-separation`).

## 6. Digest classes (byom §14.2, D-R0-1, `digest-class/`)

### 6.1 DigestRef wire shape and construction

Every digest field is a typed `DigestRef`, never an unlabelled hash:

~~~json
{ "class": "local_erasure_safe", "algorithm": "hmac-sha-256",
  "key_ref": "society-key:soc-garden/object:engram-42", "value_hex": "…" }
~~~

The wire shape is **closed**: exactly the members `class`, `algorithm`,
`value_hex`, plus `key_ref` — and nothing else. `key_ref` is required
(non-empty string) exactly for the keyed erasure classes and forbidden for
the public classes. Algorithm pairing is closed: `sha-256` only for the four
public classes, `hmac-sha-256` only for the two erasure classes. `value_hex`
is exactly 64 lowercase hex characters. Key id, class, algorithm, and value
are part of the ref; raw HMAC keys, per-object secrets, and salts never are
(any member whose name contains `secret` or `salt`, or is a raw key member
such as `key`/`key_hex`/`hmac_key`, is key material and fails closed). The
six classes:

| Class | Construction | Allowed use / erasure semantics |
|---|---|---|
| `structural_public` | SHA-256 over type-tagged canonical bytes | knowingly non-sensitive, non-erasable protocol/schema bytes only |
| `portable_public` | SHA-256 over exact bytes | content whose owner explicitly accepted a durable, publicly dictionary-testable identifier; required for truly portable content |
| `local_erasure_safe` | HMAC-SHA-256 over type-tagged canonical bytes, random **per-object** secret protected by a Society key | ordinary erasable local content and authority subjects; destroying the object secret destroys exactly that object's offline verification |
| `scope_erasure_safe` | HMAC-SHA-256 over type-tagged canonical bytes, protected **per-scope** key (per-Society index key, per-chain key) — D-R0-1 | shared-key index and chain constructions: idempotency indexes, privacy/audit chains. Honest semantics: destroying the scope key erases verifiability for the **entire scope** — every index entry, the whole chain — never one object. Single-object erasure does not destroy verification of the rest of the scope |
| `disclosed_party` | SHA-256 over exact bytes already disclosed to named recipients | visible only to those parties; always accompanied by the external-copy obligation |
| `ciphertext_public` | SHA-256 over encrypted blob bytes | sealed blobs only; never a commitment to low-entropy plaintext |

### 6.2 Content addressing and forbidden substitutions

- Public SHA-256 over ordinary erasable low-entropy content is forbidden
  (`structural_public` there, or `portable_public` without disclosure, is
  rejected).
- Content addressing is typed: `local_erasure_safe` for erasable per-object
  plaintext, `scope_erasure_safe` for erasable index/chain records,
  `ciphertext_public` for sealed blobs, `portable_public` only after the
  explicit durable-identifier disclosure — never a silent upgrade.
- Authority subjects take `local_erasure_safe` commitments (strictly
  per-object), never a public hash and never a scope-keyed digest.
- **The cross-boundary class rule** (added by the 2026-07-26 live-seam
  decision, S-2). A digest one protocol DEMANDS from the other across the
  protocol boundary MUST be `portable_public`: the counterparty has to derive
  the same value from the same bytes, and a keyed class is an HMAC under the
  owner's secret — the counterparty could only echo an opaque blob it can
  never check, and D-R1-2 forbids re-deriving such a value from a shared key.
  Crossing the boundary IS the durable-identifier disclosure this class
  requires: the peer already holds the content, and the digest is computed
  over exactly the FROZEN cross-boundary fragment (the members both sides
  hold), never over the owner's whole erasable record — so
  `public_hash_over_erasable_content_forbidden` is untouched. The converse
  half is equally normative: a digest the owner recomputes from its OWN
  committed state keeps `local_erasure_safe` and is therefore **never a
  request member at all** — an implementation that asks a counterparty for a
  value it computes itself has mis-drawn the boundary, and its class choice
  buys nothing while costing the counterparty per-object erasure storage.
- The two erasure classes are **mutually non-substitutable** (D-R0-1): a
  per-scope key never stands in for a per-object secret (erasing one object
  must kill exactly that object's verifiability) and a per-object secret
  never stands in for a scope key (index/chain verification must survive
  single-object erasure). Public classes remain forbidden for erasable
  content.
- Kovee's retained plaintext `raw_sha256` (kovee §10.10/§11.8) is amended by
  kovee amendment A5: as a family class it is unknown and fails closed. K0/K1
  artifact stores must pass these vectors.
- Classes are never interchangeable: a well-constructed digest of the wrong
  class is `digest_class_mismatch` even when the 32-byte value spaces
  coincide. The `matrix-required-*` vectors pin the complete 6×5 ordered
  substitution matrix with arithmetically correct offered values.

### 6.3 Acceptance rule

A digest offered where a schema field requires class `R` is checked in this
order. Steps 1–7 validate the **offered wire object** before any class or
content logic; construction violations (step 8) are reported before the
generic mismatch; positives are judged by validating the offered ref, never
by synthesizing a fresh ref and comparing:

1. typed at all (a JSON object) → else `untyped_digest_forbidden`
2. carries no key material → else `digest_ref_carries_key_material`
3. no member outside `{class, algorithm, key_ref, value_hex}` → else
   `digest_ref_unknown_member`; `class`, `algorithm`, `value_hex` present as
   strings → else `digest_ref_missing_member`
4. `class` is one of the six classes → else `unknown_digest_class`
5. class/algorithm pairing (`sha-256` public, `hmac-sha-256` erasure) → else
   `digest_ref_algorithm_class_mismatch`
6. `key_ref` present (non-empty) for the keyed erasure classes, absent for
   the public classes → else `digest_ref_key_ref_missing` /
   `digest_ref_key_ref_forbidden`
7. `value_hex` is exactly 64 lowercase hex characters → else
   `digest_ref_value_not_64_hex`
8. per-class construction rule → else one of
   `authority_subject_requires_local_erasure_safe`,
   `public_hash_over_erasable_content_forbidden`,
   `structural_public_requires_protocol_bytes`,
   `portable_requires_durable_identifier_disclosure`,
   `sealed_blob_requires_ciphertext_public`,
   `disclosed_party_requires_named_recipients`,
   `ciphertext_public_requires_ciphertext`
9. class equals `R` → else `digest_class_mismatch`
10. where the verifier holds the content (and, for keyed classes, the key),
    the value re-derives over the section 6.1/6.4 preimage → else
    `digest_value_mismatch`. A verifier without the key accepts the ref as
    well-typed but unverified — that is the honest limit of keyed digests.

Every forbidden substitution is a negative vector whose offered value is
arithmetically correct under the offered class (the vector embeds the HMAC
test secret where needed — test-only material, akson's convention for
signature keys), so each rejection is proven to be typing-only, never a
wrong-bytes accident. Wire-shape negatives (steps 2–7) are exempt: their
values are deliberately malformed or ambiguous. The `digest_value_mismatch`
negative proves its offered value is exactly the un-framed raw-bytes digest,
isolating the missing domain separation.

### 6.4 Byte content preimage: domain-separated typed-bytes framing

Classes whose construction is "over type-tagged canonical bytes"
(`structural_public`, `local_erasure_safe`, `scope_erasure_safe`) digest
**object** content as `tagged(type_tag, object)` (section 2). When such a
class digests **raw bytes** content (`*_bytes` content kinds, and blob bytes
if ever offered under them), the preimage is the domain-separated typed-bytes
frame — never the raw bytes:

~~~text
byte_preimage(byte_domain, media_type, bytes) =
  frame("bpp-typed-bytes-digest-v1") ‖ frame(byte_domain) ‖ frame("0")
  ‖ frame(media_type) ‖ frame(bytes)          frame(x) = uint64_be(len(x)) ‖ x
~~~

This mirrors the construction shape of kovee's
`dev.kovee.typed-bytes-digest.v1` (section 4) under the byom domain constant
`bpp-typed-bytes-digest-v1` (the `"0"` frame is the protocol major), so the
two projects' byte digests can never collide even over identical bytes.
Byte-content vectors carry `byte_domain` and `media_type` beside the bytes.
The exact-bytes classes (`portable_public`, `disclosed_party`,
`ciphertext_public`) keep their byom-§14.2 exact-bytes constructions: their
commitment is to bytes a counterparty already holds.

## 7. PrivacyAccessRecord before sensitive release (`privacy/`)

Allowed and denied sensitive reads both append to the privacy-access chain
(byom §15.4). The exact preimage (D-R0-1, R0/FV-04):

A `PrivacyAccessRecord` carries exactly these preimage members, all
**required**: `society_id`, `internal_access_sequence`, `access_event_id`,
`endpoint_incarnation`, `recovery_epoch`, `actor_binding_digest`,
`operation`, `purpose_ref`, `query_or_scope_digest`, `result_object_count`,
`result_bytes`, `outcome` (`allowed | denied | error`), `dependency_digest`,
`occurred_at` — plus the chain link `previous_access_digest`, which at
genesis is **wholly absent** (no member; never a null-valued
pseudo-DigestRef). The record's own self-referential `record_digest` member
is **EXCLUDED** from the preimage. A record missing a required member fails
closed (`privacy_record_missing_<member>`); a record offering its own
`record_digest` inside the preimage fails closed
(`privacy_record_preimage_carries_record_digest`).

~~~text
preimage        = tagged("bpp-privacy-access-record-v1",
                         record without its record_digest member)
record_digest   = DigestRef(scope_erasure_safe, hmac-sha-256, chain key_ref,
                            HMAC-SHA-256(chain key, preimage))
previous_access_digest
                = DigestRef(scope_erasure_safe, hmac-sha-256, chain key_ref,
                            previous record_digest.value_hex)
                  — wholly absent at genesis
~~~

The chain key is one key for the whole chain — a scope key — so both the
record digest and the chain link are class `scope_erasure_safe` (D-R0-1):
destroying the chain key erases verifiability of the entire chain, never one
record. The output is the typed `record_digest` DigestRef, never a bare hex.

Release rule: sensitive plaintext or search results are released only when
the covering record's `outcome` is `allowed` **and** the record has committed
to the separate non-rollbackable access journal (receipt stored) before
release. Otherwise:

- `outcome` ≠ `allowed` → no release, reason `access_denied` — the denied
  read still chains a record;
- journal commit failed → no release, reason
  `privacy_access_record_commit_failed` — unlogged bytes are never served.

Records carry actor, purpose, canonical query/scope digest, result
cardinality and bytes, dependencies, and outcome — never result plaintext.

## 8. Vector conventions and freezing

- `name` equals `family/filename-stem`; `xcheck.py` enforces it.
- Vectors are frozen once merged; fixes are new cases (akson's rule).
  R0 exception, recorded: the C1 corpus predates the D-R0-1 class
  ratification and the R0 wire/preimage corrections, so the affected
  idempotency, privacy, and digest-class vectors were regenerated in place
  under the R0 dispositions (2026-07-25) rather than duplicated.
- Inputs are given as parsed values (`value`), UTF-8 text (`json_utf8`),
  base64 (`json_base64` / `bytes_base64`), or synthesized repetition
  (`json_synth`) for cap cases. `ijson` inputs may set `context: "response"`
  to select the 1 MiB response cap.
- A digest-class file may carry `cases: [...]` instead of a single
  `input.offered`/`expected` pair: each sub-case supplies `name`, `offered`,
  optional per-case `disclosure` / `recipients` / `object_secret_hex` /
  `scope_secret_hex`, and its own `expected`. Both rederivers count each
  sub-case individually.
- Embedded secrets (`*_secret_hex`) are test fixtures in shape only.

## Profile-pinned decisions

Choices this profile fixes because the pinned designs state the requirement
without the exact value or encoding. Divergence from these is a profile
change, not an implementation choice:

1. JSON depth cap 64 and node cap 65 536 (designs: "bounded").
2. The I-JSON error-class taxonomy and check order of section 1, including
   token-order first-error reporting, the iterative-processing requirement,
   and the `context: "response"` 1 MiB-cap convention (amended R0/FV-05).
3. Problem `type` URI prefix `https://byom.dev/problems/` and the rule
   `type = prefix + kind`; the cross-protocol non-substitutability rule of
   section 3.1 (amended R0/L4-01).
4. Byom type-tag encoding: reserved top-level `$domain` member injected
   before JCS; fail-closed on collision (byom writes
   `JCS(type_tag(…) ‖ Object)` without fixing the byte encoding).
5. `bpp-idempotency-domain-v1` digests are class `scope_erasure_safe` under
   the per-Society index key — a scope key (re-classed by D-R0-1; byom
   writes `DigestRef(…)` without naming the class; §14.2's erasable-content
   rule forces a keyed class, and the shared index key forces the scope
   class, not `local_erasure_safe`).
6. `DigestRef` wire shape: closed member set
   `{class, algorithm, key_ref?, value_hex}`; lowercase algorithm names
   `sha-256` / `hmac-sha-256` with closed class/algorithm pairing; `key_ref`
   required exactly for the keyed erasure classes and forbidden otherwise;
   `value_hex` exactly 64 lowercase hex characters (amended R0/FV-02).
7. The rejection-reason identifiers of section 6.3, including the wire
   identifiers `digest_ref_unknown_member`, `digest_ref_missing_member`,
   `digest_ref_algorithm_class_mismatch`, `digest_ref_key_ref_missing`,
   `digest_ref_key_ref_forbidden`, `digest_ref_value_not_64_hex`, and the
   re-derivation identifier `digest_value_mismatch` (amended R0/FV-02), and
   the key-material member-name rule of section 6.1.
8. Privacy-chain construction of section 7 (amended by D-R0-1 and R0/FV-04):
   exact preimage member list with required `dependency_digest`; the
   self-referential `record_digest` excluded from the preimage; genesis link
   as whole-member absence; tag `bpp-privacy-access-record-v1`; chain-keyed
   HMAC record digests emitted as typed `scope_erasure_safe` DigestRefs;
   fail-closed identifiers `privacy_record_missing_<member>` and
   `privacy_record_preimage_carries_record_digest`.
9. Content-kind vocabulary for digest-class vectors: `protocol_bytes`,
   `erasable_plaintext_object`, `erasable_plaintext_bytes`,
   `erasable_index_object` (added with D-R0-1 for scope-keyed index/chain
   records), `sealed_blob`, `authority_subject`; byte-content vectors carry
   `byte_domain` and `media_type` beside the bytes.
10. `disclosed_party` acceptance always asserts the external-copy obligation.
11. The byte-content preimage of section 6.4: raw bytes under a
    canonical-bytes class take the `bpp-typed-bytes-digest-v1` framed
    preimage, mirroring kovee's typed-bytes construction shape under the
    byom domain string; exact-bytes classes stay raw (added R0/FV-03 — byom
    §14.2 requires type-tagged canonical bytes without fixing a byte
    encoding for non-object content).
12. The `$domain` member name is reserved at every depth of wire bodies,
    error class `reserved-domain-collision`, surfaced in token order with
    the reserved-name check before the duplicate check for the same token
    (added R0/FV-05 — kovee and byom both inject `$domain` only during
    canonicalization; it is never a wire member).
13. Acceptance step 10 of section 6.3: positive digests are validated by
    re-deriving the OFFERED ref's value where key material is available,
    and a verifier without the key treats a keyed ref as well-typed but
    unverified (added R0/FV-02).
14. The cross-boundary class rule of section 6.2 (added by the 2026-07-26
    live-seam decision S-2). Both designs name `portable_public` as the class
    for "truly portable content" without saying which fields are portable;
    two independent implementations meeting at the seam showed the omission
    is load-bearing — byom demanded four `local_erasure_safe` values from
    Kovee, two of which byom could not recompute either, which forced Kovee
    to add per-object erasure secrets for nothing and left one field
    obtainable only by reading byom's database. The rule fixes the missing
    normative half in BOTH directions (demanded across the boundary ⇒
    `portable_public` over a frozen cross-boundary fragment; recomputable by
    the owner ⇒ `local_erasure_safe` and not a request member). **Both repos
    mirror this section:** it is a family-contract rule, not a byom
    implementation choice.
