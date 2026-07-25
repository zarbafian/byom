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
 * `syntax`, `trailing-data`, `duplicate`, `unsafe-integer`, `non-finite`,
 * `unsafe-number`.
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
        // Duplicate member names at any depth, compared after escape
        // decoding (RFC 7493), surfaced in token order.
        const key = scanString();
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
 * Classify request-body bytes per the profile check order (section 1):
 * size cap, UTF-8, single parse (token-order classes), surrogates, depth,
 * node count. Returns null when acceptable, else the error class.
 * @param {Buffer} data
 * @returns {string | null}
 */
function ijsonClass(data) {
  if (data.length > REQUEST_CAP) return "oversize";
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
      const canonical = taggedJcs(BYOM_IDEMPOTENCY_TAG, d.domain_object);
      return {
        canonical: canonical.toString("utf8"),
        digest_ref: {
          class: "local_erasure_safe",
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
// Digest classes (PROFILE.md section 6)
// ---------------------------------------------------------------------------

const ERASABLE_KINDS = new Set(["erasable_plaintext_object", "erasable_plaintext_bytes"]);

/**
 * The bytes a digest-class vector commits to: type-tagged canonical bytes
 * for objects, raw bytes otherwise.
 * @param {Record<string, any>} content
 * @returns {Buffer}
 */
function digestContentBytes(content) {
  if (Object.hasOwn(content, "object")) return taggedJcs(content.type_tag, content.object);
  return contentData(content);
}

/**
 * Raw HMAC keys and per-object salts are never part of a DigestRef; the key
 * id (`key_ref`) is. Detected by name pattern rather than a fixed field
 * list: anything holding a secret, salt, or raw key.
 * @param {Record<string, unknown>} ref
 * @returns {boolean}
 */
function carriesKeyMaterial(ref) {
  return Object.keys(ref).some(
    (k) => k !== "key_ref" && (/secret|salt/i.test(k) || /^(hmac_)?key(_hex)?$/i.test(k)),
  );
}

/**
 * The section 6.3 acceptance rule for a digest offered where a schema field
 * requires class `required`; construction violations are reported before the
 * generic class mismatch.
 * @param {string} required
 * @param {string} contentKind
 * @param {unknown} offered
 * @param {{ durable_identifier_accepted?: boolean } | undefined} disclosure
 * @param {string[] | undefined} recipients
 * @returns {{ ok: boolean, reason: string | null }}
 */
function evaluateOffer(required, contentKind, offered, disclosure, recipients) {
  if (!isPlainObject(offered)) return { ok: false, reason: "untyped_digest_forbidden" };
  const ref = /** @type {Record<string, unknown>} */ (offered);
  if (carriesKeyMaterial(ref)) return { ok: false, reason: "digest_ref_carries_key_material" };
  const cls = ref.class;
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
    default:
      return { ok: false, reason: "unknown_digest_class" };
  }
  if (cls !== required) return { ok: false, reason: "digest_class_mismatch" };
  return { ok: true, reason: null };
}

/**
 * A well-constructed DigestRef of the given class over the content bytes
 * (wire shape {class, algorithm, key_ref?, value_hex}, profile decision 6).
 * @param {string} clazz
 * @param {Buffer} cbytes
 * @param {string} [secretHex]
 * @param {string} [keyRef]
 * @returns {Record<string, unknown>}
 */
function buildDigestRef(clazz, cbytes, secretHex, keyRef) {
  if (clazz === "local_erasure_safe") {
    if (secretHex === undefined) throw new Error("local_erasure_safe requires an object secret");
    return { class: clazz, algorithm: "hmac-sha-256", key_ref: keyRef, value_hex: hmacSha256Hex(secretHex, cbytes) };
  }
  return { class: clazz, algorithm: "sha-256", value_hex: sha256Hex(cbytes) };
}

// ---------------------------------------------------------------------------
// PrivacyAccessRecord chain (PROFILE.md section 7)
// ---------------------------------------------------------------------------

const PRIVACY_TAG = "bpp-privacy-access-record-v1";

/**
 * Each record links to the previous record's digest through a
 * local_erasure_safe DigestRef (value_hex null at genesis); the record
 * digest is HMAC-SHA-256 of the chain secret over the tagged canonical
 * bytes.
 * @param {Record<string, any>[]} records
 * @param {string} chainSecretHex
 * @param {string} keyRef
 * @returns {{ canonical: string, record_digest_hex: string }[]}
 */
function derivePrivacyChain(records, chainSecretHex, keyRef) {
  const derived = [];
  /** @type {string | null} */
  let prevValue = null;
  for (const rec of records) {
    const full = Object.hasOwn(rec, "previous_access_digest")
      ? { ...rec }
      : {
          ...rec,
          previous_access_digest: {
            class: "local_erasure_safe",
            algorithm: "hmac-sha-256",
            key_ref: keyRef,
            value_hex: prevValue,
          },
        };
    const canonical = taggedJcs(PRIVACY_TAG, full);
    prevValue = hmacSha256Hex(chainSecretHex, canonical);
    derived.push({ canonical: canonical.toString("utf8"), record_digest_hex: prevValue });
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

/** @typedef {{ name?: string, input: Record<string, any>, expected: Record<string, any> }} VectorCase */
/** @typedef {(name: string, kase: VectorCase) => number} Checker */

/** @type {Checker} */
function checkIjson(name, kase) {
  const cls = ijsonClass(vectorBytes(kase.input));
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

/** @type {Checker} */
function checkDigestClass(name, kase) {
  const inp = kase.input;
  const content = inp.content;
  const cbytes = digestContentBytes(content);
  const required = inp.required_class;
  const exp = kase.expected;
  if (exp.accepted) {
    const ref = buildDigestRef(required, cbytes, inp.object_secret_hex, inp.key_ref);
    const verdict = evaluateOffer(required, content.kind, ref, inp.disclosure, inp.recipients);
    expectEq(name, "acceptance", verdict, { ok: true, reason: null });
    expectEq(name, "digest_ref", ref, exp.digest_ref);
    if (Object.hasOwn(exp, "canonical")) {
      expectEq(name, "canonical", taggedJcs(content.type_tag, content.object).toString("utf8"), exp.canonical);
    }
    if (required === "disclosed_party") {
      // Decision 10: disclosed_party acceptance always asserts the
      // external-copy obligation.
      expectEq(name, "external_copy_obligation", exp.external_copy_obligation, true);
    }
  } else {
    const offered = inp.offered;
    const verdict = evaluateOffer(required, content.kind, offered, inp.disclosure, inp.recipients);
    expectEq(name, "acceptance", verdict.ok, false);
    expectEq(name, "rejection reason", verdict.reason, exp.reason);
    // Internal consistency: every negative's offered value must be
    // arithmetically correct, proving the rejection is typing-only.
    if (typeof offered === "string") {
      expectEq(name, "offered value (untyped)", offered, sha256Hex(cbytes));
    } else if (Object.hasOwn(offered, "value_hex")) {
      if (offered.class === "local_erasure_safe") {
        const secret = inp.object_secret_hex ?? offered.object_secret_hex;
        expectEq(name, "offered value (hmac)", offered.value_hex, hmacSha256Hex(secret, cbytes));
      } else {
        expectEq(name, "offered value (sha-256)", offered.value_hex, sha256Hex(cbytes));
      }
    }
  }
  return 1;
}

/** @type {Checker} */
function checkPrivacy(name, kase) {
  const inp = kase.input;
  const exp = kase.expected;
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
