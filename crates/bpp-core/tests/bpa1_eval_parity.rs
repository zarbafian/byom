//! Cross-verification of the daemon's Rust BPA-1 port (`bpp_core::bpa1`)
//! against the reference evaluator `policy/eval.py` (the B1 sheet's
//! never-widening gate): every generated policy pair must agree on
//! `well_formed` (accept/reject kind) and on the §10.2 `is_subset`
//! verdict — subset boolean and rejection kind alike — through
//! `python3 policy/eval.py batch`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Write as _;
use std::process::{Command, Stdio};

use serde_json::{json, Value};

fn eval_py() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../policy/eval.py")
        .canonicalize()
        .expect("policy/eval.py")
}

/// One digest ref usable inside atoms.
fn digest(hex_char: char) -> Value {
    json!({"class": "structural_public", "algorithm": "sha-256",
           "value_hex": hex_char.to_string().repeat(64)})
}

fn rule(effect: &str, atoms: Value) -> Value {
    json!({"effect": effect, "atoms": atoms})
}

fn policy(rules: Vec<Value>) -> Value {
    json!({ "rules": rules })
}

/// The deterministic policy corpus: valid policies over every atom
/// domain plus malformed shapes, so pairwise `is_subset` covers subset,
/// non-subset, deny-preservation, incomparability, and malformed
/// rejection through BOTH evaluators.
fn corpus() -> Vec<Value> {
    let ops_wide = json!({"operation": {"ids": ["activity_open", "delivery_submit",
                                                "continuation_write"]}});
    let ops_narrow = json!({"operation": {"ids": ["activity_open"]}});
    let path_subtree = json!({"path": {"root": "kovee:realm-1", "segments": ["work"],
                                       "match": "subtree"}});
    let path_exact = json!({"path": {"root": "kovee:realm-1",
                                     "segments": ["work", "notes.md"], "match": "exact"}});
    let path_other_root = json!({"path": {"root": "kovee:realm-2", "segments": [],
                                          "match": "subtree"}});
    let net_wide = json!({"network_destination": {"scheme": "https",
        "host": {"dns": "example.com"}, "ports": {"first": 1, "last": 65535},
        "protocol": "tcp"}});
    let net_narrow = json!({"network_destination": {"scheme": "https",
        "host": {"dns": "example.com"}, "ports": {"first": 443, "last": 443},
        "protocol": "tcp"}});
    let net_cidr = json!({"network_destination": {"scheme": "https",
        "host": {"ip4_cidr": {"octets": [10, 0, 0, 0], "prefix_len": 8}},
        "ports": {"first": 443, "last": 443}, "protocol": "tcp"}});
    let net_cidr_narrow = json!({"network_destination": {"scheme": "https",
        "host": {"ip4_cidr": {"octets": [10, 1, 0, 0], "prefix_len": 16}},
        "ports": {"first": 443, "last": 443}, "protocol": "tcp"}});
    let time_wide = json!({"time": {"not_before": 0, "not_after": 4102444800000i64}});
    let time_narrow = json!({"time": {"not_before": 1000, "not_after": 2000}});
    let quantity_wide = json!({"quantity": {"dimension": "tokens",
        "canonical_unit": "token", "scale": 0, "max": 100000}});
    let quantity_narrow = json!({"quantity": {"dimension": "tokens",
        "canonical_unit": "token", "scale": 0, "max": 100}});
    let quantity_other_dim = json!({"quantity": {"dimension": "requests",
        "canonical_unit": "request", "scale": 0, "max": 100}});
    let rate_wide = json!({"rate": {"dimension": "requests", "canonical_unit": "request",
        "capacity": 100, "refill_amount": 10, "refill_period_milliseconds": 1000,
        "max_burst": 50, "epoch": "epoch-1", "clock": "authority_server"}});
    let rate_narrow = json!({"rate": {"dimension": "requests", "canonical_unit": "request",
        "capacity": 10, "refill_amount": 1, "refill_period_milliseconds": 1000,
        "max_burst": 5, "epoch": "epoch-1", "clock": "authority_server"}});
    let rate_other_window = json!({"rate": {"dimension": "requests",
        "canonical_unit": "request", "capacity": 10, "refill_amount": 1,
        "refill_period_milliseconds": 2000, "max_burst": 5, "epoch": "epoch-1",
        "clock": "authority_server"}});
    let class_a = json!({"classification": {"lattice": digest('a'),
                                            "allowed": ["public", "internal"]}});
    let class_a_narrow = json!({"classification": {"lattice": digest('a'),
                                                   "allowed": ["public"]}});
    let class_b = json!({"classification": {"lattice": digest('b'),
                                            "allowed": ["public"]}});
    let purpose_wide = json!({"purpose": {"snapshot": digest('c'),
                                          "path": ["improve"]}});
    let purpose_narrow = json!({"purpose": {"snapshot": digest('c'),
                                            "path": ["improve", "docs"]}});
    let purpose_other = json!({"purpose": {"snapshot": digest('d'),
                                           "path": ["improve"]}});
    let assurance = json!({"assurance": {"order": digest('e'),
                                         "admitted": ["developer", "managed"]}});
    let assurance_narrow = json!({"assurance": {"order": digest('e'),
                                                "admitted": ["managed"]}});
    let objects = json!({"object": {"ids": ["kovee:obj-1", "kovee:obj-2"]}});
    let objects_narrow = json!({"object": {"ids": ["kovee:obj-1"]}});
    let schema_evidence = json!({"schema_evidence": {"schema": digest('a'),
        "verifier": digest('b'), "attestor": digest('c'),
        "assurance_policy": digest('d')}});

    let mut out = vec![
        policy(vec![rule("allow", json!({}))]),
        policy(vec![rule("allow", ops_wide.clone())]),
        policy(vec![rule("allow", ops_narrow.clone())]),
        policy(vec![rule("allow", path_subtree.clone())]),
        policy(vec![rule("allow", path_exact.clone())]),
        policy(vec![rule("allow", path_other_root)]),
        policy(vec![rule("allow", net_wide)]),
        policy(vec![rule("allow", net_narrow)]),
        policy(vec![rule("allow", net_cidr)]),
        policy(vec![rule("allow", net_cidr_narrow)]),
        policy(vec![rule("allow", time_wide.clone())]),
        policy(vec![rule("allow", time_narrow)]),
        policy(vec![rule("allow", quantity_wide)]),
        policy(vec![rule("allow", quantity_narrow.clone())]),
        policy(vec![rule("allow", quantity_other_dim)]),
        policy(vec![rule("allow", rate_wide)]),
        policy(vec![rule("allow", rate_narrow)]),
        policy(vec![rule("allow", rate_other_window)]),
        policy(vec![rule("allow", class_a.clone())]),
        policy(vec![rule("allow", class_a_narrow)]),
        policy(vec![rule("allow", class_b)]),
        policy(vec![rule("allow", purpose_wide)]),
        policy(vec![rule("allow", purpose_narrow)]),
        policy(vec![rule("allow", purpose_other)]),
        policy(vec![rule("allow", assurance)]),
        policy(vec![rule("allow", assurance_narrow)]),
        policy(vec![rule("allow", objects)]),
        policy(vec![rule("allow", objects_narrow)]),
        policy(vec![rule("allow", schema_evidence)]),
        // Deny-preservation shapes.
        policy(vec![
            rule("allow", json!({})),
            rule("deny", ops_narrow.clone()),
        ]),
        policy(vec![
            rule("allow", ops_wide.clone()),
            rule("deny", ops_narrow.clone()),
        ]),
        policy(vec![
            rule("deny", ops_wide.clone()),
            rule("allow", json!({})),
        ]),
        // Multi-domain rules.
        policy(vec![rule(
            "allow",
            json!({"operation": {"ids": ["activity_open"]},
                   "time": time_wide["time"].clone(),
                   "classification": class_a["classification"].clone()}),
        )]),
        policy(vec![
            rule("allow", ops_wide.clone()),
            rule("allow", path_subtree.clone()),
        ]),
        // Malformed shapes (both evaluators must reject with the same
        // kind).
        json!({"rules": [{"effect": "allow",
                          "atoms": {"callback": "https://evil.example/x"}}]}),
        json!({"rules": [{"effect": "permit", "atoms": {}}]}),
        json!({"rules": [{"effect": "allow",
                          "atoms": {"operation": {"ids": ["activity_open",
                                                          "activity_open"]}}}]}),
        json!({"not_rules": []}),
        json!({"rules": [
            {"effect": "allow", "atoms": {"operation": {"ids": ["a"]}}},
            {"effect": "allow", "atoms": {"operation": {"ids": ["a"]}}}]}),
        json!({"rules": [{"effect": "allow",
                          "atoms": {"time": {"not_before": 10, "not_after": 5}}}]}),
    ];
    // A couple of seeded combinations for breadth (deterministic).
    let mut seed: u64 = 45217;
    let atoms = [
        ops_wide,
        ops_narrow,
        path_subtree,
        path_exact,
        time_wide,
        quantity_narrow,
        class_a,
    ];
    for _ in 0..24 {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let a = &atoms[(seed >> 33) as usize % atoms.len()];
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let b = &atoms[(seed >> 33) as usize % atoms.len()];
        let mut merged = a.as_object().unwrap().clone();
        for (k, v) in b.as_object().unwrap() {
            merged.insert(k.clone(), v.clone());
        }
        let effect = if seed & 4 == 0 { "allow" } else { "deny" };
        out.push(policy(vec![
            rule("allow", Value::Object(merged)),
            rule(effect, b.clone()),
        ]));
    }
    out
}

fn run_eval_batch(cases: &[Value]) -> Vec<Value> {
    let mut child = Command::new("python3")
        .arg(eval_py())
        .arg("batch")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn python3 policy/eval.py batch");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(serde_json::to_string(cases).unwrap().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "eval.py batch failed");
    serde_json::from_slice::<Vec<Value>>(&output.stdout).unwrap()
}

#[test]
fn rust_port_agrees_with_the_reference_evaluator() {
    let corpus = corpus();

    // 1. well_formed parity on every corpus policy: accepted exactly
    //    when the reference accepts, same rejection kind otherwise.
    let wf_cases: Vec<Value> = corpus
        .iter()
        .map(|p| json!({"policy_op": "well_formed", "policy": p}))
        .collect();
    let wf_results = run_eval_batch(&wf_cases);
    for (i, (p, reference)) in corpus.iter().zip(&wf_results).enumerate() {
        let port = bpp_core::bpa1::validate_policy(p);
        match (reference["ok"].as_bool(), &port) {
            (Some(true), Ok(())) => {}
            (Some(false), Err(e)) => {
                assert_eq!(
                    json!(e.kind),
                    reference["error"]["kind"],
                    "well_formed kind diverges on corpus[{i}]: {p}"
                );
            }
            _ => panic!(
                "well_formed diverges on corpus[{i}]: reference {reference} port {port:?}\n{p}"
            ),
        }
    }

    // 2. is_subset parity over every ordered pair: subset boolean and
    //    rejection kind must both agree.
    let mut pairs = Vec::new();
    let mut cases = Vec::new();
    for (i, child) in corpus.iter().enumerate() {
        for (j, parent) in corpus.iter().enumerate() {
            pairs.push((i, j));
            cases.push(json!({"policy_op": "is_subset",
                              "child": child, "parent": parent}));
        }
    }
    let results = run_eval_batch(&cases);
    assert_eq!(results.len(), pairs.len());
    let mut subsets = 0;
    let mut rejections = 0;
    for ((i, j), reference) in pairs.iter().zip(&results) {
        let port = bpp_core::bpa1::is_subset(&corpus[*i], &corpus[*j]);
        match (reference["ok"].as_bool(), &port) {
            (Some(true), Ok(subset)) => {
                assert_eq!(
                    Some(*subset),
                    reference["subset"].as_bool(),
                    "is_subset verdict diverges on ({i},{j}):\nchild {}\nparent {}",
                    corpus[*i],
                    corpus[*j]
                );
                if *subset {
                    subsets += 1;
                }
            }
            (Some(false), Err(e)) => {
                assert_eq!(
                    json!(e.kind),
                    reference["error"]["kind"],
                    "rejection kind diverges on ({i},{j})"
                );
                rejections += 1;
            }
            _ => panic!(
                "is_subset diverges on ({i},{j}): reference {reference} port {port:?}\n\
                 child {}\nparent {}",
                corpus[*i], corpus[*j]
            ),
        }
    }
    // The differential actually exercised all three verdict classes.
    assert!(subsets > 0, "no subset-true pair in the corpus");
    assert!(rejections > 0, "no rejected pair in the corpus");
    assert!(
        results.len() - rejections > subsets,
        "no subset-false pair in the corpus"
    );
}
