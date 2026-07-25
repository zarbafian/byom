#!/usr/bin/env python3
"""BPA-1 reference evaluator (DESIGN.md §10.5; ADR-0001 accepted; Python stdlib only).

What you write — a policy value and a question about it:

    {"rules": [
      {"effect": "allow", "atoms": {"operation": {"ids": ["events_read"]}}},
      {"effect": "deny",  "atoms": {"binding":   {"ids": ["prov:acme"]}}}]}

    intersect(a, b)      -> {"ok": true, "value": <canonical policy>, "canonical": <JCS>}
    is_subset(child, p)  -> {"ok": true, "subset": true|false}
    decide(policy, req)  -> {"ok": true, "decision": "allow"|"deny",
                             "matched_allow": n, "matched_deny": n}
    canonical(policy)    -> {"ok": true, "canonical": <JCS>,
                             "tagged_canonical": <JCS with $domain>, "sha256_hex": <hex>}

TOTALITY (ADR-0001): every function is a total pure function over I-JSON input —
no I/O, no clock, no name resolution, no callbacks, and no exception escapes.
Every call returns either {"ok": true, ...} or the typed rejection
{"ok": false, "error": {"kind": "malformed"|"overflow"|"incomparable",
"where": <JSON pointer or domain>}} (§14.9 mapping: malformed -> invalid,
overflow -> policy_overflow, incomparable -> policy_conflict). A malformed AST
always fails closed with `malformed` at the first offending location in the
fixed validation order below; it is never partially evaluated.

SEMANTICS (frozen by ADR-0001, grounded in §10.5):
- A policy is a bounded set of rules; each rule {effect: allow|deny, atoms}
  constrains a subset of the twelve closed §10.5 domains (fixed table order:
  operation, object, path, network_destination, binding, purpose,
  classification, time, quantity, rate, assurance, schema_evidence). An
  omitted domain is unconstrained; an empty atoms object is the universal
  rule; an unknown domain key fails closed.
- decide: deny wins, absence of a matching allow is deny (§10.5). An allow
  rule matches only when every constrained domain is present in the request
  and its value is a member of the atom; a deny rule conservatively matches
  an absent request domain (fail closed).
- is_subset(child, parent): every child allow rule must be covered by some
  parent allow rule (for every domain the parent rule constrains, the child
  rule constrains it with an atom-level subset — an unconstrained child
  domain is wider, never narrower: §10.2 / gap note G33), and every
  applicable parent deny (one overlapping the child's allow region) must be
  preserved by a child deny at least as wide.
- intersect(a, b): pairwise product of allow rules with atom-level meets
  (an empty meet drops the pair), plus the union of both deny sets (§10.5
  deny preservation under intersection); the result is canonical.
- Incomparable values reject (§10.5): before any evaluation, every same-domain
  atom pair across the two inputs (or policy x request) is checked for
  comparability — different classification lattices, assurance orders,
  purpose snapshots, quantity dimension/unit/scale/currency/pricing revision,
  rate dimension/unit/epoch/refill-period (D-RT-4: rate claims need equal
  window boundaries — anything finer is consume-time behavior, so differing
  refill periods fail closed), or a DNS host against an IP/CIDR host reject
  with {"kind": "incomparable", "where": <domain>}. The scan order is fixed
  (first policy's rules, then second's, then the domain table order), so both
  independent evaluators report the identical first conflict.
- Canonical form: set members sorted by UTF-16 code units, rules sorted by
  their JCS UTF-8 BYTES, and duplicate rules REJECT with malformed at the
  second occurrence — never a silent dedupe (D-RT-5 / RT-08); canonical
  bytes are RFC 8785 JCS and the policy digest is SHA-256 over the
  $domain-tagged canonical form with domain "bpa1-policy-v1"
  (family-vectors/PROFILE.md §2, D-R0-1).
- Post-parse numeric equivalence: JSON parsers cannot distinguish 1.0 from 1
  in every language, so integral floats are accepted as their integer value;
  non-integral numbers are malformed (§10.5 floating-point quantities
  reject; the wire-level I-JSON profile already rejects the "1.0" spelling
  in canonical bytes).

CLI:
    python3 policy/eval.py check spec/vectors/policy   # self-check vs golden vectors
    python3 policy/eval.py batch < cases.json          # JSON array in, results out

policy/eval.mjs is the independent second implementation (same contract,
written against this docstring's rules, not this code); conformance/run.py and
run-checks.sh hold both to every vector and to a seeded differential run.
"""

from __future__ import annotations

import hashlib
import json
import re
import sys
from pathlib import Path

SAFE_MAX = 2**53 - 1
MAX_RULES = 256      # §14.9 bounded BPA policy nodes, pinned per ADR-0001
MAX_SET = 256        # §14.9 "maximum 256 list items"
MAX_SEGMENTS = 64    # bounded path / purpose depth (§14.9)
DIGEST_DOMAIN = "bpa1-policy-v1"

DOMAINS = (
    "operation", "object", "path", "network_destination", "binding",
    "purpose", "classification", "time", "quantity", "rate", "assurance",
    "schema_evidence",
)

RE_IDENTIFIER = re.compile(r"^[\x21-\x7e]{1,128}$")
RE_OP_ID = re.compile(r"^[a-z][a-z0-9_]{0,127}$")
RE_SQID = re.compile(r"^[\x21-\x39\x3b-\x7e]{1,64}:[\x21-\x7e]{1,63}$")
RE_ALABEL = re.compile(
    r"^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?(\.[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?)*$")
RE_SCHEME = re.compile(r"^[a-z][a-z0-9+.-]{0,31}$")
RE_DIM = re.compile(r"^[a-z][a-z0-9_]{0,63}$")
RE_CCY = re.compile(r"^[A-Z]{3}$")
RE_HEX64 = re.compile(r"^[0-9a-f]{64}$")

DIGEST_CLASSES = ("structural_public", "portable_public", "local_erasure_safe",
                  "scope_erasure_safe", "disclosed_party", "ciphertext_public")
DIGEST_PUBLIC = ("structural_public", "portable_public", "disclosed_party",
                 "ciphertext_public")


class PolicyError(Exception):
    """Typed rejection: kind is malformed | overflow | incomparable."""

    def __init__(self, kind: str, where: str):
        super().__init__(f"{kind} at {where!r}")
        self.kind = kind
        self.where = where


def _err(kind: str, where: str) -> dict:
    return {"ok": False, "error": {"kind": kind, "where": where}}


# ------------------------------------------------------------------- JCS ----

def _u16key(s: str) -> bytes:
    """UTF-16 code-unit sort key (RFC 8785 member ordering; surrogatepass so a
    malformed input with unpaired-surrogate keys still fails closed, never
    with an encoding exception)."""
    return s.encode("utf-16-be", "surrogatepass")


def _jcs_string(s: str) -> str:
    esc = {0x08: "\\b", 0x09: "\\t", 0x0A: "\\n", 0x0C: "\\f", 0x0D: "\\r",
           0x22: '\\"', 0x5C: "\\\\"}
    out = ['"']
    for ch in s:
        cp = ord(ch)
        if cp in esc:
            out.append(esc[cp])
        elif cp < 0x20:
            out.append("\\u%04x" % cp)
        else:
            out.append(ch)
    out.append('"')
    return "".join(out)


def jcs(value) -> str:
    """RFC 8785 JCS over the validated BPA-1 value space: null, bool, safe
    integers, strings, arrays, objects. Floats never reach this point
    (validation normalizes integral floats and rejects the rest)."""
    if value is None:
        return "null"
    if value is True:
        return "true"
    if value is False:
        return "false"
    if isinstance(value, int):
        if abs(value) > SAFE_MAX:
            raise ValueError("unsafe integer")
        return str(value)
    if isinstance(value, str):
        return _jcs_string(value)
    if isinstance(value, list):
        return "[" + ",".join(jcs(v) for v in value) + "]"
    if isinstance(value, dict):
        items = sorted(value.items(), key=lambda kv: _u16key(kv[0]))
        return "{" + ",".join(_jcs_string(k) + ":" + jcs(v)
                              for k, v in items) + "}"
    raise TypeError(f"unsupported type in canonical value: {type(value)}")


# ------------------------------------------------- validation primitives ----

def _ptr_escape(key: str) -> str:
    return key.replace("~", "~0").replace("/", "~1")


def _obj(v, where: str) -> dict:
    if not isinstance(v, dict):
        raise PolicyError("malformed", where)
    return v


def _required(v: dict, where: str, keys) -> None:
    for k in keys:
        if k not in v:
            raise PolicyError("malformed", f"{where}/{k}")


def _extras(v: dict, where: str, allowed) -> None:
    extras = sorted((k for k in v if k not in allowed), key=_u16key)
    if extras:
        raise PolicyError("malformed", f"{where}/{_ptr_escape(extras[0])}")


def _int(v, where: str, lo: int, hi: int) -> int:
    """Safe integer with post-parse float equivalence (see module docstring)."""
    if isinstance(v, bool):
        raise PolicyError("malformed", where)
    if isinstance(v, float):
        if not v.is_integer():
            raise PolicyError("malformed", where)
        v = int(v)
    if not isinstance(v, int) or v < lo or v > hi:
        raise PolicyError("malformed", where)
    return v


def _string(v, where: str, pattern: re.Pattern) -> str:
    if not isinstance(v, str) or not pattern.match(v):
        raise PolicyError("malformed", where)
    return v


def _u16len(s: str) -> int:
    return len(s.encode("utf-16-be", "surrogatepass")) // 2


def _has_unpaired_surrogate(s: str) -> bool:
    # Python holds json-escape-decoded unpaired surrogates as lone code
    # points in D800-DFFF; paired ones become single astral code points.
    return any(0xD800 <= ord(ch) <= 0xDFFF for ch in s)


def _segment(v, where: str) -> str:
    import unicodedata
    if not isinstance(v, str) or _has_unpaired_surrogate(v):
        raise PolicyError("malformed", where)
    n = _u16len(v)
    if n < 1 or n > 255 or v in (".", ".."):
        raise PolicyError("malformed", where)
    for ch in v:
        if ch == "/" or ord(ch) < 0x20:
            raise PolicyError("malformed", where)
    if not unicodedata.is_normalized("NFC", v):
        raise PolicyError("malformed", where)
    return v


def _id_array(v, where: str, item_re: re.Pattern, max_items: int) -> list:
    if not isinstance(v, list):
        raise PolicyError("malformed", where)
    if len(v) > max_items:
        raise PolicyError("overflow", where)
    out = [_string(s, f"{where}/{j}", item_re) for j, s in enumerate(v)]
    if len(set(out)) != len(out):
        raise PolicyError("malformed", where)
    return out


def _digest_ref(v, where: str) -> dict:
    _obj(v, where)
    _required(v, where, ("class", "algorithm", "value_hex"))
    _extras(v, where, {"class", "algorithm", "key_ref", "value_hex"})
    cls = v["class"]
    if cls not in DIGEST_CLASSES:
        raise PolicyError("malformed", f"{where}/class")
    alg = v["algorithm"]
    if alg not in ("sha-256", "hmac-sha-256"):
        raise PolicyError("malformed", f"{where}/algorithm")
    _string(v["value_hex"], f"{where}/value_hex", RE_HEX64)
    out = {"class": cls, "algorithm": alg, "value_hex": v["value_hex"]}
    if cls in DIGEST_PUBLIC:
        if alg != "sha-256":
            raise PolicyError("malformed", f"{where}/algorithm")
        if "key_ref" in v:
            raise PolicyError("malformed", f"{where}/key_ref")
    else:
        if alg != "hmac-sha-256":
            raise PolicyError("malformed", f"{where}/algorithm")
        if "key_ref" not in v:
            raise PolicyError("malformed", f"{where}/key_ref")
        out["key_ref"] = _string(v["key_ref"], f"{where}/key_ref",
                                 RE_IDENTIFIER)
    return out


# ------------------------------------------------------- atom validators ----

def _v_id_set(item_re):
    def check(atom, w):
        _obj(atom, w)
        _required(atom, w, ("ids",))
        _extras(atom, w, {"ids"})
        return {"ids": _id_array(atom["ids"], f"{w}/ids", item_re, MAX_SET)}
    return check


def _v_path(atom, w):
    _obj(atom, w)
    _required(atom, w, ("root", "segments", "match"))
    _extras(atom, w, {"root", "segments", "match"})
    root = _string(atom["root"], f"{w}/root", RE_SQID)
    segs = atom["segments"]
    if not isinstance(segs, list):
        raise PolicyError("malformed", f"{w}/segments")
    if len(segs) > MAX_SEGMENTS:
        raise PolicyError("overflow", f"{w}/segments")
    segs = [_segment(s, f"{w}/segments/{j}") for j, s in enumerate(segs)]
    match = atom["match"]
    if match not in ("exact", "subtree"):
        raise PolicyError("malformed", f"{w}/match")
    return {"root": root, "segments": segs, "match": match}


def _cidr(v, w, ncomp: int, comp_bits: int, max_prefix: int, member: str):
    _obj(v, w)
    _required(v, w, (member, "prefix_len"))
    _extras(v, w, {member, "prefix_len"})
    comps = v[member]
    if not isinstance(comps, list) or len(comps) != ncomp:
        raise PolicyError("malformed", f"{w}/{member}")
    comps = [_int(c, f"{w}/{member}/{j}", 0, (1 << comp_bits) - 1)
             for j, c in enumerate(comps)]
    prefix = _int(v["prefix_len"], f"{w}/prefix_len", 0, max_prefix)
    # Normalized CIDR: host bits below the prefix are zero (§10.5).
    for j, c in enumerate(comps):
        covered = min(max(prefix - j * comp_bits, 0), comp_bits)
        mask = (1 << (comp_bits - covered)) - 1
        if c & mask:
            raise PolicyError("malformed", w)
    return {member: comps, "prefix_len": prefix}


def _v_host_atom(host, w):
    _obj(host, w)
    keys = set(host)
    if keys == {"dns"}:
        dns = host["dns"]
        if not isinstance(dns, str) or len(dns) > 253 or not RE_ALABEL.match(dns):
            raise PolicyError("malformed", f"{w}/dns")
        return {"dns": dns}
    if keys == {"ip4_cidr"}:
        return {"ip4_cidr": _cidr(host["ip4_cidr"], f"{w}/ip4_cidr",
                                  4, 8, 32, "octets")}
    if keys == {"ip6_cidr"}:
        return {"ip6_cidr": _cidr(host["ip6_cidr"], f"{w}/ip6_cidr",
                                  8, 16, 128, "groups")}
    raise PolicyError("malformed", w)


def _v_ports(v, w):
    _obj(v, w)
    _required(v, w, ("first", "last"))
    _extras(v, w, {"first", "last"})
    first = _int(v["first"], f"{w}/first", 0, 65535)
    last = _int(v["last"], f"{w}/last", 0, 65535)
    if first > last:
        raise PolicyError("malformed", f"{w}/last")
    return {"first": first, "last": last}


def _v_network(atom, w):
    _obj(atom, w)
    _required(atom, w, ("scheme", "host", "ports", "protocol"))
    _extras(atom, w, {"scheme", "host", "ports", "protocol"})
    return {
        "scheme": _string(atom["scheme"], f"{w}/scheme", RE_SCHEME),
        "host": _v_host_atom(atom["host"], f"{w}/host"),
        "ports": _v_ports(atom["ports"], f"{w}/ports"),
        "protocol": _string(atom["protocol"], f"{w}/protocol", RE_SCHEME),
    }


def _v_purpose(atom, w):
    _obj(atom, w)
    _required(atom, w, ("snapshot", "path"))
    _extras(atom, w, {"snapshot", "path"})
    snapshot = _digest_ref(atom["snapshot"], f"{w}/snapshot")
    path = atom["path"]
    if not isinstance(path, list) or not path:
        raise PolicyError("malformed", f"{w}/path")
    if len(path) > MAX_SEGMENTS:
        raise PolicyError("overflow", f"{w}/path")
    path = [_string(s, f"{w}/path/{j}", RE_IDENTIFIER)
            for j, s in enumerate(path)]
    if len(set(path)) != len(path):
        raise PolicyError("malformed", f"{w}/path")
    return {"snapshot": snapshot, "path": path}


def _v_classification(atom, w):
    _obj(atom, w)
    _required(atom, w, ("lattice", "allowed"))
    _extras(atom, w, {"lattice", "allowed"})
    return {"lattice": _digest_ref(atom["lattice"], f"{w}/lattice"),
            "allowed": _id_array(atom["allowed"], f"{w}/allowed",
                                 RE_IDENTIFIER, MAX_SET)}


def _v_time(atom, w):
    _obj(atom, w)
    _required(atom, w, ("not_before", "not_after"))
    _extras(atom, w, {"not_before", "not_after"})
    nb = _int(atom["not_before"], f"{w}/not_before", 0, SAFE_MAX)
    na = _int(atom["not_after"], f"{w}/not_after", 0, SAFE_MAX)
    if nb > na:
        raise PolicyError("malformed", f"{w}/not_after")
    return {"not_before": nb, "not_after": na}


def _v_quantity_shape(atom, w, value_key: str):
    _obj(atom, w)
    _required(atom, w, ("dimension", "canonical_unit", "scale", value_key))
    _extras(atom, w, {"dimension", "canonical_unit", "scale", value_key,
                      "currency", "pricing_revision"})
    out = {
        "dimension": _string(atom["dimension"], f"{w}/dimension", RE_DIM),
        "canonical_unit": _string(atom["canonical_unit"],
                                  f"{w}/canonical_unit", RE_DIM),
        "scale": _int(atom["scale"], f"{w}/scale", 0, 12),
        value_key: _int(atom[value_key], f"{w}/{value_key}", 0, SAFE_MAX),
    }
    if out["dimension"] == "money":
        if "currency" not in atom:
            raise PolicyError("malformed", f"{w}/currency")
        out["currency"] = _string(atom["currency"], f"{w}/currency", RE_CCY)
        if "pricing_revision" not in atom:
            raise PolicyError("malformed", f"{w}/pricing_revision")
        out["pricing_revision"] = _string(atom["pricing_revision"],
                                          f"{w}/pricing_revision",
                                          RE_IDENTIFIER)
    else:
        if "currency" in atom:
            raise PolicyError("malformed", f"{w}/currency")
        if "pricing_revision" in atom:
            raise PolicyError("malformed", f"{w}/pricing_revision")
    return out


def _v_quantity(atom, w):
    return _v_quantity_shape(atom, w, "max")


def _v_rate(atom, w):
    _obj(atom, w)
    fields = ("dimension", "canonical_unit", "capacity", "refill_amount",
              "refill_period_milliseconds", "max_burst", "epoch", "clock")
    _required(atom, w, fields)
    _extras(atom, w, set(fields))
    out = {
        "dimension": _string(atom["dimension"], f"{w}/dimension", RE_DIM),
        "canonical_unit": _string(atom["canonical_unit"],
                                  f"{w}/canonical_unit", RE_DIM),
        "capacity": _int(atom["capacity"], f"{w}/capacity", 0, SAFE_MAX),
        "refill_amount": _int(atom["refill_amount"], f"{w}/refill_amount",
                              0, SAFE_MAX),
        "refill_period_milliseconds": _int(
            atom["refill_period_milliseconds"],
            f"{w}/refill_period_milliseconds", 1, SAFE_MAX),
        "max_burst": _int(atom["max_burst"], f"{w}/max_burst", 0, SAFE_MAX),
        "epoch": _string(atom["epoch"], f"{w}/epoch", RE_IDENTIFIER),
    }
    if atom["clock"] != "authority_server":
        raise PolicyError("malformed", f"{w}/clock")
    out["clock"] = "authority_server"
    return out


def _v_assurance(atom, w):
    _obj(atom, w)
    _required(atom, w, ("order", "admitted"))
    _extras(atom, w, {"order", "admitted"})
    return {"order": _digest_ref(atom["order"], f"{w}/order"),
            "admitted": _id_array(atom["admitted"], f"{w}/admitted",
                                  RE_IDENTIFIER, MAX_SET)}


def _v_schema_evidence(atom, w):
    _obj(atom, w)
    fields = ("schema", "verifier", "attestor", "assurance_policy")
    _required(atom, w, fields)
    _extras(atom, w, set(fields))
    return {k: _digest_ref(atom[k], f"{w}/{k}") for k in fields}


ATOM_VALIDATORS = {
    "operation": _v_id_set(RE_OP_ID),
    "object": _v_id_set(RE_SQID),
    "path": _v_path,
    "network_destination": _v_network,
    "binding": _v_id_set(RE_SQID),
    "purpose": _v_purpose,
    "classification": _v_classification,
    "time": _v_time,
    "quantity": _v_quantity,
    "rate": _v_rate,
    "assurance": _v_assurance,
    "schema_evidence": _v_schema_evidence,
}


def validate_policy(p, base: str = "") -> dict:
    """Returns the normalized policy or raises the first PolicyError in the
    fixed validation order (top-down; required fields in declaration order,
    then unknown keys in UTF-16 sorted order, then fields in declaration
    order, then cross-field rules)."""
    _obj(p, base or "")
    _required(p, base, ("rules",))
    _extras(p, base, {"rules"})
    rules = p["rules"]
    if not isinstance(rules, list):
        raise PolicyError("malformed", f"{base}/rules")
    if len(rules) > MAX_RULES:
        raise PolicyError("overflow", f"{base}/rules")
    out = []
    for i, r in enumerate(rules):
        w = f"{base}/rules/{i}"
        _obj(r, w)
        _required(r, w, ("effect", "atoms"))
        _extras(r, w, {"effect", "atoms"})
        if r["effect"] not in ("allow", "deny"):
            raise PolicyError("malformed", f"{w}/effect")
        atoms = r["atoms"]
        _obj(atoms, f"{w}/atoms")
        _extras(atoms, f"{w}/atoms", set(DOMAINS))
        natoms = {}
        for d in DOMAINS:
            if d in atoms:
                natoms[d] = ATOM_VALIDATORS[d](atoms[d], f"{w}/atoms/{d}")
        out.append({"effect": r["effect"], "atoms": natoms})
    # D-RT-5 (RT-08): duplicate rules REJECT (malformed) at the second
    # occurrence in input order — never a silent dedupe. Duplicates compare
    # in per-rule canonical form (set members sorted), the same form the
    # canonical ordering uses.
    seen = set()
    for i, r in enumerate(out):
        key = jcs(_canonical_rule(r))
        if key in seen:
            raise PolicyError("malformed", f"{base}/rules/{i}")
        seen.add(key)
    return {"rules": out}


# ---------------------------------------------------- request validators ----

def _v_point_path(v, w):
    _obj(v, w)
    _required(v, w, ("root", "segments"))
    _extras(v, w, {"root", "segments"})
    root = _string(v["root"], f"{w}/root", RE_SQID)
    segs = v["segments"]
    if not isinstance(segs, list):
        raise PolicyError("malformed", f"{w}/segments")
    if len(segs) > MAX_SEGMENTS:
        raise PolicyError("overflow", f"{w}/segments")
    return {"root": root,
            "segments": [_segment(s, f"{w}/segments/{j}")
                         for j, s in enumerate(segs)]}


def _v_point_host(host, w):
    _obj(host, w)
    keys = set(host)
    if keys == {"dns"}:
        dns = host["dns"]
        if not isinstance(dns, str) or len(dns) > 253 or not RE_ALABEL.match(dns):
            raise PolicyError("malformed", f"{w}/dns")
        return {"dns": dns}
    if keys == {"ip4"}:
        comps = host["ip4"]
        if not isinstance(comps, list) or len(comps) != 4:
            raise PolicyError("malformed", f"{w}/ip4")
        return {"ip4": [_int(c, f"{w}/ip4/{j}", 0, 255)
                        for j, c in enumerate(comps)]}
    if keys == {"ip6"}:
        comps = host["ip6"]
        if not isinstance(comps, list) or len(comps) != 8:
            raise PolicyError("malformed", f"{w}/ip6")
        return {"ip6": [_int(c, f"{w}/ip6/{j}", 0, 65535)
                        for j, c in enumerate(comps)]}
    raise PolicyError("malformed", w)


def _v_point_network(v, w):
    _obj(v, w)
    _required(v, w, ("scheme", "host", "port", "protocol"))
    _extras(v, w, {"scheme", "host", "port", "protocol"})
    return {
        "scheme": _string(v["scheme"], f"{w}/scheme", RE_SCHEME),
        "host": _v_point_host(v["host"], f"{w}/host"),
        "port": _int(v["port"], f"{w}/port", 0, 65535),
        "protocol": _string(v["protocol"], f"{w}/protocol", RE_SCHEME),
    }


def _v_point_classification(v, w):
    _obj(v, w)
    _required(v, w, ("lattice", "element"))
    _extras(v, w, {"lattice", "element"})
    return {"lattice": _digest_ref(v["lattice"], f"{w}/lattice"),
            "element": _string(v["element"], f"{w}/element", RE_IDENTIFIER)}


def _v_point_time(v, w):
    _obj(v, w)
    _required(v, w, ("at",))
    _extras(v, w, {"at"})
    return {"at": _int(v["at"], f"{w}/at", 0, SAFE_MAX)}


def _v_point_assurance(v, w):
    _obj(v, w)
    _required(v, w, ("order", "profile"))
    _extras(v, w, {"order", "profile"})
    return {"order": _digest_ref(v["order"], f"{w}/order"),
            "profile": _string(v["profile"], f"{w}/profile", RE_IDENTIFIER)}


POINT_VALIDATORS = {
    "operation": lambda v, w: _string(v, w, RE_OP_ID),
    "object": lambda v, w: _string(v, w, RE_SQID),
    "path": _v_point_path,
    "network_destination": _v_point_network,
    "binding": lambda v, w: _string(v, w, RE_SQID),
    "purpose": _v_purpose,
    "classification": _v_point_classification,
    "time": _v_point_time,
    "quantity": lambda v, w: _v_quantity_shape(v, w, "amount"),
    "rate": _v_rate,
    "assurance": _v_point_assurance,
    "schema_evidence": _v_schema_evidence,
}


def validate_request(req, base: str) -> dict:
    _obj(req, base)
    _extras(req, base, set(DOMAINS))
    return {d: POINT_VALIDATORS[d](req[d], f"{base}/{d}")
            for d in DOMAINS if d in req}


# ------------------------------------------------------------ the algebra ---

def _host_kind(host: dict) -> str:
    return "dns" if "dns" in host else "ip"


def comparable(domain: str, a: dict, b: dict) -> bool:
    """Symmetric comparability precondition (§10.5 incomparable values
    reject); b may be an atom or a request point of the same domain."""
    if domain == "purpose":
        return jcs(a["snapshot"]) == jcs(b["snapshot"])
    if domain == "classification":
        return jcs(a["lattice"]) == jcs(b["lattice"])
    if domain == "assurance":
        return jcs(a["order"]) == jcs(b["order"])
    if domain == "quantity":
        if (a["dimension"] != b["dimension"]
                or a["canonical_unit"] != b["canonical_unit"]
                or a["scale"] != b["scale"]):
            return False
        if a["dimension"] == "money":
            return (a["currency"] == b["currency"]
                    and a["pricing_revision"] == b["pricing_revision"])
        return True
    if domain == "rate":
        # D-RT-4 (RT-07): rate claims are decidable from the encoding alone
        # only under EQUAL window boundaries — same refill period on the
        # same epoch. A child 1/1ms under a parent 10/10ms refills before
        # the parent boundary, so differing periods are incomparable (fail
        # closed); boundary-finer semantics (active interval, reserved
        # share, alignment) are consume-time behavior under §10.5's atomic
        # ancestor-counter locking, never a policy-algebra claim. BPA-2 may
        # tighten.
        return (a["dimension"] == b["dimension"]
                and a["canonical_unit"] == b["canonical_unit"]
                and a["epoch"] == b["epoch"]
                and a["refill_period_milliseconds"]
                == b["refill_period_milliseconds"])
    if domain == "network_destination":
        return _host_kind(a["host"]) == _host_kind(b["host"])
    return True  # operation, object, binding, path, time, schema_evidence


def _prefix_le(prefix: list, full: list) -> bool:
    return len(prefix) <= len(full) and full[:len(prefix)] == prefix


def _cidr_covers(p: dict, c: dict, member: str, comp_bits: int) -> bool:
    """True when CIDR p covers CIDR/address c (c may have a longer prefix)."""
    if c["prefix_len"] < p["prefix_len"]:
        return False
    for j, (pc, cc) in enumerate(zip(p[member], c[member])):
        covered = min(max(p["prefix_len"] - j * comp_bits, 0), comp_bits)
        mask = ((1 << covered) - 1) << (comp_bits - covered)
        if (pc ^ cc) & mask:
            return False
    return True


def _host_subset(c: dict, p: dict) -> bool:
    if "dns" in c and "dns" in p:
        return c["dns"] == p["dns"]
    if "ip4_cidr" in c and "ip4_cidr" in p:
        return _cidr_covers(p["ip4_cidr"], c["ip4_cidr"], "octets", 8)
    if "ip6_cidr" in c and "ip6_cidr" in p:
        return _cidr_covers(p["ip6_cidr"], c["ip6_cidr"], "groups", 16)
    return False  # ip4 vs ip6: decidably disjoint


def _rate_contained(c: dict, p: dict) -> bool:
    """§10.5 rate containment under D-RT-4 (RT-07): comparable() has
    already required EQUAL window boundaries (same refill period, same
    epoch), so containment is exactly componentwise — capacity, burst, and
    refill amount. Ratio cross-multiplication is deliberately gone: it
    blessed a child that refills before the parent boundary."""
    return (c["capacity"] <= p["capacity"]
            and c["max_burst"] <= p["max_burst"]
            and c["refill_amount"] <= p["refill_amount"])


def atom_subset(domain: str, c: dict, p: dict) -> bool:
    """region(c) subseteq region(p); both atoms comparable and validated."""
    if domain in ("operation", "object", "binding"):
        return set(c["ids"]) <= set(p["ids"])
    if domain == "path":
        if c["root"] != p["root"]:
            return False
        if p["match"] == "subtree":
            return _prefix_le(p["segments"], c["segments"])
        return c["match"] == "exact" and c["segments"] == p["segments"]
    if domain == "network_destination":
        return (c["scheme"] == p["scheme"]
                and c["protocol"] == p["protocol"]
                and p["ports"]["first"] <= c["ports"]["first"]
                and c["ports"]["last"] <= p["ports"]["last"]
                and _host_subset(c["host"], p["host"]))
    if domain == "purpose":
        return _prefix_le(p["path"], c["path"])
    if domain == "classification":
        return set(c["allowed"]) <= set(p["allowed"])
    if domain == "time":
        return (p["not_before"] <= c["not_before"]
                and c["not_after"] <= p["not_after"])
    if domain == "quantity":
        return c["max"] <= p["max"]
    if domain == "rate":
        return _rate_contained(c, p)
    if domain == "assurance":
        return set(c["admitted"]) <= set(p["admitted"])
    # schema_evidence
    return all(jcs(c[k]) == jcs(p[k])
               for k in ("schema", "verifier", "attestor", "assurance_policy"))


def _sorted_set(values) -> list:
    return sorted(values, key=_u16key)


def atom_intersect(domain: str, a: dict, b: dict):
    """The atom-level meet; None when the intersection region is empty.
    Comparability is a precondition (checked by the callers' pre-pass)."""
    if domain in ("operation", "object", "binding"):
        ids = _sorted_set(set(a["ids"]) & set(b["ids"]))
        return {"ids": ids} if ids else None
    if domain == "path":
        if a["root"] != b["root"]:
            return None
        sa, sb = a["segments"], b["segments"]
        if a["match"] == "exact" and b["match"] == "exact":
            return dict(a) if sa == sb else None
        if a["match"] == "exact":
            return dict(a) if _prefix_le(sb, sa) else None
        if b["match"] == "exact":
            return dict(b) if _prefix_le(sa, sb) else None
        if _prefix_le(sa, sb):
            return dict(b)
        if _prefix_le(sb, sa):
            return dict(a)
        return None
    if domain == "network_destination":
        if a["scheme"] != b["scheme"] or a["protocol"] != b["protocol"]:
            return None
        first = max(a["ports"]["first"], b["ports"]["first"])
        last = min(a["ports"]["last"], b["ports"]["last"])
        if first > last:
            return None
        ha, hb = a["host"], b["host"]
        if _host_subset(ha, hb):
            host = ha
        elif _host_subset(hb, ha):
            host = hb
        else:
            return None
        return {"scheme": a["scheme"], "host": dict(host),
                "ports": {"first": first, "last": last},
                "protocol": a["protocol"]}
    if domain == "purpose":
        if _prefix_le(a["path"], b["path"]):
            return {"snapshot": dict(a["snapshot"]), "path": list(b["path"])}
        if _prefix_le(b["path"], a["path"]):
            return {"snapshot": dict(a["snapshot"]), "path": list(a["path"])}
        return None
    if domain == "classification":
        allowed = _sorted_set(set(a["allowed"]) & set(b["allowed"]))
        if not allowed:
            return None
        return {"lattice": dict(a["lattice"]), "allowed": allowed}
    if domain == "time":
        nb = max(a["not_before"], b["not_before"])
        na = min(a["not_after"], b["not_after"])
        return {"not_before": nb, "not_after": na} if nb <= na else None
    if domain == "quantity":
        out = dict(a)
        out["max"] = min(a["max"], b["max"])
        return out
    if domain == "rate":
        # D-RT-4: comparability already pinned equal window boundaries, so
        # the meet is componentwise on the shared period.
        return {
            "dimension": a["dimension"],
            "canonical_unit": a["canonical_unit"],
            "capacity": min(a["capacity"], b["capacity"]),
            "refill_amount": min(a["refill_amount"], b["refill_amount"]),
            "refill_period_milliseconds": a["refill_period_milliseconds"],
            "max_burst": min(a["max_burst"], b["max_burst"]),
            "epoch": a["epoch"],
            "clock": "authority_server",
        }
    if domain == "assurance":
        admitted = _sorted_set(set(a["admitted"]) & set(b["admitted"]))
        if not admitted:
            return None
        return {"order": dict(a["order"]), "admitted": admitted}
    # schema_evidence
    keys = ("schema", "verifier", "attestor", "assurance_policy")
    if all(jcs(a[k]) == jcs(b[k]) for k in keys):
        return {k: dict(a[k]) for k in keys}
    return None


def member(domain: str, point, atom: dict) -> bool:
    """Is the request point inside the atom region (comparable, validated)."""
    if domain in ("operation", "object", "binding"):
        return point in atom["ids"]
    if domain == "path":
        if point["root"] != atom["root"]:
            return False
        if atom["match"] == "exact":
            return point["segments"] == atom["segments"]
        return _prefix_le(atom["segments"], point["segments"])
    if domain == "network_destination":
        if (point["scheme"] != atom["scheme"]
                or point["protocol"] != atom["protocol"]
                or not (atom["ports"]["first"] <= point["port"]
                        <= atom["ports"]["last"])):
            return False
        host, phost = atom["host"], point["host"]
        if "dns" in host:
            return phost.get("dns") == host["dns"]
        if "ip4_cidr" in host:
            return ("ip4" in phost and _cidr_covers(
                host["ip4_cidr"],
                {"octets": phost["ip4"], "prefix_len": 32}, "octets", 8))
        return ("ip6" in phost and _cidr_covers(
            host["ip6_cidr"],
            {"groups": phost["ip6"], "prefix_len": 128}, "groups", 16))
    if domain == "purpose":
        return _prefix_le(atom["path"], point["path"])
    if domain == "classification":
        return point["element"] in atom["allowed"]
    if domain == "time":
        return atom["not_before"] <= point["at"] <= atom["not_after"]
    if domain == "quantity":
        return point["amount"] <= atom["max"]
    if domain == "rate":
        return _rate_contained(point, atom)
    if domain == "assurance":
        return point["profile"] in atom["admitted"]
    # schema_evidence
    return all(jcs(point[k]) == jcs(atom[k])
               for k in ("schema", "verifier", "attestor", "assurance_policy"))


# -------------------------------------------------------- canonical form ----

def _canonical_rule(r: dict) -> dict:
    """Per-rule canonical form: set members sorted (UTF-16 code units)."""
    atoms = {}
    for d, atom in r["atoms"].items():
        atom = dict(atom)
        for key in ("ids", "allowed", "admitted"):
            if key in atom:
                atom[key] = _sorted_set(atom[key])
        atoms[d] = atom
    return {"effect": r["effect"], "atoms": atoms}


def canonicalize(policy: dict) -> dict:
    """Canonical form: set members sorted (UTF-16 code units), rules sorted
    by their JCS UTF-8 BYTES (D-RT-5 / RT-08 — never UTF-16 string order,
    which disagrees on astral vs U+E000..U+FFFF code points). Duplicate
    INPUT rules were already rejected by validate_policy (D-RT-5); the
    dedupe below only ever folds identical DERIVED rules an intersection
    legitimately produces (e.g. one deny rule shared by both factors).
    Semantics-preserving: rule order is irrelevant to
    decide/is_subset/intersect."""
    rules = [_canonical_rule(r) for r in policy["rules"]]
    keyed = sorted(((jcs(r), r) for r in rules),
                   key=lambda kr: kr[0].encode("utf-8"))
    out, seen = [], None
    for k, r in keyed:
        if k != seen:
            out.append(r)
            seen = k
    return {"rules": out}


def canonical_bytes(policy: dict):
    canon = canonicalize(policy)
    plain = jcs(canon)
    tagged = jcs({**canon, "$domain": DIGEST_DOMAIN})
    digest = hashlib.sha256(tagged.encode("utf-8")).hexdigest()
    return canon, plain, tagged, digest


# ------------------------------------------------------------ operations ----

def _comparability_prepass(rules_a, rules_b):
    """Fixed scan order (first input's rules, second input's rules, domain
    table order): the first incomparable same-domain atom pair rejects."""
    for ra in rules_a:
        for rb in rules_b:
            for d in DOMAINS:
                if d in ra["atoms"] and d in rb["atoms"]:
                    if not comparable(d, ra["atoms"][d], rb["atoms"][d]):
                        raise PolicyError("incomparable", d)


def op_well_formed(policy) -> dict:
    try:
        validate_policy(policy)
    except PolicyError as e:
        return _err(e.kind, e.where)
    return {"ok": True}


def op_canonical(policy) -> dict:
    try:
        p = validate_policy(policy)
    except PolicyError as e:
        return _err(e.kind, e.where)
    _, plain, tagged, digest = canonical_bytes(p)
    return {"ok": True, "canonical": plain, "tagged_canonical": tagged,
            "sha256_hex": digest}


def op_intersect(a, b) -> dict:
    try:
        pa = validate_policy(a, "/a")
        pb = validate_policy(b, "/b")
        _comparability_prepass(pa["rules"], pb["rules"])
    except PolicyError as e:
        return _err(e.kind, e.where)
    merged = []
    for ra in pa["rules"]:
        if ra["effect"] != "allow":
            continue
        for rb in pb["rules"]:
            if rb["effect"] != "allow":
                continue
            atoms, empty = {}, False
            for d in DOMAINS:
                ina, inb = d in ra["atoms"], d in rb["atoms"]
                if ina and inb:
                    meet = atom_intersect(d, ra["atoms"][d], rb["atoms"][d])
                    if meet is None:
                        empty = True
                        break
                    atoms[d] = meet
                elif ina:
                    atoms[d] = ra["atoms"][d]
                elif inb:
                    atoms[d] = rb["atoms"][d]
            if not empty:
                merged.append({"effect": "allow", "atoms": atoms})
    for p in (pa, pb):
        merged.extend(r for r in p["rules"] if r["effect"] == "deny")
    canon = canonicalize({"rules": merged})
    if len(canon["rules"]) > MAX_RULES:
        return _err("overflow", "/rules")
    return {"ok": True, "value": canon, "canonical": jcs(canon)}


def op_is_subset(child, parent) -> dict:
    try:
        pc = validate_policy(child, "/child")
        pp = validate_policy(parent, "/parent")
        _comparability_prepass(pc["rules"], pp["rules"])
    except PolicyError as e:
        return _err(e.kind, e.where)
    c_allow = [r for r in pc["rules"] if r["effect"] == "allow"]
    c_deny = [r for r in pc["rules"] if r["effect"] == "deny"]
    p_allow = [r for r in pp["rules"] if r["effect"] == "allow"]
    p_deny = [r for r in pp["rules"] if r["effect"] == "deny"]

    def covers(rp, rc):
        # Every domain the parent rule constrains must be constrained at
        # least as tightly by the child rule (absence is wider — §10.2/G33).
        for d in DOMAINS:
            if d in rp["atoms"]:
                if d not in rc["atoms"]:
                    return False
                if not atom_subset(d, rc["atoms"][d], rp["atoms"][d]):
                    return False
        return True

    def overlaps(r1, r2):
        for d in DOMAINS:
            if d in r1["atoms"] and d in r2["atoms"]:
                if atom_intersect(d, r1["atoms"][d], r2["atoms"][d]) is None:
                    return False
        return True

    for rc in c_allow:
        if not any(covers(rp, rc) for rp in p_allow):
            return {"ok": True, "subset": False}
    for rd in p_deny:
        applicable = any(overlaps(rd, rc) for rc in c_allow)
        if not applicable:
            continue
        preserved = any(covers(rd2, rd) for rd2 in c_deny)
        if not preserved:
            return {"ok": True, "subset": False}
    return {"ok": True, "subset": True}


def op_decide(policy, request) -> dict:
    try:
        p = validate_policy(policy, "/policy")
        req = validate_request(request, "/request")
        for r in p["rules"]:
            for d in DOMAINS:
                if d in r["atoms"] and d in req:
                    if not comparable(d, r["atoms"][d], req[d]):
                        raise PolicyError("incomparable", d)
    except PolicyError as e:
        return _err(e.kind, e.where)
    matched_allow = matched_deny = 0
    for r in p["rules"]:
        if r["effect"] == "allow":
            if all(d in req and member(d, req[d], r["atoms"][d])
                   for d in r["atoms"]):
                matched_allow += 1
        else:
            # A deny conservatively matches an absent request domain.
            if all(d not in req or member(d, req[d], r["atoms"][d])
                   for d in r["atoms"]):
                matched_deny += 1
    decision = "deny" if matched_deny or not matched_allow else "allow"
    return {"ok": True, "decision": decision,
            "matched_allow": matched_allow, "matched_deny": matched_deny}


def run_case(case: dict) -> dict:
    """Batch/vector entry point: one {"policy_op": ..., ...} case in, one
    typed result out. Total: never raises on any I-JSON input."""
    op = case.get("policy_op") if isinstance(case, dict) else None
    if op == "well_formed":
        return op_well_formed(case.get("policy"))
    if op == "canonical":
        return op_canonical(case.get("policy"))
    if op == "intersect":
        return op_intersect(case.get("a"), case.get("b"))
    if op == "is_subset":
        return op_is_subset(case.get("child"), case.get("parent"))
    if op == "decide":
        return op_decide(case.get("policy"), case.get("request"))
    return _err("malformed", "/policy_op")


# ----------------------------------------------------------------- CLI ------

def _cmd_batch() -> int:
    cases = json.loads(sys.stdin.read())
    print(json.dumps([run_case(c) for c in cases]))
    return 0


def _cmd_check(vector_dir: Path) -> int:
    failures = checked = 0
    for path in sorted(vector_dir.rglob("*.json")):
        vector = json.loads(path.read_text(encoding="utf-8"))
        inp = vector.get("input", {})
        if "policy_op" not in inp:
            continue
        result = run_case(inp)
        expected = vector.get("expected", {}).get("result")
        checked += 1
        if jcs(result) != jcs(expected):
            failures += 1
            print(f"FAIL  {path.name}\n      derived:  {jcs(result)}\n"
                  f"      expected: {jcs(expected)}")
    print(f"eval.py: {checked - failures}/{checked} policy vectors agree")
    return 1 if failures or not checked else 0


def main(argv) -> int:
    if len(argv) >= 2 and argv[1] == "batch":
        return _cmd_batch()
    if len(argv) >= 3 and argv[1] == "check":
        return _cmd_check(Path(argv[2]))
    print(__doc__.split("\n\n")[0])
    print("usage: eval.py batch < cases.json | eval.py check <vector-dir>")
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
