#!/usr/bin/env node
// BPA-1 independent evaluator (DESIGN.md 10.5; ADR-0001 accepted).
// Node builtins only (node:fs, node:crypto) - no npm dependencies.
//
// This is the SECOND, independent implementation of the BPA-1 contract
// frozen by ADR-0001 and documented in policy/eval.py's module docstring:
// same closed twelve-domain AST, same fixed validation order and JSON
// pointers, same comparability pre-pass, same deny-wins decide, same
// canonical form (set members sorted by UTF-16 code units, rules sorted by
// JCS bytes, duplicates removed), same bpa1-policy-v1 tagged digest.
// It shares no code with eval.py; conformance/run.py and run-checks.sh hold
// both to every golden vector and to a seeded differential run.
//
// TOTALITY: every entry point returns {ok: true, ...} or the typed
// rejection {ok: false, error: {kind: "malformed"|"overflow"|"incomparable",
// where: <JSON pointer or domain>}}. No exception escapes on any I-JSON
// input; a malformed AST fails closed at the first offending location in
// the fixed validation order. Exact integer arithmetic uses BigInt where a
// product could exceed 2^53 (rate refill cross-multiplication).
//
// CLI:
//   node policy/eval.mjs check spec/vectors/policy   # self-check vs vectors
//   node policy/eval.mjs batch < cases.json          # JSON array in/out

import { readFileSync, readdirSync, writeSync } from "node:fs";
import { createHash } from "node:crypto";
import { join } from "node:path";

const SAFE_MAX = Number.MAX_SAFE_INTEGER; // 2^53 - 1
const MAX_RULES = 256;
const MAX_SET = 256;
const MAX_SEGMENTS = 64;
const DIGEST_DOMAIN = "bpa1-policy-v1";

const DOMAINS = [
  "operation", "object", "path", "network_destination", "binding",
  "purpose", "classification", "time", "quantity", "rate", "assurance",
  "schema_evidence",
];

const RE_IDENTIFIER = /^[\x21-\x7e]{1,128}$/;
const RE_OP_ID = /^[a-z][a-z0-9_]{0,127}$/;
const RE_SQID = /^[\x21-\x39\x3b-\x7e]{1,64}:[\x21-\x7e]{1,63}$/;
const RE_ALABEL =
  /^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?(\.[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?)*$/;
const RE_SCHEME = /^[a-z][a-z0-9+.-]{0,31}$/;
const RE_DIM = /^[a-z][a-z0-9_]{0,63}$/;
const RE_CCY = /^[A-Z]{3}$/;
const RE_HEX64 = /^[0-9a-f]{64}$/;

const DIGEST_CLASSES = new Set([
  "structural_public", "portable_public", "local_erasure_safe",
  "scope_erasure_safe", "disclosed_party", "ciphertext_public",
]);
const DIGEST_PUBLIC = new Set([
  "structural_public", "portable_public", "disclosed_party",
  "ciphertext_public",
]);

class PolicyErr extends Error {
  constructor(kind, where) {
    super(`${kind} at ${where}`);
    this.kind = kind;
    this.where = where;
  }
}

const fail = (kind, where) => ({ ok: false, error: { kind, where } });
const isObj = (v) => v !== null && typeof v === "object" && !Array.isArray(v);

// ------------------------------------------------------------------ JCS ----
// Default JS string comparison is by UTF-16 code units - exactly RFC 8785
// member ordering - so plain sort() is the canonical order.

function jcsString(s) {
  const esc = { 8: "\\b", 9: "\\t", 10: "\\n", 12: "\\f", 13: "\\r",
                34: '\\"', 92: "\\\\" };
  let out = '"';
  for (const ch of s) {
    const cp = ch.codePointAt(0);
    if (cp < 0x20) {
      out += esc[cp] ?? "\\u" + cp.toString(16).padStart(4, "0");
    } else if (cp === 34 || cp === 92) {
      out += esc[cp];
    } else {
      out += ch;
    }
  }
  return out + '"';
}

function jcs(value) {
  if (value === null) return "null";
  if (value === true) return "true";
  if (value === false) return "false";
  if (typeof value === "number") {
    if (!Number.isInteger(value) || Math.abs(value) > SAFE_MAX) {
      throw new TypeError("non-canonical number");
    }
    return String(value); // safe integers never use exponent form
  }
  if (typeof value === "string") return jcsString(value);
  if (Array.isArray(value)) return "[" + value.map(jcs).join(",") + "]";
  if (isObj(value)) {
    return "{" + Object.keys(value).sort().map(
      (k) => jcsString(k) + ":" + jcs(value[k])).join(",") + "}";
  }
  throw new TypeError("unsupported type in canonical value");
}

// ---------------------------------------------------------- validation -----

const ptrEscape = (k) => k.replaceAll("~", "~0").replaceAll("/", "~1");

function needObj(v, where) {
  if (!isObj(v)) throw new PolicyErr("malformed", where);
  return v;
}

function needKeys(v, where, keys) {
  for (const k of keys) {
    if (!(k in v)) throw new PolicyErr("malformed", `${where}/${k}`);
  }
}

function noExtras(v, where, allowed) {
  const extras = Object.keys(v).filter((k) => !allowed.has(k)).sort();
  if (extras.length) {
    throw new PolicyErr("malformed", `${where}/${ptrEscape(extras[0])}`);
  }
}

function needInt(v, where, lo, hi) {
  // Post-parse numeric equivalence: JSON.parse cannot preserve a "1.0"
  // spelling, so only genuinely non-integral numbers are rejected here.
  if (typeof v !== "number" || !Number.isInteger(v) || v < lo || v > hi) {
    throw new PolicyErr("malformed", where);
  }
  return v;
}

function needStr(v, where, re) {
  if (typeof v !== "string" || !re.test(v)) {
    throw new PolicyErr("malformed", where);
  }
  return v;
}

function hasUnpairedSurrogate(s) {
  for (let i = 0; i < s.length; i++) {
    const u = s.charCodeAt(i);
    if (u >= 0xd800 && u <= 0xdbff) {
      const next = s.charCodeAt(i + 1);
      if (next >= 0xdc00 && next <= 0xdfff) i++;
      else return true;
    } else if (u >= 0xdc00 && u <= 0xdfff) {
      return true;
    }
  }
  return false;
}

function needSegment(v, where) {
  if (typeof v !== "string" || hasUnpairedSurrogate(v)) {
    throw new PolicyErr("malformed", where);
  }
  if (v.length < 1 || v.length > 255 || v === "." || v === "..") {
    throw new PolicyErr("malformed", where);
  }
  for (let i = 0; i < v.length; i++) {
    const u = v.charCodeAt(i);
    if (u === 0x2f || u < 0x20) throw new PolicyErr("malformed", where);
  }
  if (v.normalize("NFC") !== v) throw new PolicyErr("malformed", where);
  return v;
}

function needIdArray(v, where, re, maxItems) {
  if (!Array.isArray(v)) throw new PolicyErr("malformed", where);
  if (v.length > maxItems) throw new PolicyErr("overflow", where);
  const out = v.map((s, j) => needStr(s, `${where}/${j}`, re));
  if (new Set(out).size !== out.length) {
    throw new PolicyErr("malformed", where);
  }
  return out;
}

function needDigestRef(v, where) {
  needObj(v, where);
  needKeys(v, where, ["class", "algorithm", "value_hex"]);
  noExtras(v, where, new Set(["class", "algorithm", "key_ref", "value_hex"]));
  if (!DIGEST_CLASSES.has(v.class)) {
    throw new PolicyErr("malformed", `${where}/class`);
  }
  if (v.algorithm !== "sha-256" && v.algorithm !== "hmac-sha-256") {
    throw new PolicyErr("malformed", `${where}/algorithm`);
  }
  needStr(v.value_hex, `${where}/value_hex`, RE_HEX64);
  const out = { class: v.class, algorithm: v.algorithm,
                value_hex: v.value_hex };
  if (DIGEST_PUBLIC.has(v.class)) {
    if (v.algorithm !== "sha-256") {
      throw new PolicyErr("malformed", `${where}/algorithm`);
    }
    if ("key_ref" in v) throw new PolicyErr("malformed", `${where}/key_ref`);
  } else {
    if (v.algorithm !== "hmac-sha-256") {
      throw new PolicyErr("malformed", `${where}/algorithm`);
    }
    if (!("key_ref" in v)) {
      throw new PolicyErr("malformed", `${where}/key_ref`);
    }
    out.key_ref = needStr(v.key_ref, `${where}/key_ref`, RE_IDENTIFIER);
  }
  return out;
}

// atom validators, one per closed domain -----------------------------------

const vIdSet = (re) => (atom, w) => {
  needObj(atom, w);
  needKeys(atom, w, ["ids"]);
  noExtras(atom, w, new Set(["ids"]));
  return { ids: needIdArray(atom.ids, `${w}/ids`, re, MAX_SET) };
};

function vPath(atom, w) {
  needObj(atom, w);
  needKeys(atom, w, ["root", "segments", "match"]);
  noExtras(atom, w, new Set(["root", "segments", "match"]));
  const root = needStr(atom.root, `${w}/root`, RE_SQID);
  if (!Array.isArray(atom.segments)) {
    throw new PolicyErr("malformed", `${w}/segments`);
  }
  if (atom.segments.length > MAX_SEGMENTS) {
    throw new PolicyErr("overflow", `${w}/segments`);
  }
  const segments = atom.segments.map(
    (s, j) => needSegment(s, `${w}/segments/${j}`));
  if (atom.match !== "exact" && atom.match !== "subtree") {
    throw new PolicyErr("malformed", `${w}/match`);
  }
  return { root, segments, match: atom.match };
}

function vCidr(v, w, ncomp, compBits, maxPrefix, memberKey) {
  needObj(v, w);
  needKeys(v, w, [memberKey, "prefix_len"]);
  noExtras(v, w, new Set([memberKey, "prefix_len"]));
  const raw = v[memberKey];
  if (!Array.isArray(raw) || raw.length !== ncomp) {
    throw new PolicyErr("malformed", `${w}/${memberKey}`);
  }
  const comps = raw.map(
    (c, j) => needInt(c, `${w}/${memberKey}/${j}`, 0, (1 << compBits) - 1));
  const prefix = needInt(v.prefix_len, `${w}/prefix_len`, 0, maxPrefix);
  comps.forEach((c, j) => { // normalized: host bits below the prefix are 0
    const covered = Math.min(Math.max(prefix - j * compBits, 0), compBits);
    if (c & ((1 << (compBits - covered)) - 1)) {
      throw new PolicyErr("malformed", w);
    }
  });
  return { [memberKey]: comps, prefix_len: prefix };
}

function vHostAtom(host, w) {
  needObj(host, w);
  const keys = Object.keys(host);
  if (keys.length === 1 && keys[0] === "dns") {
    if (typeof host.dns !== "string" || host.dns.length > 253
        || !RE_ALABEL.test(host.dns)) {
      throw new PolicyErr("malformed", `${w}/dns`);
    }
    return { dns: host.dns };
  }
  if (keys.length === 1 && keys[0] === "ip4_cidr") {
    return { ip4_cidr: vCidr(host.ip4_cidr, `${w}/ip4_cidr`, 4, 8, 32,
                             "octets") };
  }
  if (keys.length === 1 && keys[0] === "ip6_cidr") {
    return { ip6_cidr: vCidr(host.ip6_cidr, `${w}/ip6_cidr`, 8, 16, 128,
                             "groups") };
  }
  throw new PolicyErr("malformed", w);
}

function vPorts(v, w) {
  needObj(v, w);
  needKeys(v, w, ["first", "last"]);
  noExtras(v, w, new Set(["first", "last"]));
  const first = needInt(v.first, `${w}/first`, 0, 65535);
  const last = needInt(v.last, `${w}/last`, 0, 65535);
  if (first > last) throw new PolicyErr("malformed", `${w}/last`);
  return { first, last };
}

function vNetwork(atom, w) {
  needObj(atom, w);
  needKeys(atom, w, ["scheme", "host", "ports", "protocol"]);
  noExtras(atom, w, new Set(["scheme", "host", "ports", "protocol"]));
  return {
    scheme: needStr(atom.scheme, `${w}/scheme`, RE_SCHEME),
    host: vHostAtom(atom.host, `${w}/host`),
    ports: vPorts(atom.ports, `${w}/ports`),
    protocol: needStr(atom.protocol, `${w}/protocol`, RE_SCHEME),
  };
}

function vPurpose(atom, w) {
  needObj(atom, w);
  needKeys(atom, w, ["snapshot", "path"]);
  noExtras(atom, w, new Set(["snapshot", "path"]));
  const snapshot = needDigestRef(atom.snapshot, `${w}/snapshot`);
  if (!Array.isArray(atom.path) || atom.path.length < 1) {
    throw new PolicyErr("malformed", `${w}/path`);
  }
  if (atom.path.length > MAX_SEGMENTS) {
    throw new PolicyErr("overflow", `${w}/path`);
  }
  const path = atom.path.map(
    (s, j) => needStr(s, `${w}/path/${j}`, RE_IDENTIFIER));
  if (new Set(path).size !== path.length) {
    throw new PolicyErr("malformed", `${w}/path`);
  }
  return { snapshot, path };
}

function vClassification(atom, w) {
  needObj(atom, w);
  needKeys(atom, w, ["lattice", "allowed"]);
  noExtras(atom, w, new Set(["lattice", "allowed"]));
  return {
    lattice: needDigestRef(atom.lattice, `${w}/lattice`),
    allowed: needIdArray(atom.allowed, `${w}/allowed`, RE_IDENTIFIER,
                         MAX_SET),
  };
}

function vTime(atom, w) {
  needObj(atom, w);
  needKeys(atom, w, ["not_before", "not_after"]);
  noExtras(atom, w, new Set(["not_before", "not_after"]));
  const nb = needInt(atom.not_before, `${w}/not_before`, 0, SAFE_MAX);
  const na = needInt(atom.not_after, `${w}/not_after`, 0, SAFE_MAX);
  if (nb > na) throw new PolicyErr("malformed", `${w}/not_after`);
  return { not_before: nb, not_after: na };
}

function vQuantityShape(atom, w, valueKey) {
  needObj(atom, w);
  needKeys(atom, w, ["dimension", "canonical_unit", "scale", valueKey]);
  noExtras(atom, w, new Set(["dimension", "canonical_unit", "scale",
                             valueKey, "currency", "pricing_revision"]));
  const out = {
    dimension: needStr(atom.dimension, `${w}/dimension`, RE_DIM),
    canonical_unit: needStr(atom.canonical_unit, `${w}/canonical_unit`,
                            RE_DIM),
    scale: needInt(atom.scale, `${w}/scale`, 0, 12),
    [valueKey]: needInt(atom[valueKey], `${w}/${valueKey}`, 0, SAFE_MAX),
  };
  if (out.dimension === "money") {
    if (!("currency" in atom)) {
      throw new PolicyErr("malformed", `${w}/currency`);
    }
    out.currency = needStr(atom.currency, `${w}/currency`, RE_CCY);
    if (!("pricing_revision" in atom)) {
      throw new PolicyErr("malformed", `${w}/pricing_revision`);
    }
    out.pricing_revision = needStr(atom.pricing_revision,
                                   `${w}/pricing_revision`, RE_IDENTIFIER);
  } else {
    if ("currency" in atom) {
      throw new PolicyErr("malformed", `${w}/currency`);
    }
    if ("pricing_revision" in atom) {
      throw new PolicyErr("malformed", `${w}/pricing_revision`);
    }
  }
  return out;
}

const RATE_FIELDS = ["dimension", "canonical_unit", "capacity",
                     "refill_amount", "refill_period_milliseconds",
                     "max_burst", "epoch", "clock"];

function vRate(atom, w) {
  needObj(atom, w);
  needKeys(atom, w, RATE_FIELDS);
  noExtras(atom, w, new Set(RATE_FIELDS));
  const out = {
    dimension: needStr(atom.dimension, `${w}/dimension`, RE_DIM),
    canonical_unit: needStr(atom.canonical_unit, `${w}/canonical_unit`,
                            RE_DIM),
    capacity: needInt(atom.capacity, `${w}/capacity`, 0, SAFE_MAX),
    refill_amount: needInt(atom.refill_amount, `${w}/refill_amount`,
                           0, SAFE_MAX),
    refill_period_milliseconds: needInt(
      atom.refill_period_milliseconds, `${w}/refill_period_milliseconds`,
      1, SAFE_MAX),
    max_burst: needInt(atom.max_burst, `${w}/max_burst`, 0, SAFE_MAX),
    epoch: needStr(atom.epoch, `${w}/epoch`, RE_IDENTIFIER),
  };
  if (atom.clock !== "authority_server") {
    throw new PolicyErr("malformed", `${w}/clock`);
  }
  out.clock = "authority_server";
  return out;
}

function vAssurance(atom, w) {
  needObj(atom, w);
  needKeys(atom, w, ["order", "admitted"]);
  noExtras(atom, w, new Set(["order", "admitted"]));
  return {
    order: needDigestRef(atom.order, `${w}/order`),
    admitted: needIdArray(atom.admitted, `${w}/admitted`, RE_IDENTIFIER,
                          MAX_SET),
  };
}

const EVIDENCE_FIELDS = ["schema", "verifier", "attestor",
                         "assurance_policy"];

function vSchemaEvidence(atom, w) {
  needObj(atom, w);
  needKeys(atom, w, EVIDENCE_FIELDS);
  noExtras(atom, w, new Set(EVIDENCE_FIELDS));
  const out = {};
  for (const k of EVIDENCE_FIELDS) {
    out[k] = needDigestRef(atom[k], `${w}/${k}`);
  }
  return out;
}

const ATOM_VALIDATORS = {
  operation: vIdSet(RE_OP_ID),
  object: vIdSet(RE_SQID),
  path: vPath,
  network_destination: vNetwork,
  binding: vIdSet(RE_SQID),
  purpose: vPurpose,
  classification: vClassification,
  time: vTime,
  quantity: (a, w) => vQuantityShape(a, w, "max"),
  rate: vRate,
  assurance: vAssurance,
  schema_evidence: vSchemaEvidence,
};

const DOMAIN_SET = new Set(DOMAINS);

function validatePolicy(p, base = "") {
  needObj(p, base);
  needKeys(p, base, ["rules"]);
  noExtras(p, base, new Set(["rules"]));
  if (!Array.isArray(p.rules)) throw new PolicyErr("malformed", `${base}/rules`);
  if (p.rules.length > MAX_RULES) {
    throw new PolicyErr("overflow", `${base}/rules`);
  }
  const rules = p.rules.map((r, i) => {
    const w = `${base}/rules/${i}`;
    needObj(r, w);
    needKeys(r, w, ["effect", "atoms"]);
    noExtras(r, w, new Set(["effect", "atoms"]));
    if (r.effect !== "allow" && r.effect !== "deny") {
      throw new PolicyErr("malformed", `${w}/effect`);
    }
    needObj(r.atoms, `${w}/atoms`);
    noExtras(r.atoms, `${w}/atoms`, DOMAIN_SET);
    const atoms = {};
    for (const d of DOMAINS) {
      if (d in r.atoms) {
        atoms[d] = ATOM_VALIDATORS[d](r.atoms[d], `${w}/atoms/${d}`);
      }
    }
    return { effect: r.effect, atoms };
  });
  return { rules };
}

// request points ------------------------------------------------------------

function vPointPath(v, w) {
  needObj(v, w);
  needKeys(v, w, ["root", "segments"]);
  noExtras(v, w, new Set(["root", "segments"]));
  const root = needStr(v.root, `${w}/root`, RE_SQID);
  if (!Array.isArray(v.segments)) {
    throw new PolicyErr("malformed", `${w}/segments`);
  }
  if (v.segments.length > MAX_SEGMENTS) {
    throw new PolicyErr("overflow", `${w}/segments`);
  }
  return { root,
           segments: v.segments.map(
             (s, j) => needSegment(s, `${w}/segments/${j}`)) };
}

function vPointHost(host, w) {
  needObj(host, w);
  const keys = Object.keys(host);
  if (keys.length === 1 && keys[0] === "dns") {
    if (typeof host.dns !== "string" || host.dns.length > 253
        || !RE_ALABEL.test(host.dns)) {
      throw new PolicyErr("malformed", `${w}/dns`);
    }
    return { dns: host.dns };
  }
  if (keys.length === 1 && keys[0] === "ip4") {
    if (!Array.isArray(host.ip4) || host.ip4.length !== 4) {
      throw new PolicyErr("malformed", `${w}/ip4`);
    }
    return { ip4: host.ip4.map((c, j) => needInt(c, `${w}/ip4/${j}`, 0, 255)) };
  }
  if (keys.length === 1 && keys[0] === "ip6") {
    if (!Array.isArray(host.ip6) || host.ip6.length !== 8) {
      throw new PolicyErr("malformed", `${w}/ip6`);
    }
    return { ip6: host.ip6.map(
      (c, j) => needInt(c, `${w}/ip6/${j}`, 0, 65535)) };
  }
  throw new PolicyErr("malformed", w);
}

function vPointNetwork(v, w) {
  needObj(v, w);
  needKeys(v, w, ["scheme", "host", "port", "protocol"]);
  noExtras(v, w, new Set(["scheme", "host", "port", "protocol"]));
  return {
    scheme: needStr(v.scheme, `${w}/scheme`, RE_SCHEME),
    host: vPointHost(v.host, `${w}/host`),
    port: needInt(v.port, `${w}/port`, 0, 65535),
    protocol: needStr(v.protocol, `${w}/protocol`, RE_SCHEME),
  };
}

const POINT_VALIDATORS = {
  operation: (v, w) => needStr(v, w, RE_OP_ID),
  object: (v, w) => needStr(v, w, RE_SQID),
  path: vPointPath,
  network_destination: vPointNetwork,
  binding: (v, w) => needStr(v, w, RE_SQID),
  purpose: vPurpose,
  classification: (v, w) => {
    needObj(v, w);
    needKeys(v, w, ["lattice", "element"]);
    noExtras(v, w, new Set(["lattice", "element"]));
    return { lattice: needDigestRef(v.lattice, `${w}/lattice`),
             element: needStr(v.element, `${w}/element`, RE_IDENTIFIER) };
  },
  time: (v, w) => {
    needObj(v, w);
    needKeys(v, w, ["at"]);
    noExtras(v, w, new Set(["at"]));
    return { at: needInt(v.at, `${w}/at`, 0, SAFE_MAX) };
  },
  quantity: (v, w) => vQuantityShape(v, w, "amount"),
  rate: vRate,
  assurance: (v, w) => {
    needObj(v, w);
    needKeys(v, w, ["order", "profile"]);
    noExtras(v, w, new Set(["order", "profile"]));
    return { order: needDigestRef(v.order, `${w}/order`),
             profile: needStr(v.profile, `${w}/profile`, RE_IDENTIFIER) };
  },
  schema_evidence: vSchemaEvidence,
};

function validateRequest(req, base) {
  needObj(req, base);
  noExtras(req, base, DOMAIN_SET);
  const out = {};
  for (const d of DOMAINS) {
    if (d in req) out[d] = POINT_VALIDATORS[d](req[d], `${base}/${d}`);
  }
  return out;
}

// ---------------------------------------------------------- the algebra ----

const hostKind = (host) => ("dns" in host ? "dns" : "ip");

function comparableAtoms(domain, a, b) {
  switch (domain) {
    case "purpose": return jcs(a.snapshot) === jcs(b.snapshot);
    case "classification": return jcs(a.lattice) === jcs(b.lattice);
    case "assurance": return jcs(a.order) === jcs(b.order);
    case "quantity":
      if (a.dimension !== b.dimension
          || a.canonical_unit !== b.canonical_unit || a.scale !== b.scale) {
        return false;
      }
      return a.dimension !== "money"
        || (a.currency === b.currency
            && a.pricing_revision === b.pricing_revision);
    case "rate":
      return a.dimension === b.dimension
        && a.canonical_unit === b.canonical_unit && a.epoch === b.epoch;
    case "network_destination":
      return hostKind(a.host) === hostKind(b.host);
    default:
      return true;
  }
}

const prefixLe = (prefix, full) =>
  prefix.length <= full.length
  && prefix.every((seg, i) => full[i] === seg);

function cidrCovers(p, c, memberKey, compBits) {
  if (c.prefix_len < p.prefix_len) return false;
  return p[memberKey].every((pc, j) => {
    const covered = Math.min(Math.max(p.prefix_len - j * compBits, 0),
                             compBits);
    const mask = ((1 << covered) - 1) << (compBits - covered);
    return ((pc ^ c[memberKey][j]) & mask) === 0;
  });
}

function hostSubset(c, p) {
  if ("dns" in c && "dns" in p) return c.dns === p.dns;
  if ("ip4_cidr" in c && "ip4_cidr" in p) {
    return cidrCovers(p.ip4_cidr, c.ip4_cidr, "octets", 8);
  }
  if ("ip6_cidr" in c && "ip6_cidr" in p) {
    return cidrCovers(p.ip6_cidr, c.ip6_cidr, "groups", 16);
  }
  return false; // ip4 vs ip6: decidably disjoint
}

// 10.5 rate containment; BigInt keeps the cross-multiplication exact.
const rateContained = (c, p) =>
  c.capacity <= p.capacity && c.max_burst <= p.max_burst
  && BigInt(c.refill_amount) * BigInt(p.refill_period_milliseconds)
     <= BigInt(p.refill_amount) * BigInt(c.refill_period_milliseconds);

const setLe = (c, p) => {
  const ps = new Set(p);
  return c.every((x) => ps.has(x));
};

function atomSubset(domain, c, p) {
  switch (domain) {
    case "operation": case "object": case "binding":
      return setLe(c.ids, p.ids);
    case "path":
      if (c.root !== p.root) return false;
      if (p.match === "subtree") return prefixLe(p.segments, c.segments);
      return c.match === "exact"
        && jcs(c.segments) === jcs(p.segments);
    case "network_destination":
      return c.scheme === p.scheme && c.protocol === p.protocol
        && p.ports.first <= c.ports.first && c.ports.last <= p.ports.last
        && hostSubset(c.host, p.host);
    case "purpose": return prefixLe(p.path, c.path);
    case "classification": return setLe(c.allowed, p.allowed);
    case "time":
      return p.not_before <= c.not_before && c.not_after <= p.not_after;
    case "quantity": return c.max <= p.max;
    case "rate": return rateContained(c, p);
    case "assurance": return setLe(c.admitted, p.admitted);
    default: // schema_evidence
      return EVIDENCE_FIELDS.every((k) => jcs(c[k]) === jcs(p[k]));
  }
}

const sortedSet = (values) => [...new Set(values)].sort();

function atomIntersect(domain, a, b) {
  switch (domain) {
    case "operation": case "object": case "binding": {
      const bs = new Set(b.ids);
      const ids = sortedSet(a.ids.filter((x) => bs.has(x)));
      return ids.length ? { ids } : null;
    }
    case "path": {
      if (a.root !== b.root) return null;
      if (a.match === "exact" && b.match === "exact") {
        return jcs(a.segments) === jcs(b.segments) ? { ...a } : null;
      }
      if (a.match === "exact") {
        return prefixLe(b.segments, a.segments) ? { ...a } : null;
      }
      if (b.match === "exact") {
        return prefixLe(a.segments, b.segments) ? { ...b } : null;
      }
      if (prefixLe(a.segments, b.segments)) return { ...b };
      if (prefixLe(b.segments, a.segments)) return { ...a };
      return null;
    }
    case "network_destination": {
      if (a.scheme !== b.scheme || a.protocol !== b.protocol) return null;
      const first = Math.max(a.ports.first, b.ports.first);
      const last = Math.min(a.ports.last, b.ports.last);
      if (first > last) return null;
      let host;
      if (hostSubset(a.host, b.host)) host = a.host;
      else if (hostSubset(b.host, a.host)) host = b.host;
      else return null;
      return { scheme: a.scheme, host: { ...host },
               ports: { first, last }, protocol: a.protocol };
    }
    case "purpose":
      if (prefixLe(a.path, b.path)) {
        return { snapshot: { ...a.snapshot }, path: [...b.path] };
      }
      if (prefixLe(b.path, a.path)) {
        return { snapshot: { ...a.snapshot }, path: [...a.path] };
      }
      return null;
    case "classification": {
      const bs = new Set(b.allowed);
      const allowed = sortedSet(a.allowed.filter((x) => bs.has(x)));
      return allowed.length
        ? { lattice: { ...a.lattice }, allowed } : null;
    }
    case "time": {
      const nb = Math.max(a.not_before, b.not_before);
      const na = Math.min(a.not_after, b.not_after);
      return nb <= na ? { not_before: nb, not_after: na } : null;
    }
    case "quantity":
      return { ...a, max: Math.min(a.max, b.max) };
    case "rate": {
      const ra = BigInt(a.refill_amount)
        * BigInt(b.refill_period_milliseconds);
      const rb = BigInt(b.refill_amount)
        * BigInt(a.refill_period_milliseconds);
      let refill;
      if (ra < rb) refill = a;
      else if (rb < ra) refill = b;
      else {
        refill = a.refill_period_milliseconds
          >= b.refill_period_milliseconds ? a : b;
      }
      return {
        dimension: a.dimension,
        canonical_unit: a.canonical_unit,
        capacity: Math.min(a.capacity, b.capacity),
        refill_amount: refill.refill_amount,
        refill_period_milliseconds: refill.refill_period_milliseconds,
        max_burst: Math.min(a.max_burst, b.max_burst),
        epoch: a.epoch,
        clock: "authority_server",
      };
    }
    case "assurance": {
      const bs = new Set(b.admitted);
      const admitted = sortedSet(a.admitted.filter((x) => bs.has(x)));
      return admitted.length ? { order: { ...a.order }, admitted } : null;
    }
    default: { // schema_evidence
      if (EVIDENCE_FIELDS.every((k) => jcs(a[k]) === jcs(b[k]))) {
        const out = {};
        for (const k of EVIDENCE_FIELDS) out[k] = { ...a[k] };
        return out;
      }
      return null;
    }
  }
}

function memberOf(domain, point, atom) {
  switch (domain) {
    case "operation": case "object": case "binding":
      return atom.ids.includes(point);
    case "path":
      if (point.root !== atom.root) return false;
      if (atom.match === "exact") {
        return jcs(point.segments) === jcs(atom.segments);
      }
      return prefixLe(atom.segments, point.segments);
    case "network_destination": {
      if (point.scheme !== atom.scheme || point.protocol !== atom.protocol
          || point.port < atom.ports.first || point.port > atom.ports.last) {
        return false;
      }
      const host = atom.host;
      if ("dns" in host) return point.host.dns === host.dns;
      if ("ip4_cidr" in host) {
        return "ip4" in point.host && cidrCovers(
          host.ip4_cidr, { octets: point.host.ip4, prefix_len: 32 },
          "octets", 8);
      }
      return "ip6" in point.host && cidrCovers(
        host.ip6_cidr, { groups: point.host.ip6, prefix_len: 128 },
        "groups", 16);
    }
    case "purpose": return prefixLe(atom.path, point.path);
    case "classification": return atom.allowed.includes(point.element);
    case "time":
      return atom.not_before <= point.at && point.at <= atom.not_after;
    case "quantity": return point.amount <= atom.max;
    case "rate": return rateContained(point, atom);
    case "assurance": return atom.admitted.includes(point.profile);
    default: // schema_evidence
      return EVIDENCE_FIELDS.every((k) => jcs(point[k]) === jcs(atom[k]));
  }
}

// ------------------------------------------------------- canonical form ----

function canonicalize(policy) {
  const rules = policy.rules.map((r) => {
    const atoms = {};
    for (const d of Object.keys(r.atoms)) {
      const atom = { ...r.atoms[d] };
      for (const key of ["ids", "allowed", "admitted"]) {
        if (key in atom) atom[key] = [...atom[key]].sort();
      }
      atoms[d] = atom;
    }
    return { effect: r.effect, atoms };
  });
  const keyed = rules.map((r) => [jcs(r), r]);
  keyed.sort((x, y) => (x[0] < y[0] ? -1 : x[0] > y[0] ? 1 : 0));
  const out = [];
  let seen = null;
  for (const [k, r] of keyed) {
    if (k !== seen) { out.push(r); seen = k; }
  }
  return { rules: out };
}

function canonicalBytes(policy) {
  const canon = canonicalize(policy);
  const plain = jcs(canon);
  const tagged = jcs({ ...canon, $domain: DIGEST_DOMAIN });
  const digest = createHash("sha256").update(tagged, "utf8").digest("hex");
  return { canon, plain, tagged, digest };
}

// ---------------------------------------------------------- operations -----

function comparabilityPrepass(rulesA, rulesB) {
  for (const ra of rulesA) {
    for (const rb of rulesB) {
      for (const d of DOMAINS) {
        if (d in ra.atoms && d in rb.atoms
            && !comparableAtoms(d, ra.atoms[d], rb.atoms[d])) {
          throw new PolicyErr("incomparable", d);
        }
      }
    }
  }
}

function opWellFormed(policy) {
  try {
    validatePolicy(policy);
  } catch (e) {
    if (e instanceof PolicyErr) return fail(e.kind, e.where);
    throw e;
  }
  return { ok: true };
}

function opCanonical(policy) {
  let p;
  try {
    p = validatePolicy(policy);
  } catch (e) {
    if (e instanceof PolicyErr) return fail(e.kind, e.where);
    throw e;
  }
  const { plain, tagged, digest } = canonicalBytes(p);
  return { ok: true, canonical: plain, tagged_canonical: tagged,
           sha256_hex: digest };
}

function opIntersect(a, b) {
  let pa, pb;
  try {
    pa = validatePolicy(a, "/a");
    pb = validatePolicy(b, "/b");
    comparabilityPrepass(pa.rules, pb.rules);
  } catch (e) {
    if (e instanceof PolicyErr) return fail(e.kind, e.where);
    throw e;
  }
  const merged = [];
  for (const ra of pa.rules) {
    if (ra.effect !== "allow") continue;
    for (const rb of pb.rules) {
      if (rb.effect !== "allow") continue;
      const atoms = {};
      let empty = false;
      for (const d of DOMAINS) {
        const inA = d in ra.atoms;
        const inB = d in rb.atoms;
        if (inA && inB) {
          const meet = atomIntersect(d, ra.atoms[d], rb.atoms[d]);
          if (meet === null) { empty = true; break; }
          atoms[d] = meet;
        } else if (inA) {
          atoms[d] = ra.atoms[d];
        } else if (inB) {
          atoms[d] = rb.atoms[d];
        }
      }
      if (!empty) merged.push({ effect: "allow", atoms });
    }
  }
  for (const p of [pa, pb]) {
    for (const r of p.rules) if (r.effect === "deny") merged.push(r);
  }
  const canon = canonicalize({ rules: merged });
  if (canon.rules.length > MAX_RULES) return fail("overflow", "/rules");
  return { ok: true, value: canon, canonical: jcs(canon) };
}

function opIsSubset(child, parent) {
  let pc, pp;
  try {
    pc = validatePolicy(child, "/child");
    pp = validatePolicy(parent, "/parent");
    comparabilityPrepass(pc.rules, pp.rules);
  } catch (e) {
    if (e instanceof PolicyErr) return fail(e.kind, e.where);
    throw e;
  }
  const cAllow = pc.rules.filter((r) => r.effect === "allow");
  const cDeny = pc.rules.filter((r) => r.effect === "deny");
  const pAllow = pp.rules.filter((r) => r.effect === "allow");
  const pDeny = pp.rules.filter((r) => r.effect === "deny");

  // region(inner) subseteq region(outer): every domain the outer rule
  // constrains must be constrained at least as tightly by the inner rule
  // (absence is wider - DESIGN.md 10.2 / gap note G33).
  const covers = (outer, inner) => DOMAINS.every((d) => {
    if (!(d in outer.atoms)) return true;
    return d in inner.atoms && atomSubset(d, inner.atoms[d], outer.atoms[d]);
  });

  const overlaps = (r1, r2) => DOMAINS.every((d) => {
    if (!(d in r1.atoms) || !(d in r2.atoms)) return true;
    return atomIntersect(d, r1.atoms[d], r2.atoms[d]) !== null;
  });

  for (const rc of cAllow) {
    if (!pAllow.some((rp) => covers(rp, rc))) {
      return { ok: true, subset: false };
    }
  }
  for (const rd of pDeny) {
    if (!cAllow.some((rc) => overlaps(rd, rc))) continue; // inapplicable
    if (!cDeny.some((rd2) => covers(rd2, rd))) {
      return { ok: true, subset: false };
    }
  }
  return { ok: true, subset: true };
}

function opDecide(policy, request) {
  let p, req;
  try {
    p = validatePolicy(policy, "/policy");
    req = validateRequest(request, "/request");
    for (const r of p.rules) {
      for (const d of DOMAINS) {
        if (d in r.atoms && d in req
            && !comparableAtoms(d, r.atoms[d], req[d])) {
          throw new PolicyErr("incomparable", d);
        }
      }
    }
  } catch (e) {
    if (e instanceof PolicyErr) return fail(e.kind, e.where);
    throw e;
  }
  let matchedAllow = 0;
  let matchedDeny = 0;
  for (const r of p.rules) {
    const ds = Object.keys(r.atoms);
    if (r.effect === "allow") {
      if (ds.every((d) => d in req && memberOf(d, req[d], r.atoms[d]))) {
        matchedAllow++;
      }
    } else if (ds.every(
        (d) => !(d in req) || memberOf(d, req[d], r.atoms[d]))) {
      matchedDeny++; // a deny conservatively matches an absent domain
    }
  }
  const decision = matchedDeny > 0 || matchedAllow === 0 ? "deny" : "allow";
  return { ok: true, decision,
           matched_allow: matchedAllow, matched_deny: matchedDeny };
}

export function runCase(c) {
  const op = isObj(c) ? c.policy_op : undefined;
  if (op === "well_formed") return opWellFormed(c.policy);
  if (op === "canonical") return opCanonical(c.policy);
  if (op === "intersect") return opIntersect(c.a, c.b);
  if (op === "is_subset") return opIsSubset(c.child, c.parent);
  if (op === "decide") return opDecide(c.policy, c.request);
  return fail("malformed", "/policy_op");
}

// ------------------------------------------------------------------ CLI ----

function walkJson(dir) {
  const out = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, entry.name);
    if (entry.isDirectory()) out.push(...walkJson(p));
    else if (entry.name.endsWith(".json")) out.push(p);
  }
  return out.sort();
}

function cmdCheck(dir) {
  let checked = 0;
  let failures = 0;
  for (const path of walkJson(dir)) {
    const vector = JSON.parse(readFileSync(path, "utf8"));
    const inp = vector.input ?? {};
    if (!("policy_op" in inp)) continue;
    const result = runCase(inp);
    checked++;
    if (jcs(result) !== jcs(vector.expected?.result ?? null)) {
      failures++;
      console.log(`FAIL  ${path}\n      derived:  ${jcs(result)}\n`
                  + `      expected: ${jcs(vector.expected?.result ?? null)}`);
    }
  }
  console.log(`eval.mjs: ${checked - failures}/${checked} policy vectors agree`);
  return failures || !checked ? 1 : 0;
}

function cmdBatch() {
  const cases = JSON.parse(readFileSync(0, "utf8"));
  // Synchronous write: process.exit must never truncate the batch reply.
  writeSync(1, JSON.stringify(cases.map(runCase)) + "\n");
  return 0;
}

const mode = process.argv[2];
if (mode === "batch") {
  process.exit(cmdBatch());
} else if (mode === "check" && process.argv[3]) {
  process.exit(cmdCheck(process.argv[3]));
} else if (process.argv[1] && import.meta.url.endsWith("eval.mjs")) {
  console.log("usage: eval.mjs batch < cases.json | eval.mjs check <dir>");
  process.exit(2);
}
