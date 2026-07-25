#!/usr/bin/env node
// @ts-check
/**
 * Independent TypeScript-family rederiver for the byom-hosted family vectors
 * (C1). JSDoc-typed plain ES modules; zero runtime dependencies; Node >= 20.
 *
 * Walks family-vectors/ (one directory per family, one JSON file per case:
 * {name, description, input, expected}) and re-derives every `expected`.
 * The derivations are implemented from PROFILE.md in this directory -- not
 * ported from xcheck.py (the Python rederiver) or any other implementation.
 * Exits nonzero on any mismatch and on an empty vector tree.
 *
 * Run: node family-vectors/tscheck/check.mjs [root]
 */

import { createHash, createHmac } from "node:crypto";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

/** @type {string[]} */
const FAILURES = [];

/**
 * @param {string} name
 * @param {string} message
 */
function fail(name, message) {
  FAILURES.push(`${name}: ${message}`);
}

/**
 * Structural equality over the JSON value space (objects compare by key set,
 * not key order).
 * @param {unknown} a
 * @param {unknown} b
 * @returns {boolean}
 */
function deepEqual(a, b) {
  if (a === b) return true;
  if (a === null || b === null || typeof a !== "object" || typeof b !== "object") return false;
  const aArr = Array.isArray(a);
  if (aArr !== Array.isArray(b)) return false;
  if (aArr) {
    const bl = /** @type {unknown[]} */ (b);
    return /** @type {unknown[]} */ (a).length === bl.length &&
      /** @type {unknown[]} */ (a).every((x, i) => deepEqual(x, bl[i]));
  }
  const ao = /** @type {Record<string, unknown>} */ (a);
  const bo = /** @type {Record<string, unknown>} */ (b);
  const keys = Object.keys(ao);
  return keys.length === Object.keys(bo).length &&
    keys.every((k) => Object.hasOwn(bo, k) && deepEqual(ao[k], bo[k]));
}

/**
 * @param {string} name
 * @param {string} what
 * @param {unknown} actual
 * @param {unknown} expected
 */
function expectEq(name, what, actual, expected) {
  if (!deepEqual(actual, expected)) {
    fail(name, `${what} differs\n  actual:   ${JSON.stringify(actual)}\n  expected: ${JSON.stringify(expected)}`);
  }
}

// ---------------------------------------------------------------------------
// RFC 8785 JCS and byom type tags (PROFILE.md section 2)
// ---------------------------------------------------------------------------

/** @type {Record<number, string>} */
const JCS_SHORT_ESCAPES = {
  0x08: "\\b",
  0x09: "\\t",
  0x0a: "\\n",
  0x0c: "\\f",
  0x0d: "\\r",
  0x22: '\\"',
  0x5c: "\\\\",
};

/**
 * JCS string form: short escapes plus \u00xx for remaining C0 controls,
 * everything else literal (UTF-8 once encoded).
 * @param {string} s
 * @returns {string}
 */
function jcsString(s) {
  let out = '"';
  for (let i = 0; i < s.length; i++) {
    const u = s.charCodeAt(i);
    const short = JCS_SHORT_ESCAPES[u];
    if (short !== undefined) out += short;
    else if (u < 0x20) out += "\\u" + u.toString(16).padStart(4, "0");
    else out += s[i];
  }
  return out + '"';
}

/**
 * RFC 8785 serialization of an I-JSON value. Two properties come for free in
 * ECMAScript, which RFC 8785 is defined against: `String(number)` IS the
 * required Number::toString(10) minimal form (10.0 -> "10", -0 -> "0",
 * 1e-7 -> "1e-7", 1e21 -> "1e+21"), and the default Array.prototype.sort()
 * compares UTF-16 code units, which is exactly the required key order.
 * @param {unknown} v
 * @returns {string}
 */
function jcsSerialize(v) {
  if (v === null) return "null";
  const t = typeof v;
  if (t === "boolean") return v ? "true" : "false";
  if (t === "string") return jcsString(/** @type {string} */ (v));
  if (t === "number") {
    const n = /** @type {number} */ (v);
    if (!Number.isFinite(n)) throw new Error("non-finite number in JCS input");
    return String(n);
  }
  if (Array.isArray(v)) return "[" + v.map(jcsSerialize).join(",") + "]";
  if (t === "object") {
    const obj = /** @type {Record<string, unknown>} */ (v);
    const keys = Object.keys(obj).sort();
    return "{" + keys.map((k) => jcsString(k) + ":" + jcsSerialize(obj[k])).join(",") + "}";
  }
  throw new Error(`unsupported value of type ${t} in JCS input`);
}

/**
 * @param {unknown} value
 * @returns {Buffer} canonical UTF-8 bytes
 */
function jcs(value) {
  return Buffer.from(jcsSerialize(value), "utf8");
}

/**
 * Byom type-tagged canonical bytes: inject the reserved `$domain` member at
 * the top level, then JCS. An object that already carries `$domain` fails
 * closed (PROFILE.md section 2).
 * @param {string} tag
 * @param {Record<string, unknown>} obj
 * @returns {Buffer}
 */
function taggedJcs(tag, obj) {
  if (obj === null || typeof obj !== "object" || Array.isArray(obj)) {
    throw new Error("type-tagged canonicalization requires an object");
  }
  if (Object.hasOwn(obj, "$domain")) throw new Error("object already carries a $domain member");
  return jcs({ ...obj, $domain: tag });
}

/** @param {Buffer} data */
function sha256Hex(data) {
  return createHash("sha256").update(data).digest("hex");
}

/**
 * @param {string} secretHex
 * @param {Buffer} data
 */
function hmacSha256Hex(secretHex, data) {
  return createHmac("sha256", Buffer.from(secretHex, "hex")).update(data).digest("hex");
}

// ---------------------------------------------------------------------------
// Strict I-JSON acceptance (PROFILE.md section 1)
// ---------------------------------------------------------------------------

const REQUEST_CAP = 256 * 1024; // bytes
const RESPONSE_CAP = 1024 * 1024; // bytes (vectors select it with input.context === "response")
const DEPTH_CAP = 64; // nested containers
const NODE_CAP = 65536; // JSON values per document
const SAFE_MAX = 9007199254740991; // 2^53 - 1
const SAFE_MAX_BIG = 9007199254740991n;

/** A profile I-JSON rejection carrying its error class. */
class IJsonError extends Error {
  /** @param {string} cls */
  constructor(cls) {
    super(cls);
    this.cls = cls;
  }
}

const STRICT_UTF8 = new TextDecoder("utf-8", { fatal: true });

/**
 * Single-pass validating scanner for exactly one strict JSON text. Iterative
 * (explicit container stack, no recursion), so pathological nesting inside
 * the 256 KiB cap can never overflow the call stack; the stack length is the
 * container depth. Values are never materialized -- the scanner only counts
 * nodes, tracks depth, records decoded-string surrogate health, and enforces
 * the token-order error classes of PROFILE.md section 1 order 3:
 * `syntax`, `trailing-data`, `duplicate`, `reserved-domain-collision`,
 * `unsafe-integer`, `non-finite`, `unsafe-number`.
 * @param {string} text
 * @returns {{ nodes: number, maxDepth: number, loneSurrogate: boolean }}
 */
function scanOneJsonText(text) {
  let pos = 0;
  let nodes = 0;
  let maxDepth = 0;
  let loneSurrogate = false;
  /** @type {{ keys: Set<string> | null }[]} open containers; keys null for arrays */
  const stack = [];

  const syntax = () => {
    throw new IJsonError("syntax");
  };
  /** @param {string | undefined} c */
  const isDigit = (c) => c !== undefined && c >= "0" && c <= "9";
  const skipWs = () => {
    while (pos < text.length) {
      const c = text[pos];
      if (c !== " " && c !== "\t" && c !== "\n" && c !== "\r") break;
      pos++;
    }
  };

  /** Scan a string token at `pos` (opening quote), decoding escapes. */
  const scanString = () => {
    pos++; // opening quote
    let out = "";
    for (;;) {
      if (pos >= text.length) syntax();
      const u = text.charCodeAt(pos);
      if (u === 0x22) {
        pos++;
        break;
      }
      if (u === 0x5c) {
        pos++;
        if (pos >= text.length) syntax();
        const e = text[pos];
        if (e === '"' || e === "\\" || e === "/") out += e;
        else if (e === "b") out += "\b";
        else if (e === "f") out += "\f";
        else if (e === "n") out += "\n";
        else if (e === "r") out += "\r";
        else if (e === "t") out += "\t";
        else if (e === "u") {
          const hex = text.slice(pos + 1, pos + 5);
          if (!/^[0-9a-fA-F]{4}$/.test(hex)) syntax();
          out += String.fromCharCode(parseInt(hex, 16));
          pos += 4;
        } else syntax();
        pos++;
      } else if (u < 0x20) {
        syntax(); // raw control character in a string
      } else {
        out += text[pos];
        pos++;
      }
    }
    // Surrogate health after escape decoding (raw text is already valid
    // UTF-8, so unpaired halves can only arrive via \uXXXX escapes). The
    // profile reports this as its own ordered check (order 4), so only a
    // flag is recorded here.
    for (let i = 0; i < out.length; i++) {
      const u = out.charCodeAt(i);
      if (u >= 0xd800 && u <= 0xdbff) {
        const next = i + 1 < out.length ? out.charCodeAt(i + 1) : 0;
        if (next >= 0xdc00 && next <= 0xdfff) i++;
        else loneSurrogate = true;
      } else if (u >= 0xdc00 && u <= 0xdfff) {
        loneSurrogate = true;
      }
    }
    return out;
  };

  /** Scan a number token at `pos` ('-' or digit) and classify it. */
  const scanNumber = () => {
    const start = pos;
    if (text[pos] === "-") {
      pos++;
      // json's -Infinity spelling is the non-finite class, not a syntax error
      if (text.startsWith("Infinity", pos)) throw new IJsonError("non-finite");
    }
    if (text[pos] === "0") pos++;
    else if (isDigit(text[pos])) {
      while (isDigit(text[pos])) pos++;
    } else syntax();
    let isFloat = false;
    if (text[pos] === ".") {
      isFloat = true;
      pos++;
      if (!isDigit(text[pos])) syntax();
      while (isDigit(text[pos])) pos++;
    }
    if (text[pos] === "e" || text[pos] === "E") {
      isFloat = true;
      pos++;
      if (text[pos] === "+" || text[pos] === "-") pos++;
      if (!isDigit(text[pos])) syntax();
      while (isDigit(text[pos])) pos++;
    }
    const token = text.slice(start, pos);
    if (!isFloat) {
      // Exact magnitude check on the token, immune to double rounding.
      const big = BigInt(token);
      if (big > SAFE_MAX_BIG || big < -SAFE_MAX_BIG) throw new IJsonError("unsafe-integer");
    } else {
      const v = Number(token);
      if (!Number.isFinite(v)) throw new IJsonError("unsafe-number");
      if (Number.isInteger(v) && Math.abs(v) > SAFE_MAX) throw new IJsonError("unsafe-number");
    }
  };

  const VALUE = 0; // a value is required
  const VALUE_OR_CLOSE = 1; // just after '[': a value or ']'
  const KEY_OR_CLOSE = 2; // just after '{': a key or '}'
  const KEY = 3; // after ',' in an object: a key
  const COLON = 4;
  const COMMA_OR_CLOSE = 5; // after a completed member/element
  let state = VALUE;
  let done = false;

  const afterValue = () => {
    if (stack.length === 0) done = true;
    else state = COMMA_OR_CLOSE;
  };

  while (!done) {
    skipWs();
    if (pos >= text.length) syntax();
    const ch = text[pos];
    switch (state) {
      case VALUE:
      case VALUE_OR_CLOSE: {
        if (state === VALUE_OR_CLOSE && ch === "]") {
          pos++;
          stack.pop();
          afterValue();
          break;
        }
        if (ch === "{") {
          pos++;
          nodes++;
          stack.push({ keys: new Set() });
          if (stack.length > maxDepth) maxDepth = stack.length;
          state = KEY_OR_CLOSE;
        } else if (ch === "[") {
          pos++;
          nodes++;
          stack.push({ keys: null });
          if (stack.length > maxDepth) maxDepth = stack.length;
          state = VALUE_OR_CLOSE;
        } else if (ch === '"') {
          scanString();
          nodes++;
          afterValue();
        } else if (ch === "-" || isDigit(ch)) {
          scanNumber();
          nodes++;
          afterValue();
        } else if (text.startsWith("true", pos)) {
          pos += 4;
          nodes++;
          afterValue();
        } else if (text.startsWith("false", pos)) {
          pos += 5;
          nodes++;
          afterValue();
        } else if (text.startsWith("null", pos)) {
          pos += 4;
          nodes++;
          afterValue();
        } else if (text.startsWith("NaN", pos) || text.startsWith("Infinity", pos)) {
          throw new IJsonError("non-finite");
        } else syntax();
        break;
      }
      case KEY_OR_CLOSE:
        if (ch === "}") {
          pos++;
          stack.pop();
          afterValue();
          break;
        }
      // fall through: a key is required
      case KEY: {
        if (ch !== '"') syntax();
        // Member names in token order, compared after escape decoding
        // (RFC 7493). The reserved-name check precedes the duplicate check
        // for the same token: `$domain` is canonicalization-reserved
        // (PROFILE.md section 2) and never a wire member at any depth.
        const key = scanString();
        if (key === "$domain") throw new IJsonError("reserved-domain-collision");
        const keys = /** @type {Set<string>} */ (stack[stack.length - 1].keys);
        if (keys.has(key)) throw new IJsonError("duplicate");
        keys.add(key);
        state = COLON;
        break;
      }
      case COLON:
        if (ch !== ":") syntax();
        pos++;
        state = VALUE;
        break;
      case COMMA_OR_CLOSE: {
        const top = stack[stack.length - 1];
        if (ch === ",") {
          pos++;
          state = top.keys !== null ? KEY : VALUE;
        } else if (ch === (top.keys !== null ? "}" : "]")) {
          pos++;
          stack.pop();
          afterValue();
        } else syntax();
        break;
      }
    }
  }
  skipWs();
  if (pos < text.length) throw new IJsonError("trailing-data"); // exactly one JSON text
  return { nodes, maxDepth, loneSurrogate };
}

/**
 * Classify body bytes per the profile check order (section 1): size cap
 * (256 KiB for requests, 1 MiB for responses), UTF-8, single parse
 * (token-order classes), surrogates, depth, node count. Returns null when
 * acceptable, else the error class.
 * @param {Buffer} data
 * @param {string} [context] "request" (default) or "response"
 * @returns {string | null}
 */
function ijsonClass(data, context) {
  if (data.length > (context === "response" ? RESPONSE_CAP : REQUEST_CAP)) return "oversize";
  let text;
  try {
    text = STRICT_UTF8.decode(data);
  } catch {
    return "invalid-utf8";
  }
  let scan;
  try {
    scan = scanOneJsonText(text);
  } catch (e) {
    if (e instanceof IJsonError) return e.cls;
    throw e;
  }
  if (scan.loneSurrogate) return "unpaired-surrogate";
  if (scan.maxDepth > DEPTH_CAP) return "over-depth";
  if (scan.nodes > NODE_CAP) return "over-nodes";
  return null;
}

/**
 * Input bytes of an ijson vector (PROFILE.md section 8 conventions).
 * @param {Record<string, any>} input
 * @returns {Buffer}
 */
function vectorBytes(input) {
  if (Object.hasOwn(input, "json_utf8")) return Buffer.from(input.json_utf8, "utf8");
  if (Object.hasOwn(input, "json_base64")) return Buffer.from(input.json_base64, "base64");
  if (Object.hasOwn(input, "json_synth")) {
    const s = input.json_synth;
    return Buffer.from((s.prefix ?? "") + (s.repeat ?? "").repeat(s.count ?? 0) + (s.suffix ?? ""), "utf8");
  }
  throw new Error("no input bytes in vector");
}

// ---------------------------------------------------------------------------
// Idempotency-domain derivations (PROFILE.md sections 4 and 5)
// ---------------------------------------------------------------------------

const KOVEE_COD_DOMAIN = "dev.kovee.canonical-object-digest.v1";
const KOVEE_TBD_DOMAIN = "dev.kovee.typed-bytes-digest.v1";
const BYOM_IDEMPOTENCY_TAG = "bpp-idempotency-domain-v1";

/**
 * @param {string} objectKind
 * @param {string} schemaRef
 * @param {Record<string, unknown>} projection
 * @returns {Buffer}
 */
function koveeCanonicalObject(objectKind, schemaRef, projection) {
  return jcs({
    $domain: KOVEE_COD_DOMAIN,
    protocol_major: 0,
    object_kind: objectKind,
    schema_ref: schemaRef,
    projection,
  });
}

/**
 * frame(x) = uint64_be(len(x)) || x
 * @param {Buffer} b
 * @returns {Buffer}
 */
function frame(b) {
  const len = Buffer.alloc(8);
  len.writeBigUInt64BE(BigInt(b.length));
  return Buffer.concat([len, b]);
}

/**
 * @param {string} domain
 * @param {string} mediaOrSchemaRef
 * @param {Buffer} data
 * @returns {string}
 */
function koveeTypedBytesDigest(domain, mediaOrSchemaRef, data) {
  return sha256Hex(
    Buffer.concat([
      frame(Buffer.from(KOVEE_TBD_DOMAIN, "utf8")),
      frame(Buffer.from(domain, "utf8")),
      frame(Buffer.from("0", "utf8")),
      frame(Buffer.from(mediaOrSchemaRef, "utf8")),
      frame(data),
    ]),
  );
}

/**
 * DSSE PAE: "DSSEv1 " len(type) " " type " " len(payload) " " payload
 * (lengths are byte counts, rendered in decimal ASCII).
 * @param {string} payloadType
 * @param {Buffer} payload
 * @returns {Buffer}
 */
function dssePae(payloadType, payload) {
  const typeBytes = Buffer.from(payloadType, "utf8");
  return Buffer.concat([
    Buffer.from(`DSSEv1 ${typeBytes.length} `, "utf8"),
    typeBytes,
    Buffer.from(` ${payload.length} `, "utf8"),
    payload,
  ]);
}

/**
 * @param {Record<string, any>} d holder of bytes_utf8 or bytes_base64
 * @returns {Buffer}
 */
function contentData(d) {
  if (Object.hasOwn(d, "bytes_utf8")) return Buffer.from(d.bytes_utf8, "utf8");
  if (Object.hasOwn(d, "bytes_base64")) return Buffer.from(d.bytes_base64, "base64");
  throw new Error("no content bytes");
}

/**
 * One `input.derivations[]` entry; `kind` selects the construction
 * (PROFILE.md section 5 table).
 * @param {Record<string, any>} d
 * @returns {Record<string, unknown>}
 */
function deriveIdempotency(d) {
  switch (d.kind) {
    case "bpp-idempotency-domain-v1": {
      // Shared per-Society index key: a scope key, so the digest is class
      // scope_erasure_safe (D-R0-1) -- erasing the scope key erases the
      // whole index's verifiability, never one entry.
      const canonical = taggedJcs(BYOM_IDEMPOTENCY_TAG, d.domain_object);
      return {
        canonical: canonical.toString("utf8"),
        digest_ref: {
          class: "scope_erasure_safe",
          algorithm: "hmac-sha-256",
          key_ref: d.key_ref,
          value_hex: hmacSha256Hex(d.index_secret_hex, canonical),
        },
      };
    }
    case "kcp-command-idempotency": {
      /** @type {Record<string, unknown>} */
      let projection;
      if (Object.hasOwn(d, "projection")) {
        projection = d.projection;
      } else {
        projection = {};
        for (const field of d.projection_fields) {
          if (Object.hasOwn(d.raw_command, field)) projection[field] = d.raw_command[field];
        }
      }
      const canonical = koveeCanonicalObject("kcp-command-idempotency", d.schema_ref, projection);
      return { canonical: canonical.toString("utf8"), sha256_hex: sha256Hex(canonical) };
    }
    case "dev.kovee.canonical-object-digest.v1": {
      const canonical = koveeCanonicalObject(d.object_kind, d.schema_ref, d.projection);
      return { canonical: canonical.toString("utf8"), sha256_hex: sha256Hex(canonical) };
    }
    case "dev.kovee.typed-bytes-digest.v1":
      return { digest_hex: koveeTypedBytesDigest(d.domain, d.media_or_schema_ref, contentData(d)) };
    case "akson-dsse-pae": {
      const pae = dssePae(d.payload_type, Buffer.from(d.payload_utf8, "utf8"));
      return { pae_utf8: pae.toString("utf8"), sha256_hex: sha256Hex(pae) };
    }
    case "byom-tagged-structural": {
      const canonical = taggedJcs(d.type_tag, d.object);
      return {
        canonical: canonical.toString("utf8"),
        digest_ref: { class: "structural_public", algorithm: "sha-256", value_hex: sha256Hex(canonical) },
      };
    }
    default:
      throw new Error(`unknown derivation kind ${JSON.stringify(d.kind)}`);
  }
}

/**
 * @param {Record<string, any>} result
 * @returns {string}
 */
function primaryHex(result) {
  if (result.digest_ref !== undefined) return result.digest_ref.value_hex;
  return result.sha256_hex ?? result.digest_hex;
}

// ---------------------------------------------------------------------------
// RFC 9457 problem shape (PROFILE.md section 3)
// ---------------------------------------------------------------------------

const PROBLEM_TYPE_PREFIX = "https://byom.dev/problems/";

// The closed 29-kind enum of byom section 14.9 (also pinned by
// spec/schemas/bpp-failure.schema.json $defs.problemKind).
const PROBLEM_KINDS = new Set([
  "invalid",
  "unsupported_version",
  "feature_unavailable",
  "forbidden_surface",
  "forbidden",
  "not_found",
  "stale_revision",
  "stale_binding",
  "stale_assembly_epoch",
  "stale_lease",
  "idempotency_mismatch",
  "position_ineligible",
  "decision_incomplete",
  "independence_conflict",
  "authority_widening",
  "mandate_held",
  "admission_required",
  "classification_unmapped",
  "policy_conflict",
  "policy_overflow",
  "budget_exceeded",
  "effect_ambiguous",
  "authority_witness_unknown",
  "endpoint_sealed",
  "cursor_expired",
  "unavailable",
  "formation_requires_participation",
  "external_command_not_terminalizable",
  "internal",
]);

/** @param {unknown} v */
function isPlainObject(v) {
  return v !== null && typeof v === "object" && !Array.isArray(v);
}

/**
 * @param {any} env the failure envelope
 * @returns {{ valid: boolean, error: string | null }}
 */
function problemShape(env) {
  if (!isPlainObject(env) || env.outcome !== "problem") return { valid: false, error: "envelope-outcome" };
  const p = env.problem;
  if (!isPlainObject(p)) return { valid: false, error: "problem-not-object" };
  for (const member of ["type", "title", "kind"]) {
    if (!Object.hasOwn(p, member)) return { valid: false, error: `missing-${member}` };
  }
  if (typeof p.title !== "string") return { valid: false, error: "title-not-string" };
  if (typeof p.kind !== "string" || !PROBLEM_KINDS.has(p.kind)) return { valid: false, error: "unknown-kind" };
  if (p.type !== PROBLEM_TYPE_PREFIX + p.kind) return { valid: false, error: "type-kind-mismatch" };
  if (Object.hasOwn(p, "status")) {
    if (typeof p.status !== "number" || !Number.isInteger(p.status)) {
      return { valid: false, error: "status-not-integer" };
    }
    if (p.status < 400 || p.status > 599) return { valid: false, error: "status-out-of-range" };
  }
  return { valid: true, error: null }; // extension members carry no authority
}

// ---------------------------------------------------------------------------
// Digest classes (PROFILE.md section 6, D-R0-1)
// ---------------------------------------------------------------------------

const ERASABLE_KINDS = new Set(["erasable_plaintext_object", "erasable_plaintext_bytes", "erasable_index_object"]);

/** Class -> the only algorithm valid for it (PROFILE.md section 6.1). */
const DIGEST_CLASS_ALGORITHM = new Map([
  ["structural_public", "sha-256"],
  ["portable_public", "sha-256"],
  ["disclosed_party", "sha-256"],
  ["ciphertext_public", "sha-256"],
  ["local_erasure_safe", "hmac-sha-256"],
  ["scope_erasure_safe", "hmac-sha-256"],
]);

const KEYED_CLASSES = new Set(["local_erasure_safe", "scope_erasure_safe"]);

// Classes whose construction is over type-tagged canonical bytes; raw-bytes
// content under them takes the domain-separated byte preimage (section 6.4).
const CANONICAL_BYTES_CLASSES = new Set(["structural_public", "local_erasure_safe", "scope_erasure_safe"]);

const WIRE_MEMBERS = new Set(["class", "algorithm", "key_ref", "value_hex"]);

const BYOM_TBD_DOMAIN = "bpp-typed-bytes-digest-v1";

/**
 * Domain-separated preimage for raw-bytes content under a canonical-bytes
 * class (PROFILE.md section 6.4): the family typed-bytes framing rule with
 * the byom domain constant.
 * @param {string} byteDomain
 * @param {string} mediaType
 * @param {Buffer} data
 * @returns {Buffer}
 */
function byomBytePreimage(byteDomain, mediaType, data) {
  return Buffer.concat([
    frame(Buffer.from(BYOM_TBD_DOMAIN, "utf8")),
    frame(Buffer.from(byteDomain, "utf8")),
    frame(Buffer.from("0", "utf8")),
    frame(Buffer.from(mediaType, "utf8")),
    frame(data),
  ]);
}

/**
 * The bytes a digest of class `clazz` commits to for this content:
 * type-tagged canonical bytes for objects; the framed byte preimage for raw
 * bytes under canonical-bytes classes; exact raw bytes for the exact-bytes
 * classes (portable_public, disclosed_party, ciphertext_public).
 * @param {Record<string, any>} content
 * @param {string} clazz
 * @returns {Buffer}
 */
function preimageBytes(content, clazz) {
  if (Object.hasOwn(content, "object")) return taggedJcs(content.type_tag, content.object);
  const raw = contentData(content);
  if (CANONICAL_BYTES_CLASSES.has(clazz)) return byomBytePreimage(content.byte_domain, content.media_type, raw);
  return raw;
}

/**
 * Raw HMAC keys, secrets, and salts are never part of a DigestRef; the key
 * id (`key_ref`) is. Detected by name pattern rather than a fixed field
 * list: anything holding a secret, salt, or raw key.
 * @param {string} member
 * @returns {boolean}
 */
function isKeyMaterial(member) {
  return member !== "key_ref" && (/secret|salt/i.test(member) || /^(hmac_)?key(_hex)?$/i.test(member));
}

/**
 * The closed DigestRef wire shape {class, algorithm, key_ref?, value_hex}
 * (PROFILE.md section 6.3 steps 1-7), validated before any class logic.
 * @param {unknown} offered
 * @returns {string | null} null when well-formed, else the rejection reason
 */
function validateWire(offered) {
  if (!isPlainObject(offered)) return "untyped_digest_forbidden";
  const ref = /** @type {Record<string, unknown>} */ (offered);
  const members = Object.keys(ref);
  if (members.some(isKeyMaterial)) return "digest_ref_carries_key_material";
  if (members.some((m) => !WIRE_MEMBERS.has(m))) return "digest_ref_unknown_member";
  for (const member of ["class", "algorithm", "value_hex"]) {
    if (typeof ref[member] !== "string") return "digest_ref_missing_member";
  }
  const cls = /** @type {string} */ (ref.class);
  const algorithm = DIGEST_CLASS_ALGORITHM.get(cls);
  if (algorithm === undefined) return "unknown_digest_class";
  if (ref.algorithm !== algorithm) return "digest_ref_algorithm_class_mismatch";
  if (KEYED_CLASSES.has(cls)) {
    if (typeof ref.key_ref !== "string" || ref.key_ref.length === 0) return "digest_ref_key_ref_missing";
  } else if (Object.hasOwn(ref, "key_ref")) {
    return "digest_ref_key_ref_forbidden";
  }
  if (!/^[0-9a-f]{64}$/.test(/** @type {string} */ (ref.value_hex))) return "digest_ref_value_not_64_hex";
  return null;
}

/**
 * The section 6.3 acceptance rule for a digest offered where a schema field
 * requires class `required`: wire validation first, then per-class
 * construction rules, then class equality, then value re-derivation where
 * the verifier holds the material to re-derive.
 * @param {string} required
 * @param {Record<string, any>} content
 * @param {unknown} offered
 * @param {{ durable_identifier_accepted?: boolean } | undefined} disclosure
 * @param {string[] | undefined} recipients
 * @param {string | undefined} offeredSecretHex
 * @returns {{ ok: boolean, reason: string | null }}
 */
function evaluateOffer(required, content, offered, disclosure, recipients, offeredSecretHex) {
  const wire = validateWire(offered);
  if (wire !== null) return { ok: false, reason: wire };
  const ref = /** @type {Record<string, any>} */ (offered);
  const cls = /** @type {string} */ (ref.class);
  const contentKind = content.kind;
  switch (cls) {
    case "structural_public":
      if (contentKind === "authority_subject") {
        return { ok: false, reason: "authority_subject_requires_local_erasure_safe" };
      }
      if (ERASABLE_KINDS.has(contentKind)) {
        return { ok: false, reason: "public_hash_over_erasable_content_forbidden" };
      }
      if (contentKind !== "protocol_bytes") {
        return { ok: false, reason: "structural_public_requires_protocol_bytes" };
      }
      break;
    case "portable_public":
      if (disclosure?.durable_identifier_accepted !== true) {
        return { ok: false, reason: "portable_requires_durable_identifier_disclosure" };
      }
      break;
    case "local_erasure_safe":
      if (contentKind === "sealed_blob") {
        return { ok: false, reason: "sealed_blob_requires_ciphertext_public" };
      }
      break;
    case "scope_erasure_safe":
      if (contentKind === "sealed_blob") {
        return { ok: false, reason: "sealed_blob_requires_ciphertext_public" };
      }
      if (contentKind === "authority_subject") {
        // Authority subjects are strictly per-object commitments (D-R0-1).
        return { ok: false, reason: "authority_subject_requires_local_erasure_safe" };
      }
      break;
    case "disclosed_party":
      if (!Array.isArray(recipients) || recipients.length === 0) {
        return { ok: false, reason: "disclosed_party_requires_named_recipients" };
      }
      break;
    case "ciphertext_public":
      if (contentKind !== "sealed_blob") {
        return { ok: false, reason: "ciphertext_public_requires_ciphertext" };
      }
      break;
  }
  if (cls !== required) return { ok: false, reason: "digest_class_mismatch" };
  const pre = preimageBytes(content, cls);
  let expectedHex;
  if (KEYED_CLASSES.has(cls)) {
    if (offeredSecretHex === undefined) return { ok: true, reason: null }; // offline re-derivation needs the key
    expectedHex = hmacSha256Hex(offeredSecretHex, pre);
  } else {
    expectedHex = sha256Hex(pre);
  }
  if (ref.value_hex !== expectedHex) return { ok: false, reason: "digest_value_mismatch" };
  return { ok: true, reason: null };
}

// Reasons produced before the construction/class steps: the offered value is
// not required to be arithmetically meaningful for these.
const WIRE_SHAPE_REASONS = new Set([
  "digest_ref_carries_key_material",
  "digest_ref_unknown_member",
  "digest_ref_missing_member",
  "unknown_digest_class",
  "digest_ref_algorithm_class_mismatch",
  "digest_ref_key_ref_missing",
  "digest_ref_key_ref_forbidden",
  "digest_ref_value_not_64_hex",
]);

// ---------------------------------------------------------------------------
// PrivacyAccessRecord chain (PROFILE.md section 7)
// ---------------------------------------------------------------------------

const PRIVACY_TAG = "bpp-privacy-access-record-v1";

// Every preimage member of a PrivacyAccessRecord except the chain link
// (previous_access_digest, absent at genesis) and the record's own
// record_digest, which is EXCLUDED from the preimage (PROFILE.md section 7).
const PRIVACY_REQUIRED_MEMBERS = [
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
];

/** A rejected PrivacyAccessRecord chain, carrying its error identifier. */
class PrivacyChainError extends Error {
  /** @param {string} error */
  constructor(error) {
    super(error);
    this.chainError = error;
  }
}

/**
 * Each record links to the previous record's digest through a
 * scope_erasure_safe DigestRef under the chain key (D-R0-1); genesis is
 * whole-member ABSENCE of previous_access_digest, never a null-valued
 * pseudo-DigestRef. The record digest is a typed scope_erasure_safe
 * DigestRef whose value is HMAC-SHA-256 of the chain key over the tagged
 * canonical bytes of the record WITHOUT its own record_digest member.
 * @param {Record<string, any>[]} records
 * @param {string} chainSecretHex
 * @param {string} keyRef
 * @returns {{ canonical: string, record_digest: Record<string, string> }[]}
 */
function derivePrivacyChain(records, chainSecretHex, keyRef) {
  const derived = [];
  /** @type {string | null} */
  let prevValue = null;
  for (const rec of records) {
    for (const member of PRIVACY_REQUIRED_MEMBERS) {
      if (!Object.hasOwn(rec, member)) throw new PrivacyChainError(`privacy_record_missing_${member}`);
    }
    if (Object.hasOwn(rec, "record_digest")) {
      throw new PrivacyChainError("privacy_record_preimage_carries_record_digest");
    }
    const full =
      prevValue !== null && !Object.hasOwn(rec, "previous_access_digest")
        ? {
            ...rec,
            previous_access_digest: {
              class: "scope_erasure_safe",
              algorithm: "hmac-sha-256",
              key_ref: keyRef,
              value_hex: prevValue,
            },
          }
        : { ...rec };
    const canonical = taggedJcs(PRIVACY_TAG, full);
    prevValue = hmacSha256Hex(chainSecretHex, canonical);
    derived.push({
      canonical: canonical.toString("utf8"),
      record_digest: {
        class: "scope_erasure_safe",
        algorithm: "hmac-sha-256",
        key_ref: keyRef,
        value_hex: prevValue,
      },
    });
  }
  return derived;
}

/**
 * Release rule: sensitive bytes are released only when the covering record's
 * outcome is `allowed` AND the record committed to the non-rollbackable
 * access journal before release.
 * @param {string} lastOutcome
 * @param {boolean} journalCommitted
 * @returns {{ release: boolean, reason: string | null }}
 */
function privacyRelease(lastOutcome, journalCommitted) {
  if (lastOutcome !== "allowed") return { release: false, reason: "access_denied" };
  if (!journalCommitted) return { release: false, reason: "privacy_access_record_commit_failed" };
  return { release: true, reason: null };
}

// ---------------------------------------------------------------------------
// Family checkers
// ---------------------------------------------------------------------------

/** @typedef {{ name?: string, input: Record<string, any>, expected: Record<string, any>, cases?: Record<string, any>[] }} VectorCase */
/** @typedef {(name: string, kase: VectorCase) => number} Checker */

/** @type {Checker} */
function checkIjson(name, kase) {
  const cls = ijsonClass(vectorBytes(kase.input), kase.input.context ?? "request");
  expectEq(name, "validity", cls === null, kase.expected.valid);
  if (!kase.expected.valid) expectEq(name, "error class", cls, kase.expected.error);
  return 1;
}

/** @type {Checker} */
function checkJcs(name, kase) {
  const canonical = jcs(kase.input.value);
  expectEq(name, "canonical", canonical.toString("utf8"), kase.expected.canonical);
  expectEq(name, "sha256", sha256Hex(canonical), kase.expected.sha256_hex);
  return 1;
}

/** @type {Checker} */
function checkProblem(name, kase) {
  const { valid, error } = problemShape(kase.input.envelope);
  expectEq(name, "validity", valid, kase.expected.valid);
  if (!kase.expected.valid) expectEq(name, "error class", error, kase.expected.error);
  return 1;
}

/** @type {Checker} */
function checkIdempotency(name, kase) {
  const results = kase.input.derivations.map(deriveIdempotency);
  expectEq(name, "results", results, kase.expected.results);
  const relation = kase.expected.relation;
  if (relation !== undefined) {
    const hexes = results.map(primaryHex);
    const unique = new Set(hexes).size;
    if (relation === "distinct") expectEq(name, "distinct digests", unique, hexes.length);
    else if (relation === "equal") expectEq(name, "equal digests", unique, 1);
    else fail(name, `unknown relation ${JSON.stringify(relation)}`);
  }
  return 1;
}

/**
 * The secret (if any) matching the OFFERED keyed class, from the sub-case
 * first, then the vector input.
 * @param {Record<string, any>} sub
 * @param {Record<string, any>} inp
 * @returns {string | undefined}
 */
function offeredSecret(sub, inp) {
  for (const holder of [sub, inp]) {
    for (const member of ["object_secret_hex", "scope_secret_hex"]) {
      if (Object.hasOwn(holder, member)) return holder[member];
    }
  }
  return undefined;
}

/** @type {Checker} */
function checkDigestClass(name, kase) {
  const inp = kase.input;
  const content = inp.content;
  const required = inp.required_class;
  const subs = kase.cases ?? [{ name: null, offered: inp.offered, expected: kase.expected }];
  let count = 0;
  for (const sub of subs) {
    const cname = sub.name ? `${name}/${sub.name}` : name;
    const disclosure = Object.hasOwn(sub, "disclosure") ? sub.disclosure : inp.disclosure;
    const recipients = Object.hasOwn(sub, "recipients") ? sub.recipients : inp.recipients;
    const secret = offeredSecret(sub, inp);
    const offered = sub.offered;
    const exp = sub.expected;
    const verdict = evaluateOffer(required, content, offered, disclosure, recipients, secret);
    if (exp.accepted) {
      // Positive cases validate the OFFERED ref (wire, construction, class,
      // and value re-derivation) -- never synthesize-and-compare.
      expectEq(cname, "acceptance", verdict, { ok: true, reason: null });
      expectEq(cname, "digest_ref", offered, exp.digest_ref);
      if (Object.hasOwn(exp, "canonical")) {
        expectEq(cname, "canonical", taggedJcs(content.type_tag, content.object).toString("utf8"), exp.canonical);
      }
      if (required === "disclosed_party") {
        // Decision 10: disclosed_party acceptance always asserts the
        // external-copy obligation.
        expectEq(cname, "external_copy_obligation", exp.external_copy_obligation, true);
      }
    } else {
      expectEq(cname, "acceptance", verdict.ok, false);
      expectEq(cname, "rejection reason", verdict.reason, exp.reason);
      // Internal consistency: a typing-only rejection's offered value must
      // be arithmetically correct under the OFFERED class. Wire-shape
      // rejections carry no meaningful value; the digest_value_mismatch
      // negative instead proves its value is exactly the un-framed
      // raw-bytes digest.
      if (typeof offered === "string") {
        const exact = Object.hasOwn(content, "object")
          ? taggedJcs(content.type_tag, content.object)
          : contentData(content);
        expectEq(cname, "offered value (untyped)", offered, sha256Hex(exact));
      } else if (verdict.reason === "digest_value_mismatch") {
        expectEq(
          cname,
          "offered value (raw, un-framed preimage)",
          offered.value_hex,
          hmacSha256Hex(/** @type {string} */ (secret), contentData(content)),
        );
      } else if (!WIRE_SHAPE_REASONS.has(/** @type {string} */ (verdict.reason))) {
        const pre = preimageBytes(content, offered.class);
        if (KEYED_CLASSES.has(offered.class)) {
          expectEq(cname, "offered value (hmac)", offered.value_hex, hmacSha256Hex(/** @type {string} */ (secret), pre));
        } else {
          expectEq(cname, "offered value (sha-256)", offered.value_hex, sha256Hex(pre));
        }
      }
    }
    count += 1;
  }
  return count;
}

/** @type {Checker} */
function checkPrivacy(name, kase) {
  const inp = kase.input;
  const exp = kase.expected;
  if (exp.chain_valid === false) {
    try {
      derivePrivacyChain(inp.records, inp.chain_secret_hex, inp.key_ref);
      fail(name, "chain derivation unexpectedly succeeded");
    } catch (e) {
      if (!(e instanceof PrivacyChainError)) throw e;
      expectEq(name, "chain error", e.chainError, exp.error);
    }
    return 1;
  }
  const derived = derivePrivacyChain(inp.records, inp.chain_secret_hex, inp.key_ref);
  expectEq(name, "records", derived, exp.records);
  const last = inp.records[inp.records.length - 1];
  const verdict = privacyRelease(last.outcome, inp.journal_committed);
  expectEq(name, "release_permitted", verdict.release, exp.release_permitted);
  expectEq(name, "release reason", verdict.reason, exp.reason ?? null);
  return 1;
}

/** @type {Record<string, Checker>} */
const CHECKERS = {
  ijson: checkIjson,
  jcs: checkJcs,
  problem: checkProblem,
  idempotency: checkIdempotency,
  "digest-class": checkDigestClass,
  privacy: checkPrivacy,
};

// ---------------------------------------------------------------------------
// Walker
// ---------------------------------------------------------------------------

function main() {
  const here = dirname(fileURLToPath(import.meta.url));
  const root = process.argv[2] !== undefined ? resolve(process.argv[2]) : dirname(here);
  let files = 0;
  let cases = 0;
  /** @type {Map<string, number>} */
  const perFamily = new Map();
  const familyDirs = readdirSync(root, { withFileTypes: true })
    .filter((e) => e.isDirectory() && e.name !== "tscheck") // tscheck/ is this rederiver, not a family
    .map((e) => e.name)
    .sort();
  for (const family of familyDirs) {
    const checker = CHECKERS[family];
    const dir = join(root, family);
    for (const fname of readdirSync(dir).filter((f) => f.endsWith(".json")).sort()) {
      const path = join(dir, fname);
      if (checker === undefined) {
        fail(path, `no checker registered for family ${JSON.stringify(family)}`);
        continue;
      }
      /** @type {VectorCase} */
      const kase = JSON.parse(readFileSync(path, "utf8"));
      const expectedName = `${family}/${fname.slice(0, -".json".length)}`;
      if (kase.name !== expectedName) {
        fail(path, `vector name ${JSON.stringify(kase.name)} != ${JSON.stringify(expectedName)}`);
      }
      const label = kase.name ?? path;
      let n = 0;
      try {
        n = checker(label, kase);
      } catch (e) {
        // a malformed vector is a failure, not a crash
        const err = /** @type {Error} */ (e);
        fail(label, `checker raised ${err.constructor.name}: ${err.message}`);
      }
      files += 1;
      cases += n;
      perFamily.set(family, (perFamily.get(family) ?? 0) + n);
    }
  }

  if (FAILURES.length > 0) {
    console.log(`tscheck: ${FAILURES.length} failure(s) across ${files} vector file(s)`);
    for (const f of FAILURES) console.log(`  FAIL ${f}`);
    return 1;
  }
  if (files === 0) {
    console.log(`tscheck: no vectors found under ${root}`);
    return 1;
  }
  const detail = [...perFamily.entries()]
    .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
    .map(([k, v]) => `${k}=${v}`)
    .join(", ");
  console.log(`tscheck: ${files} vector files, ${cases} cases OK (${detail})`);
  return 0;
}

process.exit(main());
