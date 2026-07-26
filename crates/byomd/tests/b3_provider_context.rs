//! B3 slice 3 — the §12.1 / §16.6-item-5 ProviderContextManifest byom
//! source fields (Δ5).
//!
//! Byom supplies Kovee an EXACT source-field set and nothing else; Kovee
//! alone owns the ProviderContextManifest and the final provider-visible
//! ordering and bytes. This suite pins:
//!
//! - the fragment is exactly the frozen 17-member set, no more, no fewer;
//! - `context_source_digest` is byom's digest over exactly that canonical
//!   fragment, and it is what `ByomEpisodeBinding.context_source_digest`
//!   carries;
//! - the read is REFUSED when the Episode/attempt/context refs do not match
//!   — possession of a manifest grants nothing (§12.1).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::kind_of;
use common::runtime::{merge, Claim, Fixture, Subordinate};
use serde_json::{json, Value};

/// The frozen required member set of
/// `spec/governed-work/provider-context-manifest-byom-fields.schema.json`,
/// transcribed verbatim (sorted for comparison).
const FROZEN_FIELDS: [&str; 17] = [
    "activity_stream_ref",
    "authorization_dependency_digest",
    "byom_attempt_ref",
    "byom_endpoint_ref",
    "byom_fence_epoch",
    "classification_overlay_digest",
    "context_manifest_digest",
    "context_manifest_ref",
    "disclosure_ceiling_ref",
    "episode_ref",
    "explicit_omissions",
    "mandate_use_refs",
    "ordered_source_items",
    "participant_binding_epoch",
    "participant_ref",
    "purpose_ref",
    "society_ref",
];

fn claimed(tag: &str) -> (Fixture, String, Claim) {
    let f = Fixture::start(tag, 8);
    let wake = f.wake("w1");
    let ep = f.request_episode(&wake, "e1");
    f.admit_placement(&ep, "p1", Subordinate::Confirmed(200));
    let c = f.claim(&ep.episode_id, "worker-a", 600, 7, "c1");
    (f, ep.episode_id, c)
}

fn show(f: &Fixture, episode: &str, attempt: &str, manifest: &str) -> Value {
    f.daemon.call(
        "projection",
        &json!({
            "version": "0.2", "op": "context_manifest_show",
            "episode_ref": episode,
            "byom_attempt_ref": attempt,
            "context_manifest_ref": manifest,
        }),
    )
}

#[test]
fn the_byom_source_fields_are_exactly_the_frozen_set() {
    let (f, episode, c) = claimed("b3-pcm-fields");

    // The claim already returns byom's own derivation.
    let reply = show(&f, &episode, &c.attempt_ref, "ctxman-1");
    assert_eq!(reply["outcome"], "ok", "{reply}");
    let fields = &reply["result"]["provider_context_manifest_byom_fields"];
    let mut present: Vec<&str> = fields
        .as_object()
        .expect("the fragment is an object")
        .keys()
        .map(String::as_str)
        .collect();
    present.sort_unstable();
    assert_eq!(
        present,
        FROZEN_FIELDS.to_vec(),
        "the fragment is EXACTLY the frozen C2 member set: no convenience \
         context may be appended outside Kovee's final manifest chain (§12.1)"
    );

    // Every member is bound to committed state.
    assert_eq!(fields["episode_ref"], json!(episode));
    assert_eq!(fields["byom_attempt_ref"], json!(c.attempt_ref));
    assert_eq!(fields["byom_fence_epoch"], json!(c.byom_fence_epoch));
    assert_eq!(fields["participant_ref"], "part-agent-1");
    assert_eq!(fields["society_ref"], json!(f.society_id));
    assert_eq!(fields["activity_stream_ref"], json!(f.stream));
    assert_eq!(fields["purpose_ref"], "purpose-explore-1");
    assert_eq!(fields["context_manifest_ref"], "ctxman-1");
    assert_eq!(
        fields["explicit_omissions"],
        json!([]),
        "an omission is explicit or absent, never silent (§12.1)"
    );
    assert!(!fields["ordered_source_items"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(fields["mandate_use_refs"], json!(["muse-1"]));

    // The digest is over EXACTLY this fragment, in the portable class a
    // peer can re-derive without a byom secret.
    let digest = &reply["result"]["context_source_digest"];
    assert_eq!(digest["class"], "portable_public");
    assert_eq!(digest["algorithm"], "sha-256");
    assert_eq!(
        &reply["result"]["byom_episode_binding_context_source_digest"], digest,
        "ByomEpisodeBinding.context_source_digest IS the digest over exactly \
         this canonical fragment (§16.6 item 5)"
    );
    // And the same value comes back on the claim itself, so Kovee binds
    // byom's derivation rather than its own echo.
    assert_eq!(
        c.binding["context_source_digest_recomputed"], *digest,
        "the claim records byom's own derivation"
    );
    assert_eq!(
        c.binding["provider_context_manifest_byom_fields"], *fields,
        "the claim carries the same fragment the read projects"
    );
    // Kovee's ownership is stated, not implied.
    assert!(reply["result"]["owner"]
        .as_str()
        .unwrap()
        .contains("kovee owns the ProviderContextManifest"));
}

#[test]
fn the_source_fields_are_refused_when_the_episode_or_context_refs_do_not_match() {
    let (f, episode, c) = claimed("b3-pcm-refuse");

    // A context manifest the Episode does not carry.
    let wrong_manifest = show(&f, &episode, &c.attempt_ref, "ctxman-someone-elses");
    assert_eq!(
        kind_of(&wrong_manifest),
        "stale_binding",
        "{wrong_manifest}"
    );
    assert!(wrong_manifest["problem"]["detail"]
        .as_str()
        .unwrap()
        .contains("committed ContextManifest"));

    // An attempt that binds no committed binding for this Episode.
    let wrong_attempt = show(&f, &episode, "att-someone-elses", "ctxman-1");
    assert_eq!(kind_of(&wrong_attempt), "not_found", "{wrong_attempt}");

    // An Episode that does not exist — non-enumerating.
    let wrong_episode = show(&f, "ep-nope", &c.attempt_ref, "ctxman-1");
    assert_eq!(kind_of(&wrong_episode), "not_found", "{wrong_episode}");

    // A SUPERSEDED attempt materializes nothing: the holder yields, a
    // successor claims, and the old attempt's fragment is refused even
    // though the refs once matched.
    let worker = f.worker_token(&episode);
    let started = f.start_episode(&episode, &c, "s-pcm");
    assert_eq!(started["outcome"], "ok", "{started}");
    let running = Claim {
        lease_revision: started["result"]["lease_revision"].as_u64().unwrap(),
        ..c.clone()
    };
    let mut yielded = json!({
        "version": "0.2", "op": "episode_yield",
        "meta": f.meta("yld-pcm", Some(running.lease_revision)),
        "target_state": "waiting",
    });
    merge(&mut yielded, f.fences(&episode, &running));
    let yielded = f.runtime(&worker, &yielded);
    assert_eq!(yielded["outcome"], "ok", "{yielded}");
    let second = f.claim(&episode, "worker-b", 600, 8, "c2");
    assert_ne!(second.attempt_ref, c.attempt_ref);
    let fenced = show(&f, &episode, &c.attempt_ref, "ctxman-1");
    assert_eq!(kind_of(&fenced), "stale_binding", "{fenced}");
    assert!(fenced["problem"]["detail"]
        .as_str()
        .unwrap()
        .contains("fenced or released"));
    // The successor attempt has its own fragment, at its own fence.
    let successor = show(&f, &episode, &second.attempt_ref, "ctxman-1");
    assert_eq!(successor["outcome"], "ok", "{successor}");
    assert_eq!(
        successor["result"]["provider_context_manifest_byom_fields"]["byom_fence_epoch"],
        json!(second.byom_fence_epoch)
    );
}

#[test]
fn the_episode_context_manifest_is_immutable_across_attempts() {
    let (f, episode, c) = claimed("b3-pcm-immutable");
    // A re-claim naming a DIFFERENT ContextManifest is refused: a new
    // manifest requires a new Episode, never a silent substitution (§12.1).
    let token = f.worker_token(&episode);
    let started = f.start_episode(&episode, &c, "s-imm");
    assert_eq!(started["outcome"], "ok", "{started}");
    let running = Claim {
        lease_revision: started["result"]["lease_revision"].as_u64().unwrap(),
        ..c.clone()
    };
    let mut yielded = json!({
        "version": "0.2", "op": "episode_yield",
        "meta": f.meta("yld-imm", Some(running.lease_revision)),
        "target_state": "waiting",
    });
    merge(&mut yielded, f.fences(&episode, &running));
    let yielded = f.runtime(&token, &yielded);
    assert_eq!(yielded["outcome"], "ok", "{yielded}");
    let substituted = f.runtime(
        &token,
        &json!({
            "version": "0.2", "op": "episode_claim",
            "meta": f.meta("clm-sub", None),
            "episode_ref": episode,
            "generation": 1,
            "holder_runtime_binding": "worker-b",
            "claim_subject_digest": common::test_digest(0xd1),
            "lease_ttl_seconds": 600,
            "kovee_invocation_ref": "kovee-inv-sub",
            "kovee_invocation_fence": 9,
            "stable_binding_key": "bindkey-sub",
            "context_manifest_ref": "ctxman-substituted",
            "context_manifest_digest": common::test_digest(0xd2),
            "context_source_digest": common::runtime::portable_digest(0xd3),
            "mandate_use_refs": ["muse-1"],
            "allowed_local_commitments": ["kovee_local_note"],
        }),
    );
    assert_eq!(kind_of(&substituted), "stale_binding", "{substituted}");
    assert!(substituted["problem"]["detail"]
        .as_str()
        .unwrap()
        .contains("immutable"));
    // The committed manifest is untouched.
    assert_eq!(
        f.row(
            "SELECT context_manifest_ref FROM episodes WHERE episode_id = ?1",
            &episode
        ),
        Some("ctxman-1".to_owned())
    );
    let _ = c;
}
