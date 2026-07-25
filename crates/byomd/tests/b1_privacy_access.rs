//! The §15.4 privacy chain (`b1_privacy_access`, family PROFILE §7):
//! allowed AND denied sensitive reads chain a PrivacyAccessRecord with
//! the canonical query/scope digest, result cardinality AND byte counts,
//! and the dependency binding; a failed record write BLOCKS the read
//! (`privacy_access_record_commit_failed`); and the whole chain
//! re-verifies genesis → head. Operator-resistant witnessing is
//! explicitly unclaimed at the developer profile.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use byom_store::Store;
use common::*;
use serde_json::{json, Value};

#[test]
fn privacy_chain_digest_bytes_dependency_and_failure() {
    let mut daemon = TestDaemon::start("privacy");
    let (society_id, cursor, _incarnation) = bootstrap_society(&daemon, "privacy");

    // One committed event to read sensitively.
    let events = daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "events_read",
                "continuation": cursor, "page_size": 8}),
    );
    let first = events["result"]["events"][0].clone();
    let event_id = first["event_id"].as_str().unwrap().to_owned();

    // 1. ALLOWED: the payload is released only behind the committed
    //    record.
    let allowed = daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "event_payload", "event_id": event_id}),
    );
    assert_eq!(allowed["outcome"], "ok", "{allowed}");
    assert!(allowed["result"]["payload"].is_object(), "{allowed}");
    assert_eq!(
        allowed["result"]["payload_digest"]["class"],
        "local_erasure_safe"
    );

    // 2. DENIED (absent record): still chains a record, then not_found.
    let denied = daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "event_payload",
                "event_id": "evt-does-not-exist"}),
    );
    assert_eq!(kind_of(&denied), "not_found", "{denied}");

    // 3. DENIED (digest pin mismatch): a wrong payload_digest refuses
    //    and chains a denied record.
    let pinned = daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "event_payload", "event_id": event_id,
                "payload_digest": test_digest(0x99)}),
    );
    assert_eq!(kind_of(&pinned), "stale_binding", "{pinned}");

    // Inspect the chain offline (the daemon stopped; same-UID store).
    daemon.stop();
    let expected_bytes: u64;
    {
        let store = Store::open(&daemon.data_dir).unwrap();
        // The chain re-verifies genesis → head: three records.
        let verified = byom_store::privacy::verify_chain(&store, &society_id).unwrap();
        assert_eq!(verified, 3, "three chained records");

        let rows: Vec<(i64, String)> = {
            let mut stmt = store
                .conn()
                .prepare(
                    "SELECT internal_access_sequence, record FROM privacy_access_records
                     WHERE society_id = ?1 ORDER BY internal_access_sequence",
                )
                .unwrap();
            let out = stmt
                .query_map([&society_id], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap();
            out
        };
        assert_eq!(rows.len(), 3);
        let records: Vec<Value> = rows
            .iter()
            .map(|(_, text)| serde_json::from_str(text).unwrap())
            .collect();

        // Dense sequencing and the chain link shape.
        for (i, (seq, _)) in rows.iter().enumerate() {
            assert_eq!(*seq, i as i64 + 1, "dense internal_access_sequence");
        }
        assert!(records[0].get("previous_access_digest").is_none());
        assert!(records[1]["previous_access_digest"]["value_hex"].is_string());

        // The allowed record: cardinality AND byte counts, canonical
        // query digest, dependency binding.
        let allowed_record = &records[0];
        assert_eq!(allowed_record["operation"], "event_payload");
        assert_eq!(allowed_record["outcome"], "allowed");
        assert_eq!(allowed_record["result_object_count"], 1);
        let stored_payload: String = store
            .conn()
            .query_row(
                "SELECT payload FROM events WHERE event_id = ?1",
                [&event_id],
                |r| r.get(0),
            )
            .unwrap();
        expected_bytes = stored_payload.len() as u64;
        assert_eq!(
            allowed_record["result_bytes"].as_u64().unwrap(),
            expected_bytes,
            "byte count covers exactly the released payload"
        );
        // The canonical query digest re-derives from the exact query
        // under the record's own access event key.
        let access_event_id = allowed_record["access_event_id"].as_str().unwrap();
        let query = json!({"op": "event_payload", "event_id": event_id,
                           "payload_digest": Value::Null});
        let rederived = store
            .record_digest(&society_id, access_event_id, "bpp-privacy-query-v0", &query)
            .unwrap();
        assert_eq!(
            allowed_record["query_or_scope_digest"]["value_hex"],
            json!(rederived.value_hex),
            "canonical query digest re-derives"
        );
        // The dependency binding names this endpoint's honest developer
        // assurance (operator-resistant witnessing unclaimed).
        assert_eq!(
            allowed_record["dependency_digest"]["class"],
            "local_erasure_safe"
        );
        let dependency = store
            .record_digest(
                &society_id,
                access_event_id,
                "bpp-privacy-dependency-set-v0",
                &json!({
                    "surface_actor": "projection:local",
                    "society_id": society_id,
                    "assurance": "developer",
                    "witnessing":
                        "internal-logging-only (operator-resistant witnessing unclaimed)",
                }),
            )
            .unwrap();
        assert_eq!(
            allowed_record["dependency_digest"]["value_hex"],
            json!(dependency.value_hex),
            "dependency binding re-derives"
        );
        // The record digest is scope-keyed (scope_erasure_safe): erasing
        // the chain key erases the whole chain, never one record.
        assert_eq!(
            allowed_record["record_digest"]["class"],
            "scope_erasure_safe"
        );

        // Both denied records chained too.
        assert_eq!(records[1]["outcome"], "denied");
        assert_eq!(records[1]["result_object_count"], 0);
        assert_eq!(records[1]["result_bytes"], 0);
        assert_eq!(records[2]["outcome"], "denied");
    }

    // 4. FAILURE: a failed record write BLOCKS the read — unlogged bytes
    //    are never served — and appends nothing.
    daemon.restart(&[("BYOMD_PRIVACY_FAIL", "1")]);
    let blocked = daemon.call(
        "projection",
        &json!({"version": "0.2", "op": "event_payload", "event_id": event_id}),
    );
    assert_eq!(kind_of(&blocked), "unavailable", "{blocked}");
    assert_eq!(
        blocked["problem"]["title"], "privacy_access_record_commit_failed",
        "{blocked}"
    );
    assert!(
        blocked.get("result").is_none(),
        "no payload leaks: {blocked}"
    );
    daemon.stop();
    let store = Store::open(&daemon.data_dir).unwrap();
    let verified = byom_store::privacy::verify_chain(&store, &society_id).unwrap();
    assert_eq!(verified, 3, "the failed write appended nothing");
    let _ = expected_bytes;
}
