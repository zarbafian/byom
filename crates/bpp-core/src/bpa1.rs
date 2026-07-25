//! BPA-1 over the frozen AST (ADR-0001; spec/schemas/ops `bpa1Policy`),
//! ported from the reference evaluator `policy/eval.py` ONLY as far as
//! the daemon needs it: structural validation of policy values arriving
//! in requests (unknown atom keys, executable-looking members, malformed
//! atoms all fail closed) and the §10.2 never-widening subset check used
//! by `mandate_derive`. Decide/intersect/canonical-digest stay with the
//! two reference evaluators; tests cross-verify this port against
//! `policy/eval.py batch`.
//!
//! What you write:
//! ```
//! use bpp_core::bpa1;
//! let narrow = serde_json::json!({"rules": [{"effect": "allow",
//!     "atoms": {"operation": {"ids": ["activity_open"]}}}]});
//! let wide = serde_json::json!({"rules": [{"effect": "allow",
//!     "atoms": {}}]});
//! bpa1::validate_policy(&narrow).unwrap();
//! assert!(bpa1::is_subset(&narrow, &wide).unwrap());
//! assert!(!bpa1::is_subset(&wide, &narrow).unwrap());
//! ```
//!
//! Deviation (documented, fail-closed): the reference evaluator verifies
//! NFC normalization of Unicode path segments; this dependency-free port
//! accepts ASCII segments only and rejects any non-ASCII segment as
//! `malformed` — strictly narrower than the reference, never wider.

use serde_json::{Map, Value};

use crate::canonical::SAFE_MAX;

/// The closed twelve-domain atom table of §10.5, in table order.
pub const DOMAINS: [&str; 12] = [
    "operation",
    "object",
    "path",
    "network_destination",
    "binding",
    "purpose",
    "classification",
    "time",
    "quantity",
    "rate",
    "assurance",
    "schema_evidence",
];

const MAX_RULES: usize = 256;
const MAX_SET: usize = 256;
const MAX_SEGMENTS: usize = 64;

/// Typed rejection, mirroring eval.py: `malformed | overflow | incomparable`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{kind} at {at:?}")]
pub struct Bpa1Error {
    pub kind: &'static str,
    pub at: String,
}

fn err(kind: &'static str, at: &str) -> Bpa1Error {
    Bpa1Error {
        kind,
        at: at.to_owned(),
    }
}

type R<T> = Result<T, Bpa1Error>;

// -------------------------------------------------------- primitives ----

fn obj<'v>(v: &'v Value, at: &str) -> R<&'v Map<String, Value>> {
    v.as_object().ok_or_else(|| err("malformed", at))
}

fn required(m: &Map<String, Value>, at: &str, keys: &[&str]) -> R<()> {
    for k in keys {
        if !m.contains_key(*k) {
            return Err(err("malformed", &format!("{at}/{k}")));
        }
    }
    Ok(())
}

fn extras(m: &Map<String, Value>, at: &str, allowed: &[&str]) -> R<()> {
    let mut unknown: Vec<&String> = m
        .keys()
        .filter(|k| !allowed.contains(&k.as_str()))
        .collect();
    unknown.sort_by_key(|s| s.encode_utf16().collect::<Vec<u16>>());
    if let Some(first) = unknown.first() {
        let escaped = first.replace('~', "~0").replace('/', "~1");
        return Err(err("malformed", &format!("{at}/{escaped}")));
    }
    Ok(())
}

fn int(v: &Value, at: &str, lo: u64, hi: u64) -> R<u64> {
    // The wire is strict I-JSON: only safe integers reach here.
    let n = v.as_u64().ok_or_else(|| err("malformed", at))?;
    if n < lo || n > hi {
        return Err(err("malformed", at));
    }
    Ok(n)
}

fn is_identifier(s: &str) -> bool {
    crate::envelope::is_identifier(s)
}

fn is_op_id(s: &str) -> bool {
    crate::envelope::is_operation_id(s)
}

fn is_sqid(s: &str) -> bool {
    // ^[\x21-\x39\x3b-\x7e]{1,64}:[\x21-\x7e]{1,63}$
    let Some(colon) = s.find(':') else {
        return false;
    };
    let (source, id) = (&s[..colon], &s[colon + 1..]);
    !source.is_empty()
        && source.len() <= 64
        && source
            .bytes()
            .all(|b| (0x21..=0x7e).contains(&b) && b != b':')
        && !id.is_empty()
        && id.len() <= 63
        && id.bytes().all(|b| (0x21..=0x7e).contains(&b))
}

fn is_alabel(s: &str) -> bool {
    if s.is_empty() || s.len() > 253 {
        return false;
    }
    s.split('.').all(|label| {
        let b = label.as_bytes();
        !b.is_empty()
            && b.len() <= 63
            && b.first()
                .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
            && b.last()
                .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
            && b.iter()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
    })
}

fn is_scheme(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && b.len() <= 32
        && b[0].is_ascii_lowercase()
        && b[1..].iter().all(|c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, b'+' | b'.' | b'-')
        })
}

fn is_dim(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && b.len() <= 64
        && b[0].is_ascii_lowercase()
        && b[1..]
            .iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_')
}

fn is_ccy(s: &str) -> bool {
    s.len() == 3 && s.bytes().all(|b| b.is_ascii_uppercase())
}

fn is_hex64(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn string<'v>(v: &'v Value, at: &str, check: fn(&str) -> bool) -> R<&'v str> {
    let s = v.as_str().ok_or_else(|| err("malformed", at))?;
    if !check(s) {
        return Err(err("malformed", at));
    }
    Ok(s)
}

fn segment<'v>(v: &'v Value, at: &str) -> R<&'v str> {
    let s = v.as_str().ok_or_else(|| err("malformed", at))?;
    // ASCII-only in this port (module note): fail closed on anything the
    // reference would have to NFC-verify.
    if !s.is_ascii() {
        return Err(err("malformed", at));
    }
    if s.is_empty() || s.len() > 255 || s == "." || s == ".." {
        return Err(err("malformed", at));
    }
    if s.bytes().any(|b| b == b'/' || b < 0x20) {
        return Err(err("malformed", at));
    }
    Ok(s)
}

fn id_array(v: &Value, at: &str, check: fn(&str) -> bool, max_items: usize) -> R<Vec<String>> {
    let list = v.as_array().ok_or_else(|| err("malformed", at))?;
    if list.len() > max_items {
        return Err(err("overflow", at));
    }
    let mut out = Vec::with_capacity(list.len());
    for (j, item) in list.iter().enumerate() {
        out.push(string(item, &format!("{at}/{j}"), check)?.to_owned());
    }
    let mut dedup = out.clone();
    dedup.sort();
    dedup.dedup();
    if dedup.len() != out.len() {
        return Err(err("malformed", at));
    }
    Ok(out)
}

const DIGEST_CLASSES: [&str; 6] = [
    "structural_public",
    "portable_public",
    "local_erasure_safe",
    "scope_erasure_safe",
    "disclosed_party",
    "ciphertext_public",
];
const DIGEST_PUBLIC: [&str; 4] = [
    "structural_public",
    "portable_public",
    "disclosed_party",
    "ciphertext_public",
];

fn digest_ref(v: &Value, at: &str) -> R<()> {
    let m = obj(v, at)?;
    required(m, at, &["class", "algorithm", "value_hex"])?;
    extras(m, at, &["class", "algorithm", "key_ref", "value_hex"])?;
    let class = m["class"].as_str().unwrap_or_default();
    if !DIGEST_CLASSES.contains(&class) {
        return Err(err("malformed", &format!("{at}/class")));
    }
    let alg = m["algorithm"].as_str().unwrap_or_default();
    if !matches!(alg, "sha-256" | "hmac-sha-256") {
        return Err(err("malformed", &format!("{at}/algorithm")));
    }
    string(&m["value_hex"], &format!("{at}/value_hex"), is_hex64)?;
    if DIGEST_PUBLIC.contains(&class) {
        if alg != "sha-256" {
            return Err(err("malformed", &format!("{at}/algorithm")));
        }
        if m.contains_key("key_ref") {
            return Err(err("malformed", &format!("{at}/key_ref")));
        }
    } else {
        if alg != "hmac-sha-256" {
            return Err(err("malformed", &format!("{at}/algorithm")));
        }
        let key_ref = m
            .get("key_ref")
            .ok_or_else(|| err("malformed", &format!("{at}/key_ref")))?;
        string(key_ref, &format!("{at}/key_ref"), is_identifier)?;
    }
    Ok(())
}

// ---------------------------------------------------- atom validators ----

fn v_id_set(atom: &Value, at: &str, check: fn(&str) -> bool) -> R<()> {
    let m = obj(atom, at)?;
    required(m, at, &["ids"])?;
    extras(m, at, &["ids"])?;
    id_array(&m["ids"], &format!("{at}/ids"), check, MAX_SET)?;
    Ok(())
}

fn v_path(atom: &Value, at: &str) -> R<()> {
    let m = obj(atom, at)?;
    required(m, at, &["root", "segments", "match"])?;
    extras(m, at, &["root", "segments", "match"])?;
    string(&m["root"], &format!("{at}/root"), is_sqid)?;
    let segs = m["segments"]
        .as_array()
        .ok_or_else(|| err("malformed", &format!("{at}/segments")))?;
    if segs.len() > MAX_SEGMENTS {
        return Err(err("overflow", &format!("{at}/segments")));
    }
    for (j, s) in segs.iter().enumerate() {
        segment(s, &format!("{at}/segments/{j}"))?;
    }
    match m["match"].as_str() {
        Some("exact") | Some("subtree") => Ok(()),
        _ => Err(err("malformed", &format!("{at}/match"))),
    }
}

fn v_cidr(
    v: &Value,
    at: &str,
    ncomp: usize,
    comp_bits: u32,
    max_prefix: u64,
    member: &str,
) -> R<()> {
    let m = obj(v, at)?;
    required(m, at, &[member, "prefix_len"])?;
    extras(m, at, &[member, "prefix_len"])?;
    let comps = m[member]
        .as_array()
        .ok_or_else(|| err("malformed", &format!("{at}/{member}")))?;
    if comps.len() != ncomp {
        return Err(err("malformed", &format!("{at}/{member}")));
    }
    let mut values = Vec::with_capacity(ncomp);
    for (j, c) in comps.iter().enumerate() {
        values.push(int(
            c,
            &format!("{at}/{member}/{j}"),
            0,
            (1u64 << comp_bits) - 1,
        )?);
    }
    let prefix = int(&m["prefix_len"], &format!("{at}/prefix_len"), 0, max_prefix)?;
    for (j, c) in values.iter().enumerate() {
        let covered = (prefix as i64 - (j as i64) * comp_bits as i64).clamp(0, comp_bits as i64);
        let mask = (1u64 << (comp_bits as i64 - covered)) - 1;
        if c & mask != 0 {
            return Err(err("malformed", at));
        }
    }
    Ok(())
}

fn v_host_atom(host: &Value, at: &str) -> R<()> {
    let m = obj(host, at)?;
    let keys: Vec<&str> = m.keys().map(String::as_str).collect();
    match keys.as_slice() {
        ["dns"] => {
            string(&m["dns"], &format!("{at}/dns"), is_alabel)?;
            Ok(())
        }
        ["ip4_cidr"] => v_cidr(
            &m["ip4_cidr"],
            &format!("{at}/ip4_cidr"),
            4,
            8,
            32,
            "octets",
        ),
        ["ip6_cidr"] => v_cidr(
            &m["ip6_cidr"],
            &format!("{at}/ip6_cidr"),
            8,
            16,
            128,
            "groups",
        ),
        _ => Err(err("malformed", at)),
    }
}

fn v_network(atom: &Value, at: &str) -> R<()> {
    let m = obj(atom, at)?;
    required(m, at, &["scheme", "host", "ports", "protocol"])?;
    extras(m, at, &["scheme", "host", "ports", "protocol"])?;
    string(&m["scheme"], &format!("{at}/scheme"), is_scheme)?;
    v_host_atom(&m["host"], &format!("{at}/host"))?;
    let ports = obj(&m["ports"], &format!("{at}/ports"))?;
    required(ports, &format!("{at}/ports"), &["first", "last"])?;
    extras(ports, &format!("{at}/ports"), &["first", "last"])?;
    let first = int(&ports["first"], &format!("{at}/ports/first"), 0, 65535)?;
    let last = int(&ports["last"], &format!("{at}/ports/last"), 0, 65535)?;
    if first > last {
        return Err(err("malformed", &format!("{at}/ports/last")));
    }
    string(&m["protocol"], &format!("{at}/protocol"), is_scheme)?;
    Ok(())
}

fn v_purpose(atom: &Value, at: &str) -> R<()> {
    let m = obj(atom, at)?;
    required(m, at, &["snapshot", "path"])?;
    extras(m, at, &["snapshot", "path"])?;
    digest_ref(&m["snapshot"], &format!("{at}/snapshot"))?;
    let path = m["path"]
        .as_array()
        .ok_or_else(|| err("malformed", &format!("{at}/path")))?;
    if path.is_empty() {
        return Err(err("malformed", &format!("{at}/path")));
    }
    if path.len() > MAX_SEGMENTS {
        return Err(err("overflow", &format!("{at}/path")));
    }
    let mut seen = std::collections::BTreeSet::new();
    for (j, s) in path.iter().enumerate() {
        let s = string(s, &format!("{at}/path/{j}"), is_identifier)?;
        if !seen.insert(s.to_owned()) {
            return Err(err("malformed", &format!("{at}/path")));
        }
    }
    Ok(())
}

fn v_classification(atom: &Value, at: &str) -> R<()> {
    let m = obj(atom, at)?;
    required(m, at, &["lattice", "allowed"])?;
    extras(m, at, &["lattice", "allowed"])?;
    digest_ref(&m["lattice"], &format!("{at}/lattice"))?;
    id_array(
        &m["allowed"],
        &format!("{at}/allowed"),
        is_identifier,
        MAX_SET,
    )?;
    Ok(())
}

fn v_time(atom: &Value, at: &str) -> R<()> {
    let m = obj(atom, at)?;
    required(m, at, &["not_before", "not_after"])?;
    extras(m, at, &["not_before", "not_after"])?;
    let nb = int(&m["not_before"], &format!("{at}/not_before"), 0, SAFE_MAX)?;
    let na = int(&m["not_after"], &format!("{at}/not_after"), 0, SAFE_MAX)?;
    if nb > na {
        return Err(err("malformed", &format!("{at}/not_after")));
    }
    Ok(())
}

fn v_quantity_shape(atom: &Value, at: &str, value_key: &str) -> R<()> {
    let m = obj(atom, at)?;
    required(m, at, &["dimension", "canonical_unit", "scale", value_key])?;
    extras(
        m,
        at,
        &[
            "dimension",
            "canonical_unit",
            "scale",
            value_key,
            "currency",
            "pricing_revision",
        ],
    )?;
    let dimension = string(&m["dimension"], &format!("{at}/dimension"), is_dim)?;
    string(
        &m["canonical_unit"],
        &format!("{at}/canonical_unit"),
        is_dim,
    )?;
    int(&m["scale"], &format!("{at}/scale"), 0, 12)?;
    int(&m[value_key], &format!("{at}/{value_key}"), 0, SAFE_MAX)?;
    if dimension == "money" {
        let ccy = m
            .get("currency")
            .ok_or_else(|| err("malformed", &format!("{at}/currency")))?;
        string(ccy, &format!("{at}/currency"), is_ccy)?;
        let rev = m
            .get("pricing_revision")
            .ok_or_else(|| err("malformed", &format!("{at}/pricing_revision")))?;
        string(rev, &format!("{at}/pricing_revision"), is_identifier)?;
    } else {
        if m.contains_key("currency") {
            return Err(err("malformed", &format!("{at}/currency")));
        }
        if m.contains_key("pricing_revision") {
            return Err(err("malformed", &format!("{at}/pricing_revision")));
        }
    }
    Ok(())
}

/// Validates one closed BPA-1 quantity atom (used verbatim by the
/// `budgetRequestSet` items and the assent `rate_limit` sibling shapes).
pub fn validate_quantity_atom(atom: &Value) -> R<()> {
    v_quantity_shape(atom, "", "max")
}

/// Validates one closed §10.5 RateCeiling atom.
pub fn validate_rate_atom(atom: &Value) -> R<()> {
    v_rate(atom, "")
}

fn v_rate(atom: &Value, at: &str) -> R<()> {
    let m = obj(atom, at)?;
    let fields = [
        "dimension",
        "canonical_unit",
        "capacity",
        "refill_amount",
        "refill_period_milliseconds",
        "max_burst",
        "epoch",
        "clock",
    ];
    required(m, at, &fields)?;
    extras(m, at, &fields)?;
    string(&m["dimension"], &format!("{at}/dimension"), is_dim)?;
    string(
        &m["canonical_unit"],
        &format!("{at}/canonical_unit"),
        is_dim,
    )?;
    int(&m["capacity"], &format!("{at}/capacity"), 0, SAFE_MAX)?;
    int(
        &m["refill_amount"],
        &format!("{at}/refill_amount"),
        0,
        SAFE_MAX,
    )?;
    int(
        &m["refill_period_milliseconds"],
        &format!("{at}/refill_period_milliseconds"),
        1,
        SAFE_MAX,
    )?;
    int(&m["max_burst"], &format!("{at}/max_burst"), 0, SAFE_MAX)?;
    string(&m["epoch"], &format!("{at}/epoch"), is_identifier)?;
    if m["clock"].as_str() != Some("authority_server") {
        return Err(err("malformed", &format!("{at}/clock")));
    }
    Ok(())
}

fn v_assurance(atom: &Value, at: &str) -> R<()> {
    let m = obj(atom, at)?;
    required(m, at, &["order", "admitted"])?;
    extras(m, at, &["order", "admitted"])?;
    digest_ref(&m["order"], &format!("{at}/order"))?;
    id_array(
        &m["admitted"],
        &format!("{at}/admitted"),
        is_identifier,
        MAX_SET,
    )?;
    Ok(())
}

fn v_schema_evidence(atom: &Value, at: &str) -> R<()> {
    let m = obj(atom, at)?;
    let fields = ["schema", "verifier", "attestor", "assurance_policy"];
    required(m, at, &fields)?;
    extras(m, at, &fields)?;
    for k in fields {
        digest_ref(&m[k], &format!("{at}/{k}"))?;
    }
    Ok(())
}

fn validate_atom(domain: &str, atom: &Value, at: &str) -> R<()> {
    match domain {
        "operation" => v_id_set(atom, at, is_op_id),
        "object" | "binding" => v_id_set(atom, at, is_sqid),
        "path" => v_path(atom, at),
        "network_destination" => v_network(atom, at),
        "purpose" => v_purpose(atom, at),
        "classification" => v_classification(atom, at),
        "time" => v_time(atom, at),
        "quantity" => v_quantity_shape(atom, at, "max"),
        "rate" => v_rate(atom, at),
        "assurance" => v_assurance(atom, at),
        "schema_evidence" => v_schema_evidence(atom, at),
        _ => Err(err("malformed", at)),
    }
}

// ------------------------------------------------------- validation ----

fn canonical_rule_key(rule: &Map<String, Value>) -> R<String> {
    // Per-rule canonical form: set members sorted by UTF-16 code units,
    // then the family JCS bytes as the comparison key.
    let mut rule = rule.clone();
    if let Some(atoms) = rule.get_mut("atoms").and_then(Value::as_object_mut) {
        for atom in atoms.values_mut() {
            if let Some(atom) = atom.as_object_mut() {
                for key in ["ids", "allowed", "admitted"] {
                    if let Some(Value::Array(items)) = atom.get_mut(key) {
                        items.sort_by_key(|v| {
                            v.as_str()
                                .unwrap_or_default()
                                .encode_utf16()
                                .collect::<Vec<u16>>()
                        });
                    }
                }
            }
        }
    }
    let bytes =
        crate::canonical::jcs(&Value::Object(rule)).map_err(|_| err("malformed", "/rules"))?;
    String::from_utf8(bytes).map_err(|_| err("malformed", "/rules"))
}

/// Structural validation of one policy value against the frozen AST
/// (fail closed on unknown members, executable-looking keys, malformed
/// atoms, over-cap sets, duplicate rules). Returns `()` — callers keep
/// the original value; canonical form stays with the reference
/// evaluators.
pub fn validate_policy(p: &Value) -> R<()> {
    validate_policy_at(p, "")
}

fn validate_policy_at(p: &Value, base: &str) -> R<()> {
    let m = obj(p, base)?;
    required(m, base, &["rules"])?;
    extras(m, base, &["rules"])?;
    let rules = m["rules"]
        .as_array()
        .ok_or_else(|| err("malformed", &format!("{base}/rules")))?;
    if rules.len() > MAX_RULES {
        return Err(err("overflow", &format!("{base}/rules")));
    }
    let mut seen = std::collections::BTreeSet::new();
    for (i, r) in rules.iter().enumerate() {
        let at = format!("{base}/rules/{i}");
        let rm = obj(r, &at)?;
        required(rm, &at, &["effect", "atoms"])?;
        extras(rm, &at, &["effect", "atoms"])?;
        if !matches!(rm["effect"].as_str(), Some("allow") | Some("deny")) {
            return Err(err("malformed", &format!("{at}/effect")));
        }
        let atoms = obj(&rm["atoms"], &format!("{at}/atoms"))?;
        extras(atoms, &format!("{at}/atoms"), &DOMAINS)?;
        for d in DOMAINS {
            if let Some(atom) = atoms.get(d) {
                validate_atom(d, atom, &format!("{at}/atoms/{d}"))?;
            }
        }
        // D-RT-5: duplicate rules reject at the second occurrence.
        if !seen.insert(canonical_rule_key(rm)?) {
            return Err(err("malformed", &at));
        }
    }
    Ok(())
}

// ----------------------------------------------------------- algebra ----

fn jcs_eq(a: &Value, b: &Value) -> bool {
    match (crate::canonical::jcs(a), crate::canonical::jcs(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

fn host_kind(host: &Value) -> &'static str {
    if host.get("dns").is_some() {
        "dns"
    } else {
        "ip"
    }
}

fn comparable(domain: &str, a: &Value, b: &Value) -> bool {
    match domain {
        "purpose" => jcs_eq(&a["snapshot"], &b["snapshot"]),
        "classification" => jcs_eq(&a["lattice"], &b["lattice"]),
        "assurance" => jcs_eq(&a["order"], &b["order"]),
        "quantity" => {
            if a["dimension"] != b["dimension"]
                || a["canonical_unit"] != b["canonical_unit"]
                || a["scale"] != b["scale"]
            {
                return false;
            }
            if a["dimension"].as_str() == Some("money") {
                a["currency"] == b["currency"] && a["pricing_revision"] == b["pricing_revision"]
            } else {
                true
            }
        }
        // D-RT-4 (RT-07): equal window boundaries or incomparable.
        "rate" => {
            a["dimension"] == b["dimension"]
                && a["canonical_unit"] == b["canonical_unit"]
                && a["epoch"] == b["epoch"]
                && a["refill_period_milliseconds"] == b["refill_period_milliseconds"]
        }
        "network_destination" => host_kind(&a["host"]) == host_kind(&b["host"]),
        _ => true,
    }
}

fn str_set(v: &Value, key: &str) -> std::collections::BTreeSet<String> {
    v[key]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn segments_of(v: &Value, key: &str) -> Vec<String> {
    v[key]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn prefix_le(prefix: &[String], full: &[String]) -> bool {
    prefix.len() <= full.len() && full[..prefix.len()] == *prefix
}

fn cidr_covers(p: &Value, c: &Value, member: &str, comp_bits: u32) -> bool {
    let (Some(pp), Some(cp)) = (p["prefix_len"].as_u64(), c["prefix_len"].as_u64()) else {
        return false;
    };
    if cp < pp {
        return false;
    }
    let (Some(pcs), Some(ccs)) = (p[member].as_array(), c[member].as_array()) else {
        return false;
    };
    for (j, (pc, cc)) in pcs.iter().zip(ccs.iter()).enumerate() {
        let covered = (pp as i64 - (j as i64) * comp_bits as i64).clamp(0, comp_bits as i64);
        let mask = if covered == 0 {
            0
        } else {
            ((1u64 << covered) - 1) << (comp_bits as i64 - covered)
        };
        let (Some(pc), Some(cc)) = (pc.as_u64(), cc.as_u64()) else {
            return false;
        };
        if (pc ^ cc) & mask != 0 {
            return false;
        }
    }
    true
}

fn host_subset(c: &Value, p: &Value) -> bool {
    if c.get("dns").is_some() && p.get("dns").is_some() {
        return c["dns"] == p["dns"];
    }
    if c.get("ip4_cidr").is_some() && p.get("ip4_cidr").is_some() {
        return cidr_covers(&p["ip4_cidr"], &c["ip4_cidr"], "octets", 8);
    }
    if c.get("ip6_cidr").is_some() && p.get("ip6_cidr").is_some() {
        return cidr_covers(&p["ip6_cidr"], &c["ip6_cidr"], "groups", 16);
    }
    false
}

fn rate_contained(c: &Value, p: &Value) -> bool {
    c["capacity"].as_u64() <= p["capacity"].as_u64()
        && c["max_burst"].as_u64() <= p["max_burst"].as_u64()
        && c["refill_amount"].as_u64() <= p["refill_amount"].as_u64()
}

/// `region(c) ⊆ region(p)` for one comparable, validated domain atom pair.
fn atom_subset(domain: &str, c: &Value, p: &Value) -> bool {
    match domain {
        "operation" | "object" | "binding" => str_set(c, "ids").is_subset(&str_set(p, "ids")),
        "path" => {
            if c["root"] != p["root"] {
                return false;
            }
            let (cs, ps) = (segments_of(c, "segments"), segments_of(p, "segments"));
            if p["match"].as_str() == Some("subtree") {
                prefix_le(&ps, &cs)
            } else {
                c["match"].as_str() == Some("exact") && cs == ps
            }
        }
        "network_destination" => {
            c["scheme"] == p["scheme"]
                && c["protocol"] == p["protocol"]
                && p["ports"]["first"].as_u64() <= c["ports"]["first"].as_u64()
                && c["ports"]["last"].as_u64() <= p["ports"]["last"].as_u64()
                && host_subset(&c["host"], &p["host"])
        }
        "purpose" => prefix_le(&segments_of(p, "path"), &segments_of(c, "path")),
        "classification" => str_set(c, "allowed").is_subset(&str_set(p, "allowed")),
        "time" => {
            p["not_before"].as_u64() <= c["not_before"].as_u64()
                && c["not_after"].as_u64() <= p["not_after"].as_u64()
        }
        "quantity" => c["max"].as_u64() <= p["max"].as_u64(),
        "rate" => rate_contained(c, p),
        "assurance" => str_set(c, "admitted").is_subset(&str_set(p, "admitted")),
        // schema_evidence: exact tuple equality.
        _ => ["schema", "verifier", "attestor", "assurance_policy"]
            .iter()
            .all(|k| jcs_eq(&c[*k], &p[*k])),
    }
}

/// Is the intersection region of two comparable atoms empty?
fn atoms_disjoint(domain: &str, a: &Value, b: &Value) -> bool {
    match domain {
        "operation" | "object" | "binding" => str_set(a, "ids")
            .intersection(&str_set(b, "ids"))
            .next()
            .is_none(),
        "path" => {
            if a["root"] != b["root"] {
                return true;
            }
            let (sa, sb) = (segments_of(a, "segments"), segments_of(b, "segments"));
            match (a["match"].as_str(), b["match"].as_str()) {
                (Some("exact"), Some("exact")) => sa != sb,
                (Some("exact"), _) => !prefix_le(&sb, &sa),
                (_, Some("exact")) => !prefix_le(&sa, &sb),
                _ => !prefix_le(&sa, &sb) && !prefix_le(&sb, &sa),
            }
        }
        "network_destination" => {
            if a["scheme"] != b["scheme"] || a["protocol"] != b["protocol"] {
                return true;
            }
            let first = a["ports"]["first"]
                .as_u64()
                .max(b["ports"]["first"].as_u64());
            let last = a["ports"]["last"].as_u64().min(b["ports"]["last"].as_u64());
            if first > last {
                return true;
            }
            !host_subset(&a["host"], &b["host"]) && !host_subset(&b["host"], &a["host"])
        }
        "purpose" => {
            let (pa, pb) = (segments_of(a, "path"), segments_of(b, "path"));
            !prefix_le(&pa, &pb) && !prefix_le(&pb, &pa)
        }
        "classification" => str_set(a, "allowed")
            .intersection(&str_set(b, "allowed"))
            .next()
            .is_none(),
        "time" => {
            a["not_before"].as_u64().max(b["not_before"].as_u64())
                > a["not_after"].as_u64().min(b["not_after"].as_u64())
        }
        // Quantity/rate meets are componentwise minima: never empty.
        "quantity" | "rate" => false,
        "assurance" => str_set(a, "admitted")
            .intersection(&str_set(b, "admitted"))
            .next()
            .is_none(),
        _ => !["schema", "verifier", "attestor", "assurance_policy"]
            .iter()
            .all(|k| jcs_eq(&a[*k], &b[*k])),
    }
}

fn rules_of(p: &Value) -> Vec<&Value> {
    p["rules"]
        .as_array()
        .map(|a| a.iter().collect())
        .unwrap_or_default()
}

fn atoms_of(r: &Value) -> &Value {
    &r["atoms"]
}

/// Every domain the covering rule constrains must be constrained at
/// least as tightly by the covered rule (absence is wider — §10.2/G33).
fn covers(covering: &Value, covered: &Value) -> bool {
    for d in DOMAINS {
        if let Some(pa) = atoms_of(covering).get(d) {
            match atoms_of(covered).get(d) {
                Some(ca) => {
                    if !atom_subset(d, ca, pa) {
                        return false;
                    }
                }
                None => return false,
            }
        }
    }
    true
}

fn overlaps(a: &Value, b: &Value) -> bool {
    for d in DOMAINS {
        if let (Some(aa), Some(ba)) = (atoms_of(a).get(d), atoms_of(b).get(d)) {
            if atoms_disjoint(d, aa, ba) {
                return false;
            }
        }
    }
    true
}

/// The §10.2 never-widening check: is `child` a mechanical subset of
/// `parent`? Both are validated first; an incomparable same-domain atom
/// pair rejects (`incomparable`), exactly as `policy/eval.py
/// op_is_subset`.
pub fn is_subset(child: &Value, parent: &Value) -> R<bool> {
    validate_policy_at(child, "/child")?;
    validate_policy_at(parent, "/parent")?;
    let (child_rules, parent_rules) = (rules_of(child), rules_of(parent));
    // Comparability prepass, fixed scan order.
    for rc in &child_rules {
        for rp in &parent_rules {
            for d in DOMAINS {
                if let (Some(a), Some(b)) = (atoms_of(rc).get(d), atoms_of(rp).get(d)) {
                    if !comparable(d, a, b) {
                        return Err(err("incomparable", d));
                    }
                }
            }
        }
    }
    let c_allow: Vec<&&Value> = child_rules
        .iter()
        .filter(|r| r["effect"] == "allow")
        .collect();
    let c_deny: Vec<&&Value> = child_rules
        .iter()
        .filter(|r| r["effect"] == "deny")
        .collect();
    let p_allow: Vec<&&Value> = parent_rules
        .iter()
        .filter(|r| r["effect"] == "allow")
        .collect();
    let p_deny: Vec<&&Value> = parent_rules
        .iter()
        .filter(|r| r["effect"] == "deny")
        .collect();

    for rc in &c_allow {
        if !p_allow.iter().any(|rp| covers(rp, rc)) {
            return Ok(false);
        }
    }
    for rd in &p_deny {
        let applicable = c_allow.iter().any(|rc| overlaps(rd, rc));
        if !applicable {
            continue;
        }
        let preserved = c_deny.iter().any(|rd2| covers(rd2, rd));
        if !preserved {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn executable_looking_members_fail_the_closed_ast() {
        let p = json!({"rules": [{"effect": "allow",
            "atoms": {"callback": "https://evil.example/hook"}}]});
        let e = validate_policy(&p).unwrap_err();
        assert_eq!(e.kind, "malformed");
        assert!(e.at.contains("callback"), "{e:?}");
    }

    #[test]
    fn duplicate_rules_reject_at_second_occurrence() {
        let rule = json!({"effect": "allow", "atoms": {"operation": {"ids": ["a", "b"]}}});
        let dup = json!({"effect": "allow", "atoms": {"operation": {"ids": ["b", "a"]}}});
        let p = json!({"rules": [rule, dup]});
        let e = validate_policy(&p).unwrap_err();
        assert_eq!(e.at, "/rules/1");
    }

    #[test]
    fn widening_is_detected_and_narrowing_passes() {
        let parent = json!({"rules": [{"effect": "allow",
            "atoms": {"operation": {"ids": ["activity_open", "delivery_submit"]}}}]});
        let child = json!({"rules": [{"effect": "allow",
            "atoms": {"operation": {"ids": ["activity_open"]}}}]});
        let wide = json!({"rules": [{"effect": "allow",
            "atoms": {"operation": {"ids": ["activity_open", "society_dissolve"]}}}]});
        assert!(is_subset(&child, &parent).unwrap());
        assert!(!is_subset(&wide, &parent).unwrap());
    }

    #[test]
    fn parent_denies_must_be_preserved_where_applicable() {
        let parent = json!({"rules": [
            {"effect": "allow", "atoms": {}},
            {"effect": "deny", "atoms": {"operation": {"ids": ["society_dissolve"]}}}]});
        let child_bad = json!({"rules": [{"effect": "allow", "atoms": {}}]});
        let child_ok = json!({"rules": [
            {"effect": "allow", "atoms": {"operation": {"ids": ["activity_open"]}}}]});
        assert!(!is_subset(&child_bad, &parent).unwrap());
        // The child allow does not overlap the deny region... it does:
        // overlap requires shared-domain non-disjointness; the deny
        // constrains operation and the child's operation set is disjoint,
        // so the deny is inapplicable.
        assert!(is_subset(&child_ok, &parent).unwrap());
    }

    #[test]
    fn incomparable_lattices_reject() {
        let a = json!({"rules": [{"effect": "allow", "atoms": {"classification": {
            "lattice": {"class": "structural_public", "algorithm": "sha-256",
                        "value_hex": "a".repeat(64)},
            "allowed": ["public"]}}}]});
        let b = json!({"rules": [{"effect": "allow", "atoms": {"classification": {
            "lattice": {"class": "structural_public", "algorithm": "sha-256",
                        "value_hex": "b".repeat(64)},
            "allowed": ["public"]}}}]});
        let e = is_subset(&a, &b).unwrap_err();
        assert_eq!(e.kind, "incomparable");
    }
}
