//! Strict I-JSON acceptance (family-vectors/PROFILE.md §1, normative;
//! DESIGN.md §14.2/§14.9): one UTF-8 JSON text within hard caps, no
//! duplicate members, no reserved `$domain` member at any depth, integers
//! within ±(2^53−1), finite floats, no unpaired surrogates, depth ≤ 64,
//! ≤ 65 536 JSON values. Checks run in the profile order and the first
//! offending token names the error class; the parser is iterative, so
//! nesting bounded only by the size cap can never crash it.
//!
//! What you write:
//! ```
//! use bpp_core::ijson::{parse_request, ErrorClass};
//! assert!(parse_request(br#"{"a":1}"#).is_ok());
//! let err = parse_request(br#"{"a":1,"a":2}"#).unwrap_err();
//! assert_eq!(err.class, ErrorClass::Duplicate);
//! ```

use std::collections::BTreeSet;

use serde_json::{Map, Value};

use crate::limits::{JSON_DEPTH_MAX, JSON_NODES_MAX, REQUEST_MAX_BYTES, RESPONSE_MAX_BYTES};

/// The profile §1 error-class taxonomy, in check order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClass {
    Oversize,
    InvalidUtf8,
    Syntax,
    TrailingData,
    Duplicate,
    ReservedDomainCollision,
    UnsafeInteger,
    NonFinite,
    UnsafeNumber,
    UnpairedSurrogate,
    OverDepth,
    OverNodes,
}

impl ErrorClass {
    /// The profile's wire identifier for this class.
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorClass::Oversize => "oversize",
            ErrorClass::InvalidUtf8 => "invalid-utf8",
            ErrorClass::Syntax => "syntax",
            ErrorClass::TrailingData => "trailing-data",
            ErrorClass::Duplicate => "duplicate",
            ErrorClass::ReservedDomainCollision => "reserved-domain-collision",
            ErrorClass::UnsafeInteger => "unsafe-integer",
            ErrorClass::NonFinite => "non-finite",
            ErrorClass::UnsafeNumber => "unsafe-number",
            ErrorClass::UnpairedSurrogate => "unpaired-surrogate",
            ErrorClass::OverDepth => "over-depth",
            ErrorClass::OverNodes => "over-nodes",
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("not strict I-JSON: {}", self.class.as_str())]
pub struct IjsonError {
    pub class: ErrorClass,
}

fn fail(class: ErrorClass) -> IjsonError {
    IjsonError { class }
}

/// Parses one request body under the 256 KiB cap (§14.9).
pub fn parse_request(bytes: &[u8]) -> Result<Value, IjsonError> {
    parse_with_cap(bytes, REQUEST_MAX_BYTES)
}

/// Parses one response body under the 1 MiB cap (§14.9).
pub fn parse_response(bytes: &[u8]) -> Result<Value, IjsonError> {
    parse_with_cap(bytes, RESPONSE_MAX_BYTES)
}

/// Profile §1 acceptance under an explicit size cap, in check order.
pub fn parse_with_cap(bytes: &[u8], cap: usize) -> Result<Value, IjsonError> {
    // Order 1: the inclusive size cap, before any parsing.
    if bytes.len() > cap {
        return Err(fail(ErrorClass::Oversize));
    }
    // Order 2: valid UTF-8.
    let text = std::str::from_utf8(bytes).map_err(|_| fail(ErrorClass::InvalidUtf8))?;
    Parser::new(text).run()
}

/// One frame of the explicit container stack. Values are materialized
/// only within the depth cap; deeper frames track structure and object
/// keys (order-3 duplicate detection stays token-ordered) without ever
/// building a deep tree — so no recursive drop can overflow the stack.
enum Frame {
    Array {
        items: Option<Vec<Value>>,
    },
    Object {
        map: Option<Map<String, Value>>,
        keys: BTreeSet<String>,
        pending_key: Option<String>,
    },
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
    nodes: usize,
    max_depth: usize,
    surrogate: bool,
}

impl<'a> Parser<'a> {
    fn new(text: &'a str) -> Parser<'a> {
        Parser {
            bytes: text.as_bytes(),
            pos: 0,
            nodes: 0,
            max_depth: 0,
            surrogate: false,
        }
    }

    /// The single scan: order-3 classes fail at their token; surrogate
    /// (order 4), depth (order 5), and node (order 6) findings are
    /// deferred to after the scan, per the profile check order.
    fn run(mut self) -> Result<Value, IjsonError> {
        let mut stack: Vec<Frame> = Vec::new();
        let mut top: Option<Value> = None;

        self.skip_ws();
        loop {
            // Expect one value here (top level, array item, or member value).
            let value = self.scan_value(&mut stack)?;
            // `None` means a container was opened; descend.
            if let Some(v) = value {
                let mut completed = v;
                // Attach and unwind completed containers.
                loop {
                    match stack.last_mut() {
                        None => {
                            self.skip_ws();
                            if self.pos != self.bytes.len() {
                                return Err(fail(ErrorClass::TrailingData));
                            }
                            if self.surrogate {
                                return Err(fail(ErrorClass::UnpairedSurrogate));
                            }
                            if self.max_depth > JSON_DEPTH_MAX {
                                return Err(fail(ErrorClass::OverDepth));
                            }
                            if self.nodes > JSON_NODES_MAX {
                                return Err(fail(ErrorClass::OverNodes));
                            }
                            top = Some(completed);
                            break;
                        }
                        Some(Frame::Array { items }) => {
                            if let Some(items) = items {
                                items.push(completed);
                            }
                            self.skip_ws();
                            match self.next_byte()? {
                                b',' => {
                                    self.skip_ws();
                                    break; // next array item
                                }
                                b']' => {
                                    let done = match stack.pop() {
                                        Some(Frame::Array { items }) => {
                                            items.map(Value::Array).unwrap_or(Value::Null)
                                        }
                                        _ => return Err(fail(ErrorClass::Syntax)),
                                    };
                                    completed = done;
                                    continue; // unwind further
                                }
                                _ => return Err(fail(ErrorClass::Syntax)),
                            }
                        }
                        Some(Frame::Object {
                            map, pending_key, ..
                        }) => {
                            let key = pending_key.take().ok_or(fail(ErrorClass::Syntax))?;
                            if let Some(map) = map {
                                map.insert(key, completed);
                            }
                            self.skip_ws();
                            match self.next_byte()? {
                                b',' => {
                                    self.skip_ws();
                                    self.scan_member_key(&mut stack)?;
                                    break; // next member value
                                }
                                b'}' => {
                                    let done = match stack.pop() {
                                        Some(Frame::Object { map, .. }) => {
                                            map.map(Value::Object).unwrap_or(Value::Null)
                                        }
                                        _ => return Err(fail(ErrorClass::Syntax)),
                                    };
                                    completed = done;
                                    continue; // unwind further
                                }
                                _ => return Err(fail(ErrorClass::Syntax)),
                            }
                        }
                    }
                }
                if let Some(v) = top {
                    return Ok(v);
                }
            }
            self.skip_ws();
        }
    }

    /// Scans one value token. Opening a container pushes a frame and
    /// returns `None`; an immediately closed container (`[]`/`{}`) or a
    /// scalar returns `Some`.
    fn scan_value(&mut self, stack: &mut Vec<Frame>) -> Result<Option<Value>, IjsonError> {
        self.nodes = self.nodes.saturating_add(1);
        match self.peek_byte()? {
            b'[' => {
                self.pos += 1;
                let depth = stack.len() + 1;
                self.max_depth = self.max_depth.max(depth);
                self.skip_ws();
                if self.peek_byte()? == b']' {
                    self.pos += 1;
                    return Ok(Some(Value::Array(Vec::new())));
                }
                stack.push(Frame::Array {
                    items: (depth <= JSON_DEPTH_MAX).then(Vec::new),
                });
                Ok(None)
            }
            b'{' => {
                self.pos += 1;
                let depth = stack.len() + 1;
                self.max_depth = self.max_depth.max(depth);
                self.skip_ws();
                if self.peek_byte()? == b'}' {
                    self.pos += 1;
                    return Ok(Some(Value::Object(Map::new())));
                }
                stack.push(Frame::Object {
                    map: (depth <= JSON_DEPTH_MAX).then(Map::new),
                    keys: BTreeSet::new(),
                    pending_key: None,
                });
                self.scan_member_key(stack)?;
                Ok(None)
            }
            b'"' => Ok(Some(Value::String(self.scan_string()?))),
            b't' => {
                self.expect_literal(b"true")?;
                Ok(Some(Value::Bool(true)))
            }
            b'f' => {
                self.expect_literal(b"false")?;
                Ok(Some(Value::Bool(false)))
            }
            b'n' => {
                // `null`, but the profile classifies a bare NaN under
                // non-finite even though JSON has no such literal.
                self.expect_literal(b"null")?;
                Ok(Some(Value::Null))
            }
            b'N' => {
                self.expect_literal_class(b"NaN", ErrorClass::NonFinite)?;
                Err(fail(ErrorClass::NonFinite))
            }
            b'I' => {
                self.expect_literal_class(b"Infinity", ErrorClass::NonFinite)?;
                Err(fail(ErrorClass::NonFinite))
            }
            b'-' if self.bytes.get(self.pos + 1) == Some(&b'I') => Err(fail(ErrorClass::NonFinite)),
            b'-' | b'0'..=b'9' => Ok(Some(self.scan_number()?)),
            _ => Err(fail(ErrorClass::Syntax)),
        }
    }

    /// Scans one member key plus its colon. Within the single key token
    /// the reserved-`$domain` check precedes the duplicate check
    /// (profile-pinned decision 12).
    fn scan_member_key(&mut self, stack: &mut [Frame]) -> Result<(), IjsonError> {
        if self.peek_byte()? != b'"' {
            return Err(fail(ErrorClass::Syntax));
        }
        let key = self.scan_string()?;
        if key == "$domain" {
            return Err(fail(ErrorClass::ReservedDomainCollision));
        }
        match stack.last_mut() {
            Some(Frame::Object {
                keys, pending_key, ..
            }) => {
                if !keys.insert(key.clone()) {
                    return Err(fail(ErrorClass::Duplicate));
                }
                *pending_key = Some(key);
            }
            _ => return Err(fail(ErrorClass::Syntax)),
        }
        self.skip_ws();
        if self.next_byte()? != b':' {
            return Err(fail(ErrorClass::Syntax));
        }
        self.skip_ws();
        Ok(())
    }

    fn scan_number(&mut self) -> Result<Value, IjsonError> {
        let start = self.pos;
        if self.peek_byte()? == b'-' {
            self.pos += 1;
        }
        // Integer part: 0 | [1-9][0-9]*
        match self.peek_byte()? {
            b'0' => self.pos += 1,
            b'1'..=b'9' => {
                while matches!(self.bytes.get(self.pos), Some(b'0'..=b'9')) {
                    self.pos += 1;
                }
            }
            _ => return Err(fail(ErrorClass::Syntax)),
        }
        let mut is_float = false;
        if self.bytes.get(self.pos) == Some(&b'.') {
            is_float = true;
            self.pos += 1;
            if !matches!(self.bytes.get(self.pos), Some(b'0'..=b'9')) {
                return Err(fail(ErrorClass::Syntax));
            }
            while matches!(self.bytes.get(self.pos), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        if matches!(self.bytes.get(self.pos), Some(b'e' | b'E')) {
            is_float = true;
            self.pos += 1;
            if matches!(self.bytes.get(self.pos), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            if !matches!(self.bytes.get(self.pos), Some(b'0'..=b'9')) {
                return Err(fail(ErrorClass::Syntax));
            }
            while matches!(self.bytes.get(self.pos), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        // The slice is ASCII by construction.
        let text = std::str::from_utf8(&self.bytes[start..self.pos])
            .map_err(|_| fail(ErrorClass::Syntax))?;
        if is_float {
            let f: f64 = text.parse().map_err(|_| fail(ErrorClass::Syntax))?;
            if !f.is_finite() {
                // A float literal overflowing to infinity: unsafe-number
                // (the non-finite class is for NaN/Infinity literals).
                return Err(fail(ErrorClass::UnsafeNumber));
            }
            if f.fract() == 0.0 && f.abs() > crate::canonical::SAFE_MAX_F64 {
                return Err(fail(ErrorClass::UnsafeNumber));
            }
            Ok(serde_json::Number::from_f64(f)
                .map(Value::Number)
                .unwrap_or(Value::Null))
        } else {
            let i: i128 = text.parse().map_err(|_| fail(ErrorClass::UnsafeInteger))?;
            if i.unsigned_abs() > crate::canonical::SAFE_MAX as u128 {
                return Err(fail(ErrorClass::UnsafeInteger));
            }
            Ok(Value::from(i as i64))
        }
    }

    /// Scans one string token, decoding escapes. An unpaired surrogate is
    /// recorded (order 4, reported after the scan) and substituted.
    fn scan_string(&mut self) -> Result<String, IjsonError> {
        // Opening quote.
        if self.next_byte()? != b'"' {
            return Err(fail(ErrorClass::Syntax));
        }
        let mut out = String::new();
        loop {
            let b = self.next_byte()?;
            match b {
                b'"' => return Ok(out),
                b'\\' => match self.next_byte()? {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'/' => out.push('/'),
                    b'b' => out.push('\u{8}'),
                    b'f' => out.push('\u{c}'),
                    b'n' => out.push('\n'),
                    b'r' => out.push('\r'),
                    b't' => out.push('\t'),
                    b'u' => {
                        let unit = self.scan_hex4()?;
                        if (0xd800..=0xdbff).contains(&unit) {
                            // High surrogate: needs \uDC00..\uDFFF next.
                            if self.bytes.get(self.pos) == Some(&b'\\')
                                && self.bytes.get(self.pos + 1) == Some(&b'u')
                            {
                                let save = self.pos;
                                self.pos += 2;
                                let low = self.scan_hex4()?;
                                if (0xdc00..=0xdfff).contains(&low) {
                                    let c = 0x10000
                                        + ((unit as u32 - 0xd800) << 10)
                                        + (low as u32 - 0xdc00);
                                    out.push(char::from_u32(c).unwrap_or('\u{fffd}'));
                                } else {
                                    self.pos = save;
                                    self.surrogate = true;
                                    out.push('\u{fffd}');
                                }
                            } else {
                                self.surrogate = true;
                                out.push('\u{fffd}');
                            }
                        } else if (0xdc00..=0xdfff).contains(&unit) {
                            self.surrogate = true;
                            out.push('\u{fffd}');
                        } else {
                            out.push(char::from_u32(unit as u32).unwrap_or('\u{fffd}'));
                        }
                    }
                    _ => return Err(fail(ErrorClass::Syntax)),
                },
                0x00..=0x1f => return Err(fail(ErrorClass::Syntax)),
                _ => {
                    // Continue the UTF-8 sequence starting at b.
                    let width = utf8_width(b).ok_or(fail(ErrorClass::Syntax))?;
                    let start = self.pos - 1;
                    for _ in 1..width {
                        self.next_byte()?;
                    }
                    let s = std::str::from_utf8(&self.bytes[start..self.pos])
                        .map_err(|_| fail(ErrorClass::Syntax))?;
                    out.push_str(s);
                }
            }
        }
    }

    fn scan_hex4(&mut self) -> Result<u16, IjsonError> {
        let mut v: u16 = 0;
        for _ in 0..4 {
            let b = self.next_byte()?;
            let d = match b {
                b'0'..=b'9' => b - b'0',
                b'a'..=b'f' => b - b'a' + 10,
                b'A'..=b'F' => b - b'A' + 10,
                _ => return Err(fail(ErrorClass::Syntax)),
            };
            v = (v << 4) | d as u16;
        }
        Ok(v)
    }

    fn expect_literal(&mut self, lit: &[u8]) -> Result<(), IjsonError> {
        self.expect_literal_class(lit, ErrorClass::Syntax)
    }

    fn expect_literal_class(&mut self, lit: &[u8], class: ErrorClass) -> Result<(), IjsonError> {
        if self.bytes[self.pos..].starts_with(lit) {
            self.pos += lit.len();
            Ok(())
        } else {
            Err(fail(class))
        }
    }

    fn skip_ws(&mut self) {
        while matches!(self.bytes.get(self.pos), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn peek_byte(&self) -> Result<u8, IjsonError> {
        self.bytes
            .get(self.pos)
            .copied()
            .ok_or(fail(ErrorClass::Syntax))
    }

    fn next_byte(&mut self) -> Result<u8, IjsonError> {
        let b = self.peek_byte()?;
        self.pos += 1;
        Ok(b)
    }
}

fn utf8_width(first: u8) -> Option<usize> {
    match first {
        0x20..=0x7f => Some(1),
        0xc2..=0xdf => Some(2),
        0xe0..=0xef => Some(3),
        0xf0..=0xf4 => Some(4),
        _ => None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_plain_object() {
        let v = parse_request("{\"b\":2,\"a\":[1,2,3],\"s\":\"xé\"}".as_bytes()).unwrap();
        assert_eq!(v["a"][2], 3);
        assert_eq!(v["s"], "xé");
    }

    #[test]
    fn duplicate_beats_a_later_unsafe_number() {
        // PROFILE §1 token order (mixed-error-order).
        let err = parse_request(br#"{"a":1,"a":2,"n":1e400}"#).unwrap_err();
        assert_eq!(err.class, ErrorClass::Duplicate);
    }

    #[test]
    fn reserved_domain_precedes_duplicate_for_the_same_token() {
        let err = parse_request(br#"{"$domain":1,"$domain":2}"#).unwrap_err();
        assert_eq!(err.class, ErrorClass::ReservedDomainCollision);
    }

    #[test]
    fn pathological_depth_reports_over_depth_without_crashing() {
        let mut doc = "[".repeat(3000);
        doc.push('1');
        doc.push_str(&"]".repeat(3000));
        let err = parse_request(doc.as_bytes()).unwrap_err();
        assert_eq!(err.class, ErrorClass::OverDepth);
    }

    #[test]
    fn depth_at_cap_is_inclusive() {
        let mut doc = "[".repeat(JSON_DEPTH_MAX);
        doc.push('1');
        doc.push_str(&"]".repeat(JSON_DEPTH_MAX));
        parse_request(doc.as_bytes()).unwrap();
    }

    #[test]
    fn unpaired_surrogate_is_deferred_past_order_three() {
        // The duplicate later in the text still wins: order 3 < order 4.
        let err = parse_request(br#"{"s":"\ud800","a":1,"a":2}"#).unwrap_err();
        assert_eq!(err.class, ErrorClass::Duplicate);
        let err = parse_request(br#"{"s":"\ud800"}"#).unwrap_err();
        assert_eq!(err.class, ErrorClass::UnpairedSurrogate);
    }
}
