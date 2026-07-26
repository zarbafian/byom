//! B3 slice 3 — the model-egress act/effect chain (DESIGN.md §13.1-§13.3,
//! family contract Δ4/L38-L43).
//!
//! ```text
//! act_intent_prepare   -> the Δ4 model_egress class subject, compiled by
//!                         the kernel from the dependency closure
//! act_intent_position  -> the human GATE seat assents (R21)
//! act_intent_finalize  -> ONE GovernanceDecision bound to the digest
//! execution_permit_consume -> the ExecutionConsumptionReceipt Kovee's
//!                         broker must hold BEFORE egress (max_uses: 1)
//! ```
//!
//! And each refusal, exactly: no permit, spent permit, stale fence, wrong
//! class subject.
//!
//! The receipt is checked MEMBER BY MEMBER against the frozen §13.1 shape
//! and each of its digests is re-derived here — a rendered `null` or a
//! class the consumer cannot check is a failure, not a silent gap
//! (kovee seam finding on `ExecutionConsumptionReceipt`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::runtime::{
    act_context_digest, act_context_ref, act_disclosure_digest, act_disclosure_ref,
    host_effect_binding, host_effect_credential, portable_digest, sign_host_effect, Act, Claim,
    Fixture, Subordinate,
};
use common::{kind_of, test_digest};
use serde_json::{json, Value};

const BROKER: &str = "kovee-model-broker";

/// The §11.8 typed byte digest of the exact provider-request bytes: the one
/// genuinely host-owned member of the host-effect binding fragment, and the
/// only one a consumption chooses. Every other member byom rebuilds from its
/// own committed act (R3-L01, D-R3-3).
const REQUEST_BYTES: &str = "f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7";
/// A DIFFERENT set of request bytes: a different effect, so a different
/// binding fragment and a different `host_effect_digest`.
const OTHER_REQUEST_BYTES: &str =
    "0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f";

/// The FROZEN §13.1 result shape, committed with the bundle. Every receipt
/// assertion below is driven by this file, so a member the shape defines
/// but the daemon does not render fails the suite instead of reaching the
/// consumer as `null`.
const RECEIPT_SCHEMA: &str =
    include_str!("../../../spec/schemas/ops/execution-permit-consume-result.schema.json");

/// The cross-boundary canonicalization domains, written out HERE rather
/// than imported from byomd: the consumer holds only the frozen tag and
/// member set, so a change on either side of the seam must fail this test.
const RECEIPT_BINDING_TAG: &str = "bpp-execution-consumption-receipt-binding-v0";
const MANDATE_USE_BINDING_TAG: &str = "bpp-mandate-use-binding-v0";

/// Every member of the frozen shape with the digest class it pins (`None`
/// for the non-digest members), read out of the schema's contextual
/// `$defs` — the machine-checked class parity of PROFILE.md §6.2.
fn receipt_shape() -> Vec<(String, Option<String>)> {
    let schema: Value = serde_json::from_str(RECEIPT_SCHEMA).unwrap();
    let defs = schema["$defs"].clone();
    schema["properties"]
        .as_object()
        .expect("the frozen shape names its members")
        .iter()
        .map(|(name, body)| {
            let class = body["$ref"]
                .as_str()
                .and_then(|r| r.strip_prefix("#/$defs/"))
                .and_then(|def| defs[def]["properties"]["class"]["const"].as_str())
                .map(str::to_owned);
            (name.clone(), class)
        })
        .collect()
}

/// EVERY member the frozen shape defines is PRESENT, NON-NULL, and carries
/// exactly the digest class that shape pins — asserted member by member,
/// so "the field exists" can never pass for "the field is rendered".
fn assert_receipt_complete(receipt: &Value) {
    let members = receipt.as_object().expect("the receipt is an object");
    let shape = receipt_shape();
    for (name, class) in &shape {
        let value = members
            .get(name)
            .unwrap_or_else(|| panic!("the receipt omits {name}: {receipt}"));
        assert!(
            !value.is_null(),
            "the receipt renders {name} null — the consumer cannot verify a binding \
             it is never told: {receipt}"
        );
        let Some(class) = class else { continue };
        assert_eq!(
            value["class"],
            json!(class),
            "{name} digest class: {receipt}"
        );
        assert_eq!(
            value["value_hex"].as_str().map(str::len),
            Some(64),
            "{name} value_hex: {receipt}"
        );
        // PROFILE.md §6.1: the class/algorithm pairing and `key_ref`
        // presence are closed, in both directions.
        if class == "portable_public" {
            assert_eq!(value["algorithm"], "sha-256", "{name}: {receipt}");
            assert!(
                value.get("key_ref").is_none(),
                "{name} is unkeyed — the consumer re-derives it: {receipt}"
            );
        } else {
            assert_eq!(value["algorithm"], "hmac-sha-256", "{name}: {receipt}");
            assert!(
                value["key_ref"].is_string(),
                "{name} names the key its verifiability dies with: {receipt}"
            );
        }
    }
    for name in members.keys() {
        assert!(
            name == "replayed" || shape.iter().any(|(n, _)| n == name),
            "the receipt carries {name}, which the frozen shape does not define: {receipt}"
        );
    }
}

/// The `portable_public` DigestRef a CONSUMER derives: SHA-256 over the
/// `$domain`-tagged canonical bytes of the fragment it holds.
fn consumer_derived(tag: &str, fragment: &Value) -> Value {
    let bytes = bpp_core::canonical::tagged_canonical(tag, fragment).unwrap();
    json!({
        "class": "portable_public",
        "algorithm": "sha-256",
        "value_hex": bpp_core::canonical::sha256_hex(&bytes),
    })
}

/// Both cross-boundary receipt digests, RE-DERIVED from the published
/// receipt exactly as Kovee's broker must (A8, PROFILE.md §6.2): the
/// MandateUse pin over its four published members, and the receipt pin
/// over every §13.1 member except itself. Nothing is taken on trust —
/// each expected value is computed here from the fragment.
fn assert_cross_boundary_digests_rederive(receipt: &Value) {
    let mandate_use = consumer_derived(
        MANDATE_USE_BINDING_TAG,
        &json!({
            "mandate_use_id": receipt["mandate_use_ref"],
            "intent_ref": receipt["intent_ref"],
            "use_key": receipt["stable_execution_key"],
            "consumed_at": receipt["issued_at"],
        }),
    );
    assert_eq!(
        receipt["mandate_use_digest"], mandate_use,
        "mandate_use_digest must re-derive over the frozen MandateUse binding \
         fragment the receipt publishes: {receipt}"
    );
    let mut fragment = receipt.as_object().unwrap().clone();
    fragment.remove("digest");
    fragment.remove("replayed");
    let pin = consumer_derived(RECEIPT_BINDING_TAG, &Value::Object(fragment));
    assert_eq!(
        receipt["digest"], pin,
        "the receipt's own digest must re-derive over exactly the members it \
         publishes: {receipt}"
    );
}

/// The committed golden receipt: the bundle's own vector for the result.
const RECEIPT_VECTOR: &str =
    include_str!("../../../spec/vectors/ops/execution-permit-consume-result-valid.json");

/// The golden vector is BYTE-PINNED, not just shape-valid: its two
/// cross-boundary digests re-derive here over exactly the fragments the
/// frozen tags name. A changed tag, a changed member set, or a changed
/// canonicalization breaks this test — which is what makes the vector a
/// contract the consumer can implement against.
#[test]
fn the_committed_receipt_vector_re_derives_its_cross_boundary_digests() {
    let vector: Value = serde_json::from_str(RECEIPT_VECTOR).unwrap();
    assert_eq!(vector["expected"]["valid"], true);
    let receipt = &vector["input"]["value"];
    assert_receipt_complete(receipt);
    assert_cross_boundary_digests_rederive(receipt);
}

/// A running Episode holding its lease, plus its committed binding digest.
fn running(tag: &str) -> (Fixture, String, Claim, Value) {
    let f = Fixture::start(tag, 8);
    let wake = f.wake("w1");
    let ep = f.request_episode(&wake, "e1");
    f.admit_placement(&ep, "p1", Subordinate::Confirmed(200));
    let c = f.claim(&ep.episode_id, "worker-a", 600, 7, "c1");
    let started = f.start_episode(&ep.episode_id, &c, "s1");
    assert_eq!(started["outcome"], "ok", "{started}");
    let binding_digest = f.binding_digest(&c.binding_ref);
    (f, ep.episode_id, c, binding_digest)
}

fn consume(f: &Fixture, act: &Act, episode: &str, c: &Claim, key: &str) -> Value {
    let token = f.permit_token(&act.intent_id);
    f.consume_permit_with(
        &token,
        act,
        key,
        "e1",
        &act.stable_execution_key,
        BROKER,
        Some(episode),
        c.byom_fence_epoch,
        c.kovee_invocation_fence,
        act.revision,
        REQUEST_BYTES,
    )
}

#[test]
fn the_model_egress_act_chain_runs_end_to_end_and_yields_one_receipt() {
    let (f, episode, c, binding_digest) = running("b3-act-e2e");

    // -- prepare: the Δ4 class subject is COMPILED, never caller-shaped --
    let prepared = f.prepare_act_raw("a1", "model_egress", Some(BROKER));
    assert_eq!(prepared["outcome"], "ok", "{prepared}");
    let r = &prepared["result"];
    assert_eq!(r["state"], "prepared");
    assert_eq!(r["act_class"], "model_egress");
    let atoms = &r["act_class_subject"]["subject_atoms"];
    for domain in [
        "operation",
        "purpose",
        "binding",
        "classification",
        "quantity",
    ] {
        assert!(
            !atoms[domain].is_null(),
            "model_egress makes {domain} mandatory (Δ4): {atoms}"
        );
    }
    // Exactly the mandatory domains: an unnecessary extra domain would
    // fence the act against policies that do not constrain it.
    let mut present: Vec<&str> = atoms
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    present.sort_unstable();
    assert_eq!(
        present,
        [
            "binding",
            "classification",
            "operation",
            "purpose",
            "quantity"
        ],
        "the compiled subject carries exactly its class's mandatory domains"
    );
    assert_eq!(
        atoms["binding"],
        json!(format!("kovee:{BROKER}")),
        "the class subject pins the EXACT provider binding bytes leave through"
    );
    assert_eq!(atoms["quantity"]["dimension"], "output_bytes");
    // The subject is server-derived: the request carried no atoms member.
    assert!(!r["preparation_trace"]["field_sources"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(
        r["preparation_trace"]["output_subject_digest"], r["subject_digest"],
        "the trace commits to the exact prepared subject (RT-04)"
    );
    let intent_id = r["intent_id"].as_str().unwrap().to_owned();
    let seat = r["required_seat_refs"][0].as_str().unwrap().to_owned();
    let subject_digest = r["subject_digest"].clone();

    // -- REFUSAL 1: no permit. The act is not authorized yet, so there is
    // nothing to consume — not even for the right channel.
    let act_unauthorized = Act {
        intent_id: intent_id.clone(),
        seat_ref: seat.clone(),
        subject_digest: subject_digest.clone(),
        intent_digest: f.intent_digest(&intent_id),
        stable_execution_key: r["stable_execution_key"].as_str().unwrap().to_owned(),
        budget_reservation_set_ref: r["budget_reservation_set_ref"].as_str().unwrap().to_owned(),
        revision: 1,
        context_manifest_ref: act_context_ref("a1"),
        context_digest: act_context_digest(),
        disclosure_manifest_ref: act_disclosure_ref("a1"),
        disclosure_digest: act_disclosure_digest(),
    };
    assert!(
        f.token_path_exists(&format!("runtime-permit-{intent_id}.token")),
        "the permit channel is bound to the exact PREPARED act, so byom's own \
         state check answers the consumption honestly"
    );

    // -- position + finalize --
    let positioned = f.governance(&json!({
        "version": "0.2", "op": "act_intent_position",
        "meta": f.meta("actpos-a1", None),
        "proposal_ref": intent_id,
        "proposal_revision": 1,
        "subject_digest": subject_digest,
        "seat_ref": seat,
        "value": "assent",
    }));
    assert_eq!(positioned["outcome"], "ok", "{positioned}");
    assert_eq!(
        f.row(
            "SELECT state FROM act_intents WHERE intent_id = ?1",
            &intent_id
        ),
        Some("awaiting_decision".to_owned())
    );
    let finalized = f.governance(&json!({
        "version": "0.2", "op": "act_intent_finalize",
        "meta": f.meta("actfin-a1", Some(1)),
        "intent_id": intent_id,
        "subject_digest": subject_digest,
    }));
    assert_eq!(finalized["outcome"], "ok", "{finalized}");
    assert_eq!(finalized["result"]["state"], "authorized");
    assert_eq!(
        finalized["result"]["authorization_decision_ref"],
        json!(format!("dec-act-{intent_id}")),
        "ONE GovernanceDecision, derived from the subject it decides"
    );
    let act = Act {
        revision: finalized["result"]["revision"].as_u64().unwrap(),
        ..act_unauthorized.clone()
    };

    // -- consume: the receipt Kovee's broker must hold before egress ----
    let consumed = consume(&f, &act, &episode, &c, "p1");
    assert_eq!(consumed["outcome"], "ok", "{consumed}");
    let receipt = &consumed["result"];
    assert_eq!(receipt["max_uses"], 1, "one-shot BY CONSTRUCTION");
    assert_eq!(receipt["intent_ref"], json!(intent_id));
    assert_eq!(
        receipt["stable_execution_key"],
        json!(act.stable_execution_key)
    );
    assert_eq!(receipt["driver_audience"], BROKER);
    assert_eq!(receipt["episode_ref"], json!(episode));
    assert!(
        !receipt["mandate_use_ref"].is_null(),
        "MandateUse inserted once"
    );

    // -- the receipt Kovee must be able to CHECK, member by member -------
    // Every member of the frozen shape is rendered (nothing null), each
    // digest carries the class the shape pins, and no member the shape
    // does not define is present.
    assert_receipt_complete(receipt);
    // The two cross-boundary pins re-derive here, from the receipt's own
    // published bytes.
    assert_cross_boundary_digests_rederive(receipt);
    // The four published binding digests, each compared against an
    // INDEPENDENT source — byom's own committed row, or the exact pair the
    // ACT WAS AUTHORIZED FOR — never against the receipt itself and never
    // against the request (A8/R3-A01: byom recomputes its own, and renders
    // the committed value of the host's).
    assert_eq!(
        receipt["intent_digest"],
        f.intent_digest(&intent_id),
        "intent_digest is the committed ActIntent record digest, recomputed by byom"
    );
    assert_eq!(
        receipt["subject_digest"], subject_digest,
        "subject_digest is the exact authorized act subject, recomputed by byom"
    );
    assert_eq!(
        receipt["disclosure_digest"],
        act_disclosure_digest(),
        "disclosure_digest is the manifest the GATE SEAT assented to, read from \
         byom's committed act — never the value the consumption carried"
    );
    assert_eq!(
        receipt["episode_fence_digest"], binding_digest,
        "episode_fence_digest is the committed ByomEpisodeBinding digest"
    );
    // A8's converse half: byom's OWN keyed MandateUse record commitment
    // still exists, is still per-object erasable, and is NOT what the
    // receipt published — the consumer is never handed a blob it can only
    // echo.
    let own_use_digest: Value = serde_json::from_str(
        &f.row(
            "SELECT digest FROM mandate_uses WHERE mandate_use_id = ?1",
            receipt["mandate_use_ref"].as_str().unwrap(),
        )
        .expect("the MandateUse record digest"),
    )
    .unwrap();
    assert_eq!(own_use_digest["class"], "local_erasure_safe");
    assert_ne!(
        own_use_digest, receipt["mandate_use_digest"],
        "the receipt publishes the cross-boundary pin, never byom's keyed \
         record digest (A8)"
    );

    assert_eq!(
        f.count("SELECT COUNT(*) FROM mandate_uses"),
        1,
        "exactly one MandateUse for one consumption"
    );
    assert_eq!(
        f.count("SELECT COUNT(*) FROM execution_consumption_receipts"),
        1
    );
    assert_eq!(
        f.row(
            "SELECT state FROM act_intents WHERE intent_id = ?1",
            &intent_id
        ),
        Some("consumed".to_owned())
    );
    assert!(f.ledger().conserves(), "{:?}", f.ledger());

    // The exact same canonical request and key returns the SAME receipt.
    let replayed = consume(&f, &act, &episode, &c, "p1");
    assert_eq!(
        replayed, consumed,
        "the exact retry replays byte-identically"
    );
}

#[test]
fn a_consumption_without_an_authorizing_decision_is_refused() {
    let (f, episode, c, _binding_digest) = running("b3-act-nopermit");
    let prepared = f.prepare_act_raw("a1", "model_egress", Some(BROKER));
    assert_eq!(prepared["outcome"], "ok", "{prepared}");
    let r = &prepared["result"];
    let intent_id = r["intent_id"].as_str().unwrap().to_owned();
    let act = Act {
        intent_id: intent_id.clone(),
        seat_ref: r["required_seat_refs"][0].as_str().unwrap().to_owned(),
        subject_digest: r["subject_digest"].clone(),
        intent_digest: f.intent_digest(&intent_id),
        stable_execution_key: r["stable_execution_key"].as_str().unwrap().to_owned(),
        budget_reservation_set_ref: r["budget_reservation_set_ref"].as_str().unwrap().to_owned(),
        revision: 1,
        context_manifest_ref: act_context_ref("a1"),
        context_digest: act_context_digest(),
        disclosure_manifest_ref: act_disclosure_ref("a1"),
        disclosure_digest: act_disclosure_digest(),
    };
    // NO PERMIT: the act carries no GovernanceDecision, so there is nothing
    // to consume — and byom says exactly that, on the act's own channel.
    let token = f.permit_token(&act.intent_id);
    let no_permit = f.consume_permit_with(
        &token,
        &act,
        "p0",
        "e1",
        &act.stable_execution_key,
        BROKER,
        Some(&episode),
        c.byom_fence_epoch,
        c.kovee_invocation_fence,
        1,
        REQUEST_BYTES,
    );
    assert_eq!(kind_of(&no_permit), "decision_incomplete", "{no_permit}");
    assert!(
        no_permit["problem"]["detail"]
            .as_str()
            .unwrap()
            .contains("prepared"),
        "{no_permit}"
    );

    // A token from another channel class never reaches the state check.
    let worker = f.worker_token(&episode);
    let forged = f.consume_permit_with(
        &worker,
        &act,
        "p0b",
        "e1",
        &act.stable_execution_key,
        BROKER,
        Some(&episode),
        c.byom_fence_epoch,
        c.kovee_invocation_fence,
        1,
        REQUEST_BYTES,
    );
    assert_eq!(kind_of(&forged), "forbidden", "{forged}");

    // The same act, positioned but NOT finalized: still no permit.
    let positioned = f.governance(&json!({
        "version": "0.2", "op": "act_intent_position",
        "meta": f.meta("actpos-a1", None),
        "proposal_ref": act.intent_id,
        "proposal_revision": 1,
        "subject_digest": act.subject_digest,
        "seat_ref": act.seat_ref,
        "value": "assent",
    }));
    assert_eq!(positioned["outcome"], "ok", "{positioned}");
    let awaiting = f.consume_permit_with(
        &token,
        &act,
        "p1",
        "e1",
        &act.stable_execution_key,
        BROKER,
        Some(&episode),
        c.byom_fence_epoch,
        c.kovee_invocation_fence,
        1,
        REQUEST_BYTES,
    );
    assert_eq!(kind_of(&awaiting), "decision_incomplete", "{awaiting}");
    assert!(awaiting["problem"]["detail"]
        .as_str()
        .unwrap()
        .contains("awaiting_decision"));
    assert_eq!(
        f.count("SELECT COUNT(*) FROM execution_consumption_receipts"),
        0,
        "no receipt exists for an act nothing authorized"
    );
    assert_eq!(f.count("SELECT COUNT(*) FROM mandate_uses"), 0);
}

#[test]
fn a_spent_one_shot_permit_refuses_a_second_consumption() {
    let (f, episode, c, _binding_digest) = running("b3-act-spent");
    let act = f.authorized_act("a1", "model_egress", BROKER);
    let first = consume(&f, &act, &episode, &c, "p1");
    assert_eq!(first["outcome"], "ok", "{first}");

    let token = f.permit_token(&act.intent_id);
    // A DIFFERENT key can never claim the spent one-shot decision.
    let other_key = f.consume_permit_with(
        &token,
        &act,
        "pk-other",
        "e1",
        "exec-some-other-key",
        BROKER,
        Some(&episode),
        c.byom_fence_epoch,
        c.kovee_invocation_fence,
        act.revision + 1,
        REQUEST_BYTES,
    );
    assert_eq!(kind_of(&other_key), "stale_revision", "{other_key}");
    assert!(other_key["problem"]["detail"]
        .as_str()
        .unwrap()
        .contains("SPENT one-shot decision"));

    // The SAME key with the byte-identical binding recovers the retained
    // receipt (the host that crashed after step 5 asks for no new
    // authority); a CHANGED binding under that key conflicts.
    let replay = f.consume_permit_with(
        &token,
        &act,
        "pk-replay",
        "e1",
        &act.stable_execution_key,
        BROKER,
        Some(&episode),
        c.byom_fence_epoch,
        c.kovee_invocation_fence,
        act.revision + 1,
        REQUEST_BYTES,
    );
    assert_eq!(
        replay["outcome"], "ok",
        "the exact binding replays: {replay}"
    );
    assert_eq!(replay["result"]["replayed"], true);
    assert_eq!(
        replay["result"]["receipt_id"],
        first["result"]["receipt_id"]
    );
    // The recovered receipt is rendered from the STORED row while the first
    // was rendered from the row being written — one renderer, so they are
    // identical but for the replay marker, and every digest still
    // re-derives. (The mint path rendering its in-memory row is exactly
    // where the null-digest gap lived.)
    assert_receipt_complete(&replay["result"]);
    assert_cross_boundary_digests_rederive(&replay["result"]);
    let mut recovered = replay["result"].as_object().unwrap().clone();
    assert_eq!(recovered.remove("replayed"), Some(json!(true)));
    assert_eq!(
        Value::Object(recovered),
        first["result"],
        "a receipt recovered after a crash is the receipt first returned"
    );
    let changed = f.consume_permit_with(
        &token,
        &act,
        "pk-changed",
        "e1",
        &act.stable_execution_key,
        BROKER,
        Some(&episode),
        c.byom_fence_epoch,
        c.kovee_invocation_fence,
        act.revision + 1,
        OTHER_REQUEST_BYTES,
    );
    assert_eq!(kind_of(&changed), "idempotency_mismatch", "{changed}");
    // And a second MandateUse was never inserted.
    assert_eq!(f.count("SELECT COUNT(*) FROM mandate_uses"), 1);
    assert_eq!(
        f.count("SELECT COUNT(*) FROM execution_consumption_receipts"),
        1
    );
    assert!(f.ledger().conserves(), "{:?}", f.ledger());
}

#[test]
fn a_stale_fence_cannot_consume_an_execution_permit() {
    let (f, episode, c, binding_digest) = running("b3-act-fence");
    let act = f.authorized_act("a1", "model_egress", BROKER);
    let token = f.permit_token(&act.intent_id);

    // A superseded BYOM lease fence.
    let stale_byom = f.consume_permit_with(
        &token,
        &act,
        "p1",
        "e1",
        &act.stable_execution_key,
        BROKER,
        Some(&episode),
        c.byom_fence_epoch + 1,
        c.kovee_invocation_fence,
        act.revision,
        REQUEST_BYTES,
    );
    assert_eq!(kind_of(&stale_byom), "stale_revision", "{stale_byom}");
    assert!(stale_byom["problem"]["detail"]
        .as_str()
        .unwrap()
        .contains("byom_fence_epoch"));

    // A superseded HOST invocation fence — one current fence is not enough
    // (family contract L21).
    let stale_host = f.consume_permit_with(
        &token,
        &act,
        "p2",
        "e1",
        &act.stable_execution_key,
        BROKER,
        Some(&episode),
        c.byom_fence_epoch,
        c.kovee_invocation_fence + 1,
        act.revision,
        REQUEST_BYTES,
    );
    assert_eq!(kind_of(&stale_host), "stale_revision", "{stale_host}");
    assert!(stale_host["problem"]["detail"]
        .as_str()
        .unwrap()
        .contains("host_fence_epoch"));

    // An episode_ref naming an Episode that holds no lease at all: the
    // fence digest is byom's OWN committed value now (A8), so the binding
    // is located from the named Episode rather than pinned by a caller
    // echo — and an Episode with no lease head can lend no fences.
    let no_lease = f.consume_permit_with(
        &token,
        &act,
        "p3",
        "e1",
        &act.stable_execution_key,
        BROKER,
        Some("ep-nobody-holds-this"),
        c.byom_fence_epoch,
        c.kovee_invocation_fence,
        act.revision,
        REQUEST_BYTES,
    );
    assert_eq!(kind_of(&no_lease), "stale_revision", "{no_lease}");
    assert_eq!(
        f.count("SELECT COUNT(*) FROM execution_consumption_receipts"),
        0,
        "no receipt is minted behind a stale fence"
    );
    assert_eq!(f.count("SELECT COUNT(*) FROM mandate_uses"), 0);
    // Under the CURRENT fences the same consumption succeeds, and the fence
    // digest it publishes is byom's own committed binding — which is
    // exactly why there is no caller echo left to pin (A8).
    let ok = consume(&f, &act, &episode, &c, "p4");
    assert_eq!(ok["outcome"], "ok", "{ok}");
    assert_eq!(ok["result"]["episode_fence_digest"], binding_digest);
}

#[test]
fn a_wrong_class_subject_cannot_reach_the_model_egress_driver() {
    let (f, episode, c, _binding_digest) = running("b3-act-class");

    // (a) The class subject pins the EXACT provider binding: a broker with
    // another audience cannot consume the act.
    let act = f.authorized_act("a1", "model_egress", BROKER);
    let token = f.permit_token(&act.intent_id);
    let wrong_audience = f.consume_permit_with(
        &token,
        &act,
        "p1",
        "e1",
        &act.stable_execution_key,
        "kovee-other-broker",
        Some(&episode),
        c.byom_fence_epoch,
        c.kovee_invocation_fence,
        act.revision,
        REQUEST_BYTES,
    );
    assert_eq!(
        kind_of(&wrong_audience),
        "stale_binding",
        "{wrong_audience}"
    );

    // (b) An act whose `kind` is NOT one of the five Δ4 classes carries no
    // class subject at all, so it can never leave through a class-bound
    // driver. The preparation itself is honest about it.
    let open_kind = f.prepare_act_raw("a2", "legacy_tool_call", Some(BROKER));
    assert_eq!(kind_of(&open_kind), "forbidden", "{open_kind}");
    assert!(
        open_kind["problem"]["detail"]
            .as_str()
            .unwrap()
            .contains("act class"),
        "the Mandate bounds the act classes: {open_kind}"
    );

    // (c) A model-egress act with NO driver audience pins no provider
    // binding, and Δ4 makes `binding` mandatory: preparation fails closed.
    let no_binding = f.prepare_act_raw("a3", "model_egress", None);
    assert_eq!(kind_of(&no_binding), "policy_conflict", "{no_binding}");
    assert!(
        no_binding["problem"]["detail"]
            .as_str()
            .unwrap()
            .contains("binding"),
        "{no_binding}"
    );
    assert_eq!(
        f.count("SELECT COUNT(*) FROM execution_consumption_receipts"),
        0
    );
    let _ = portable_digest(0x01);
}

// ================= the R3 negatives (reviews/2026-07-26-r3-*) ============
//
// Each test below IS the review's own live probe, kept as a permanent
// negative. Every one of them failed against the code this bundle
// replaces — the failures are quoted in the bundle's report.

/// One authorized act plus its permit channel, and the consumption body a
/// probe mutates. The body carries the act's OWN authorized disclosure
/// pair, so a probe that changes it changes exactly one thing.
fn probe(f: &Fixture, tag: &str) -> (Act, String) {
    let act = f.authorized_act(tag, "model_egress", BROKER);
    let token = f.permit_token(&act.intent_id);
    (act, token)
}

fn probe_body(f: &Fixture, act: &Act, episode: &str, c: &Claim, key: &str) -> Value {
    f.consume_body(
        act,
        key,
        "probe-1",
        &act.stable_execution_key,
        BROKER,
        Some(episode),
        c.byom_fence_epoch,
        c.kovee_invocation_fence,
        act.revision,
        REQUEST_BYTES,
    )
}

/// The FROZEN membership of the act subject a gate seat assents to,
/// written out HERE rather than imported: the daemon composes its subject
/// against its own `ACT_SUBJECT_FIELDS` and fails closed on a missing
/// member, so dropping a pair from the projection alone breaks every act;
/// dropping it from the projection AND the frozen set breaks this list.
/// One of those two is what the R3 confirmation's mutation did, and the
/// disclosure test stayed green through it.
const ASSENTED_SUBJECT_MEMBERS: [&str; 18] = [
    "intent_id",
    "kind",
    "act_class_subject",
    "execution_kind",
    "subject_ref",
    "subject_revision",
    "requested_by_participant",
    "mandate_ref",
    "mandate_revision",
    "mandate_digest",
    "context_manifest_ref",
    "context_manifest_digest",
    "disclosure_manifest_ref",
    "disclosure_manifest_digest",
    "driver_audience",
    "budget_reservation_set_ref",
    "preconditions",
    "stable_execution_key",
];

/// R3-A01: the FIRST consumption cannot substitute a CONTEXT or a
/// DISCLOSURE. The review's probe consumed with a disclosure the act never
/// carried and the receipt published the caller's digest, so the authorized
/// manifest and the receipted one differed with nothing in the record
/// showing it; the confirmation then found the context pair unbound
/// altogether — never presented, never compared — and this test green even
/// with all four manifest members deleted from the assented subject.
#[test]
fn a_substituted_disclosure_cannot_consume_the_permit() {
    let (f, episode, c, _binding) = running("b3-act-disclosure");
    let (act, token) = probe(&f, "a1");

    // The lock the confirmation asked for: the authority a seat positions
    // on carries BOTH manifest pairs, and its membership is pinned here.
    assert_eq!(
        byomd::act_ops::ACT_SUBJECT_FIELDS,
        ASSENTED_SUBJECT_MEMBERS,
        "the assented act subject must pin both manifest ref/digest pairs: \
         a member that leaves this projection leaves the authority"
    );

    // (a) the same reference, DIFFERENT content — R3's exact probe.
    let mut swapped = probe_body(&f, &act, &episode, &c, "d1");
    swapped["disclosure_digest"] = portable_digest(0xd9);
    let reply = f.consume_signed(&token, &swapped);
    assert_eq!(kind_of(&reply), "stale_binding", "{reply}");
    assert!(
        reply["problem"]["detail"]
            .as_str()
            .unwrap()
            .contains("disclosure_digest"),
        "{reply}"
    );

    // (b) a different manifest entirely.
    let mut renamed = probe_body(&f, &act, &episode, &c, "d2");
    renamed["disclosure_manifest_ref"] = json!("disclosure-somebody-elses");
    let reply = f.consume_signed(&token, &renamed);
    assert_eq!(kind_of(&reply), "stale_binding", "{reply}");
    assert!(
        reply["problem"]["detail"]
            .as_str()
            .unwrap()
            .contains("disclosure_manifest_ref"),
        "{reply}"
    );

    // (c) the pair DROPPED: an act authorized against a disclosure is not
    // consumable without one.
    let mut dropped = probe_body(&f, &act, &episode, &c, "d3");
    let body = dropped.as_object_mut().unwrap();
    body.remove("disclosure_manifest_ref");
    body.remove("disclosure_digest");
    let reply = f.consume_signed(&token, &dropped);
    assert_eq!(kind_of(&reply), "stale_binding", "{reply}");

    // The SAME three probes against the CONTEXT pair, which the permit used
    // to carry no member for at all: an act authorized under one
    // ContextManifest is not consumable under another, and one that pins a
    // context is not consumable without presenting it.
    let mut swapped = probe_body(&f, &act, &episode, &c, "x1");
    swapped["context_digest"] = portable_digest(0xc9);
    let reply = f.consume_signed(&token, &swapped);
    assert_eq!(kind_of(&reply), "stale_binding", "{reply}");
    assert!(
        reply["problem"]["detail"]
            .as_str()
            .unwrap()
            .contains("context_digest"),
        "{reply}"
    );

    let mut renamed = probe_body(&f, &act, &episode, &c, "x2");
    renamed["context_manifest_ref"] = json!("context-somebody-elses");
    let reply = f.consume_signed(&token, &renamed);
    assert_eq!(kind_of(&reply), "stale_binding", "{reply}");
    assert!(
        reply["problem"]["detail"]
            .as_str()
            .unwrap()
            .contains("context_manifest_ref"),
        "{reply}"
    );

    let mut dropped = probe_body(&f, &act, &episode, &c, "x3");
    let body = dropped.as_object_mut().unwrap();
    body.remove("context_manifest_ref");
    body.remove("context_digest");
    let reply = f.consume_signed(&token, &dropped);
    assert_eq!(kind_of(&reply), "stale_binding", "{reply}");
    assert!(
        reply["problem"]["detail"]
            .as_str()
            .unwrap()
            .contains("context_manifest_ref"),
        "a consumption that presents no context for an act that pins one is \
         refused, not executed blind: {reply}"
    );

    // Nothing was consumed by any of them, and the honest consumption
    // publishes the COMMITTED digests: the receipt renders the disclosure
    // the act was authorized for, and byom's own ledger renders the
    // committed context pair (the receipt shape is frozen with the
    // consuming host, so the context binding is published there).
    assert_eq!(f.count("SELECT COUNT(*) FROM mandate_uses"), 0);
    let ok = f.consume_signed(&token, &probe_body(&f, &act, &episode, &c, "d4"));
    assert_eq!(ok["outcome"], "ok", "{ok}");
    assert_eq!(
        ok["result"]["disclosure_digest"],
        act_disclosure_digest(),
        "the receipt renders the committed disclosure, not the request's"
    );
    let consumed: Value = serde_json::from_str(
        &f.row(
            "SELECT payload FROM events WHERE kind = 'act-intent.consumed'
               AND object_ref = ?1",
            &act.intent_id,
        )
        .expect("the consumption event"),
    )
    .unwrap();
    assert_eq!(
        consumed["context_manifest_ref"],
        json!(act.context_manifest_ref),
        "the consumption renders the COMMITTED context binding: {consumed}"
    );
    assert_eq!(
        consumed["context_digest"],
        act_context_digest(),
        "and its committed digest: {consumed}"
    );
}

/// R3-A02: the permit is bound to one exact REGISTERED host Effect. The
/// review's probe consumed for a nonexistent, caller-chosen effect with an
/// arbitrary shaped digest.
#[test]
fn an_unregistered_or_different_host_effect_cannot_consume_the_permit() {
    let (f, episode, c, _binding) = running("b3-act-effect");
    let (act, token) = probe(&f, "a1");
    let body = probe_body(&f, &act, &episode, &c, "e1");

    // (a) UNREGISTERED: a caller-chosen effect ref and digest, with no
    // registration credential that covers them — R3's exact probe.
    let mut unregistered = body.clone();
    unregistered["host_effect_credential"] = json!("0".repeat(64));
    let reply = f.runtime(&token, &unregistered);
    assert_eq!(kind_of(&reply), "forbidden", "{reply}");
    assert!(
        reply["problem"]["detail"]
            .as_str()
            .unwrap()
            .contains("exact prepared host Effect"),
        "{reply}"
    );

    // (b) a DIFFERENT effect than the one registered: the credential is
    // the host's own, minted for effect A, and the request names B.
    let mut different = body.clone();
    different["host_effect_credential"] = json!(host_effect_credential(
        &token,
        &act.intent_id,
        &act.stable_execution_key,
        "kovee-effect-registered-a",
        &portable_digest(0xf7),
    ));
    different["host_effect_ref"] = json!("kovee-effect-b");
    let reply = f.runtime(&token, &different);
    assert_eq!(kind_of(&reply), "forbidden", "{reply}");

    // (c) the same effect ref with a DIFFERENT digest: the registration
    // covers the digest too, so a re-pointed effect record is refused.
    let mut repointed = body.clone();
    repointed["host_effect_credential"] = json!(host_effect_credential(
        &token,
        &act.intent_id,
        &act.stable_execution_key,
        body["host_effect_ref"].as_str().unwrap(),
        &portable_digest(0xf7),
    ));
    repointed["host_effect_digest"] = portable_digest(0x5a);
    let reply = f.runtime(&token, &repointed);
    assert_eq!(kind_of(&reply), "forbidden", "{reply}");

    // (d) a credential minted under ANOTHER act's permit token proves
    // nothing here.
    let other = f.authorized_act("a2", "model_egress", BROKER);
    let other_token = f.permit_token(&other.intent_id);
    let mut borrowed = body.clone();
    borrowed["host_effect_credential"] = json!(host_effect_credential(
        &other_token,
        &act.intent_id,
        &act.stable_execution_key,
        body["host_effect_ref"].as_str().unwrap(),
        &body["host_effect_digest"],
    ));
    let reply = f.runtime(&token, &borrowed);
    assert_eq!(kind_of(&reply), "forbidden", "{reply}");

    assert_eq!(
        f.count("SELECT COUNT(*) FROM execution_consumption_receipts"),
        0,
        "no receipt is minted for an Effect the host never registered"
    );
    assert_eq!(f.count("SELECT COUNT(*) FROM mandate_uses"), 0);
    // The registered Effect — same ref, same digest, host-minted credential
    // — consumes exactly once.
    let ok = f.consume_signed(&token, &body);
    assert_eq!(ok["outcome"], "ok", "{ok}");
}

/// R3-A03: finalization locks the EXACT active Position revisions, and the
/// consumption executes under those same slots. The review found an empty
/// position-reference list and synthesized seat descriptors.
#[test]
fn act_finalization_locks_the_exact_active_position_revisions() {
    let (f, episode, c, _binding) = running("b3-act-positions");
    let prepared = f.prepare_act_raw("a1", "model_egress", Some(BROKER));
    let r = &prepared["result"];
    let intent_id = r["intent_id"].as_str().unwrap().to_owned();
    let seat = r["required_seat_refs"][0].as_str().unwrap().to_owned();
    let subject_digest = r["subject_digest"].clone();
    let positioned = f.governance(&json!({
        "version": "0.2", "op": "act_intent_position",
        "meta": f.meta("actpos-a1", None),
        "proposal_ref": intent_id,
        "proposal_revision": 1,
        "subject_digest": subject_digest,
        "seat_ref": seat,
        "value": "assent",
    }));
    assert_eq!(positioned["outcome"], "ok", "{positioned}");
    let position_ref = positioned["result"]["position_id"]
        .as_str()
        .expect("the recorded PositionRevision")
        .to_owned();
    let position_digest: Value = serde_json::from_str(
        &f.row(
            "SELECT digest FROM position_revisions WHERE position_id = ?1",
            &position_ref,
        )
        .expect("the position record digest"),
    )
    .unwrap();

    let finalized = f.governance(&json!({
        "version": "0.2", "op": "act_intent_finalize",
        "meta": f.meta("actfin-a1", Some(1)),
        "intent_id": intent_id,
        "subject_digest": subject_digest,
    }));
    assert_eq!(finalized["outcome"], "ok", "{finalized}");

    // The published lock: the exact Position revision, its digest, and the
    // binding epoch it was cast at.
    let slots = &finalized["result"]["authorization_slot_snapshot"];
    assert_eq!(slots[0]["seat_ref"], json!(seat), "{slots}");
    assert_eq!(slots[0]["position_ref"], json!(position_ref), "{slots}");
    assert_eq!(slots[0]["position_digest"], position_digest, "{slots}");
    assert_eq!(slots[0]["value"], "assent", "{slots}");
    assert_eq!(
        slots[0]["participant_binding_epoch"],
        f.number(
            "SELECT binding_epoch FROM participants WHERE participant_id = ?1",
            slots[0]["participant_ref"].as_str().unwrap()
        )
        .map(|n| json!(n))
        .unwrap(),
        "the locked epoch is the participant's CURRENT one: {slots}"
    );

    // The GovernanceDecision itself names that Position revision — it used
    // to name none at all — and seats the actor that authored it.
    let refs = f
        .row(
            "SELECT position_refs FROM governance_decisions WHERE decision_id = ?1",
            &format!("dec-act-{intent_id}"),
        )
        .expect("the act authorization decision");
    assert_eq!(
        refs,
        json!([position_ref]).to_string(),
        "the decision locks the exact active Position revision"
    );
    // ... and it carries the position DIGESTS, not only the references: a
    // reference names which row existed, the digest is what ties the
    // authority to the immutable revision that carried it. The confirmation
    // found these only in the separate slot snapshot.
    let locks = f
        .row(
            "SELECT position_locks FROM governance_decisions WHERE decision_id = ?1",
            &format!("dec-act-{intent_id}"),
        )
        .expect("the act authorization decision");
    assert_eq!(
        serde_json::from_str::<Value>(&locks).unwrap(),
        json!([{
            "position_ref": position_ref,
            "position_revision": 1,
            "position_digest": position_digest,
        }]),
        "the decision must carry the exact position ref, revision AND digest"
    );
    let snapshot = f
        .row(
            "SELECT seat_snapshot FROM governance_decisions WHERE decision_id = ?1",
            &format!("dec-act-{intent_id}"),
        )
        .expect("the decision's slot snapshot");
    let snapshot: Value = serde_json::from_str(&snapshot).unwrap();
    assert_eq!(
        snapshot[0]["actor_ref"],
        json!("governance:sovereign"),
        "the seat carries the actor that AUTHORED the position: {snapshot}"
    );

    // And the consumption re-derives the slot snapshot from the CURRENT
    // active positions: it executes under exactly the locked slots.
    let act = Act {
        intent_id: intent_id.clone(),
        seat_ref: seat,
        subject_digest,
        intent_digest: f.intent_digest(&intent_id),
        stable_execution_key: r["stable_execution_key"].as_str().unwrap().to_owned(),
        budget_reservation_set_ref: r["budget_reservation_set_ref"].as_str().unwrap().to_owned(),
        revision: finalized["result"]["revision"].as_u64().unwrap(),
        context_manifest_ref: act_context_ref("a1"),
        context_digest: act_context_digest(),
        disclosure_manifest_ref: act_disclosure_ref("a1"),
        disclosure_digest: act_disclosure_digest(),
    };
    let ok = consume(&f, &act, &episode, &c, "p1");
    assert_eq!(ok["outcome"], "ok", "{ok}");

    // -- the negatives the lock exists FOR ------------------------------
    // Each of these is a state change this slice has no operation for
    // (positions close at authorization, and nothing rebinds a principal
    // here), so each is made in the store and then answered on the wire.

    // (a) SUPERSESSION at consumption. The seat's assent is appended as a
    // new immutable revision — same seat, same value, same subject, same
    // binding epoch, so every check but the snapshot still passes. The act
    // may not execute under slots its authorization never locked.
    let superseded_act = f.authorized_act("a2", "model_egress", BROKER);
    let superseded_position = f
        .row(
            "SELECT position_ref FROM position_seat_heads WHERE proposal_ref = ?1",
            &superseded_act.intent_id,
        )
        .expect("the act's seat head");
    f.supersede_position(&superseded_position, "pos-superseding-a2");
    let reply = consume(&f, &superseded_act, &episode, &c, "p2");
    assert_eq!(kind_of(&reply), "stale_binding", "{reply}");
    assert!(
        reply["problem"]["detail"]
            .as_str()
            .unwrap()
            .contains("no longer the ones this act's authorization locked"),
        "a superseded position invalidates the permit: {reply}"
    );

    // (b) A REBOUND principal at consumption, and (c) at finalization. The
    // pending act is positioned FIRST, so one rebinding fences both: a
    // changed binding epoch invalidates the position instead of silently
    // recasting it (DESIGN.md §1106).
    let rebound_act = f.authorized_act("a3", "model_egress", BROKER);
    let pending = f.prepare_act_raw("a4", "model_egress", Some(BROKER));
    let pending_id = pending["result"]["intent_id"].as_str().unwrap().to_owned();
    let pending_subject = pending["result"]["subject_digest"].clone();
    let pending_positioned = f.governance(&json!({
        "version": "0.2", "op": "act_intent_position",
        "meta": f.meta("actpos-a4", None),
        "proposal_ref": pending_id,
        "proposal_revision": 1,
        "subject_digest": pending_subject,
        "seat_ref": pending["result"]["required_seat_refs"][0],
        "value": "assent",
    }));
    assert_eq!(pending_positioned["outcome"], "ok", "{pending_positioned}");
    let seat_participant = f
        .row(
            "SELECT participant_ref FROM position_seat_heads WHERE proposal_ref = ?1",
            &pending_id,
        )
        .or_else(|| {
            f.row(
                "SELECT participant_ref FROM position_revisions WHERE proposal_ref = ?1",
                &pending_id,
            )
        })
        .expect("the seat's participant");
    let epoch = f.rebind_participant(&seat_participant);

    let reply = consume(&f, &rebound_act, &episode, &c, "p3");
    assert_eq!(kind_of(&reply), "decision_incomplete", "{reply}");
    let detail = reply["problem"]["detail"].as_str().unwrap();
    assert!(
        detail.contains("binding epoch") && detail.contains(&epoch.to_string()),
        "a rebound principal invalidates the permit it positioned, and the \
         refusal names the epoch that moved: {reply}"
    );
    let refused = f.governance(&json!({
        "version": "0.2", "op": "act_intent_finalize",
        "meta": f.meta("actfin-a4", Some(1)),
        "intent_id": pending_id,
        "subject_digest": pending_subject,
    }));
    assert_eq!(kind_of(&refused), "decision_incomplete", "{refused}");
    let detail = refused["problem"]["detail"].as_str().unwrap();
    assert!(
        detail.contains("binding epoch") && detail.contains(&epoch.to_string()),
        "finalization names the epoch that moved: {refused}"
    );
    assert!(
        f.row(
            "SELECT authorization_decision_ref FROM act_intents WHERE intent_id = ?1",
            &pending_id
        )
        .is_none(),
        "and no decision was formed over the invalidated position"
    );
    assert_eq!(
        f.count("SELECT COUNT(*) FROM execution_consumption_receipts"),
        1,
        "only the honest consumption minted a receipt"
    );
}

/// R3-A04: a CHANGED consumed request never replays. The review changed
/// the disclosure pair and received `ok`, `replayed: true` with the old
/// receipt. Every substantive member is mutated here, one at a time.
#[test]
fn every_changed_member_of_a_consumed_request_conflicts() {
    let (f, episode, c, _binding) = running("b3-act-replay");
    let (act, token) = probe(&f, "a1");
    let first = f.consume_signed(&token, &probe_body(&f, &act, &episode, &c, "r1"));
    assert_eq!(first["outcome"], "ok", "{first}");

    // The exact request replays byte-identically — the crash-recovery path.
    let mut again = probe_body(&f, &act, &episode, &c, "r2");
    again["meta"]["expected_revision"] = json!(act.revision + 1);
    let replay = f.consume_signed(&token, &again);
    assert_eq!(replay["outcome"], "ok", "{replay}");
    assert_eq!(replay["result"]["replayed"], true);
    assert_eq!(
        replay["result"]["receipt_id"],
        first["result"]["receipt_id"]
    );

    // The comparison is against a STORED commitment, not a second
    // recomputation: the frozen semantic-request digest byom wrote when it
    // consumed. Deleting that emission — from the receipt it is stored on,
    // or from the consumption event that publishes it — is what this test
    // has to notice, because a recomputed "committed" side only ever proves
    // that today's rebuild equals today's rebuild.
    let stored_digest: Value = serde_json::from_str(
        &f.row(
            "SELECT semantic_request_digest FROM execution_consumption_receipts
               WHERE stable_execution_key = ?1",
            &act.stable_execution_key,
        )
        .expect("the receipt stores the frozen semantic-request digest"),
    )
    .unwrap();
    assert_eq!(
        stored_digest["class"], "local_erasure_safe",
        "byom's own commitment to the request it honored: {stored_digest}"
    );
    assert_eq!(
        stored_digest["value_hex"].as_str().map(str::len),
        Some(64),
        "{stored_digest}"
    );
    let consumed: Value = serde_json::from_str(
        &f.row(
            "SELECT payload FROM events WHERE kind = 'act-intent.consumed'
               AND object_ref = ?1",
            &act.intent_id,
        )
        .expect("the consumption event"),
    )
    .unwrap();
    assert_eq!(
        consumed["semantic_request_digest"], stored_digest,
        "the event publishes the SAME frozen digest the receipt stores: {consumed}"
    );

    // The mutation matrix: EVERY substantive member, one at a time.
    let mutations: Vec<(&str, Value)> = vec![
        ("host_effect_ref", json!("kovee-effect-other")),
        ("host_effect_digest", portable_digest(0x0f)),
        ("context_manifest_ref", json!("context-other")),
        ("context_digest", portable_digest(0xc9)),
        ("disclosure_manifest_ref", json!("disclosure-other")),
        ("disclosure_digest", portable_digest(0xd9)),
        ("driver_audience", json!("kovee-other-broker")),
        ("budget_reservation_set_ref", json!("rset-other")),
        ("episode_ref", json!("ep-other")),
        ("byom_fence_epoch", json!(c.byom_fence_epoch + 1)),
        ("host_fence_epoch", json!(c.kovee_invocation_fence + 1)),
    ];
    for (member, value) in mutations {
        let mut changed = probe_body(&f, &act, &episode, &c, &format!("r-{member}"));
        changed["meta"]["expected_revision"] = json!(act.revision + 1);
        changed[member] = value;
        let reply = f.consume_signed(&token, &changed);
        assert_eq!(
            kind_of(&reply),
            "idempotency_mismatch",
            "a consumed request with a changed {member} replayed: {reply}"
        );
        assert!(
            reply["problem"]["detail"]
                .as_str()
                .unwrap()
                .contains(member),
            "the refusal names the member that changed: {reply}"
        );
    }
    // Dropping either optional manifest pair is a change too.
    for (name, pair) in [
        (
            "r-drop-disclosure",
            ["disclosure_manifest_ref", "disclosure_digest"],
        ),
        ("r-drop-context", ["context_manifest_ref", "context_digest"]),
    ] {
        let mut dropped = probe_body(&f, &act, &episode, &c, name);
        dropped["meta"]["expected_revision"] = json!(act.revision + 1);
        let body = dropped.as_object_mut().unwrap();
        for member in pair {
            body.remove(member);
        }
        let reply = f.consume_signed(&token, &dropped);
        assert_eq!(kind_of(&reply), "idempotency_mismatch", "{name}: {reply}");
    }

    assert_eq!(
        f.count("SELECT COUNT(*) FROM execution_consumption_receipts"),
        1,
        "not one of the mutations minted a receipt"
    );
    assert_eq!(f.count("SELECT COUNT(*) FROM mandate_uses"), 1);
    assert!(f.ledger().conserves(), "{:?}", f.ledger());
}

/// R3-L01 / D-R3-3: A8 holds in BOTH directions on this request — checked
/// against the frozen schema, and against the daemon itself.
const CONSUME_REQUEST_SCHEMA: &str =
    include_str!("../../../spec/schemas/ops/execution-permit-consume-request.schema.json");
const PREPARE_REQUEST_SCHEMA: &str =
    include_str!("../../../spec/schemas/ops/act-intent-prepare-request.schema.json");

fn class_of(schema: &Value, member: &str) -> String {
    let def = schema["properties"][member]["$ref"]
        .as_str()
        .unwrap_or_default()
        .strip_prefix("#/$defs/")
        .unwrap_or_default()
        .to_owned();
    schema["$defs"][&def]["properties"]["class"]["const"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

#[test]
fn the_act_family_request_shapes_obey_a8_in_both_directions() {
    let consume: Value = serde_json::from_str(CONSUME_REQUEST_SCHEMA).unwrap();
    let prepare: Value = serde_json::from_str(PREPARE_REQUEST_SCHEMA).unwrap();
    // The converse half: a digest byom recomputes from its OWN committed
    // state is not a request member at all.
    for owned in ["intent_digest", "subject_digest", "episode_fence_digest"] {
        assert!(
            consume["properties"].get(owned).is_none(),
            "{owned} is byom's own recomputed digest: it must not be a request member (A8)"
        );
    }
    // The demanded half: a peer-owned digest byom must verify travels as a
    // frozen portable_public fragment.
    for peer in ["host_effect_digest", "disclosure_digest"] {
        assert_eq!(
            class_of(&consume, peer),
            "portable_public",
            "{peer} is the host's own value byom cannot derive (A8)"
        );
    }
    for peer in ["context_manifest_digest", "disclosure_manifest_digest"] {
        assert_eq!(
            class_of(&prepare, peer),
            "portable_public",
            "{peer} is the host's own manifest, compared again at consumption (A8)"
        );
    }
    // byom's own authority subject stays keyed where it IS byom's to
    // verify: the finalize CAS pin is not a cross-boundary demand.
    assert_eq!(class_of(&prepare, "mandate_digest"), "local_erasure_safe");
}

#[test]
fn the_daemon_refuses_a_request_that_echoes_byoms_own_digests() {
    let (f, episode, c, binding) = running("b3-act-a8");
    let (act, token) = probe(&f, "a1");
    for (member, value) in [
        ("intent_digest", act.intent_digest.clone()),
        ("subject_digest", act.subject_digest.clone()),
        ("episode_fence_digest", binding.clone()),
    ] {
        let mut echoed = probe_body(&f, &act, &episode, &c, "a8");
        echoed[member] = value;
        let reply = f.consume_signed(&token, &echoed);
        assert_eq!(
            kind_of(&reply),
            "invalid",
            "echoing byom's own {member} must fail the closed shape, not be \
             quietly ignored: {reply}"
        );
    }
    // And a host digest offered as a keyed value byom could never derive is
    // a class mismatch, not a silent acceptance.
    let mut keyed = probe_body(&f, &act, &episode, &c, "a8-class");
    keyed["host_effect_digest"] = test_digest(0xf7);
    let reply = f.consume_signed(&token, &keyed);
    assert_eq!(kind_of(&reply), "invalid", "{reply}");
}

/// **R3-L01, reproduced.** The registration credential (R3-A02) proves WHO
/// sent `host_effect_digest`. It never proved WHAT the digest is the digest
/// of: byom authenticated a tuple that CONTAINED the value and then stored
/// whatever it was handed. It "explicitly does not hold the kovee row", so
/// the digest was asserted, not verified against anything both sides hold.
///
/// D-R3-3 requires a peer-owned digest byom must verify to travel as a frozen
/// `portable_public` fragment whose members byom holds — the shape kovee
/// already consumes for byom's parent budget. byom now REBUILDS that fragment
/// out of its own committed ActIntent plus the two host-owned members, and
/// re-derives the digest.
///
/// The vector is kovee's OWN recording, so the verifier does not derive its
/// own expectation. That was exactly the L02 weakness, applied here from the
/// start.
#[test]
fn the_host_effect_digest_must_re_derive_from_the_frozen_binding_fragment() {
    const VECTOR: &str = include_str!("vectors/kovee-host-effect-binding.json");

    // -- the pinned KOVEE vector: byom's rebuild reproduces it ------------
    let vector: Value = serde_json::from_str(VECTOR).expect("the pinned kovee vector parses");
    assert_eq!(
        vector["owner"], "kovee",
        "this vector is kovee's, not byom's"
    );
    let i = &vector["inputs"];
    let (digest, key, fragment) = host_effect_binding(
        i["host_effect_ref"].as_str().unwrap(),
        i["intent_ref"].as_str().unwrap(),
        i["stable_execution_key"].as_str().unwrap(),
        i["context_manifest_ref"].as_str().unwrap(),
        &i["context_digest"],
        i["disclosure_manifest_ref"].as_str().unwrap(),
        &i["disclosure_digest"],
        i["final_provider_request_typed_byte_digest"]
            .as_str()
            .unwrap(),
    );
    assert_eq!(
        fragment, vector["fragment"],
        "byom's rebuild is not kovee's recorded fragment: the two sides no \
         longer agree on the frozen member set or its canonicalization"
    );
    assert_eq!(
        digest["value_hex"],
        vector["host_effect_digest"]["value_hex"]
    );
    assert_eq!(digest["class"], "portable_public", "unkeyed by A8");
    assert_eq!(key, vector["fragment"]["external_idempotency_key"]);

    // -- and the LIVE daemon refuses a digest that does not re-derive -----
    let (f, episode, c, _binding) = running("b3-act-binding");
    let (act, token) = probe(&f, "a1");
    let body = probe_body(&f, &act, &episode, &c, "e1");

    // (a) a well-formed `portable_public` digest of NOTHING, registered by
    // the host under its own permit token. R3's probe, with the credential
    // check satisfied: it used to be stored and republished on the receipt.
    let mut asserted = body.clone();
    asserted["host_effect_digest"] = portable_digest(0x5a);
    sign_host_effect(&token, &mut asserted);
    let reply = f.runtime(&token, &asserted);
    assert_eq!(kind_of(&reply), "forbidden", "{reply}");
    assert!(
        reply["problem"]["detail"]
            .as_str()
            .unwrap()
            .contains("kovee-host-effect-binding-v1"),
        "the refusal names the fragment it could not re-derive: {reply}"
    );

    // (b) the digest of a fragment over a DIFFERENT effect reference: the
    // members byom holds are its own, so a host cannot move the effect.
    let (other_digest, other_key, _) = host_effect_binding(
        "kovee-effect-elsewhere",
        &act.intent_id,
        &act.stable_execution_key,
        &act.context_manifest_ref,
        &act.context_digest,
        &act.disclosure_manifest_ref,
        &act.disclosure_digest,
        REQUEST_BYTES,
    );
    let mut moved = body.clone();
    moved["host_effect_digest"] = other_digest;
    moved["host_effect_external_idempotency_key"] = json!(other_key);
    sign_host_effect(&token, &mut moved);
    assert_eq!(kind_of(&f.runtime(&token, &moved)), "forbidden");

    // (c) the digest of a fragment over a context the act was NOT
    // authorized under. The presented pair still matches the committed one,
    // so only the REBUILD catches this.
    let (wrong_ctx, _, _) = host_effect_binding(
        body["host_effect_ref"].as_str().unwrap(),
        &act.intent_id,
        &act.stable_execution_key,
        "context-somebody-elses",
        &portable_digest(0x11),
        &act.disclosure_manifest_ref,
        &act.disclosure_digest,
        REQUEST_BYTES,
    );
    let mut wrong = body.clone();
    wrong["host_effect_digest"] = wrong_ctx;
    sign_host_effect(&token, &mut wrong);
    assert_eq!(kind_of(&f.runtime(&token, &wrong)), "forbidden");

    // (d) the two host-owned members are tied to each other and to byom's
    // one-shot key: an idempotency key that is not
    // `kovee-model-{key}-{bytes[..16]}` is refused before anything else.
    let mut untied = body.clone();
    untied["host_effect_external_idempotency_key"] = json!("kovee-model-something-else");
    sign_host_effect(&token, &mut untied);
    let reply = f.runtime(&token, &untied);
    assert_eq!(kind_of(&reply), "forbidden", "{reply}");
    assert!(
        reply["problem"]["detail"]
            .as_str()
            .unwrap()
            .contains("host_effect_external_idempotency_key must be"),
        "{reply}"
    );

    assert_eq!(
        f.count("SELECT COUNT(*) FROM execution_consumption_receipts"),
        0,
        "no receipt is minted for a digest byom could not derive"
    );
    // The derived digest — the one the host really computed over the frozen
    // fragment — consumes exactly once.
    let ok = f.consume_signed(&token, &body);
    assert_eq!(ok["outcome"], "ok", "{ok}");
}
