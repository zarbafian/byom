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
    pub stream: String,
    pub tag: String,
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
    Denied,
    Uncertain,
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
                    "allowed_operations": ["activity_open", "continuation_write",
                                           "wake_intent_submit"],
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
        ok(
            "mandate_issue",
            &daemon.call(
                "governance",
                &json!({
                    "version": "0.2", "op": "mandate_issue",
                    "meta": meta(&incarnation, &format!("{tag}-missue"), Some(1)),
                    "mandate_id": mandate_id,
                    "subject_digest": prepared_mandate["result"]["subject_digest"],
                }),
            ),
        );

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
        Episode {
            episode_id: reply["result"]["episode_id"].as_str().unwrap().to_owned(),
            wake_intent_ref: wake.to_owned(),
            admission_ref: Fixture::admission_ref(wake),
            bridge_ref: format!("bridge-{allocation_ref}"),
            stable_external_key: format!("sub-{allocation_ref}"),
            allocation_ref,
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
        let allocation_digest = self.allocation_digest(&ep.allocation_ref);
        let subordinate = match sub {
            Subordinate::Confirmed(amount) => json!({
                "stable_external_reservation_key": ep.stable_external_key,
                "outcome": "confirmed",
                "subordinate_reservation_ref": format!("kovee-sub-{key}"),
                "revision": 1,
                "digest": portable_digest(0x5c),
                "items": [{
                    "kovee_account_ref": "kovee-acct-1",
                    "dimension": "unit", "unit": "unit", "amount": amount,
                    "parent_account_ref": PARENT_ACCOUNT,
                    "parent_account_revision": 1,
                    "parent_dimension": "unit", "parent_unit": "unit",
                    "parent_worst_case_amount": WORST_CASE,
                }],
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

    /// The committed ResourceAllocation digest the placement adapter must
    /// pin (read straight from the store — the harness's inspection
    /// channel).
    pub fn allocation_digest(&self, allocation: &str) -> Value {
        let conn = rusqlite::Connection::open(self.daemon.data_dir.join("byom.db")).unwrap();
        let text: String = conn
            .query_row(
                "SELECT digest FROM resource_allocations WHERE allocation_id = ?1",
                [allocation],
                |r| r.get(0),
            )
            .unwrap_or_else(|e| panic!("allocation digest {allocation}: {e}"));
        serde_json::from_str(&text).unwrap()
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
                "claim_subject_digest": test_digest(0xd1),
                "lease_ttl_seconds": ttl,
                "kovee_invocation_ref": format!("kovee-inv-{key}"),
                "kovee_invocation_fence": kovee_fence,
                "stable_binding_key": binding_key,
                "context_manifest_ref": "ctxman-1",
                "context_manifest_digest": test_digest(0xd2),
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
