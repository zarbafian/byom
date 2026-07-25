//! Envelope + slice-1 op vector round-trips against the frozen B0.1
//! corpus (spec/vectors): strict I-JSON acceptance classes, envelope
//! shapes, the failure convention, the IdempotencyDomain digest
//! derivation (byte-pinned HMAC vectors), and the slice-1 operations'
//! closed request schemas — valid vectors deserialize (and the
//! serializable envelope types re-serialize byte-equally), invalid
//! vectors are rejected.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use bpp_core::digest::DigestRef;
use bpp_core::envelope::{MutationMeta, RawRequest, Success};
use bpp_core::idempotency::IdempotencyDomain;
use bpp_core::ijson;
use bpp_core::ops;
use bpp_core::problem::parse_failure;
use serde_json::Value;

fn vectors_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/vectors")
}

fn load(path: &Path) -> Value {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

fn base64_decode(s: &str) -> Vec<u8> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let mut buf: u32 = 0;
    let mut bits = 0;
    for &b in s.as_bytes() {
        if b == b'=' {
            break;
        }
        let v = ALPHABET.iter().position(|&a| a == b).unwrap() as u32;
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    out
}

fn synth(spec: &Value) -> Vec<u8> {
    let mut out = Vec::new();
    if let Some(prefix) = spec.get("prefix").and_then(Value::as_str) {
        out.extend_from_slice(prefix.as_bytes());
    }
    let repeat = spec.get("repeat").and_then(Value::as_str).unwrap_or("");
    let count = spec.get("count").and_then(Value::as_u64).unwrap_or(0);
    for _ in 0..count {
        out.extend_from_slice(repeat.as_bytes());
    }
    if let Some(suffix) = spec.get("suffix").and_then(Value::as_str) {
        out.extend_from_slice(suffix.as_bytes());
    }
    out
}

/// Validates one `input.schema` + `input.value` vector; returns Ok(())
/// when the value is accepted under that schema's typed shape.
fn check_schema(schema: &str, ref_: Option<&str>, value: &Value) -> Result<(), String> {
    match schema {
        "bpp-request" => {
            let req = RawRequest::from_value(value).map_err(|p| p.title)?;
            if ref_ == Some("#/$defs/mutationRequest") && req.meta.is_none() {
                return Err("required meta absent on a mutation".to_owned());
            }
            Ok(())
        }
        "bpp-mutation-meta" => {
            let meta: MutationMeta =
                serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
            meta.validate()
        }
        "bpp-success" => {
            let s: Success = serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
            s.validate()?;
            // Round-trip: byte-equal canonical re-serialization.
            let back = serde_json::to_value(&s).map_err(|e| e.to_string())?;
            assert_eq!(&back, value, "success round-trip");
            Ok(())
        }
        "bpp-failure" => {
            let f = parse_failure(value).map_err(|e| e.to_string())?;
            let back = serde_json::to_value(&f).map_err(|e| e.to_string())?;
            assert_eq!(&back, value, "failure round-trip");
            Ok(())
        }
        "bpp-idempotency-domain" => {
            if ref_ == Some("#/$defs/digestRef") {
                let d: DigestRef =
                    serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
                d.validate_wire().map_err(|e| e.to_string())?;
                return Ok(());
            }
            let domain: IdempotencyDomain =
                serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
            domain.validate().map_err(|e| e.to_string())
        }
        "hello-request" => ops::NegotiationRequest::parse(value, "hello").map(|_| ()),
        "protocol-info-request" => {
            ops::NegotiationRequest::parse(value, "protocol_info").map(|_| ())
        }
        "feature-info-request" => ops::NegotiationRequest::parse(value, "feature_info").map(|_| ()),
        "hello-result" => {
            let r: ops::HelloResult =
                serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
            r.validate()?;
            assert_eq!(
                &serde_json::to_value(&r).unwrap(),
                value,
                "hello-result round-trip"
            );
            Ok(())
        }
        "protocol-info-result" => {
            let r: ops::ProtocolInfoResult =
                serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
            r.validate()?;
            assert_eq!(
                &serde_json::to_value(&r).unwrap(),
                value,
                "protocol-info-result round-trip"
            );
            Ok(())
        }
        "feature-info-result" => {
            let r: ops::FeatureInfoResult =
                serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
            r.validate()?;
            assert_eq!(
                &serde_json::to_value(&r).unwrap(),
                value,
                "feature-info-result round-trip"
            );
            Ok(())
        }
        // Slice-1 operation request schemas.
        "society-prepare-request" => ops::SocietyPrepareRequest::parse(value).map(|_| ()),
        "society-bootstrap-request" => ops::SocietyBootstrapRequest::parse(value).map(|_| ()),
        "society-show-request" => ops::SocietyShowRequest::parse(value).map(|_| ()),
        "membership-offer-request" => ops::MembershipOfferRequest::parse(value).map(|_| ()),
        "membership-accept-request" => ops::MembershipAcceptRequest::parse(value).map(|_| ()),
        "membership-refuse-request" => ops::MembershipRefuseRequest::parse(value).map(|_| ()),
        "participant-admit-request" => ops::ParticipantAdmitRequest::parse(value).map(|_| ()),
        "manifestation-admit-request" => ops::ManifestationAdmitRequest::parse(value).map(|_| ()),
        "participant-show-request" => ops::ParticipantShowRequest::parse(value).map(|_| ()),
        "events-read-request" => ops::EventsReadRequest::parse(value).map(|_| ()),
        other => panic!("unhandled schema {other}"),
    }
}

/// One vector file. Returns true when this harness covers the vector.
fn run_vector(path: &Path) -> bool {
    let vector = load(path);
    let name = vector["name"].as_str().unwrap_or("?").to_owned();
    let input = &vector["input"];
    let expected = &vector["expected"];

    // Digest-derivation vectors: byte-pinned canonical + HMAC.
    if let (Some(domain_tag), Some(secret), Some(key_ref)) = (
        input.get("domain").and_then(Value::as_str),
        input.get("index_secret_hex").and_then(Value::as_str),
        input.get("key_ref").and_then(Value::as_str),
    ) {
        assert_eq!(
            domain_tag,
            bpp_core::idempotency::IDEMPOTENCY_DOMAIN_TAG,
            "{name}"
        );
        let domain: IdempotencyDomain = serde_json::from_value(input["value"].clone())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let canonical = domain.canonical_bytes().unwrap();
        assert_eq!(
            std::str::from_utf8(&canonical).unwrap(),
            expected["canonical"].as_str().unwrap(),
            "{name}: canonical bytes"
        );
        let digest = domain.digest(&unhex(secret), key_ref).unwrap();
        let expected_ref: DigestRef =
            serde_json::from_value(expected["digest_ref"].clone()).unwrap();
        assert_eq!(digest, expected_ref, "{name}: digest ref");
        return true;
    }

    // Raw / synthesized byte vectors: the strict I-JSON acceptance gate.
    let bytes: Option<Vec<u8>> = if let Some(raw) = input.get("raw").and_then(Value::as_str) {
        Some(raw.as_bytes().to_vec())
    } else if let Some(b64) = input.get("raw_base64").and_then(Value::as_str) {
        Some(base64_decode(b64))
    } else {
        input.get("json_synth").map(synth)
    };
    if let Some(bytes) = bytes {
        let outcome = if input.get("context").and_then(Value::as_str) == Some("response") {
            ijson::parse_response(&bytes)
        } else {
            ijson::parse_request(&bytes)
        };
        let valid = expected["valid"].as_bool().unwrap();
        match outcome {
            Ok(_) => assert!(valid, "{name}: accepted but expected invalid"),
            Err(e) => {
                assert!(!valid, "{name}: rejected but expected valid ({e})");
                if let Some(class) = expected.get("error").and_then(Value::as_str) {
                    assert_eq!(e.class.as_str(), class, "{name}: error class");
                }
            }
        }
        return true;
    }

    // Schema-typed vectors.
    let Some(schema) = input.get("schema").and_then(Value::as_str) else {
        panic!("{name}: unrecognized input form");
    };
    let ref_ = input.get("ref").and_then(Value::as_str);
    let outcome = check_schema(schema, ref_, &input["value"]);
    let valid = expected["valid"].as_bool().unwrap();
    match outcome {
        Ok(()) => assert!(valid, "{name}: accepted but expected invalid"),
        Err(e) => assert!(!valid, "{name}: rejected but expected valid ({e})"),
    }
    true
}

#[test]
fn envelope_vectors_round_trip() {
    let dir = vectors_dir().join("envelope");
    let mut covered = 0;
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    entries.sort();
    for path in &entries {
        if run_vector(path) {
            covered += 1;
        }
    }
    assert_eq!(covered, entries.len(), "every envelope vector is covered");
    assert!(covered >= 50, "corpus present ({covered})");
}

#[test]
fn slice1_op_request_vectors() {
    // The slice-1 operations' request vectors: valid deserialize,
    // invalid reject, through the same typed parsers the daemon uses.
    const SLICE1_SCHEMAS: [&str; 10] = [
        "society-prepare-request",
        "society-bootstrap-request",
        "society-show-request",
        "membership-offer-request",
        "membership-accept-request",
        "membership-refuse-request",
        "participant-admit-request",
        "manifestation-admit-request",
        "participant-show-request",
        "events-read-request",
    ];
    let dir = vectors_dir().join("ops");
    let mut covered = 0;
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    entries.sort();
    for path in &entries {
        let vector = load(path);
        let Some(schema) = vector["input"].get("schema").and_then(Value::as_str) else {
            continue;
        };
        if !SLICE1_SCHEMAS.contains(&schema) {
            continue;
        }
        run_vector(path);
        covered += 1;
    }
    assert!(covered >= 18, "slice-1 op vectors covered ({covered})");
}
