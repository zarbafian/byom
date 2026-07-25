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

REQUEST_CAP = 256 * 1024   # bytes
RESPONSE_CAP = 1024 * 1024  # bytes (stated for completeness; vectors use REQUEST_CAP)
DEPTH_CAP = 64             # nested containers
NODE_CAP = 65536           # total JSON values per document


class IJsonError(Exception):
    def __init__(self, cls: str):
        super().__init__(cls)
        self.cls = cls


def _pairs_hook(pairs):
    seen = set()
    for key, _ in pairs:
        if key in seen:
            raise IJsonError("duplicate")
        seen.add(key)
    return dict(pairs)


def _int_hook(token: str):
    value = int(token)
    if abs(value) > SAFE_MAX:
        raise IJsonError("unsafe-integer")
    return value


def _float_hook(token: str):
    value = float(token)
    if value != value or value in (float("inf"), float("-inf")):
        raise IJsonError("unsafe-number")
    if value.is_integer() and abs(value) > SAFE_MAX:
        raise IJsonError("unsafe-number")
    return value


def _const_hook(token: str):
    raise IJsonError("non-finite")


def _has_surrogate(v) -> bool:
    if isinstance(v, str):
        return any(0xD800 <= ord(c) <= 0xDFFF for c in v)
    if isinstance(v, list):
        return any(_has_surrogate(x) for x in v)
    if isinstance(v, dict):
        return any(_has_surrogate(k) or _has_surrogate(x) for k, x in v.items())
    return False


def _depth(v) -> int:
    if isinstance(v, list):
        return 1 + max((_depth(x) for x in v), default=0)
    if isinstance(v, dict):
        return 1 + max((_depth(x) for x in v.values()), default=0)
    return 0


def _nodes(v) -> int:
    if isinstance(v, list):
        return 1 + sum(_nodes(x) for x in v)
    if isinstance(v, dict):
        return 1 + sum(_nodes(x) for x in v.values())
    return 1


def ijson_class(data: bytes):
    """Returns None when `data` is an acceptable strict-I-JSON request body,
    else the profile error class. Check order: size, UTF-8, parse (syntax /
    duplicates / numeric caps / non-finite), surrogates, depth, node count."""
    if len(data) > REQUEST_CAP:
        return "oversize"
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError:
        return "invalid-utf8"
    try:
        value = json.loads(
            text,
            object_pairs_hook=_pairs_hook,
            parse_int=_int_hook,
            parse_float=_float_hook,
            parse_constant=_const_hook,
        )
    except IJsonError as e:
        return e.cls
    except json.JSONDecodeError as e:
        return "trailing-data" if e.msg.startswith("Extra data") else "syntax"
    if _has_surrogate(value):
        return "unpaired-surrogate"
    if _depth(value) > DEPTH_CAP:
        return "over-depth"
    if _nodes(value) > NODE_CAP:
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
                "class": "local_erasure_safe",
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
# Digest classes (byom section 14.2; PROFILE.md section 6)
# --------------------------------------------------------------------------

DIGEST_CLASSES = ("structural_public", "portable_public", "local_erasure_safe", "disclosed_party", "ciphertext_public")

KEY_MATERIAL_FIELDS = {"object_secret_hex", "secret_hex", "key_hex", "hmac_key", "secret", "salt", "salt_hex"}

ERASABLE_KINDS = {"erasable_plaintext_object", "erasable_plaintext_bytes"}


def content_bytes(content: dict) -> bytes:
    if "object" in content:
        return tagged_jcs(content["type_tag"], content["object"])
    return _content_data(content)


def evaluate_offer(required: str, content_kind: str, offered, disclosure, recipients):
    """The profile acceptance rule for a digest offered where a schema field
    requires `required`. Construction violations are reported before the
    generic class mismatch (PROFILE.md section 6.3)."""
    if not isinstance(offered, dict):
        return False, "untyped_digest_forbidden"
    if KEY_MATERIAL_FIELDS & offered.keys():
        return False, "digest_ref_carries_key_material"
    cls = offered.get("class")
    if cls == "structural_public":
        if content_kind == "authority_subject":
            return False, "authority_subject_requires_local_erasure_safe"
        if content_kind in ERASABLE_KINDS:
            return False, "public_hash_over_erasable_content_forbidden"
        if content_kind != "protocol_bytes":
            return False, "structural_public_requires_protocol_bytes"
    elif cls == "portable_public":
        if not (disclosure or {}).get("durable_identifier_accepted"):
            return False, "portable_requires_durable_identifier_disclosure"
    elif cls == "local_erasure_safe":
        if content_kind == "sealed_blob":
            return False, "sealed_blob_requires_ciphertext_public"
    elif cls == "disclosed_party":
        if not recipients:
            return False, "disclosed_party_requires_named_recipients"
    elif cls == "ciphertext_public":
        if content_kind != "sealed_blob":
            return False, "ciphertext_public_requires_ciphertext"
    else:
        return False, "unknown_digest_class"
    if cls != required:
        return False, "digest_class_mismatch"
    return True, None


def build_digest_ref(clazz: str, cbytes: bytes, secret_hex, key_ref):
    if clazz == "local_erasure_safe":
        return {
            "class": clazz,
            "algorithm": "hmac-sha-256",
            "key_ref": key_ref,
            "value_hex": hmac_sha256_hex(secret_hex, cbytes),
        }
    return {"class": clazz, "algorithm": "sha-256", "value_hex": sha256_hex(cbytes)}


# --------------------------------------------------------------------------
# PrivacyAccessRecord chain (byom section 15.4; PROFILE.md section 7)
# --------------------------------------------------------------------------

PRIVACY_TAG = "bpp-privacy-access-record-v1"


def derive_privacy_chain(records, chain_secret_hex, key_ref):
    derived = []
    prev_value = None
    for rec in records:
        rec = dict(rec)
        if "previous_access_digest" not in rec:
            rec["previous_access_digest"] = {
                "class": "local_erasure_safe",
                "algorithm": "hmac-sha-256",
                "key_ref": key_ref,
                "value_hex": prev_value,
            }
        canonical = tagged_jcs(PRIVACY_TAG, rec)
        prev_value = hmac_sha256_hex(chain_secret_hex, canonical)
        derived.append({"canonical": canonical.decode("utf-8"), "record_digest_hex": prev_value})
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
    cls = ijson_class(vector_bytes(case["input"]))
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


def check_digest_class(name: str, case: dict) -> int:
    inp = case["input"]
    content = inp["content"]
    cbytes = content_bytes(content)
    required = inp["required_class"]
    subs = case.get("cases")
    if subs is None:
        subs = [
            {
                "name": None,
                "offered": inp.get("offered"),
                "object_secret_hex": inp.get("object_secret_hex"),
                "key_ref": inp.get("key_ref"),
                "expected": case["expected"],
            }
        ]
    count = 0
    for sub in subs:
        cname = name + (f"/{sub['name']}" if sub.get("name") else "")
        disclosure = sub.get("disclosure", inp.get("disclosure"))
        recipients = sub.get("recipients", inp.get("recipients"))
        exp = sub["expected"]
        if exp["accepted"]:
            ref = build_digest_ref(required, cbytes, sub.get("object_secret_hex"), sub.get("key_ref"))
            ok, reason = evaluate_offer(required, content["kind"], ref, disclosure, recipients)
            expect_eq(cname, "acceptance", (ok, reason), (True, None))
            expect_eq(cname, "digest_ref", ref, exp["digest_ref"])
            if "canonical" in exp:
                expect_eq(cname, "canonical", tagged_jcs(content["type_tag"], content["object"]).decode("utf-8"), exp["canonical"])
            if required == "disclosed_party":
                expect_eq(cname, "external_copy_obligation", True, exp.get("external_copy_obligation"))
        else:
            offered = sub["offered"]
            ok, reason = evaluate_offer(required, content["kind"], offered, disclosure, recipients)
            expect_eq(cname, "acceptance", ok, False)
            expect_eq(cname, "rejection reason", reason, exp["reason"])
            # Internal consistency: the offered value must be arithmetically
            # correct so the rejection is proven to be typing-only.
            if isinstance(offered, str):
                expect_eq(cname, "offered value (untyped)", offered, sha256_hex(cbytes))
            elif "value_hex" in offered:
                if offered.get("class") == "local_erasure_safe":
                    secret = sub.get("object_secret_hex") or offered.get("object_secret_hex")
                    expect_eq(cname, "offered value (hmac)", offered["value_hex"], hmac_sha256_hex(secret, cbytes))
                else:
                    expect_eq(cname, "offered value (sha256)", offered["value_hex"], sha256_hex(cbytes))
        count += 1
    return count


def check_privacy(name: str, case: dict) -> int:
    inp, exp = case["input"], case["expected"]
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
