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
    consume_disclosure_digest, portable_digest, Act, Claim, Fixture, Subordinate,
};
use common::{kind_of, test_digest};
use serde_json::{json, Value};

const BROKER: &str = "kovee-model-broker";

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

fn consume(
    f: &Fixture,
    act: &Act,
    episode: &str,
    binding_digest: &Value,
    c: &Claim,
    key: &str,
) -> Value {
    let token = f.permit_token(&act.intent_id);
    f.consume_permit_with(
        &token,
        act,
        key,
        "e1",
        &act.stable_execution_key,
        BROKER,
        Some((episode, binding_digest)),
        c.byom_fence_epoch,
        c.kovee_invocation_fence,
        act.revision,
        test_digest(0xf7),
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
    let consumed = consume(&f, &act, &episode, &binding_digest, &c, "p1");
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
    // The four ECHO digests, each compared against an INDEPENDENT source —
    // byom's committed row, or the exact value the request carried — never
    // against the receipt itself.
    assert_eq!(
        receipt["intent_digest"],
        f.intent_digest(&intent_id),
        "intent_digest is the committed ActIntent record digest"
    );
    assert_eq!(
        receipt["subject_digest"], subject_digest,
        "subject_digest is the exact authorized act subject the caller pinned"
    );
    assert_eq!(
        receipt["disclosure_digest"],
        consume_disclosure_digest(),
        "disclosure_digest is the exact manifest digest the consumption bound"
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
    let replayed = consume(&f, &act, &episode, &binding_digest, &c, "p1");
    assert_eq!(
        replayed, consumed,
        "the exact retry replays byte-identically"
    );
}

#[test]
fn a_consumption_without_an_authorizing_decision_is_refused() {
    let (f, episode, c, binding_digest) = running("b3-act-nopermit");
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
        Some((&episode, &binding_digest)),
        c.byom_fence_epoch,
        c.kovee_invocation_fence,
        1,
        test_digest(0xf7),
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
        Some((&episode, &binding_digest)),
        c.byom_fence_epoch,
        c.kovee_invocation_fence,
        1,
        test_digest(0xf7),
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
        Some((&episode, &binding_digest)),
        c.byom_fence_epoch,
        c.kovee_invocation_fence,
        1,
        test_digest(0xf7),
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
    let (f, episode, c, binding_digest) = running("b3-act-spent");
    let act = f.authorized_act("a1", "model_egress", BROKER);
    let first = consume(&f, &act, &episode, &binding_digest, &c, "p1");
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
        Some((&episode, &binding_digest)),
        c.byom_fence_epoch,
        c.kovee_invocation_fence,
        act.revision + 1,
        test_digest(0xf7),
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
        Some((&episode, &binding_digest)),
        c.byom_fence_epoch,
        c.kovee_invocation_fence,
        act.revision + 1,
        test_digest(0xf7),
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
        Some((&episode, &binding_digest)),
        c.byom_fence_epoch,
        c.kovee_invocation_fence,
        act.revision + 1,
        test_digest(0x0f),
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
        Some((&episode, &binding_digest)),
        c.byom_fence_epoch + 1,
        c.kovee_invocation_fence,
        act.revision,
        test_digest(0xf7),
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
        Some((&episode, &binding_digest)),
        c.byom_fence_epoch,
        c.kovee_invocation_fence + 1,
        act.revision,
        test_digest(0xf7),
    );
    assert_eq!(kind_of(&stale_host), "stale_revision", "{stale_host}");
    assert!(stale_host["problem"]["detail"]
        .as_str()
        .unwrap()
        .contains("host_fence_epoch"));

    // A fence digest pinning some other binding.
    let stale_digest = f.consume_permit_with(
        &token,
        &act,
        "p3",
        "e1",
        &act.stable_execution_key,
        BROKER,
        Some((&episode, &test_digest(0x11))),
        c.byom_fence_epoch,
        c.kovee_invocation_fence,
        act.revision,
        test_digest(0xf7),
    );
    assert_eq!(kind_of(&stale_digest), "stale_binding", "{stale_digest}");
    assert_eq!(
        f.count("SELECT COUNT(*) FROM execution_consumption_receipts"),
        0,
        "no receipt is minted behind a stale fence"
    );
    assert_eq!(f.count("SELECT COUNT(*) FROM mandate_uses"), 0);
}

#[test]
fn a_wrong_class_subject_cannot_reach_the_model_egress_driver() {
    let (f, episode, c, binding_digest) = running("b3-act-class");

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
        Some((&episode, &binding_digest)),
        c.byom_fence_epoch,
        c.kovee_invocation_fence,
        act.revision,
        test_digest(0xf7),
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
