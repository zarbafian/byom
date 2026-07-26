//! The Kovee side of the seam, as a test fixture: it derives exactly the
//! digests DESIGN.md §16.3 says both parties recompute, so the tests
//! prove agreement rather than restating byomd's own arithmetic.
//!
//! What one formation looks like:
//! ```text
//! let seam = install_seam(&mut daemon, &society, &incarnation, 0);
//! let attempt = seam.form(&society, &incarnation, 0, &human, "k-1", "nonce-1",
//!                         proposal(&[&human]), position(&human, "assent"));
//! let reply = daemon.call_raw("governance", Some(&attempt.credential),
//!                             &attempt.request.to_string()).unwrap();
//! ```

#![allow(dead_code)]

use bpp_core::canonical::sha256_hex;
use bpp_core::digest::DigestRef;
use bpp_core::hostint;
use serde_json::{json, Value};

use super::{far_future, TestDaemon};

/// A `portable_public` digest over one `$domain`-tagged object — the
/// cross-boundary class both sides recompute (PROFILE §6.2, RT-02).
pub fn portable(tag: &str, object: &Value) -> Value {
    to_value(&hostint::portable_digest(tag, object).expect("portable digest"))
}

/// The digest a self-describing record carries over itself.
pub fn sealed(tag: &str, record: &Value) -> Value {
    to_value(&hostint::self_digest(tag, record).expect("self digest"))
}

fn to_value(d: &DigestRef) -> Value {
    serde_json::to_value(d).expect("digest ref")
}

/// Kovee's own per-Society IdempotencyDomain digest: the keyed
/// `scope_erasure_safe` class §14.2 requires, under Kovee's index key.
pub fn kovee_domain_digest(realm: &str, key: &str) -> Value {
    json!({
        "class": "scope_erasure_safe",
        "algorithm": "hmac-sha-256",
        "key_ref": format!("kovee-index:{realm}"),
        "value_hex": sha256_hex(format!("{realm}|{key}").as_bytes()),
    })
}

/// One installed seam: the endpoint configuration plus everything a
/// gateway needs to mint credentials and commands against it.
pub struct Seam {
    pub realm_ref: String,
    pub binding_ref: String,
    pub binding_revision: u64,
    pub binding_epoch: u64,
    pub endpoint_ref: String,
    pub endpoint_root_id: String,
    pub audience: String,
    pub issuer: String,
    pub binding_digest: Value,
    pub recovery_digest: Value,
    pub recovery_ref: String,
    pub recovery_workload_token: String,
    pub society_ref: String,
    pub incarnation: String,
}

/// One prepared formation attempt: the stable command plus the fresh
/// per-attempt envelope, and the credential preamble that authorises it.
pub struct Attempt {
    pub request: Value,
    pub credential: String,
    pub credential_value: Value,
    pub canonical_command_digest: Value,
    pub idempotency_domain_digest: Value,
    pub command: Value,
    pub intent_ref: String,
}

/// The canonical EndeavorProposal body (the B0.1 `endeavor_propose`
/// subject members, carried opaque through §16.3).
pub fn proposal(sponsors: &[&str], tag: &str) -> Value {
    json!({
        "purpose_ref": "purpose-kovee-1",
        "purpose_digest": super::test_digest(0xe1),
        "sponsor_participant_refs": sponsors,
        "governance_rule_set_ref": "rules-endeavor-kovee",
        "outcome_schema_refs": ["schema-change-set-1"],
        "acceptance_rule_ref": "rule-accept-1",
        "classification_join_ref": "class-join-1",
        "budget_account_set_ref": format!("budget-kovee-{tag}"),
    })
}

/// The source principal's own explicit Position filling the sole seat.
pub fn position(participant: &str, value: &str) -> Value {
    json!({
        "participant_ref": participant,
        "value": value,
        "assent_mode": "direct_participant",
    })
}

/// A policy-derived Position — §16.3 forbids invoking an automatic
/// assent policy through this convenience operation.
pub fn policy_position(participant: &str) -> Value {
    json!({
        "participant_ref": participant,
        "value": "assent",
        "assent_mode": "policy_derived",
    })
}

fn binding_record(
    realm: &str,
    binding_ref: &str,
    endpoint_ref: &str,
    incarnation: &str,
    audience: &str,
    epoch: u64,
    historical: &str,
) -> Value {
    let mut record = json!({
        "binding_ref": binding_ref,
        "realm_ref": realm,
        "binding_revision": 2,
        "binding_epoch": epoch,
        "byom_endpoint_ref": endpoint_ref,
        "endpoint_incarnation": incarnation,
        "compatibility_bundle": "byom_governed_work_v1",
        "delegated_principal_audience": audience,
        "external_authorization_audience": format!("{audience}-effects"),
        "historical_recovery_mode": historical,
        "recovery_authorization_policy_ref": "recovery-policy-1",
        "recovery_authorization_policy_digest": portable("bpp-kovee-recovery-policy-v0",
                                                         &json!({"policy": "recovery-policy-1"})),
        "status": "active",
        "dependency_digest": portable("bpp-kovee-dependency-set-v0", &json!({"realm": realm})),
    });
    record["digest"] = portable("bpp-kovee-realm-byom-binding-v0", &record);
    record
}

fn mapping_record(realm: &str, society: &str, epoch: u64) -> Value {
    let mut record = json!({
        "realm_ref": realm,
        "society_ref": society,
        "society_recovery_epoch": epoch,
        "allowed_project_and_space_selectors": ["project/*"],
        "classification_binding_ref": "class-bind-1",
        "governance_owner_binding_ref": "owner-binding-1",
        "governance_owner_binding_digest": portable("bpp-kovee-governance-owner-binding-v0",
                                                    &json!({"owner": "byom"})),
        "status": "active",
        "revision": 2,
    });
    record["digest"] = portable("bpp-kovee-society-mapping-v0", &record);
    record
}

/// Installs the Kovee host binding as endpoint configuration and
/// restarts the daemon, exactly as `kovee governance enable` would
/// (amendment A2: Kovee may configure and bind byomd, never author
/// Society state). Returns the seam plus the published R42
/// recovery-workload token.
pub fn install_seam(daemon: &mut TestDaemon, society: &str, incarnation: &str, epoch: u64) -> Seam {
    install_seam_with(daemon, society, incarnation, epoch, |_| {})
}

/// The same, letting a test edit the configuration before it lands
/// (historical recovery mode, restore lineages, witness receipts).
pub fn install_seam_with(
    daemon: &mut TestDaemon,
    society: &str,
    incarnation: &str,
    epoch: u64,
    edit: impl FnOnce(&mut Value),
) -> Seam {
    let realm = "realm-kovee-1";
    let binding_ref = "krbb-1";
    let endpoint_ref = "byomd-endpoint-1";
    let audience = "aud-kovee-delegated";
    let issuer = "kovee-gateway-1";
    let endpoint_root_id = "endpoint-root-1";
    let binding = binding_record(
        realm,
        binding_ref,
        endpoint_ref,
        incarnation,
        audience,
        1,
        "exact_formation_intent_only",
    );
    let recovery_digest = portable(
        "bpp-kovee-recovery-binding-v0",
        &json!({"binding": "recovery-binding-1", "revision": 3}),
    );
    let mut config = json!({
        "realm_byom_binding": binding,
        "society_mapping": mapping_record(realm, society, epoch),
        "delegated_principal_issuers": [issuer],
        "recovery_binding": {
            "binding_ref": "recovery-binding-1",
            "binding_revision": 3,
            "binding_epoch": 1,
            "digest": recovery_digest,
        },
        "endpoint_root_id": endpoint_root_id,
        "external_witness_receipts": [],
        "restore_lineages": [],
        "restore_lineage_proofs": [],
    });
    edit(&mut config);
    write_config(daemon, &config);
    daemon.restart(&[]);
    let binding = config["realm_byom_binding"].clone();
    Seam {
        realm_ref: realm.to_owned(),
        binding_ref: binding["binding_ref"].as_str().unwrap().to_owned(),
        binding_revision: binding["binding_revision"].as_u64().unwrap(),
        binding_epoch: binding["binding_epoch"].as_u64().unwrap(),
        endpoint_ref: endpoint_ref.to_owned(),
        endpoint_root_id: endpoint_root_id.to_owned(),
        audience: audience.to_owned(),
        issuer: issuer.to_owned(),
        binding_digest: binding["digest"].clone(),
        recovery_digest: config["recovery_binding"]["digest"].clone(),
        recovery_ref: "recovery-binding-1".to_owned(),
        recovery_workload_token: read_recovery_token(daemon, binding_ref),
        society_ref: society.to_owned(),
        incarnation: incarnation.to_owned(),
    }
}

pub fn write_config(daemon: &TestDaemon, config: &Value) {
    let dir = daemon.data_dir.join("kovee");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("host-binding.json"), config.to_string()).unwrap();
}

pub fn read_recovery_token(daemon: &TestDaemon, binding_ref: &str) -> String {
    let path = daemon
        .data_dir
        .join("channels")
        .join(format!("recovery-workload-{binding_ref}.token"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read recovery token {}: {e}", path.display()))
        .trim()
        .to_owned()
}

/// How a test may bend one attempt away from the exact contract.
#[derive(Default, Clone)]
pub struct Bend {
    pub audience: Option<String>,
    pub issuer: Option<String>,
    pub allowed_operations: Option<Vec<String>>,
    pub realm_binding_ref: Option<String>,
    pub realm_binding_epoch: Option<u64>,
    pub credential_subject_digest: Option<Value>,
    pub society_ref: Option<String>,
    pub society_recovery_epoch: Option<u64>,
    pub participant_binding_epoch: Option<u64>,
    pub expired: bool,
}

impl Seam {
    /// The delegated-principal credential, self-digested exactly as the
    /// endpoint recomputes it.
    #[allow(clippy::too_many_arguments)]
    pub fn credential(
        &self,
        participant: &str,
        principal: &str,
        nonce: &str,
        subject_digest: &Value,
        operations: &[&str],
        epoch: u64,
        bend: &Bend,
    ) -> Value {
        let mut record = json!({
            "credential_id": format!("dpc-{nonce}"),
            "issuer_ref": bend.issuer.clone().unwrap_or_else(|| self.issuer.clone()),
            "nonce": nonce,
            "sender_constraint": {
                "method": "mtls",
                "key_binding_digest": portable("bpp-kovee-sender-key-v0",
                                               &json!({"key": principal})),
            },
            "source_principal_ref": principal,
            "source_actor_binding_digest": self.actor_binding(principal),
            "bound_participant_ref": participant,
            "participant_binding_epoch": bend.participant_binding_epoch.unwrap_or(1),
            "society_ref": bend.society_ref.clone().unwrap_or_else(|| self.society_ref.clone()),
            "society_recovery_epoch": bend.society_recovery_epoch.unwrap_or(epoch),
            "endpoint_incarnation": self.incarnation,
            "realm_byom_binding_ref": bend.realm_binding_ref.clone()
                .unwrap_or_else(|| self.binding_ref.clone()),
            "realm_byom_binding_revision": self.binding_revision,
            "realm_byom_binding_epoch": bend.realm_binding_epoch.unwrap_or(self.binding_epoch),
            "realm_byom_binding_digest": self.binding_digest,
            "audience": bend.audience.clone().unwrap_or_else(|| self.audience.clone()),
            "surface": "governance",
            "allowed_operations": operations,
            "delegated_principal_subject_digest":
                bend.credential_subject_digest.clone().unwrap_or_else(|| subject_digest.clone()),
            "authentication_observation_ref": format!("obs-{nonce}"),
            "authentication_observation_digest": portable("bpp-kovee-auth-observation-v0",
                                                          &json!({"observation": nonce})),
            "assurance_level": "phishing-resistant-step-up",
            "issued_at": if bend.expired { "2020-01-01T00:00:00Z" } else { "2026-01-01T00:00:00Z" },
            "expires_at": if bend.expired { "2020-01-01T01:00:00Z".to_owned() } else { far_future() },
        });
        record["digest"] = portable(hostint::CREDENTIAL_TAG, &record);
        record
    }

    /// The durable source-principal binding the channel supplies.
    pub fn actor_binding(&self, principal: &str) -> Value {
        portable(
            "bpp-kovee-source-actor-binding-v0",
            &json!({"principal": principal, "realm": self.realm_ref}),
        )
    }

    pub fn preamble(credential: &Value) -> String {
        let parsed = hostint::DelegatedPrincipalCredential::parse(credential)
            .unwrap_or_else(|e| panic!("credential {e}: {credential}"));
        hostint::encode_credential(&parsed).expect("encode")
    }

    /// One complete `kovee_endeavor_form` attempt.
    #[allow(clippy::too_many_arguments)]
    pub fn form(
        &self,
        epoch: u64,
        participant: &str,
        principal: &str,
        key: &str,
        nonce: &str,
        proposal: Value,
        position: Value,
        bend: &Bend,
    ) -> Attempt {
        let proposal_digest = portable(hostint::PROPOSAL_TAG, &proposal);
        let position_digest = portable(hostint::POSITION_TAG, &position);
        let rule_set = proposal["governance_rule_set_ref"]
            .as_str()
            .unwrap()
            .to_owned();
        let mut seats: Vec<Value> = proposal["sponsor_participant_refs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| json!({"kind": "sponsor", "participant_ref": p, "surface": "participant"}))
            .collect();
        seats.sort_by_key(|s| s["participant_ref"].as_str().unwrap_or_default().to_owned());
        let snapshot = json!({
            "society_ref": self.society_ref,
            "society_recovery_epoch": epoch,
            "governance_rule_set_ref": rule_set,
            "endeavor_proposal_digest": proposal_digest,
            "required_seats": seats,
        });
        let domain = kovee_domain_digest(&self.realm_ref, key);
        let intent_ref = format!("kfi-{key}");
        let command = json!({
            "kovee_formation_intent_ref": intent_ref,
            "byom_endpoint_ref": self.endpoint_ref,
            "command_endpoint_incarnation": self.incarnation,
            "realm_byom_binding_ref": bend.realm_binding_ref.clone()
                .unwrap_or_else(|| self.binding_ref.clone()),
            "realm_byom_binding_revision": self.binding_revision,
            "realm_byom_binding_epoch": bend.realm_binding_epoch.unwrap_or(self.binding_epoch),
            "realm_byom_binding_digest": self.binding_digest,
            "society_ref": bend.society_ref.clone().unwrap_or_else(|| self.society_ref.clone()),
            "society_recovery_epoch": bend.society_recovery_epoch.unwrap_or(epoch),
            "source_principal_ref": principal,
            "source_actor_binding_digest": self.actor_binding(principal),
            "context_bundle_ref": format!("ctx-{key}"),
            "context_bundle_digest": portable("bpp-kovee-context-bundle-v0",
                                              &json!({"bundle": key})),
            "endeavor_proposal": proposal,
            "endeavor_proposal_digest": proposal_digest,
            "source_principal_position": position,
            "source_principal_position_digest": position_digest,
            "expected_governance_rule_set_ref": rule_set,
            "expected_slot_snapshot_digest": portable(hostint::SLOT_SNAPSHOT_TAG, &snapshot),
            "byom_command_idempotency_key": key,
            "idempotency_domain_digest": domain,
        });
        let canonical = to_value(&hostint::command_digest(&command).expect("command digest"));
        let credential = self.credential(
            participant,
            principal,
            nonce,
            &canonical,
            &["kovee_endeavor_form"],
            epoch,
            bend,
        );
        let request = json!({
            "version": "0.2",
            "op": "kovee_endeavor_form",
            "meta": {
                "request_id": format!("req-{nonce}"),
                "idempotency_key": key,
                "expected_endpoint_incarnation": self.incarnation,
                "expected_recovery_epoch": epoch,
            },
            "command": command,
            "canonical_command_digest": canonical,
            "attempt_id": format!("att-{nonce}"),
            "attempt_nonce": nonce,
            "attempt_recovery_binding_ref": self.recovery_ref,
            "attempt_recovery_binding_revision": 3,
            "attempt_recovery_binding_epoch": 1,
            "attempt_recovery_binding_digest": self.recovery_digest,
            "authentication_observation_ref": format!("obs-{nonce}"),
            "authentication_observation_digest": portable("bpp-kovee-auth-observation-v0",
                                                          &json!({"observation": nonce})),
            "authentication_proof": self.attempt_proof(&canonical, &domain, nonce, principal),
        });
        Attempt {
            request,
            credential: Seam::preamble(&credential),
            credential_value: credential,
            canonical_command_digest: canonical,
            idempotency_domain_digest: domain,
            command,
            intent_ref,
        }
    }

    /// The §16.3 per-attempt authentication binding, derived exactly as
    /// the endpoint recomputes it.
    pub fn attempt_proof(
        &self,
        canonical: &Value,
        domain: &Value,
        nonce: &str,
        principal: &str,
    ) -> String {
        let d = |v: &Value| serde_json::from_value::<DigestRef>(v.clone()).expect("digest");
        hostint::attempt_proof(
            &d(canonical),
            &d(domain),
            nonce,
            &d(&self.recovery_digest),
            &d(&self.actor_binding(principal)),
        )
        .expect("attempt proof")
    }

    /// One `external_command_result_query` request over a target domain.
    #[allow(clippy::too_many_arguments)]
    pub fn query(
        &self,
        epoch: u64,
        principal: &str,
        key: &str,
        canonical: &Value,
        target_incarnation: &str,
        target_epoch: u64,
        proof: Option<(&str, &Value)>,
    ) -> Value {
        let mut request = json!({
            "version": "0.2",
            "op": "external_command_result_query",
            "current_byom_endpoint_ref": self.endpoint_ref,
            "current_endpoint_incarnation": self.incarnation,
            "current_recovery_binding_ref": self.recovery_ref,
            "current_recovery_binding_revision": 3,
            "current_recovery_binding_epoch": 1,
            "current_recovery_binding_digest": self.recovery_digest,
            "kovee_formation_intent_ref": format!("kfi-{key}"),
            "target_byom_endpoint_ref": self.endpoint_ref,
            "target_endpoint_incarnation": target_incarnation,
            "target_realm_byom_binding_ref": self.binding_ref,
            "target_realm_byom_binding_revision": self.binding_revision,
            "target_realm_byom_binding_epoch": self.binding_epoch,
            "target_realm_byom_binding_digest": self.binding_digest,
            "target_society_ref": self.society_ref,
            "target_society_recovery_epoch": target_epoch,
            "source_principal_ref": principal,
            "source_actor_binding_digest": self.actor_binding(principal),
            "operation": "kovee_endeavor_form",
            "byom_command_idempotency_key": key,
            "canonical_command_digest": canonical,
            "idempotency_domain_digest": kovee_domain_digest(&self.realm_ref, key),
        });
        let _ = epoch;
        if let Some((proof_ref, proof_digest)) = proof {
            request["restore_lineage_proof_ref"] = json!(proof_ref);
            request["restore_lineage_proof_digest"] = proof_digest.clone();
        }
        request
    }

    /// One `external_command_terminalize` attempt (request + preamble).
    #[allow(clippy::too_many_arguments)]
    pub fn terminalize(
        &self,
        epoch: u64,
        participant: &str,
        principal: &str,
        key: &str,
        nonce: &str,
        canonical: &Value,
        target_incarnation: &str,
        target_epoch: u64,
        reason: &str,
        proof: Option<(&str, &Value)>,
    ) -> (Value, String) {
        let domain = kovee_domain_digest(&self.realm_ref, key);
        let credential = self.credential(
            participant,
            principal,
            nonce,
            canonical,
            &["external_command_terminalize"],
            epoch,
            &Bend::default(),
        );
        let d = |v: &Value| serde_json::from_value::<DigestRef>(v.clone()).expect("digest");
        let proof_value = hostint::attempt_proof(
            &d(canonical),
            &d(&domain),
            nonce,
            &d(&self.recovery_digest),
            &d(&self.actor_binding(principal)),
        )
        .expect("proof");
        let mut request = json!({
            "version": "0.2",
            "op": "external_command_terminalize",
            "meta": {
                "request_id": format!("req-{nonce}"),
                "idempotency_key": key,
                "expected_endpoint_incarnation": self.incarnation,
                "expected_recovery_epoch": epoch,
            },
            "kovee_formation_intent_ref": format!("kfi-{key}"),
            "current_recovery_binding_ref": self.recovery_ref,
            "current_recovery_binding_revision": 3,
            "current_recovery_binding_epoch": 1,
            "current_recovery_binding_digest": self.recovery_digest,
            "target_byom_endpoint_ref": self.endpoint_ref,
            "target_endpoint_incarnation": target_incarnation,
            "target_society_ref": self.society_ref,
            "target_society_recovery_epoch": target_epoch,
            "source_principal_ref": principal,
            "target_source_actor_binding_digest": self.actor_binding(principal),
            "current_source_actor_binding_digest": self.actor_binding(principal),
            "operation": "kovee_endeavor_form",
            "byom_command_idempotency_key": key,
            "canonical_command_digest": canonical,
            "idempotency_domain_digest": domain,
            "reason": reason,
            "authentication_observation_ref": format!("obs-{nonce}"),
            "authentication_observation_digest": portable("bpp-kovee-auth-observation-v0",
                                                          &json!({"observation": nonce})),
            "authentication_proof": proof_value,
        });
        if let Some((proof_ref, proof_digest)) = proof {
            request["restore_lineage_proof_ref"] = json!(proof_ref);
            request["restore_lineage_proof_digest"] = proof_digest.clone();
        }
        (request, Seam::preamble(&credential))
    }
}

/// One RestoreLineage hop, self-digested as the endpoint recomputes it.
pub fn lineage(
    lineage_id: &str,
    root: &str,
    society: &str,
    from: (&str, u64),
    to: (&str, u64),
    retention: &str,
    witness_ref: &str,
) -> Value {
    let mut record = json!({
        "lineage_id": lineage_id,
        "endpoint_root_id": root,
        "predecessor_endpoint_incarnation": from.0,
        "successor_endpoint_incarnation": to.0,
        "society_ref": society,
        "predecessor_society_recovery_epoch": from.1,
        "successor_society_recovery_epoch": to.1,
        "predecessor_authority_journal_head": format!("{lineage_id}-head"),
        "predecessor_idempotency_checkpoint_ref": format!("{lineage_id}-checkpoint"),
        "predecessor_idempotency_checkpoint_digest":
            portable("bpp-idempotency-checkpoint-v0", &json!({"c": lineage_id})),
        "idempotency_retention": retention,
        "predecessor_domain_execution": "permanently_fenced",
        "recovery_event_ref": format!("{lineage_id}-recovery"),
        "external_witness_ref": witness_ref,
        "external_witness_receipt_digest":
            portable("bpp-external-witness-receipt-v0", &json!({"w": witness_ref})),
        "issued_at": "2026-01-01T00:00:00Z",
        "status": "current",
    });
    record["digest"] = portable(hostint::LINEAGE_EVIDENCE_TAG, &record);
    record
}

/// A RestoreLineageProof over the given hops, in target-to-current order.
#[allow(clippy::too_many_arguments)]
pub fn lineage_proof(
    proof_id: &str,
    root: &str,
    society: &str,
    target: (&str, u64),
    current: (&str, u64),
    domain_digest: &Value,
    hops: &[Value],
) -> Value {
    let mut record = json!({
        "proof_id": proof_id,
        "endpoint_root_id": root,
        "society_ref": society,
        "target_endpoint_incarnation": target.0,
        "target_society_recovery_epoch": target.1,
        "current_endpoint_incarnation": current.0,
        "current_society_recovery_epoch": current.1,
        "hop_count": hops.len(),
        "ordered_hops": hops.iter().map(|h| json!({
            "lineage_ref": h["lineage_id"],
            "lineage_digest": h["digest"],
        })).collect::<Vec<_>>(),
        "target_idempotency_domain_digest": domain_digest,
        "composed_at": "2026-01-02T00:00:00Z",
        "verifier_version": "restore-verifier-1",
    });
    record["digest"] = portable("bpp-restore-lineage-proof-v0", &record);
    record
}

/// The witness receipt entry the endpoint must hold to verify one hop.
pub fn witness_receipt(witness_ref: &str) -> Value {
    json!({
        "witness_ref": witness_ref,
        "receipt_digest": portable("bpp-external-witness-receipt-v0",
                                   &json!({"w": witness_ref})),
    })
}
