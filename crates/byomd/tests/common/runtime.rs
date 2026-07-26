//! Shared fixture for the B3 slice-2 runtime suites: one Society, one
//! admitted agent Participant with an issued Mandate and an open
//! exploration ActivityStream — the state the FOUR-STAGE activation
//! starts from — plus thin wrappers for each stage so a test reads as the
//! sequence it is asserting.
//!
//! What a test writes:
//! ```ignore
//! let f = Fixture::start("b3-episode", 8);
//! let wake = f.wake("w1");
//! let ep = f.request_episode(&wake, "e1");        // stages 1-3
//! f.admit_placement(&ep, "p1", Confirmed(200));  // stage 4 + queue
//! let claim = f.claim(&ep, "worker-a", 300, 7, "b1");
//! ```

#![allow(dead_code)]

use serde_json::{json, Value};

use super::{
    far_future, manifestation_decision, meta, offer_decision, read_candidate_token,
    read_participant_token, sovereign_id, test_digest, TestDaemon,
};

pub struct Fixture {
    pub daemon: TestDaemon,
    pub incarnation: String,
    pub society_id: String,
    pub agent_token: String,
    pub mandate_id: String,
    /// The exact prepared Mandate subject digest an act must pin.
    pub mandate_subject_digest: Value,
    /// The Mandate's CURRENT revision (issue advanced it): an act pins it.
    pub mandate_revision: u64,
    pub stream: String,
    pub tag: String,
}

/// The disclosure manifest an act is PREPARED with — the pair the gate
/// seat assents to. The consumption presents exactly this pair: the helper
/// used to prepare `disclosure-{key}`/`0xe2` and then consume a different
/// manifest entirely, which is how the substitution defect stayed green
/// (R3-A01).
pub fn act_disclosure_ref(key: &str) -> String {
    format!("disclosure-{key}")
}

/// Its digest: the HOST's own object, `portable_public` so both sides hold
/// the same bytes and byom can compare the presented pair with the
/// assented one (A8, PROFILE.md §6.2).
pub fn act_disclosure_digest() -> Value {
    portable_digest(0xe2)
}

/// One prepared-and-authorized act, with the refs its consumption needs.
#[derive(Debug, Clone)]
pub struct Act {
    pub intent_id: String,
    pub seat_ref: String,
    pub subject_digest: Value,
    pub intent_digest: Value,
    pub stable_execution_key: String,
    pub budget_reservation_set_ref: String,
    pub revision: u64,
    /// The disclosure pair the act was AUTHORIZED for.
    pub disclosure_manifest_ref: String,
    pub disclosure_digest: Value,
}

/// The `host_effect_credential` a consuming host mints, derived here
/// INDEPENDENTLY of byomd (the derivation kovee's client must mirror):
/// `HMAC-SHA-256(permit channel token, $domain-tagged canonical tuple)`
/// over exactly {intent_ref, stable_execution_key, host_effect_ref,
/// host_effect_digest}.
pub fn host_effect_credential(
    permit_token: &str,
    intent_ref: &str,
    stable_execution_key: &str,
    host_effect_ref: &str,
    host_effect_digest: &Value,
) -> String {
    let tuple = json!({
        "intent_ref": intent_ref,
        "stable_execution_key": stable_execution_key,
        "host_effect_ref": host_effect_ref,
        "host_effect_digest": host_effect_digest,
    });
    let bytes =
        bpp_core::canonical::tagged_canonical("bpp-host-effect-registration-v0", &tuple).unwrap();
    bpp_core::canonical::hex(&bpp_core::canonical::hmac_sha256(
        permit_token.trim().as_bytes(),
        &bytes,
    ))
}

/// Signs a consumption body's host-effect tuple with the permit token: the
/// host registers the exact Effect it durably created before consuming
/// (§13.1 step 3). A probe that mutates the effect ref or digest and
/// re-signs reaches byom's state checks; one that does not is refused as
/// unregistered.
pub fn sign_host_effect(permit_token: &str, body: &mut Value) {
    let credential = host_effect_credential(
        permit_token,
        body["intent_ref"].as_str().unwrap_or_default(),
        body["stable_execution_key"].as_str().unwrap_or_default(),
        body["host_effect_ref"].as_str().unwrap_or_default(),
        &body["host_effect_digest"],
    );
    merge(body, json!({"host_effect_credential": credential}));
}

/// One requested Episode with the refs the runtime commands need.
#[derive(Debug, Clone)]
pub struct Episode {
    pub episode_id: String,
    pub wake_intent_ref: String,
    pub admission_ref: String,
    pub allocation_ref: String,
    pub bridge_ref: String,
    pub stable_external_key: String,
    /// The allocation pin `episode_request` PUBLISHED (seam finding S-1):
    /// the harness takes it from the reply, never from `byom.db`.
    pub allocation_digest: Value,
    /// The frozen `portable_public` parent-budget fragment the same reply
    /// published (R3-L02): exact set/bridge refs, revisions, set digest,
    /// stable key, and the exact parent items.
    pub parent_budget: Value,
}

/// One held lease (the claim CAS result).
#[derive(Debug, Clone)]
pub struct Claim {
    pub attempt_ref: String,
    pub byom_fence_epoch: u64,
    pub kovee_invocation_fence: u64,
    pub lease_revision: u64,
    pub binding_ref: String,
    pub binding: Value,
}

/// What Kovee reports about the `byom_subordinate` reservation.
pub enum Subordinate {
    /// Confirmed, possibly NARROWED to this amount (never above parent).
    Confirmed(u64),
    /// Confirmed with an EXACT reported item list — what a probe needs to
    /// report two children against one parent item (R3-U04).
    ConfirmedItems(Vec<Value>),
    Denied,
    Uncertain,
}

/// One reported subordinate item pinned to the parent item byom PUBLISHED.
pub fn subordinate_item(parent: &Value, amount: u64) -> Value {
    json!({
        "kovee_account_ref": "kovee-acct-1",
        "dimension": parent["dimension"],
        "unit": parent["unit"],
        "amount": amount,
        "parent_account_ref": parent["account_ref"],
        "parent_account_revision": parent["account_revision"],
        "parent_dimension": parent["dimension"],
        "parent_unit": parent["unit"],
        "parent_worst_case_amount": parent["worst_case_amount"],
    })
}

pub const WORST_CASE: u64 = 256;
pub const PARENT_ACCOUNT: &str = "budget-mandate-1";

impl Fixture {
    /// Boots the Society and the agent, and issues one Mandate whose
    /// `concurrency_ceiling` is the RATE ceiling the kernel enforces on
    /// Episodes in flight.
    pub fn start(tag: &str, concurrency_ceiling: u64) -> Fixture {
        Fixture::start_with_env(tag, concurrency_ceiling, &[])
    }

    pub fn start_with_env(tag: &str, concurrency_ceiling: u64, env: &[(&str, &str)]) -> Fixture {
        let daemon = TestDaemon::start_with_env(tag, env);
        Fixture::build(daemon, tag, concurrency_ceiling)
    }

    fn build(daemon: TestDaemon, tag: &str, concurrency_ceiling: u64) -> Fixture {
        let incarnation = daemon.incarnation();
        let ok = |what: &str, reply: &Value| {
            assert_eq!(reply["outcome"], "ok", "{what}: {reply}");
        };
        // -- genesis --
        let prepared = daemon.call(
            "governance",
            &json!({
                "version": "0.2", "op": "society_prepare",
                "meta": meta(&incarnation, &format!("{tag}-prep"), None),
                "home_authority_ref": "auth-home-1",
                "proposed_charter_ref": "charter-draft-1",
                "proposed_charter_digest": test_digest(0xa1),
                "classification_binding_ref": "class-bind-1",
                "classification_binding_digest": test_digest(0xa2),
            }),
        );
        ok("prepare", &prepared);
        let society_id = prepared["result"]["society_id"]
            .as_str()
            .unwrap()
            .to_owned();
        ok(
            "bootstrap",
            &daemon.call(
                "governance",
                &json!({
                    "version": "0.2", "op": "society_bootstrap",
                    "meta": meta(&incarnation, &format!("{tag}-boot"), Some(1)),
                    "society_id": society_id,
                    "preparation_ref": prepared["result"]["preparation_ref"],
                    "subject_digest": prepared["result"]["subject_digest"],
                }),
            ),
        );
        // -- onboarding --
        let subject = test_digest(0xb1);
        let offered = daemon.call(
            "governance",
            &json!({
                "version": "0.2", "op": "membership_offer",
                "meta": meta(&incarnation, &format!("{tag}-offer"), None),
                "participant_ref": "part-agent-1",
                "proposed_standing_ref": "standing-proposal-1",
                "subject_digest": subject,
                "offered_by_decision_ref": format!("dec-society-{society_id}"),
                "expires_at": far_future(),
            }),
        );
        ok("offer", &offered);
        let offer_id = offered["result"]["offer_id"].as_str().unwrap().to_owned();
        let cand_token = read_candidate_token(&daemon, &offer_id);
        let accepted = daemon.call_candidate(
            &cand_token,
            &json!({
                "version": "0.2", "op": "membership_accept",
                "meta": meta(&incarnation, &format!("{tag}-accept"), Some(1)),
                "offer_ref": offer_id,
                "subject_digest": subject,
            }),
        );
        ok("accept", &accepted);
        let admitted = daemon.call(
            "governance",
            &json!({
                "version": "0.2", "op": "participant_admit",
                "meta": meta(&incarnation, &format!("{tag}-admit"), Some(2)),
                "offer_ref": offer_id,
                "membership_acceptance_ref": accepted["result"]["acceptance_id"],
                "admitted_by_decision_ref": offer_decision(&offer_id),
                "admission_subject_digest": subject,
            }),
        );
        ok("admit", &admitted);
        // The proposed Manifestation admission creates (read straight
        // from the store — the harness's inspection channel).
        let manifestation_id: String = {
            let conn = rusqlite::Connection::open(daemon.data_dir.join("byom.db")).unwrap();
            conn.query_row(
                "SELECT manifestation_id FROM manifestation_revisions
                 WHERE participant_ref = 'part-agent-1' LIMIT 1",
                [],
                |r| r.get(0),
            )
            .expect("proposed manifestation")
        };
        ok(
            "manifestation_admit",
            &daemon.call(
                "governance",
                &json!({
                    "version": "0.2", "op": "manifestation_admit",
                    "meta": meta(&incarnation, &format!("{tag}-manif"), Some(1)),
                    "manifestation_ref": manifestation_id,
                    "admitted_by_decision_ref": manifestation_decision(&manifestation_id),
                }),
            ),
        );
        let agent_token = read_participant_token(&daemon, "part-agent-1");
        let _ = sovereign_id(&daemon, &society_id);

        // -- the mandate chain --
        let prepared_mandate = daemon
            .call_raw(
                "participant",
                Some(&agent_token),
                &json!({
                    "version": "0.2", "op": "mandate_prepare",
                    "meta": meta(&incarnation, &format!("{tag}-mprep"), None),
                    "grantee_participant_ref": "part-agent-1",
                    "purpose_ref": "purpose-explore-1",
                    // The Δ4 act classes this Mandate bounds, alongside the
                    // activity operations (family contract Δ4: act classes
                    // are carried in ActIntent subjects and bounded by
                    // Mandates).
                    "allowed_operations": ["activity_open", "continuation_write",
                                           "wake_intent_submit",
                                           "model_egress", "share"],
                    "resource_selectors": ["res-repo-1"],
                    "data_class_selectors": ["class-public"],
                    "destination_selectors": [],
                    "budget_ceiling_set_ref": PARENT_ACCOUNT,
                    "concurrency_ceiling": concurrency_ceiling,
                    "delegation": {"allowed": false, "max_depth": 0,
                                   "max_children": 0, "grantee_selectors": []},
                    "expires_at": far_future(),
                })
                .to_string(),
            )
            .unwrap();
        ok("mandate_prepare", &prepared_mandate);
        let mandate_id = prepared_mandate["result"]["mandate_id"]
            .as_str()
            .unwrap()
            .to_owned();
        let seat = prepared_mandate["result"]["required_seat_refs"][0]
            .as_str()
            .unwrap()
            .to_owned();
        ok(
            "mandate_position",
            &daemon.call(
                "governance",
                &json!({
                    "version": "0.2", "op": "mandate_position",
                    "meta": meta(&incarnation, &format!("{tag}-mpos"), None),
                    "proposal_ref": mandate_id,
                    "proposal_revision": 1,
                    "subject_digest": prepared_mandate["result"]["subject_digest"],
                    "seat_ref": seat,
                    "value": "assent",
                }),
            ),
        );
        let issued = daemon.call(
            "governance",
            &json!({
                "version": "0.2", "op": "mandate_issue",
                "meta": meta(&incarnation, &format!("{tag}-missue"), Some(1)),
                "mandate_id": mandate_id,
                "subject_digest": prepared_mandate["result"]["subject_digest"],
            }),
        );
        ok("mandate_issue", &issued);
        let mandate_revision = issued["result"]["revision"].as_u64().unwrap();

        // -- the ActivityStream the Episodes run under --
        let opened = daemon
            .call_raw(
                "participant",
                Some(&agent_token),
                &json!({
                    "version": "0.2", "op": "activity_open",
                    "meta": meta(&incarnation, &format!("{tag}-explore"), None),
                    "kind": "exploration",
                    "purpose_ref": "purpose-explore-1",
                    "purpose_digest": test_digest(0xc0),
                    "mandate_refs": [mandate_id],
                    "budget_account_set_ref": PARENT_ACCOUNT,
                })
                .to_string(),
            )
            .unwrap();
        ok("activity_open", &opened);
        let stream = opened["result"]["activity_stream_id"]
            .as_str()
            .unwrap()
            .to_owned();
        Fixture {
            daemon,
            incarnation,
            society_id,
            agent_token,
            mandate_id,
            mandate_subject_digest: prepared_mandate["result"]["subject_digest"].clone(),
            mandate_revision,
            stream,
            tag: tag.to_owned(),
        }
    }

    // ------------------------------------------------- participant ----

    pub fn participant(&self, request: &Value) -> Value {
        self.daemon
            .call_raw("participant", Some(&self.agent_token), &request.to_string())
            .unwrap_or_else(|e| panic!("participant call: {e}\n{request}"))
    }

    pub fn governance(&self, request: &Value) -> Value {
        self.daemon.call("governance", request)
    }

    pub fn runtime(&self, token: &str, request: &Value) -> Value {
        self.daemon
            .call_raw("runtime", Some(token), &request.to_string())
            .unwrap_or_else(|e| panic!("runtime call: {e}\n{request}"))
    }

    pub fn meta(&self, key: &str, expected_revision: Option<u64>) -> Value {
        meta(
            &self.incarnation,
            &format!("{}-{key}", self.tag),
            expected_revision,
        )
    }

    /// Stage 1: the participant channel — and nothing else — authors a
    /// WakeIntent (§11.1).
    pub fn wake(&self, key: &str) -> String {
        let reply = self.participant(&json!({
            "version": "0.2", "op": "wake_intent_submit",
            "meta": self.meta(&format!("wake-{key}"), None),
            "activity_stream_ref": self.stream,
            "generation": 1,
            "origin": "direct_participant",
            "exact_cause_ref": format!("cause-{key}"),
            "exact_cause_digest": test_digest(0xc2),
            "purpose_ref": "purpose-explore-1",
            "stable_wake_key": format!("wake-{}-{key}", self.tag),
            "expires_at": far_future(),
        }));
        assert_eq!(reply["outcome"], "ok", "wake_intent_submit: {reply}");
        reply["result"]["wake_intent_id"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    pub fn admission_ref(wake: &str) -> String {
        format!("adm-{wake}-r1")
    }
    pub fn allocation_ref(wake: &str) -> String {
        format!("alloc-{wake}-r1")
    }

    /// Stages 2 and 3 (kernel) plus the Episode: `episode_request`.
    pub fn request_episode_raw(&self, wake: &str, key: &str) -> Value {
        self.participant(&json!({
            "version": "0.2", "op": "episode_request",
            "meta": self.meta(&format!("ereq-{key}"), None),
            "activity_stream_ref": self.stream,
            "generation": 1,
            "wake_intent_ref": wake,
            "activation_admission_ref": Fixture::admission_ref(wake),
        }))
    }

    pub fn request_episode(&self, wake: &str, key: &str) -> Episode {
        let reply = self.request_episode_raw(wake, key);
        assert_eq!(reply["outcome"], "ok", "episode_request: {reply}");
        assert_eq!(
            reply["result"]["state"], "eligible",
            "the Episode is eligible but NOT queued: queueing needs both exact \
             reservation sets (§11.4)"
        );
        let allocation_ref = Fixture::allocation_ref(wake);
        // The result names the allocation it created AND its published
        // cross-boundary digest: everything stage 4 needs, over the wire.
        assert_eq!(
            reply["result"]["resource_allocation_id"], allocation_ref,
            "episode_request publishes the stage-3 allocation it created"
        );
        let allocation_digest = reply["result"]["resource_allocation_digest"].clone();
        assert_eq!(
            allocation_digest["class"], "portable_public",
            "the published allocation pin is portable_public — both sides derive it \
             (seam finding S-2): {allocation_digest}"
        );
        // R3-L02: the reply also publishes the FROZEN parent-budget
        // fragment, so nothing downstream names a reference by convention
        // or takes a parent amount from its own caller's arguments.
        let parent_budget = reply["result"]["parent_budget"].clone();
        assert_eq!(
            parent_budget["digest"]["class"], "portable_public",
            "the parent-budget fragment is portable_public: {parent_budget}"
        );
        Episode {
            episode_id: reply["result"]["episode_id"].as_str().unwrap().to_owned(),
            wake_intent_ref: wake.to_owned(),
            admission_ref: Fixture::admission_ref(wake),
            bridge_ref: parent_budget["external_budget_bridge_ref"]
                .as_str()
                .unwrap()
                .to_owned(),
            stable_external_key: parent_budget["stable_external_reservation_key"]
                .as_str()
                .unwrap()
                .to_owned(),
            allocation_ref,
            allocation_digest,
            parent_budget,
        }
    }

    // ------------------------------------------- runtime channels ----

    fn token_file(&self, name: &str) -> String {
        let path = self.daemon.data_dir.join("channels").join(name);
        std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
            .trim()
            .to_owned()
    }

    pub fn worker_token(&self, episode: &str) -> String {
        self.token_file(&format!("runtime-worker-{episode}.token"))
    }
    pub fn meter_token(&self, episode: &str) -> String {
        self.token_file(&format!("runtime-meter-{episode}.token"))
    }
    pub fn placement_token(&self, allocation: &str) -> String {
        self.token_file(&format!("runtime-placement-{allocation}.token"))
    }
    /// The trusted host effect service's channel, bound to one act's
    /// one-shot key (R34).
    pub fn permit_token(&self, intent_id: &str) -> String {
        self.token_file(&format!("runtime-permit-{intent_id}.token"))
    }
    /// The Kovee model broker's channel, bound to one OnboardingComputeIntent
    /// (R32).
    pub fn broker_token(&self, compute_intent: &str) -> String {
        self.token_file(&format!("runtime-broker-{compute_intent}.token"))
    }
    /// The candidate workload's channel, bound to one offer AND its fence
    /// (R31): advancing the fence invalidates the token itself.
    pub fn onboarding_token(&self, onboarding_id: &str) -> String {
        self.token_file(&format!("runtime-onboarding-{onboarding_id}.token"))
    }
    /// The narrow Kovee attention adapter's channel, bound to one
    /// ActivityStream generation.
    pub fn attention_token(&self, stream: &str) -> String {
        self.token_file(&format!("runtime-attention-{stream}.token"))
    }
    pub fn token_path_exists(&self, name: &str) -> bool {
        self.daemon.data_dir.join("channels").join(name).exists()
    }

    // ----------------------------------------- the §13.1 act chain ----

    /// `act_intent_prepare` (participant, R19) for one Δ4 act class.
    pub fn prepare_act_raw(&self, key: &str, kind: &str, driver_audience: Option<&str>) -> Value {
        let mut body = json!({
            "version": "0.2", "op": "act_intent_prepare",
            "meta": self.meta(&format!("actprep-{key}"), None),
            "kind": kind,
            "execution_kind": "external_effect",
            "subject_ref": format!("subject-{key}"),
            "subject_revision": 1,
            "mandate_ref": self.mandate_id,
            "mandate_revision": self.mandate_revision,
            "mandate_digest": self.mandate_subject_digest,
            // Both manifests are the HOST's objects, pinned as exact
            // ref-and-digest pairs that enter the assented subject and are
            // compared again at consumption (R3-A01). `portable_public`,
            // because the consuming host has to present the same value (A8).
            "context_manifest_ref": "ctxman-1",
            "context_manifest_digest": portable_digest(0xe1),
            "disclosure_manifest_ref": act_disclosure_ref(key),
            "disclosure_manifest_digest": act_disclosure_digest(),
        });
        if let Some(audience) = driver_audience {
            merge(&mut body, json!({"driver_audience": audience}));
        }
        self.participant(&body)
    }

    /// prepare + position (governance gate seat) + finalize: the authorized
    /// act a permit consumption needs.
    pub fn authorized_act(&self, key: &str, kind: &str, driver_audience: &str) -> Act {
        let prepared = self.prepare_act_raw(key, kind, Some(driver_audience));
        assert_eq!(prepared["outcome"], "ok", "act_intent_prepare: {prepared}");
        let r = &prepared["result"];
        let intent_id = r["intent_id"].as_str().unwrap().to_owned();
        let seat_ref = r["required_seat_refs"][0].as_str().unwrap().to_owned();
        let subject_digest = r["subject_digest"].clone();
        let positioned = self.governance(&json!({
            "version": "0.2", "op": "act_intent_position",
            "meta": self.meta(&format!("actpos-{key}"), None),
            "proposal_ref": intent_id,
            "proposal_revision": 1,
            "subject_digest": subject_digest,
            "seat_ref": seat_ref,
            "value": "assent",
        }));
        assert_eq!(
            positioned["outcome"], "ok",
            "act_intent_position: {positioned}"
        );
        let finalized = self.governance(&json!({
            "version": "0.2", "op": "act_intent_finalize",
            "meta": self.meta(&format!("actfin-{key}"), Some(1)),
            "intent_id": intent_id,
            "subject_digest": subject_digest,
        }));
        assert_eq!(
            finalized["outcome"], "ok",
            "act_intent_finalize: {finalized}"
        );
        Act {
            intent_digest: self.intent_digest(&intent_id),
            stable_execution_key: r["stable_execution_key"].as_str().unwrap().to_owned(),
            budget_reservation_set_ref: r["budget_reservation_set_ref"]
                .as_str()
                .unwrap()
                .to_owned(),
            revision: finalized["result"]["revision"].as_u64().unwrap(),
            disclosure_manifest_ref: act_disclosure_ref(key),
            disclosure_digest: act_disclosure_digest(),
            intent_id,
            seat_ref,
            subject_digest,
        }
    }

    /// The committed ActIntent record digest a consumption must pin (read
    /// straight from the store — the harness's inspection channel).
    pub fn intent_digest(&self, intent_id: &str) -> Value {
        let text = self
            .row(
                "SELECT intent_digest FROM act_intents WHERE intent_id = ?1",
                intent_id,
            )
            .unwrap_or_else(|| panic!("intent digest {intent_id}"));
        serde_json::from_str(&text).unwrap()
    }

    /// The committed ByomEpisodeBinding digest an episode-bound act pins.
    pub fn binding_digest(&self, binding_ref: &str) -> Value {
        let text = self
            .row(
                "SELECT digest FROM byom_episode_bindings WHERE binding_id = ?1",
                binding_ref,
            )
            .unwrap_or_else(|| panic!("binding digest {binding_ref}"));
        serde_json::from_str(&text).unwrap()
    }

    /// The `execution_permit_consume` body (runtime, R34) under an explicit
    /// key, effect and fence pair, so every refusal can be probed exactly.
    /// UNSIGNED: `sign_host_effect` mints the registration credential, and
    /// a probe may mutate any member before signing.
    ///
    /// The disclosure pair carried is the act's OWN authorized pair — the
    /// receipt publishes byom's committed value, so presenting anything
    /// else is refused rather than copied (R3-A01).
    ///
    /// byom's own digests (`intent_digest`, `subject_digest`,
    /// `episode_fence_digest`) are NOT members: byom recomputes each from
    /// its committed state (A8, R3-L01).
    #[allow(clippy::too_many_arguments)]
    pub fn consume_body(
        &self,
        act: &Act,
        key: &str,
        effect_key: &str,
        stable_key: &str,
        driver_audience: &str,
        episode: Option<&str>,
        byom_fence: u64,
        host_fence: u64,
        expected_revision: u64,
        host_effect_digest: Value,
    ) -> Value {
        let mut body = json!({
            "version": "0.2", "op": "execution_permit_consume",
            "meta": self.meta(&format!("perm-{key}"), Some(expected_revision)),
            "stable_execution_key": stable_key,
            "intent_ref": act.intent_id,
            "host_effect_ref": format!("kovee-effect-{effect_key}"),
            "host_effect_digest": host_effect_digest,
            "disclosure_manifest_ref": act.disclosure_manifest_ref,
            "disclosure_digest": act.disclosure_digest,
            "driver_audience": driver_audience,
            "budget_reservation_set_ref": act.budget_reservation_set_ref,
            "byom_fence_epoch": byom_fence,
            "host_fence_epoch": host_fence,
        });
        if let Some(episode_ref) = episode {
            merge(&mut body, json!({"episode_ref": episode_ref}));
        }
        body
    }

    /// The same consumption, signed for the exact host Effect it names and
    /// sent on the permit channel.
    #[allow(clippy::too_many_arguments)]
    pub fn consume_permit_with(
        &self,
        token: &str,
        act: &Act,
        key: &str,
        effect_key: &str,
        stable_key: &str,
        driver_audience: &str,
        episode: Option<&str>,
        byom_fence: u64,
        host_fence: u64,
        expected_revision: u64,
        host_effect_digest: Value,
    ) -> Value {
        let mut body = self.consume_body(
            act,
            key,
            effect_key,
            stable_key,
            driver_audience,
            episode,
            byom_fence,
            host_fence,
            expected_revision,
            host_effect_digest,
        );
        sign_host_effect(token, &mut body);
        self.runtime(token, &body)
    }

    /// A consumption body a probe has already shaped: signed here for the
    /// effect it names, so a mutated member reaches byom's own checks
    /// rather than the registration gate.
    pub fn consume_signed(&self, token: &str, body: &Value) -> Value {
        let mut body = body.clone();
        sign_host_effect(token, &mut body);
        self.runtime(token, &body)
    }

    /// Stage 4: the narrow Kovee placement adapter, carrying the
    /// `byom_subordinate` outcome.
    pub fn admit_placement_raw(&self, ep: &Episode, key: &str, sub: Subordinate) -> Value {
        let token = self.placement_token(&ep.allocation_ref);
        self.admit_placement_with(ep, key, sub, &token)
    }

    /// The same call under an EXPLICIT placement token — a released
    /// allocation loses its token file, so a test that probes the
    /// terminal saga keeps the token it held.
    pub fn admit_placement_with(
        &self,
        ep: &Episode,
        key: &str,
        sub: Subordinate,
        token: &str,
    ) -> Value {
        // The pin comes from the `episode_request` REPLY the harness kept —
        // there is no inspection path left (seam finding S-1).
        let allocation_digest = ep.allocation_digest.clone();
        let subordinate = match sub {
            // The reported item pins the parent item byom PUBLISHED in the
            // fragment, not a value this harness names (R3-L02).
            Subordinate::Confirmed(amount) => json!({
                "stable_external_reservation_key": ep.stable_external_key,
                "outcome": "confirmed",
                "subordinate_reservation_ref": format!("kovee-sub-{key}"),
                "revision": 1,
                "digest": portable_digest(0x5c),
                "items": [subordinate_item(&ep.parent_budget["items"][0], amount)],
            }),
            Subordinate::ConfirmedItems(items) => json!({
                "stable_external_reservation_key": ep.stable_external_key,
                "outcome": "confirmed",
                "subordinate_reservation_ref": format!("kovee-sub-{key}"),
                "revision": 1,
                "digest": portable_digest(0x5c),
                "items": items,
            }),
            Subordinate::Denied => json!({
                "stable_external_reservation_key": ep.stable_external_key,
                "outcome": "denied",
            }),
            Subordinate::Uncertain => json!({
                "stable_external_reservation_key": ep.stable_external_key,
                "outcome": "uncertain",
            }),
        };
        self.runtime(
            token,
            &json!({
                "version": "0.2", "op": "placement_admit",
                "meta": self.meta(&format!("plc-{key}"), None),
                "resource_allocation_ref": ep.allocation_ref,
                "resource_allocation_digest": allocation_digest,
                "kovee_placement_ref": format!("kovee-placement-{key}"),
                "kovee_placement_revision": 1,
                "kovee_placement_digest": portable_digest(0x5d),
                "source_binding_epoch": 1,
                "selected_manifestation_ref": "manif-selected-1",
                "kovee_invocation_ref": format!("kovee-inv-{key}"),
                "kovee_fence_epoch": 7,
                "subordinate_reservation": subordinate,
            }),
        )
    }

    pub fn admit_placement(&self, ep: &Episode, key: &str, sub: Subordinate) -> Value {
        let reply = self.admit_placement_raw(ep, key, sub);
        assert_eq!(reply["outcome"], "ok", "placement_admit: {reply}");
        reply
    }

    pub fn row(&self, sql: &str, key: &str) -> Option<String> {
        let conn = rusqlite::Connection::open(self.daemon.data_dir.join("byom.db")).unwrap();
        conn.query_row(sql, [key], |r| r.get::<_, Option<String>>(0))
            .ok()
            .flatten()
    }

    /// A parameterless count over the store.
    pub fn count(&self, sql: &str) -> i64 {
        let conn = rusqlite::Connection::open(self.daemon.data_dir.join("byom.db")).unwrap();
        conn.query_row(sql, [], |r| r.get::<_, i64>(0))
            .unwrap_or_else(|e| panic!("count {sql}: {e}"))
    }

    pub fn number(&self, sql: &str, key: &str) -> Option<i64> {
        let conn = rusqlite::Connection::open(self.daemon.data_dir.join("byom.db")).unwrap();
        conn.query_row(sql, [key], |r| r.get::<_, i64>(0)).ok()
    }

    /// The §11.4 conservation ledger row of the parent account.
    pub fn ledger(&self) -> Ledger {
        let conn = rusqlite::Connection::open(self.daemon.data_dir.join("byom.db")).unwrap();
        conn.query_row(
            "SELECT ceiling, remaining, reserved, committed, uncertain,
                    delegated_to_children
             FROM budget_accounts WHERE account_ref = ?1 AND dimension = 'unit'",
            [PARENT_ACCOUNT],
            |r| {
                Ok(Ledger {
                    ceiling: r.get(0)?,
                    remaining: r.get(1)?,
                    reserved: r.get(2)?,
                    committed: r.get(3)?,
                    uncertain: r.get(4)?,
                    delegated: r.get(5)?,
                })
            },
        )
        .expect("parent budget account")
    }

    /// Stage: `episode_claim` — the lease CAS.
    #[allow(clippy::too_many_arguments)]
    pub fn claim_raw(
        &self,
        episode: &str,
        holder: &str,
        ttl: u64,
        kovee_fence: u64,
        binding_key: &str,
        key: &str,
    ) -> Value {
        let token = self.worker_token(episode);
        self.runtime(
            &token,
            &json!({
                "version": "0.2", "op": "episode_claim",
                "meta": self.meta(&format!("clm-{key}"), None),
                "episode_ref": episode,
                "generation": 1,
                "holder_runtime_binding": holder,
                "lease_ttl_seconds": ttl,
                "kovee_invocation_ref": format!("kovee-inv-{key}"),
                "kovee_invocation_fence": kovee_fence,
                "stable_binding_key": binding_key,
                "context_manifest_ref": "ctxman-1",
                "context_manifest_digest": portable_digest(0xd2),
                "context_source_digest": portable_digest(0xd3),
                "mandate_use_refs": ["muse-1"],
                "allowed_local_commitments": ["kovee_local_note"],
                "provider_context_manifest_ref": "kovee-pcm-1",
                "provider_context_manifest_digest": test_digest(0xd4),
            }),
        )
    }

    pub fn claim(
        &self,
        episode: &str,
        holder: &str,
        ttl: u64,
        kovee_fence: u64,
        key: &str,
    ) -> Claim {
        let reply = self.claim_raw(
            episode,
            holder,
            ttl,
            kovee_fence,
            &format!("bindkey-{episode}-{key}"),
            key,
        );
        assert_eq!(reply["outcome"], "ok", "episode_claim: {reply}");
        Claim {
            attempt_ref: reply["result"]["byom_attempt_ref"]
                .as_str()
                .unwrap()
                .to_owned(),
            byom_fence_epoch: reply["result"]["byom_fence_epoch"].as_u64().unwrap(),
            kovee_invocation_fence: reply["result"]["kovee_invocation_fence"].as_u64().unwrap(),
            lease_revision: reply["result"]["lease_revision"].as_u64().unwrap(),
            binding_ref: reply["result"]["byom_episode_binding_ref"]
                .as_str()
                .unwrap()
                .to_owned(),
            binding: reply["result"]["byom_episode_binding"].clone(),
        }
    }

    /// The DUAL fence block every protected command presents.
    pub fn fences(&self, episode: &str, c: &Claim) -> Value {
        json!({
            "episode_ref": episode,
            "generation": 1,
            "byom_attempt_ref": c.attempt_ref,
            "byom_fence_epoch": c.byom_fence_epoch,
            "kovee_invocation_fence": c.kovee_invocation_fence,
        })
    }

    pub fn start_episode(&self, episode: &str, c: &Claim, key: &str) -> Value {
        let token = self.worker_token(episode);
        let mut body = json!({
            "version": "0.2", "op": "episode_start",
            "meta": self.meta(&format!("srt-{key}"), Some(c.lease_revision)),
        });
        merge(&mut body, self.fences(episode, c));
        self.runtime(&token, &body)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ledger {
    pub ceiling: i64,
    pub remaining: i64,
    pub reserved: i64,
    pub committed: i64,
    pub uncertain: i64,
    pub delegated: i64,
}

impl Ledger {
    /// §11.4: `ceiling = remaining + reserved + committed + uncertain +
    /// delegated_to_children`, at every observation.
    pub fn conserves(&self) -> bool {
        self.ceiling
            == self.remaining + self.reserved + self.committed + self.uncertain + self.delegated
    }
}

pub fn portable_digest(seed: u8) -> Value {
    json!({
        "class": "portable_public",
        "algorithm": "sha-256",
        "value_hex": format!("{:02x}", seed).repeat(32),
    })
}

/// Merges the members of `extra` into `into` (request builders).
pub fn merge(into: &mut Value, extra: Value) {
    let (Some(target), Some(source)) = (into.as_object_mut(), extra.as_object()) else {
        return;
    };
    for (k, v) in source {
        target.insert(k.clone(), v.clone());
    }
}
