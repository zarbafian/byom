#!/usr/bin/env python3
"""Independent rederiver for the byom-hosted family vectors (C1).

Walks family-vectors/ (one directory per family, one JSON file per case:
{name, description, input, expected}) and re-derives every `expected` with
Python-stdlib-only code (json, hashlib, hmac, base64) that shares nothing
with the Rust/TypeScript implementations. Exits nonzero on any mismatch.

RFC 8785 (JCS) is implemented locally below -- no network, no third-party
packages. See PROFILE.md in this directory for the profile the vectors pin.

Run: python3 family-vectors/xcheck.py [root]
"""

from __future__ import annotations

import base64
import hashlib
import hmac
import json
import pathlib
import sys

FAILURES: list[str] = []


def fail(name: str, message: str) -> None:
    FAILURES.append(f"{name}: {message}")


def expect_eq(name: str, what: str, actual, expected) -> None:
    if actual != expected:
        fail(name, f"{what} differs\n  actual:   {actual!r}\n  expected: {expected!r}")


# --------------------------------------------------------------------------
# RFC 8785 JCS (minimal local implementation)
# --------------------------------------------------------------------------

SAFE_MAX = 2**53 - 1

_SHORT_ESCAPES = {8: "\\b", 9: "\\t", 10: "\\n", 12: "\\f", 13: "\\r", 34: '\\"', 92: "\\\\"}


def _es_number(v: float) -> str:
    """ECMAScript Number::toString(10) for a finite double (RFC 8785 3.2.2.3).

    Python's repr() already yields the shortest round-trip decimal digits
    (same digits as ES); only the layout rules differ, applied here.
    """
    if v != v or v in (float("inf"), float("-inf")):
        raise ValueError("non-finite number in JCS input")
    if v == 0.0:
        return "0"  # covers -0.0, as in ES
    sign = "-" if v < 0 else ""
    r = repr(abs(v))
    if "e" in r:
        mant, _, exp_s = r.partition("e")
        exp = int(exp_s)
    else:
        mant, exp = r, 0
    ip, _, fp = mant.partition(".")
    digits = (ip + fp).lstrip("0")
    stripped = digits.rstrip("0")
    trailing = len(digits) - len(stripped)
    k = len(stripped)
    n = k + trailing + exp - len(fp)  # value == 0.<stripped> * 10**n
    s = stripped
    if k <= n <= 21:
        out = s + "0" * (n - k)
    elif 0 < n <= 21:
        out = s[:n] + "." + s[n:]
    elif -6 < n <= 0:
        out = "0." + "0" * (-n) + s
    else:
        e = n - 1
        out = s[0] + ("." + s[1:] if k > 1 else "") + "e" + ("+" if e >= 0 else "-") + str(abs(e))
    return sign + out


def _escape(s: str) -> str:
    out = []
    for ch in s:
        o = ord(ch)
        if o in _SHORT_ESCAPES:
            out.append(_SHORT_ESCAPES[o])
        elif o < 0x20:
            out.append("\\u%04x" % o)
        else:
            out.append(ch)
    return "".join(out)


def _ser(v) -> str:
    if v is None:
        return "null"
    if v is True:
        return "true"
    if v is False:
        return "false"
    if isinstance(v, str):
        return '"' + _escape(v) + '"'
    if isinstance(v, int):
        if abs(v) > SAFE_MAX:
            raise ValueError("integer outside I-JSON safe range")
        return str(v)
    if isinstance(v, float):
        return _es_number(v)
    if isinstance(v, list):
        return "[" + ",".join(_ser(x) for x in v) + "]"
    if isinstance(v, dict):
        for k in v:
            if not isinstance(k, str):
                raise ValueError("non-string object key")
        keys = sorted(v, key=lambda k: k.encode("utf-16-be"))
        return "{" + ",".join('"' + _escape(k) + '":' + _ser(v[k]) for k in keys) + "}"
    raise ValueError(f"unsupported type {type(v)!r}")


def jcs(value) -> bytes:
    return _ser(value).encode("utf-8")


def tagged_jcs(tag: str, obj: dict) -> bytes:
    """Byom type-tagged canonical bytes: inject the reserved `$domain` member
    at the top level, then JCS. Fails closed if the object already claims a
    `$domain` (PROFILE.md section 2)."""
    if not isinstance(obj, dict):
        raise ValueError("type-tagged canonicalization requires an object")
    if "$domain" in obj:
        raise ValueError("object already carries a $domain member")
    return jcs({**obj, "$domain": tag})


def sha256_hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def hmac_sha256_hex(secret_hex: str, data: bytes) -> str:
    return hmac.new(bytes.fromhex(secret_hex), data, hashlib.sha256).hexdigest()


# --------------------------------------------------------------------------
# Strict I-JSON acceptance (profile caps -- PROFILE.md section 1)
# --------------------------------------------------------------------------

REQUEST_CAP = 256 * 1024    # bytes
RESPONSE_CAP = 1024 * 1024  # bytes (vectors select it with input.context == "response")
DEPTH_CAP = 64              # nested containers
NODE_CAP = 65536            # total JSON values per document


class IJsonError(Exception):
    def __init__(self, cls: str):
        super().__init__(cls)
        self.cls = cls


def _scan_one_json_text(text: str):
    """Single-pass validating token scanner for exactly one strict JSON text.

    Iterative (explicit container stack, no recursion), so pathological
    nesting inside the size cap can never raise RecursionError; the stack
    length is the container depth. Values are never materialized -- the
    scanner counts nodes, tracks maximum depth, records decoded-string
    surrogate health, and raises the order-3 error classes of PROFILE.md
    section 1 in token order: `syntax`, `trailing-data`, `duplicate`,
    `reserved-domain-collision`, `unsafe-integer`, `non-finite`,
    `unsafe-number`. Returns (nodes, max_depth, lone_surrogate).
    """
    pos = 0
    n = len(text)
    nodes = 0
    max_depth = 0
    lone_surrogate = False
    stack: list = []  # per-container: a key set for objects, None for arrays

    def syntax():
        raise IJsonError("syntax")

    def digit(i: int) -> bool:
        return i < n and "0" <= text[i] <= "9"

    def skip_ws():
        nonlocal pos
        while pos < n and text[pos] in " \t\n\r":
            pos += 1

    def scan_string() -> str:
        """Scan a string token at `pos` (opening quote), decoding escapes."""
        nonlocal pos, lone_surrogate
        pos += 1  # opening quote
        out: list[str] = []
        while True:
            if pos >= n:
                syntax()
            ch = text[pos]
            if ch == '"':
                pos += 1
                break
            if ch == "\\":
                pos += 1
                if pos >= n:
                    syntax()
                e = text[pos]
                if e in '"\\/':
                    out.append(e)
                elif e == "b":
                    out.append("\b")
                elif e == "f":
                    out.append("\f")
                elif e == "n":
                    out.append("\n")
                elif e == "r":
                    out.append("\r")
                elif e == "t":
                    out.append("\t")
                elif e == "u":
                    hex4 = text[pos + 1 : pos + 5]
                    if len(hex4) != 4 or any(c not in "0123456789abcdefABCDEF" for c in hex4):
                        syntax()
                    out.append(chr(int(hex4, 16)))
                    pos += 4
                else:
                    syntax()
                pos += 1
            elif ord(ch) < 0x20:
                syntax()  # raw control character in a string
            else:
                out.append(ch)
                pos += 1
        s = "".join(out)
        # Surrogate health after escape decoding (raw text is already valid
        # UTF-8, so unpaired halves can only arrive via \uXXXX escapes). The
        # profile reports this as its own ordered check (order 4), so only a
        # flag is recorded here.
        i = 0
        while i < len(s):
            u = ord(s[i])
            if 0xD800 <= u <= 0xDBFF:
                if i + 1 < len(s) and 0xDC00 <= ord(s[i + 1]) <= 0xDFFF:
                    i += 1
                else:
                    lone_surrogate = True
            elif 0xDC00 <= u <= 0xDFFF:
                lone_surrogate = True
            i += 1
        return s

    def scan_number():
        """Scan a number token at `pos` ('-' or digit) and classify it."""
        nonlocal pos
        start = pos
        if text[pos] == "-":
            pos += 1
            # json's -Infinity spelling is the non-finite class, not syntax
            if text.startswith("Infinity", pos):
                raise IJsonError("non-finite")
        if pos < n and text[pos] == "0":
            pos += 1
        elif digit(pos):
            while digit(pos):
                pos += 1
        else:
            syntax()
        is_float = False
        if pos < n and text[pos] == ".":
            is_float = True
            pos += 1
            if not digit(pos):
                syntax()
            while digit(pos):
                pos += 1
        if pos < n and text[pos] in "eE":
            is_float = True
            pos += 1
            if pos < n and text[pos] in "+-":
                pos += 1
            if not digit(pos):
                syntax()
            while digit(pos):
                pos += 1
        token = text[start:pos]
        if not is_float:
            # Exact magnitude check on the token, immune to double rounding.
            if abs(int(token)) > SAFE_MAX:
                raise IJsonError("unsafe-integer")
        else:
            v = float(token)
            if v != v or v in (float("inf"), float("-inf")):
                raise IJsonError("unsafe-number")
            if v.is_integer() and abs(v) > SAFE_MAX:
                raise IJsonError("unsafe-number")

    VALUE = 0            # a value is required
    VALUE_OR_CLOSE = 1   # just after '[': a value or ']'
    KEY_OR_CLOSE = 2     # just after '{': a key or '}'
    KEY = 3              # after ',' in an object: a key
    COLON = 4
    COMMA_OR_CLOSE = 5   # after a completed member/element
    state = VALUE
    done = False

    def bump_depth():
        nonlocal max_depth
        if len(stack) > max_depth:
            max_depth = len(stack)

    while not done:
        skip_ws()
        if pos >= n:
            syntax()
        ch = text[pos]
        if state in (VALUE, VALUE_OR_CLOSE):
            if state == VALUE_OR_CLOSE and ch == "]":
                pos += 1
                stack.pop()
                if not stack:
                    done = True
                else:
                    state = COMMA_OR_CLOSE
                continue
            if ch == "{":
                pos += 1
                nodes += 1
                stack.append(set())
                bump_depth()
                state = KEY_OR_CLOSE
            elif ch == "[":
                pos += 1
                nodes += 1
                stack.append(None)
                bump_depth()
                state = VALUE_OR_CLOSE
            elif ch == '"':
                scan_string()
                nodes += 1
                done, state = (True, state) if not stack else (False, COMMA_OR_CLOSE)
            elif ch == "-" or digit(pos):
                scan_number()
                nodes += 1
                done, state = (True, state) if not stack else (False, COMMA_OR_CLOSE)
            elif text.startswith("true", pos):
                pos += 4
                nodes += 1
                done, state = (True, state) if not stack else (False, COMMA_OR_CLOSE)
            elif text.startswith("false", pos):
                pos += 5
                nodes += 1
                done, state = (True, state) if not stack else (False, COMMA_OR_CLOSE)
            elif text.startswith("null", pos):
                pos += 4
                nodes += 1
                done, state = (True, state) if not stack else (False, COMMA_OR_CLOSE)
            elif text.startswith("NaN", pos) or text.startswith("Infinity", pos):
                raise IJsonError("non-finite")
            else:
                syntax()
        elif state in (KEY_OR_CLOSE, KEY):
            if state == KEY_OR_CLOSE and ch == "}":
                pos += 1
                stack.pop()
                if not stack:
                    done = True
                else:
                    state = COMMA_OR_CLOSE
                continue
            if ch != '"':
                syntax()
            # Member names in token order: the reserved-name check precedes
            # the duplicate check for the same token; names compare after
            # escape decoding (RFC 7493).
            key = scan_string()
            if key == "$domain":
                raise IJsonError("reserved-domain-collision")
            keys = stack[-1]
            if key in keys:
                raise IJsonError("duplicate")
            keys.add(key)
            state = COLON
        elif state == COLON:
            if ch != ":":
                syntax()
            pos += 1
            state = VALUE
        else:  # COMMA_OR_CLOSE
            top_keys = stack[-1]
            if ch == ",":
                pos += 1
                state = KEY if top_keys is not None else VALUE
            elif ch == ("}" if top_keys is not None else "]"):
                pos += 1
                stack.pop()
                if not stack:
                    done = True
                else:
                    state = COMMA_OR_CLOSE
            else:
                syntax()
    skip_ws()
    if pos < n:
        raise IJsonError("trailing-data")  # exactly one JSON text
    return nodes, max_depth, lone_surrogate


def ijson_class(data: bytes, context: str = "request"):
    """Returns None when `data` is an acceptable strict-I-JSON body for the
    given context ("request": 256 KiB cap; "response": 1 MiB cap), else the
    profile error class. Check order: size, UTF-8, token scan (syntax /
    trailing-data / duplicates / reserved `$domain` / numeric caps /
    non-finite), surrogates, depth, node count."""
    cap = RESPONSE_CAP if context == "response" else REQUEST_CAP
    if len(data) > cap:
        return "oversize"
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError:
        return "invalid-utf8"
    try:
        nodes, max_depth, lone_surrogate = _scan_one_json_text(text)
    except IJsonError as e:
        return e.cls
    if lone_surrogate:
        return "unpaired-surrogate"
    if max_depth > DEPTH_CAP:
        return "over-depth"
    if nodes > NODE_CAP:
        return "over-nodes"
    return None


def vector_bytes(inp: dict) -> bytes:
    if "json_utf8" in inp:
        return inp["json_utf8"].encode("utf-8")
    if "json_base64" in inp:
        return base64.b64decode(inp["json_base64"])
    if "json_synth" in inp:
        s = inp["json_synth"]
        return (s.get("prefix", "") + s.get("repeat", "") * s.get("count", 0) + s.get("suffix", "")).encode("utf-8")
    raise ValueError("no input bytes")


# --------------------------------------------------------------------------
# Idempotency-domain digests (PROFILE.md section 5)
# --------------------------------------------------------------------------

KOVEE_COD_DOMAIN = "dev.kovee.canonical-object-digest.v1"
KOVEE_TBD_DOMAIN = b"dev.kovee.typed-bytes-digest.v1"
BYOM_IDEMPOTENCY_TAG = "bpp-idempotency-domain-v1"


def kovee_canonical_object(object_kind: str, schema_ref: str, projection: dict) -> bytes:
    return jcs(
        {
            "$domain": KOVEE_COD_DOMAIN,
            "protocol_major": 0,
            "object_kind": object_kind,
            "schema_ref": schema_ref,
            "projection": projection,
        }
    )


def _frame(b: bytes) -> bytes:
    return len(b).to_bytes(8, "big") + b


def kovee_typed_bytes_digest(domain: str, media_or_schema_ref: str, data: bytes) -> str:
    return sha256_hex(
        _frame(KOVEE_TBD_DOMAIN)
        + _frame(domain.encode("utf-8"))
        + _frame(b"0")
        + _frame(media_or_schema_ref.encode("utf-8"))
        + _frame(data)
    )


def dsse_pae(payload_type: str, payload: bytes) -> bytes:
    return b"DSSEv1 %d %s %d %s" % (len(payload_type), payload_type.encode("utf-8"), len(payload), payload)


def _content_data(d: dict) -> bytes:
    if "bytes_utf8" in d:
        return d["bytes_utf8"].encode("utf-8")
    if "bytes_base64" in d:
        return base64.b64decode(d["bytes_base64"])
    raise ValueError("no content bytes")


def derive_idempotency(d: dict) -> dict:
    kind = d["kind"]
    if kind == "bpp-idempotency-domain-v1":
        canonical = tagged_jcs(BYOM_IDEMPOTENCY_TAG, d["domain_object"])
        value = hmac_sha256_hex(d["index_secret_hex"], canonical)
        return {
            "canonical": canonical.decode("utf-8"),
            "digest_ref": {
                "class": "scope_erasure_safe",
                "algorithm": "hmac-sha-256",
                "key_ref": d["key_ref"],
                "value_hex": value,
            },
        }
    if kind == "kcp-command-idempotency":
        if "projection" in d:
            projection = d["projection"]
        else:
            raw = d["raw_command"]
            projection = {k: raw[k] for k in d["projection_fields"] if k in raw}
        canonical = kovee_canonical_object("kcp-command-idempotency", d["schema_ref"], projection)
        return {"canonical": canonical.decode("utf-8"), "sha256_hex": sha256_hex(canonical)}
    if kind == "dev.kovee.canonical-object-digest.v1":
        canonical = kovee_canonical_object(d["object_kind"], d["schema_ref"], d["projection"])
        return {"canonical": canonical.decode("utf-8"), "sha256_hex": sha256_hex(canonical)}
    if kind == "dev.kovee.typed-bytes-digest.v1":
        return {"digest_hex": kovee_typed_bytes_digest(d["domain"], d["media_or_schema_ref"], _content_data(d))}
    if kind == "akson-dsse-pae":
        p = dsse_pae(d["payload_type"], d["payload_utf8"].encode("utf-8"))
        return {"pae_utf8": p.decode("utf-8"), "sha256_hex": sha256_hex(p)}
    if kind == "byom-tagged-structural":
        canonical = tagged_jcs(d["type_tag"], d["object"])
        return {
            "canonical": canonical.decode("utf-8"),
            "digest_ref": {"class": "structural_public", "algorithm": "sha-256", "value_hex": sha256_hex(canonical)},
        }
    raise ValueError(f"unknown derivation kind {kind!r}")


def _primary_hex(result: dict) -> str:
    if "digest_ref" in result:
        return result["digest_ref"]["value_hex"]
    return result.get("sha256_hex") or result["digest_hex"]


# --------------------------------------------------------------------------
# RFC 9457 problem shape (PROFILE.md section 3)
# --------------------------------------------------------------------------

PROBLEM_TYPE_PREFIX = "https://byom.dev/problems/"

PROBLEM_KINDS = frozenset(
    """
    invalid unsupported_version feature_unavailable forbidden_surface
    forbidden not_found stale_revision stale_binding stale_assembly_epoch
    stale_lease idempotency_mismatch position_ineligible decision_incomplete
    independence_conflict authority_widening mandate_held admission_required
    classification_unmapped policy_conflict policy_overflow budget_exceeded
    effect_ambiguous authority_witness_unknown endpoint_sealed cursor_expired
    unavailable formation_requires_participation
    external_command_not_terminalizable internal
    """.split()
)


def problem_shape(env):
    if not isinstance(env, dict) or env.get("outcome") != "problem":
        return False, "envelope-outcome"
    p = env.get("problem")
    if not isinstance(p, dict):
        return False, "problem-not-object"
    for member in ("type", "title", "kind"):
        if member not in p:
            return False, f"missing-{member}"
    if not isinstance(p["title"], str):
        return False, "title-not-string"
    if not isinstance(p["kind"], str) or p["kind"] not in PROBLEM_KINDS:
        return False, "unknown-kind"
    if p["type"] != PROBLEM_TYPE_PREFIX + p["kind"]:
        return False, "type-kind-mismatch"
    if "status" in p:
        st = p["status"]
        if isinstance(st, bool) or not isinstance(st, int):
            return False, "status-not-integer"
        if not 400 <= st <= 599:
            return False, "status-out-of-range"
    return True, None


# --------------------------------------------------------------------------
# Digest classes (byom section 14.2, D-R0-1; PROFILE.md section 6)
# --------------------------------------------------------------------------

# Class -> the only algorithm valid for it (PROFILE.md section 6.1).
DIGEST_CLASS_ALGORITHM = {
    "structural_public": "sha-256",
    "portable_public": "sha-256",
    "disclosed_party": "sha-256",
    "ciphertext_public": "sha-256",
    "local_erasure_safe": "hmac-sha-256",
    "scope_erasure_safe": "hmac-sha-256",
}

KEYED_CLASSES = frozenset({"local_erasure_safe", "scope_erasure_safe"})

# Classes whose construction is over type-tagged canonical bytes; raw-bytes
# content under them takes the domain-separated byte preimage (section 6.4).
CANONICAL_BYTES_CLASSES = frozenset({"structural_public", "local_erasure_safe", "scope_erasure_safe"})

WIRE_MEMBERS = frozenset({"class", "algorithm", "key_ref", "value_hex"})

HEX_LOWER = set("0123456789abcdef")

ERASABLE_KINDS = {"erasable_plaintext_object", "erasable_plaintext_bytes", "erasable_index_object"}

BYOM_TBD_DOMAIN = b"bpp-typed-bytes-digest-v1"


def byom_byte_preimage(byte_domain: str, media_type: str, data: bytes) -> bytes:
    """Domain-separated preimage for raw-bytes content under a canonical-bytes
    class (PROFILE.md section 6.4): the family typed-bytes framing rule with
    the byom domain constant."""
    return (
        _frame(BYOM_TBD_DOMAIN)
        + _frame(byte_domain.encode("utf-8"))
        + _frame(b"0")
        + _frame(media_type.encode("utf-8"))
        + _frame(data)
    )


def preimage_bytes(content: dict, clazz: str) -> bytes:
    """The bytes a digest of class `clazz` commits to for this content:
    type-tagged canonical bytes for objects; the framed byte preimage for raw
    bytes under canonical-bytes classes; exact raw bytes for the exact-bytes
    classes (portable_public, disclosed_party, ciphertext_public)."""
    if "object" in content:
        return tagged_jcs(content["type_tag"], content["object"])
    raw = _content_data(content)
    if clazz in CANONICAL_BYTES_CLASSES:
        return byom_byte_preimage(content["byte_domain"], content["media_type"], raw)
    return raw


def _is_key_material(member: str) -> bool:
    """Raw HMAC keys, secrets, and salts are never part of a DigestRef; the
    key id (`key_ref`) is."""
    if member == "key_ref":
        return False
    low = member.lower()
    return "secret" in low or "salt" in low or low in ("key", "key_hex", "hmac_key")


def validate_wire(offered):
    """The closed DigestRef wire shape {class, algorithm, key_ref?, value_hex}
    (PROFILE.md section 6.3 steps 1-7), validated before any class logic.
    Returns None when well-formed, else the rejection reason."""
    if not isinstance(offered, dict):
        return "untyped_digest_forbidden"
    if any(_is_key_material(k) for k in offered):
        return "digest_ref_carries_key_material"
    if set(offered) - WIRE_MEMBERS:
        return "digest_ref_unknown_member"
    for member in ("class", "algorithm", "value_hex"):
        if not isinstance(offered.get(member), str):
            return "digest_ref_missing_member"
    clazz = offered["class"]
    if clazz not in DIGEST_CLASS_ALGORITHM:
        return "unknown_digest_class"
    if offered["algorithm"] != DIGEST_CLASS_ALGORITHM[clazz]:
        return "digest_ref_algorithm_class_mismatch"
    if clazz in KEYED_CLASSES:
        key_ref = offered.get("key_ref")
        if not isinstance(key_ref, str) or not key_ref:
            return "digest_ref_key_ref_missing"
    elif "key_ref" in offered:
        return "digest_ref_key_ref_forbidden"
    value = offered["value_hex"]
    if len(value) != 64 or any(c not in HEX_LOWER for c in value):
        return "digest_ref_value_not_64_hex"
    return None


def evaluate_offer(required: str, content: dict, offered, disclosure, recipients, offered_secret_hex=None):
    """The profile acceptance rule for a digest offered where a schema field
    requires class `required` (PROFILE.md section 6.3): wire validation first,
    then per-class construction rules, then class equality, then value
    re-derivation where the verifier holds the material to re-derive."""
    wire = validate_wire(offered)
    if wire is not None:
        return False, wire
    clazz = offered["class"]
    kind = content["kind"]
    if clazz == "structural_public":
        if kind == "authority_subject":
            return False, "authority_subject_requires_local_erasure_safe"
        if kind in ERASABLE_KINDS:
            return False, "public_hash_over_erasable_content_forbidden"
        if kind != "protocol_bytes":
            return False, "structural_public_requires_protocol_bytes"
    elif clazz == "portable_public":
        if not (disclosure or {}).get("durable_identifier_accepted"):
            return False, "portable_requires_durable_identifier_disclosure"
    elif clazz == "local_erasure_safe":
        if kind == "sealed_blob":
            return False, "sealed_blob_requires_ciphertext_public"
    elif clazz == "scope_erasure_safe":
        if kind == "sealed_blob":
            return False, "sealed_blob_requires_ciphertext_public"
        if kind == "authority_subject":
            return False, "authority_subject_requires_local_erasure_safe"
    elif clazz == "disclosed_party":
        if not recipients:
            return False, "disclosed_party_requires_named_recipients"
    elif clazz == "ciphertext_public":
        if kind != "sealed_blob":
            return False, "ciphertext_public_requires_ciphertext"
    if clazz != required:
        return False, "digest_class_mismatch"
    pre = preimage_bytes(content, clazz)
    if clazz in KEYED_CLASSES:
        if offered_secret_hex is None:
            return True, None  # well-typed; offline re-derivation needs the key
        expected_hex = hmac_sha256_hex(offered_secret_hex, pre)
    else:
        expected_hex = sha256_hex(pre)
    if offered["value_hex"] != expected_hex:
        return False, "digest_value_mismatch"
    return True, None


# Reasons produced before the construction/class steps: the offered value is
# not required to be arithmetically meaningful for these.
WIRE_SHAPE_REASONS = frozenset(
    {
        "digest_ref_carries_key_material",
        "digest_ref_unknown_member",
        "digest_ref_missing_member",
        "unknown_digest_class",
        "digest_ref_algorithm_class_mismatch",
        "digest_ref_key_ref_missing",
        "digest_ref_key_ref_forbidden",
        "digest_ref_value_not_64_hex",
    }
)


# --------------------------------------------------------------------------
# PrivacyAccessRecord chain (byom section 15.4, D-R0-1; PROFILE.md section 7)
# --------------------------------------------------------------------------

PRIVACY_TAG = "bpp-privacy-access-record-v1"

# Every preimage member of a PrivacyAccessRecord except the chain link
# (previous_access_digest, absent at genesis) and the record's own
# record_digest, which is EXCLUDED from the preimage.
PRIVACY_REQUIRED_MEMBERS = (
    "society_id",
    "internal_access_sequence",
    "access_event_id",
    "endpoint_incarnation",
    "recovery_epoch",
    "actor_binding_digest",
    "operation",
    "purpose_ref",
    "query_or_scope_digest",
    "result_object_count",
    "result_bytes",
    "outcome",
    "dependency_digest",
    "occurred_at",
)


class PrivacyChainError(Exception):
    def __init__(self, error: str):
        super().__init__(error)
        self.error = error


def derive_privacy_chain(records, chain_secret_hex, key_ref):
    derived = []
    prev_value = None
    for rec in records:
        for member in PRIVACY_REQUIRED_MEMBERS:
            if member not in rec:
                raise PrivacyChainError(f"privacy_record_missing_{member}")
        if "record_digest" in rec:
            raise PrivacyChainError("privacy_record_preimage_carries_record_digest")
        rec = dict(rec)
        if prev_value is not None and "previous_access_digest" not in rec:
            rec["previous_access_digest"] = {
                "class": "scope_erasure_safe",
                "algorithm": "hmac-sha-256",
                "key_ref": key_ref,
                "value_hex": prev_value,
            }
        # Genesis is whole-member absence: no previous_access_digest member,
        # never a null-valued pseudo-DigestRef.
        canonical = tagged_jcs(PRIVACY_TAG, rec)
        prev_value = hmac_sha256_hex(chain_secret_hex, canonical)
        derived.append(
            {
                "canonical": canonical.decode("utf-8"),
                "record_digest": {
                    "class": "scope_erasure_safe",
                    "algorithm": "hmac-sha-256",
                    "key_ref": key_ref,
                    "value_hex": prev_value,
                },
            }
        )
    return derived


def privacy_release(last_outcome: str, journal_committed: bool):
    """Sensitive plaintext or search results are released only when the
    covering PrivacyAccessRecord has outcome `allowed` AND it committed to the
    non-rollbackable access journal (receipt stored) before release."""
    if last_outcome != "allowed":
        return False, "access_denied"
    if not journal_committed:
        return False, "privacy_access_record_commit_failed"
    return True, None


# --------------------------------------------------------------------------
# Family checkers
# --------------------------------------------------------------------------


def check_ijson(name: str, case: dict) -> int:
    inp = case["input"]
    cls = ijson_class(vector_bytes(inp), inp.get("context", "request"))
    exp = case["expected"]
    expect_eq(name, "validity", cls is None, exp["valid"])
    if not exp["valid"]:
        expect_eq(name, "error class", cls, exp["error"])
    return 1


def check_jcs(name: str, case: dict) -> int:
    canonical = jcs(case["input"]["value"])
    exp = case["expected"]
    expect_eq(name, "canonical", canonical.decode("utf-8"), exp["canonical"])
    expect_eq(name, "sha256", sha256_hex(canonical), exp["sha256_hex"])
    return 1


def check_problem(name: str, case: dict) -> int:
    valid, error = problem_shape(case["input"]["envelope"])
    exp = case["expected"]
    expect_eq(name, "validity", valid, exp["valid"])
    if not exp["valid"]:
        expect_eq(name, "error class", error, exp["error"])
    return 1


def check_idempotency(name: str, case: dict) -> int:
    results = [derive_idempotency(d) for d in case["input"]["derivations"]]
    exp = case["expected"]
    expect_eq(name, "results", results, exp["results"])
    relation = exp.get("relation")
    if relation:
        hexes = [_primary_hex(r) for r in results]
        if relation == "distinct":
            expect_eq(name, "distinct digests", len(set(hexes)), len(hexes))
        elif relation == "equal":
            expect_eq(name, "equal digests", len(set(hexes)), 1)
        else:
            fail(name, f"unknown relation {relation!r}")
    return 1


def _offered_secret(sub: dict, inp: dict):
    for holder in (sub, inp):
        for member in ("object_secret_hex", "scope_secret_hex"):
            if member in holder:
                return holder[member]
    return None


def check_digest_class(name: str, case: dict) -> int:
    inp = case["input"]
    content = inp["content"]
    required = inp["required_class"]
    subs = case.get("cases")
    if subs is None:
        subs = [
            {
                "name": None,
                "offered": inp.get("offered"),
                "expected": case["expected"],
            }
        ]
    count = 0
    for sub in subs:
        cname = name + (f"/{sub['name']}" if sub.get("name") else "")
        disclosure = sub.get("disclosure", inp.get("disclosure"))
        recipients = sub.get("recipients", inp.get("recipients"))
        secret = _offered_secret(sub, inp)
        offered = sub["offered"]
        exp = sub["expected"]
        ok, reason = evaluate_offer(required, content, offered, disclosure, recipients, secret)
        if exp["accepted"]:
            # Positive cases validate the OFFERED ref (wire, construction,
            # class, and value re-derivation) -- never synthesize-and-compare.
            expect_eq(cname, "acceptance", (ok, reason), (True, None))
            expect_eq(cname, "digest_ref", offered, exp["digest_ref"])
            if "canonical" in exp:
                expect_eq(cname, "canonical", tagged_jcs(content["type_tag"], content["object"]).decode("utf-8"), exp["canonical"])
            if required == "disclosed_party":
                expect_eq(cname, "external_copy_obligation", True, exp.get("external_copy_obligation"))
        else:
            expect_eq(cname, "acceptance", ok, False)
            expect_eq(cname, "rejection reason", reason, exp["reason"])
            # Internal consistency: a typing-only rejection's offered value
            # must be arithmetically correct under the OFFERED class, so each
            # rejection is proven to be typing-only, never a wrong-bytes
            # accident. Wire-shape rejections have no meaningful value; the
            # digest_value_mismatch negative instead proves its value is
            # exactly the un-framed raw-bytes digest.
            if isinstance(offered, str):
                exact = tagged_jcs(content["type_tag"], content["object"]) if "object" in content else _content_data(content)
                expect_eq(cname, "offered value (untyped)", offered, sha256_hex(exact))
            elif reason == "digest_value_mismatch":
                expect_eq(
                    cname,
                    "offered value (raw, un-framed preimage)",
                    offered["value_hex"],
                    hmac_sha256_hex(secret, _content_data(content)),
                )
            elif reason not in WIRE_SHAPE_REASONS:
                pre = preimage_bytes(content, offered["class"])
                if offered["class"] in KEYED_CLASSES:
                    expect_eq(cname, "offered value (hmac)", offered["value_hex"], hmac_sha256_hex(secret, pre))
                else:
                    expect_eq(cname, "offered value (sha-256)", offered["value_hex"], sha256_hex(pre))
        count += 1
    return count


def check_privacy(name: str, case: dict) -> int:
    inp, exp = case["input"], case["expected"]
    if exp.get("chain_valid") is False:
        try:
            derive_privacy_chain(inp["records"], inp["chain_secret_hex"], inp["key_ref"])
        except PrivacyChainError as e:
            expect_eq(name, "chain error", e.error, exp["error"])
        else:
            fail(name, "chain derivation unexpectedly succeeded")
        return 1
    derived = derive_privacy_chain(inp["records"], inp["chain_secret_hex"], inp["key_ref"])
    expect_eq(name, "records", derived, exp["records"])
    release, reason = privacy_release(inp["records"][-1]["outcome"], inp["journal_committed"])
    expect_eq(name, "release_permitted", release, exp["release_permitted"])
    expect_eq(name, "release reason", reason, exp.get("reason"))
    return 1


CHECKERS = {
    "ijson": check_ijson,
    "jcs": check_jcs,
    "problem": check_problem,
    "idempotency": check_idempotency,
    "digest-class": check_digest_class,
    "privacy": check_privacy,
}


def main() -> int:
    root = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else pathlib.Path(__file__).resolve().parent
    files = 0
    cases = 0
    per_family: dict[str, int] = {}
    for path in sorted(root.rglob("*.json")):
        family = path.relative_to(root).parts[0]
        if family == "tscheck":  # the TypeScript rederiver's own tree (package.json), not a family
            continue
        checker = CHECKERS.get(family)
        if checker is None:
            fail(str(path), f"no checker registered for family {family!r}")
            continue
        case = json.loads(path.read_text())
        expected_name = f"{family}/{path.stem}"
        if case.get("name") != expected_name:
            fail(str(path), f"vector name {case.get('name')!r} != {expected_name!r}")
        try:
            n = checker(case.get("name", str(path)), case)
        except Exception as e:  # a malformed vector is a failure, not a crash
            fail(case.get("name", str(path)), f"checker raised {type(e).__name__}: {e}")
            n = 0
        files += 1
        cases += n
        per_family[family] = per_family.get(family, 0) + n

    if FAILURES:
        print(f"xcheck: {len(FAILURES)} failure(s) across {files} vector file(s)")
        for f in FAILURES:
            print(f"  FAIL {f}")
        return 1
    if files == 0:
        print(f"xcheck: no vectors found under {root}")
        return 1
    detail = ", ".join(f"{k}={v}" for k, v in sorted(per_family.items()))
    print(f"xcheck: {files} vector files, {cases} cases OK ({detail})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
