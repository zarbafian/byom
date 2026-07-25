//! Canonical bytes and keyed digests (family-vectors/PROFILE.md §2,
//! normative): RFC 8785 JCS over the I-JSON value space, the byom
//! `$domain` type-tag injection, HMAC-SHA-256, and hex helpers.
//!
//! What you write:
//! ```
//! use bpp_core::canonical::tagged_canonical;
//! let bytes = tagged_canonical("bpp-idempotency-domain-v1",
//!     &serde_json::json!({"b": 2, "a": 1})).unwrap();
//! assert_eq!(std::str::from_utf8(&bytes).unwrap(),
//!     "{\"$domain\":\"bpp-idempotency-domain-v1\",\"a\":1,\"b\":2}");
//! ```

use serde_json::Value;
use sha2::{Digest as _, Sha256};

/// The largest I-JSON safe integer magnitude, 2^53 − 1 (§14.2).
pub const SAFE_MAX: u64 = 9_007_199_254_740_991;
/// The same bound as an exact f64 (integer-valued float acceptance).
pub const SAFE_MAX_F64: f64 = 9_007_199_254_740_991.0;

#[derive(Debug, thiserror::Error)]
pub enum CanonicalError {
    #[error("unsafe integer in canonical input")]
    UnsafeInteger,
    #[error("non-finite number in canonical input")]
    NonFinite,
    #[error("type tag requires an object without $domain")]
    TagCollision,
}

/// RFC 8785 JCS bytes: object keys sorted by UTF-16 code units, ES
/// minimal number form, the two-character escapes plus `\u00xx` for
/// remaining controls.
pub fn jcs(value: &Value) -> Result<Vec<u8>, CanonicalError> {
    let mut out = Vec::new();
    jcs_into(value, &mut out)?;
    Ok(out)
}

fn jcs_into(value: &Value, out: &mut Vec<u8>) -> Result<(), CanonicalError> {
    match value {
        Value::Null => out.extend_from_slice(b"null"),
        Value::Bool(true) => out.extend_from_slice(b"true"),
        Value::Bool(false) => out.extend_from_slice(b"false"),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                if i.unsigned_abs() > SAFE_MAX {
                    return Err(CanonicalError::UnsafeInteger);
                }
                out.extend_from_slice(i.to_string().as_bytes());
            } else if let Some(u) = n.as_u64() {
                if u > SAFE_MAX {
                    return Err(CanonicalError::UnsafeInteger);
                }
                out.extend_from_slice(u.to_string().as_bytes());
            } else if let Some(f) = n.as_f64() {
                out.extend_from_slice(es_number(f)?.as_bytes());
            } else {
                return Err(CanonicalError::NonFinite);
            }
        }
        Value::String(s) => jcs_string(s, out),
        Value::Array(items) => {
            out.push(b'[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                jcs_into(item, out)?;
            }
            out.push(b']');
        }
        Value::Object(map) => {
            let mut entries: Vec<(&String, &Value)> = map.iter().collect();
            entries.sort_by(|a, b| {
                let ka: Vec<u16> = a.0.encode_utf16().collect();
                let kb: Vec<u16> = b.0.encode_utf16().collect();
                ka.cmp(&kb)
            });
            out.push(b'{');
            for (i, (key, val)) in entries.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                jcs_string(key, out);
                out.push(b':');
                jcs_into(val, out)?;
            }
            out.push(b'}');
        }
    }
    Ok(())
}

fn jcs_string(s: &str, out: &mut Vec<u8>) {
    out.push(b'"');
    for ch in s.chars() {
        match ch {
            '\u{8}' => out.extend_from_slice(b"\\b"),
            '\t' => out.extend_from_slice(b"\\t"),
            '\n' => out.extend_from_slice(b"\\n"),
            '\u{c}' => out.extend_from_slice(b"\\f"),
            '\r' => out.extend_from_slice(b"\\r"),
            '"' => out.extend_from_slice(b"\\\""),
            '\\' => out.extend_from_slice(b"\\\\"),
            c if (c as u32) < 0x20 => {
                out.extend_from_slice(format!("\\u{:04x}", c as u32).as_bytes());
            }
            c => {
                let mut buf = [0u8; 4];
                out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            }
        }
    }
    out.push(b'"');
}

/// ECMAScript `Number::toString(10)` for a finite double (RFC 8785).
pub fn es_number(v: f64) -> Result<String, CanonicalError> {
    if !v.is_finite() {
        return Err(CanonicalError::NonFinite);
    }
    if v == 0.0 {
        return Ok("0".to_owned());
    }
    let sign = if v < 0.0 { "-" } else { "" };
    let sci = format!("{:e}", v.abs()); // e.g. "1.25e-8", "1e21"
    let (mant, exp_s) = sci.split_once('e').unwrap_or((sci.as_str(), "0"));
    let exp: i64 = exp_s.parse().unwrap_or(0);
    let (ip, fp) = mant.split_once('.').unwrap_or((mant, ""));
    let digits: String = format!("{ip}{fp}");
    let stripped = digits.trim_start_matches('0');
    let s = stripped.trim_end_matches('0');
    let trailing = (stripped.len() - s.len()) as i64;
    let k = s.len() as i64;
    // n: position of the decimal point relative to the digit string.
    let n = k + trailing + exp - fp.len() as i64;
    let out = if k <= n && n <= 21 {
        format!("{s}{}", "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        format!("{}.{}", &s[..n as usize], &s[n as usize..])
    } else if -6 < n && n <= 0 {
        format!("0.{}{s}", "0".repeat((-n) as usize))
    } else {
        let e = n - 1;
        let mant_out = if k > 1 {
            format!("{}.{}", &s[..1], &s[1..])
        } else {
            s.to_owned()
        };
        format!("{mant_out}e{}{}", if e >= 0 { "+" } else { "-" }, e.abs())
    };
    Ok(format!("{sign}{out}"))
}

/// Byom type-tagged canonical bytes (PROFILE §2, pinned decision 4):
/// inject the reserved `$domain` member at the top level, then JCS. An
/// object already carrying `$domain` fails closed.
pub fn tagged_canonical(tag: &str, object: &Value) -> Result<Vec<u8>, CanonicalError> {
    let Value::Object(map) = object else {
        return Err(CanonicalError::TagCollision);
    };
    if map.contains_key("$domain") {
        return Err(CanonicalError::TagCollision);
    }
    let mut tagged = map.clone();
    tagged.insert("$domain".to_owned(), Value::String(tag.to_owned()));
    jcs(&Value::Object(tagged))
}

/// HMAC-SHA-256 (RFC 2104).
pub fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    let mut key_block = [0u8; 64];
    if key.len() > 64 {
        let mut h = Sha256::new();
        h.update(key);
        key_block[..32].copy_from_slice(&h.finalize());
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }
    let mut inner = Sha256::new();
    inner.update(key_block.map(|b| b ^ 0x36));
    inner.update(msg);
    let inner_hash = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(key_block.map(|b| b ^ 0x5c));
    outer.update(inner_hash);
    outer.finalize().into()
}

pub fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    hex(&h.finalize())
}

pub fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn hmac_matches_rfc4231_case_two() {
        let mac = hmac_sha256(b"Jefe", b"what do ya want for nothing?");
        assert_eq!(
            hex(&mac),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn es_number_minimal_forms() {
        assert_eq!(es_number(10.0).unwrap(), "10");
        assert_eq!(es_number(-0.0).unwrap(), "0");
        assert_eq!(es_number(1e-7).unwrap(), "1e-7");
        assert_eq!(es_number(1e21).unwrap(), "1e+21");
        assert_eq!(es_number(1.5).unwrap(), "1.5");
    }

    #[test]
    fn an_existing_domain_member_fails_closed() {
        let v = serde_json::json!({"$domain": "evil"});
        assert!(tagged_canonical("t", &v).is_err());
    }
}
