//! A strict interpreter for exactly the JSON-Schema subset the C3a
//! tools document uses — hand-written checks driven by the embedded
//! document (the kovee-mcp pattern), no JSON-Schema crate.
//!
//! Two entry points:
//! - [`check_supported`] walks a schema once at startup and refuses any
//!   keyword, `type`, or `pattern` this interpreter does not implement,
//!   so a document evolution can never be silently under-validated —
//!   the server fails to start instead;
//! - [`validate`] enforces the schema on every tool input before
//!   dispatch. Every document shape is closed
//!   (`additionalProperties: false`), so an envelope- or
//!   channel-derived member riding in on tool input (`actor_ref`,
//!   `meta`, `version`, …) is refused as an unknown member.
//!
//! Keywords compose conjunctively (draft 2020-12): a node's `$ref` is
//! applied alongside its sibling keywords (the document's contextual
//! digest-class bindings are `$ref: digestRef` plus a refining
//! `properties`), and `oneOf` arms refine an already-checked base.
//!
//! `pattern` literals are mapped verbatim to hand-written predicates;
//! the timestamp pattern maps to `bpp_core::time::parse_rfc3339_utc`,
//! which enforces the same lexical shape plus calendar validity — the
//! daemon's own RT-17 discipline, so the combined outcome is identical.

use serde_json::{Map, Value};

type JsonMap = Map<String, Value>;

/// The schema keywords this interpreter implements — exactly the
/// constructs the C3a document uses.
const KEYWORDS: [&str; 21] = [
    "$comment",
    "$defs",
    "$ref",
    "additionalProperties",
    "const",
    "description",
    "enum",
    "items",
    "maxItems",
    "maxLength",
    "maximum",
    "minItems",
    "minLength",
    "minimum",
    "not",
    "oneOf",
    "pattern",
    "properties",
    "required",
    "type",
    "uniqueItems",
];

// ---------------------------------------------------------- patterns ----

fn is_visible_ascii(s: &str, min: usize, max: usize) -> bool {
    (min..=max).contains(&s.len()) && s.bytes().all(|b| (0x21..=0x7e).contains(&b))
}

/// `^[\x21-\x7e]{1,128}$` — the §14.9 identifier.
fn is_identifier(s: &str) -> bool {
    bpp_core::envelope::is_identifier(s)
}

/// `^[\x21-\x7e]{1,4096}$` — the G38 continuation-token bound.
fn is_visible_4096(s: &str) -> bool {
    is_visible_ascii(s, 1, 4096)
}

/// `^[0-9a-f]{64}$` — a 32-byte digest in lowercase hex.
fn is_digest_hex(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// `^[a-z][a-z0-9_]{0,127}$` — the §14.6 operation-id shape.
fn is_operation_id(s: &str) -> bool {
    bpp_core::envelope::is_operation_id(s)
}

/// `^[a-z][a-z0-9_]{0,63}$` — a short snake token.
fn is_snake_64(s: &str) -> bool {
    let bytes = s.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 64
        && bytes[0].is_ascii_lowercase()
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

/// `^[A-Z]{3}$` — an ISO-4217-shaped currency code.
fn is_currency_code(s: &str) -> bool {
    s.len() == 3 && s.bytes().all(|b| b.is_ascii_uppercase())
}

/// `^[a-z][a-z0-9+.-]{0,31}$` — a URI scheme / protocol label.
fn is_scheme(s: &str) -> bool {
    let bytes = s.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 32
        && bytes[0].is_ascii_lowercase()
        && bytes[1..].iter().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'+' | b'.' | b'-')
        })
}

/// `^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?(\.[a-z0-9]...)*$` — a lowercase
/// DNS a-label host: dot-separated labels of 1..=63, alphanumeric ends,
/// hyphens inside.
fn is_dns_host(s: &str) -> bool {
    !s.is_empty()
        && s.split('.').all(|label| {
            let bytes = label.as_bytes();
            !bytes.is_empty()
                && bytes.len() <= 63
                && bytes
                    .iter()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
                && bytes[0] != b'-'
                && bytes[bytes.len() - 1] != b'-'
        })
}

/// `^(?!\.{1,2}$)[^/\x00-\x1f]{1,255}$` — one path segment: 1..=255
/// scalar values, no `/`, no C0 controls, never `.` or `..`.
fn is_path_segment(s: &str) -> bool {
    let count = s.chars().count();
    (1..=255).contains(&count)
        && s != "."
        && s != ".."
        && s.chars().all(|c| c != '/' && c as u32 > 0x1f)
}

/// `^[\x21-\x39\x3b-\x7e]{1,64}:[\x21-\x7e]{1,63}$` — a
/// source-qualified id: colon-free visible-ASCII source, `:`, then a
/// visible-ASCII local part.
fn is_qualified_id(s: &str) -> bool {
    let Some((source, local)) = s.split_once(':') else {
        return false;
    };
    is_visible_ascii(source, 1, 64) && !source.contains(':') && is_visible_ascii(local, 1, 63)
}

/// The RFC 3339 UTC instant pattern — enforced through the daemon's own
/// wire parser (lexical shape plus RT-17 calendar validity).
fn is_timestamp(s: &str) -> bool {
    bpp_core::time::parse_rfc3339_utc(s).is_some()
}

/// Maps a document `pattern` literal to the predicate implementing it.
/// An unmapped pattern is unsupported and refuses at startup.
fn matcher_for(pattern: &str) -> Option<fn(&str) -> bool> {
    Some(match pattern {
        r"^[\x21-\x7e]{1,128}$" => is_identifier,
        r"^[\x21-\x7e]{1,4096}$" => is_visible_4096,
        r"^[0-9a-f]{64}$" => is_digest_hex,
        r"^[a-z][a-z0-9_]{0,127}$" => is_operation_id,
        r"^[a-z][a-z0-9_]{0,63}$" => is_snake_64,
        r"^[A-Z]{3}$" => is_currency_code,
        r"^[a-z][a-z0-9+.-]{0,31}$" => is_scheme,
        r"^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?(\.[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?)*$" => {
            is_dns_host
        }
        r"^(?!\.{1,2}$)[^/\x00-\x1f]{1,255}$" => is_path_segment,
        r"^[\x21-\x39\x3b-\x7e]{1,64}:[\x21-\x7e]{1,63}$" => is_qualified_id,
        r"^[0-9]{4}-(0[1-9]|1[0-2])-(0[1-9]|[12][0-9]|3[01])T([01][0-9]|2[0-3]):[0-5][0-9]:[0-5][0-9](\.[0-9]{1,9})?Z$" => {
            is_timestamp
        }
        _ => return None,
    })
}

// -------------------------------------------------------- resolution ----

/// Resolves a `$defs` reference from the schema ROOT. The document uses
/// both `#/$defs/<name>` and the nested `#/$defs/bpa1Policy/$defs/<name>`
/// (the RT-16 verbatim-copy discipline keeps the BPA-1 defs inside the
/// copied bpa1Policy def); every even segment must be `$defs`.
fn resolve<'a>(root: &'a Value, reference: &Value) -> Result<&'a Value, String> {
    let Some(text) = reference.as_str() else {
        return Err("$ref is not a string".to_owned());
    };
    let Some(path) = text.strip_prefix("#/") else {
        return Err(format!("$ref {text:?} is not a local reference"));
    };
    let mut node = root;
    let mut expect_defs = true;
    for segment in path.split('/') {
        if expect_defs && segment != "$defs" {
            return Err(format!("$ref {text:?} is not a $defs path"));
        }
        node = node
            .as_object()
            .and_then(|m| m.get(segment))
            .ok_or_else(|| format!("$ref {text:?} does not resolve"))?;
        expect_defs = !expect_defs;
    }
    if !expect_defs {
        return Err(format!("$ref {text:?} ends on $defs, not a definition"));
    }
    Ok(node)
}

// ------------------------------------------------------ supportedness ----

/// Verifies at startup that `root` (a tool `input_schema` with its
/// `$defs`) uses only constructs this interpreter implements.
pub fn check_supported(root: &Value) -> Result<(), String> {
    walk(root, root)
}

fn walk(root: &Value, schema: &Value) -> Result<(), String> {
    // The `false` boolean schema (nothing validates) marks a member as
    // must-be-absent in the document's oneOf arms; `true` (anything
    // validates) is not a construct the document uses — refuse it.
    if schema == &Value::Bool(false) {
        return Ok(());
    }
    let Some(map) = schema.as_object() else {
        return Err("schema node is not an object".to_owned());
    };
    for (key, member) in map {
        if !KEYWORDS.contains(&key.as_str()) {
            return Err(format!("unsupported schema keyword {key:?}"));
        }
        match key.as_str() {
            "$defs" => {
                let Some(defs) = member.as_object() else {
                    return Err("$defs is not an object".to_owned());
                };
                for sub in defs.values() {
                    walk(root, sub)?;
                }
            }
            "$ref" => {
                resolve(root, member)?;
            }
            "type" => match member.as_str() {
                Some("object" | "string" | "integer" | "array" | "boolean") => {}
                _ => return Err(format!("unsupported type {member}")),
            },
            "additionalProperties" if member != &Value::Bool(false) => {
                return Err("additionalProperties must be false".to_owned());
            }
            "pattern" => {
                let literal = member.as_str().unwrap_or("");
                if matcher_for(literal).is_none() {
                    return Err(format!("unsupported pattern {literal:?}"));
                }
            }
            "enum" if member.as_array().is_none_or(|a| a.is_empty()) => {
                return Err("enum is not a non-empty array".to_owned());
            }
            "required" => {
                let ok = member
                    .as_array()
                    .is_some_and(|a| a.iter().all(Value::is_string));
                if !ok {
                    return Err("required is not a string array".to_owned());
                }
            }
            "properties" => {
                let Some(props) = member.as_object() else {
                    return Err("properties is not an object".to_owned());
                };
                for sub in props.values() {
                    walk(root, sub)?;
                }
            }
            "items" | "not" => walk(root, member)?,
            "oneOf" => {
                let Some(arms) = member.as_array() else {
                    return Err("oneOf is not an array".to_owned());
                };
                for arm in arms {
                    walk(root, arm)?;
                }
            }
            "minLength" | "maxLength" | "minItems" | "maxItems" if member.as_u64().is_none() => {
                return Err(format!("{key} is not a non-negative integer"));
            }
            "minimum" | "maximum" if member.as_i64().is_none() => {
                return Err(format!("{key} is not an integer"));
            }
            "uniqueItems" if !member.is_boolean() => {
                return Err("uniqueItems is not a boolean".to_owned());
            }
            "description" | "$comment" if !member.is_string() => {
                return Err(format!("{key} is not a string"));
            }
            _ => {}
        }
    }
    Ok(())
}

// --------------------------------------------------------- validation ----

/// Validates one tool input against its document schema.
pub fn validate(root: &Value, input: &Value) -> Result<(), String> {
    node(root, root, input, "input")
}

fn is_integer(value: &Value) -> bool {
    // serde_json keeps 1.0 as a float, so a non-integer representation
    // fails — the I-JSON discipline of the daemon's own parser.
    value.as_i64().is_some() || value.as_u64().is_some()
}

fn node(root: &Value, schema: &Value, value: &Value, path: &str) -> Result<(), String> {
    // The `false` schema: nothing validates (a must-be-absent member).
    if schema == &Value::Bool(false) {
        return Err(format!("{path} is not allowed here (false schema)"));
    }
    let Some(map) = schema.as_object() else {
        return Err(format!("{path}: schema node is not an object"));
    };
    // Conjunctive keyword application (draft 2020-12): $ref first, then
    // the sibling keywords refine the same value.
    if let Some(reference) = map.get("$ref") {
        node(root, resolve(root, reference)?, value, path)?;
    }
    if let Some(want) = map.get("const") {
        if value != want {
            return Err(format!("{path} is not the const value {want}"));
        }
    }
    if let Some(allowed) = map.get("enum").and_then(Value::as_array) {
        if !allowed.iter().any(|candidate| candidate == value) {
            return Err(format!("{path} is not one of the closed enum values"));
        }
    }
    if let Some(negated) = map.get("not") {
        if node(root, negated, value, path).is_ok() {
            return Err(format!("{path} matches the excluded (not) shape"));
        }
    }
    if let Some(arms) = map.get("oneOf").and_then(Value::as_array) {
        let hits = arms
            .iter()
            .filter(|arm| node(root, arm, value, path).is_ok())
            .count();
        if hits != 1 {
            return Err(format!(
                "{path} matches {hits} of the {} oneOf arms (exactly one required)",
                arms.len()
            ));
        }
    }
    if let Some(wanted) = map.get("type").and_then(Value::as_str) {
        let ok = match wanted {
            "object" => value.is_object(),
            "string" => value.is_string(),
            "integer" => is_integer(value),
            "array" => value.is_array(),
            "boolean" => value.is_boolean(),
            other => return Err(format!("{path}: unsupported schema type {other:?}")),
        };
        if !ok {
            return Err(format!("{path} is not of type {wanted}"));
        }
    }
    // Shape-directed member checks (each applies only to its JSON kind,
    // the standard keyword semantics).
    if let Some(members) = value.as_object() {
        check_object(root, map, members, path)?;
    }
    if let Some(text) = value.as_str() {
        check_string(map, text, path)?;
    }
    if is_integer(value) {
        check_integer(map, value, path)?;
    }
    if let Some(items) = value.as_array() {
        check_array(root, map, items, path)?;
    }
    Ok(())
}

fn check_object(
    root: &Value,
    schema: &JsonMap,
    members: &JsonMap,
    path: &str,
) -> Result<(), String> {
    if let Some(required) = schema.get("required").and_then(Value::as_array) {
        for name in required.iter().filter_map(Value::as_str) {
            if !members.contains_key(name) {
                return Err(format!("{path}.{name} is required"));
            }
        }
    }
    let properties = schema.get("properties").and_then(Value::as_object);
    let closed = schema.get("additionalProperties") == Some(&Value::Bool(false));
    for (name, member) in members {
        match properties.and_then(|p| p.get(name)) {
            Some(sub) => node(root, sub, member, &format!("{path}.{name}"))?,
            None if closed => {
                return Err(format!(
                    "{path}.{name} is not a member of this closed shape"
                ));
            }
            None => {} // refining arm without its own closedness
        }
    }
    Ok(())
}

fn check_string(schema: &JsonMap, text: &str, path: &str) -> Result<(), String> {
    let scalars = text.chars().count() as u64;
    if let Some(min) = schema.get("minLength").and_then(Value::as_u64) {
        if scalars < min {
            return Err(format!("{path} is shorter than {min} characters"));
        }
    }
    if let Some(max) = schema.get("maxLength").and_then(Value::as_u64) {
        if scalars > max {
            return Err(format!("{path} is longer than {max} characters"));
        }
    }
    if let Some(pattern) = schema.get("pattern").and_then(Value::as_str) {
        let Some(matches) = matcher_for(pattern) else {
            return Err(format!("{path}: unsupported pattern {pattern:?}"));
        };
        if !matches(text) {
            return Err(format!("{path} does not match {pattern:?}"));
        }
    }
    Ok(())
}

fn check_integer(schema: &JsonMap, value: &Value, path: &str) -> Result<(), String> {
    let n: i128 = match (value.as_i64(), value.as_u64()) {
        (Some(i), _) => i128::from(i),
        (None, Some(u)) => i128::from(u),
        _ => return Err(format!("{path} is not an integer")),
    };
    if let Some(min) = schema.get("minimum").and_then(Value::as_i64) {
        if n < i128::from(min) {
            return Err(format!("{path} is less than {min}"));
        }
    }
    if let Some(max) = schema.get("maximum").and_then(Value::as_i64) {
        if n > i128::from(max) {
            return Err(format!("{path} is greater than {max}"));
        }
    }
    Ok(())
}

fn check_array(root: &Value, schema: &JsonMap, items: &[Value], path: &str) -> Result<(), String> {
    let count = items.len() as u64;
    if let Some(min) = schema.get("minItems").and_then(Value::as_u64) {
        if count < min {
            return Err(format!("{path} holds fewer than {min} items"));
        }
    }
    if let Some(max) = schema.get("maxItems").and_then(Value::as_u64) {
        if count > max {
            return Err(format!("{path} holds more than {max} items"));
        }
    }
    if schema.get("uniqueItems") == Some(&Value::Bool(true)) {
        for (index, item) in items.iter().enumerate() {
            if items[..index].contains(item) {
                return Err(format!("{path}[{index}] duplicates an earlier item"));
            }
        }
    }
    if let Some(item_schema) = schema.get("items") {
        for (index, item) in items.iter().enumerate() {
            node(root, item_schema, item, &format!("{path}[{index}]"))?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::document::{self, Profile};

    fn schema_of(profile: Profile, tool: &str) -> Value {
        document::load(profile)
            .unwrap()
            .tool(tool)
            .unwrap()
            .input_schema
            .clone()
    }

    fn digest(class: &str, algorithm: &str, key_ref: Option<&str>) -> Value {
        let mut d = json!({
            "class": class,
            "algorithm": algorithm,
            "value_hex": "a".repeat(64),
        });
        if let Some(key) = key_ref {
            d["key_ref"] = json!(key);
        }
        d
    }

    #[test]
    fn every_document_schema_is_supported() {
        for profile in [Profile::Candidate, Profile::Participant] {
            // document::load already runs check_supported per tool;
            // assert directly too so a drift names the failing construct.
            for tool in document::load(profile).unwrap().tools {
                check_supported(&tool.input_schema)
                    .unwrap_or_else(|e| panic!("{}: {e}", tool.name));
            }
        }
    }

    #[test]
    fn closed_shapes_refuse_envelope_and_channel_derived_members() {
        let schema = schema_of(Profile::Participant, "byom_activity_show");
        validate(&schema, &json!({"activity_stream_ref": "act-1"})).unwrap();
        for injected in ["actor_ref", "participant_ref", "meta", "version", "op"] {
            let err = validate(
                &schema,
                &json!({"activity_stream_ref": "act-1", injected: "x"}),
            )
            .unwrap_err();
            assert!(err.contains(injected), "{err}");
            assert!(err.contains("closed shape"), "{err}");
        }
    }

    #[test]
    fn digest_class_and_key_ref_pairing_is_enforced() {
        let schema = schema_of(Profile::Candidate, "byom_membership_accept");
        let base = |d: Value| json!({"offer_ref": "offer-1", "subject_digest": d});
        validate(
            &schema,
            &base(digest("local_erasure_safe", "hmac-sha-256", Some("key-1"))),
        )
        .unwrap();
        // The keyed erasure class REQUIRES key_ref (digestRef oneOf).
        let err = validate(
            &schema,
            &base(digest("local_erasure_safe", "hmac-sha-256", None)),
        )
        .unwrap_err();
        assert!(err.contains("oneOf"), "{err}");
        // The contextual class binding is a const: a portable_public
        // digest cannot stand in for a local_erasure_safe commitment.
        let err = validate(&schema, &base(digest("portable_public", "sha-256", None))).unwrap_err();
        assert!(err.contains("const"), "{err}");
        // A public class with a smuggled key_ref hits the not-arm.
        let err = validate(
            &schema,
            &json!({"offer_ref": "offer-1", "subject_digest":
                digest("local_erasure_safe", "sha-256", Some("key-1"))}),
        )
        .unwrap_err();
        assert!(err.contains("oneOf"), "{err}");
    }

    #[test]
    fn required_pattern_and_bounds_are_enforced() {
        let schema = schema_of(Profile::Participant, "byom_wake_intent_submit");
        let base = json!({
            "activity_stream_ref": "act-1",
            "generation": 1,
            "origin": "direct_participant",
            "exact_cause_ref": "cause-1",
            "exact_cause_digest": digest("local_erasure_safe", "hmac-sha-256", Some("k-1")),
            "purpose_ref": "purpose-1",
            "stable_wake_key": "wake-key-1",
            "expires_at": "2030-01-01T00:00:00Z",
        });
        validate(&schema, &base).unwrap();
        let with = |key: &str, v: Value| {
            let mut m = base.clone();
            m[key] = v;
            m
        };
        let err = validate(&schema, &with("generation", json!(1.5))).unwrap_err();
        assert!(err.contains("integer"), "{err}");
        let err = validate(&schema, &with("generation", json!(-1))).unwrap_err();
        assert!(err.contains("less than 0"), "{err}");
        let err = validate(&schema, &with("origin", json!("kernel"))).unwrap_err();
        assert!(err.contains("enum"), "{err}");
        let err = validate(&schema, &with("stable_wake_key", json!("has space"))).unwrap_err();
        assert!(err.contains("does not match"), "{err}");
        // Lexically-shaped but calendar-impossible instants fail closed
        // (RT-17, the daemon's own parser).
        let err =
            validate(&schema, &with("expires_at", json!("2030-02-30T00:00:00Z"))).unwrap_err();
        assert!(err.contains("does not match"), "{err}");
        let mut missing = base.clone();
        missing.as_object_mut().unwrap().remove("expires_at");
        let err = validate(&schema, &missing).unwrap_err();
        assert!(err.contains("expires_at is required"), "{err}");
    }

    #[test]
    fn synthetic_constructs_behave() {
        // uniqueItems.
        let schema = json!({"type": "array", "uniqueItems": true,
                            "items": {"type": "string"}});
        validate(&schema, &json!(["a", "b"])).unwrap();
        let err = validate(&schema, &json!(["a", "a"])).unwrap_err();
        assert!(err.contains("duplicates"), "{err}");
        // not.
        let schema = json!({"type": "string", "not": {"const": "x"}});
        validate(&schema, &json!("y")).unwrap();
        let err = validate(&schema, &json!("x")).unwrap_err();
        assert!(err.contains("excluded"), "{err}");
    }

    #[test]
    fn unknown_constructs_are_refused_at_startup() {
        let err = check_supported(&json!({"type": "string", "format": "uri"})).unwrap_err();
        assert!(err.contains("format"), "{err}");
        let err = check_supported(&json!({"type": "number"})).unwrap_err();
        assert!(err.contains("unsupported type"), "{err}");
        let err = check_supported(&json!({"type": "string", "pattern": "^x+$"})).unwrap_err();
        assert!(err.contains("unsupported pattern"), "{err}");
        let err =
            check_supported(&json!({"type": "object", "additionalProperties": true})).unwrap_err();
        assert!(err.contains("additionalProperties"), "{err}");
        let err = check_supported(&json!({"$ref": "#/$defs/missing"})).unwrap_err();
        assert!(err.contains("does not resolve"), "{err}");
        let err = check_supported(&json!({"$ref": "#/properties/x"})).unwrap_err();
        assert!(err.contains("$defs path"), "{err}");
    }
}
